//! Port of the SCRAM-SHA-256 cases of `tui/internal/session/session_auth_test.go`:
//! `TestSessionNegotiatesSaslScramSha256` and the `TestSessionScram*` /
//! `TestSessionAbandonedScram*` family. The Status-entry checks wait for the Phase 3
//! Status recorder, as in `auth.rs`; the secrecy checks read the session's errors instead.

use std::time::Duration;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use hmac::{Hmac, Mac};
use sha2::Sha256;

use super::fixture::{test_config, Fixture};
use crate::session::events::{ErrorKind, SessionState};
use crate::session::saslscram::{encode_sasl_frames, SaslScram};
use crate::util::Secret;

/// The RFC 7677 vector's password and salt, and the fixed server nonce tail the C++ suite
/// appends to the client nonce (`kRfcServerNonceTail`).
const PASSWORD: &str = "pencil";
const SALT: &str = "W22ZaJ0SNY7soEsUEjb6gQ==";
const SERVER_NONCE_TAIL: &str = "%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0";

/// A session configured for SCRAM: the `user` account and the given server and NickServ
/// secrets. An empty secret is unset.
fn fixture_with(server_password: &str, nickserv_password: &str) -> Fixture {
    let mut config = test_config();
    config.sasl_account = "user".to_owned();
    config.password = server_password.to_owned().into();
    config.nickserv_password = nickserv_password.to_owned().into();
    Fixture::new(config)
}

/// Drives the session into a SCRAM exchange and returns the client-first message
/// (`startScramExchange`).
fn start_scram_exchange(fx: &Fixture) -> String {
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :sasl=SCRAM-SHA-256,PLAIN\r\n\
         :server CAP omairc ACK :sasl\r\n\
         AUTHENTICATE +\r\n",
    );
    decode_payload(&fx.last_frame())
}

/// The raw payload of one AUTHENTICATE frame, decoded from base64 (`decodeAuthenticatePayload`).
fn decode_payload(frame: &str) -> String {
    let bytes = STANDARD
        .decode(authenticate_body(frame))
        .expect("base64 payload");
    String::from_utf8(bytes).expect("a SCRAM message is ASCII")
}

/// The base64 body of an AUTHENTICATE frame, without its framing.
fn authenticate_body(frame: &str) -> &str {
    frame
        .strip_prefix("AUTHENTICATE ")
        .and_then(|rest| rest.strip_suffix("\r\n"))
        .expect("an AUTHENTICATE frame")
}

/// The AUTHENTICATE frames that carry `raw`, as the peer sends them (`saslWire`).
fn sasl_wire(raw: &str) -> String {
    encode_sasl_frames(raw.as_bytes())
        .into_iter()
        .map(|frame| String::from_utf8(frame).expect("ASCII frame"))
        .collect()
}

/// The client-first nonce: the text after `n,,n=user,r=`.
fn nonce_of(client_first: &str) -> &str {
    client_first
        .strip_prefix("n,,n=user,r=")
        .expect("the n,,n=user,r= prefix")
}

/// The server-first message for `nonce`, with the fixed tail and the vector's salt.
fn server_first_for(nonce: &str, iterations: &str) -> String {
    format!("r={nonce}{SERVER_NONCE_TAIL},s={SALT},i={iterations}")
}

/// The RFC 7677 server signature for an exchange, recomputed independently of the machine
/// (`scramServerFinalFor` in `session_test.go`, `scramServerFinal` in `tst_session.cpp`).
fn server_final_for(client_first: &str, server_first: &str) -> String {
    let bare = client_first.strip_prefix("n,,").expect("the n,, prefix");
    let nonce_at = server_first.find("r=").expect("an r= attribute");
    let comma = server_first[nonce_at..]
        .find(',')
        .expect("a comma after r=");
    let server_nonce = &server_first[nonce_at + 2..nonce_at + comma];
    let auth_message = format!("{bare},{server_first},c=biws,r={server_nonce}");

    let salt = STANDARD.decode(SALT).expect("salt");
    let mut salted = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha256>(PASSWORD.as_bytes(), &salt, 4096, &mut salted);
    let server_key = hmac_sha256(&salted, b"Server Key");
    let signature = hmac_sha256(&server_key, auth_message.as_bytes());
    format!("v={}", STANDARD.encode(signature))
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> Vec<u8> {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key).expect("any key length");
    mac.update(message);
    mac.finalize().into_bytes().to_vec()
}

