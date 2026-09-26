package session

import (
	"strings"
	"testing"
	"time"
)

// --- SASL PLAIN -----------------------------------------------------------

// TestSessionNegotiatesSaslPlain ports SessionTest::negotiatesSaslPlain.
func TestSessionNegotiatesSaslPlain(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.Password = "secret"
	fixture := newSessionFixture(t, config)

	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :sasl=PLAIN,EXTERNAL\r\n")
	assertFramesEqual(t, fixture.frames(), []string{
		"CAP LS 302\r\n",
		"CAP REQ :sasl\r\n",
		"NICK omairc\r\n",
		"USER omairc 8 * :Omairc User\r\n",
	})

	fixture.inject(":server CAP omairc ACK :sasl\r\n")
	if got := fixture.session.State(); got != StateSasl {
		t.Fatalf("state = %v, want Sasl", got)
	}
	if got := fixture.lastFrame(); got != "AUTHENTICATE PLAIN\r\n" {
		t.Fatalf("last frame = %q, want AUTHENTICATE PLAIN", got)
	}

	fixture.inject("AUTHENTICATE +\r\n")
	if got := decodeAuthenticatePayload(t, fixture.lastFrame()); string(got) != "omairc\x00omairc\x00secret" {
		t.Fatalf("SASL payload = %q, want omairc\\0omairc\\0secret", got)
	}

	fixture.inject(":server 903 omairc :SASL successful\r\n")
	if got := fixture.lastFrame(); got != "CAP END\r\n" {
		t.Fatalf("last frame = %q, want CAP END", got)
	}
}

// TestSessionSaslPlainReassemblesMultiChunkMessages ports
// SessionTest::saslPlainReassemblesMultiChunkMessages.
func TestSessionSaslPlainReassemblesMultiChunkMessages(t *testing.T) {
	secret := strings.Repeat("x", 287)
	config := sessionTestConfig(sessionTestNetworkID)
	config.Password = secret
	fixture := newSessionFixture(t, config)

	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :sasl=PLAIN\r\n" +
		":server CAP omairc ACK :sasl\r\n")
	if got := fixture.session.State(); got != StateSasl {
		t.Fatalf("state = %v, want Sasl", got)
	}

	plain := "omairc\x00omairc\x00" + secret
	if len(plain) != 301 {
		t.Fatalf("plain = %d bytes, want 301", len(plain))
	}
	encoded := base64Encode([]byte(plain))
	if len(encoded) != 404 {
		t.Fatalf("encoded = %d bytes, want 404", len(encoded))
	}

	before := len(fixture.frames())
	fixture.inject("AUTHENTICATE +\r\n")
	frames := fixture.frames()
	if len(frames) != before+2 {
		t.Fatalf("frames written = %d, want %d", len(frames)-before, 2)
	}
	if got := len(authenticateBody(frames[before])); got != 400 {
		t.Fatalf("first chunk = %d bytes, want 400", got)
	}
	if got := len(authenticateBody(frames[before+1])); got != 4 {
		t.Fatalf("second chunk = %d bytes, want 4", got)
	}
	if got := authenticateBody(frames[before]) + authenticateBody(frames[before+1]); got != encoded {
		t.Fatalf("reassembled = %q, want %q", got, encoded)
	}

	if len(fixture.handler.errors) != 0 {
		t.Fatalf("errors = %d, want 0", len(fixture.handler.errors))
	}
	if got := fixture.session.State(); got != StateSasl {
		t.Fatalf("state = %v, want Sasl", got)
	}

	fixture.inject(":server 903 omairc :SASL successful\r\n")
	if got := fixture.lastFrame(); got != "CAP END\r\n" {
		t.Fatalf("last frame = %q, want CAP END", got)
	}
	if got := fixture.session.State(); got != StateRegistering {
		t.Fatalf("state = %v, want Registering", got)
	}

	if fixture.handler.hasLabel("AUTHENTICATE") {
		t.Fatal("AUTHENTICATE must not appear as a Status label")
	}
	if fixture.handler.anyFieldContains("AUTHENTICATE") {
		t.Fatal("AUTHENTICATE must not appear in a Status entry")
	}
	if fixture.handler.anyFieldContains(secret) {
		t.Fatal("the password must not appear in a Status entry")
	}
	if fixture.handler.anyFieldContains(encoded) {
		t.Fatal("the encoded payload must not appear in a Status entry")
	}
}

