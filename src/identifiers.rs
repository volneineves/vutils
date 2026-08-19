use std::{
    fmt::Write as _,
    sync::atomic::{AtomicU32, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use rand::RngExt as _;
use uuid::Uuid;

use crate::{Result, VutilsError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UuidVersion {
    V1,
    V2,
    V3,
    V4,
    V5,
    V6,
    V7,
    V8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DceDomain {
    Person = 0,
    Group = 1,
    Organization = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UuidFormat {
    Hyphenated,
    Simple,
    Urn,
    Braced,
}

#[derive(Debug, Clone)]
pub struct UuidOptions<'a> {
    pub version: UuidVersion,
    pub namespace: Option<&'a str>,
    pub name: Option<&'a str>,
    pub node_id: Option<&'a str>,
    pub custom_bytes: Option<&'a str>,
    pub dce_domain: Option<DceDomain>,
    pub local_id: Option<u32>,
    pub dce_sequence: Option<u8>,
}

pub fn generate_uuid(options: &UuidOptions<'_>) -> Result<Uuid> {
    let node = options
        .node_id
        .map(parse_node_id)
        .transpose()?
        .unwrap_or_else(random_node_id);

    match options.version {
        UuidVersion::V1 => Ok(Uuid::now_v1(&node)),
        UuidVersion::V2 => generate_v2(options, node),
        UuidVersion::V3 => {
            let (namespace, name) = namespace_and_name(options)?;
            Ok(Uuid::new_v3(&namespace, name.as_bytes()))
        }
        UuidVersion::V4 => Ok(Uuid::new_v4()),
        UuidVersion::V5 => {
            let (namespace, name) = namespace_and_name(options)?;
            Ok(Uuid::new_v5(&namespace, name.as_bytes()))
        }
        UuidVersion::V6 => Ok(Uuid::now_v6(&node)),
        UuidVersion::V7 => Ok(Uuid::now_v7()),
        UuidVersion::V8 => {
            let custom = options.custom_bytes.ok_or_else(|| {
                VutilsError::InvalidInput("UUID v8 requires --custom-bytes (32 hex digits)".into())
            })?;
            Ok(Uuid::new_v8(parse_hex_array::<16>(custom, "custom bytes")?))
        }
    }
}

pub fn format_uuid(value: &Uuid, format: UuidFormat) -> String {
    match format {
        UuidFormat::Hyphenated => value.hyphenated().to_string(),
        UuidFormat::Simple => value.simple().to_string(),
        UuidFormat::Urn => value.urn().to_string(),
        UuidFormat::Braced => value.braced().to_string(),
    }
}

pub fn validate_uuid(value: &str) -> bool {
    Uuid::parse_str(value).is_ok()
}

pub fn generate_nanoid(length: usize) -> Result<String> {
    const ALPHABET: &[u8] = b"_-0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    if !(1..=1024).contains(&length) {
        return Err(VutilsError::InvalidInput(
            "NanoID length must be between 1 and 1024".into(),
        ));
    }
    let mut rng = rand::rng();
    Ok((0..length)
        .map(|_| ALPHABET[rng.random_range(0..ALPHABET.len())] as char)
        .collect())
}

pub fn generate_object_id() -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as u32;
    let random = rand::random::<[u8; 5]>();
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed) & 0x00ff_ffff;
    let mut bytes = [0_u8; 12];
    bytes[..4].copy_from_slice(&seconds.to_be_bytes());
    bytes[4..9].copy_from_slice(&random);
    bytes[9..].copy_from_slice(&counter.to_be_bytes()[1..]);
    hex_lower(&bytes)
}

pub fn generate_ulid() -> String {
    ulid::Ulid::generate().to_string()
}

fn generate_v2(options: &UuidOptions<'_>, node: [u8; 6]) -> Result<Uuid> {
    let local_id = options
        .local_id
        .ok_or_else(|| VutilsError::InvalidInput("UUID v2 requires --local-id".into()))?;
    let domain = options
        .dce_domain
        .ok_or_else(|| VutilsError::InvalidInput("UUID v2 requires --domain".into()))?;
    let base = Uuid::now_v1(&node);
    let mut bytes = *base.as_bytes();
    bytes[..4].copy_from_slice(&local_id.to_be_bytes());
    bytes[6] = (bytes[6] & 0x0f) | 0x20;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    if let Some(sequence) = options.dce_sequence {
        if sequence > 63 {
            return Err(VutilsError::InvalidInput(
                "UUID v2 sequence must be between 0 and 63".into(),
            ));
        }
        bytes[8] = 0x80 | sequence;
    }
    bytes[9] = domain as u8;
    Ok(Uuid::from_bytes(bytes))
}

fn namespace_and_name<'a>(options: &'a UuidOptions<'a>) -> Result<(Uuid, &'a str)> {
    let name = options
        .name
        .ok_or_else(|| VutilsError::InvalidInput("UUID v3 and v5 require --name".into()))?;
    let namespace = match options
        .namespace
        .unwrap_or("dns")
        .to_ascii_lowercase()
        .as_str()
    {
        "dns" => Uuid::NAMESPACE_DNS,
        "url" => Uuid::NAMESPACE_URL,
        "oid" => Uuid::NAMESPACE_OID,
        "x500" => Uuid::NAMESPACE_X500,
        custom => Uuid::parse_str(custom).map_err(|error| {
            VutilsError::InvalidInput(format!("invalid UUID namespace: {error}"))
        })?,
    };
    Ok((namespace, name))
}

