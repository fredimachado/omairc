package session

import (
	"bytes"
	"crypto/hmac"
	"crypto/pbkdf2"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"strconv"
)

// SCRAM failure strings, ported verbatim from src/irc/ircsaslscram.cpp so the
// mirrored test matrix can match them. None of them may contain the password or
// the server's free-form error text.
const (
	errAccountUnsendable  = "account or password cannot be sent"
	errClientNonce        = "client nonce cannot be sent"
	errNotAwaitingFirst   = "SCRAM exchange is not waiting for the server first message"
	errNotAwaitingFinal   = "SCRAM exchange is not waiting for the server final message"
	errMissingAttributes  = "server message is missing SCRAM attributes"
	errMalformedAttribute = "server message has a malformed SCRAM attribute"
	errRepeatedAttribute  = "server message repeats a SCRAM attribute"
	errServerRejected     = "the server rejected authentication"
	errNonceNotContinued  = "server nonce does not continue the client nonce"
	errIterationsRange    = "iteration count is outside the allowed range"
	errProofDerivation    = "SCRAM proof could not be derived"
	errSignatureMismatch  = "server signature does not match"
)

// Iteration bounds from ircsaslscram.cpp. A count outside them is rejected
// before PBKDF2 runs, so a hostile peer cannot burn CPU.
const (
	minIterations = 4096
	maxIterations = 1000000
)

// ScramResult is one SCRAM step outcome. Message is the raw SCRAM bytes to
// hand to SASL base64 framing; the session adds the IRC framing. Error is empty
// on success.
type ScramResult struct {
	Message []byte
	Error   string
}

// OK reports whether the step succeeded.
func (r ScramResult) OK() bool {
	return r.Error == ""
}

type scramStep int

const (
	scramIdle scramStep = iota
	scramAwaitServerFirst
	scramAwaitServerFinal
)

// SASLScram is a stateful SCRAM-SHA-256 client (RFC 5802, RFC 7677). It mirrors
// IrcSaslScram in src/irc/ircsaslscram.*: Start produces the client-first
// message, TakeServerFirst produces client-final, and TakeServerFinal verifies
// the server signature. Messages are raw SCRAM bytes; the IRC session applies
// SASL base64 framing.
//
// The zero value is an idle client, but NewSASLScram is the documented
// constructor.
type SASLScram struct {
	step            scramStep
	password        []byte
	clientNonce     []byte
	clientFirstBare []byte
	authMessage     []byte
	serverKey       []byte
}

// NewSASLScram returns an idle SCRAM-SHA-256 client.
func NewSASLScram() *SASLScram {
	return &SASLScram{}
}

// Start begins the exchange. An empty clientNonce is generated; tests pass a
// fixed nonce so the exchange matches a known vector.
func (c *SASLScram) Start(account, password string, clientNonce []byte) ScramResult {
	c.reset()
	if account == "" || containsUnsendable(account) || containsUnsendable(password) {
		return scramFailure(errAccountUnsendable)
	}

	nonce := clientNonce
	if len(nonce) == 0 {
		nonce = generateNonce()
	}
	if !acceptableNonce(nonce) {
		return scramFailure(errClientNonce)
	}

	escaped := escapeSaslName([]byte(account))
	if len(escaped) == 0 {
		return scramFailure(errAccountUnsendable)
	}

	clientFirstBare := make([]byte, 0, 2+len(escaped)+3+len(nonce))
	clientFirstBare = append(clientFirstBare, "n="...)
	clientFirstBare = append(clientFirstBare, escaped...)
	clientFirstBare = append(clientFirstBare, ",r="...)
	clientFirstBare = append(clientFirstBare, nonce...)

	c.clientNonce = cloneBytes(nonce)
	c.clientFirstBare = clientFirstBare
	c.password = []byte(password)
	c.step = scramAwaitServerFirst

	response := make([]byte, 0, 3+len(clientFirstBare))
	response = append(response, "n,,"...)
	response = append(response, clientFirstBare...)
	return scramSuccess(response)
}

