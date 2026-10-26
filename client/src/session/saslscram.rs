//! SCRAM-SHA-256 (RFC 5802, RFC 7677) as the session's SASL mechanism.
//! `SaslScram` is the client state machine. `start` returns client-first
//! `take_server_first` returns client-final with the proof, and `take_server_final`
//! verifies the server signature. Those are raw SCRAM bytes. `encode_sasl_frames` applies
//! the SASL base64 framing, `ChallengeBuffer` reassembles a challenge split over
//! AUTHENTICATE chunks, and `ScramExchange` ties them to the AUTHENTICATE dialogue that
//! the session drives (`saslScramStep` in Go).
//! Nothing here logs or reads the clock. A failure is one of the fixed `ScramFailure`
//! texts, which never carry the password or the server's own `e=` text. Key material is
//! zeroed once it is no longer needed.

mod attrs;
#[cfg(test)]
mod tests;

use std::fmt;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use crate::util::Secret;
use attrs::{decode_base64, parse, parse_iterations};

/// The longest AUTHENTICATE payload per frame (SASL 1.0 section 4.3).
const SASL_AUTHENTICATE_CHUNK: usize = 400;

/// The largest encoded challenge the session accepts (`saslIncomingEncodedLimit`).
const INCOMING_ENCODED_LIMIT: usize = 4096;

/// SHA-256 output length: the size of the salted password and of every derived key.
const KEY_LEN: usize = 32;

/// Why a SCRAM step failed. The texts are the fixed ones from `tui/internal/session/saslscram.go`, so the
/// mirrored test matrix can match them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScramFailure {
    AccountUnsendable,
    ClientNonce,
    NotAwaitingFirst,
    NotAwaitingFinal,
    MissingAttributes,
    MalformedAttribute,
    RepeatedAttribute,
    ServerRejected,
    NonceNotContinued,
    IterationsRange,
    SignatureMismatch,
    MalformedChallenge,
}

impl ScramFailure {
    fn text(self) -> &'static str {
        match self {
            Self::AccountUnsendable => "account or password cannot be sent",
            Self::ClientNonce => "client nonce cannot be sent",
            Self::NotAwaitingFirst => "SCRAM exchange is not waiting for the server first message",
            Self::NotAwaitingFinal => "SCRAM exchange is not waiting for the server final message",
            Self::MissingAttributes => "server message is missing SCRAM attributes",
            Self::MalformedAttribute => "server message has a malformed SCRAM attribute",
            Self::RepeatedAttribute => "server message repeats a SCRAM attribute",
            Self::ServerRejected => "the server rejected authentication",
            Self::NonceNotContinued => "server nonce does not continue the client nonce",
            Self::IterationsRange => "iteration count is outside the allowed range",
            Self::SignatureMismatch => "server signature does not match",
            Self::MalformedChallenge => "SASL challenge is malformed",
        }
    }
}

impl fmt::Display for ScramFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.text())
    }
}

/// One SCRAM step's outcome. `Ok` carries the raw SCRAM bytes for SASL framing, and `Err`
/// says why the exchange must fail. This is Go's `ScramResult`, as a `Result` so `?` can
/// short-circuit.
pub(crate) type ScramResult = Result<Vec<u8>, ScramFailure>;

/// Where the machine stands. `Idle` is the state before `start`, and after any failure or
/// after the server signature has been checked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Step {
    #[default]
    Idle,
    AwaitServerFirst,
    AwaitServerFinal,
}

/// A stateful SCRAM-SHA-256 client (RFC 5802, RFC 7677). It mirrors `SASLScram` in
/// `tui/internal/session/saslscram.go`. Messages are raw SCRAM bytes; `encode_sasl_frames`
/// in this file applies the SASL framing.
#[derive(Default)]
pub(crate) struct SaslScram {
    step: Step,
    password: Secret,
    client_nonce: Vec<u8>,
    client_first_bare: Vec<u8>,
    auth_message: Vec<u8>,
    server_key: Vec<u8>,
}

