//! Port of `tui/internal/irc/viewnotify.go`.
//!
//! Go reports what a reduced event dirtied as one struct of flags. Here the same
//! facts are a list of `ViewNotify` surfaces. An event yields a `Vec`, an empty
//! `Vec` means nothing changed, and the shell maps each surface to its repaint:
//! a transcript append or rebuild, a member row, the sidebar, the typing row.
//!
//! The reducer owns the classification (`ClassifyViewNotify`), because it needs
//! presence and membership facts. This module supplies the surface sets the Go
//! constructors name, the NAMES-syncing filter, and the union the reducer uses
//! to combine them.

/// One surface an event dirtied (`IrcViewNotify`, one flag at a time).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ViewNotify {
    /// The conversation list reloads: rows, unread counts, ordering.
    Conversations,
    /// The transcript reloads or appends for the affected conversation.
    Messages,
    /// The whole member panel reloads (`MemberSurfaceReset`).
    MemberReset,
    /// One member row repaints, named by its normalized nick (`MemberSurfaceRow`).
    MemberRow(String),
    /// The selected conversation's header (topic, people count) reloads.
    Selection,
    /// The typing indicator reloads.
    Typing,
    /// The typing refresh timer restarts, because the event reset the typing
    /// facts that timer watches (`RearmTyping`).
    RearmTyping,
    /// The connection footer, the Status subtitle or the last error changed. Go
    /// reports this through its own status callback; the client queues it beside the
    /// reducer's surfaces so one drain feeds the shell.
    Status,
    /// The session inbox gained, lost or reordered a row.
    Inbox,
}

/// The surfaces a `001` welcome dirties (`ViewNotifyResetAll`).
pub fn reset_all() -> Vec<ViewNotify> {
    vec![
        ViewNotify::Conversations,
        ViewNotify::Messages,
        ViewNotify::MemberReset,
        ViewNotify::Selection,
        ViewNotify::Typing,
        ViewNotify::RearmTyping,
    ]
}

/// A complete NAMES batch: the reset, without rearming typing, since NAMES does
/// not reset typing facts (`ClassifyViewNotify`, `NamesEvent` case).
pub fn names_complete() -> Vec<ViewNotify> {
    reset_all()
        .into_iter()
        .filter(|surface| *surface != ViewNotify::RearmTyping)
        .collect()
}

/// Chat lines: the list, the transcript and typing (`ViewNotifyChat`).
pub fn chat() -> Vec<ViewNotify> {
    vec![
        ViewNotify::Conversations,
        ViewNotify::Messages,
        ViewNotify::Typing,
        ViewNotify::RearmTyping,
    ]
}

/// A topic change: the selected header, not the transcript (`ViewNotifyTopic`).
pub fn topic() -> Vec<ViewNotify> {
    vec![
        ViewNotify::Selection,
        ViewNotify::Typing,
        ViewNotify::RearmTyping,
    ]
}

/// One member row, by normalized nick (`ViewNotifyMemberRow`).
pub fn member_row(normalized_nick: &str) -> Vec<ViewNotify> {
    vec![ViewNotify::MemberRow(normalized_nick.to_owned())]
}

/// The whole member panel (`ViewNotifyMemberReset`). Deliberately broader than
/// the row that changed: self-away is network-scoped and rare, and the view
/// short-circuits an unchanged list.
pub fn member_reset() -> Vec<ViewNotify> {
    vec![ViewNotify::MemberReset]
}

/// Only the typing indicator (`ViewNotifyTypingOnly`).
pub fn typing_only() -> Vec<ViewNotify> {
    vec![ViewNotify::Typing, ViewNotify::RearmTyping]
}

/// Only the transcript (`ViewNotifyTranscript`).
pub fn transcript() -> Vec<ViewNotify> {
    vec![ViewNotify::Messages]
}

