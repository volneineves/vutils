use rand::RngExt as _;
use serde::Serialize;

use crate::{Result, VutilsError, generators};

#[derive(Debug, Serialize)]
pub struct Profile {
    pub cpf: String,
    pub cnpj: String,
    pub cep: String,
    pub phone: String,
    pub pix: String,
}

pub fn profile() -> Result<Profile> {
    Ok(Profile {
        cpf: cpf(false),
        cnpj: cnpj(false),
        cep: cep(false),
        phone: phone(false),
        pix: pix("random")?,
    })
}

pub fn cpf(formatted: bool) -> String {
    let mut rng = rand::rng();
    let mut digits: Vec<u8> = (0..9).map(|_| rng.random_range(0..10)).collect();
    if digits.iter().all(|digit| *digit == digits[0]) {
        digits[8] = (digits[8] + 1) % 10;
    }
    digits.push(cpf_digit(&digits, 10));
    digits.push(cpf_digit(&digits, 11));
    if formatted {
        format!(
            "{}{}{}.{}{}{}.{}{}{}-{}{}",
            digits[0],
            digits[1],
            digits[2],
            digits[3],
            digits[4],
            digits[5],
            digits[6],
            digits[7],
            digits[8],
            digits[9],
            digits[10]
        )
    } else {
        digits_to_string(digits)
    }
}

pub fn validate_cpf(input: &str) -> bool {
    let Some(digits) = document_digits(input) else {
        return false;
    };
    digits.len() == 11
        && !digits.iter().all(|digit| *digit == digits[0])
        && cpf_digit(&digits[..9], 10) == digits[9]
        && cpf_digit(&digits[..10], 11) == digits[10]
}

pub fn cnpj(formatted: bool) -> String {
    let mut rng = rand::rng();
    let mut digits: Vec<u8> = (0..8).map(|_| rng.random_range(0..10)).collect();
    digits.extend([0, 0, 0, 1]);
    digits.push(cnpj_digit(&digits));
    digits.push(cnpj_digit(&digits));
    if formatted {
        format!(
            "{}{}.{}{}{}.{}{}{}/{}{}{}{}-{}{}",
            digits[0],
            digits[1],
            digits[2],
            digits[3],
            digits[4],
            digits[5],
            digits[6],
            digits[7],
            digits[8],
            digits[9],
            digits[10],
            digits[11],
            digits[12],
            digits[13]
        )
    } else {
        digits_to_string(digits)
    }
}

pub fn validate_cnpj(input: &str) -> bool {
    let Some(digits) = document_digits(input) else {
        return false;
    };
    digits.len() == 14
        && !digits.iter().all(|digit| *digit == digits[0])
        && cnpj_digit(&digits[..12]) == digits[12]
        && cnpj_digit(&digits[..13]) == digits[13]
}

pub fn cep(formatted: bool) -> String {
    let value = rand::rng().random_range(1_000_000..100_000_000);
    if formatted {
        format!("{:05}-{:03}", value / 1000, value % 1000)
    } else {
        format!("{value:08}")
    }
}

pub fn phone(formatted: bool) -> String {
    const DDDS: &[u8] = &[
        11, 12, 13, 14, 15, 16, 17, 18, 19, 21, 22, 24, 27, 28, 31, 32, 33, 34, 35, 37, 38, 41, 42,
        43, 44, 45, 46, 47, 48, 49, 51, 53, 54, 55, 61, 62, 63, 64, 65, 66, 67, 68, 69, 71, 73, 74,
        75, 77, 79, 81, 82, 83, 84, 85, 86, 87, 88, 89, 91, 92, 93, 94, 95, 96, 97, 98, 99,
    ];
    let mut rng = rand::rng();
    let ddd = DDDS[rng.random_range(0..DDDS.len())];
    let suffix = rng.random_range(0..100_000_000_u32);
    if formatted {
        format!("({ddd}) 9{:04}-{:04}", suffix / 10_000, suffix % 10_000)
    } else {
        format!("{ddd}9{suffix:08}")
    }
}

pub fn pix(kind: &str) -> Result<String> {
    match kind {
        "random" => Ok(uuid::Uuid::new_v4().to_string()),
        "cpf" => Ok(cpf(false)),
        "cnpj" => Ok(cnpj(false)),
        "email" => generators::email("example.com"),
        "phone" => Ok(format!("+55{}", phone(false))),
        _ => Err(VutilsError::InvalidInput(
            "PIX kind must be random, cpf, cnpj, email, or phone".into(),
        )),
    }
}

fn digits_to_string(digits: Vec<u8>) -> String {
    digits
        .into_iter()
        .map(|digit| char::from(b'0' + digit))
        .collect()
}

fn document_digits(input: &str) -> Option<Vec<u8>> {
    let mut digits = Vec::new();
    for byte in input.bytes() {
        match byte {
            b'0'..=b'9' => digits.push(byte - b'0'),
            b'.' | b'-' | b'/' | b' ' | b'\t' | b'\r' | b'\n' => {}
            _ => return None,
        }
    }
    Some(digits)
}

fn cpf_digit(digits: &[u8], initial_weight: u32) -> u8 {
    let sum: u32 = digits
        .iter()
        .enumerate()
        .map(|(index, digit)| u32::from(*digit) * (initial_weight - index as u32))
        .sum();
    let remainder = (sum * 10) % 11;
    if remainder == 10 { 0 } else { remainder as u8 }
}

fn cnpj_digit(digits: &[u8]) -> u8 {
    let weights = [6_u32, 5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2];
    let weights = if digits.len() == 12 {
        &weights[1..]
    } else {
        &weights[..]
    };
    let sum: u32 = digits
        .iter()
        .zip(weights)
        .map(|(digit, weight)| u32::from(*digit) * weight)
        .sum();
    let remainder = sum % 11;
    if remainder < 2 {
        0
    } else {
        (11 - remainder) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_documents_validate() {
        for _ in 0..1_000 {
            assert!(validate_cpf(&cpf(false)));
            assert!(validate_cnpj(&cnpj(false)));
        }
        assert!(!validate_cpf("abc52998224725"));
        assert!(!validate_cnpj("11222333000181xyz"));
    }

    #[test]
    fn complete_profile_contains_valid_brazilian_values() {
        let profile = profile().unwrap();
        assert!(validate_cpf(&profile.cpf));
        assert!(validate_cnpj(&profile.cnpj));
        assert_eq!(profile.cep.len(), 8);
        assert_eq!(profile.phone.len(), 11);
        assert!(!profile.pix.is_empty());
    }
}