impl SaslScram {
    /// Begins the exchange and returns client-first, `n,,n=<account>,r=<nonce>`. `None`
    /// draws the nonce from the OS. A test passes a fixed nonce to match a known vector.
    pub(crate) fn start(
        &mut self,
        account: &str,
        password: &Secret,
        client_nonce: Option<&[u8]>,
    ) -> ScramResult {
        self.reset();
        if account.is_empty()
            || contains_unsendable(account)
            || contains_unsendable(password.expose())
        {
            return Err(ScramFailure::AccountUnsendable);
        }
        let nonce = client_nonce.map_or_else(generate_nonce, <[u8]>::to_vec);
        if !acceptable_nonce(&nonce) {
            return Err(ScramFailure::ClientNonce);
        }

        let mut client_first_bare = b"n=".to_vec();
        client_first_bare.extend(escape_sasl_name(account));
        client_first_bare.extend_from_slice(b",r=");
        client_first_bare.extend_from_slice(&nonce);
        let mut message = b"n,,".to_vec();
        message.extend_from_slice(&client_first_bare);

        self.client_nonce = nonce;
        self.client_first_bare = client_first_bare;
        self.password = password.clone();
        self.step = Step::AwaitServerFirst;
        Ok(message)
    }

    /// Consumes the server-first message and returns client-final, which carries the proof.
    pub(crate) fn take_server_first(&mut self, message: &[u8]) -> ScramResult {
        if self.step != Step::AwaitServerFirst {
            return Err(ScramFailure::NotAwaitingFirst);
        }
        let result = self.client_final_for(message);
        if result.is_err() {
            self.reset();
        }
        result
    }

    /// Consumes the server-final message and verifies the server signature. On success the
    /// message is empty. The session still sends an empty AUTHENTICATE response before 903.
    pub(crate) fn take_server_final(&mut self, message: &[u8]) -> ScramResult {
        if self.step != Step::AwaitServerFinal {
            return Err(ScramFailure::NotAwaitingFinal);
        }
        let verified = self.verify_server_final(message);
        self.reset();
        verified.map(|()| Vec::new())
    }

    /// The derivation behind `take_server_first`: parse, check the nonce, run PBKDF2, and
    /// build the proof.
    fn client_final_for(&mut self, message: &[u8]) -> ScramResult {
        let attributes = parse(message, b"rsi")?;
        if attributes.server_error {
            return Err(ScramFailure::ServerRejected);
        }
        let (Some(nonce), Some(salt), Some(iterations)) =
            (attributes.nonce, attributes.salt, attributes.iterations)
        else {
            return Err(ScramFailure::MissingAttributes);
        };
        // RFC 5802: the server appends its own part to the client nonce. A nonce equal to
        // the client's carries none, so it is refused as well as a nonce that is not a prefix.
        if nonce.len() <= self.client_nonce.len() || !nonce.starts_with(&self.client_nonce) {
            return Err(ScramFailure::NonceNotContinued);
        }
        let iterations = parse_iterations(iterations)?;
        let salt = decode_base64(salt)?;

        let mut salted = [0u8; KEY_LEN];
        pbkdf2::pbkdf2_hmac::<Sha256>(
            self.password.expose().as_bytes(),
            &salt,
            iterations,
            &mut salted,
        );
        self.password.wipe();
        let mut client_key = hmac_sha256(&salted, b"Client Key");
        let mut stored_key: [u8; KEY_LEN] = Sha256::digest(client_key).into();
        self.server_key = hmac_sha256(&salted, b"Server Key").to_vec();
        wipe(&mut salted);

        // The GS2 header `n,,` in base64 is `biws`.
        let mut without_proof = b"c=biws,r=".to_vec();
        without_proof.extend_from_slice(nonce);
        let mut auth_message = self.client_first_bare.clone();
        auth_message.push(b',');
        auth_message.extend_from_slice(message);
        auth_message.push(b',');
        auth_message.extend_from_slice(&without_proof);

        let mut client_signature = hmac_sha256(&stored_key, &auth_message);
        let mut proof: [u8; KEY_LEN] =
            std::array::from_fn(|index| client_key[index] ^ client_signature[index]);
        wipe(&mut client_key);
        wipe(&mut stored_key);
        wipe(&mut client_signature);

        let mut client_final = without_proof;
        client_final.extend_from_slice(b",p=");
        client_final.extend_from_slice(STANDARD.encode(proof).as_bytes());
        wipe(&mut proof);
        self.auth_message = auth_message;
        self.step = Step::AwaitServerFinal;
        Ok(client_final)
    }

