package session

import (
	"bytes"
	"strings"
	"testing"
	"time"
)

// Fixed RFC 7677 test vector, mirroring the constants in tst_scram.cpp.
const (
	scramTestNonce       = "rOprNGfwEbeRWgbNEkqO"
	scramTestClientFirst = "n,,n=user,r=rOprNGfwEbeRWgbNEkqO"
	scramTestServerFirst = "r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0," +
		"s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096"
	scramTestClientFinal = "c=biws,r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0," +
		"p=dHzbZapWIk4jUhN+Ute9ytag9zjfMHgsqmmiz7AndVQ="
	scramTestServerFinal = "v=6rriTRBi23WpRR/wtup+mMhUZUn/dB5nLTJRsjl95G4="
	scramTestSalt        = "W22ZaJ0SNY7soEsUEjb6gQ=="
	// scramTestServerNonceTail is the fixed server nonce suffix the C++ suite
	// appends to the client nonce (kRfcServerNonceTail).
	scramTestServerNonceTail = "%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0"
)

// scramHidesPassword reports whether neither the error nor the message leaks
// the test password, mirroring hidesPassword in tst_scram.cpp.
func scramHidesPassword(result ScramResult) bool {
	return !strings.Contains(result.Error, "pencil") &&
		!strings.Contains(string(result.Message), "pencil")
}

// scramServerFirstWithIteration builds a server-first message with a custom
// iteration count.
func scramServerFirstWithIteration(iteration string) []byte {
	return []byte("r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,s=" +
		scramTestSalt + ",i=" + iteration)
}

func assertScramFailure(t *testing.T, result ScramResult) {
	t.Helper()
	if result.OK() {
		t.Fatal("result unexpectedly succeeded")
	}
	if len(result.Message) != 0 {
		t.Fatalf("message = %q, want empty", result.Message)
	}
	if !scramHidesPassword(result) {
		t.Fatalf("result leaked the password: %+v", result)
	}
}

// TestScramRFC7677VectorVerifiesServerSignature ports
// ScramTest::rfc7677VectorVerifiesServerSignature.
func TestScramRFC7677VectorVerifiesServerSignature(t *testing.T) {
	scram := NewSASLScram()
	started := scram.Start("user", "pencil", []byte(scramTestNonce))
	if !started.OK() {
		t.Fatalf("start failed: %s", started.Error)
	}
	if string(started.Message) != scramTestClientFirst {
		t.Fatalf("client-first = %q, want %q", started.Message, scramTestClientFirst)
	}
	if !scramHidesPassword(started) {
		t.Fatalf("start leaked the password: %+v", started)
	}

	clientFinal := scram.TakeServerFirst([]byte(scramTestServerFirst))
	if !clientFinal.OK() {
		t.Fatalf("client-final failed: %s", clientFinal.Error)
	}
	if string(clientFinal.Message) != scramTestClientFinal {
		t.Fatalf("client-final = %q, want %q", clientFinal.Message, scramTestClientFinal)
	}
	if !scramHidesPassword(clientFinal) {
		t.Fatalf("client-final leaked the password: %+v", clientFinal)
	}

	serverFinal := scram.TakeServerFinal([]byte(scramTestServerFinal))
	if !serverFinal.OK() {
		t.Fatalf("server-final failed: %s", serverFinal.Error)
	}
	if len(serverFinal.Message) != 0 {
		t.Fatalf("server-final message = %q, want empty", serverFinal.Message)
	}
	if !scramHidesPassword(serverFinal) {
		t.Fatalf("server-final leaked the password: %+v", serverFinal)
	}
}

// TestScramWrongServerSignatureFails ports
// ScramTest::wrongServerSignatureFails.
func TestScramWrongServerSignatureFails(t *testing.T) {
	scram := NewSASLScram()
	if !scram.Start("user", "pencil", []byte(scramTestNonce)).OK() {
		t.Fatal("start failed")
	}
	if !scram.TakeServerFirst([]byte(scramTestServerFirst)).OK() {
		t.Fatal("client-final failed")
	}

	rejected := scram.TakeServerFinal(
		[]byte("v=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="))
	assertScramFailure(t, rejected)
}

