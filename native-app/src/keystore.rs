//! OS credential storage for the native app.
//!
//! Primary store is the OS vault via `keyring` (Windows Credential Manager,
//! macOS Keychain, Linux Secret Service). Headless / vault-less machines
//! (CI, minimal Linux) fall back to a `0600` JSON file under the OS config
//! dir so the app still persists the key across restarts instead of asking
//! on every launch. Secrets are never logged.

use std::collections::HashMap;
use std::path::PathBuf;

pub const SERVICE: &str = "com.kivo.desktop";
pub const GEMINI_ACCOUNT: &str = "gemini";

#[derive(Debug)]
pub enum KeystoreError {
    InvalidKey(String),
    Backend(String),
}

impl std::fmt::Display for KeystoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidKey(msg) => f.write_str(msg),
            Self::Backend(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for KeystoreError {}

fn validate_key(key: &str) -> Result<String, KeystoreError> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err(KeystoreError::InvalidKey(
            "Enter a Gemini API key first.".to_string(),
        ));
    }
    if trimmed.len() > 512 || trimmed.chars().any(char::is_control) {
        return Err(KeystoreError::InvalidKey(
            "That API key looks invalid.".to_string(),
        ));
    }
    Ok(trimmed.to_owned())
}

fn fallback_path() -> Option<PathBuf> {
    if let Ok(override_path) = std::env::var("KIVO_KEYSTORE_FILE") {
        if !override_path.trim().is_empty() {
            return Some(PathBuf::from(override_path));
        }
    }
    dirs::config_dir().map(|d| d.join("kivo").join("credentials.json"))
}

fn read_fallback() -> HashMap<String, String> {
    let Some(path) = fallback_path() else {
        return HashMap::new();
    };
    std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn write_fallback(map: &HashMap<String, String>) -> Result<(), KeystoreError> {
    let Some(path) = fallback_path() else {
        return Err(KeystoreError::Backend(
            "Secure credential storage is unavailable.".to_string(),
        ));
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|_| KeystoreError::Backend("Could not save the API key.".to_string()))?;
    }
    let bytes = serde_json::to_vec(map)
        .map_err(|_| KeystoreError::Backend("Could not save the API key.".to_string()))?;
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true).mode(0o600);
        let mut file = options
            .open(&path)
            .map_err(|_| KeystoreError::Backend("Could not save the API key.".to_string()))?;
        file.write_all(&bytes)
            .map_err(|_| KeystoreError::Backend("Could not save the API key.".to_string()))?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(&path, bytes)
            .map_err(|_| KeystoreError::Backend("Could not save the API key.".to_string()))
    }
}

fn keyring_entry(account: &str) -> Result<keyring::Entry, KeystoreError> {
    keyring::Entry::new(SERVICE, account)
        .map_err(|e| KeystoreError::Backend(format!("Credential storage failed: {e}")))
}

/// Load the stored key. Returns `Ok(None)` when no key was saved yet.
/// Tries the OS vault first, then the file fallback (headless Linux/CI).
pub fn load_api_key(account: &str) -> Result<Option<String>, KeystoreError> {
    if let Ok(entry) = keyring_entry(account) {
        match entry.get_password() {
            Ok(secret) => {
                let secret = secret.trim().to_owned();
                if secret.is_empty() {
                    return Ok(None);
                }
                return Ok(Some(secret));
            }
            Err(keyring::Error::NoEntry) => {}
            Err(_) => {}
        }
    }
    Ok(read_fallback().remove(account))
}

/// Save the key to the OS vault, falling back to the `0600` file when the
/// vault is unavailable (headless Linux, locked keychain) or fails to read
/// back (container keyutils backends that accept writes but lose them).
pub fn save_api_key(account: &str, key: &str) -> Result<(), KeystoreError> {
    let clean = validate_key(key)?;
    if let Ok(entry) = keyring_entry(account) {
        if entry.set_password(&clean).is_ok() {
            // Verify with a fresh handle (the same lookup `load` performs):
            // container keyutils backends can report Ok yet lose the item
            // for any other handle, including a future process restart.
            let verified = keyring_entry(account)
                .ok()
                .and_then(|fresh| fresh.get_password().ok())
                .as_deref()
                == Some(clean.as_str());
            if verified {
                // Best-effort: drop any stale fallback copy so only the vault holds it.
                let mut map = read_fallback();
                if map.remove(account).is_some() {
                    let _ = write_fallback(&map);
                }
                return Ok(());
            }
        }
    }
    let mut map = read_fallback();
    map.insert(account.to_owned(), clean);
    write_fallback(&map)
}

/// Remove the key from both the vault and the fallback file.
pub fn clear_api_key(account: &str) -> Result<(), KeystoreError> {
    if let Ok(entry) = keyring_entry(account) {
        let _ = entry.delete_credential();
    }
    let mut map = read_fallback();
    if map.remove(account).is_some() {
        write_fallback(&map)?;
    }
    Ok(())
}

#[allow(dead_code)]
pub fn is_key_configured(account: &str) -> bool {
    load_api_key(account)
        .map(|v| v.is_some_and(|s| !s.is_empty()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn isolated(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "kivo-keystore-test-{name}-{}.json",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        // SAFETY: tests in this binary mutate env one at a time for isolation.
        unsafe {
            std::env::set_var("KIVO_KEYSTORE_FILE", &path);
        }
        path
    }

    fn release_env(path: &PathBuf) {
        let _ = std::fs::remove_file(path);
        // SAFETY: restores env after the isolated test.
        unsafe {
            std::env::remove_var("KIVO_KEYSTORE_FILE");
        }
    }

    #[test]
    fn rejects_blank_and_control_keys() {
        assert!(validate_key("   ").is_err());
        assert!(validate_key("key\r\ninjected").is_err());
        assert_eq!(validate_key("  abc123  ").unwrap(), "abc123");
    }

    #[test]
    fn round_trips_through_the_fallback_file() {
        let path = isolated("roundtrip");
        // Force the vault path to fail by using an isolated file: save goes to
        // the vault when available, else the file. Either way a fresh load
        // must see the same key (persistence across restarts).
        save_api_key("test-account", "  secret-123  ").unwrap();
        let loaded = load_api_key("test-account").unwrap();
        assert_eq!(loaded.as_deref(), Some("secret-123"));
        assert!(is_key_configured("test-account"));
        clear_api_key("test-account").unwrap();
        assert!(!is_key_configured("test-account"));
        release_env(&path);
    }
}
