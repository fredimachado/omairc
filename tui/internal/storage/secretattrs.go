package storage

import "encoding/base64"

// The Secret Service attributes of one saved secret, shared by the Linux store
// and its tests (the tests run on every platform).
// Two clients wrote these items before this one, with different attributes:
//   - The Qt client goes through QtKeychain, whose libsecret backend stores
//     user=<key name>, server=omairc and type=plaintext (or base64) under the
//     org.qt.keychain schema (qtkeychain/libsecret.cpp).
//   - The TUI and earlier builds store service=omairc and username=<key name>.
// A read tries both sets, so a password the Qt client saved survives the
// switch. A write stores one item that carries both sets, so the Qt client
// (after a rollback) and the TUI still find it, then drops the older
// single-set copies.

const (
	secretAttrsService = "omairc"

	secretAttrsAttributeService  = "service"
	secretAttrsAttributeUsername = "username"

	secretAttrsQtAttributeUser   = "user"
	secretAttrsQtAttributeServer = "server"
	secretAttrsQtAttributeType   = "type"
	secretAttrsQtTypePlaintext   = "plaintext"
	secretAttrsQtTypeBase64      = "base64"
	secretAttrsXdgSchema         = "xdg:schema"
	secretAttrsQtSchema          = "org.qt.keychain"
)

// secretAttrsNative is the TUI's set: service and username.
func secretAttrsNative(keyName string) map[string]string {
	return map[string]string{
		secretAttrsAttributeService:  secretAttrsService,
		secretAttrsAttributeUsername: keyName,
	}
}

// secretAttrsQtKeychain is QtKeychain's set: user and server. It leaves type
// out, so it finds both modes.
func secretAttrsQtKeychain(keyName string) map[string]string {
	return map[string]string{
		secretAttrsQtAttributeUser:   keyName,
		secretAttrsQtAttributeServer: secretAttrsService,
	}
}

// secretAttrsLookups are the searches a read or a delete runs, in the order a
// read prefers them.
func secretAttrsLookups(keyName string) []map[string]string {
	return []map[string]string{
		secretAttrsNative(keyName),
		secretAttrsQtKeychain(keyName),
	}
}

// secretAttrsWritten are the attributes a write stores: both sets, plus the
// type and schema QtKeychain's own lookup expects.
func secretAttrsWritten(keyName string) map[string]string {
	attributes := secretAttrsNative(keyName)
	for name, value := range secretAttrsQtKeychain(keyName) {
		attributes[name] = value
	}
	attributes[secretAttrsQtAttributeType] = secretAttrsQtTypePlaintext
	attributes[secretAttrsXdgSchema] = secretAttrsQtSchema
	return attributes
}

// secretAttrsIsWritten reports whether an item's attributes are the ones
// secretAttrsWritten stores for keyName. Items that carry only one of the
// older sets are not, and a write drops them.
func secretAttrsIsWritten(attributes map[string]string, keyName string) bool {
	for name, value := range secretAttrsWritten(keyName) {
		if attributes[name] != value {
			return false
		}
	}
	return true
}

// secretAttrsIsAsciiWhitespace matches Rust u8::is_ascii_whitespace: space
// tab, LF, form feed (\x0c), and CR. Vertical tab is not included.
func secretAttrsIsAsciiWhitespace(b byte) bool {
	return b == ' ' || b == '\t' || b == '\n' || b == '\f' || b == '\r'
}

// secretAttrsDecodeSecret returns the password an item holds. A QtKeychain
// item saved in binary mode keeps base64 text; any other item keeps the UTF-8
// password itself.
func secretAttrsDecodeSecret(secret []byte, attributes map[string]string) string {
	if attributes[secretAttrsQtAttributeType] == secretAttrsQtTypeBase64 {
		trimmed := make([]byte, 0, len(secret))
		for _, b := range secret {
			if secretAttrsIsAsciiWhitespace(b) {
				continue
			}
			trimmed = append(trimmed, b)
		}
		if decoded, err := base64.StdEncoding.DecodeString(string(trimmed)); err == nil {
			return string(decoded)
		}
	}
	return string(secret)
}

// secretAttrsMatch reports whether item carries every name/value in search.
func secretAttrsMatch(item, search map[string]string) bool {
	for name, value := range search {
		if item[name] != value {
			return false
		}
	}
	return true
}

// secretAttrsCopy returns a shallow copy so callers can mutate without sharing
// the maps returned by the builders.
func secretAttrsCopy(attributes map[string]string) map[string]string {
	out := make(map[string]string, len(attributes))
	for name, value := range attributes {
		out[name] = value
	}
	return out
}
