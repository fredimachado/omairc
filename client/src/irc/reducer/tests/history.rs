//! Replay splicing, msgid dedup, channel-context delivery, the log round trip, and
//! the CHATHISTORY page rules.

use super::*;
use crate::irc::event::{HistoryKind, MessageKindTag, MsgId, ReplayLine};
use crate::irc::membershipnoise::MembershipNoise;
use crate::irc::reducer::MAX_MESSAGES;
use crate::irc::world::{Message, MessageKind, Origin};

#[test]
fn msgid_dedup_skips_live_then_replay() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let room = reducer.conversation_key(NETWORK_A, "#omarchy");
    fold(&mut reducer, join(NETWORK_A, "#omarchy", "omairc"));
    fold(
        &mut reducer,
        MessageEvent {
            msgid: MsgId::new("same"),
            ..message(&room, "alice", "first", "#omarchy")
        },
    );
    fold(
        &mut reducer,
        history_event(&room, "#omarchy", HistoryKind::Chat, false, vec![replay_line("alice", "second", "same")]),
    );

    let conversation = state_of(&reducer, &room);
    assert_eq!(conversation.messages.len(), 2, "messages");
    assert_eq!(conversation.messages[0].body, "omairc joined", "messages[0]");
    assert_eq!(conversation.messages[1].body, "first", "messages[1]");
    assert_eq!(conversation.messages[1].origin, Origin::Live, "messages[1] origin");
}

#[test]
fn msgid_dedup_skips_replay_then_live() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let room = reducer.conversation_key(NETWORK_A, "#omarchy");
    fold(&mut reducer, join(NETWORK_A, "#omarchy", "omairc"));
    fold(
        &mut reducer,
        history_event(&room, "#omarchy", HistoryKind::Chat, false, vec![replay_line("alice", "first", "same")]),
    );
    fold(
        &mut reducer,
        MessageEvent {
            msgid: MsgId::new("same"),
            ..message(&room, "alice", "second", "#omarchy")
        },
    );

    let conversation = state_of(&reducer, &room);
    assert_eq!(conversation.messages.len(), 2, "messages");
    assert_eq!(conversation.messages[0].body, "first", "messages[0]");
    assert_eq!(conversation.messages[0].origin, Origin::Replay, "messages[0] origin");
    assert_eq!(conversation.messages[1].body, "omairc joined", "messages[1]");
}

#[test]
fn nick_merge_drops_duplicate_msgids() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let alice = reducer.conversation_key(NETWORK_A, "Alice");
    let alicia = reducer.conversation_key(NETWORK_A, "Alicia");
    fold(&mut reducer, MessageEvent {
        msgid: MsgId::new("same"),
        ..message(&alice, "Alice", "shared", "Alice")
    });
    fold(&mut reducer, MessageEvent {
        msgid: MsgId::new("same"),
        ..message(&alicia, "Alicia", "shared", "Alicia")
    });
    fold(&mut reducer, message(&alicia, "Alicia", "only here", "Alicia"));

    fold(&mut reducer, nick_change(NETWORK_A, "Alice", "Alicia"));

    let merged = state_of(&reducer, &alicia);
    assert_eq!(bodies(merged), vec!["shared", "only here", "Alice is now Alicia"]);
}

#[test]
fn nick_collision_merges_message_ids() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let alice = reducer.conversation_key(NETWORK_A, "Alice");
    let alicia = reducer.conversation_key(NETWORK_A, "Alicia");
    fold(&mut reducer, MessageEvent {
        msgid: MsgId::new("id-a"),
        ..message(&alice, "Alice", "from alice", "Alice")
    });
    fold(&mut reducer, MessageEvent {
        msgid: MsgId::new("id-b"),
        ..message(&alicia, "Alicia", "from alicia", "Alicia")
    });

    fold(&mut reducer, nick_change(NETWORK_A, "Alice", "Alicia"));

    let merged_len = state_of(&reducer, &alicia).messages.len();
    assert!(merged_len >= 2, "merged has both rows");

    fold(&mut reducer, MessageEvent {
        msgid: MsgId::new("id-a"),
        ..message(&alicia, "Alicia", "repeat a", "Alicia")
    });
    fold(&mut reducer, MessageEvent {
        msgid: MsgId::new("id-b"),
        ..message(&alicia, "Alicia", "repeat b", "Alicia")
    });
    assert_eq!(state_of(&reducer, &alicia).messages.len(), merged_len, "messages after repeats");

    fold(&mut reducer, MessageEvent {
        msgid: MsgId::new("id-c"),
        ..message(&alicia, "Alicia", "fresh", "Alicia")
    });
    let conversation = state_of(&reducer, &alicia);
    assert_eq!(conversation.messages.len(), merged_len + 1, "messages after fresh");
    assert_eq!(conversation.messages.last().expect("a row").body, "fresh", "fresh body");
}

