use std::{
    fs,
    io::Read,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use aes_gcm::{
    Aes256Gcm, Nonce as AesNonce,
    aead::{Aead as _, KeyInit as _, Payload},
};
use argon2::{
    Algorithm as Argon2Algorithm, Argon2, Params as Argon2Params, Version as Argon2Version,
    password_hash::{PasswordHash, PasswordHasher as _, PasswordVerifier as _, SaltString},
};
use base64::Engine as _;
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use data_encoding::BASE32_NOPAD;
use hmac::{Hmac, Mac as _};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256, Sha512};
use walkdir::WalkDir;
use x509_parser::prelude::parse_x509_certificate;
use zeroize::Zeroizing;

use crate::{Result, VutilsError};

#[derive(Debug, Clone, Copy)]
pub enum DigestAlgorithm {
    Sha256,
    Sha512,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionAlgorithm {
    Aes256Gcm,
    XChaCha20Poly1305,
}

impl EncryptionAlgorithm {
    pub fn name(self) -> &'static str {
        match self {
            Self::Aes256Gcm => "aes-256-gcm",
            Self::XChaCha20Poly1305 => "xchacha20-poly1305",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "aes-256-gcm" => Ok(Self::Aes256Gcm),
            "xchacha20-poly1305" => Ok(Self::XChaCha20Poly1305),
            _ => Err(VutilsError::InvalidInput(format!(
                "unsupported encryption algorithm `{value}`"
            ))),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct DecryptionResult {
    pub plaintext: Vec<u8>,
    pub algorithm: EncryptionAlgorithm,
}

const ENCRYPTION_PREFIX: &str = "vutils:v1:";
const ENCRYPTION_KDF: &str = "argon2id";
const SALT_LENGTH: usize = 16;
const ARGON2_MEMORY_KIB: u32 = 65_536;
const ARGON2_ITERATIONS: u32 = 3;
const ARGON2_PARALLELISM: u32 = 4;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EncryptionEnvelope {
    algorithm: String,
    kdf: String,
    salt: String,
    nonce: String,
    ciphertext: String,
}

pub fn encrypt(
    plaintext: &[u8],
    password: &[u8],
    algorithm: EncryptionAlgorithm,
) -> Result<String> {
    validate_encryption_password(password)?;
    let salt = rand::random::<[u8; SALT_LENGTH]>();
    let key = derive_encryption_key(password, &salt)?;
    let nonce = match algorithm {
        EncryptionAlgorithm::Aes256Gcm => rand::random::<[u8; 12]>().to_vec(),
        EncryptionAlgorithm::XChaCha20Poly1305 => rand::random::<[u8; 24]>().to_vec(),
    };
    let aad = encryption_aad(algorithm);
    let payload = Payload {
        msg: plaintext,
        aad: aad.as_bytes(),
    };
    let ciphertext = match algorithm {
        EncryptionAlgorithm::Aes256Gcm => {
            let nonce = AesNonce::try_from(nonce.as_slice()).map_err(|_| {
                VutilsError::Message("failed to initialize AES-256-GCM nonce".into())
            })?;
            Aes256Gcm::new_from_slice(key.as_ref())
                .map_err(|_| VutilsError::Message("failed to initialize AES-256-GCM".into()))?
                .encrypt(&nonce, payload)
        }
        EncryptionAlgorithm::XChaCha20Poly1305 => {
            let nonce = XNonce::try_from(nonce.as_slice()).map_err(|_| {
                VutilsError::Message("failed to initialize XChaCha20-Poly1305 nonce".into())
            })?;
            XChaCha20Poly1305::new_from_slice(key.as_ref())
                .map_err(|_| {
                    VutilsError::Message("failed to initialize XChaCha20-Poly1305".into())
                })?
                .encrypt(&nonce, payload)
        }
    }
    .map_err(|_| VutilsError::Message("encryption failed".into()))?;
    let base64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let envelope = EncryptionEnvelope {
        algorithm: algorithm.name().into(),
        kdf: ENCRYPTION_KDF.into(),
        salt: base64.encode(salt),
        nonce: base64.encode(nonce),
        ciphertext: base64.encode(ciphertext),
    };
    let encoded = serde_json::to_vec(&envelope)
        .map_err(|error| VutilsError::Message(format!("failed to encode envelope: {error}")))?;
    Ok(format!("{ENCRYPTION_PREFIX}{}", base64.encode(encoded)))
}

pub fn decrypt(
    encoded: &str,
    password: &[u8],
    expected_algorithm: Option<EncryptionAlgorithm>,
) -> Result<DecryptionResult> {
    validate_encryption_password(password)?;
    let encoded = encoded.trim();
    let payload = encoded.strip_prefix(ENCRYPTION_PREFIX).ok_or_else(|| {
        VutilsError::InvalidInput("invalid encrypted value: expected a vutils:v1 envelope".into())
    })?;
    let base64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let envelope_bytes = base64.decode(payload).map_err(|_| {
        VutilsError::InvalidInput("invalid encrypted value: malformed envelope".into())
    })?;
    let envelope: EncryptionEnvelope = serde_json::from_slice(&envelope_bytes).map_err(|_| {
        VutilsError::InvalidInput("invalid encrypted value: malformed envelope".into())
    })?;
    if envelope.kdf != ENCRYPTION_KDF {
        return Err(VutilsError::InvalidInput(format!(
            "unsupported key derivation function `{}`",
            envelope.kdf
        )));
    }
    let algorithm = EncryptionAlgorithm::parse(&envelope.algorithm)?;
    if let Some(expected) = expected_algorithm
        && expected != algorithm
    {
        return Err(VutilsError::InvalidInput(format!(
            "encrypted value uses {}, not {}",
            algorithm.name(),
            expected.name()
        )));
    }
    let salt = decode_envelope_field(&envelope.salt, "salt")?;
    if salt.len() != SALT_LENGTH {
        return Err(VutilsError::InvalidInput(
            "invalid encrypted value: incorrect salt length".into(),
        ));
    }
    let nonce = decode_envelope_field(&envelope.nonce, "nonce")?;
    let expected_nonce_length = match algorithm {
        EncryptionAlgorithm::Aes256Gcm => 12,
        EncryptionAlgorithm::XChaCha20Poly1305 => 24,
    };
    if nonce.len() != expected_nonce_length {
        return Err(VutilsError::InvalidInput(
            "invalid encrypted value: incorrect nonce length".into(),
        ));
    }
    let ciphertext = decode_envelope_field(&envelope.ciphertext, "ciphertext")?;
    let key = derive_encryption_key(password, &salt)?;
    let aad = encryption_aad(algorithm);
    let payload = Payload {
        msg: &ciphertext,
        aad: aad.as_bytes(),
    };
    let plaintext = match algorithm {
        EncryptionAlgorithm::Aes256Gcm => {
            let nonce = AesNonce::try_from(nonce.as_slice()).map_err(|_| {
                VutilsError::InvalidInput("invalid encrypted value: incorrect nonce length".into())
            })?;
            Aes256Gcm::new_from_slice(key.as_ref())
                .map_err(|_| VutilsError::Message("failed to initialize AES-256-GCM".into()))?
                .decrypt(&nonce, payload)
        }
        EncryptionAlgorithm::XChaCha20Poly1305 => {
            let nonce = XNonce::try_from(nonce.as_slice()).map_err(|_| {
                VutilsError::InvalidInput("invalid encrypted value: incorrect nonce length".into())
            })?;
            XChaCha20Poly1305::new_from_slice(key.as_ref())
                .map_err(|_| {
                    VutilsError::Message("failed to initialize XChaCha20-Poly1305".into())
                })?
                .decrypt(&nonce, payload)
        }
    }
    .map_err(|_| {
        VutilsError::InvalidInput(
            "decryption failed: wrong password or corrupted encrypted value".into(),
        )
    })?;
    Ok(DecryptionResult {
        plaintext,
        algorithm,
    })
}

fn validate_encryption_password(password: &[u8]) -> Result<()> {
    if password.is_empty() {
        return Err(VutilsError::InvalidInput(
            "encryption password cannot be empty".into(),
        ));
    }
    Ok(())
}

fn derive_encryption_key(password: &[u8], salt: &[u8]) -> Result<Zeroizing<[u8; 32]>> {
    let mut key = Zeroizing::new([0_u8; 32]);
    let params = Argon2Params::new(
        ARGON2_MEMORY_KIB,
        ARGON2_ITERATIONS,
        ARGON2_PARALLELISM,
        Some(key.len()),
    )
    .map_err(|error| VutilsError::Message(format!("invalid Argon2id parameters: {error}")))?;
    Argon2::new(Argon2Algorithm::Argon2id, Argon2Version::V0x13, params)
        .hash_password_into(password, salt, &mut *key)
        .map_err(|error| VutilsError::Message(format!("Argon2id failed: {error}")))?;
    Ok(key)
}

fn decode_envelope_field(value: &str, name: &str) -> Result<Vec<u8>> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| {
            VutilsError::InvalidInput(format!(
                "invalid encrypted value: malformed {name} encoding"
            ))
        })
}

fn encryption_aad(algorithm: EncryptionAlgorithm) -> String {
    format!("{ENCRYPTION_PREFIX}{}:{ENCRYPTION_KDF}", algorithm.name())
}

#[derive(Debug, Clone, Copy)]
pub enum TotpAlgorithm {
    Sha1,
    Sha256,
    Sha512,
}

pub fn digest(input: &[u8], algorithm: DigestAlgorithm) -> String {
    match algorithm {
        DigestAlgorithm::Sha256 => hex(&Sha256::digest(input)),
        DigestAlgorithm::Sha512 => hex(&Sha512::digest(input)),
    }
}

pub fn hmac(input: &[u8], key: &[u8], algorithm: DigestAlgorithm) -> Result<String> {
    match algorithm {
        DigestAlgorithm::Sha256 => {
            let mut mac = Hmac::<Sha256>::new_from_slice(key)
                .map_err(|_| VutilsError::InvalidInput("invalid HMAC key".into()))?;
            mac.update(input);
            Ok(hex(&mac.finalize().into_bytes()))
        }
        DigestAlgorithm::Sha512 => {
            let mut mac = Hmac::<Sha512>::new_from_slice(key)
                .map_err(|_| VutilsError::InvalidInput("invalid HMAC key".into()))?;
            mac.update(input);
            Ok(hex(&mac.finalize().into_bytes()))
        }
    }
}

pub fn argon2_hash(password: &[u8]) -> Result<String> {
    let random = rand::random::<[u8; 16]>();
    let salt = SaltString::encode_b64(&random)
        .map_err(|error| VutilsError::Message(format!("failed to create salt: {error}")))?;
    Argon2::default()
        .hash_password(password, &salt)
        .map(|value| value.to_string())
        .map_err(|error| VutilsError::Message(format!("Argon2 failed: {error}")))
}

pub fn argon2_verify(password: &[u8], encoded: &str) -> Result<bool> {
    let hash = PasswordHash::new(encoded)
        .map_err(|error| VutilsError::InvalidInput(format!("invalid Argon2 hash: {error}")))?;
    Ok(Argon2::default().verify_password(password, &hash).is_ok())
}

pub fn bcrypt_hash(password: &[u8], cost: u32) -> Result<String> {
    if !(4..=31).contains(&cost) {
        return Err(VutilsError::InvalidInput(
            "bcrypt cost must be between 4 and 31".into(),
        ));
    }
    let password = std::str::from_utf8(password)
        .map_err(|_| VutilsError::InvalidInput("bcrypt password must be UTF-8".into()))?;
    bcrypt::hash(password, cost)
        .map_err(|error| VutilsError::Message(format!("bcrypt failed: {error}")))
}

pub fn bcrypt_verify(password: &[u8], encoded: &str) -> Result<bool> {
    let password = std::str::from_utf8(password)
        .map_err(|_| VutilsError::InvalidInput("bcrypt password must be UTF-8".into()))?;
    bcrypt::verify(password, encoded)
        .map_err(|error| VutilsError::InvalidInput(format!("invalid bcrypt hash: {error}")))
}

pub fn generate_totp_secret(bytes: usize) -> Result<String> {
    if !(10..=128).contains(&bytes) {
        return Err(VutilsError::InvalidInput(
            "TOTP secret size must be between 10 and 128 bytes".into(),
        ));
    }
    let random: Vec<u8> = (0..bytes).map(|_| rand::random::<u8>()).collect();
    Ok(BASE32_NOPAD.encode(&random))
}

pub fn totp_code(
    secret: &str,
    algorithm: TotpAlgorithm,
    digits: u32,
    period: u64,
    timestamp: Option<u64>,
) -> Result<String> {
    if !matches!(digits, 6 | 8) {
        return Err(VutilsError::InvalidInput(
            "TOTP digits must be 6 or 8".into(),
        ));
    }
    if period == 0 {
        return Err(VutilsError::InvalidInput(
            "TOTP period must be positive".into(),
        ));
    }
    let secret = BASE32_NOPAD
        .decode(secret.trim().to_ascii_uppercase().as_bytes())
        .map_err(|error| {
            VutilsError::InvalidInput(format!("invalid Base32 TOTP secret: {error}"))
        })?;
    if secret.is_empty() {
        return Err(VutilsError::InvalidInput(
            "TOTP secret cannot be empty".into(),
        ));
    }
    let timestamp = timestamp.unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    });
    let counter = (timestamp / period).to_be_bytes();
    let digest = match algorithm {
        TotpAlgorithm::Sha1 => {
            return totp_sha1(&secret, &counter, digits);
        }
        TotpAlgorithm::Sha256 => {
            let mut mac = Hmac::<Sha256>::new_from_slice(&secret)
                .map_err(|_| VutilsError::InvalidInput("invalid TOTP secret".into()))?;
            mac.update(&counter);
            mac.finalize().into_bytes().to_vec()
        }
        TotpAlgorithm::Sha512 => {
            let mut mac = Hmac::<Sha512>::new_from_slice(&secret)
                .map_err(|_| VutilsError::InvalidInput("invalid TOTP secret".into()))?;
            mac.update(&counter);
            mac.finalize().into_bytes().to_vec()
        }
    };
    truncate_totp(&digest, digits)
}