    /// The check behind `take_server_final`. The state is reset by the caller either way.
    fn verify_server_final(&self, message: &[u8]) -> Result<(), ScramFailure> {
        let attributes = parse(message, b"v")?;
        if attributes.server_error {
            return Err(ScramFailure::ServerRejected);
        }
        let verifier = decode_base64(attributes.verifier.ok_or(ScramFailure::MissingAttributes)?)?;
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&self.server_key)
            .expect("HMAC takes a key of any length");
        mac.update(&self.auth_message);
        // `verify_slice` compares in constant time, as Go's `hmac.Equal` does.
        mac.verify_slice(&verifier)
            .map_err(|_| ScramFailure::SignatureMismatch)
    }

    /// Forgets the exchange: wipes the password and the server key, and returns to Idle.
    fn reset(&mut self) {
        self.password.wipe();
        wipe(&mut self.server_key);
        self.server_key.clear();
        self.client_nonce.clear();
        self.client_first_bare.clear();
        self.auth_message.clear();
        self.step = Step::Idle;
    }
}

impl fmt::Debug for SaslScram {
    // The password and the keys stay out of Debug.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SaslScram")
            .field("step", &self.step)
            .finish_non_exhaustive()
    }
}

impl Drop for SaslScram {
    fn drop(&mut self) {
        self.reset();
    }
}

/// HMAC-SHA-256 of `message` under `key` (`hmacSHA256` in Go).
fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; KEY_LEN] {
    let mut mac =
        <Hmac<Sha256> as Mac>::new_from_slice(key).expect("HMAC takes a key of any length");
    mac.update(message);
    mac.finalize().into_bytes().into()
}

/// Zeroes a key or a derived secret. `black_box` keeps the stores from being removed as
/// dead writes, since the buffer is never read again.
fn wipe(bytes: &mut [u8]) {
    bytes.fill(0);
    std::hint::black_box(bytes);
}

/// NUL, CR and LF cannot ride an IRC line, so the exchange is refused before any credential
/// is used.
fn contains_unsendable(text: &str) -> bool {
    text.bytes().any(|byte| matches!(byte, 0 | b'\r' | b'\n'))
}

/// A client nonce is printable ASCII without `,`, the SCRAM attribute separator.
fn acceptable_nonce(nonce: &[u8]) -> bool {
    !nonce.is_empty()
        && nonce
            .iter()
            .all(|&byte| (0x21..=0x7E).contains(&byte) && byte != b',')
}

/// 16 OS random bytes, base64-encoded. If the OS refuses, this returns nothing, which
/// `acceptable_nonce` then rejects (Go's `generateNonce`).
fn generate_nonce() -> Vec<u8> {
    let mut raw = [0u8; 16];
    if getrandom::fill(&mut raw).is_err() {
        return Vec::new();
    }
    STANDARD.encode(raw).into_bytes()
}

/// Escapes `=` and `,` in the account name as RFC 5802 requires. SASLprep is out of scope:
/// the account is the existing IRC login token.
fn escape_sasl_name(name: &str) -> Vec<u8> {
    let mut escaped = Vec::with_capacity(name.len());
    for byte in name.bytes() {
        match byte {
            b'=' => escaped.extend_from_slice(b"=3D"),
            b',' => escaped.extend_from_slice(b"=2C"),
            _ => escaped.push(byte),
        }
    }
    escaped
}

/// The AUTHENTICATE frames for `raw`: standard base64 in 400-byte chunks, each frame
/// CRLF-terminated. A payload whose encoding is a nonzero multiple of 400 ends with a bare
/// `+`, and an empty payload is a lone `+` (`encodeSASLFrames`).
pub(crate) fn encode_sasl_frames(raw: &[u8]) -> Vec<Vec<u8>> {
    let encoded = STANDARD.encode(raw);
    if encoded.is_empty() {
        return vec![b"AUTHENTICATE +\r\n".to_vec()];
    }
    let mut frames: Vec<Vec<u8>> = encoded
        .as_bytes()
        .chunks(SASL_AUTHENTICATE_CHUNK)
        .map(|chunk| {
            let mut frame = b"AUTHENTICATE ".to_vec();
            frame.extend_from_slice(chunk);
            frame.extend_from_slice(b"\r\n");
            frame
        })
        .collect();
    if encoded.len() % SASL_AUTHENTICATE_CHUNK == 0 {
        frames.push(b"AUTHENTICATE +\r\n".to_vec());
    }
    frames
}

/// Reassembles one inbound challenge sent over several AUTHENTICATE frames, then decodes it
/// (`m_saslIncoming` in `IrcSession::takeSaslChunk`). A payload of exactly 400 bytes asks
/// for more. A shorter one, or a bare `+` after whole chunks, completes the challenge.
#[derive(Debug, Default)]
pub(crate) struct ChallengeBuffer {
    accumulated: Vec<u8>,
}