#[test]
fn only_visible_to_you_survives_history_and_hydrate() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    fold(&mut reducer, join(NETWORK_A, "#room", "omairc"));
    fold(&mut reducer, join(NETWORK_A, "#room", "bot"));
    apply_wire(&mut reducer, "@+draft/channel-context=#Room :bot!u@h PRIVMSG omairc :just you");
    apply_wire(&mut reducer, ":alice!u@h PRIVMSG #room :everyone");

    let room = reducer.conversation_key(NETWORK_A, "#room");
    {
        let conversation = state_of(&reducer, &room);
        assert_eq!(conversation.target, "#room", "buffer name");
        let marked = message_by_body(&conversation.messages, "just you");
        assert!(marked.only_visible_to_you, "live only you");
        assert_eq!(marked.kind, MessageKind::Message, "live kind");
        assert_eq!(marked.origin, Origin::Live, "live origin");
        assert!(
            !message_by_body(&conversation.messages, "everyone").only_visible_to_you,
            "ordinary"
        );
    }
    assert!(
        reducer.find(&reducer.conversation_key(NETWORK_A, "bot")).is_none(),
        "no direct for the private delivery"
    );

    let playback = batch(
        "#room",
        HistoryKind::Chat,
        &[
            "@msgid=replay-only;+draft/channel-context=#room :bot!u@h PRIVMSG omairc :replayed",
            "@msgid=replay-public :alice!u@h PRIVMSG #room :public replay",
        ],
    );
    let history = history_of(&reducer, &playback);
    fold(&mut reducer, history);
    let conversation = state_of(&reducer, &room);
    let replayed = message_by_body(&conversation.messages, "replayed");
    assert!(replayed.only_visible_to_you, "replay only you");
    assert_eq!(replayed.origin, Origin::Replay, "replay origin");
    assert!(
        !message_by_body(&conversation.messages, "public replay").only_visible_to_you,
        "public replay"
    );

    let log = RecordingLog::default();
    let mut first = EventReducer::new();
    first.set_conversation_log(Some(Box::new(log.clone())));
    welcome_omairc(&mut first, NETWORK_A);
    fold(&mut first, join(NETWORK_A, "#room", "omairc"));
    fold(&mut first, join(NETWORK_A, "#room", "bot"));
    apply_wire(&mut first, "@msgid=stored-only;+draft/channel-context=#room :bot!u@h PRIVMSG omairc :stored");
    let mut again = EventReducer::new();
    again.set_conversation_log(Some(Box::new(log.clone())));
    welcome_omairc(&mut again, NETWORK_A);
    fold(&mut again, join(NETWORK_A, "#room", "omairc"));
    let hydrated = message_by_body(&state_of(&again, &room).messages, "stored").clone();
    assert!(hydrated.only_visible_to_you, "hydrated only you");
    assert_eq!(hydrated.origin, Origin::Replay, "hydrated origin");
}

