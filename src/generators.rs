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

#[cfg(test)]
mod tests {
    use super::*;

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
