use keyring::{Entry, Error};
use zeroize::Zeroizing;

const SERVICE: &str = "dev.vutils.cli";
const USER: &str = "last-encryption-key";
const DEFAULT_USER: &str = "default-encryption-key";
#[cfg(debug_assertions)]
const DISABLE_ENV: &str = "VUTILS_TEST_DISABLE_KEYRING";

pub(crate) fn load() -> Result<Option<Zeroizing<Vec<u8>>>, String> {
    if disabled_for_tests() {
        return Ok(None);
    }
    load_preferred(&SystemEntry(default_entry()?), &SystemEntry(entry()?))
}

pub(crate) fn remember(secret: &[u8]) -> Result<(), String> {
    if disabled_for_tests() {
        return Ok(());
    }
    remember_in(&SystemEntry(entry()?), secret)
}

pub(crate) fn forget() -> Result<bool, String> {
    if disabled_for_tests() {
        return Ok(false);
    }
    let removed_default = forget_from(&SystemEntry(default_entry()?))?;
    let removed_last = forget_from(&SystemEntry(entry()?))?;
    Ok(removed_default || removed_last)
}

pub(crate) fn save_default(secret: &[u8]) -> Result<(), String> {
    if secret.is_empty() {
        return Err("the default encryption key must not be empty".into());
    }
    if disabled_for_tests() {
        return Err("credential store disabled for tests".into());
    }
    let entry = SystemEntry(default_entry()?);
    remember_in(&entry, secret)?;
    match load_from(&entry)? {
        Some(saved) if saved.as_slice() == secret => Ok(()),
        _ => Err("credential store did not retain the default encryption key".into()),
    }
}

fn default_entry() -> Result<Entry, String> {
    system_entry(DEFAULT_USER)
}

fn entry() -> Result<Entry, String> {
    system_entry(USER)
}

fn system_entry(username: &str) -> Result<Entry, String> {
    Entry::store_status().as_ref().map_err(|error| {
        format!("cannot initialize the operating-system credential store: {error}; unlock the desktop keyring or configure crypto.password-file/crypto.password-env")
    })?;
    Entry::new(SERVICE, username)
        .map_err(|error| format!("cannot access the operating-system credential store: {error}"))
}

fn disabled_for_tests() -> bool {
    #[cfg(debug_assertions)]
    {
        std::env::var_os(DISABLE_ENV).is_some()
    }
    #[cfg(not(debug_assertions))]
    {
        false
    }
}

enum StoreError {
    Missing,
    Failure(String),
}

trait CredentialEntry {
    fn get_secret(&self) -> Result<Vec<u8>, StoreError>;
    fn set_secret(&self, secret: &[u8]) -> Result<(), StoreError>;
    fn delete(&self) -> Result<(), StoreError>;
}

struct SystemEntry(Entry);

impl CredentialEntry for SystemEntry {
    fn get_secret(&self) -> Result<Vec<u8>, StoreError> {
        self.0.get_secret().map_err(map_store_error)
    }

    fn set_secret(&self, secret: &[u8]) -> Result<(), StoreError> {
        self.0.set_secret(secret).map_err(map_store_error)
    }

    fn delete(&self) -> Result<(), StoreError> {
        self.0.delete_credential().map_err(map_store_error)
    }
}

fn map_store_error(error: Error) -> StoreError {
    match error {
        Error::NoEntry => StoreError::Missing,
        error => StoreError::Failure(error.to_string()),
    }
}

fn load_preferred(
    default: &impl CredentialEntry,
    last: &impl CredentialEntry,
) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
    match load_from(default)? {
        Some(key) => Ok(Some(key)),
        None => load_from(last),
    }
}

fn load_from(entry: &impl CredentialEntry) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
    match entry.get_secret() {
        Ok(secret) => Ok(Some(Zeroizing::new(secret))),
        Err(StoreError::Missing) => Ok(None),
        Err(StoreError::Failure(error)) => {
            Err(format!("cannot read the saved encryption key: {error}"))
        }
    }
}

fn remember_in(entry: &impl CredentialEntry, secret: &[u8]) -> Result<(), String> {
    entry.set_secret(secret).map_err(|error| match error {
        StoreError::Missing => "cannot save the encryption key: credential disappeared".into(),
        StoreError::Failure(error) => format!("cannot save the encryption key: {error}"),
    })
}

fn forget_from(entry: &impl CredentialEntry) -> Result<bool, String> {
    match entry.delete() {
        Ok(()) => Ok(true),
        Err(StoreError::Missing) => Ok(false),
        Err(StoreError::Failure(error)) => {
            Err(format!("cannot remove the saved encryption key: {error}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    #[derive(Default)]
    struct MemoryEntry {
        secret: RefCell<Option<Vec<u8>>>,
    }

    impl CredentialEntry for MemoryEntry {
        fn get_secret(&self) -> Result<Vec<u8>, StoreError> {
            self.secret.borrow().clone().ok_or(StoreError::Missing)
        }

        fn set_secret(&self, secret: &[u8]) -> Result<(), StoreError> {
            *self.secret.borrow_mut() = Some(secret.to_vec());
            Ok(())
        }

        fn delete(&self) -> Result<(), StoreError> {
            self.secret
                .borrow_mut()
                .take()
                .map(|_| ())
                .ok_or(StoreError::Missing)
        }
    }

    #[test]
    fn remembers_loads_and_forgets_binary_keys() {
        let entry = MemoryEntry::default();
        assert!(load_from(&entry).unwrap().is_none());

        remember_in(&entry, b"key\0bytes").unwrap();
        assert_eq!(
            load_from(&entry).unwrap().unwrap().as_slice(),
            b"key\0bytes"
        );
        assert!(forget_from(&entry).unwrap());
        assert!(!forget_from(&entry).unwrap());
    }
    #[test]
    fn explicit_default_survives_other_successful_operations() {
        let default = MemoryEntry::default();
        let last = MemoryEntry::default();
        remember_in(&last, b"old key").unwrap();
        assert_eq!(load_preferred(&default, &last).unwrap().unwrap().as_slice(), b"old key");
        remember_in(&default, b"pinned default").unwrap();
        remember_in(&last, b"one-off override").unwrap();
        assert_eq!(load_preferred(&default, &last).unwrap().unwrap().as_slice(), b"pinned default");
        forget_from(&default).unwrap();
        forget_from(&last).unwrap();
        assert!(load_preferred(&default, &last).unwrap().is_none());
    }

}