#[test]
fn channel_context_msgid_skips_query_after_hydrate() {
    let log = RecordingLog::default();
    let mut first = EventReducer::new();
    first.set_conversation_log(Some(Box::new(log.clone())));
    welcome_omairc(&mut first, NETWORK_A);
    fold(&mut first, join(NETWORK_A, "#room", "omairc"));
    fold(&mut first, join(NETWORK_A, "#room", "bot"));
    apply_wire(
        &mut first,
        "@msgid=stored-only;+draft/channel-context=#room :bot!u@h PRIVMSG omairc :stored",
    );

    let mut again = EventReducer::new();
    again.set_conversation_log(Some(Box::new(log.clone())));
    welcome_omairc(&mut again, NETWORK_A);
    fold(&mut again, join(NETWORK_A, "#room", "omairc"));
    let room = again.conversation_key(NETWORK_A, "#room");
    assert!(
        message_by_body(&state_of(&again, &room).messages, "stored").only_visible_to_you,
        "hydrated only you"
    );

    let bot = again.conversation_key(NETWORK_A, "bot");
    again.ensure_conversation(&bot, "bot", crate::irc::event::IrcConversationCause::UserOpen);
    let history = history_of(
        &again,
        &batch(
            "bot",
            HistoryKind::Chat,
            &["@msgid=stored-only;+draft/channel-context=#room :bot!u@h PRIVMSG omairc :stored"],
        ),
    );
    fold(&mut again, history);
    assert!(
        state_of(&again, &bot)
            .messages
            .iter()
            .all(|message| message.body != "stored"),
        "hydrate claim must still skip the query twin"
    );
}

#[test]
fn channel_context_msgid_skips_query_after_part() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    fold(&mut reducer, join(NETWORK_A, "#room", "omairc"));
    fold(&mut reducer, join(NETWORK_A, "#room", "bot"));
    apply_wire(
        &mut reducer,
        "@msgid=part-keep;+draft/channel-context=#room :bot!u@h PRIVMSG omairc :kept",
    );
    fold(&mut reducer, part(NETWORK_A, "#room", "omairc"));

    let room = reducer.conversation_key(NETWORK_A, "#room");
    assert!(
        !state_of(&reducer, &room).channel.as_ref().expect("channel").joined,
        "left the channel"
    );
    assert!(
        message_by_body(&state_of(&reducer, &room).messages, "kept").only_visible_to_you,
        "channel still holds the filing"
    );

    let bot = reducer.conversation_key(NETWORK_A, "bot");
    reducer.ensure_conversation(&bot, "bot", crate::irc::event::IrcConversationCause::UserOpen);
    let history = history_of(
        &reducer,
        &batch(
            "bot",
            HistoryKind::Chat,
            &["@msgid=part-keep;+draft/channel-context=#room :bot!u@h PRIVMSG omairc :kept"],
        ),
    );
    fold(&mut reducer, history);
    assert!(
        state_of(&reducer, &bot)
            .messages
            .iter()
            .all(|message| message.body != "kept"),
        "ownership survives PART while the channel row remains"
    );
}

#[test]
fn channel_context_msgid_skips_query_for_case_differing_sender() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    fold(&mut reducer, join(NETWORK_A, "#room", "omairc"));
    fold(&mut reducer, join(NETWORK_A, "#room", "bot"));
    apply_wire(
        &mut reducer,
        "@msgid=case-1;+draft/channel-context=#room :Bot!u@h PRIVMSG omairc :cased",
    );

    let room = reducer.conversation_key(NETWORK_A, "#room");
    assert!(
        message_by_body(&state_of(&reducer, &room).messages, "cased").only_visible_to_you,
        "live relocates despite sender case"
    );
    assert!(
        reducer.find(&reducer.conversation_key(NETWORK_A, "bot")).is_none(),
        "no live direct for cased sender"
    );

    let bot = reducer.conversation_key(NETWORK_A, "bot");
    reducer.ensure_conversation(&bot, "bot", crate::irc::event::IrcConversationCause::UserOpen);
    let history = history_of(
        &reducer,
        &batch(
            "bot",
            HistoryKind::Chat,
            &["@msgid=case-1;+draft/channel-context=#room :Bot!u@h PRIVMSG omairc :cased"],
        ),
    );
    fold(&mut reducer, history);
    assert!(
        state_of(&reducer, &bot)
            .messages
            .iter()
            .all(|message| message.body != "cased"),
        "claimed msgid skips the case-differing query twin"
    );
}

