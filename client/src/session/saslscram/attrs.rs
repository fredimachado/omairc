//! The SCRAM attribute parser and the base64 and iteration-count checks it shares with
//! the machine. Port of `parseScramAttributes`, `parseIterationCount` and `decodeBase64`
//! in `tui/internal/session/saslscram.go`, and of the bounds in `src/irc/ircsaslscram.cpp`.

use base64::{engine::general_purpose::STANDARD, Engine as _};

use super::ScramFailure;

/// Smallest iteration count a server may ask for. A lower count is refused before PBKDF2.
const MIN_ITERATIONS: i64 = 4096;

/// Largest iteration count a server may ask for. A higher count is refused before PBKDF2,
/// so a hostile peer cannot burn CPU.
const MAX_ITERATIONS: i64 = 1_000_000;

/// The fields of one SCRAM message. Values borrow the message; base64 is decoded where a
/// field is used. `server_error` records an `e=` attribute, whose text is discarded.
#[derive(Debug, Default)]
pub(super) struct Attributes<'a> {
    pub(super) nonce: Option<&'a [u8]>,
    pub(super) salt: Option<&'a [u8]>,
    pub(super) iterations: Option<&'a [u8]>,
    pub(super) verifier: Option<&'a [u8]>,
    pub(super) server_error: bool,
}

/// Parses a comma-separated SCRAM attribute list. `allowed` lists the keys this message may
/// carry; `e` is always accepted. Its text is discarded, because a hostile peer could echo
/// the password there and a failure must never carry it.
pub(super) fn parse<'a>(message: &'a [u8], allowed: &[u8]) -> Result<Attributes<'a>, ScramFailure> {
    if message.is_empty() {
        return Err(ScramFailure::MissingAttributes);
    }
    let mut attributes = Attributes::default();
    // A trailing comma yields an empty part, which the length check refuses.
    for part in message.split(|&byte| byte == b',') {
        if part.len() < 3 || part[1] != b'=' {
            return Err(ScramFailure::MalformedAttribute);
        }
        let (key, value) = (part[0], &part[2..]);
        if value.is_empty() {
            return Err(ScramFailure::MalformedAttribute);
        }
        if key == b'e' {
            if attributes.server_error {
                return Err(ScramFailure::RepeatedAttribute);
            }
            attributes.server_error = true;
            continue;
        }
        if !allowed.contains(&key) {
            return Err(ScramFailure::MalformedAttribute);
        }
        let slot = match key {
            b'r' => &mut attributes.nonce,
            b's' => &mut attributes.salt,
            b'i' => &mut attributes.iterations,
            b'v' => &mut attributes.verifier,
            _ => return Err(ScramFailure::MalformedAttribute),
        };
        if slot.is_some() {
            return Err(ScramFailure::RepeatedAttribute);
        }
        *slot = Some(value);
    }
    Ok(attributes)
}

/// The `i` attribute: digits only, a value that fits an i64, and inside the iteration
/// bounds.
pub(super) fn parse_iterations(text: &[u8]) -> Result<u32, ScramFailure> {
    if text.is_empty() || !text.iter().all(u8::is_ascii_digit) {
        return Err(ScramFailure::MalformedAttribute);
    }
    let count: i64 = std::str::from_utf8(text)
        .ok()
        .and_then(|digits| digits.parse().ok())
        .ok_or(ScramFailure::MalformedAttribute)?;
    if !(MIN_ITERATIONS..=MAX_ITERATIONS).contains(&count) {
        return Err(ScramFailure::IterationsRange);
    }
    u32::try_from(count).map_err(|_| ScramFailure::IterationsRange)
}

/// Canonical RFC 5802 base64 for an attribute value. The strict decoder refuses bytes
/// outside the alphabet (no skipped newlines), bad padding and non-zero trailing bits.
pub(super) fn decode_base64(text: &[u8]) -> Result<Vec<u8>, ScramFailure> {
    if text.is_empty() {
        return Err(ScramFailure::MalformedAttribute);
    }
    STANDARD
        .decode(text)
        .map_err(|_| ScramFailure::MalformedAttribute)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_keys_the_message_allows_and_keeps_raw_values() {
        let attributes = parse(b"r=a=b,s=c2FsdA==,i=4096", b"rsi").expect("valid");
        assert_eq!(attributes.nonce, Some(&b"a=b"[..]));
        assert_eq!(attributes.salt, Some(&b"c2FsdA=="[..]));
        assert_eq!(attributes.iterations, Some(&b"4096"[..]));
        assert!(attributes.verifier.is_none());
        assert!(!attributes.server_error);
    }

    #[test]
    fn a_trailing_comma_or_an_empty_message_is_refused() {
        assert_eq!(
            parse(b"", b"rsi").unwrap_err(),
            ScramFailure::MissingAttributes
        );
        assert_eq!(
            parse(b"r=a,", b"rsi").unwrap_err(),
            ScramFailure::MalformedAttribute
        );
    }

    #[test]
    fn iteration_counts_must_be_digits_inside_the_bounds() {
        assert_eq!(parse_iterations(b"4096"), Ok(4096));
        assert_eq!(parse_iterations(b"1000000"), Ok(1_000_000));
        assert_eq!(
            parse_iterations(b"4095"),
            Err(ScramFailure::IterationsRange)
        );
        assert_eq!(
            parse_iterations(b"1000001"),
            Err(ScramFailure::IterationsRange)
        );
        assert_eq!(
            parse_iterations(b"4096x"),
            Err(ScramFailure::MalformedAttribute)
        );
        assert_eq!(parse_iterations(b""), Err(ScramFailure::MalformedAttribute));
    }

    #[test]
    fn base64_must_be_canonical() {
        let salt = decode_base64(b"W22ZaJ0SNY7soEsUEjb6gQ==").expect("the RFC 7677 salt");
        assert_eq!(salt.len(), 16);
        assert!(
            decode_base64(b"W22ZaJ0SNY7soEsUEjb6gQ==!").is_err(),
            "a byte outside the alphabet"
        );
        assert!(
            decode_base64(b"W22ZaJ0SNY7soEsUEjb6gR==").is_err(),
            "non-zero trailing bits"
        );
        assert!(decode_base64(b"").is_err());
    }
}
