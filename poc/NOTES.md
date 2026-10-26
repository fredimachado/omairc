# Rust desktop notes

Notes for the GPUI client cutover. Keep this short; product history still
lives in root `CHANGELOG.md` (Phase 7 / M9 will copy accepted regressions
there).

## Dark mode / portal color-scheme (Linux)

GPUI 0.2.2 already reads and watches
`org.freedesktop.appearance` / `color-scheme` through ashpd
(`gpui::platform::linux::xdg_desktop_portal`) on both X11 and Wayland, and
pushes `WindowAppearance` into every window. Omairc follows that with
`cx.window_appearance()` at open and `observe_window_appearance` in
`poc/src/app.rs` (see `poc/src/desktop/systemtheme.rs`). No second portal
subscription next to `desktop/textscale.rs` - that would race GPUI.

Portal text scale stays in `desktop/textscale.rs` (Qt parity);
color-scheme does not.

**Not proven here:** a live Hyprland / session-bus flip (this cutover host
is Windows; CI Xvfb refuses the session bus). When checking on Omarchy,
toggle the portal color-scheme and confirm the base palette flips without
restart when `colors.toml` is absent.

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

Lavapipe is available in CI’s non-blocking `verify-poc` job (M7). Optional
visual grab when you want a PNG under `test-artifacts/verify-poc/rtl-spike/`:

```sh
OMAIRC_RTL_SEED=1 .cursor/skills/verify-omairc-poc/control-omairc-poc launch --demo-server
.cursor/skills/verify-omairc-poc/control-omairc-poc screenshot --feature rtl-spike --name arabic-hebrew
.cursor/skills/verify-omairc-poc/control-omairc-poc cleanup
```

No upstream GPUI/cosmic-text issue filed: the spike did not show wrong glyph
order or missing glyphs when Noto fallbacks are installed. Re-open only if a
later visual check fails despite those fonts.

## Cutover changelog draft (M9)

Copy into root `CHANGELOG.md` under the cutover release (not a ship note
until M9 lands). Draft text:

### Changed

- Desktop client is the Rust GPUI binary (`product` feature). Arch
  `PKGBUILD`, `bin/test-install`, and `bin/test-desktop` build and drive
  that binary.

### Known regressions

- Screen readers: GPUI has no AccessKit tree yet, so Accessible names and
  roles from the Qt shell are gone until upstream GPUI gains AccessKit.
  Surfaces to restore are listed in `poc/NOTES.md`.
- Other known cutover gaps (KWallet passwords, Wayland minimize) stay in
  the M9 release notes checklist.