#[test]
fn channel_context_msgid_skips_query_replay() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    fold(&mut reducer, join(NETWORK_A, "#room", "omairc"));
    fold(&mut reducer, join(NETWORK_A, "#room", "bot"));
    apply_wire(
        &mut reducer,
        "@msgid=only-1;+draft/channel-context=#room :bot!u@h PRIVMSG omairc :just you",
    );

    let room = reducer.conversation_key(NETWORK_A, "#room");
    assert!(
        message_by_body(&state_of(&reducer, &room).messages, "just you").only_visible_to_you,
        "live only you"
    );
    assert!(
        reducer.find(&reducer.conversation_key(NETWORK_A, "bot")).is_none(),
        "no live direct"
    );

    let bot = reducer.conversation_key(NETWORK_A, "bot");
    reducer.ensure_conversation(&bot, "bot", crate::irc::event::IrcConversationCause::UserOpen);
    let history = history_of(
        &reducer,
        &batch(
            "bot",
            HistoryKind::Chat,
            &["@msgid=only-1;+draft/channel-context=#room :bot!u@h PRIVMSG omairc :just you"],
        ),
    );
    assert_eq!(history.lines.len(), 1, "query lines");
    assert!(!history.lines[0].only_visible_to_you, "query stays unlabeled");
    fold(&mut reducer, history);

    let direct = state_of(&reducer, &bot);
    assert!(
        direct.messages.iter().all(|message| message.body != "just you"),
        "query must not twin the channel filing"
    );
    assert_eq!(
        message_by_body(&state_of(&reducer, &room).messages, "just you").body,
        "just you",
        "channel keeps the filing"
    );
}

#[test]
fn channel_context_direct_message_survives_when_untrusted() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    fold(&mut reducer, join(NETWORK_A, "#room", "omairc"));
    fold(&mut reducer, join(NETWORK_A, "#room", "alice"));

    apply_wire(&mut reducer, "@+draft/channel-context=#room :eve!u@h PRIVMSG omairc :stranger");
    let eve = reducer.conversation_key(NETWORK_A, "eve");
    {
        let direct = state_of(&reducer, &eve);
        assert_eq!(direct.unread, 1, "stranger unread");
        let last = direct.messages.last().expect("a row");
        assert_eq!(last.body, "stranger", "stranger body");
        assert!(!last.only_visible_to_you, "stranger flag");
    }
    let stranger = reducer.take_mention_arrival().expect("stranger mention");
    assert_eq!(stranger.author, "eve", "stranger author");
    assert_eq!(stranger.target, "eve", "stranger target");
    assert_eq!(stranger.body, "stranger", "stranger mention body");
    assert!(
        !state_of(&reducer, &reducer.conversation_key(NETWORK_A, "#room"))
            .messages
            .last()
            .expect("a row")
            .body
            .eq("stranger"),
        "stranger stays out of the channel"
    );

    apply_wire(&mut reducer, "@+draft/channel-context=#secret :alice!u@h PRIVMSG omairc :elsewhere");
    let alice = reducer.conversation_key(NETWORK_A, "alice");
    {
        let alice_direct = state_of(&reducer, &alice);
        let last = alice_direct.messages.last().expect("a row");
        assert_eq!(last.body, "elsewhere", "elsewhere body");
        assert!(!last.only_visible_to_you, "elsewhere flag");
        assert_eq!(alice_direct.unread, 1, "elsewhere unread");
    }
    let elsewhere = reducer.take_mention_arrival().expect("elsewhere mention");
    assert_eq!(elsewhere.author, "alice", "elsewhere author");
    assert_eq!(elsewhere.body, "elsewhere", "elsewhere mention body");
    assert!(
        reducer.find(&reducer.conversation_key(NETWORK_A, "#secret")).is_none(),
        "no secret channel"
    );

    apply_wire(&mut reducer, "@+draft/channel-context=#a,#b :eve!u@h PRIVMSG omairc :comma");
    {
        let direct = state_of(&reducer, &eve);
        let last = direct.messages.last().expect("a row");
        assert_eq!(last.body, "comma", "comma body");
        assert!(!last.only_visible_to_you, "comma flag");
    }
    assert!(reducer.take_mention_arrival().is_some(), "comma fallback must notify");

    let history = history_of(
        &reducer,
        &batch("alice", HistoryKind::Chat, &["@+draft/channel-context=#room :alice!u@h PRIVMSG omairc :kept in query"]),
    );
    assert_eq!(history.lines.len(), 1, "query lines");
    assert!(!history.lines[0].only_visible_to_you, "query flag");
    fold(&mut reducer, history);
    let kept = state_of(&reducer, &alice)
        .messages
        .iter()
        .find(|message| message.body == "kept in query")
        .map(|message| message.only_visible_to_you);
    assert_eq!(kept, Some(false), "query kept the line, not only visible to you");

    fold(&mut reducer, part(NETWORK_A, "#room", "omairc"));
    apply_wire(&mut reducer, "@+draft/channel-context=#room :alice!u@h PRIVMSG omairc :after part");
    {
        let direct = state_of(&reducer, &alice);
        let last = direct.messages.last().expect("a row");
        assert_eq!(last.body, "after part", "after part body");
        assert!(!last.only_visible_to_you, "after part flag");
    }
    let parted = reducer.take_mention_arrival().expect("after part mention");
    assert_eq!(parted.body, "after part", "after part mention body");
    let room = state_of(&reducer, &reducer.conversation_key(NETWORK_A, "#room"));
    assert!(!room.channel.as_ref().expect("a channel").joined, "left the channel");
    assert!(
        room.messages.iter().all(|message| message.body != "after part"),
        "parted channel kept the private line"
    );
}