// TestSessionSaslPlainExactChunkBoundaryRequiresPlus ports
// SessionTest::saslPlainExactChunkBoundaryRequiresPlus.
func TestSessionSaslPlainExactChunkBoundaryRequiresPlus(t *testing.T) {
	for _, secretLength := range []int{286, 586} {
		secret := strings.Repeat("x", secretLength)
		config := sessionTestConfig(sessionTestNetworkID)
		config.Password = secret
		fixture := newSessionFixture(t, config)

		fixture.connectTLS()
		fixture.inject(":server CAP omairc LS :sasl=PLAIN\r\n" +
			":server CAP omairc ACK :sasl\r\n")
		if got := fixture.session.State(); got != StateSasl {
			t.Fatalf("state = %v, want Sasl", got)
		}

		plain := "omairc\x00omairc\x00" + secret
		encoded := base64Encode([]byte(plain))
		if len(encoded)%400 != 0 {
			t.Fatalf("encoded length %d is not a multiple of 400", len(encoded))
		}
		chunks := len(encoded) / 400

		before := len(fixture.frames())
		fixture.inject("AUTHENTICATE +\r\n")
		frames := fixture.frames()
		if len(frames) != before+chunks+1 {
			t.Fatalf("frames written = %d, want %d", len(frames)-before, chunks+1)
		}
		var reassembled strings.Builder
		for index := 0; index < chunks; index++ {
			body := authenticateBody(frames[before+index])
			if len(body) != 400 {
				t.Fatalf("chunk %d = %d bytes, want 400", index, len(body))
			}
			reassembled.WriteString(body)
		}
		if got := frames[before+chunks]; got != "AUTHENTICATE +\r\n" {
			t.Fatalf("trailing frame = %q, want AUTHENTICATE +", got)
		}
		if got := reassembled.String(); got != encoded {
			t.Fatalf("reassembled = %q, want %q", got, encoded)
		}

		if len(fixture.handler.errors) != 0 {
			t.Fatalf("errors = %d, want 0", len(fixture.handler.errors))
		}
		fixture.inject(":server 903 omairc :SASL successful\r\n")
		if got := fixture.lastFrame(); got != "CAP END\r\n" {
			t.Fatalf("last frame = %q, want CAP END", got)
		}
		if got := fixture.session.State(); got != StateRegistering {
			t.Fatalf("state = %v, want Registering", got)
		}
		if fixture.handler.hasLabel("AUTHENTICATE") ||
			fixture.handler.anyFieldContains("AUTHENTICATE") ||
			fixture.handler.anyFieldContains(secret) ||
			fixture.handler.anyFieldContains(encoded) {
			t.Fatal("SASL secrets leaked into Status entries")
		}
	}
}

// TestSessionSaslAccountAuthenticatesAsTheBouncerName ports
// SessionTest::saslAccountAuthenticatesAsTheBouncerName.
func TestSessionSaslAccountAuthenticatesAsTheBouncerName(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.SASLAccount = "joe/libera"
	config.Password = "secret"
	fixture := newSessionFixture(t, config)

	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :sasl=PLAIN\r\n" +
		":server CAP omairc ACK :sasl\r\n" +
		"AUTHENTICATE +\r\n")

	if got := string(decodeAuthenticatePayload(t, fixture.lastFrame())); got != "joe/libera\x00joe/libera\x00secret" {
		t.Fatalf("SASL payload = %q, want joe/libera\\0joe/libera\\0secret", got)
	}
	if got := fixture.session.Nick(); got != "omairc" {
		t.Fatalf("nick = %q, want omairc", got)
	}
}

// TestSessionEmptySaslAccountAuthenticatesAsTheNick ports
// SessionTest::emptySaslAccountAuthenticatesAsTheNick.
func TestSessionEmptySaslAccountAuthenticatesAsTheNick(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.Password = "secret"
	fixture := newSessionFixture(t, config)

	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :sasl=PLAIN\r\n" +
		":server CAP omairc ACK :sasl\r\n" +
		"AUTHENTICATE +\r\n")

	if got := string(decodeAuthenticatePayload(t, fixture.lastFrame())); got != "omairc\x00omairc\x00secret" {
		t.Fatalf("SASL payload = %q, want omairc\\0omairc\\0secret", got)
	}
}

// TestSessionSendsPassWhenSaslIsUnavailable ports
// SessionTest::sendsPassWhenSaslIsUnavailable.
func TestSessionSendsPassWhenSaslIsUnavailable(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.Password = "secret"
	fixture := newSessionFixture(t, config)

	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :invite-notify\r\n")
	assertFramesEqual(t, fixture.writtenSince(1), []string{
		"PASS secret\r\n",
		"NICK omairc\r\n",
		"USER omairc 8 * :Omairc User\r\n",
		"CAP END\r\n",
	})
}

