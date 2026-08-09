use std::{
    path::{Component, Path, PathBuf},
    str::FromStr as _,
};

use chrono::{DateTime, SecondsFormat, Utc};
use ipnet::IpNet;
use semver::Version;

use crate::{Result, VutilsError};

#[derive(Debug, Clone, Copy)]
pub enum TimeUnit {
    Seconds,
    Milliseconds,
}

pub fn now(unit: TimeUnit) -> i64 {
    let timestamp = Utc::now();
    match unit {
        TimeUnit::Seconds => timestamp.timestamp(),
        TimeUnit::Milliseconds => timestamp.timestamp_millis(),
    }
}

pub fn unix_to_rfc3339(value: i64, unit: TimeUnit) -> Result<String> {
    let timestamp = match unit {
        TimeUnit::Seconds => DateTime::from_timestamp(value, 0),
        TimeUnit::Milliseconds => DateTime::from_timestamp_millis(value),
    }
    .ok_or_else(|| VutilsError::InvalidInput("timestamp is out of range".into()))?;
    Ok(timestamp.to_rfc3339_opts(SecondsFormat::Millis, true))
}

pub fn rfc3339_to_unix(value: &str, unit: TimeUnit) -> Result<i64> {
    let timestamp = DateTime::parse_from_rfc3339(value).map_err(|error| {
        VutilsError::InvalidInput(format!("invalid RFC 3339 timestamp: {error}"))
    })?;
    Ok(match unit {
        TimeUnit::Seconds => timestamp.timestamp(),
        TimeUnit::Milliseconds => timestamp.timestamp_millis(),
    })
}

pub fn parse_duration(input: &str) -> Result<u128> {
    if input.trim().is_empty() {
        return Err(VutilsError::InvalidInput("duration cannot be empty".into()));
    }
    let mut total = 0_u128;
    let mut digits = String::new();
    for character in input.trim().chars().chain(std::iter::once(' ')) {
        if character.is_ascii_digit() {
            digits.push(character);
            continue;
        }
        if character.is_whitespace() && digits.is_empty() {
            continue;
        }
        if digits.is_empty() {
            return Err(VutilsError::InvalidInput(format!(
                "duration component is missing a number before `{character}`"
            )));
        }
        let value: u128 = digits.parse().map_err(|error: std::num::ParseIntError| {
            VutilsError::InvalidInput(error.to_string())
        })?;
        digits.clear();
        let multiplier = match character {
            'd' => 86_400_000,
            'h' => 3_600_000,
            'm' => 60_000,
            's' => 1_000,
            ' ' => 1,
            _ => {
                return Err(VutilsError::InvalidInput(format!(
                    "unsupported duration unit `{character}`"
                )));
            }
        };
        total = total
            .checked_add(
                value
                    .checked_mul(multiplier)
                    .ok_or_else(|| VutilsError::InvalidInput("duration is out of range".into()))?,
            )
            .ok_or_else(|| VutilsError::InvalidInput("duration is out of range".into()))?;
    }
    Ok(total)
}

pub fn explain_cron(expression: &str, count: usize) -> Result<String> {
    if !(1..=10_000).contains(&count) {
        return Err(VutilsError::InvalidInput(
            "cron occurrence count must be between 1 and 10000".into(),
        ));
    }
    let schedule = cron::Schedule::from_str(expression)
        .map_err(|error| VutilsError::InvalidInput(format!("invalid cron expression: {error}")))?;
    let next: Vec<_> = schedule
        .upcoming(Utc)
        .take(count)
        .map(|value| value.to_rfc3339_opts(SecondsFormat::Secs, true))
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({
        "expression": expression,
        "timezone": "UTC",
        "next": next
    }))
    .map_err(|error| VutilsError::Message(error.to_string()))
}

pub fn chmod_encode(symbolic: &str) -> Result<String> {
    if symbolic.len() != 9 {
        return Err(VutilsError::InvalidInput(
            "symbolic permissions must contain exactly 9 characters, such as rwxr-xr-x".into(),
        ));
    }
    let chars: Vec<_> = symbolic.chars().collect();
    let mut output = String::new();
    for chunk in chars.chunks_exact(3) {
        let mut value = 0;
        for (index, (character, expected)) in chunk.iter().zip(['r', 'w', 'x']).enumerate() {
            if *character == expected {
                value |= 4 >> index;
            } else if *character != '-' {
                return Err(VutilsError::InvalidInput(format!(
                    "invalid permission character `{character}`"
                )));
            }
        }
        output.push(char::from(b'0' + value));
    }
    Ok(output)
}