#[test]
fn history_page_cap_tail_splices_older_chat_at_head() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let room = reducer.conversation_key(NETWORK_A, "#room");
    fold(&mut reducer, join(NETWORK_A, "#room", "omairc"));
    fold(&mut reducer, MessageEvent {
        msgid: MsgId::new("live"),
        ..message(&room, "alice", "live", "#room")
    });
    assert!(state_of(&reducer, &room).channel.as_ref().expect("a channel").history_anchor.is_some(), "join anchor");

    fold(
        &mut reducer,
        history_event(&room, "#room", HistoryKind::Chat, true, vec![replay_line("bob", "older", "old")]),
    );
    let conversation = state_of(&reducer, &room);
    assert_eq!(conversation.messages.len(), 3, "messages");
    assert_eq!(conversation.messages[0].body, "older", "messages[0]");
    assert_eq!(conversation.messages[2].body, "live", "messages[2]");
    assert!(conversation.channel.as_ref().expect("a channel").history_anchor.is_some(), "join anchor kept");
}

#[test]
fn history_page_cap_tail_splices_direct_message_at_head() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let dm = reducer.conversation_key(NETWORK_A, "alice");
    reducer.ensure_conversation(&dm, "alice", crate::irc::event::IrcConversationCause::UserOpen);
    fold(&mut reducer, MessageEvent {
        msgid: MsgId::new("live"),
        ..message(&dm, "alice", "live", "alice")
    });

    fold(
        &mut reducer,
        history_event(&dm, "alice", HistoryKind::Chat, true, vec![replay_line("alice", "older", "old")]),
    );
    let conversation = state_of(&reducer, &dm);
    assert_eq!(conversation.messages.len(), 2, "messages");
    assert_eq!(conversation.messages[0].body, "older", "messages[0]");
}

#[test]
fn history_chat_does_not_reopen_closed_direct_message() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let dm = reducer.conversation_key(NETWORK_A, "alice");
    reducer.ensure_conversation(&dm, "alice", crate::irc::event::IrcConversationCause::UserOpen);
    assert!(reducer.drop_direct_message(&dm), "drop");
    fold(
        &mut reducer,
        history_event(&dm, "alice", HistoryKind::Chat, false, vec![replay_line("alice", "stale", "id")]),
    );
    assert!(reducer.find(&dm).is_none(), "conversation absent");
}

