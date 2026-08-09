use std::str::FromStr;

use heck::{
    ToKebabCase, ToLowerCamelCase, ToShoutySnakeCase, ToSnakeCase, ToTitleCase, ToUpperCamelCase,
};
use regex::Regex;
use serde::Serialize;
use unicode_normalization::{UnicodeNormalization as _, char::is_combining_mark};

use crate::{Result, VutilsError};

#[derive(Debug, Clone, Copy)]
pub enum CaseStyle {
    Camel,
    Pascal,
    Snake,
    Kebab,
    Constant,
    Title,
}

#[derive(Debug, Clone, Copy)]
pub enum EscapeLanguage {
    Json,
    Rust,
    Kotlin,
    Java,
    CSharp,
    JavaScript,
    TypeScript,
    Python,
    Sql,
    PosixShell,
}

#[derive(Debug, Serialize)]
pub struct RegexMatch {
    pub start: usize,
    pub end: usize,
    pub value: String,
    pub groups: Vec<Option<String>>,
}

pub fn convert_case(input: &str, style: CaseStyle) -> String {
    match style {
        CaseStyle::Camel => input.to_lower_camel_case(),
        CaseStyle::Pascal => input.to_upper_camel_case(),
        CaseStyle::Snake => input.to_snake_case(),
        CaseStyle::Kebab => input.to_kebab_case(),
        CaseStyle::Constant => input.to_shouty_snake_case(),
        CaseStyle::Title => input.to_title_case(),
    }
}

pub fn slugify(input: &str) -> String {
    let normalized: String = input
        .nfkd()
        .filter(|character| !is_combining_mark(*character))
        .collect();
    let mut output = String::new();
    let mut pending_separator = false;
    for character in normalized.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            if pending_separator && !output.is_empty() {
                output.push('-');
            }
            pending_separator = false;
            output.push(character);
        } else {
            pending_separator = true;
        }
    }
    output
}

pub fn sort_lines(input: &str, unique: bool, descending: bool) -> String {
    let mut lines: Vec<_> = input.lines().collect();
    lines.sort_unstable();
    if unique {
        lines.dedup();
    }
    if descending {
        lines.reverse();
    }
    lines.join("\n")
}