/// Ports `TestSessionNegotiatesSaslScramSha256`.
#[test]
fn negotiates_sasl_scram_sha_256() {
    assert_eq!(
        server_final_for(
            "n,,n=user,r=rOprNGfwEbeRWgbNEkqO",
            "r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096",
        ),
        "v=6rriTRBi23WpRR/wtup+mMhUZUn/dB5nLTJRsjl95G4=",
        "the RFC 7677 vector"
    );

    let fx = fixture_with(PASSWORD, PASSWORD);
    let client_first = start_scram_exchange(&fx);
    assert!(
        fx.wrote("AUTHENTICATE SCRAM-SHA-256\r\n"),
        "SCRAM-SHA-256 must be chosen"
    );
    assert!(
        !fx.wrote("AUTHENTICATE PLAIN\r\n"),
        "PLAIN must not be attempted"
    );
    let nonce = nonce_of(&client_first);
    assert!(!nonce.is_empty() && !nonce.contains(','));

    let server_nonce = format!("{nonce}{SERVER_NONCE_TAIL}");
    let server_first = server_first_for(nonce, "4096");
    let mut reference = SaslScram::default();
    let started = reference
        .start(
            "user",
            &Secret::from(PASSWORD.to_owned()),
            Some(nonce.as_bytes()),
        )
        .expect("reference client-first");
    assert_eq!(started, client_first.as_bytes());
    let client_final = reference
        .take_server_first(server_first.as_bytes())
        .expect("reference client-final");
    let client_final = String::from_utf8(client_final).expect("ASCII");
    assert!(client_final.starts_with(&format!("c=biws,r={server_nonce},p=")));

    let before = fx.frames().len();
    fx.inject(&sasl_wire(&server_first));
    assert_eq!(fx.frames().len(), before + 1, "one client-final frame");
    assert_eq!(
        fx.last_frame(),
        format!("AUTHENTICATE {}\r\n", STANDARD.encode(&client_final))
    );
    assert_eq!(decode_payload(&fx.last_frame()), client_final);

    let server_final = server_final_for(&client_first, &server_first);
    fx.inject(&format!(
        "{}:server 903 omairc :SASL successful\r\n:server 001 omairc :Welcome\r\n",
        sasl_wire(&server_final)
    ));
    assert!(
        fx.wrote("AUTHENTICATE +\r\n"),
        "the empty AUTHENTICATE response is required"
    );
    assert!(fx.wrote("CAP END\r\n"), "CAP END missing");
    assert!(
        !fx.wrote("AUTHENTICATE PLAIN\r\n"),
        "PLAIN must not be attempted"
    );
    assert!(
        !fx.wrote("PRIVMSG NickServ :IDENTIFY pencil\r\n"),
        "a successful SCRAM must not IDENTIFY"
    );
    assert!(fx.wrote("JOIN #omarchy\r\n"), "autojoin missing");
    assert_eq!(fx.session.state(), SessionState::Registered);
    assert!(fx.errors().is_empty(), "errors: {:?}", fx.error_messages());
}