#[test]
fn history_page_prepend_persists_transcript_order() {
    let log = RecordingLog::default();
    let mut reducer = EventReducer::new();
    reducer.set_conversation_log(Some(Box::new(log.clone())));
    welcome_omairc(&mut reducer, NETWORK_A);
    let room = reducer.conversation_key(NETWORK_A, "#room");
    fold(&mut reducer, join(NETWORK_A, "#room", "omairc"));
    fold(&mut reducer, MessageEvent {
        msgid: MsgId::new("live"),
        ..message(&room, "alice", "live", "#room")
    });

    fold(
        &mut reducer,
        history_event(&room, "#room", HistoryKind::Chat, true, vec![replay_line("bob", "older", "old")]),
    );

    let lines = log.snapshot();
    assert!(lines.len() >= 2, "log lines = {}", lines.len());
    assert_eq!(lines[0].body, "older", "log[0]");
    assert_eq!(lines.last().expect("a line").body, "live", "log tail");
}

#[test]
fn older_page_splices_at_head_without_tail_cap_flag() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let room = reducer.conversation_key(NETWORK_A, "#room");
    fold(&mut reducer, join(NETWORK_A, "#room", "omairc"));
    fold(&mut reducer, MessageEvent {
        msgid: MsgId::new("live"),
        ..message(&room, "alice", "live", "#room")
    });
    reducer
        .conversations
        .get_mut(&room)
        .expect("room")
        .channel
        .as_mut()
        .expect("a channel")
        .history_anchor = None;

    fold(
        &mut reducer,
        history_event(&room, "#room", HistoryKind::Chat, true, vec![replay_line("bob", "older", "old")]),
    );
    assert_eq!(state_of(&reducer, &room).messages[0].body, "older", "messages[0]");
}

#[test]
fn older_page_replay_skips_unread_mark() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let room = reducer.conversation_key(NETWORK_A, "#room");
    fold(&mut reducer, join(NETWORK_A, "#room", "omairc"));
    reducer.mark_selected(&room);

    fold(
        &mut reducer,
        history_event(&room, "#room", HistoryKind::Chat, true, vec![replay_line("bob", "older", "old")]),
    );
    let conversation = state_of(&reducer, &room);
    assert!(conversation.unread_mark.is_none(), "unread mark");
    assert_eq!(conversation.unread, 0, "unread");
}

#[test]
fn older_page_shifts_history_anchor_after_deduped_latest() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let room = reducer.conversation_key(NETWORK_A, "#room");
    fold(&mut reducer, join(NETWORK_A, "#room", "omairc"));
    fold(&mut reducer, MessageEvent {
        msgid: MsgId::new("live"),
        ..message(&room, "alice", "live", "#room")
    });
    let anchor_before = state_of(&reducer, &room).channel.as_ref().expect("a channel").history_anchor.expect("anchor").sequence;

    fold(
        &mut reducer,
        history_event(&room, "#room", HistoryKind::Chat, false, vec![replay_line("alice", "live", "live")]),
    );
    let anchor = state_of(&reducer, &room).channel.as_ref().expect("a channel").history_anchor;
    assert!(anchor.is_some(), "join anchor kept");
    assert_eq!(anchor.expect("anchor").sequence, anchor_before, "anchor after deduped LATEST");

    fold(
        &mut reducer,
        history_event(&room, "#room", HistoryKind::Chat, true, vec![replay_line("bob", "older", "old")]),
    );
    let anchor = state_of(&reducer, &room).channel.as_ref().expect("a channel").history_anchor;
    assert_eq!(anchor.expect("anchor").sequence, anchor_before + 1, "anchor after older page");
}

#[test]
fn history_page_cap_tail_keeps_prepended_head_at_max_messages() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let room = reducer.conversation_key(NETWORK_A, "#room");
    fold(&mut reducer, join(NETWORK_A, "#room", "omairc"));
    {
        let conversation = reducer.conversations.get_mut(&room).expect("room");
        conversation.messages = (0..MAX_MESSAGES)
            .map(|index| Message {
                kind: MessageKind::Message,
                author: "alice".to_owned(),
                body: format!("fill-{index}"),
                time: reducer_timestamp(),
                server_time: None,
                msgid: MsgId::new(format!("fill-{index}")),
                origin: Origin::Live,
                mentioned: false,
                collapsible: false,
                sequence: index as i64 + 1,
                only_visible_to_you: false,
            })
            .collect();
        conversation.message_ids = conversation.messages.iter().map(|m| m.msgid.clone()).collect();
        conversation.next_sequence = MAX_MESSAGES as i64 + 1;
        conversation.history_page_cap_tail = true;
    }

    fold(
        &mut reducer,
        history_event(&room, "#room", HistoryKind::Chat, true, vec![replay_line("bob", "page-head", "page-head")]),
    );
    let conversation = state_of(&reducer, &room);
    assert_eq!(conversation.messages.len(), MAX_MESSAGES, "messages after prepend");
    assert_eq!(conversation.messages[0].body, "page-head", "messages[0]");
}