// TestSessionNickServOnlySaslPlainUsesNickServSecret ports
// SessionTest::nickServOnlySaslPlainUsesNickServSecret.
func TestSessionNickServOnlySaslPlainUsesNickServSecret(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.NickServPassword = "nick-secret"
	fixture := newSessionFixture(t, config)

	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :sasl=PLAIN\r\n")
	assertFramesEqual(t, fixture.frames(), []string{
		"CAP LS 302\r\n",
		"CAP REQ :sasl\r\n",
		"NICK omairc\r\n",
		"USER omairc 8 * :Omairc User\r\n",
	})
	if fixture.wrote("PASS nick-secret\r\n") {
		t.Fatal("the NickServ secret must not ride PASS")
	}

	fixture.inject(":server CAP omairc ACK :sasl\r\n" +
		"AUTHENTICATE +\r\n")
	if got := string(decodeAuthenticatePayload(t, fixture.lastFrame())); got != "omairc\x00omairc\x00nick-secret" {
		t.Fatalf("SASL payload = %q, want the NickServ secret", got)
	}

	fixture.inject(":server 903 omairc :SASL successful\r\n" +
		":server 001 omairc :Welcome\r\n")
	if fixture.wrote("PRIVMSG NickServ :IDENTIFY nick-secret\r\n") {
		t.Fatal("a successful SASL must not IDENTIFY")
	}
	if !fixture.wrote("JOIN #omarchy\r\n") {
		t.Fatal("autojoin missing")
	}
}

// TestSessionNickServOnlyWithoutSaslIdentifiesBeforeJoin ports
// SessionTest::nickServOnlyWithoutSaslIdentifiesBeforeJoin.
func TestSessionNickServOnlyWithoutSaslIdentifiesBeforeJoin(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.NickServPassword = "nick-secret"
	fixture := newSessionFixture(t, config)

	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :invite-notify\r\n" +
		":server 001 omairc :Welcome\r\n")
	frames := fixture.frames()
	if fixture.wrote("PASS nick-secret\r\n") {
		t.Fatal("no PASS is expected without a server password")
	}
	identify := indexOfFrame(frames, "PRIVMSG NickServ :IDENTIFY nick-secret\r\n")
	join := indexOfFrame(frames, "JOIN #omarchy\r\n")
	if identify < 0 || join < 0 || identify >= join {
		t.Fatalf("identify index %d join index %d, want identify before join", identify, join)
	}
}

// TestSessionBothSecretsSaslSendsPassAndPlainFromNickServ ports
// SessionTest::bothSecretsSaslSendsPassAndPlainFromNickServ.
func TestSessionBothSecretsSaslSendsPassAndPlainFromNickServ(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.Password = "server-secret"
	config.NickServPassword = "nick-secret"
	fixture := newSessionFixture(t, config)

	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :sasl=PLAIN\r\n")
	assertFramesEqual(t, fixture.frames(), []string{
		"CAP LS 302\r\n",
		"CAP REQ :sasl\r\n",
		"PASS server-secret\r\n",
		"NICK omairc\r\n",
		"USER omairc 8 * :Omairc User\r\n",
	})

	fixture.inject(":server CAP omairc ACK :sasl\r\n" +
		"AUTHENTICATE +\r\n")
	plain := string(decodeAuthenticatePayload(t, fixture.lastFrame()))
	if plain != "omairc\x00omairc\x00nick-secret" {
		t.Fatalf("SASL payload = %q, want the NickServ secret", plain)
	}
	if strings.Contains(plain, "server-secret") {
		t.Fatal("the server password must not appear in the SASL payload")
	}
}

// TestSessionBothSecretsWithoutSaslPassThenIdentifyBeforeJoin ports
// SessionTest::bothSecretsWithoutSaslPassThenIdentifyBeforeJoin.
func TestSessionBothSecretsWithoutSaslPassThenIdentifyBeforeJoin(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.Password = "server-secret"
	config.NickServPassword = "nick-secret"
	fixture := newSessionFixture(t, config)

	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :invite-notify\r\n" +
		":server 001 omairc :Welcome\r\n")
	frames := fixture.frames()
	if !fixture.wrote("PASS server-secret\r\n") {
		t.Fatal("the server password must ride PASS")
	}
	identify := indexOfFrame(frames, "PRIVMSG NickServ :IDENTIFY nick-secret\r\n")
	join := indexOfFrame(frames, "JOIN #omarchy\r\n")
	if identify < 0 || join < 0 || identify >= join {
		t.Fatalf("identify index %d join index %d, want identify before join", identify, join)
	}
}