// TakeServerFirst consumes the server-first message and returns client-final,
// including the client proof.
func (c *SASLScram) TakeServerFirst(message []byte) ScramResult {
	if c.step != scramAwaitServerFirst {
		return scramFailure(errNotAwaitingFirst)
	}

	var attributes scramAttributes
	if parseError := parseScramAttributes(message, "rsi", &attributes); parseError != "" {
		c.reset()
		return scramFailure(parseError)
	}
	if attributes.serverError {
		c.reset()
		return scramFailure(errServerRejected)
	}
	if len(attributes.nonce) == 0 || len(attributes.salt) == 0 || len(attributes.iterations) == 0 {
		c.reset()
		return scramFailure(errMissingAttributes)
	}
	if !bytes.HasPrefix(attributes.nonce, c.clientNonce) {
		c.reset()
		return scramFailure(errNonceNotContinued)
	}

	iterations, parseError := parseIterationCount(attributes.iterations)
	if parseError != "" {
		c.reset()
		return scramFailure(parseError)
	}

	salt, decodeError := decodeBase64(attributes.salt)
	if decodeError != "" {
		c.reset()
		return scramFailure(decodeError)
	}

	const digestLength = sha256.Size
	saltedPassword, pbkdf2Error := pbkdf2.Key(sha256.New, string(c.password), salt, iterations, digestLength)
	c.wipePassword()
	if pbkdf2Error != nil || len(saltedPassword) != digestLength {
		wipeBytes(saltedPassword)
		c.reset()
		return scramFailure(errProofDerivation)
	}

	clientKey := hmacSHA256([]byte("Client Key"), saltedPassword)
	storedKey := sha256.Sum256(clientKey)
	c.serverKey = hmacSHA256([]byte("Server Key"), saltedPassword)
	wipeBytes(saltedPassword)

	withoutProof := make([]byte, 0, 2+len("biws")+3+len(attributes.nonce))
	withoutProof = append(withoutProof, "c="...)
	withoutProof = append(withoutProof, base64.StdEncoding.EncodeToString([]byte("n,,"))...)
	withoutProof = append(withoutProof, ",r="...)
	withoutProof = append(withoutProof, attributes.nonce...)

	authMessage := make([]byte, 0, len(c.clientFirstBare)+len(message)+len(withoutProof)+2)
	authMessage = append(authMessage, c.clientFirstBare...)
	authMessage = append(authMessage, ',')
	authMessage = append(authMessage, message...)
	authMessage = append(authMessage, ',')
	authMessage = append(authMessage, withoutProof...)
	c.authMessage = authMessage

	clientSignature := hmacSHA256(c.authMessage, storedKey[:])
	proof := xorBytes(clientKey, clientSignature)
	wipeBytes(clientKey)
	wipeBytes(storedKey[:])
	wipeBytes(clientSignature)
	if len(proof) != digestLength {
		wipeBytes(proof)
		c.reset()
		return scramFailure(errProofDerivation)
	}

	clientFinal := append(append([]byte(nil), withoutProof...), ",p="...)
	clientFinal = append(clientFinal, base64.StdEncoding.EncodeToString(proof)...)
	wipeBytes(proof)
	c.step = scramAwaitServerFinal
	return scramSuccess(clientFinal)
}

// TakeServerFinal consumes the server-final message and verifies the server
// signature. On success Message is empty; the session still sends an empty
// AUTHENTICATE response before the server's 903.
func (c *SASLScram) TakeServerFinal(message []byte) ScramResult {
	if c.step != scramAwaitServerFinal {
		return scramFailure(errNotAwaitingFinal)
	}

	var attributes scramAttributes
	if parseError := parseScramAttributes(message, "v", &attributes); parseError != "" {
		c.reset()
		return scramFailure(parseError)
	}
	if attributes.serverError {
		c.reset()
		return scramFailure(errServerRejected)
	}
	if len(attributes.verifier) == 0 {
		c.reset()
		return scramFailure(errMissingAttributes)
	}

	verifier, decodeError := decodeBase64(attributes.verifier)
	if decodeError != "" {
		c.reset()
		return scramFailure(decodeError)
	}

	signature := hmacSHA256(c.authMessage, c.serverKey)
	matches := hmac.Equal(signature, verifier)
	wipeBytes(signature)
	wipeBytes(verifier)
	c.reset()
	if !matches {
		return scramFailure(errSignatureMismatch)
	}
	return scramSuccess(nil)
}

func (c *SASLScram) reset() {
	c.wipePassword()
	wipeBytes(c.serverKey)
	c.serverKey = nil
	c.authMessage = nil
	c.clientNonce = nil
	c.clientFirstBare = nil
	c.step = scramIdle
}

func (c *SASLScram) wipePassword() {
	wipeBytes(c.password)
	c.password = nil
}

