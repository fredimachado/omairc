//! Port of the PLAIN and non-SCRAM cases of `tui/internal/session/session_auth_test.go`.
//!
//! The SCRAM-SHA-256 cases (`TestSessionNegotiatesSaslScramSha256` and the
//! `TestSessionScram*` / `TestSessionAbandonedScram*` family) are in `scram.rs`,
//! beside this file. The Status-entry assertions wait for the Phase 3 Status
//! recorder; the AUTHENTICATE secrecy check is kept as a check that the
//! outbound event carries the masked-free wire line only where Go does.

use super::fixture::{assert_frames, test_config, Fixture};
use crate::session::events::{ErrorKind, SessionState};

/// The payload a client sent in an AUTHENTICATE frame, decoded from base64.
fn decode_authenticate(frame: &str) -> Vec<u8> {
    let body = frame
        .strip_prefix("AUTHENTICATE ")
        .and_then(|rest| rest.strip_suffix("\r\n"))
        .expect("an AUTHENTICATE frame");
    base64_decode(body).expect("base64 payload")
}

/// The base64 body of an AUTHENTICATE frame, without its framing.
fn authenticate_body(frame: &str) -> &str {
    frame
        .strip_prefix("AUTHENTICATE ")
        .and_then(|rest| rest.strip_suffix("\r\n"))
        .expect("an AUTHENTICATE frame")
}

