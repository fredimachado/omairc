package irc

import "testing"

func TestAbsoluteFileLink(t *testing.T) {
	got := AbsoluteFileLink("https://irc.example.org/upload", "/upload/hoh5eFThae4e.jpeg")
	if got != "https://irc.example.org/upload/hoh5eFThae4e.jpeg" {
		t.Fatalf("relative location = %q", got)
	}
	got = AbsoluteFileLink("https://irc.example.org/upload", "https://cdn.example/a.png")
	if got != "https://cdn.example/a.png" {
		t.Fatalf("absolute location = %q", got)
	}
	if AbsoluteFileLink("https://irc.example.org/upload", "javascript:alert(1)") != "" {
		t.Fatal("javascript location was accepted")
	}
	if AbsoluteFileLink("https://irc.example.org/upload", "https://user:pw@cdn.example/a.png") != "" {
		t.Fatal("userinfo location was accepted")
	}
	got = AbsoluteFileLink("https://irc.example.org/upload", "http://cdn.example/a.png")
	if got != "http://cdn.example/a.png" {
		t.Fatalf("http location = %q", got)
	}
	if AbsoluteFileLink("", "/a") != "" || AbsoluteFileLink("https://irc.example.org/upload", "") != "" {
		t.Fatal("empty endpoint or location was accepted")
	}
	got = AbsoluteFileLink("https://irc.example.org/upload", "/a.png#part")
	if got != "https://irc.example.org/a.png" {
		t.Fatalf("fragment = %q", got)
	}
}

func TestFileHostSendsBasicAuth(t *testing.T) {
	if !FileHostSendsBasicAuth("irc.example", "https://irc.example/upload", true) {
		t.Fatal("matching https host should authenticate")
	}
	if FileHostSendsBasicAuth("irc.example", "http://irc.example/upload", true) {
		t.Fatal("cleartext upload on a TLS connection should not authenticate")
	}
	if !FileHostSendsBasicAuth("irc.example", "http://IRC.EXAMPLE./upload", false) {
		t.Fatal("matching http host on a cleartext connection should authenticate")
	}
	if !FileHostSendsBasicAuth("irc.example", "https://uploads.example/upload", true) {
		t.Fatal("a different https host should authenticate")
	}
	if FileHostSendsBasicAuth("irc.example", "https://user:pw@irc.example/upload", true) {
		t.Fatal("userinfo should not authenticate")
	}
	if !FileHostSendsBasicAuth("", "https://irc.example/upload", true) {
		t.Fatal("an empty server host should still authenticate over https")
	}
}

func TestBasicAuthorization(t *testing.T) {
	if got := BasicAuthorization("seunghye", "no"); got != "Basic c2V1bmdoeWU6bm8=" {
		t.Fatalf("authorization = %q", got)
	}
	if BasicAuthorization("", "no") != "" || BasicAuthorization("seunghye", "") != "" {
		t.Fatal("an empty pair should not produce a header")
	}
	if BasicAuthorization("bad\nname", "no") != "" {
		t.Fatal("a control character should not produce a header")
	}
}