pub fn unique_lines(input: &str) -> String {
    let mut seen = std::collections::HashSet::new();
    input
        .lines()
        .filter(|line| seen.insert((*line).to_owned()))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn normalize_eol(input: &str, crlf: bool) -> String {
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    if crlf {
        normalized.replace('\n', "\r\n")
    } else {
        normalized
    }
}

pub fn text_diff(left: &str, right: &str) -> String {
    similar::TextDiff::from_lines(left, right)
        .unified_diff()
        .header("left", "right")
        .to_string()
}

pub fn unicode_inspect(input: &str) -> Result<String> {
    #[derive(Serialize)]
    struct CharacterInfo {
        character: String,
        code_point: String,
        utf8: Vec<u8>,
        length_utf16: usize,
    }
    let values: Vec<_> = input
        .chars()
        .map(|character| CharacterInfo {
            character: character.to_string(),
            code_point: format!("U+{:04X}", u32::from(character)),
            utf8: character.to_string().into_bytes(),
            length_utf16: character.len_utf16(),
        })
        .collect();
    serde_json::to_string_pretty(&values).map_err(|error| VutilsError::Message(error.to_string()))
}

pub fn only_digits(input: &str) -> String {
    input.chars().filter(char::is_ascii_digit).collect()
}

pub fn regex_test(pattern: &str, input: &str) -> Result<Vec<RegexMatch>> {
    let regex = Regex::new(pattern)
        .map_err(|error| VutilsError::InvalidInput(format!("invalid regex: {error}")))?;
    Ok(regex
        .captures_iter(input)
        .filter_map(|captures| {
            let whole = captures.get(0)?;
            Some(RegexMatch {
                start: whole.start(),
                end: whole.end(),
                value: whole.as_str().to_owned(),
                groups: captures
                    .iter()
                    .skip(1)
                    .map(|value| value.map(|capture| capture.as_str().to_owned()))
                    .collect(),
            })
        })
        .collect())
}

pub fn regex_replace(
    pattern: &str,
    replacement: &str,
    input: &str,
    first_only: bool,
) -> Result<String> {
    let regex = Regex::new(pattern)
        .map_err(|error| VutilsError::InvalidInput(format!("invalid regex: {error}")))?;
    Ok(if first_only {
        regex.replace(input, replacement).into_owned()
    } else {
        regex.replace_all(input, replacement).into_owned()
    })
}

pub fn escape_string(input: &str, language: EscapeLanguage) -> Result<String> {
    match language {
        EscapeLanguage::Json => {
            serde_json::to_string(input).map_err(|error| VutilsError::Message(error.to_string()))
        }
        EscapeLanguage::Rust
        | EscapeLanguage::Kotlin
        | EscapeLanguage::Java
        | EscapeLanguage::CSharp
        | EscapeLanguage::JavaScript
        | EscapeLanguage::TypeScript
        | EscapeLanguage::Python => Ok(quote_c_style(input)),
        EscapeLanguage::Sql => Ok(format!("'{}'", input.replace('\'', "''"))),
        EscapeLanguage::PosixShell => Ok(shell_words::quote(input).into_owned()),
    }
}

pub fn unescape_string(input: &str, language: EscapeLanguage) -> Result<String> {
    match language {
        EscapeLanguage::Json
        | EscapeLanguage::Rust
        | EscapeLanguage::Kotlin
        | EscapeLanguage::Java
        | EscapeLanguage::CSharp
        | EscapeLanguage::JavaScript
        | EscapeLanguage::TypeScript
        | EscapeLanguage::Python => serde_json::from_str(input)
            .map_err(|error| VutilsError::InvalidInput(format!("invalid quoted string: {error}"))),
        EscapeLanguage::Sql => {
            let inner = input
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
                .ok_or_else(|| {
                    VutilsError::InvalidInput("SQL literal must be single-quoted".into())
                })?;
            let mut output = String::new();
            let mut characters = inner.chars().peekable();
            while let Some(character) = characters.next() {
                if character == '\'' && characters.next_if_eq(&'\'').is_none() {
                    return Err(VutilsError::InvalidInput(
                        "SQL literal contains an unescaped single quote".into(),
                    ));
                }
                output.push(character);
            }
            Ok(output)
        }
        EscapeLanguage::PosixShell => {
            let values = shell_words::split(input).map_err(|error| {
                VutilsError::InvalidInput(format!("invalid POSIX shell word: {error}"))
            })?;
            if values.len() != 1 {
                return Err(VutilsError::InvalidInput(
                    "input must represent exactly one POSIX shell word".into(),
                ));
            }
            values.into_iter().next().ok_or_else(|| {
                VutilsError::InvalidInput(
                    "input must represent exactly one POSIX shell word".into(),
                )
            })
        }
    }
}

pub fn convert_number(input: &str, from: u32, to: u32) -> Result<String> {
    if !matches!(from, 2 | 8 | 10 | 16) || !matches!(to, 2 | 8 | 10 | 16) {
        return Err(VutilsError::InvalidInput(
            "number bases must be one of 2, 8, 10, or 16".into(),
        ));
    }
    let input = input.trim().replace('_', "");
    let negative = input.starts_with('-');
    let digits = input
        .trim_start_matches('-')
        .trim_start_matches("0x")
        .trim_start_matches("0b")
        .trim_start_matches("0o");
    let value = i128::from_str_radix(digits, from).map_err(|error| {
        VutilsError::InvalidInput(format!("invalid base-{from} number: {error}"))
    })?;
    let value = if negative { -value } else { value };
    Ok(format_radix(value, to))
}

pub fn format_bytes(bytes: u128, iec: bool, precision: usize) -> Result<String> {
    if precision > 20 {
        return Err(VutilsError::InvalidInput(
            "byte-size precision cannot exceed 20 digits".into(),
        ));
    }
    let (base, units): (f64, &[&str]) = if iec {
        (1024.0, &["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"])
    } else {
        (1000.0, &["B", "kB", "MB", "GB", "TB", "PB", "EB"])
    };
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= base && unit + 1 < units.len() {
        value /= base;
        unit += 1;
    }
    Ok(format!("{value:.precision$} {}", units[unit]))
}

pub fn parse_bytes(input: &str) -> Result<u128> {
    let regex = Regex::new(r"(?i)^\s*([0-9]+(?:\.[0-9]+)?)\s*([kmgtpe]?i?b)?\s*$")
        .map_err(|error| VutilsError::Message(format!("internal byte-size pattern: {error}")))?;
    let captures = regex.captures(input).ok_or_else(|| {
        VutilsError::InvalidInput("byte size must look like `1.5 MiB` or `2000 kB`".into())
    })?;
    let number = f64::from_str(&captures[1])
        .map_err(|error| VutilsError::InvalidInput(error.to_string()))?;
    let unit = captures
        .get(2)
        .map_or("b", |value| value.as_str())
        .to_ascii_lowercase();
    let power = match unit.as_str() {
        "b" => 0,
        "kb" | "kib" => 1,
        "mb" | "mib" => 2,
        "gb" | "gib" => 3,
        "tb" | "tib" => 4,
        "pb" | "pib" => 5,
        "eb" | "eib" => 6,
        _ => {
            return Err(VutilsError::InvalidInput(format!(
                "unsupported byte-size unit `{unit}`"
            )));
        }
    };
    let base: f64 = if unit.contains('i') { 1024.0 } else { 1000.0 };
    let result = number * base.powi(power);
    if !result.is_finite() || result < 0.0 || result > u128::MAX as f64 {
        return Err(VutilsError::InvalidInput(
            "byte size is out of range".into(),
        ));
    }
    Ok(result.round() as u128)
}

fn quote_c_style(input: &str) -> String {
    serde_json::to_string(input).unwrap_or_else(|_| "\"\"".into())
}

fn format_radix(value: i128, radix: u32) -> String {
    let negative = value < 0;
    let mut magnitude = value.unsigned_abs();
    let mut digits = Vec::new();
    loop {
        let digit = (magnitude % u128::from(radix)) as u8;
        digits.push(if digit < 10 {
            (b'0' + digit) as char
        } else {
            (b'a' + digit - 10) as char
        });
        magnitude /= u128::from(radix);
        if magnitude == 0 {
            break;
        }
    }
    if negative {
        digits.push('-');
    }
    digits.iter().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_is_ascii_and_stable() {
        assert_eq!(slugify("Olá, Mundo!"), "ola-mundo");
    }

    #[test]
    fn number_conversion_handles_negative_values() {
        assert_eq!(convert_number("-255", 10, 16).unwrap(), "-ff");
    }

    #[test]
    fn shell_words_round_trip() {
        let escaped = escape_string("it's safe", EscapeLanguage::PosixShell).unwrap();
        assert_eq!(
            unescape_string(&escaped, EscapeLanguage::PosixShell).unwrap(),
            "it's safe"
        );
    }

    #[test]
    fn sql_unescape_rejects_unescaped_quotes() {
        assert!(unescape_string("'a'b'", EscapeLanguage::Sql).is_err());
        assert_eq!(
            unescape_string("'O''Reilly'", EscapeLanguage::Sql).unwrap(),
            "O'Reilly"
        );
    }
}