fn base64_decode(text: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let trimmed = text.trim_end_matches('=');
    let mut out = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0;
    for byte in trimmed.bytes() {
        let value = ALPHABET.iter().position(|&candidate| candidate == byte)? as u32;
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Some(out)
}

fn base64_encode(raw: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in raw.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = u32::from(chunk.get(1).copied().unwrap_or(0));
        let b2 = u32::from(chunk.get(2).copied().unwrap_or(0));
        let word = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(word >> 18) as usize & 63] as char);
        out.push(ALPHABET[(word >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(word >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[word as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn index_of(frames: &[String], want: &str) -> Option<usize> {
    frames.iter().position(|frame| frame == want)
}

#[test]
fn negotiates_sasl_plain() {
    let mut config = test_config();
    config.password = "secret".to_owned().into();
    let fx = Fixture::new(config);
    fx.connect_tls();
    fx.inject(":server CAP omairc LS :sasl=PLAIN,EXTERNAL\r\n");
    assert_frames(
        &fx.frames(),
        &[
            "CAP LS 302\r\n",
            "CAP REQ :sasl\r\n",
            "NICK omairc\r\n",
            "USER omairc 8 * :Omairc User\r\n",
        ],
    );

    fx.inject(":server CAP omairc ACK :sasl\r\n");
    assert_eq!(fx.session.state(), SessionState::Sasl);
    assert_eq!(fx.last_frame(), "AUTHENTICATE PLAIN\r\n");

    fx.inject("AUTHENTICATE +\r\n");
    assert_eq!(
        decode_authenticate(&fx.last_frame()),
        b"omairc\0omairc\0secret"
    );

    fx.inject(":server 903 omairc :SASL successful\r\n");
    assert_eq!(fx.last_frame(), "CAP END\r\n");
}

#[test]
fn sasl_plain_reassembles_multi_chunk_messages() {
    let secret = "x".repeat(287);
    let mut config = test_config();
    config.password = secret.clone().into();
    let fx = Fixture::new(config);
    fx.connect_tls();
    fx.inject(":server CAP omairc LS :sasl=PLAIN\r\n:server CAP omairc ACK :sasl\r\n");
    assert_eq!(fx.session.state(), SessionState::Sasl);

    let plain = format!("omairc\0omairc\0{secret}");
    assert_eq!(plain.len(), 301);
    let encoded = base64_encode(plain.as_bytes());
    assert_eq!(encoded.len(), 404);

    let before = fx.frames().len();
    fx.inject("AUTHENTICATE +\r\n");
    let frames = fx.frames();
    assert_eq!(frames.len(), before + 2, "two chunks");
    assert_eq!(authenticate_body(&frames[before]).len(), 400);
    assert_eq!(authenticate_body(&frames[before + 1]).len(), 4);
    let reassembled = format!(
        "{}{}",
        authenticate_body(&frames[before]),
        authenticate_body(&frames[before + 1])
    );
    assert_eq!(reassembled, encoded);
    assert!(fx.errors().is_empty());
    assert_eq!(fx.session.state(), SessionState::Sasl);

    fx.inject(":server 903 omairc :SASL successful\r\n");
    assert_eq!(fx.last_frame(), "CAP END\r\n");
    assert_eq!(fx.session.state(), SessionState::Registering);
}

#[test]
fn sasl_plain_exact_chunk_boundary_requires_plus() {
    for secret_length in [286, 586] {
        let secret = "x".repeat(secret_length);
        let mut config = test_config();
        config.password = secret.clone().into();
        let fx = Fixture::new(config);
        fx.connect_tls();
        fx.inject(":server CAP omairc LS :sasl=PLAIN\r\n:server CAP omairc ACK :sasl\r\n");
        assert_eq!(fx.session.state(), SessionState::Sasl);

        let encoded = base64_encode(format!("omairc\0omairc\0{secret}").as_bytes());
        assert_eq!(
            encoded.len() % 400,
            0,
            "encoded length must be a multiple of 400"
        );
        let chunks = encoded.len() / 400;
        let before = fx.frames().len();
        fx.inject("AUTHENTICATE +\r\n");
        let frames = fx.frames();
        assert_eq!(frames.len(), before + chunks + 1);
        let mut reassembled = String::new();
        for index in 0..chunks {
            let body = authenticate_body(&frames[before + index]);
            assert_eq!(body.len(), 400);
            reassembled.push_str(body);
        }
        assert_eq!(frames[before + chunks], "AUTHENTICATE +\r\n");
        assert_eq!(reassembled, encoded);
        assert!(fx.errors().is_empty());

        fx.inject(":server 903 omairc :SASL successful\r\n");
        assert_eq!(fx.last_frame(), "CAP END\r\n");
        assert_eq!(fx.session.state(), SessionState::Registering);
    }
}

#[test]
fn sasl_account_authenticates_as_the_bouncer_name() {
    let mut config = test_config();
    config.sasl_account = "joe/freenode".to_owned();
    config.password = "secret".to_owned().into();
    let fx = Fixture::new(config);
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :sasl=PLAIN\r\n:server CAP omairc ACK :sasl\r\nAUTHENTICATE +\r\n",
    );
    assert_eq!(
        decode_authenticate(&fx.last_frame()),
        b"joe/freenode\0joe/freenode\0secret"
    );
    assert_eq!(fx.session.nick(), "omairc");
}

#[test]
fn empty_sasl_account_authenticates_as_the_nick() {
    let mut config = test_config();
    config.password = "secret".to_owned().into();
    let fx = Fixture::new(config);
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :sasl=PLAIN\r\n:server CAP omairc ACK :sasl\r\nAUTHENTICATE +\r\n",
    );
    assert_eq!(
        decode_authenticate(&fx.last_frame()),
        b"omairc\0omairc\0secret"
    );
}

#[test]
fn sends_pass_when_sasl_is_unavailable() {
    let mut config = test_config();
    config.password = "secret".to_owned().into();
    let fx = Fixture::new(config);
    fx.connect_tls();
    fx.inject(":server CAP omairc LS :invite-notify\r\n");
    assert_frames(
        &fx.written_since(1),
        &[
            "PASS secret\r\n",
            "NICK omairc\r\n",
            "USER omairc 8 * :Omairc User\r\n",
            "CAP END\r\n",
        ],
    );
}

#[test]
fn nickserv_only_sasl_plain_uses_nickserv_secret() {
    let mut config = test_config();
    config.nickserv_password = "nick-secret".to_owned().into();
    let fx = Fixture::new(config);
    fx.connect_tls();
    fx.inject(":server CAP omairc LS :sasl=PLAIN\r\n");
    assert_frames(
        &fx.frames(),
        &[
            "CAP LS 302\r\n",
            "CAP REQ :sasl\r\n",
            "NICK omairc\r\n",
            "USER omairc 8 * :Omairc User\r\n",
        ],
    );
    assert!(
        !fx.wrote("PASS nick-secret\r\n"),
        "the NickServ secret must not ride PASS"
    );

    fx.inject(":server CAP omairc ACK :sasl\r\nAUTHENTICATE +\r\n");
    assert_eq!(
        decode_authenticate(&fx.last_frame()),
        b"omairc\0omairc\0nick-secret"
    );

    fx.inject(":server 903 omairc :SASL successful\r\n:server 001 omairc :Welcome\r\n");
    assert!(
        !fx.wrote("PRIVMSG NickServ :IDENTIFY nick-secret\r\n"),
        "a successful SASL must not IDENTIFY"
    );
    assert!(fx.wrote("JOIN #omarchy\r\n"), "autojoin missing");
}

#[test]
fn nickserv_only_without_sasl_identifies_before_join() {
    let mut config = test_config();
    config.nickserv_password = "nick-secret".to_owned().into();
    let fx = Fixture::new(config);
    fx.connect_tls();
    fx.inject(":server CAP omairc LS :invite-notify\r\n:server 001 omairc :Welcome\r\n");
    let frames = fx.frames();
    assert!(
        !fx.wrote("PASS nick-secret\r\n"),
        "no PASS is expected without a server password"
    );
    let identify = index_of(&frames, "PRIVMSG NickServ :IDENTIFY nick-secret\r\n");
    let join = index_of(&frames, "JOIN #omarchy\r\n");
    match (identify, join) {
        (Some(identify), Some(join)) => assert!(identify < join),
        other => panic!("identify and join must both be written: {other:?}"),
    }
}

#[test]
fn both_secrets_sasl_sends_pass_and_plain_from_nickserv() {
    let mut config = test_config();
    config.password = "server-secret".to_owned().into();
    config.nickserv_password = "nick-secret".to_owned().into();
    let fx = Fixture::new(config);
    fx.connect_tls();
    fx.inject(":server CAP omairc LS :sasl=PLAIN\r\n");
    assert_frames(
        &fx.frames(),
        &[
            "CAP LS 302\r\n",
            "CAP REQ :sasl\r\n",
            "PASS server-secret\r\n",
            "NICK omairc\r\n",
            "USER omairc 8 * :Omairc User\r\n",
        ],
    );
    fx.inject(":server CAP omairc ACK :sasl\r\nAUTHENTICATE +\r\n");
    let plain = decode_authenticate(&fx.last_frame());
    assert_eq!(plain, b"omairc\0omairc\0nick-secret");
    assert!(!plain.windows(13).any(|window| window == b"server-secret"));
}

#[test]
fn both_secrets_without_sasl_pass_then_identify_before_join() {
    let mut config = test_config();
    config.password = "server-secret".to_owned().into();
    config.nickserv_password = "nick-secret".to_owned().into();
    let fx = Fixture::new(config);
    fx.connect_tls();
    fx.inject(":server CAP omairc LS :invite-notify\r\n:server 001 omairc :Welcome\r\n");
    assert!(
        fx.wrote("PASS server-secret\r\n"),
        "the server password must ride PASS"
    );
    let frames = fx.frames();
    let identify = index_of(&frames, "PRIVMSG NickServ :IDENTIFY nick-secret\r\n");
    let join = index_of(&frames, "JOIN #omarchy\r\n");
    match (identify, join) {
        (Some(identify), Some(join)) => assert!(identify < join),
        other => panic!("identify and join must both be written: {other:?}"),
    }
}

#[test]
fn sasl_success_does_not_identify() {
    let mut config = test_config();
    config.password = "server-secret".to_owned().into();
    config.nickserv_password = "nick-secret".to_owned().into();
    let fx = Fixture::new(config);
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :sasl=PLAIN\r\n\
         :server CAP omairc ACK :sasl\r\n\
         AUTHENTICATE +\r\n\
         :server 903 omairc :SASL successful\r\n\
         :server 001 omairc :Welcome\r\n",
    );
    assert!(!fx.wrote("PRIVMSG NickServ :IDENTIFY nick-secret\r\n"));
    assert!(fx.wrote("JOIN #omarchy\r\n"));
}

#[test]
fn sasl_failure_does_not_fall_through_to_identify() {
    let mut config = test_config();
    config.nickserv_password = "nick-secret".to_owned().into();
    let fx = Fixture::new(config);
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :sasl=PLAIN\r\n\
         :server CAP omairc ACK :sasl\r\n\
         AUTHENTICATE +\r\n\
         :server 904 omairc :SASL failed\r\n\
         :server 001 omairc :Welcome\r\n",
    );
    assert_eq!(fx.errors().len(), 1);
    assert_eq!(fx.session.state(), SessionState::Failed);
    assert!(!fx.wrote("PRIVMSG NickServ :IDENTIFY nick-secret\r\n"));
    assert!(!fx.wrote("JOIN #omarchy\r\n"));
}

#[test]
fn authentication_failure_is_explicit() {
    let mut config = test_config();
    config.password = "secret".to_owned().into();
    let fx = Fixture::new(config);
    fx.connect_tls();
    fx.inject(":server CAP omairc LS :sasl\r\n:server CAP omairc ACK :sasl\r\n:server 904 omairc :SASL failed\r\n");
    assert_eq!(fx.session.state(), SessionState::Failed);
    let errors = fx.errors();
    assert_eq!(errors.last().unwrap().0, ErrorKind::Authentication);
}