// scramAttributes is the parsed shape of one SCRAM attribute list. Values are
// kept raw; base64 decoding happens where the field is used.
type scramAttributes struct {
	nonce       []byte
	salt        []byte
	iterations  []byte
	verifier    []byte
	serverError bool
}

// parseScramAttributes parses a comma-separated SCRAM attribute list. allowed
// lists the field keys the message may carry; 'e' is always accepted. It
// returns an empty string on success or a failure message.
func parseScramAttributes(message []byte, allowed string, attributes *scramAttributes) string {
	if len(message) == 0 {
		return errMissingAttributes
	}

	start := 0
	for start < len(message) {
		comma := bytes.IndexByte(message[start:], ',')
		end := len(message)
		if comma >= 0 {
			end = start + comma
		}
		part := message[start:end]
		if len(part) < 3 || part[1] != '=' {
			return errMalformedAttribute
		}
		key := part[0]
		value := part[2:]
		if len(value) == 0 {
			return errMalformedAttribute
		}

		if key == 'e' {
			if attributes.serverError {
				return errRepeatedAttribute
			}
			// The server text is discarded. A hostile peer could echo the
			// password there, and error strings must not carry it.
			attributes.serverError = true
		} else if !keyListed(allowed, key) {
			return errMalformedAttribute
		} else {
			var slot *[]byte
			switch key {
			case 'r':
				slot = &attributes.nonce
			case 's':
				slot = &attributes.salt
			case 'i':
				slot = &attributes.iterations
			case 'v':
				slot = &attributes.verifier
			}
			if slot == nil || len(*slot) != 0 {
				return errRepeatedAttribute
			}
			*slot = cloneBytes(value)
		}

		if comma < 0 {
			break
		}
		start = end + 1
		if start >= len(message) {
			return errMalformedAttribute
		}
	}
	return ""
}

func keyListed(allowed string, key byte) bool {
	for index := 0; index < len(allowed); index++ {
		if allowed[index] == key {
			return true
		}
	}
	return false
}

// parseIterationCount parses the 'i' attribute. All bytes must be digits, the
// value must fit an int64, and it must fall inside [minIterations,
// maxIterations].
func parseIterationCount(text []byte) (int, string) {
	if len(text) == 0 {
		return 0, errMalformedAttribute
	}
	for _, digit := range text {
		if digit < '0' || digit > '9' {
			return 0, errMalformedAttribute
		}
	}
	parsed, err := strconv.ParseInt(string(text), 10, 64)
	if err != nil {
		return 0, errMalformedAttribute
	}
	if parsed < minIterations || parsed > maxIterations {
		return 0, errIterationsRange
	}
	return int(parsed), ""
}

// decodeBase64 decodes canonical RFC 5802 Base64: every byte must belong to the
// alphabet (Qt aborts on illegal characters, and Go's decoder would otherwise
// skip newlines), padding must be well formed, and the result must be non-empty.
func decodeBase64(text []byte) ([]byte, string) {
	if len(text) == 0 {
		return nil, errMalformedAttribute
	}
	for _, encoded := range text {
		if !isBase64Byte(encoded) {
			return nil, errMalformedAttribute
		}
	}
	decoded, err := base64.StdEncoding.Strict().DecodeString(string(text))
	if err != nil || len(decoded) == 0 {
		return nil, errMalformedAttribute
	}
	return decoded, ""
}

func isBase64Byte(value byte) bool {
	switch {
	case value >= 'A' && value <= 'Z':
		return true
	case value >= 'a' && value <= 'z':
		return true
	case value >= '0' && value <= '9':
		return true
	case value == '+', value == '/', value == '=':
		return true
	}
	return false
}

// containsUnsendable reports whether a UTF-8 string carries NUL, CR, or LF.
// Those cannot ride an IRC line, so the exchange is refused before any
// credential is used. Ported from ircsaslscram.cpp.
func containsUnsendable(text string) bool {
	for index := 0; index < len(text); index++ {
		switch text[index] {
		case 0, '\r', '\n':
			return true
		}
	}
	return false
}

// acceptableNonce reports whether a client nonce is made of printable ASCII
// without the SCRAM attribute separator.
func acceptableNonce(nonce []byte) bool {
	if len(nonce) == 0 {
		return false
	}
	for _, value := range nonce {
		if value < 0x21 || value > 0x7E || value == ',' {
			return false
		}
	}
	return true
}

