use std::io::{Read, Write};

use base64::{Engine as _, engine::general_purpose};
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use percent_encoding::{AsciiSet, CONTROLS, percent_decode, utf8_percent_encode};

use crate::{Result, VutilsError};

const URL_COMPONENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'!')
    .add(b'"')
    .add(b'#')
    .add(b'$')
    .add(b'%')
    .add(b'&')
    .add(b'\'')
    .add(b'(')
    .add(b')')
    .add(b'*')
    .add(b'+')
    .add(b',')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'=')
    .add(b'?')
    .add(b'@')
    .add(b'[')
    .add(b']');

pub fn base64_encode(input: &[u8], url_safe: bool, padding: bool) -> String {
    match (url_safe, padding) {
        (false, true) => general_purpose::STANDARD.encode(input),
        (false, false) => general_purpose::STANDARD_NO_PAD.encode(input),
        (true, true) => general_purpose::URL_SAFE.encode(input),
        (true, false) => general_purpose::URL_SAFE_NO_PAD.encode(input),
    }
}

pub fn base64_decode(input: &str, url_safe: bool, padding: bool) -> Result<Vec<u8>> {
    let engine = match (url_safe, padding) {
        (false, true) => &general_purpose::STANDARD,
        (false, false) => &general_purpose::STANDARD_NO_PAD,
        (true, true) => &general_purpose::URL_SAFE,
        (true, false) => &general_purpose::URL_SAFE_NO_PAD,
    };
    let normalized: String = input
        .chars()
        .filter(|character| !character.is_ascii_whitespace())
        .collect();
    engine
        .decode(normalized)
        .map_err(|error| VutilsError::InvalidInput(format!("invalid Base64: {error}")))
}

pub fn hex_encode(input: &[u8], uppercase: bool) -> String {
    input
        .iter()
        .map(|byte| {
            if uppercase {
                format!("{byte:02X}")
            } else {
                format!("{byte:02x}")
            }
        })
        .collect()
}

pub fn hex_decode(input: &str) -> Result<Vec<u8>> {
    let mut normalized: String = input
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    if let Some(without_prefix) = normalized
        .strip_prefix("0x")
        .or_else(|| normalized.strip_prefix("0X"))
    {
        normalized = without_prefix.to_owned();
    }
    if normalized.len() % 2 != 0 {
        return Err(VutilsError::InvalidInput(
            "hex input must contain an even number of digits".into(),
        ));
    }
    normalized
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let pair = std::str::from_utf8(pair)
                .map_err(|error| VutilsError::InvalidInput(error.to_string()))?;
            u8::from_str_radix(pair, 16)
                .map_err(|_| VutilsError::InvalidInput(format!("invalid hex pair `{pair}`")))
        })
        .collect()
}

pub fn url_encode(input: &str, form: bool) -> String {
    let encoded = utf8_percent_encode(input, URL_COMPONENT).to_string();
    if form {
        encoded.replace("%20", "+")
    } else {
        encoded
    }
}

pub fn url_decode(input: &str, form: bool) -> Result<String> {
    let normalized = if form {
        input.replace('+', " ")
    } else {
        input.to_owned()
    };
    percent_decode(normalized.as_bytes())
        .decode_utf8()
        .map(String::from)
        .map_err(|error| VutilsError::InvalidInput(format!("invalid URL encoding: {error}")))
}

pub fn html_encode(input: &str) -> String {
    html_escape::encode_safe(input).into_owned()
}

pub fn html_decode(input: &str) -> String {
    html_escape::decode_html_entities(input).into_owned()
}

pub fn gzip_compress(input: &[u8], level: u32) -> Result<Vec<u8>> {
    if level > 9 {
        return Err(VutilsError::InvalidInput(
            "GZip level must be between 0 and 9".into(),
        ));
    }
    let mut encoder = GzEncoder::new(Vec::new(), Compression::new(level));
    encoder.write_all(input)?;
    encoder.finish().map_err(Into::into)
}

pub fn gzip_decompress(input: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = GzDecoder::new(input);
    let mut output = Vec::new();
    decoder
        .read_to_end(&mut output)
        .map_err(|error| VutilsError::InvalidInput(format!("invalid GZip input: {error}")))?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_codecs_round_trip() {
        let input = b"hello\0world";
        assert_eq!(
            base64_decode(&base64_encode(input, true, false), true, false).unwrap(),
            input
        );
        assert_eq!(hex_decode(&hex_encode(input, false)).unwrap(), input);
        assert_eq!(
            gzip_decompress(&gzip_compress(input, 6).unwrap()).unwrap(),
            input
        );
    }

    #[test]
    fn url_component_round_trip() {
        let input = "hello world/olá";
        assert_eq!(url_decode(&url_encode(input, false), false).unwrap(), input);
    }

    #[test]
    fn decoders_accept_conventional_whitespace_and_prefixes() {
        assert_eq!(
            base64_decode("aGVs\nbG8=\n", false, true).unwrap(),
            b"hello"
        );
        assert_eq!(hex_decode("0XCA FE").unwrap(), [0xca, 0xfe]);
    }
}