impl ChallengeBuffer {
    /// Consumes one AUTHENTICATE payload. `Ok(None)` means more chunks are due, `Ok(Some)`
    /// is the decoded challenge, and `Err` means the stream is malformed.
    pub(crate) fn take(&mut self, payload: &[u8]) -> Result<Option<Vec<u8>>, ScramFailure> {
        let bare_plus = payload == b"+";
        let mid_chunk =
            !self.accumulated.is_empty() && self.accumulated.len() % SASL_AUTHENTICATE_CHUNK != 0;
        let overflows = !bare_plus
            && (payload.len() > INCOMING_ENCODED_LIMIT
                || self.accumulated.len() > INCOMING_ENCODED_LIMIT - payload.len());
        if payload.is_empty() || mid_chunk || overflows {
            self.accumulated.clear();
            return Err(ScramFailure::MalformedChallenge);
        }

        if !bare_plus {
            self.accumulated.extend_from_slice(payload);
            if payload.len() == SASL_AUTHENTICATE_CHUNK {
                return Ok(None);
            }
        }
        let encoded = std::mem::take(&mut self.accumulated);
        if encoded.is_empty() {
            return Ok(Some(Vec::new()));
        }
        STANDARD
            .decode(&encoded)
            .map(Some)
            .map_err(|_| ScramFailure::MalformedChallenge)
    }
}

/// Where the AUTHENTICATE dialogue stands (`saslScramStep` in Go).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Phase {
    /// `AUTHENTICATE SCRAM-SHA-256` is out. The server's `+` prompt is due.
    #[default]
    AwaitPrompt,
    /// Client-first is out. The server-first challenge is due.
    AwaitServerFirst,
    /// Client-final is out. The server-final challenge is due.
    AwaitServerFinal,
    /// The server signature verified, and the empty response is out.
    Verified,
}

/// What the session sends after one AUTHENTICATE payload.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ScramReply {
    /// Raw bytes for the next AUTHENTICATE response: client-first, client-final, or empty.
    Send(Vec<u8>),
    /// Nothing to send yet: a challenge chunk arrived, or the exchange already verified.
    Continue,
    /// The exchange failed. The session fails authentication and never retries with PLAIN.
    Fail,
}

/// One SCRAM exchange as the AUTHENTICATE dialogue drives it: the machine, the phase, and
/// the reassembly of the server's challenges (Go's `saslScram*` fields and
/// `handleScramAuthenticateLocked`).
#[derive(Debug, Default)]
pub(crate) struct ScramExchange {
    phase: Phase,
    scram: SaslScram,
    incoming: ChallengeBuffer,
}

impl ScramExchange {
    /// Whether the server signature has verified.
    pub(crate) fn is_verified(&self) -> bool {
        self.phase == Phase::Verified
    }

    /// Handles one AUTHENTICATE payload from the server, with the credential the session
    /// would send for PLAIN.
    pub(crate) fn take_payload(
        &mut self,
        payload: &[u8],
        account: &str,
        password: &Secret,
    ) -> ScramReply {
        match self.phase {
            Phase::AwaitPrompt => {
                if payload != b"+" {
                    return ScramReply::Fail;
                }
                match self.scram.start(account, password, None) {
                    Ok(message) => {
                        self.phase = Phase::AwaitServerFirst;
                        ScramReply::Send(message)
                    }
                    Err(_) => ScramReply::Fail,
                }
            }
            Phase::Verified => ScramReply::Continue,
            Phase::AwaitServerFirst | Phase::AwaitServerFinal => {
                let message = match self.incoming.take(payload) {
                    Ok(Some(message)) => message,
                    Ok(None) => return ScramReply::Continue,
                    Err(_) => return ScramReply::Fail,
                };
                if self.phase == Phase::AwaitServerFirst {
                    match self.scram.take_server_first(&message) {
                        Ok(client_final) => {
                            self.phase = Phase::AwaitServerFinal;
                            ScramReply::Send(client_final)
                        }
                        Err(_) => ScramReply::Fail,
                    }
                } else {
                    match self.scram.take_server_final(&message) {
                        Ok(_) => {
                            self.phase = Phase::Verified;
                            // SASL 3.1 still wants an empty response before the server's 903.
                            ScramReply::Send(Vec::new())
                        }
                        Err(_) => ScramReply::Fail,
                    }
                }
            }
        }
    }
}