// TestSessionSaslSuccessDoesNotIdentify ports
// SessionTest::saslSuccessDoesNotIdentify.
func TestSessionSaslSuccessDoesNotIdentify(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.Password = "server-secret"
	config.NickServPassword = "nick-secret"
	fixture := newSessionFixture(t, config)

	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :sasl=PLAIN\r\n" +
		":server CAP omairc ACK :sasl\r\n" +
		"AUTHENTICATE +\r\n" +
		":server 903 omairc :SASL successful\r\n" +
		":server 001 omairc :Welcome\r\n")
	if fixture.wrote("PRIVMSG NickServ :IDENTIFY nick-secret\r\n") {
		t.Fatal("a successful SASL must not IDENTIFY")
	}
	if !fixture.wrote("JOIN #omarchy\r\n") {
		t.Fatal("autojoin missing")
	}
}

// TestSessionSaslFailureDoesNotFallThroughToIdentify ports
// SessionTest::saslFailureDoesNotFallThroughToIdentify.
func TestSessionSaslFailureDoesNotFallThroughToIdentify(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.NickServPassword = "nick-secret"
	fixture := newSessionFixture(t, config)

	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :sasl=PLAIN\r\n" +
		":server CAP omairc ACK :sasl\r\n" +
		"AUTHENTICATE +\r\n" +
		":server 904 omairc :SASL failed\r\n" +
		":server 001 omairc :Welcome\r\n")
	if len(fixture.handler.errors) != 1 {
		t.Fatalf("errors = %d, want 1", len(fixture.handler.errors))
	}
	if got := fixture.session.State(); got != StateFailed {
		t.Fatalf("state = %v, want Failed", got)
	}
	if fixture.wrote("PRIVMSG NickServ :IDENTIFY nick-secret\r\n") {
		t.Fatal("a failed SASL must not fall through to IDENTIFY")
	}
	if fixture.wrote("JOIN #omarchy\r\n") {
		t.Fatal("a failed SASL must not autojoin")
	}
}

// TestSessionAuthenticationFailureIsExplicit ports
// SessionTest::authenticationFailureIsExplicit.
func TestSessionAuthenticationFailureIsExplicit(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.Password = "secret"
	fixture := newSessionFixture(t, config)

	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :sasl\r\n" +
		":server CAP omairc ACK :sasl\r\n" +
		":server 904 omairc :SASL failed\r\n")
	if got := fixture.session.State(); got != StateFailed {
		t.Fatalf("state = %v, want Failed", got)
	}
	last := fixture.handler.errors[len(fixture.handler.errors)-1]
	if last.kind != ErrorAuthentication {
		t.Fatalf("error kind = %v, want Authentication", last.kind)
	}
}

// --- SASL SCRAM-SHA-256 ---------------------------------------------------

// TestSessionNegotiatesSaslScramSha256 ports
// SessionTest::negotiatesSaslScramSha256.
func TestSessionNegotiatesSaslScramSha256(t *testing.T) {
	if got := scramServerFinalFor(t, scramTestClientFirst, scramTestServerFirst); got != scramTestServerFinal {
		t.Fatalf("scramServerFinal = %q, want %q", got, scramTestServerFinal)
	}

	config := sessionTestConfig(sessionTestNetworkID)
	config.SASLAccount = "user"
	config.Password = "pencil"
	config.NickServPassword = "pencil"
	fixture := newSessionFixture(t, config)

	clientFirst := string(startScramExchange(t, fixture))
	if !fixture.wrote("AUTHENTICATE SCRAM-SHA-256\r\n") {
		t.Fatal("SCRAM-SHA-256 must be chosen")
	}
	if fixture.wrote("AUTHENTICATE PLAIN\r\n") {
		t.Fatal("PLAIN must not be attempted")
	}
	if !strings.HasPrefix(clientFirst, "n,,n=user,r=") {
		t.Fatalf("client-first = %q, want the n,,n=user,r= prefix", clientFirst)
	}
	nonce := clientFirst[len("n,,n=user,r="):]
	if nonce == "" || strings.Contains(nonce, ",") {
		t.Fatalf("nonce = %q, want a non-empty comma-free value", nonce)
	}

	serverNonce := nonce + scramTestServerNonceTail
	serverFirst := "r=" + serverNonce + ",s=" + scramTestSalt + ",i=4096"
	expected := NewSASLScram()
	started := expected.Start("user", "pencil", []byte(nonce))
	if !started.OK() || string(started.Message) != clientFirst {
		t.Fatalf("reference client-first mismatch: %+v", started)
	}
	clientFinal := expected.TakeServerFirst([]byte(serverFirst))
	if !clientFinal.OK() {
		t.Fatalf("reference client-final failed: %s", clientFinal.Error)
	}
	if !strings.HasPrefix(string(clientFinal.Message), "c=biws,r="+serverNonce+",p=") {
		t.Fatalf("client-final = %q, want the c=biws,r=...,p= prefix", clientFinal.Message)
	}

	beforeFinal := len(fixture.frames())
	fixture.inject(saslWire([]byte(serverFirst)))
	if len(fixture.frames()) != beforeFinal+1 {
		t.Fatalf("frames written = %d, want 1", len(fixture.frames())-beforeFinal)
	}
	wireFinal := fixture.lastFrame()
	if want := "AUTHENTICATE " + base64Encode(clientFinal.Message) + "\r\n"; wireFinal != want {
		t.Fatalf("client-final frame = %q, want %q", wireFinal, want)
	}
	if got := decodeAuthenticatePayload(t, wireFinal); string(got) != string(clientFinal.Message) {
		t.Fatalf("decoded client-final = %q, want %q", got, clientFinal.Message)
	}

	fixture.inject(saslWire([]byte(scramServerFinalFor(t, clientFirst, serverFirst))) +
		":server 903 omairc :SASL successful\r\n" +
		":server 001 omairc :Welcome\r\n")
	if !fixture.wrote("AUTHENTICATE +\r\n") {
		t.Fatal("the empty AUTHENTICATE response is required")
	}
	if !fixture.wrote("CAP END\r\n") {
		t.Fatal("CAP END missing")
	}
	if fixture.wrote("AUTHENTICATE PLAIN\r\n") {
		t.Fatal("PLAIN must not be attempted")
	}
	if fixture.wrote("PRIVMSG NickServ :IDENTIFY pencil\r\n") {
		t.Fatal("a successful SCRAM must not IDENTIFY")
	}
	if !fixture.wrote("JOIN #omarchy\r\n") {
		t.Fatal("autojoin missing")
	}
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}

	proofAt := strings.Index(string(clientFinal.Message), ",p=")
	if proofAt < 0 {
		t.Fatal("client-final is missing the proof")
	}
	proof := string(clientFinal.Message)[proofAt+3:]
	if proof == "" {
		t.Fatal("proof must not be empty")
	}
	for _, entry := range fixture.handler.status {
		if strings.Contains(entry.Text(), "pencil") {
			t.Fatal("the password leaked into a Status entry")
		}
		if strings.Contains(entry.Text(), proof) {
			t.Fatal("the proof leaked into a Status entry")
		}
		if strings.Contains(entry.Label(), "AUTHENTICATE") {
			t.Fatal("AUTHENTICATE leaked into a Status label")
		}
	}
}

