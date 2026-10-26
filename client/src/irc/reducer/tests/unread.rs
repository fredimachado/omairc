//! Unread counts, the New-messages boundary, mention and inbox arrivals, mute and
//! nick-merge marks.

use super::*;
use crate::irc::world::MessageKind;

#[test]
fn unread_mentions_respect_selection() {
    let mut reducer = EventReducer::new();
    welcome(&mut reducer, NETWORK_A, "Potato[]");
    let selected = reducer.conversation_key(NETWORK_A, "#selected");
    let background = reducer.conversation_key(NETWORK_A, "#background");
    reducer.mark_selected(&selected);

    fold(&mut reducer, message(&selected, "Alice", "Potato{}: selected", "#selected"));
    fold(&mut reducer, message(&background, "Alice", "hello Potato{}", "#background"));
    fold(&mut reducer, message(&background, "Potato{}", "my own text", "#background"));

    let selected_state = state_of(&reducer, &selected);
    assert_eq!(selected_state.unread, 0, "selected unread");
    assert_eq!(selected_state.mentions, 0, "selected mentions");
    let background_state = state_of(&reducer, &background);
    assert_eq!(background_state.unread, 1, "background unread");
    assert_eq!(background_state.mentions, 1, "background mentions");

    reducer.mark_selected(&background);
    let background_state = state_of(&reducer, &background);
    assert_eq!(background_state.unread, 0, "background unread after select");
    assert_eq!(background_state.mentions, 0, "background mentions after select");
}

#[test]
fn window_inactive_marks_selected_chat_unread() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let selected = reducer.conversation_key(NETWORK_A, "#selected");
    let background = reducer.conversation_key(NETWORK_A, "#background");
    reducer.mark_selected(&selected);

    fold(&mut reducer, message(&selected, "Alice", "while-focused", "#selected"));
    let state = state_of(&reducer, &selected);
    assert_eq!(state.unread, 0, "focused unread");
    assert!(state.unread_mark.is_none(), "focused unreadMark");

    reducer.set_window_active(false);
    fold(&mut reducer, message(&selected, "Bob", "first-unfocused", "#selected"));
    fold(&mut reducer, message(&selected, "Bob", "second-unfocused", "#selected"));
    let state = state_of(&reducer, &selected);
    assert_eq!(state.unread, 2, "unfocused unread");
    assert_eq!(state.unread_mark, Some(state.messages[1].sequence), "unfocused unreadMark");

    reducer.set_window_active(true);
    fold(&mut reducer, message(&selected, "Bob", "while-focused-again", "#selected"));
    let state = state_of(&reducer, &selected);
    assert_eq!(state.unread, 2, "refocused unread");
    assert!(state.unread_mark.is_some(), "refocused unreadMark");

    // A background conversation still plants unread regardless of focus.
    fold(&mut reducer, message(&background, "Alice", "background", "#background"));
    let state = state_of(&reducer, &background);
    assert_eq!(state.unread, 1, "background unread");
    assert!(state.unread_mark.is_some(), "background unreadMark");
}

#[test]
fn mark_read_consumes_unread_but_keeps_mark() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let selected = reducer.conversation_key(NETWORK_A, "#selected");
    reducer.mark_selected(&selected);
    reducer.set_window_active(false);

    fold(&mut reducer, message(&selected, "Alice", "omairc: away ping", "#selected"));
    {
        let state = state_of(&reducer, &selected);
        assert_eq!(state.unread, 1, "unread");
        assert_eq!(state.mentions, 1, "mentions");
        assert!(state.unread_mark.is_some(), "unreadMark");
    }
    assert!(reducer.take_mention_arrival().is_some(), "mention arrival is missing");
    assert!(
        reducer.take_inbox_arrival().is_none(),
        "selected conversation must not plant an inbox arrival"
    );

    assert!(reducer.mark_read(&selected), "markRead changed");
    let state = state_of(&reducer, &selected);
    assert_eq!(state.unread, 0, "unread after read");
    assert_eq!(state.mentions, 0, "mentions after read");
    assert!(state.unread_mark.is_some(), "mark kept");

    // A later selection (a real switch away and back) clears the mark.
    let other = reducer.conversation_key(NETWORK_A, "#other");
    reducer.mark_selected(&other);
    reducer.mark_selected(&selected);
    assert!(state_of(&reducer, &selected).unread_mark.is_none(), "mark cleared on reselect");
}