// TestScramServerNonceMustContinueClientNonce ports
// ScramTest::serverNonceMustContinueClientNonce.
func TestScramServerNonceMustContinueClientNonce(t *testing.T) {
	scram := NewSASLScram()
	if !scram.Start("user", "pencil", []byte(scramTestNonce)).OK() {
		t.Fatal("start failed")
	}

	serverFirst := "r=XXXXXXXXXXXXXXXXXXXX%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,s=" +
		scramTestSalt + ",i=4096"
	assertScramFailure(t, scram.TakeServerFirst([]byte(serverFirst)))
}

// TestScramNonceEqualsSignsBelongToTheValue ports
// ScramTest::nonceEqualsSignsBelongToTheValue.
func TestScramNonceEqualsSignsBelongToTheValue(t *testing.T) {
	scram := NewSASLScram()
	if !scram.Start("user", "pencil", []byte("abc")).OK() {
		t.Fatal("start failed")
	}

	serverFirst := "r=abc=def=ghi,s=" + scramTestSalt + ",i=4096"
	clientFinal := scram.TakeServerFirst([]byte(serverFirst))
	if !clientFinal.OK() {
		t.Fatalf("client-final failed: %s", clientFinal.Error)
	}
	if !strings.HasPrefix(string(clientFinal.Message), "c=biws,r=abc=def=ghi,p=") {
		t.Fatalf("client-final = %q, want the r=abc=def=ghi prefix", clientFinal.Message)
	}
	if !scramHidesPassword(clientFinal) {
		t.Fatalf("client-final leaked the password: %+v", clientFinal)
	}
}

// TestScramIterationCountBelowMinimumFails ports
// ScramTest::iterationCountBelowMinimumFails.
func TestScramIterationCountBelowMinimumFails(t *testing.T) {
	scram := NewSASLScram()
	if !scram.Start("user", "pencil", []byte(scramTestNonce)).OK() {
		t.Fatal("start failed")
	}

	rejected := scram.TakeServerFirst(scramServerFirstWithIteration("4095"))
	assertScramFailure(t, rejected)
}

// TestScramIterationCountAboveMaximumFailsWithoutPbkdf2 ports
// ScramTest::iterationCountAboveMaximumFailsWithoutPbkdf2.
func TestScramIterationCountAboveMaximumFailsWithoutPbkdf2(t *testing.T) {
	scram := NewSASLScram()
	if !scram.Start("user", "pencil", []byte(scramTestNonce)).OK() {
		t.Fatal("start failed")
	}

	started := time.Now()
	rejected := scram.TakeServerFirst(scramServerFirstWithIteration("1000001"))
	elapsed := time.Since(started)
	assertScramFailure(t, rejected)
	if elapsed >= 200*time.Millisecond {
		t.Fatalf("elapsed = %v, want < 200ms: iteration count above 1000000 must fail before PBKDF2", elapsed)
	}
}

// TestScramUsernameEscapesEqualsAndComma ports
// ScramTest::usernameEscapesEqualsAndComma.
func TestScramUsernameEscapesEqualsAndComma(t *testing.T) {
	scram := NewSASLScram()
	started := scram.Start("a=b,c", "pencil", []byte("nonce"))
	if !started.OK() {
		t.Fatalf("start failed: %s", started.Error)
	}
	if string(started.Message) != "n,,n=a=3Db=2Cc,r=nonce" {
		t.Fatalf("client-first = %q, want the escaped name", started.Message)
	}
	if !scramHidesPassword(started) {
		t.Fatalf("start leaked the password: %+v", started)
	}
}