pub fn chmod_decode(octal: &str) -> Result<String> {
    let octal = if octal.len() == 4 {
        octal.strip_prefix('0').unwrap_or(octal)
    } else {
        octal
    };
    if octal.len() != 3
        || !octal
            .chars()
            .all(|character| matches!(character, '0'..='7'))
    {
        return Err(VutilsError::InvalidInput(
            "octal permissions must contain exactly three digits from 0 to 7".into(),
        ));
    }
    let mut output = String::new();
    for character in octal.chars() {
        let value = character.to_digit(8).ok_or_else(|| {
            VutilsError::InvalidInput(
                "octal permissions must contain exactly three digits from 0 to 7".into(),
            )
        })?;
        output.push(if value & 4 != 0 { 'r' } else { '-' });
        output.push(if value & 2 != 0 { 'w' } else { '-' });
        output.push(if value & 1 != 0 { 'x' } else { '-' });
    }
    Ok(output)
}

pub fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match normalized.components().next_back() {
                Some(Component::Normal(_)) => {
                    normalized.pop();
                }
                Some(Component::RootDir | Component::Prefix(_)) => {}
                Some(Component::ParentDir) | None => normalized.push(".."),
                Some(Component::CurDir) => {}
            },
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

pub fn relative_path(from: &Path, to: &Path) -> Result<PathBuf> {
    let from = normalize_path(from);
    let to = normalize_path(to);
    let from_parts: Vec<_> = from.components().collect();
    let to_parts: Vec<_> = to.components().collect();
    if from_parts.first().map(|component| component.as_os_str())
        != to_parts.first().map(|component| component.as_os_str())
    {
        return Err(VutilsError::InvalidInput(
            "paths do not share the same root".into(),
        ));
    }
    let shared = from_parts
        .iter()
        .zip(&to_parts)
        .take_while(|(left, right)| left == right)
        .count();
    let mut result = PathBuf::new();
    for _ in shared..from_parts.len() {
        result.push("..");
    }
    for component in &to_parts[shared..] {
        result.push(component.as_os_str());
    }
    if result.as_os_str().is_empty() {
        result.push(".");
    }
    Ok(result)
}

pub fn semver_bump(input: &str, kind: &str) -> Result<String> {
    let mut version = Version::parse(input)
        .map_err(|error| VutilsError::InvalidInput(format!("invalid semantic version: {error}")))?;
    match kind {
        "major" => {
            version.major = version.major.checked_add(1).ok_or_else(|| {
                VutilsError::InvalidInput("semantic version component overflow".into())
            })?;
            version.minor = 0;
            version.patch = 0;
        }
        "minor" => {
            version.minor = version.minor.checked_add(1).ok_or_else(|| {
                VutilsError::InvalidInput("semantic version component overflow".into())
            })?;
            version.patch = 0;
        }
        "patch" => {
            version.patch = version.patch.checked_add(1).ok_or_else(|| {
                VutilsError::InvalidInput("semantic version component overflow".into())
            })?;
        }
        _ => {
            return Err(VutilsError::InvalidInput(
                "bump kind must be major, minor, or patch".into(),
            ));
        }
    }
    version.pre = semver::Prerelease::EMPTY;
    version.build = semver::BuildMetadata::EMPTY;
    Ok(version.to_string())
}

pub fn inspect_cidr(input: &str) -> Result<String> {
    let network = IpNet::from_str(input)
        .map_err(|error| VutilsError::InvalidInput(format!("invalid CIDR: {error}")))?;
    let value = match network {
        IpNet::V4(network) => serde_json::json!({
            "version": 4,
            "network": network.network().to_string(),
            "broadcast": network.broadcast().to_string(),
            "netmask": network.netmask().to_string(),
            "prefix": network.prefix_len(),
            "hosts": if network.prefix_len() < 31 { (1_u128 << (32 - network.prefix_len())) - 2 } else { 1_u128 << (32 - network.prefix_len()) }
        }),
        IpNet::V6(network) => serde_json::json!({
            "version": 6,
            "network": network.network().to_string(),
            "netmask": network.netmask().to_string(),
            "prefix": network.prefix_len(),
            "addresses": if network.prefix_len() == 0 { "340282366920938463463374607431768211456".to_owned() } else { (1_u128 << (128 - network.prefix_len())).to_string() }
        }),
    };
    serde_json::to_string_pretty(&value).map_err(|error| VutilsError::Message(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permissions_round_trip() {
        assert_eq!(chmod_encode("rwxr-xr-x").unwrap(), "755");
        assert_eq!(chmod_decode("755").unwrap(), "rwxr-xr-x");
    }

    #[test]
    fn timestamps_round_trip() {
        let value = 1_700_000_000_i64;
        assert_eq!(
            rfc3339_to_unix(
                &unix_to_rfc3339(value, TimeUnit::Seconds).unwrap(),
                TimeUnit::Seconds
            )
            .unwrap(),
            value
        );
    }

    #[test]
    fn duration_rejects_empty_input() {
        assert!(parse_duration("").is_err());
        assert!(parse_duration("   ").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn absolute_path_normalization_does_not_escape_root() {
        assert_eq!(normalize_path(Path::new("/../a")), PathBuf::from("/a"));
    }

    #[test]
    fn semver_bump_rejects_component_overflow() {
        assert!(semver_bump("18446744073709551615.0.0", "major").is_err());
    }
}
