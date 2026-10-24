# PoC / Rust desktop notes

Notes for the GPUI client cutover. Keep this short; product history still
lives in root `CHANGELOG.md` (Phase 7 will copy accepted regressions there).

## Known regressions

### No screen-reader tree (AccessKit)

GPUI does not expose an AccessKit tree yet, so the Rust desktop client has
no screen-reader names or roles. Qt Omairc sets `Accessible.name` (and
roles where needed) across the shell; those surfaces are lost until
upstream GPUI gains AccessKit.

**Decision:** document and track upstream GPUI. Do not fork GPUI for a11y.

Surfaces to restore later (from `src/qml/`):

- **Sidebar** — conversation rows (`conversationName`, typing description);
  network expand/collapse and “Edit connection”; Inbox; About / “Restart
  to update”.
- **Transcript** — “Messages in …” list; Status console; nick hits; “only
  visible to you”; typing dots; unseen jump (“Jump to first new message” /
  “Scroll to latest messages”).
- **Composer** — “Message composer” / “Find”; “Send message”; “Insert a
  file”.
- **Sheets** — Connect (network rail, Add network, Connection/Preferences
  tabs, preference toggles, forget-password actions, Discard / Disconnect /
  Apply, shortcuts entry); Jump / Open channel / Filter channels / Nick /
  Link; About (updates, GitHub link, OK).
- **Other** — members panel (“Members of …”, jump, member rows);
  show/hide members; leave / join / close header actions; field help marks.

When AccessKit lands upstream, reintroduce equivalent names and roles on
the GPUI views that replace these QML surfaces.