#[test]
fn mark_all_read_clears_every_conversation() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let selected = reducer.conversation_key(NETWORK_A, "#selected");
    let background = reducer.conversation_key(NETWORK_A, "#background");
    reducer.mark_selected(&selected);
    reducer.set_window_active(false);

    fold(&mut reducer, message(&selected, "Alice", "omairc: away ping", "#selected"));
    fold(&mut reducer, message(&background, "Bob", "ordinary", "#background"));
    {
        let selected_state = state_of(&reducer, &selected);
        assert_eq!(selected_state.unread, 1, "selected unread");
        assert_eq!(selected_state.mentions, 1, "selected mentions");
        assert!(selected_state.unread_mark.is_some(), "selected mark");
        let background_state = state_of(&reducer, &background);
        assert_eq!(background_state.unread, 1, "background unread");
        assert_eq!(background_state.mentions, 0, "background mentions");
        assert!(background_state.unread_mark.is_some(), "background mark");
    }

    assert!(reducer.mark_all_read(), "markAllRead changed");
    let selected_state = state_of(&reducer, &selected);
    assert_eq!((selected_state.unread, selected_state.mentions), (0, 0));
    assert!(selected_state.unread_mark.is_some(), "selected mark kept");
    let background_state = state_of(&reducer, &background);
    assert_eq!((background_state.unread, background_state.mentions), (0, 0));
    assert!(background_state.unread_mark.is_some(), "background mark kept");
    assert!(!reducer.mark_all_read(), "second markAllRead");
}

#[test]
fn mention_arrival_survives_selection() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let selected = reducer.conversation_key(NETWORK_A, "#selected");
    reducer.mark_selected(&selected);

    fold(&mut reducer, message(&selected, "Alice", "omairc: ping", "#selected"));
    let mention = reducer.take_mention_arrival().expect("mention present");
    assert_eq!(mention.author, "Alice");
    assert_eq!(mention.body, "omairc: ping");
    assert_eq!(state_of(&reducer, &selected).mentions, 0, "mentions");
    assert!(reducer.take_mention_arrival().is_none(), "mention arrival must be consumed once");

    fold(&mut reducer, message(&selected, "Alice", "no nick here", "#selected"));
    assert!(reducer.take_mention_arrival().is_none(), "plain chat must not mention");

    fold(&mut reducer, message(&selected, "omairc", "omairc: self", "#selected"));
    assert!(reducer.take_mention_arrival().is_none(), "self chat must not mention");
}

#[test]
fn mention_arrival_on_direct_message() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let dm = reducer.conversation_key(NETWORK_A, "Alice");
    let room = reducer.conversation_key(NETWORK_A, "#room");

    fold(&mut reducer, message(&dm, "Alice", "hello", "Alice"));
    let unselected = reducer.take_mention_arrival().expect("unselected mention");
    assert_eq!(unselected.author, "Alice");
    assert_eq!(unselected.body, "hello");
    assert_eq!(state_of(&reducer, &dm).mentions, 0, "dm mentions");
    assert_eq!(state_of(&reducer, &dm).unread, 1, "dm unread");

    reducer.mark_selected(&dm);
    fold(&mut reducer, message(&dm, "Alice", "hello", "Alice"));
    assert!(
        reducer.take_mention_arrival().is_some(),
        "selected direct message must still plant a mention arrival"
    );
    assert_eq!(state_of(&reducer, &dm).mentions, 0, "selected dm mentions");
    assert_eq!(state_of(&reducer, &dm).unread, 0, "selected dm unread");

    fold(&mut reducer, message(&room, "Alice", "hello", "#room"));
    assert!(
        reducer.take_mention_arrival().is_none(),
        "channel chat without a nick hit must not mention"
    );

    reducer.clear_selection();
    fold(&mut reducer, message(&dm, "Alice", "hey omairc", "Alice"));
    assert!(reducer.take_mention_arrival().is_some(), "nick mention must plant an arrival");
    assert_eq!(state_of(&reducer, &dm).mentions, 1, "dm mentions after nick mention");

    fold(&mut reducer, action(&dm, "Alice", "waves", "Alice"));
    assert!(reducer.take_mention_arrival().is_some(), "action must plant a direct arrival");

    fold(&mut reducer, notice(&dm, "Alice", "hello", "Alice"));
    assert!(reducer.take_mention_arrival().is_none(), "notice must not mention");

    fold(&mut reducer, message(&dm, "omairc", "hello", "Alice"));
    assert!(reducer.take_mention_arrival().is_none(), "self direct message must not mention");
}