#[test]
fn chat_history_after_keeps_lines_from_before_rejoin() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let room = reducer.conversation_key(NETWORK_A, "#omarchy");
    fold(&mut reducer, join(NETWORK_A, "#omarchy", "omairc"));
    fold(&mut reducer, message(&room, "alice", "seen", "#omarchy"));
    welcome_omairc(&mut reducer, NETWORK_A);
    fold(&mut reducer, join(NETWORK_A, "#omarchy", "omairc"));
    fold(
        &mut reducer,
        history_event(&room, "#omarchy", HistoryKind::Chat, false, vec![replay_line("alice", "gap", "gap-id")]),
    );

    let conversation = state_of(&reducer, &room);
    assert_eq!(conversation.messages.len(), 4, "messages");
    assert_eq!(conversation.messages[0].body, "omairc joined", "messages[0]");
    assert_eq!(conversation.messages[1].body, "seen", "messages[1]");
    assert_eq!(conversation.messages[2].body, "gap", "messages[2]");
    assert_eq!(conversation.messages[2].origin, Origin::Replay, "gap origin");
    assert_eq!(conversation.messages[3].body, "omairc joined", "messages[3]");
}

#[test]
fn welcome_clears_history_page_cap_tail() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let room = reducer.conversation_key(NETWORK_A, "#room");
    fold(&mut reducer, join(NETWORK_A, "#room", "omairc"));
    reducer.conversations.get_mut(&room).expect("room").history_page_cap_tail = true;
    welcome_omairc(&mut reducer, NETWORK_A);
    assert!(!state_of(&reducer, &room).history_page_cap_tail, "tail cap cleared");
}

#[test]
fn hidden_membership_older_page_keeps_history_page_cap_tail() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let room = reducer.conversation_key(NETWORK_A, "#room");
    fold(&mut reducer, join(NETWORK_A, "#room", "omairc"));
    reducer.set_membership_noise(MembershipNoise::Hidden);
    reducer.conversations.get_mut(&room).expect("room").history_page_cap_tail = true;

    let membership_line = ReplayLine {
        author: String::new(),
        body: "alice joined".to_owned(),
        timestamp: reducer_timestamp(),
        kind: MessageKindTag::Event,
        msgid: MsgId::new("join-page"),
        server_time: None,
        only_visible_to_you: false,
    };
    fold(
        &mut reducer,
        history_event(&room, "#room", HistoryKind::Chat, true, vec![membership_line]),
    );

    let conversation = state_of(&reducer, &room);
    assert!(conversation.history_page_cap_tail, "cap tail stays armed");
    assert_eq!(conversation.messages.len(), 1, "messages");
    assert_eq!(conversation.messages[0].body, "omairc joined", "self join");
    assert_eq!(conversation.people_count(), 1, "people");
}

#[test]
fn history_page_cap_tail_marks_and_clears() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let room = reducer.conversation_key(NETWORK_A, "#room");
    fold(&mut reducer, join(NETWORK_A, "#room", "omairc"));
    assert!(!state_of(&reducer, &room).history_page_cap_tail, "unarmed by default");
    reducer.mark_history_page_cap_tail(&room);
    assert!(state_of(&reducer, &room).history_page_cap_tail, "armed");
    reducer.clear_history_page_cap_tail(&room);
    assert!(!state_of(&reducer, &room).history_page_cap_tail, "cleared");
    reducer.mark_history_page_cap_tail(&reducer.conversation_key(NETWORK_A, "ghost"));
    assert!(reducer.find(&reducer.conversation_key(NETWORK_A, "ghost")).is_none(), "missing stays missing");
}
