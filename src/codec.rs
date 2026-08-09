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
    if !normalized.len().is_multiple_of(2) {
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

pub fn binary_encode(input: &[u8], spaced: bool) -> String {
    let separators = usize::from(spaced).saturating_mul(input.len().saturating_sub(1));
    let mut output = String::with_capacity(input.len().saturating_mul(8) + separators);
    for (index, byte) in input.iter().enumerate() {
        if spaced && index > 0 {
            output.push(' ');
        }
        for shift in (0..8).rev() {
            output.push(if byte & (1 << shift) == 0 { '0' } else { '1' });
        }
    }
    output
}

pub fn binary_decode(input: &str) -> Result<Vec<u8>> {
    let mut bits = Vec::with_capacity(input.len());
    for character in input.chars() {
        match character {
            '0' => bits.push(0),
            '1' => bits.push(1),
            '_' | ' ' | '\t' | '\r' | '\n' => {}
            _ => {
                return Err(VutilsError::InvalidInput(format!(
                    "invalid binary digit `{character}`; expected 0 or 1"
                )));
            }
        }
    }
    if !bits.len().is_multiple_of(8) {
        return Err(VutilsError::InvalidInput(format!(
            "binary input must contain complete 8-bit bytes; found {} bits",
            bits.len()
        )));
    }
    Ok(bits
        .chunks_exact(8)
        .map(|chunk| chunk.iter().fold(0_u8, |value, bit| (value << 1) | bit))
        .collect())
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
        assert_eq!(binary_decode(&binary_encode(input, true)).unwrap(), input);
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
        assert_eq!(binary_decode("0100_0001 01000010").unwrap(), b"AB");
    }

    #[test]
    fn binary_decoder_rejects_invalid_or_partial_bytes() {
        assert!(binary_decode("0100000x").is_err());
        assert!(binary_decode("101").is_err());
    }
}