// TestSessionScramNumericBeforeServerFinalFails ports
// SessionTest::scramNumericBeforeServerFinalFails.
func TestSessionScramNumericBeforeServerFinalFails(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.SASLAccount = "user"
	config.Password = "pencil"
	config.NickServPassword = "pencil"
	fixture := newSessionFixture(t, config)

	clientFirst := string(startScramExchange(t, fixture))
	nonce := clientFirst[len("n,,n=user,r="):]
	serverFirst := "r=" + nonce + scramTestServerNonceTail + ",s=" + scramTestSalt + ",i=4096"
	fixture.inject(saslWire([]byte(serverFirst)))
	if got := fixture.session.State(); got != StateSasl {
		t.Fatalf("state = %v, want Sasl", got)
	}
	if len(fixture.handler.errors) != 0 {
		t.Fatalf("errors = %d, want 0", len(fixture.handler.errors))
	}
	if !strings.HasPrefix(fixture.lastFrame(), "AUTHENTICATE ") ||
		fixture.lastFrame() == "AUTHENTICATE SCRAM-SHA-256\r\n" {
		t.Fatalf("last frame = %q, want the client-final", fixture.lastFrame())
	}

	fixture.inject(":server 903 omairc :SASL successful\r\n" +
		":server 001 omairc :Welcome\r\n")
	if len(fixture.handler.errors) != 1 {
		t.Fatalf("errors = %d, want 1", len(fixture.handler.errors))
	}
	if got := fixture.handler.errors[0]; got.kind != ErrorAuthentication {
		t.Fatalf("error kind = %v, want Authentication", got.kind)
	} else if strings.Contains(got.message, "pencil") {
		t.Fatal("the password leaked into the error")
	}
	if got := fixture.session.State(); got != StateFailed {
		t.Fatalf("state = %v, want Failed", got)
	}
	if fixture.wrote("CAP END\r\n") || fixture.wrote("AUTHENTICATE PLAIN\r\n") ||
		fixture.wrote("PRIVMSG NickServ :IDENTIFY pencil\r\n") ||
		fixture.wrote("JOIN #omarchy\r\n") {
		t.Fatal("a failed SCRAM must not register")
	}
}