fn parse_node_id(value: &str) -> Result<[u8; 6]> {
    parse_hex_array::<6>(value, "node ID")
}

fn random_node_id() -> [u8; 6] {
    let mut node = rand::random::<[u8; 6]>();
    node[0] |= 0x01;
    node
}

fn parse_hex_array<const N: usize>(value: &str, label: &str) -> Result<[u8; N]> {
    let normalized: String = value
        .chars()
        .filter(|character| !matches!(character, ':' | '-' | ' '))
        .collect();
    if normalized.len() != N * 2 {
        return Err(VutilsError::InvalidInput(format!(
            "{label} must contain exactly {} hex digits",
            N * 2
        )));
    }
    let mut bytes = [0_u8; N];
    for (index, chunk) in normalized.as_bytes().chunks_exact(2).enumerate() {
        let pair = std::str::from_utf8(chunk)
            .map_err(|error| VutilsError::InvalidInput(error.to_string()))?;
        bytes[index] = u8::from_str_radix(pair, 16).map_err(|_| {
            VutilsError::InvalidInput(format!("{label} contains non-hex characters"))
        })?;
    }
    Ok(bytes)
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(version: UuidVersion) -> UuidOptions<'static> {
        UuidOptions {
            version,
            namespace: None,
            name: None,
            node_id: None,
            custom_bytes: None,
            dce_domain: None,
            local_id: None,
            dce_sequence: None,
        }
    }

    #[test]
    fn generates_v7() {
        assert_eq!(
            generate_uuid(&options(UuidVersion::V7))
                .unwrap()
                .get_version_num(),
            7
        );
    }

    #[test]
    fn v2_embeds_local_id_and_domain() {
        let value = generate_uuid(&UuidOptions {
            version: UuidVersion::V2,
            node_id: Some("010203040506"),
            dce_domain: Some(DceDomain::Group),
            local_id: Some(42),
            ..options(UuidVersion::V2)
        })
        .unwrap();
        assert_eq!(value.get_version_num(), 2);
        assert_eq!(
            u32::from_be_bytes(value.as_bytes()[..4].try_into().unwrap()),
            42
        );
        assert_eq!(value.as_bytes()[9], 1);
    }

    #[test]
    fn object_id_has_expected_shape() {
        let value = generate_object_id();
        assert_eq!(value.len(), 24);
        assert!(value.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }

    #[test]
    fn validates_supported_uuid_representations() {
        for value in [
            "018f1f4e-7b2c-7abc-8def-0123456789ab",
            "018f1f4e7b2c7abc8def0123456789ab",
            "urn:uuid:018f1f4e-7b2c-7abc-8def-0123456789ab",
            "{018f1f4e-7b2c-7abc-8def-0123456789ab}",
        ] {
            assert!(validate_uuid(value), "expected {value} to be valid");
        }
    }

    #[test]
    fn rejects_malformed_uuids() {
        for value in [
            "",
            "018f1f4e-7b2c-7abc-8def-0123456789a",
            "018f1f4e-7b2c-7abc-8def-0123456789ag",
            "018f1f4e-7b2c-7abc-8def-0123456789ab-extra",
        ] {
            assert!(!validate_uuid(value), "expected {value} to be invalid");
        }
    }
}