// generateNonce returns 16 random bytes base64-encoded. A failure returns nil,
// which acceptableNonce then rejects.
func generateNonce() []byte {
	raw := make([]byte, 16)
	if _, err := rand.Read(raw); err != nil {
		return nil
	}
	return []byte(base64.StdEncoding.EncodeToString(raw))
}

// escapeSaslName escapes '=' and ',' in the SASL account as RFC 5802 requires.
// SASLprep is out of scope because the account is the existing IRC login token.
func escapeSaslName(name []byte) []byte {
	escaped := make([]byte, 0, len(name))
	for _, value := range name {
		switch value {
		case '=':
			escaped = append(escaped, "=3D"...)
		case ',':
			escaped = append(escaped, "=2C"...)
		default:
			escaped = append(escaped, value)
		}
	}
	return escaped
}

func hmacSHA256(message, key []byte) []byte {
	mac := hmac.New(sha256.New, key)
	mac.Write(message)
	return mac.Sum(nil)
}

func xorBytes(left, right []byte) []byte {
	if len(left) != len(right) {
		return nil
	}
	out := make([]byte, len(left))
	for index := range left {
		out[index] = left[index] ^ right[index]
	}
	return out
}

func wipeBytes(bytes []byte) {
	for index := range bytes {
		bytes[index] = 0
	}
}

func scramFailure(message string) ScramResult {
	return ScramResult{Error: message}
}

func scramSuccess(message []byte) ScramResult {
	return ScramResult{Message: message}
}

// The AUTHENTICATE framing constants come from IrcSession in
// src/irc/ircsession.cpp. The C++ framing is not a standalone helper there: it
// lives in IrcSession::sendSaslResponse and IrcSession::takeSaslChunk beside the
// session's own state. The two helpers below are therefore unexported and are
// only a private mirror for the session wave to adopt; they are not part of the
// cross-package contract yet.
const (
	saslAuthenticateChunk    = 400
	saslIncomingEncodedLimit = 4096
)

// encodeSASLFrames wraps raw SCRAM bytes for AUTHENTICATE: standard Base64 in
// 400-byte chunks, with the mandatory trailing "+" when the payload is a
// nonzero multiple of the chunk size, and a lone "+" for an empty payload.
// Frames include their CRLF. Mirrors IrcSession::sendSaslResponse.
func encodeSASLFrames(raw []byte) [][]byte {
	encoded := base64.StdEncoding.EncodeToString(raw)
	if len(encoded) == 0 {
		return [][]byte{[]byte("AUTHENTICATE +\r\n")}
	}

	var frames [][]byte
	for offset := 0; offset < len(encoded); offset += saslAuthenticateChunk {
		end := offset + saslAuthenticateChunk
		if end > len(encoded) {
			end = len(encoded)
		}
		frames = append(frames, []byte("AUTHENTICATE "+encoded[offset:end]+"\r\n"))
	}
	if len(encoded)%saslAuthenticateChunk == 0 {
		frames = append(frames, []byte("AUTHENTICATE +\r\n"))
	}
	return frames
}

// saslChallengeBuffer reassembles inbound AUTHENTICATE chunks and Base64-decodes
// the complete message. Mirrors the m_saslIncoming half of
// IrcSession::takeSaslChunk; the session owns the failure handling.
type saslChallengeBuffer struct {
	accumulated []byte
}

// Take consumes one AUTHENTICATE payload. complete is false while more chunks
// are expected; ok is false when the stream is malformed and the session must
// fail the exchange.
func (b *saslChallengeBuffer) Take(payload []byte) (message []byte, complete, ok bool) {
	if len(payload) == 0 ||
		(len(b.accumulated) > 0 && len(b.accumulated)%saslAuthenticateChunk != 0) {
		b.accumulated = nil
		return nil, false, false
	}

	if !bytes.Equal(payload, []byte("+")) {
		if len(payload) > saslIncomingEncodedLimit ||
			len(b.accumulated) > saslIncomingEncodedLimit-len(payload) {
			b.accumulated = nil
			return nil, false, false
		}
		b.accumulated = append(b.accumulated, payload...)
		if len(payload) == saslAuthenticateChunk {
			return nil, false, true
		}
	}

	encoded := b.accumulated
	b.accumulated = nil
	if len(encoded) == 0 {
		return nil, true, true
	}
	decoded, decodeError := decodeBase64(encoded)
	if decodeError != "" {
		return nil, false, false
	}
	return decoded, true, true
}