// TestSessionScramFailureDoesNotFallThroughToPlain ports
// SessionTest::scramFailureDoesNotFallThroughToPlain.
func TestSessionScramFailureDoesNotFallThroughToPlain(t *testing.T) {
	aborts := []string{
		":server 904 omairc :SASL failed\r\n:server 001 omairc :Welcome\r\n",
		":server 905 omairc :SASL message too long\r\n:server 001 omairc :Welcome\r\n",
		"AUTHENTICATE *\r\n:server 001 omairc :Welcome\r\n",
	}
	for _, abort := range aborts {
		config := sessionTestConfig(sessionTestNetworkID)
		config.SASLAccount = "user"
		config.NickServPassword = "pencil"
		fixture := newSessionFixture(t, config)

		startScramExchange(t, fixture)
		if !fixture.wrote("AUTHENTICATE SCRAM-SHA-256\r\n") {
			t.Fatal("SCRAM-SHA-256 must be chosen")
		}

		fixture.inject(abort)
		if len(fixture.handler.errors) != 1 {
			t.Fatalf("errors = %d, want 1", len(fixture.handler.errors))
		}
		if got := fixture.handler.errors[0]; got.kind != ErrorAuthentication {
			t.Fatalf("error kind = %v, want Authentication", got.kind)
		} else if strings.Contains(got.message, "pencil") {
			t.Fatal("the password leaked into the error")
		}
		if got := fixture.session.State(); got != StateFailed {
			t.Fatalf("state = %v, want Failed", got)
		}
		if fixture.wrote("AUTHENTICATE PLAIN\r\n") || fixture.wrote("CAP END\r\n") ||
			fixture.wrote("PRIVMSG NickServ :IDENTIFY pencil\r\n") ||
			fixture.wrote("JOIN #omarchy\r\n") {
			t.Fatal("a failed SCRAM must not fall through")
		}
	}
}

// TestSessionScramPlainReadvertisementDoesNotFinishExchange ports
// SessionTest::scramPlainReadvertisementDoesNotFinishExchange.
func TestSessionScramPlainReadvertisementDoesNotFinishExchange(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.SASLAccount = "user"
	config.Password = "pencil"
	config.NickServPassword = "pencil"
	fixture := newSessionFixture(t, config)

	startScramExchange(t, fixture)
	if !fixture.wrote("AUTHENTICATE SCRAM-SHA-256\r\n") {
		t.Fatal("SCRAM-SHA-256 must be chosen")
	}
	before := len(fixture.frames())

	fixture.inject(":server CAP omairc DEL :sasl\r\n" +
		":server CAP omairc NEW :sasl=PLAIN\r\n" +
		":server CAP omairc ACK :sasl\r\n" +
		":server 903 omairc :SASL successful\r\n" +
		":server 001 omairc :Welcome\r\n")

	for _, frame := range fixture.writtenSince(before) {
		if strings.HasPrefix(frame, "AUTHENTICATE ") {
			t.Fatalf("unexpected AUTHENTICATE frame %q", frame)
		}
	}
	if fixture.wrote("AUTHENTICATE PLAIN\r\n") {
		t.Fatal("PLAIN must not be attempted")
	}
	if len(fixture.handler.errors) != 1 {
		t.Fatalf("errors = %d, want 1", len(fixture.handler.errors))
	}
	if got := fixture.handler.errors[0]; got.kind != ErrorAuthentication {
		t.Fatalf("error kind = %v, want Authentication", got.kind)
	} else if strings.Contains(got.message, "pencil") {
		t.Fatal("the password leaked into the error")
	}
	if got := fixture.session.State(); got != StateFailed {
		t.Fatalf("state = %v, want Failed", got)
	}
	if fixture.wrote("CAP END\r\n") ||
		fixture.wrote("PRIVMSG NickServ :IDENTIFY pencil\r\n") ||
		fixture.wrote("JOIN #omarchy\r\n") {
		t.Fatal("a failed SCRAM must not register")
	}
}

// TestSessionScramServerFirstIllegalBase64Fails ports
// SessionTest::scramServerFirstIllegalBase64Fails.
func TestSessionScramServerFirstIllegalBase64Fails(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.SASLAccount = "user"
	config.Password = "pencil"
	fixture := newSessionFixture(t, config)

	clientFirst := string(startScramExchange(t, fixture))
	nonce := clientFirst[len("n,,n=user,r="):]
	serverFirst := "r=" + nonce + scramTestServerNonceTail + ",s=" + scramTestSalt + ",i=4096"
	wire := saslWire([]byte(serverFirst))
	if !strings.HasPrefix(wire, "AUTHENTICATE ") || !strings.HasSuffix(wire, "\r\n") {
		t.Fatalf("wire = %q, want an AUTHENTICATE frame", wire)
	}
	wire = wire[:len(wire)-2] + "!" + "\r\n"

	before := len(fixture.frames())
	fixture.inject(wire)

	if len(fixture.frames()) != before {
		t.Fatalf("frames written = %d, want 0", len(fixture.frames())-before)
	}
	if len(fixture.handler.errors) != 1 {
		t.Fatalf("errors = %d, want 1", len(fixture.handler.errors))
	}
	if got := fixture.handler.errors[0]; got.kind != ErrorAuthentication {
		t.Fatalf("error kind = %v, want Authentication", got.kind)
	} else if strings.Contains(got.message, "pencil") {
		t.Fatal("the password leaked into the error")
	}
	if got := fixture.session.State(); got != StateFailed {
		t.Fatalf("state = %v, want Failed", got)
	}
	if fixture.wrote("AUTHENTICATE PLAIN\r\n") || fixture.wrote("CAP END\r\n") {
		t.Fatal("a failed SCRAM must not register")
	}
}