/// Ports `TestSessionScramNumericBeforeServerFinalFails`. A 903 before the server signature
/// verifies is a failure, and nothing registers.
#[test]
fn scram_numeric_before_server_final_fails() {
    let fx = fixture_with(PASSWORD, PASSWORD);
    let client_first = start_scram_exchange(&fx);
    let server_first = server_first_for(nonce_of(&client_first), "4096");
    fx.inject(&sasl_wire(&server_first));
    assert_eq!(fx.session.state(), SessionState::Sasl);
    assert!(fx.errors().is_empty(), "errors: {:?}", fx.error_messages());
    let last = fx.last_frame();
    assert!(
        last.starts_with("AUTHENTICATE ") && last != "AUTHENTICATE SCRAM-SHA-256\r\n",
        "last frame = {last:?}, want the client-final"
    );

    fx.inject(":server 903 omairc :SASL successful\r\n:server 001 omairc :Welcome\r\n");
    assert_failed_authentication(&fx, PASSWORD);
    assert!(!fx.wrote("CAP END\r\n"));
    assert!(!fx.wrote("AUTHENTICATE PLAIN\r\n"));
    assert!(!fx.wrote("PRIVMSG NickServ :IDENTIFY pencil\r\n"));
    assert!(
        !fx.wrote("JOIN #omarchy\r\n"),
        "a failed SCRAM must not register"
    );
}

/// Ports `TestSessionScramFailureDoesNotFallThroughToPlain`.
#[test]
fn scram_failure_does_not_fall_through_to_plain() {
    let aborts = [
        ":server 904 omairc :SASL failed\r\n:server 001 omairc :Welcome\r\n",
        ":server 905 omairc :SASL message too long\r\n:server 001 omairc :Welcome\r\n",
        "AUTHENTICATE *\r\n:server 001 omairc :Welcome\r\n",
    ];
    for abort in aborts {
        let fx = fixture_with("", PASSWORD);
        start_scram_exchange(&fx);
        assert!(
            fx.wrote("AUTHENTICATE SCRAM-SHA-256\r\n"),
            "SCRAM-SHA-256 must be chosen"
        );

        fx.inject(abort);
        assert_failed_authentication(&fx, PASSWORD);
        assert!(!fx.wrote("AUTHENTICATE PLAIN\r\n"), "abort {abort:?}");
        assert!(!fx.wrote("CAP END\r\n"), "abort {abort:?}");
        assert!(
            !fx.wrote("PRIVMSG NickServ :IDENTIFY pencil\r\n"),
            "abort {abort:?}"
        );
        assert!(!fx.wrote("JOIN #omarchy\r\n"), "abort {abort:?}");
    }
}

/// Ports `TestSessionScramPlainReadvertisementDoesNotFinishExchange`. A re-advertised PLAIN
/// mechanism does not retarget an exchange that already started.
#[test]
fn scram_plain_readvertisement_does_not_finish_exchange() {
    let fx = fixture_with(PASSWORD, PASSWORD);
    start_scram_exchange(&fx);
    assert!(
        fx.wrote("AUTHENTICATE SCRAM-SHA-256\r\n"),
        "SCRAM-SHA-256 must be chosen"
    );
    let before = fx.frames().len();

    fx.inject(
        ":server CAP omairc DEL :sasl\r\n\
         :server CAP omairc NEW :sasl=PLAIN\r\n\
         :server CAP omairc ACK :sasl\r\n\
         :server 903 omairc :SASL successful\r\n\
         :server 001 omairc :Welcome\r\n",
    );
    assert!(
        fx.written_since(before)
            .iter()
            .all(|frame| !frame.starts_with("AUTHENTICATE ")),
        "no AUTHENTICATE frame may follow the re-advertisement"
    );
    assert!(
        !fx.wrote("AUTHENTICATE PLAIN\r\n"),
        "PLAIN must not be attempted"
    );
    assert_failed_authentication(&fx, PASSWORD);
    assert!(!fx.wrote("CAP END\r\n"));
    assert!(!fx.wrote("PRIVMSG NickServ :IDENTIFY pencil\r\n"));
    assert!(
        !fx.wrote("JOIN #omarchy\r\n"),
        "a failed SCRAM must not register"
    );
}

