//! Closed-channel persistence across unsolicited joins and relaunch.

use crate::irc::command::ComposerSurface;

use super::persistence::{
    closed_has, disk, feed, frame_count, has_conversation, launch, libera, save, Loopbacks, LIBERA,
};
use crate::util::testdir::TempDir;

#[test]
fn an_unsolicited_self_join_keeps_a_closed_channel() {
    const CONVERSATION: ComposerSurface = ComposerSurface::Conversation;
    let dir = TempDir::new("closed-channel-restart");
    let backing = disk(&dir);
    let loopbacks = Loopbacks::default();
    {
        let mut first = launch(&backing, Box::new(loopbacks.clone()));
        save(&mut first, libera(LIBERA));
        assert!(first.start(LIBERA));
        feed(&mut first, &loopbacks, LIBERA, ":irc.libera.chat 001 fred :Welcome");
        feed(
            &mut first,
            &loopbacks,
            LIBERA,
            ":fred!u@h JOIN :#omarchy\r\n\
             :fred!u@h JOIN :#lab\r\n\
             :fred!u@h JOIN :#desk",
        );
        for channel in ["#lab", "#desk"] {
            first.select_conversation(LIBERA, channel);
            assert!(first.submit(CONVERSATION, "/part"), "/part {channel}");
            assert!(first.submit(CONVERSATION, "/close"), "/close {channel}");
            assert!(
                !has_conversation(&first, LIBERA, channel),
                "{channel} stayed after close"
            );
            assert!(closed_has(&first, LIBERA, channel), "close did not remember {channel}");
        }
        assert_eq!(frame_count(&loopbacks, LIBERA, "PART #lab"), 1);

        feed(
            &mut first,
            &loopbacks,
            LIBERA,
            ":fred!u@h JOIN :#lab\r\n\
             :irc.libera.chat 353 fred = #lab :fred\r\n\
             :irc.libera.chat 366 fred #lab :End of NAMES\r\n\
             :lena!u@h PRIVMSG #lab :still",
        );
        assert!(
            !has_conversation(&first, LIBERA, "#lab"),
            "an unsolicited join brought #lab back"
        );
        assert!(
            closed_has(&first, LIBERA, "#lab"),
            "the unsolicited join forgot the close"
        );

        first.select_conversation(LIBERA, "#omarchy");
        assert!(
            first.submit(CONVERSATION, "/join #desk,#gamma"),
            "SendMessage(/join #desk,#gamma)"
        );
        assert!(
            has_conversation(&first, LIBERA, "#gamma"),
            "the last join target was not opened"
        );
        assert!(
            !closed_has(&first, LIBERA, "#desk"),
            "an explicit join left #desk closed"
        );
        assert!(
            closed_has(&first, LIBERA, "#lab"),
            "#lab close was cleared with #desk"
        );
        feed(
            &mut first,
            &loopbacks,
            LIBERA,
            ":fred!u@h JOIN :#desk\r\n\
             :irc.libera.chat 353 fred = #desk :fred\r\n\
             :irc.libera.chat 366 fred #desk :End of NAMES",
        );
        assert!(
            has_conversation(&first, LIBERA, "#desk"),
            "an explicit join did not open #desk"
        );
        assert!(
            !closed_has(&first, LIBERA, "#desk"),
            "an explicit join left #desk closed after the echo"
        );
        assert!(
            closed_has(&first, LIBERA, "#lab"),
            "#lab close was cleared with #desk"
        );
    }

    let mut again = launch(&backing, Box::new(loopbacks.clone()));
    assert!(again.start(LIBERA));
    feed(&mut again, &loopbacks, LIBERA, ":irc.libera.chat 001 fred :Welcome");
    feed(
        &mut again,
        &loopbacks,
        LIBERA,
        ":fred!u@h JOIN :#lab\r\n\
         :irc.libera.chat 353 fred = #lab :fred\r\n\
         :irc.libera.chat 366 fred #lab :End of NAMES\r\n\
         :lena!u@h PRIVMSG #lab :still",
    );
    assert!(
        !has_conversation(&again, LIBERA, "#lab"),
        "restart join brought #lab back"
    );
    assert!(
        closed_has(&again, LIBERA, "#lab"),
        "restart join forgot the close"
    );
    assert_eq!(
        frame_count(&loopbacks, LIBERA, "PART #lab"),
        0,
        "restart join wrote PART"
    );
    again.open_status(LIBERA);
    assert!(again.submit(ComposerSurface::Status, "/join #lab"));
    assert!(
        has_conversation(&again, LIBERA, "#lab"),
        "an explicit join did not open #lab"
    );
    assert!(
        !closed_has(&again, LIBERA, "#lab"),
        "an explicit join left #lab closed"
    );
}