// TestScramAccountOrPasswordWithControlBytesIsRejected ports
// ScramTest::accountOrPasswordWithControlBytesIsRejected.
func TestScramAccountOrPasswordWithControlBytesIsRejected(t *testing.T) {
	accounts := []string{
		"user\rname",
		"user\nname",
		"\x00",
		"us\x00er",
	}
	for _, account := range accounts {
		scram := NewSASLScram()
		rejected := scram.Start(account, "pencil", nil)
		if rejected.OK() {
			t.Fatalf("start(%q) unexpectedly succeeded", account)
		}
		if len(rejected.Message) != 0 {
			t.Fatalf("start(%q) message = %q, want empty", account, rejected.Message)
		}
		if !scramHidesPassword(rejected) {
			t.Fatalf("start(%q) leaked the password: %+v", account, rejected)
		}
		if scram.TakeServerFirst([]byte(scramTestServerFirst)).OK() {
			t.Fatalf("takeServerFirst after rejected start(%q) succeeded", account)
		}
	}

	passwords := []string{
		"pencil\r",
		"pencil\n",
		"pencil\x00",
	}
	for _, password := range passwords {
		scram := NewSASLScram()
		rejected := scram.Start("user", password, nil)
		if rejected.OK() {
			t.Fatalf("start with password %q unexpectedly succeeded", password)
		}
		if len(rejected.Message) != 0 {
			t.Fatalf("start with password %q message = %q, want empty", password, rejected.Message)
		}
		if strings.Contains(rejected.Error, "pencil") {
			t.Fatalf("start with password %q leaked the password", password)
		}
	}
}

// TestScramServerErrorAttributeFails ports
// ScramTest::serverErrorAttributeFails.
func TestScramServerErrorAttributeFails(t *testing.T) {
	scram := NewSASLScram()
	if !scram.Start("user", "pencil", []byte(scramTestNonce)).OK() {
		t.Fatal("start failed")
	}
	assertScramFailure(t, scram.TakeServerFirst([]byte("e=pencil")))

	again := NewSASLScram()
	if !again.Start("user", "pencil", []byte(scramTestNonce)).OK() {
		t.Fatal("start failed")
	}
	if !again.TakeServerFirst([]byte(scramTestServerFirst)).OK() {
		t.Fatal("client-final failed")
	}
	assertScramFailure(t, again.TakeServerFinal([]byte("e=pencil")))
}

// TestScramMissingOrGarbageAttributeFails ports
// ScramTest::missingOrGarbageAttributeFails.
func TestScramMissingOrGarbageAttributeFails(t *testing.T) {
	missing := NewSASLScram()
	if !missing.Start("user", "pencil", []byte(scramTestNonce)).OK() {
		t.Fatal("start failed")
	}
	noSalt := missing.TakeServerFirst(
		[]byte("r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,i=4096"))
	if noSalt.OK() || len(noSalt.Message) != 0 {
		t.Fatalf("no-salt takeServerFirst = %+v, want failure", noSalt)
	}

	garbage := NewSASLScram()
	if !garbage.Start("user", "pencil", []byte(scramTestNonce)).OK() {
		t.Fatal("start failed")
	}
	extra := garbage.TakeServerFirst([]byte(scramTestServerFirst + ",m=reserved"))
	if extra.OK() || len(extra.Message) != 0 {
		t.Fatalf("extra-attribute takeServerFirst = %+v, want failure", extra)
	}
	if !scramHidesPassword(extra) {
		t.Fatalf("takeServerFirst leaked the password: %+v", extra)
	}
}