// TestSessionScramSaslReassemblesChunkedMessages ports
// SessionTest::scramSaslReassemblesChunkedMessages.
func TestSessionScramSaslReassemblesChunkedMessages(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.SASLAccount = "user"
	config.Password = "pencil"
	fixture := newSessionFixture(t, config)

	clientFirst := string(startScramExchange(t, fixture))
	nonce := clientFirst[len("n,,n=user,r="):]
	target := 600
	base := 56 + len(nonce)
	for target <= base {
		target += 300
	}
	serverNonce := nonce + strings.Repeat("A", target-base)
	serverFirst := "r=" + serverNonce + ",s=" + scramTestSalt + ",i=4096"
	if len(base64Encode([]byte(serverFirst))) <= 400 {
		t.Fatal("server-first must exceed one chunk")
	}

	expected := NewSASLScram()
	if !expected.Start("user", "pencil", []byte(nonce)).OK() {
		t.Fatal("reference start failed")
	}
	clientFinal := expected.TakeServerFirst([]byte(serverFirst))
	if !clientFinal.OK() {
		t.Fatalf("reference client-final failed: %s", clientFinal.Error)
	}
	encodedFinal := base64Encode(clientFinal.Message)
	if len(encodedFinal)%400 != 0 || len(encodedFinal) < 800 {
		t.Fatalf("encoded final length = %d, want >= 800 and a multiple of 400", len(encodedFinal))
	}

	before := len(fixture.frames())
	fixture.inject(saslWire([]byte(serverFirst)))
	frames := fixture.frames()
	chunks := len(encodedFinal) / 400
	if len(frames) != before+chunks+1 {
		t.Fatalf("frames written = %d, want %d", len(frames)-before, chunks+1)
	}
	var reassembled strings.Builder
	for index := 0; index < chunks; index++ {
		body := authenticateBody(frames[before+index])
		if len(body) != 400 {
			t.Fatalf("chunk %d = %d bytes, want 400", index, len(body))
		}
		reassembled.WriteString(body)
	}
	if got := frames[before+chunks]; got != "AUTHENTICATE +\r\n" {
		t.Fatalf("trailing frame = %q, want AUTHENTICATE +", got)
	}
	if got := reassembled.String(); got != encodedFinal {
		t.Fatalf("reassembled = %q, want %q", got, encodedFinal)
	}

	fixture.inject(saslWire([]byte(scramServerFinalFor(t, clientFirst, serverFirst))) +
		":server 903 omairc :SASL successful\r\n")
	if !fixture.wrote("CAP END\r\n") {
		t.Fatal("CAP END missing")
	}
	if fixture.wrote("AUTHENTICATE PLAIN\r\n") {
		t.Fatal("PLAIN must not be attempted")
	}
	if got := fixture.session.State(); got != StateRegistering {
		t.Fatalf("state = %v, want Registering", got)
	}
}

// TestSessionScramWelcomeBeforeVerifierFails ports
// SessionTest::scramWelcomeBeforeVerifierFails.
func TestSessionScramWelcomeBeforeVerifierFails(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.SASLAccount = "user"
	config.Password = "pencil"
	config.NickServPassword = "pencil"
	fixture := newSessionFixture(t, config)

	clientFirst := string(startScramExchange(t, fixture))
	if !strings.HasPrefix(clientFirst, "n,,n=user,r=") {
		t.Fatalf("client-first = %q, want the n,,n=user,r= prefix", clientFirst)
	}
	if got := fixture.session.State(); got != StateSasl {
		t.Fatalf("state = %v, want Sasl", got)
	}

	fixture.inject(":server 001 omairc :Welcome\r\n" +
		":server 903 omairc :SASL successful\r\n")
	if len(fixture.handler.errors) != 1 {
		t.Fatalf("errors = %d, want 1", len(fixture.handler.errors))
	}
	if got := fixture.handler.errors[0]; got.kind != ErrorAuthentication {
		t.Fatalf("error kind = %v, want Authentication", got.kind)
	} else if strings.Contains(got.message, "pencil") {
		t.Fatal("the password leaked into the error")
	}
	if got := fixture.session.State(); got != StateFailed {
		t.Fatalf("state = %v, want Failed", got)
	}
	if fixture.wrote("CAP END\r\n") || fixture.wrote("AUTHENTICATE PLAIN\r\n") ||
		fixture.wrote("PRIVMSG NickServ :IDENTIFY pencil\r\n") ||
		fixture.wrote("JOIN #omarchy\r\n") || fixture.wrote("JOIN &local\r\n") {
		t.Fatal("a failed SCRAM must not register")
	}
}