#[test]
fn mention_arrival_carries_network_target_and_msgid() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let room = reducer.conversation_key(NETWORK_A, "#omarchy");
    fold(
        &mut reducer,
        MessageEvent {
            msgid: MsgId::new("mid-1"),
            ..message(&room, "Alice", "omairc: ping", "#omarchy")
        },
    );
    let mention = reducer.take_mention_arrival().expect("mention present");
    assert_eq!(mention.author, "Alice");
    assert_eq!(mention.body, "omairc: ping");
    assert_eq!(mention.network_id, NETWORK_A);
    assert_eq!(mention.target, "#omarchy");
    assert_eq!(mention.msgid.value, "mid-1");

    let dm = reducer.conversation_key(NETWORK_A, "Alice");
    fold(
        &mut reducer,
        MessageEvent {
            msgid: MsgId::new("dm-7"),
            ..message(&dm, "Alice", "hello", "Alice")
        },
    );
    let direct = reducer.take_mention_arrival().expect("direct present");
    assert_eq!(direct.author, "Alice");
    assert_eq!(direct.body, "hello");
    assert_eq!(direct.network_id, NETWORK_A);
    assert_eq!(direct.target, "Alice");
    assert_eq!(direct.msgid.value, "dm-7");
}

#[test]
fn muted_chat_does_not_mention() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let room = reducer.conversation_key(NETWORK_A, "#room");
    reducer.set_muted(&room, true);
    fold(&mut reducer, message(&room, "Alice", "omairc: ping", "#room"));

    assert!(reducer.take_mention_arrival().is_none(), "muted chat must not mention");
    {
        let muted = state_of(&reducer, &room);
        assert!(muted.muted, "muted");
        assert_eq!(muted.mentions, 0, "mentions");
        assert_eq!(muted.unread, 1, "unread");
        assert_eq!(muted.messages.last().expect("a row").body, "omairc: ping");
    }

    reducer.mark_selected(&room);
    assert!(state_of(&reducer, &room).muted, "muted after select");
    assert_eq!(state_of(&reducer, &room).unread, 0, "unread after select");

    reducer.clear_selection();
    reducer.set_muted(&room, false);
    assert!(!state_of(&reducer, &room).muted, "unmuted");
    fold(&mut reducer, message(&room, "Alice", "omairc: back", "#room"));
    assert!(reducer.take_mention_arrival().is_some(), "unmuted chat must mention again");
    assert_eq!(state_of(&reducer, &room).mentions, 1, "mentions after unmute");
}

#[test]
fn highlight_word_mentions_like_nick() {
    let mut reducer = EventReducer::new();
    welcome(&mut reducer, NETWORK_A, "fred");
    reducer.set_highlight_words(NETWORK_A, &["omairc".to_owned()]);
    let selected = reducer.conversation_key(NETWORK_A, "#selected");
    let background = reducer.conversation_key(NETWORK_A, "#background");
    reducer.mark_selected(&selected);

    fold(&mut reducer, message(&background, "Alice", "please review omairc", "#background"));
    assert_eq!(state_of(&reducer, &background).unread, 1, "unread");
    assert_eq!(state_of(&reducer, &background).mentions, 1, "mentions");
    let hit = reducer.take_mention_arrival().expect("highlight arrival");
    assert_eq!(hit.author, "Alice");
    assert_eq!(hit.body, "please review omairc");

    fold(&mut reducer, message(&background, "Alice", "please review omaircd", "#background"));
    assert_eq!(state_of(&reducer, &background).unread, 2, "unread after word-continuation");
    assert_eq!(state_of(&reducer, &background).mentions, 1, "mentions after word-continuation");
    assert!(reducer.take_mention_arrival().is_none(), "a word continuation must not mention");

    reducer.set_highlight_words(NETWORK_A, &["deploy".to_owned()]);
    fold(&mut reducer, message(&background, "Alice", "please review deploy", "#background"));
    assert_eq!(state_of(&reducer, &background).mentions, 2, "mentions after replacement word");
    assert!(reducer.take_mention_arrival().is_some(), "the replacement word must mention");

    fold(&mut reducer, message(&background, "Alice", "please review deployment", "#background"));
    assert_eq!(state_of(&reducer, &background).mentions, 2, "mentions after deployment");
    assert!(reducer.take_mention_arrival().is_none(), "deployment must not mention for the word deploy");

    reducer.set_highlight_words(NETWORK_A, &[]);
    fold(&mut reducer, message(&background, "Alice", "fred: still a nick", "#background"));
    assert_eq!(state_of(&reducer, &background).mentions, 3, "mentions after clearing words");
    assert!(reducer.take_mention_arrival().is_some(), "the nick must still mention");
}

