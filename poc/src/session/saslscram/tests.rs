//! Port of `tui/internal/session/saslscram_test.go`, which in turn mirrors `tst_scram.cpp`.
//! The RFC 7677 vector is the one both suites pin.

use super::*;

/// The fixed nonce and messages of the RFC 7677 vector (`tst_scram.cpp`).
const NONCE: &str = "rOprNGfwEbeRWgbNEkqO";
const CLIENT_FIRST: &str = "n,,n=user,r=rOprNGfwEbeRWgbNEkqO";
const SERVER_FIRST: &str =
    "r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096";
const CLIENT_FINAL: &str = "c=biws,r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,p=dHzbZapWIk4jUhN+Ute9ytag9zjfMHgsqmmiz7AndVQ=";
const SERVER_FINAL: &str = "v=6rriTRBi23WpRR/wtup+mMhUZUn/dB5nLTJRsjl95G4=";
const SALT: &str = "W22ZaJ0SNY7soEsUEjb6gQ==";

fn pencil() -> Secret {
    Secret::from("pencil".to_owned())
}

/// Whether neither the failure text nor the message carries the test password.
fn hides_password(result: &ScramResult) -> bool {
    let text = match result {
        Ok(message) => String::from_utf8_lossy(message).into_owned(),
        Err(failure) => failure.to_string(),
    };
    !text.contains("pencil")
}

/// A server-first message with a custom iteration count, over the vector's nonce and salt.
fn server_first_with_iterations(iterations: &str) -> Vec<u8> {
    format!("r={NONCE}%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,s={SALT},i={iterations}").into_bytes()
}

/// Ports `ScramTest::rfc7677VectorVerifiesServerSignature`.
#[test]
fn rfc7677_vector_verifies_server_signature() {
    let mut scram = SaslScram::default();
    let started = scram.start("user", &pencil(), Some(NONCE.as_bytes()));
    assert_eq!(started.as_deref(), Ok(CLIENT_FIRST.as_bytes()));
    assert!(hides_password(&started));

    let client_final = scram.take_server_first(SERVER_FIRST.as_bytes());
    assert_eq!(client_final.as_deref(), Ok(CLIENT_FINAL.as_bytes()));
    assert!(hides_password(&client_final));

    let server_final = scram.take_server_final(SERVER_FINAL.as_bytes());
    assert_eq!(server_final, Ok(Vec::new()));
}

/// Ports `ScramTest::wrongServerSignatureFails`.
#[test]
fn wrong_server_signature_fails() {
    let mut scram = SaslScram::default();
    scram
        .start("user", &pencil(), Some(NONCE.as_bytes()))
        .expect("start");
    scram
        .take_server_first(SERVER_FIRST.as_bytes())
        .expect("client-final");
    let rejected = scram.take_server_final(b"v=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=");
    assert_eq!(rejected, Err(ScramFailure::SignatureMismatch));
}

/// Ports `ScramTest::serverNonceMustContinueClientNonce`.
#[test]
fn server_nonce_must_continue_client_nonce() {
    let mut scram = SaslScram::default();
    scram
        .start("user", &pencil(), Some(NONCE.as_bytes()))
        .expect("start");
    let server_first =
        format!("r=XXXXXXXXXXXXXXXXXXXX%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,s={SALT},i=4096");
    assert_eq!(
        scram.take_server_first(server_first.as_bytes()),
        Err(ScramFailure::NonceNotContinued)
    );
}

/// Ports `ScramTest::nonceEqualsSignsBelongToTheValue`.
#[test]
fn nonce_equals_signs_belong_to_the_value() {
    let mut scram = SaslScram::default();
    scram.start("user", &pencil(), Some(b"abc")).expect("start");
    let client_final = scram
        .take_server_first(format!("r=abc=def=ghi,s={SALT},i=4096").as_bytes())
        .expect("client-final");
    assert!(client_final.starts_with(b"c=biws,r=abc=def=ghi,p="));
}

/// Ports `ScramTest::iterationCountBelowMinimumFails`.
#[test]
fn iteration_count_below_minimum_fails() {
    let mut scram = SaslScram::default();
    scram
        .start("user", &pencil(), Some(NONCE.as_bytes()))
        .expect("start");
    assert_eq!(
        scram.take_server_first(&server_first_with_iterations("4095")),
        Err(ScramFailure::IterationsRange)
    );
}

