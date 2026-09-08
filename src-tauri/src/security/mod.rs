use std::{fmt, str};

use serde::{Deserialize, Deserializer, Serialize, de};

/// A secret owned by the trusted process.
///
/// The type deliberately does not implement `Clone`, `Display`, or `Serialize` so a
/// credential cannot accidentally cross IPC or end up in diagnostics.
pub struct SecretString(Vec<u8>);

impl SecretString {
    pub fn new(value: String) -> Result<Self, CredentialError> {
        let mut bytes = value.into_bytes();
        let start = bytes
            .iter()
            .position(|byte| !byte.is_ascii_whitespace())
            .unwrap_or(bytes.len());
        let end = bytes
            .iter()
            .rposition(|byte| !byte.is_ascii_whitespace())
            .map_or(start, |position| position + 1);
        let length = end.saturating_sub(start);
        if length == 0 || length > 512 {
            bytes.fill(0);
            return Err(CredentialError::InvalidSecret);
        }

        if start > 0 {
            bytes.copy_within(start..end, 0);
        }
        bytes[length..].fill(0);
        bytes.truncate(length);
        Ok(Self(bytes))
    }

    pub(crate) fn expose(&self) -> &str {
        // `SecretString` can only be constructed from a valid UTF-8 `String`.
        str::from_utf8(&self.0).expect("secret invariant violated")
    }
}

impl Drop for SecretString {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretString([REDACTED])")
    }
}

impl<'de> Deserialize<'de> for SecretString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialStatus {
    pub configured: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum CredentialError {
    #[allow(dead_code)]
    Unavailable,
    AccessDenied,
    InvalidSecret,
    Backend,
}

impl fmt::Display for CredentialError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Unavailable => "Secure credential storage is unavailable.",
            Self::AccessDenied => "Secure credential storage access was denied.",
            Self::InvalidSecret => "The API key is empty or invalid.",
            Self::Backend => "Secure credential storage failed.",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for CredentialError {}

/// Implemented by the macOS Keychain and Windows Credential Manager adapters.
///
/// Implementations must never persist the value anywhere except the operating
/// system's credential vault and must not include it in backend error messages.
pub trait CredentialStore: Send + Sync {
    fn save_api_key(&self, secret: &SecretString) -> Result<(), CredentialError>;
    fn load_api_key(&self) -> Result<Option<SecretString>, CredentialError>;
    fn clear_api_key(&self) -> Result<(), CredentialError>;

    fn status(&self) -> Result<CredentialStatus, CredentialError> {
        Ok(CredentialStatus {
            configured: self.load_api_key()?.is_some(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::SecretString;

    #[test]
    fn secret_debug_output_is_redacted() {
        let secret = SecretString::new("test-api-key".into()).unwrap();
        let debug = format!("{secret:?}");
        assert_eq!(debug, "SecretString([REDACTED])");
        assert!(!debug.contains("test-api-key"));
    }

    #[test]
    fn trims_secret_at_the_trust_boundary() {
        let secret = SecretString::new("  test-api-key\n".into()).unwrap();
        assert_eq!(secret.expose(), "test-api-key");
    }
}