#[test]
fn muted_chat_still_plants_unread_mark() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    let room = reducer.conversation_key(NETWORK_A, "#room");
    reducer.set_muted(&room, true);
    fold(&mut reducer, message(&room, "Alice", "omairc: ping", "#room"));

    let muted = state_of(&reducer, &room);
    assert!(muted.muted, "muted");
    assert_eq!(muted.mentions, 0, "mentions");
    assert_eq!(muted.unread, 1, "unread");
    assert_eq!(muted.unread_mark, Some(muted.messages.last().expect("a row").sequence));
}

#[test]
fn nick_merge_adopts_or_keeps_unread_mark() {
    // adopt: the moved conversation's boundary moves onto the merged row.
    {
        let mut reducer = EventReducer::new();
        welcome_omairc(&mut reducer, NETWORK_A);
        let alice = reducer.conversation_key(NETWORK_A, "Alice");
        let alicia = reducer.conversation_key(NETWORK_A, "Alicia");
        reducer.mark_selected(&alicia);
        fold(&mut reducer, message(&alicia, "Alicia", "dest-only", "Alicia"));
        assert!(state_of(&reducer, &alicia).unread_mark.is_none(), "dest unreadMark");
        reducer.clear_selection();
        fold(&mut reducer, message(&alice, "Alice", "moved-chat", "Alice"));
        assert!(state_of(&reducer, &alice).unread_mark.is_some(), "moved unreadMark");

        fold(&mut reducer, nick_change(NETWORK_A, "Alice", "Alicia"));
        let merged = state_of(&reducer, &alicia);
        let dest_only = merged.messages.iter().find(|m| m.body == "dest-only").expect("dest-only");
        let moved_chat = merged.messages.iter().find(|m| m.body == "moved-chat").expect("moved-chat");
        let mark = merged.unread_mark.expect("merged unreadMark");
        assert_eq!(mark, moved_chat.sequence, "adopted mark");
        assert_ne!(mark, dest_only.sequence, "merged mark must not point at the destination-only row");
    }

    // keep: the destination already had a boundary, so it stays.
    {
        let mut reducer = EventReducer::new();
        welcome_omairc(&mut reducer, NETWORK_A);
        let from_alice = reducer.conversation_key(NETWORK_A, "Alice");
        let from_alicia = reducer.conversation_key(NETWORK_A, "Alicia");
        fold(&mut reducer, message(&from_alice, "Alice", "from alice", "Alice"));
        reducer.mark_selected(&from_alicia);
        fold(&mut reducer, message(&from_alicia, "Alicia", "dest-prefix", "Alicia"));
        reducer.clear_selection();
        fold(&mut reducer, message(&from_alicia, "Alicia", "from alicia", "Alicia"));
        assert!(state_of(&reducer, &from_alice).unread_mark.is_some(), "source unreadMark");
        assert!(state_of(&reducer, &from_alicia).unread_mark.is_some(), "dest unreadMark");
        let dest_mark = state_of(&reducer, &from_alicia).unread_mark.expect("dest mark");
        assert_ne!(dest_mark, state_of(&reducer, &from_alice).unread_mark.expect("source mark"));

        fold(&mut reducer, nick_change(NETWORK_A, "Alice", "Alicia"));
        let merged = state_of(&reducer, &from_alicia);
        assert_eq!(merged.unread_mark, Some(dest_mark), "kept mark");
        let dest_unread = merged.messages.iter().find(|m| m.body == "from alicia").expect("row");
        let moved_unread = merged.messages.iter().find(|m| m.body == "from alice").expect("row");
        assert_eq!(merged.unread_mark, Some(dest_unread.sequence), "kept mark points at destination row");
        assert_ne!(dest_unread.sequence, moved_unread.sequence, "the two rows must have distinct sequences");
    }
}

#[test]
fn transcript_mention_flag_follows_the_current_nick() {
    // The row's flag is read against the nick as it is now, as Go's
    // IsTranscriptHighlight reads it at view time.
    let mut reducer = EventReducer::new();
    welcome(&mut reducer, NETWORK_A, "omairc");
    let room = reducer.conversation_key(NETWORK_A, "#room");
    fold(&mut reducer, message(&room, "Alice", "omairc: ping", "#room"));
    assert!(state_of(&reducer, &room).messages[0].mentioned, "named the nick at arrival");

    welcome(&mut reducer, NETWORK_A, "fred");
    assert!(
        !state_of(&reducer, &room).messages[0].mentioned,
        "a row that named the old nick stops highlighting"
    );
    assert!(reducer.is_transcript_highlight(NETWORK_A, "Alice", MessageKind::Message, "hello fred"));
    assert!(!reducer.is_transcript_highlight(NETWORK_A, "fred", MessageKind::Message, "hello fred"), "own line");
}