// TestScramVerifierOrSaltWithTrailingJunkFails ports
// ScramTest::verifierOrSaltWithTrailingJunkFails.
func TestScramVerifierOrSaltWithTrailingJunkFails(t *testing.T) {
	salt := NewSASLScram()
	if !salt.Start("user", "pencil", []byte(scramTestNonce)).OK() {
		t.Fatal("start failed")
	}
	salted := salt.TakeServerFirst(
		[]byte("r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,s=" +
			scramTestSalt + "!,i=4096"))
	if salted.OK() || len(salted.Message) != 0 {
		t.Fatalf("salted takeServerFirst = %+v, want failure", salted)
	}
	if !scramHidesPassword(salted) {
		t.Fatalf("takeServerFirst leaked the password: %+v", salted)
	}

	verifier := NewSASLScram()
	if !verifier.Start("user", "pencil", []byte(scramTestNonce)).OK() {
		t.Fatal("start failed")
	}
	if !verifier.TakeServerFirst([]byte(scramTestServerFirst)).OK() {
		t.Fatal("client-final failed")
	}
	rejected := verifier.TakeServerFinal([]byte(scramTestServerFinal + "!"))
	assertScramFailure(t, rejected)
}

// TestScramServerFinalBeforeServerFirstFails ports
// ScramTest::serverFinalBeforeServerFirstFails.
func TestScramServerFinalBeforeServerFirstFails(t *testing.T) {
	scram := NewSASLScram()
	if !scram.Start("user", "pencil", []byte(scramTestNonce)).OK() {
		t.Fatal("start failed")
	}
	early := scram.TakeServerFinal([]byte(scramTestServerFinal))
	assertScramFailure(t, early)

	clientFinal := scram.TakeServerFirst([]byte(scramTestServerFirst))
	if !clientFinal.OK() {
		t.Fatalf("client-final failed: %s", clientFinal.Error)
	}
	if string(clientFinal.Message) != scramTestClientFinal {
		t.Fatalf("client-final = %q, want %q", clientFinal.Message, scramTestClientFinal)
	}
}

// TestScramSecondCallInTheWrongStateFails ports
// ScramTest::secondCallInTheWrongStateFails.
func TestScramSecondCallInTheWrongStateFails(t *testing.T) {
	scram := NewSASLScram()
	if !scram.Start("user", "pencil", []byte(scramTestNonce)).OK() {
		t.Fatal("start failed")
	}
	if !scram.TakeServerFirst([]byte(scramTestServerFirst)).OK() {
		t.Fatal("client-final failed")
	}
	repeated := scram.TakeServerFirst([]byte(scramTestServerFirst))
	if repeated.OK() || len(repeated.Message) != 0 {
		t.Fatalf("repeated takeServerFirst = %+v, want failure", repeated)
	}

	if !scram.TakeServerFinal([]byte(scramTestServerFinal)).OK() {
		t.Fatal("server-final failed")
	}
	again := scram.TakeServerFinal([]byte(scramTestServerFinal))
	if again.OK() || len(again.Message) != 0 {
		t.Fatalf("repeated takeServerFinal = %+v, want failure", again)
	}
}

// TestScramGeneratedNonceIsPrintableAndHasNoComma ports
// ScramTest::generatedNonceIsPrintableAndHasNoComma.
func TestScramGeneratedNonceIsPrintableAndHasNoComma(t *testing.T) {
	scram := NewSASLScram()
	started := scram.Start("user", "pencil", nil)
	if !started.OK() {
		t.Fatalf("start failed: %s", started.Error)
	}
	const prefix = "n,,n=user,r="
	if !bytes.HasPrefix(started.Message, []byte(prefix)) {
		t.Fatalf("client-first = %q, want the %q prefix", started.Message, prefix)
	}
	nonce := started.Message[len(prefix):]
	if len(nonce) < 16 {
		t.Fatalf("nonce = %q, want at least 16 bytes", nonce)
	}
	for _, value := range nonce {
		if value < 0x21 || value > 0x7E {
			t.Fatalf("nonce byte %#x is not printable ASCII", value)
		}
		if value == ',' {
			t.Fatal("nonce must not contain a comma")
		}
	}

	other := NewSASLScram()
	again := other.Start("user", "pencil", nil)
	if !again.OK() {
		t.Fatalf("start failed: %s", again.Error)
	}
	if bytes.Equal(again.Message, started.Message) {
		t.Fatal("two generated nonces must differ")
	}
}
