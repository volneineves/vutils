use rand::RngExt as _;

use crate::{Result, VutilsError};

const LOWER: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
const UPPER: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &[u8] = b"0123456789";
const SYMBOLS: &[u8] = b"!@#$%^&*()-_=+[]{};:,.?";

pub fn password(length: usize, symbols: bool, exclude_ambiguous: bool) -> Result<String> {
    let category_count = if symbols { 4 } else { 3 };
    if length < category_count || length > 4096 {
        return Err(VutilsError::InvalidInput(format!(
            "password length must be between {category_count} and 4096"
        )));
    }
    let filter = |value: &&u8| !exclude_ambiguous || !b"0O1lI|".contains(value);
    let categories: Vec<Vec<u8>> = [LOWER, UPPER, DIGITS, SYMBOLS]
        .into_iter()
        .take(category_count)
        .map(|category| category.iter().filter(filter).copied().collect())
        .collect();
    let all: Vec<u8> = categories.iter().flatten().copied().collect();
    let mut rng = rand::rng();
    let mut output: Vec<u8> = categories
        .iter()
        .map(|category| category[rng.random_range(0..category.len())])
        .collect();
    output.extend((output.len()..length).map(|_| all[rng.random_range(0..all.len())]));
    for index in (1..output.len()).rev() {
        let target = rng.random_range(0..=index);
        output.swap(index, target);
    }
    String::from_utf8(output).map_err(|error| VutilsError::Message(error.to_string()))
}

pub fn token(length: usize, alphabet: Option<&str>) -> Result<String> {
    let alphabet =
        alphabet.unwrap_or("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789");
    if alphabet.is_empty() {
        return Err(VutilsError::InvalidInput(
            "token alphabet cannot be empty".into(),
        ));
    }
    if !(1..=4096).contains(&length) {
        return Err(VutilsError::InvalidInput(
            "token length must be between 1 and 4096".into(),
        ));
    }
    let values: Vec<char> = alphabet.chars().collect();
    let mut rng = rand::rng();
    Ok((0..length)
        .map(|_| values[rng.random_range(0..values.len())])
        .collect())
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
        digits
            .into_iter()
            .map(|digit| char::from(b'0' + digit))
            .collect()
    }
}

pub fn validate_cpf(input: &str) -> bool {
    let digits = ascii_digits(input);
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
        digits
            .into_iter()
            .map(|digit| char::from(b'0' + digit))
            .collect()
    }
}

pub fn validate_cnpj(input: &str) -> bool {
    let digits = ascii_digits(input);
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

pub fn email(domain: &str) -> Result<String> {
    if domain.is_empty()
        || domain.contains(|character: char| character.is_whitespace() || character == '@')
    {
        return Err(VutilsError::InvalidInput("invalid email domain".into()));
    }
    Ok(format!(
        "{}@{domain}",
        token(12, Some("abcdefghijklmnopqrstuvwxyz0123456789"))?
    ))
}

pub fn name() -> String {
    const FIRST: &[&str] = &[
        "Ana", "Bruno", "Camila", "Daniel", "Eduarda", "Felipe", "Gabriela", "Henrique", "Isabela",
        "João", "Larissa", "Marcos",
    ];
    const LAST: &[&str] = &[
        "Almeida", "Barbosa", "Costa", "Dias", "Ferreira", "Gomes", "Lima", "Martins", "Oliveira",
        "Pereira", "Ribeiro", "Silva",
    ];
    let mut rng = rand::rng();
    format!(
        "{} {}",
        FIRST[rng.random_range(0..FIRST.len())],
        LAST[rng.random_range(0..LAST.len())]
    )
}

pub fn pix(kind: &str) -> Result<String> {
    match kind {
        "random" => Ok(uuid::Uuid::new_v4().to_string()),
        "cpf" => Ok(cpf(false)),
        "cnpj" => Ok(cnpj(false)),
        "email" => email("example.com"),
        "phone" => Ok(format!("+55{}", phone(false))),
        _ => Err(VutilsError::InvalidInput(
            "PIX kind must be random, cpf, cnpj, email, or phone".into(),
        )),
    }
}

pub fn lorem(words: usize) -> Result<String> {
    const WORDS: &[&str] = &[
        "lorem",
        "ipsum",
        "dolor",
        "sit",
        "amet",
        "consectetur",
        "adipiscing",
        "elit",
        "sed",
        "do",
        "eiusmod",
        "tempor",
        "incididunt",
        "ut",
        "labore",
        "et",
        "dolore",
        "magna",
        "aliqua",
    ];
    if !(1..=100_000).contains(&words) {
        return Err(VutilsError::InvalidInput(
            "word count must be between 1 and 100000".into(),
        ));
    }
    Ok((0..words)
        .map(|index| WORDS[index % WORDS.len()])
        .collect::<Vec<_>>()
        .join(" "))
}

fn ascii_digits(input: &str) -> Vec<u8> {
    input
        .bytes()
        .filter(u8::is_ascii_digit)
        .map(|byte| byte - b'0')
        .collect()
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
    }

    #[test]
    fn password_contains_required_categories() {
        let value = password(32, true, false).unwrap();
        assert!(
            value
                .chars()
                .any(|character| character.is_ascii_lowercase())
        );
        assert!(
            value
                .chars()
                .any(|character| character.is_ascii_uppercase())
        );
        assert!(value.chars().any(|character| character.is_ascii_digit()));
        assert!(
            value
                .chars()
                .any(|character| SYMBOLS.contains(&(character as u8)))
        );
    }
}
