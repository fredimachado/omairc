# Rust desktop notes

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
  Apply / Remove (Confirm remove … when armed), shortcuts entry); Jump /
  Open channel / Filter channels / Nick /
  Link; About (updates, GitHub link, OK).
- **Other** — members panel (“Members of …”, jump, member rows);
  show/hide members; leave / join / close header actions; field help marks.

When AccessKit lands upstream, reintroduce equivalent names and roles on
the GPUI views that replace these QML surfaces.

## RTL / bidirectional text

**Spike (P1):** when `OMAIRC_RTL_SEED=1` is set with `--demo-server`, the
demo appends one Arabic and one Hebrew PRIVMSG to `#omarchy` after the
parity seed (`demo::wire::rtl_seed_lines`). Off by default. Same GPUI
`StyledText` / cosmic-text path as the composer and topic.

**Outcome: RTL looks OK** (closed for the spike). GPUI 0.2.2 shapes through
cosmic-text, which uses `unicode-bidi` and rustybuzz. On Linux, cosmic-text’s
script fallbacks include `Noto Sans Arabic` and `Noto Sans Hebrew` when the
bundled `iA Writer Mono S` face has no coverage (it does not). No GPUI fork.

Visual proof was not taken on this host: `control-omairc-poc` needs lavapipe
(`vulkan-swrast` / `lvp_icd.json`) for Xvfb, and that ICD was not installed
(no passwordless sudo). Re-check when lavapipe is available:

```sh
OMAIRC_RTL_SEED=1 .cursor/skills/verify-omairc-poc/control-omairc-poc launch --demo-server
.cursor/skills/verify-omairc-poc/control-omairc-poc screenshot --feature rtl-spike --name arabic-hebrew
.cursor/skills/verify-omairc-poc/control-omairc-poc cleanup
```

Evidence (when taken) lives under `test-artifacts/verify-poc/rtl-spike/`.

TODO: if a visual check shows wrong glyph order or missing glyphs despite
Noto fallbacks, track upstream GPUI/cosmic-text; do not fork GPUI.