/// Ports `TestSessionScramServerFirstIllegalBase64Fails`. A byte outside the base64 alphabet
/// in the server-first challenge fails the exchange before anything is written.
#[test]
fn scram_server_first_illegal_base64_fails() {
    let fx = fixture_with(PASSWORD, "");
    let client_first = start_scram_exchange(&fx);
    let server_first = server_first_for(nonce_of(&client_first), "4096");
    let wire = sasl_wire(&server_first);
    let wire = format!("{}!\r\n", wire.strip_suffix("\r\n").expect("CRLF"));

    let before = fx.frames().len();
    fx.inject(&wire);
    assert_eq!(fx.frames().len(), before, "no frame may be written");
    assert_failed_authentication(&fx, PASSWORD);
    assert!(!fx.wrote("AUTHENTICATE PLAIN\r\n"));
    assert!(!fx.wrote("CAP END\r\n"), "a failed SCRAM must not register");
}

/// Ports `TestSessionScramSaslReassemblesChunkedMessages`. A client-final longer than one
/// chunk is sent as full 400-byte frames and then a bare `+`.
#[test]
fn scram_sasl_reassembles_chunked_messages() {
    let fx = fixture_with(PASSWORD, "");
    let client_first = start_scram_exchange(&fx);
    let nonce = nonce_of(&client_first).to_owned();

    // Grow the server nonce until the client-final is a multiple of 400 encoded bytes and
    // longer than one chunk, as the Go test does.
    let mut target = 600;
    let base = 56 + nonce.len();
    while target <= base {
        target += 300;
    }
    let server_nonce = format!("{nonce}{}", "A".repeat(target - base));
    let server_first = format!("r={server_nonce},s={SALT},i=4096");
    assert!(
        STANDARD.encode(&server_first).len() > 400,
        "server-first must exceed one chunk"
    );

    let mut reference = SaslScram::default();
    reference
        .start(
            "user",
            &Secret::from(PASSWORD.to_owned()),
            Some(nonce.as_bytes()),
        )
        .expect("reference start");
    let client_final = reference
        .take_server_first(server_first.as_bytes())
        .expect("reference client-final");
    let encoded_final = STANDARD.encode(&client_final);
    assert!(
        encoded_final.len() % 400 == 0 && encoded_final.len() >= 800,
        "encoded final length = {}",
        encoded_final.len()
    );
    let chunks = encoded_final.len() / 400;

    let before = fx.frames().len();
    fx.inject(&sasl_wire(&server_first));
    let frames = fx.frames();
    assert_eq!(
        frames.len(),
        before + chunks + 1,
        "full chunks, then the bare +"
    );
    let reassembled: String = (0..chunks)
        .map(|index| {
            let body = authenticate_body(&frames[before + index]);
            assert_eq!(body.len(), 400, "chunk {index}");
            body
        })
        .collect();
    assert_eq!(reassembled, encoded_final);
    assert_eq!(frames[before + chunks], "AUTHENTICATE +\r\n");

    let server_final = server_final_for(&client_first, &server_first);
    fx.inject(&format!(
        "{}:server 903 omairc :SASL successful\r\n",
        sasl_wire(&server_final)
    ));
    assert!(fx.wrote("CAP END\r\n"), "CAP END missing");
    assert!(
        !fx.wrote("AUTHENTICATE PLAIN\r\n"),
        "PLAIN must not be attempted"
    );
    assert_eq!(fx.session.state(), SessionState::Registering);
}

/// Ports `TestSessionScramWelcomeBeforeVerifierFails`. A 001 while the exchange is still
/// open fails authentication, and a later 903 does not revive it.
#[test]
fn scram_welcome_before_verifier_fails() {
    let fx = fixture_with(PASSWORD, PASSWORD);
    let client_first = start_scram_exchange(&fx);
    assert!(
        client_first.starts_with("n,,n=user,r="),
        "client-first = {client_first:?}"
    );
    assert_eq!(fx.session.state(), SessionState::Sasl);

    fx.inject(":server 001 omairc :Welcome\r\n:server 903 omairc :SASL successful\r\n");
    assert_failed_authentication(&fx, PASSWORD);
    assert!(!fx.wrote("CAP END\r\n"));
    assert!(!fx.wrote("AUTHENTICATE PLAIN\r\n"));
    assert!(!fx.wrote("PRIVMSG NickServ :IDENTIFY pencil\r\n"));
    assert!(!fx.wrote("JOIN #omarchy\r\n"));
    assert!(!fx.wrote("JOIN &local\r\n"));
}

