use keyring::{Entry, Error};
use zeroize::Zeroizing;

const SERVICE: &str = "dev.vutils.cli";
const USER: &str = "last-encryption-key";
#[cfg(debug_assertions)]
const DISABLE_ENV: &str = "VUTILS_TEST_DISABLE_KEYRING";

pub(crate) fn load() -> Result<Option<Zeroizing<Vec<u8>>>, String> {
    if disabled_for_tests() {
        return Ok(None);
    }
    load_from(&SystemEntry(entry()?))
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
    forget_from(&SystemEntry(entry()?))
}

fn entry() -> Result<Entry, String> {
    Entry::new(SERVICE, USER)
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
}