/// Ports `ScramTest::iterationCountAboveMaximumFailsWithoutPbkdf2`. The Go test times the
/// step, but `session/` does not read the clock outside `clock.rs`. The ordering is checked
/// structurally instead: `client_final_for` calls `parse_iterations` before the PBKDF2 call.
#[test]
fn iteration_count_above_maximum_fails_without_pbkdf2() {
    let mut scram = SaslScram::default();
    scram
        .start("user", &pencil(), Some(NONCE.as_bytes()))
        .expect("start");
    let rejected = scram.take_server_first(&server_first_with_iterations("1000001"));
    assert_eq!(rejected, Err(ScramFailure::IterationsRange));
    assert!(hides_password(&rejected));
}

/// Ports `ScramTest::usernameEscapesEqualsAndComma`.
#[test]
fn username_escapes_equals_and_comma() {
    let mut scram = SaslScram::default();
    let started = scram.start("a=b,c", &pencil(), Some(b"nonce"));
    assert_eq!(started.as_deref(), Ok(&b"n,,n=a=3Db=2Cc,r=nonce"[..]));
    assert!(hides_password(&started));
}

/// Ports `ScramTest::accountOrPasswordWithControlBytesIsRejected`.
#[test]
fn account_or_password_with_control_bytes_is_rejected() {
    for account in ["user\rname", "user\nname", "\0", "us\0er"] {
        let mut scram = SaslScram::default();
        let rejected = scram.start(account, &pencil(), None);
        assert_eq!(
            rejected,
            Err(ScramFailure::AccountUnsendable),
            "account {account:?}"
        );
        assert!(hides_password(&rejected));
        assert_eq!(
            scram.take_server_first(SERVER_FIRST.as_bytes()),
            Err(ScramFailure::NotAwaitingFirst),
            "a refused start leaves nothing to continue"
        );
    }

    for password in ["pencil\r", "pencil\n", "pencil\0"] {
        let mut scram = SaslScram::default();
        let rejected = scram.start("user", &Secret::from(password.to_owned()), None);
        assert_eq!(
            rejected,
            Err(ScramFailure::AccountUnsendable),
            "password {password:?}"
        );
        assert!(hides_password(&rejected));
    }
}

/// Ports `ScramTest::serverErrorAttributeFails`.
#[test]
fn server_error_attribute_fails() {
    let mut scram = SaslScram::default();
    scram
        .start("user", &pencil(), Some(NONCE.as_bytes()))
        .expect("start");
    let rejected = scram.take_server_first(b"e=pencil");
    assert_eq!(rejected, Err(ScramFailure::ServerRejected));
    assert!(hides_password(&rejected));

    let mut again = SaslScram::default();
    again
        .start("user", &pencil(), Some(NONCE.as_bytes()))
        .expect("start");
    again
        .take_server_first(SERVER_FIRST.as_bytes())
        .expect("client-final");
    let rejected = again.take_server_final(b"e=pencil");
    assert_eq!(rejected, Err(ScramFailure::ServerRejected));
    assert!(hides_password(&rejected));
}

/// Ports `ScramTest::missingOrGarbageAttributeFails`.
#[test]
fn missing_or_garbage_attribute_fails() {
    let mut missing = SaslScram::default();
    missing
        .start("user", &pencil(), Some(NONCE.as_bytes()))
        .expect("start");
    let no_salt = missing
        .take_server_first(format!("r={NONCE}%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,i=4096").as_bytes());
    assert_eq!(no_salt, Err(ScramFailure::MissingAttributes));

    let mut garbage = SaslScram::default();
    garbage
        .start("user", &pencil(), Some(NONCE.as_bytes()))
        .expect("start");
    let extra = garbage.take_server_first(format!("{SERVER_FIRST},m=reserved").as_bytes());
    assert_eq!(extra, Err(ScramFailure::MalformedAttribute));
    assert!(hides_password(&extra));
}

/// Ports `ScramTest::verifierOrSaltWithTrailingJunkFails`.
#[test]
fn verifier_or_salt_with_trailing_junk_fails() {
    let mut salt = SaslScram::default();
    salt.start("user", &pencil(), Some(NONCE.as_bytes()))
        .expect("start");
    let salted = salt.take_server_first(
        format!("r={NONCE}%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,s={SALT}!,i=4096").as_bytes(),
    );
    assert!(salted.is_err());
    assert!(hides_password(&salted));

    let mut verifier = SaslScram::default();
    verifier
        .start("user", &pencil(), Some(NONCE.as_bytes()))
        .expect("start");
    verifier
        .take_server_first(SERVER_FIRST.as_bytes())
        .expect("client-final");
    let rejected = verifier.take_server_final(format!("{SERVER_FINAL}!").as_bytes());
    assert_eq!(rejected, Err(ScramFailure::MalformedAttribute));
}

