package storage

import "testing"

const secretAttrsTestKey = "omairc/v1/2:ab/4:fred/8:irc.test/8:password"

// secretAttrsQtItem is what QtKeychain's secret_password_store leaves on an
// item the Qt client saved.
func secretAttrsQtItem() map[string]string {
	return map[string]string{
		"user":       secretAttrsTestKey,
		"server":     "omairc",
		"type":       "plaintext",
		"xdg:schema": "org.qt.keychain",
	}
}

func TestSecretAttrsReadFindsQtClientItem(t *testing.T) {
	item := secretAttrsQtItem()
	if secretAttrsMatch(item, secretAttrsNative(secretAttrsTestKey)) {
		t.Fatal("the TUI search alone must miss a QtKeychain item")
	}
	found := false
	for _, search := range secretAttrsLookups(secretAttrsTestKey) {
		if secretAttrsMatch(item, search) {
			found = true
			break
		}
	}
	if !found {
		t.Fatal("lookups must find the item the Qt client saved")
	}
}

func TestSecretAttrsReadFindsTUIItem(t *testing.T) {
	item := secretAttrsNative(secretAttrsTestKey)
	found := false
	for _, search := range secretAttrsLookups(secretAttrsTestKey) {
		if secretAttrsMatch(item, search) {
			found = true
			break
		}
	}
	if !found {
		t.Fatal("lookups must find the item the TUI saved")
	}
}

func TestSecretAttrsWrittenAnswersEveryClientLookup(t *testing.T) {
	item := secretAttrsWritten(secretAttrsTestKey)
	if !secretAttrsMatch(item, secretAttrsNative(secretAttrsTestKey)) {
		t.Fatal("written item must answer the TUI lookup")
	}
	if !secretAttrsMatch(item, secretAttrsQtKeychain(secretAttrsTestKey)) {
		t.Fatal("written item must answer the QtKeychain attribute lookup")
	}
	// QtKeychain's own text lookup: user, server and type=plaintext.
	qtLookup := map[string]string{
		"user":   secretAttrsTestKey,
		"server": "omairc",
		"type":   "plaintext",
	}
	if !secretAttrsMatch(item, qtLookup) {
		t.Fatal("written item must answer the Qt client's text lookup")
	}
	if !secretAttrsIsWritten(item, secretAttrsTestKey) {
		t.Fatal("written item must be recognized as the dual-set shape")
	}
}

func TestSecretAttrsSingleSetItemsAreNotWritten(t *testing.T) {
	if secretAttrsIsWritten(secretAttrsQtItem(), secretAttrsTestKey) {
		t.Fatal("a QtKeychain-only item is not the written dual-set item")
	}
	if secretAttrsIsWritten(secretAttrsNative(secretAttrsTestKey), secretAttrsTestKey) {
		t.Fatal("a TUI-only item is not the written dual-set item")
	}
	if secretAttrsIsWritten(secretAttrsWritten("another"), secretAttrsTestKey) {
		t.Fatal("a dual-set item for another key must not match")
	}
}

func TestSecretAttrsDecodeBase64AndPlaintext(t *testing.T) {
	binary := secretAttrsCopy(secretAttrsQtItem())
	binary["type"] = "base64"
	if got := secretAttrsDecodeSecret([]byte("aHVudGVyMg==\n"), binary); got != "hunter2" {
		t.Fatalf("base64 decode = %q, want hunter2", got)
	}
	if got := secretAttrsDecodeSecret([]byte("hunter2"), secretAttrsQtItem()); got != "hunter2" {
		t.Fatalf("plaintext decode = %q, want hunter2", got)
	}
	if got := secretAttrsDecodeSecret([]byte("pässword"), secretAttrsNative(secretAttrsTestKey)); got != "pässword" {
		t.Fatalf("utf-8 decode = %q, want pässword", got)
	}
	// Text that is not base64 is kept rather than lost.
	if got := secretAttrsDecodeSecret([]byte("not base64!"), binary); got != "not base64!" {
		t.Fatalf("invalid base64 kept = %q, want not base64!", got)
	}
}

func TestSecretAttrsWrittenShape(t *testing.T) {
	item := secretAttrsWritten(secretAttrsTestKey)
	want := map[string]string{
		"service":    "omairc",
		"username":   secretAttrsTestKey,
		"user":       secretAttrsTestKey,
		"server":     "omairc",
		"type":       "plaintext",
		"xdg:schema": "org.qt.keychain",
	}
	if len(item) != len(want) {
		t.Fatalf("written attrs len = %d, want %d (%v)", len(item), len(want), item)
	}
	for name, value := range want {
		if item[name] != value {
			t.Errorf("written[%q] = %q, want %q", name, item[name], value)
		}
	}
}