pub fn verify_totp(
    secret: &str,
    expected: &str,
    algorithm: TotpAlgorithm,
    digits: u32,
    period: u64,
    timestamp: Option<u64>,
    window: u64,
) -> Result<bool> {
    if !matches!(digits, 6 | 8) {
        return Err(VutilsError::InvalidInput(
            "TOTP digits must be 6 or 8".into(),
        ));
    }
    if window > 1_000 {
        return Err(VutilsError::InvalidInput(
            "TOTP verification window cannot exceed 1000 steps".into(),
        ));
    }
    if expected.len() != digits as usize || !expected.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(VutilsError::InvalidInput(format!(
            "TOTP code must contain exactly {digits} digits"
        )));
    }
    let center = timestamp.unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    });
    for offset in 0..=window {
        let delta = offset.saturating_mul(period);
        for candidate in [center.saturating_sub(delta), center.saturating_add(delta)] {
            if totp_code(secret, algorithm, digits, period, Some(candidate))? == expected {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub fn decode_jwt(token: &str) -> Result<String> {
    let parts: Vec<_> = token.trim().split('.').collect();
    if parts.len() != 3 {
        return Err(VutilsError::InvalidInput(
            "JWT must contain header.payload.signature".into(),
        ));
    }
    let header = decode_jwt_part(parts[0], "header")?;
    let payload = decode_jwt_part(parts[1], "payload")?;
    let now = chrono::Utc::now().timestamp();
    let mut claims = serde_json::Map::new();
    for name in ["exp", "iat", "nbf"] {
        if let Some(value) = payload.get(name).and_then(serde_json::Value::as_i64) {
            let formatted = chrono::DateTime::from_timestamp(value, 0)
                .map(|date| date.to_rfc3339())
                .unwrap_or_else(|| "out-of-range".into());
            claims.insert(
                name.into(),
                serde_json::json!({
                    "unix": value,
                    "rfc3339": formatted,
                    "status": if name == "exp" && value < now { "expired" } else if name == "nbf" && value > now { "not-yet-valid" } else { "informational" }
                }),
            );
        }
    }
    serde_json::to_string_pretty(&serde_json::json!({
        "verified": false,
        "header": header,
        "payload": payload,
        "temporal_claims": claims
    }))
    .map_err(|error| VutilsError::Message(error.to_string()))
}

pub fn checksum_file(path: &Path, algorithm: DigestAlgorithm) -> Result<String> {
    let mut file = fs::File::open(path).map_err(|source| VutilsError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let mut buffer = [0_u8; 64 * 1024];
    match algorithm {
        DigestAlgorithm::Sha256 => {
            let mut digest = Sha256::new();
            loop {
                let read = file.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                digest.update(&buffer[..read]);
            }
            Ok(hex(&digest.finalize()))
        }
        DigestAlgorithm::Sha512 => {
            let mut digest = Sha512::new();
            loop {
                let read = file.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                digest.update(&buffer[..read]);
            }
            Ok(hex(&digest.finalize()))
        }
    }
}

pub fn checksum_directory(
    path: &Path,
    algorithm: DigestAlgorithm,
    follow_links: bool,
) -> Result<String> {
    let mut entries = Vec::new();
    for entry in WalkDir::new(path).follow_links(follow_links) {
        let entry = entry.map_err(|error| {
            VutilsError::Message(format!("directory traversal failed: {error}"))
        })?;
        if entry.file_type().is_file() {
            let relative = entry
                .path()
                .strip_prefix(path)
                .map_err(|error| VutilsError::Message(error.to_string()))?;
            entries.push((
                relative.to_path_buf(),
                checksum_file(entry.path(), algorithm)?,
            ));
        }
    }
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(entries
        .into_iter()
        .map(|(path, checksum)| format!("{checksum}  {}", path.to_string_lossy()))
        .collect::<Vec<_>>()
        .join("\n"))
}

pub fn inspect_pem(input: &str) -> Result<String> {
    #[derive(Serialize)]
    struct PemBlock {
        label: String,
        decoded_bytes: usize,
        sensitive: bool,
    }
    let mut blocks = Vec::new();
    let mut remainder = input;
    while let Some(begin) = remainder.find("-----BEGIN ") {
        remainder = &remainder[begin + 11..];
        let label_end = remainder
            .find("-----")
            .ok_or_else(|| VutilsError::InvalidInput("invalid PEM begin marker".into()))?;
        let label = &remainder[..label_end];
        remainder = &remainder[label_end + 5..];
        let end_marker = format!("-----END {label}-----");
        let end = remainder.find(&end_marker).ok_or_else(|| {
            VutilsError::InvalidInput(format!("missing PEM end marker for {label}"))
        })?;
        let encoded: String = remainder[..end]
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|error| VutilsError::InvalidInput(format!("invalid PEM Base64: {error}")))?;
        blocks.push(PemBlock {
            label: label.to_owned(),
            decoded_bytes: bytes.len(),
            sensitive: label.contains("PRIVATE KEY"),
        });
        remainder = &remainder[end + end_marker.len()..];
    }
    if blocks.is_empty() {
        return Err(VutilsError::InvalidInput("no PEM blocks found".into()));
    }
    serde_json::to_string_pretty(&blocks).map_err(|error| VutilsError::Message(error.to_string()))
}

pub fn inspect_certificate(input: &str) -> Result<String> {
    let der = decode_first_certificate(input)?;
    let (_, certificate) = parse_x509_certificate(&der).map_err(|error| {
        VutilsError::InvalidInput(format!("invalid X.509 certificate: {error}"))
    })?;
    let mut sans = Vec::new();
    if let Ok(Some(extension)) = certificate.subject_alternative_name() {
        sans.extend(
            extension
                .value
                .general_names
                .iter()
                .map(|name| name.to_string()),
        );
    }
    let value = serde_json::json!({
        "subject": certificate.subject().to_string(),
        "issuer": certificate.issuer().to_string(),
        "serial": certificate.raw_serial_as_string(),
        "not_before": certificate.validity().not_before.to_rfc2822().unwrap_or_else(|_| "out-of-range".into()),
        "not_after": certificate.validity().not_after.to_rfc2822().unwrap_or_else(|_| "out-of-range".into()),
        "signature_algorithm": certificate.signature_algorithm.algorithm.to_id_string(),
        "subject_alt_names": sans
    });
    serde_json::to_string_pretty(&value).map_err(|error| VutilsError::Message(error.to_string()))
}

pub fn qr_svg(input: &str, module_size: u32) -> Result<String> {
    validate_qr_size(module_size)?;
    let code = qrcode::QrCode::new(input.as_bytes())
        .map_err(|error| VutilsError::InvalidInput(format!("cannot encode QR content: {error}")))?;
    Ok(code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(module_size, module_size)
        .build())
}

pub fn qr_terminal(input: &str) -> Result<String> {
    let code = qrcode::QrCode::new(input.as_bytes())
        .map_err(|error| VutilsError::InvalidInput(format!("cannot encode QR content: {error}")))?;
    Ok(code
        .render::<qrcode::render::unicode::Dense1x2>()
        .dark_color(qrcode::render::unicode::Dense1x2::Dark)
        .light_color(qrcode::render::unicode::Dense1x2::Light)
        .build())
}

pub fn qr_png(input: &str, dimensions: u32) -> Result<Vec<u8>> {
    validate_qr_size(dimensions)?;
    let code = qrcode::QrCode::new(input.as_bytes())
        .map_err(|error| VutilsError::InvalidInput(format!("cannot encode QR content: {error}")))?;
    let image = code
        .render::<image::Luma<u8>>()
        .min_dimensions(dimensions, dimensions)
        .build();
    let mut output = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageLuma8(image)
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|error| VutilsError::Message(format!("failed to encode QR PNG: {error}")))?;
    Ok(output.into_inner())
}

fn validate_qr_size(size: u32) -> Result<()> {
    if !(16..=4_096).contains(&size) {
        return Err(VutilsError::InvalidInput(
            "QR image size must be between 16 and 4096 pixels".into(),
        ));
    }
    Ok(())
}

fn totp_sha1(secret: &[u8], counter: &[u8], digits: u32) -> Result<String> {
    use sha1::Sha1;
    let mut mac = Hmac::<Sha1>::new_from_slice(secret)
        .map_err(|_| VutilsError::InvalidInput("invalid TOTP secret".into()))?;
    mac.update(counter);
    truncate_totp(&mac.finalize().into_bytes(), digits)
}

fn truncate_totp(digest: &[u8], digits: u32) -> Result<String> {
    let offset = usize::from(digest.last().copied().unwrap_or_default() & 0x0f);
    if offset + 4 > digest.len() {
        return Err(VutilsError::Message(
            "HMAC digest is too short for TOTP".into(),
        ));
    }
    let binary = (u32::from(digest[offset] & 0x7f) << 24)
        | (u32::from(digest[offset + 1]) << 16)
        | (u32::from(digest[offset + 2]) << 8)
        | u32::from(digest[offset + 3]);
    let modulo = 10_u32.pow(digits);
    Ok(format!(
        "{:0width$}",
        binary % modulo,
        width = digits as usize
    ))
}

fn decode_jwt_part(input: &str, label: &str) -> Result<serde_json::Value> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(input)
        .map_err(|error| VutilsError::InvalidInput(format!("invalid JWT {label}: {error}")))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| VutilsError::InvalidInput(format!("JWT {label} is not JSON: {error}")))
}