/// A membership change: list, transcript, members, selection and typing
/// (`ViewNotifyMembership`).
pub fn membership() -> Vec<ViewNotify> {
    vec![
        ViewNotify::Conversations,
        ViewNotify::Messages,
        ViewNotify::MemberReset,
        ViewNotify::Selection,
        ViewNotify::Typing,
    ]
}

/// Drops the member and selection surfaces while the selected channel's NAMES
/// burst is still syncing. This is the filter `ClassifyViewNotify` applies, and
/// the list, transcript and typing surfaces pass through unchanged.
pub fn without_member_refresh(notifies: Vec<ViewNotify>) -> Vec<ViewNotify> {
    notifies
        .into_iter()
        .filter(|surface| {
            !matches!(
                surface,
                ViewNotify::MemberReset | ViewNotify::MemberRow(_) | ViewNotify::Selection
            )
        })
        .collect()
}

/// Adds `more` to `into`, skipping surfaces already present. Go ORs its flags, so
/// a second surface for the same event never repeats a repaint.
pub fn merge(into: &mut Vec<ViewNotify>, more: Vec<ViewNotify>) {
    for surface in more {
        if !into.contains(&surface) {
            into.push(surface);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn welcome_resets_everything_and_names_leaves_typing_alone() {
        let reset = reset_all();
        assert!(reset.contains(&ViewNotify::RearmTyping));
        assert!(reset.contains(&ViewNotify::MemberReset));

        let names = names_complete();
        assert!(!names.contains(&ViewNotify::RearmTyping));
        assert!(names.contains(&ViewNotify::Selection));
        assert_eq!(names.len(), reset.len() - 1);
    }

    #[test]
    fn chat_and_topic_match_the_go_surfaces() {
        assert_eq!(
            chat(),
            vec![
                ViewNotify::Conversations,
                ViewNotify::Messages,
                ViewNotify::Typing,
                ViewNotify::RearmTyping,
            ]
        );
        assert_eq!(
            topic(),
            vec![
                ViewNotify::Selection,
                ViewNotify::Typing,
                ViewNotify::RearmTyping
            ]
        );
        assert!(!topic().contains(&ViewNotify::Messages));
    }

    #[test]
    fn membership_touches_every_list_surface_but_not_rearm() {
        let surfaces = membership();
        assert!(surfaces.contains(&ViewNotify::Conversations));
        assert!(surfaces.contains(&ViewNotify::Messages));
        assert!(surfaces.contains(&ViewNotify::MemberReset));
        assert!(surfaces.contains(&ViewNotify::Selection));
        assert!(surfaces.contains(&ViewNotify::Typing));
        assert!(!surfaces.contains(&ViewNotify::RearmTyping));
    }

    #[test]
    fn member_row_carries_the_normalized_nick() {
        assert_eq!(
            member_row("alice"),
            vec![ViewNotify::MemberRow("alice".to_owned())]
        );
        assert!(member_reset().contains(&ViewNotify::MemberReset));
        assert_eq!(transcript(), vec![ViewNotify::Messages]);
        assert_eq!(
            typing_only(),
            vec![ViewNotify::Typing, ViewNotify::RearmTyping]
        );
    }

    #[test]
    fn syncing_filter_keeps_list_transcript_and_typing_only() {
        let filtered = without_member_refresh(reset_all());
        assert_eq!(
            filtered,
            vec![
                ViewNotify::Conversations,
                ViewNotify::Messages,
                ViewNotify::Typing,
                ViewNotify::RearmTyping,
            ]
        );
        assert!(without_member_refresh(member_row("bob")).is_empty());
    }

    #[test]
    fn merge_unions_without_repeats() {
        let mut combined = chat();
        merge(&mut combined, topic());
        assert_eq!(
            combined,
            vec![
                ViewNotify::Conversations,
                ViewNotify::Messages,
                ViewNotify::Typing,
                ViewNotify::RearmTyping,
                ViewNotify::Selection,
            ]
        );
        merge(&mut combined, Vec::new());
        assert_eq!(combined.len(), 5);
    }
}