// TestSessionAbandonedScramBeforeAuthenticateRegisters ports
// SessionTest::abandonedScramBeforeAuthenticateRegisters.
func TestSessionAbandonedScramBeforeAuthenticateRegisters(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.SASLAccount = "user"
	config.Password = "server-secret"
	config.NickServPassword = "nick-secret"
	fixture := newSessionFixture(t, config)

	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :sasl=SCRAM-SHA-256\r\n")
	if !fixture.wrote("CAP REQ :sasl\r\n") {
		t.Fatal("sasl must be requested")
	}
	if fixture.clock.Pending() == 0 {
		t.Fatal("capability timer must be pending")
	}
	if fixture.wrote("AUTHENTICATE SCRAM-SHA-256\r\n") {
		t.Fatal("AUTHENTICATE must wait for the ACK")
	}
	if fixture.wrote("CAP END\r\n") {
		t.Fatal("CAP END must wait for SASL")
	}
	if got := fixture.session.State(); got != StateRegistering {
		t.Fatalf("state = %v, want Registering", got)
	}

	fixture.clock.Advance(10000 * time.Millisecond)
	if !fixture.wrote("CAP END\r\n") {
		t.Fatal("CAP END must follow the capability timeout")
	}
	if fixture.wrote("AUTHENTICATE SCRAM-SHA-256\r\n") || fixture.wrote("AUTHENTICATE PLAIN\r\n") {
		t.Fatal("no AUTHENTICATE is expected")
	}
	if got := fixture.session.State(); got != StateRegistering {
		t.Fatalf("state = %v, want Registering", got)
	}
	if len(fixture.handler.errors) != 0 {
		t.Fatalf("errors = %d, want 0", len(fixture.handler.errors))
	}

	fixture.inject(":server 001 omairc :Welcome\r\n")
	if len(fixture.handler.errors) != 0 {
		t.Fatalf("errors = %d, want 0", len(fixture.handler.errors))
	}
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}
	if fixture.wrote("PRIVMSG NickServ :IDENTIFY nick-secret\r\n") {
		t.Fatal("an abandoned SCRAM must not IDENTIFY")
	}
	if !fixture.wrote("JOIN #omarchy\r\n") {
		t.Fatal("autojoin missing")
	}
}

// TestSessionScramSaslChunkLimitFails ports
// SessionTest::scramSaslChunkLimitFails.
func TestSessionScramSaslChunkLimitFails(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.SASLAccount = "user"
	config.Password = "pencil"
	fixture := newSessionFixture(t, config)

	startScramExchange(t, fixture)
	if got := fixture.session.State(); got != StateSasl {
		t.Fatalf("state = %v, want Sasl", got)
	}
	before := len(fixture.frames())

	const (
		chunk = 400
		limit = 4096
	)
	var wire strings.Builder
	sent := 0
	for sent <= limit {
		wire.WriteString("AUTHENTICATE ")
		wire.WriteString(strings.Repeat("A", chunk))
		wire.WriteString("\r\n")
		sent += chunk
	}
	if sent <= limit {
		t.Fatalf("sent = %d, want > %d", sent, limit)
	}

	fixture.inject(wire.String())
	if len(fixture.handler.errors) != 1 {
		t.Fatalf("errors = %d, want 1", len(fixture.handler.errors))
	}
	if got := fixture.handler.errors[0]; got.kind != ErrorAuthentication {
		t.Fatalf("error kind = %v, want Authentication", got.kind)
	} else if strings.Contains(got.message, "pencil") {
		t.Fatal("the password leaked into the error")
	}
	if got := fixture.session.State(); got != StateFailed {
		t.Fatalf("state = %v, want Failed", got)
	}
	if len(fixture.frames()) != before {
		t.Fatalf("frames written = %d, want 0", len(fixture.frames())-before)
	}
	if fixture.wrote("AUTHENTICATE PLAIN\r\n") || fixture.wrote("CAP END\r\n") {
		t.Fatal("a failed SCRAM must not register")
	}
}

// indexOfFrame returns the index of an exact frame, or -1.
func indexOfFrame(frames []string, frame string) int {
	for index, candidate := range frames {
		if candidate == frame {
			return index
		}
	}
	return -1
}