fn decode_first_certificate(input: &str) -> Result<Vec<u8>> {
    let begin = input
        .find("-----BEGIN CERTIFICATE-----")
        .ok_or_else(|| VutilsError::InvalidInput("no CERTIFICATE PEM block found".into()))?;
    let value = &input[begin + 27..];
    let end = value
        .find("-----END CERTIFICATE-----")
        .ok_or_else(|| VutilsError::InvalidInput("unterminated CERTIFICATE PEM block".into()))?;
    let encoded: String = value[..end]
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| VutilsError::InvalidInput(format!("invalid certificate Base64: {error}")))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_known_vector() {
        assert_eq!(
            digest(b"abc", DigestAlgorithm::Sha256),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn totp_matches_rfc_vector() {
        let secret = BASE32_NOPAD.encode(b"12345678901234567890");
        assert_eq!(
            totp_code(&secret, TotpAlgorithm::Sha1, 8, 30, Some(59)).unwrap(),
            "94287082"
        );
    }

    #[test]
    fn totp_verification_bounds_window_and_avoids_timestamp_overflow() {
        let secret = BASE32_NOPAD.encode(b"12345678901234567890");
        assert!(
            verify_totp(
                &secret,
                "000000",
                TotpAlgorithm::Sha1,
                6,
                u64::MAX,
                Some(u64::MAX),
                1,
            )
            .is_ok()
        );
        assert!(
            verify_totp(
                &secret,
                "000000",
                TotpAlgorithm::Sha1,
                6,
                30,
                Some(0),
                1_001,
            )
            .is_err()
        );
    }

    #[test]
    fn password_hashes_verify() {
        let argon = argon2_hash(b"secret").unwrap();
        assert!(argon2_verify(b"secret", &argon).unwrap());
        let bcrypt = bcrypt_hash(b"secret", 4).unwrap();
        assert!(bcrypt_verify(b"secret", &bcrypt).unwrap());
    }

    #[test]
    fn password_encryption_round_trips_all_algorithms() {
        for algorithm in [
            EncryptionAlgorithm::Aes256Gcm,
            EncryptionAlgorithm::XChaCha20Poly1305,
        ] {
            let encoded = encrypt(b"binary\0payload", b"correct horse", algorithm).unwrap();
            assert!(encoded.starts_with(ENCRYPTION_PREFIX));
            let decrypted = decrypt(&encoded, b"correct horse", Some(algorithm)).unwrap();
            assert_eq!(decrypted.plaintext, b"binary\0payload");
            assert_eq!(decrypted.algorithm, algorithm);
        }
    }

    #[test]
    fn password_encryption_uses_random_salt_and_rejects_invalid_inputs() {
        let first = encrypt(b"message", b"password", EncryptionAlgorithm::Aes256Gcm).unwrap();
        let second = encrypt(b"message", b"password", EncryptionAlgorithm::Aes256Gcm).unwrap();
        assert_ne!(first, second);
        assert!(decrypt(&first, b"wrong", None).is_err());
        assert!(
            decrypt(
                &first,
                b"password",
                Some(EncryptionAlgorithm::XChaCha20Poly1305)
            )
            .is_err()
        );
        assert!(encrypt(b"message", b"", EncryptionAlgorithm::Aes256Gcm).is_err());

        let mut corrupted = first.into_bytes();
        let last = corrupted.last_mut().unwrap();
        *last = if *last == b'A' { b'B' } else { b'A' };
        assert!(decrypt(std::str::from_utf8(&corrupted).unwrap(), b"password", None).is_err());
    }
}