/// Ports `ScramTest::serverFinalBeforeServerFirstFails`. The early call leaves the exchange
/// waiting for server-first.
#[test]
fn server_final_before_server_first_fails() {
    let mut scram = SaslScram::default();
    scram
        .start("user", &pencil(), Some(NONCE.as_bytes()))
        .expect("start");
    assert_eq!(
        scram.take_server_final(SERVER_FINAL.as_bytes()),
        Err(ScramFailure::NotAwaitingFinal)
    );
    assert_eq!(
        scram.take_server_first(SERVER_FIRST.as_bytes()).as_deref(),
        Ok(CLIENT_FINAL.as_bytes())
    );
}

/// Ports `ScramTest::secondCallInTheWrongStateFails`.
#[test]
fn second_call_in_the_wrong_state_fails() {
    let mut scram = SaslScram::default();
    scram
        .start("user", &pencil(), Some(NONCE.as_bytes()))
        .expect("start");
    scram
        .take_server_first(SERVER_FIRST.as_bytes())
        .expect("client-final");
    assert_eq!(
        scram.take_server_first(SERVER_FIRST.as_bytes()),
        Err(ScramFailure::NotAwaitingFirst)
    );

    scram
        .take_server_final(SERVER_FINAL.as_bytes())
        .expect("server-final");
    assert_eq!(
        scram.take_server_final(SERVER_FINAL.as_bytes()),
        Err(ScramFailure::NotAwaitingFinal)
    );
}

/// Ports `ScramTest::generatedNonceIsPrintableAndHasNoComma`.
#[test]
fn generated_nonce_is_printable_and_has_no_comma() {
    let mut scram = SaslScram::default();
    let started = scram.start("user", &pencil(), None).expect("start");
    let prefix = b"n,,n=user,r=";
    assert!(started.starts_with(prefix), "client-first = {started:?}");
    let nonce = &started[prefix.len()..];
    assert!(nonce.len() >= 16, "nonce = {nonce:?}");
    assert!(nonce
        .iter()
        .all(|&byte| (0x21..=0x7E).contains(&byte) && byte != b','));

    let mut other = SaslScram::default();
    let again = other.start("user", &pencil(), None).expect("start");
    assert_ne!(again, started, "two generated nonces must differ");
}

/// The AUTHENTICATE frames carry the base64 of the raw message, 400 bytes at a time, and a
/// message whose encoding is an exact multiple of 400 ends with a bare `+`.
#[test]
fn encode_sasl_frames_chunks_at_400_and_marks_an_exact_boundary() {
    let frames = |raw: &[u8]| -> Vec<String> {
        encode_sasl_frames(raw)
            .into_iter()
            .map(|frame| String::from_utf8(frame).expect("ascii frame"))
            .collect()
    };
    assert_eq!(frames(b""), ["AUTHENTICATE +\r\n"]);
    assert_eq!(frames(b"a"), ["AUTHENTICATE YQ==\r\n"]);

    // 300 bytes encode to exactly 400 characters: one full frame, then the bare `+`.
    let exact = vec![b'x'; 300];
    let encoded = frames(&exact);
    assert_eq!(encoded.len(), 2);
    assert_eq!(encoded[0].len(), "AUTHENTICATE \r\n".len() + 400);
    assert_eq!(encoded[1], "AUTHENTICATE +\r\n");
}

/// A challenge split over full chunks is reassembled; a short chunk completes it, and a
/// malformed or oversized stream fails.
#[test]
fn challenge_buffer_reassembles_chunks_and_refuses_bad_streams() {
    let raw = vec![b'z'; 450];
    let encoded = STANDARD.encode(&raw);
    let mut buffer = ChallengeBuffer::default();
    assert_eq!(buffer.take(&encoded.as_bytes()[..400]), Ok(None));
    assert_eq!(buffer.take(&encoded.as_bytes()[400..]), Ok(Some(raw)));

    let mut bad = ChallengeBuffer::default();
    assert_eq!(bad.take(b"AB!D"), Err(ScramFailure::MalformedChallenge));
    assert_eq!(bad.take(b""), Err(ScramFailure::MalformedChallenge));

    let mut oversized = ChallengeBuffer::default();
    let chunk = [b'A'; 400];
    for _ in 0..10 {
        assert_eq!(oversized.take(&chunk), Ok(None));
    }
    assert_eq!(
        oversized.take(&chunk),
        Err(ScramFailure::MalformedChallenge)
    );
}