/// Ports `TestSessionAbandonedScramBeforeAuthenticateRegisters`. A SCRAM exchange that never
/// reached AUTHENTICATE times out with CAP END, registers, and does not IDENTIFY.
#[test]
fn abandoned_scram_before_authenticate_registers() {
    let fx = fixture_with("server-secret", "nick-secret");
    fx.connect_tls();
    fx.inject(":server CAP omairc LS :sasl=SCRAM-SHA-256\r\n");
    assert!(fx.wrote("CAP REQ :sasl\r\n"), "sasl must be requested");
    assert!(
        fx.clock.pending() > 0,
        "the capability timer must be pending"
    );
    assert!(
        !fx.wrote("AUTHENTICATE SCRAM-SHA-256\r\n"),
        "AUTHENTICATE must wait for the ACK"
    );
    assert!(!fx.wrote("CAP END\r\n"), "CAP END must wait for SASL");
    assert_eq!(fx.session.state(), SessionState::Registering);

    fx.clock.advance(Duration::from_millis(10_000));
    assert!(
        fx.wrote("CAP END\r\n"),
        "CAP END must follow the capability timeout"
    );
    assert!(!fx.wrote("AUTHENTICATE SCRAM-SHA-256\r\n"));
    assert!(!fx.wrote("AUTHENTICATE PLAIN\r\n"));
    assert_eq!(fx.session.state(), SessionState::Registering);
    assert!(fx.errors().is_empty(), "errors: {:?}", fx.error_messages());

    fx.inject(":server 001 omairc :Welcome\r\n");
    assert!(fx.errors().is_empty(), "errors: {:?}", fx.error_messages());
    assert_eq!(fx.session.state(), SessionState::Registered);
    assert!(
        !fx.wrote("PRIVMSG NickServ :IDENTIFY nick-secret\r\n"),
        "an abandoned SCRAM must not IDENTIFY"
    );
    assert!(fx.wrote("JOIN #omarchy\r\n"), "autojoin missing");
}

/// Ports `TestSessionScramSaslChunkLimitFails`. An inbound challenge beyond the 4096-byte
/// limit fails the exchange, and nothing is written.
#[test]
fn scram_sasl_chunk_limit_fails() {
    let fx = fixture_with(PASSWORD, "");
    start_scram_exchange(&fx);
    assert_eq!(fx.session.state(), SessionState::Sasl);
    let before = fx.frames().len();

    let mut wire = String::new();
    let mut sent = 0;
    while sent <= 4096 {
        wire.push_str("AUTHENTICATE ");
        wire.push_str(&"A".repeat(400));
        wire.push_str("\r\n");
        sent += 400;
    }
    fx.inject(&wire);
    assert_failed_authentication(&fx, PASSWORD);
    assert_eq!(fx.frames().len(), before, "no frame may be written");
    assert!(!fx.wrote("AUTHENTICATE PLAIN\r\n"));
    assert!(!fx.wrote("CAP END\r\n"), "a failed SCRAM must not register");
}

/// The shared failure check: exactly one Authentication error, which does not carry the
/// password, and the session is Failed.
#[track_caller]
fn assert_failed_authentication(fx: &Fixture, password: &str) {
    let errors = fx.errors();
    assert_eq!(errors.len(), 1, "errors: {errors:?}");
    let (kind, message) = &errors[0];
    assert_eq!(*kind, ErrorKind::Authentication);
    assert!(
        !message.contains(password),
        "the password leaked into the error"
    );
    assert_eq!(fx.session.state(), SessionState::Failed);
}
