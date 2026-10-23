//! A credential held as text. Its `Debug` is redacted and it has no `Display`, so a
//! password cannot reach a log line, a Status entry or an error by accident. The
//! plaintext leaves only through `expose`, at the wire or the credential store.

use std::fmt;

/// A password or token. It compares by value and never prints its text.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// The plaintext. Call it only where the bytes leave the process: a wire writer or the
    /// credential store.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<String> for Secret {
    fn from(text: String) -> Self {
        Self(text)
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_prints_the_redacted_marker_and_never_the_text() {
        let secret = Secret::from("hunter2".to_owned());
        assert_eq!(format!("{secret:?}"), "Secret(<redacted>)");
        assert!(!format!("{:?}", vec![secret]).contains("hunter2"));
    }

    #[test]
    fn expose_returns_the_text_and_equality_compares_it() {
        let secret = Secret::from("hunter2".to_owned());
        assert_eq!(secret.expose(), "hunter2");
        assert_eq!(secret, Secret::from("hunter2".to_owned()));
        assert_ne!(secret, Secret::default());
    }

    #[test]
    fn the_default_is_empty() {
        assert!(Secret::default().is_empty());
        assert_eq!(Secret::default().expose(), "");
        assert!(!Secret::from("x".to_owned()).is_empty());
    }
}
