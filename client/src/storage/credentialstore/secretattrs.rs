//! The Secret Service attributes of one saved secret, shared by the Linux store and
//! its tests (the tests run on every platform).
//! Two clients wrote these items before this one, with different attributes:
//! - The Qt client goes through QtKeychain, whose libsecret backend stores
//!   `user=<key name>`, `server=omairc` and `type=plaintext` (or `base64`) under the
//!   `org.qt.keychain` schema (`qtkeychain/libsecret.cpp`).
//! - The TUI and earlier builds of this client store `service=omairc` and
//! A read tries both sets, so a password the Qt client saved survives the switch. A
//! write stores one item that carries both sets, so the Qt client (after a rollback)
//! and the TUI still find it, then drops the older single-set copies.

use std::collections::HashMap;

use base64::Engine;

/// The service every client stores its secrets under.
pub const SERVICE: &str = "omairc";

/// The attribute names the TUI and earlier builds of this client use.
const ATTRIBUTE_SERVICE: &str = "service";
const ATTRIBUTE_USERNAME: &str = "username";

/// The attribute names QtKeychain's libsecret backend uses.
const QT_ATTRIBUTE_USER: &str = "user";
const QT_ATTRIBUTE_SERVER: &str = "server";
const QT_ATTRIBUTE_TYPE: &str = "type";
/// QtKeychain's mode for text data, which is what the Qt client saves.
const QT_TYPE_PLAINTEXT: &str = "plaintext";
/// QtKeychain's mode for binary data. The secret is base64 text.
const QT_TYPE_BASE64: &str = "base64";
/// The schema attribute libsecret adds to every item it stores.
const XDG_SCHEMA: &str = "xdg:schema";
const QT_SCHEMA: &str = "org.qt.keychain";

/// The TUI's set: `service` and `username`.
pub fn native(key_name: &str) -> HashMap<&str, &str> {
    HashMap::from([(ATTRIBUTE_SERVICE, SERVICE), (ATTRIBUTE_USERNAME, key_name)])
}

/// QtKeychain's set: `user` and `server`. It leaves `type` out, so it finds both modes.
pub fn qtkeychain(key_name: &str) -> HashMap<&str, &str> {
    HashMap::from([(QT_ATTRIBUTE_USER, key_name), (QT_ATTRIBUTE_SERVER, SERVICE)])
}

/// The searches a read or a delete runs, in the order a read prefers them.
pub fn lookups(key_name: &str) -> [HashMap<&str, &str>; 2] {
    [native(key_name), qtkeychain(key_name)]
}

/// The attributes a write stores: both sets, plus the `type` and schema QtKeychain's
/// own lookup expects.
pub fn written(key_name: &str) -> HashMap<&str, &str> {
    let mut attributes = native(key_name);
    attributes.extend(qtkeychain(key_name));
    attributes.insert(QT_ATTRIBUTE_TYPE, QT_TYPE_PLAINTEXT);
    attributes.insert(XDG_SCHEMA, QT_SCHEMA);
    attributes
}

/// Whether an item's attributes are the ones [`written`] stores for `key_name`. Items
/// that carry only one of the older sets are not, and a write drops them.
pub fn is_written(attributes: &HashMap<String, String>, key_name: &str) -> bool {
    written(key_name)
        .iter()
        .all(|(name, value)| attributes.get(*name).map(String::as_str) == Some(*value))
}

/// The password an item holds. A QtKeychain item saved in binary mode keeps base64 text
/// any other item keeps the UTF-8 password itself.
pub fn decode_secret(secret: &[u8], attributes: &HashMap<String, String>) -> String {
    if attributes.get(QT_ATTRIBUTE_TYPE).map(String::as_str) == Some(QT_TYPE_BASE64) {
        let trimmed: Vec<u8> = secret
            .iter()
            .copied()
            .filter(|byte| !byte.is_ascii_whitespace())
            .collect();
        if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(trimmed) {
            return String::from_utf8_lossy(&bytes).into_owned();
        }
    }
    String::from_utf8_lossy(secret).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "omairc/v1/2:ab/4:fred/8:irc.test/8:password";

    fn owned(attributes: &HashMap<&str, &str>) -> HashMap<String, String> {
        attributes
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    }

    /// What QtKeychain's `secret_password_store` leaves on an item the Qt client saved.
    fn qt_item() -> HashMap<String, String> {
        owned(&HashMap::from([
            ("user", KEY),
            ("server", "omairc"),
            ("type", "plaintext"),
            ("xdg:schema", "org.qt.keychain"),
        ]))
    }

    fn matches(item: &HashMap<String, String>, search: &HashMap<&str, &str>) -> bool {
        search
            .iter()
            .all(|(name, value)| item.get(*name).map(String::as_str) == Some(*value))
    }

    #[test]
    fn a_read_finds_the_item_the_qt_client_saved() {
        let item = qt_item();
        assert!(!matches(&item, &native(KEY)), "the TUI search alone misses it");
        assert!(lookups(KEY).iter().any(|search| matches(&item, search)));
    }

    #[test]
    fn a_read_finds_the_item_the_tui_saved() {
        let item = owned(&native(KEY));
        assert!(lookups(KEY).iter().any(|search| matches(&item, search)));
    }

    #[test]
    fn a_written_item_answers_every_client_lookup() {
        let item = owned(&written(KEY));
        assert!(matches(&item, &native(KEY)), "the TUI");
        assert!(matches(&item, &qtkeychain(KEY)), "this client");
        // QtKeychain's own text lookup: `user`, `server` and `type=plaintext`.
        let qt_lookup = HashMap::from([("user", KEY), ("server", "omairc"), ("type", "plaintext")]);
        assert!(matches(&item, &qt_lookup), "the Qt client");
        assert!(is_written(&item, KEY));
    }

    #[test]
    fn single_set_items_are_not_the_written_item() {
        assert!(!is_written(&qt_item(), KEY));
        assert!(!is_written(&owned(&native(KEY)), KEY));
        assert!(!is_written(&owned(&written("another")), KEY));
    }

    #[test]
    fn a_base64_item_decodes_and_a_text_item_stays_as_is() {
        let mut binary = qt_item();
        binary.insert("type".to_owned(), "base64".to_owned());
        assert_eq!(decode_secret(b"aHVudGVyMg==\n", &binary), "hunter2");
        assert_eq!(decode_secret(b"hunter2", &qt_item()), "hunter2");
        assert_eq!(decode_secret("pässword".as_bytes(), &owned(&native(KEY))), "pässword");
        // Text that is not base64 is kept rather than lost.
        assert_eq!(decode_secret(b"not base64!", &binary), "not base64!");
    }
}
