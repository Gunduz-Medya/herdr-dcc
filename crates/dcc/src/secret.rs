//! Secrets live in the OS keychain (macOS Keychain through the `keyring` crate). There is no
//! `get` command, on purpose. Mods run without a sandbox and can call any program, so the only
//! protection is that dcc never hands a value back. What exists is `exists`, `delete`, and `set`
//! (hidden prompt on a terminal, never argv, never stdin of a pipe). Commands that need a secret
//! use it inside the dcc process (see the SSH phase).

use std::collections::HashMap;
use std::sync::Mutex;

use keyring::{Entry, Error as KeyringError};

use crate::error::{Error, Result};

/// Keychain service name. Every DCC secret lives under it.
pub const SERVICE: &str = "dcc";

pub trait SecretBackend {
    fn set(&self, name: &str, value: &str) -> Result<()>;
    fn exists(&self, name: &str) -> Result<bool>;
    /// Returns false when there was nothing to delete.
    fn delete(&self, name: &str) -> Result<bool>;
    /// The value, for use inside the dcc process that needs it (the ssh askpass server). It is
    /// never printed, logged or put on a command line, and no command exposes it.
    fn reveal(&self, name: &str) -> Result<Option<String>>;
}

/// A name is a short identifier. It ends up in the keychain item, never a value.
pub fn validate_name(name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '.'));
    if ok {
        Ok(())
    } else {
        Err(Error::usage(
            "secret name must be 1 to 64 characters of a-z, 0-9, dash, underscore or dot",
        ))
    }
}

/// Stores the value read by `read_hidden`. One trailing line ending is dropped, nothing else is
/// touched. An empty value is refused. Error messages never contain the value.
pub fn set_with(
    backend: &dyn SecretBackend,
    name: &str,
    read_hidden: impl FnOnce(&str) -> std::io::Result<String>,
) -> Result<()> {
    validate_name(name)?;
    let mut value = read_hidden(&format!("Secret value for {name} (input hidden): "))
        .map_err(|e| Error::io(format!("could not read hidden input: {e}")))?;
    if value.ends_with('\n') {
        value.pop();
        if value.ends_with('\r') {
            value.pop();
        }
    }
    if value.is_empty() {
        return Err(Error::usage("empty secret, nothing stored"));
    }
    backend.set(name, &value)
}

pub struct KeyringBackend {
    service: String,
}

impl KeyringBackend {
    pub fn new(service: &str) -> Self {
        Self {
            service: service.to_string(),
        }
    }
    fn entry(&self, name: &str) -> Result<Entry> {
        Entry::new(&self.service, name).map_err(|e| Error::external(format!("keychain error: {e}")))
    }
}

impl SecretBackend for KeyringBackend {
    fn set(&self, name: &str, value: &str) -> Result<()> {
        self.entry(name)?
            .set_password(value)
            .map_err(|e| Error::external(format!("keychain error on set: {e}")))
    }
    fn exists(&self, name: &str) -> Result<bool> {
        // The keyring v1 API has no way to ask without reading. The value is read into this
        // process, dropped at once, and never printed, logged or returned.
        match self.entry(name)?.get_password() {
            Ok(_) => Ok(true),
            Err(KeyringError::NoEntry) => Ok(false),
            Err(e) => Err(Error::external(format!("keychain error on exists: {e}"))),
        }
    }
    fn reveal(&self, name: &str) -> Result<Option<String>> {
        match self.entry(name)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(e) => Err(Error::external(format!("keychain error on read: {e}"))),
        }
    }
    fn delete(&self, name: &str) -> Result<bool> {
        match self.entry(name)?.delete_credential() {
            Ok(()) => Ok(true),
            Err(KeyringError::NoEntry) => Ok(false),
            Err(e) => Err(Error::external(format!("keychain error on delete: {e}"))),
        }
    }
}

/// In memory backend for tests.
#[derive(Default)]
pub struct MemoryBackend {
    items: Mutex<HashMap<String, String>>,
}

impl MemoryBackend {
    pub fn new() -> Self {
        Self::default()
    }
    /// Test only, there is no such thing on the real backend.
    pub fn peek(&self, name: &str) -> Option<String> {
        self.items.lock().unwrap().get(name).cloned()
    }
}

impl SecretBackend for MemoryBackend {
    fn set(&self, name: &str, value: &str) -> Result<()> {
        self.items
            .lock()
            .unwrap()
            .insert(name.to_string(), value.to_string());
        Ok(())
    }
    fn exists(&self, name: &str) -> Result<bool> {
        Ok(self.items.lock().unwrap().contains_key(name))
    }
    fn delete(&self, name: &str) -> Result<bool> {
        Ok(self.items.lock().unwrap().remove(name).is_some())
    }
    fn reveal(&self, name: &str) -> Result<Option<String>> {
        Ok(self.items.lock().unwrap().get(name).cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_rules() {
        assert!(validate_name("ssh-prod.key_1").is_ok());
        for bad in ["", "Has Space", "UPPER", "a/b", &"x".repeat(65)] {
            assert_eq!(validate_name(bad).unwrap_err().kind, "usage", "{bad}");
        }
    }

    #[test]
    fn set_exists_delete_round_trip() {
        let b = MemoryBackend::new();
        assert!(!b.exists("k").unwrap());
        set_with(&b, "k", |_| Ok("value\n".to_string())).unwrap();
        assert!(b.exists("k").unwrap());
        assert_eq!(b.peek("k").as_deref(), Some("value"));
        assert!(b.delete("k").unwrap());
        assert!(!b.delete("k").unwrap());
    }

    #[test]
    fn crlf_and_empty_input() {
        let b = MemoryBackend::new();
        set_with(&b, "k", |_| Ok("v\r\n".to_string())).unwrap();
        assert_eq!(b.peek("k").as_deref(), Some("v"));
        assert_eq!(
            set_with(&b, "e", |_| Ok("\n".to_string()))
                .unwrap_err()
                .kind,
            "usage"
        );
        assert!(!b.exists("e").unwrap());
    }

    #[test]
    fn errors_never_carry_the_value() {
        let b = MemoryBackend::new();
        let err = set_with(&b, "BAD NAME", |_| Ok("top-secret-123".to_string())).unwrap_err();
        assert!(!err.to_string().contains("top-secret-123"));
        let err = set_with(&b, "k", |_| Err(std::io::Error::other("boom"))).unwrap_err();
        assert!(!err.to_string().contains("top-secret-123"));
    }
}
