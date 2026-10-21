//! Hostile inbound input must never panic the session. Each line below reaches a
//! different dispatch path (batch tokens, CAP subcommands, CHATHISTORY FAIL,
//! read markers, labels, CTCP, malformed frames) with inputs a server could send:
//! multi-byte sigils, empty fields, bare CRLF, truncated tags and oversized values.

use super::fixture::{test_config, Fixture};

const HOSTILE: &[&str] = &[
    "BATCH é",
    "BATCH +",
    "BATCH -",
    "BATCH ±ref chathistory #c",
    "@batch= :x BATCH +",
    "@batch=ñ :a!b@c PRIVMSG #c :hi",
    "CAP",
    "CAP * LS",
    "CAP * NEW",
    "CAP * DEL :",
    "CAP * ACK :-",
    "CAP * NAK :é",
    "AUTHENTICATE é",
    "005 omairc CHANTYPES= CHATHISTORY=-5 :x",
    "005 omairc CHATHISTORY=99999999999999999999999 :x",
    "FAIL CHATHISTORY",
    "FAIL CHATHISTORY INVALID_TARGET AFTER",
    "FAIL MARKREAD é é é é :x",
    "MARKREAD",
    "MARKREAD #c timestamp=é",
    "PING",
    "PONG",
    ":nick!user@host PRIVMSG omairc :\u{1}\u{1}",
    ":nick!user@host PRIVMSG omairc :\u{1}PING \u{1}",
    ":nick!user@host PRIVMSG omairc :\u{1}é\u{1}",
    "774 omairc a 99999",
    "903",
    "904 omairc",
    "433 * é",
    "421 omairc CAP",
    "ERROR",
    "ERROR :",
    "475 omairc é",
    "NICK é",
    "366 omairc é",
];

#[test]
fn hostile_lines_never_panic_the_session() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.inject(":server CAP omairc LS :multi-prefix batch chathistory message-tags labeled-response draft/read-marker\r\n");
    fx.inject(":server 001 omairc :Welcome\r\n");
    for line in HOSTILE {
        fx.inject(&format!("{line}\r\n"));
        // Also feed the same line as a partial frame, split mid-way.
        let half = line.len() / 2;
        fx.transport.inject_bytes(&line.as_bytes()[..half]);
        fx.transport.inject_bytes(&line.as_bytes()[half..]);
        fx.inject("\r\n");
    }
    // A giant unterminated run and a bare NUL must also be survivable.
    fx.transport.inject_bytes(&vec![b'x'; 9000]);
    fx.transport.inject_bytes(b"\x00\r\n");
    let _ = fx.events();
}
