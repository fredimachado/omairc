# omairc-tui contributor notes

These rules extend the repository root `AGENTS.md` for the Go terminal client.
When the two disagree about the shared contract, the root file wins.

## Module

- `tui/` is a standalone Go module: `github.com/fredimachado/omairc/tui`.
  The repo root is not a Go module, so every `go` invocation must run with
  `tui/` as the working directory or `go` walks up and fails.
- The `go` directive is `go 1.25.0`, the patch-level minimum that
  `github.com/charmbracelet/ultraviolet` (pulled in by
  `charm.land/bubbletea/v2` v2.0.9) declares. Do not lower it.
- The module path is domain-qualified so Phase 12's `go install` for Linux
  works without a rename.
- Layout: `cmd/omairc-tui/` (flags and `main`) and
  `cmd/control-omairc-tui/` (its CLI), `internal/irc/` (portable core,
  mirrors `src/irc/`), `internal/session/` (transport, SCRAM, the session
  state machine, the session manager, and the clock seam),
  `internal/controller/` (the Go `IrcController` core and its view
  snapshots),   `internal/connection/` (the `NetworkProfile` + `Connection` port of
  `IrcNetworkProfile`/`IrcConnection`; persistence and credentials now persist
  through `internal/storage`), `internal/storage/` (the filesystem and
  keychain leaf: a QSettings-compatible INI store, the profile store, the
  open-direct and playback-time stores, the on-disk conversation log, and the
  credential store), `internal/demo/` (the `IrcDemoServer` seed
  harness behind `--demo-server`, mirrors `src/irc/ircdemoserver.cpp`),
  `internal/ui/`
  (Bubble Tea shell, mirrors `src/qml/` plus `OmaircWindow.qml`),
  `internal/theme/` (the Omarchy palette: `colors.toml` parsing, the derived
  structural colors plus the fixed semantic/nick block, `Fallback`, and the
  live mtime-poll `Watcher`; lipgloss-free, wrapped by `internal/ui` at the
  rendering edge),
  `internal/avatar/` (peer-avatar fetch, bounded decode, cache, and
  half-block raster), `internal/openurl/` (the OS URL-handler seam),
  `internal/gate/` (the PTY parity driver: VT grid, OSC title capture, key
  writer, screenshot), `internal/version/` (injected build version), and
  `bin/` (gate scripts).
- `tui/bin/omairc-tui` is a build artifact and is gitignored.

## Package boundaries

- `internal/irc` is the portable core. It never imports `net`, `os`, or
  `crypto/tls`; sockets, the filesystem, and TLS live in `internal/session`.
  `bin/check-conventions` gates this mechanically (test files may read
  testdata with `os`). One documented exception: `internal/irc/avatarurl.go`
  ports `src/irc/ircavatarurl.cpp` (QUrl + QHostAddress), so it may import
  `net/url` and `net/netip` — pure URL and IP-literal parsing, not a socket.
  The gate lists that file explicitly; do not widen the exemption.
- `internal/avatar`, `internal/openurl`, and `internal/theme` are integration
  packages like `internal/notify`. `internal/avatar` owns `net/http`, the
  DNS-pinned (SSRF-safe) fetch, bounded png/jpeg/gif decode, the 64-entry
  cache, and the half-block raster; it never renders lipgloss.
  `internal/openurl` is the `xdg-open` seam on Linux with no-op stubs
  elsewhere. `internal/theme` reads Omarchy's `colors.toml`, derives the
  structural palette, keeps the fixed semantic and nick colors, and polls the
  theme tree through a `session.Clock`; it stays lipgloss-free and imports no
  view code. `internal/ui` may import all three; it still must not import
  `internal/irc` — that is the only import the view/driver gate forbids.
- Visual divergence from Qt is a deliberate policy, not drift. Behavior,
  chords, the window title, and the controller contract stay identical; only
  presentation may diverge. `internal/theme` encodes the split: the semantic
  colors (`good` / `warning` / `danger` / `mention` / `unread`) and the nick
  palette are fixed and shared with `OmaircStyle.qml`, while the surfaces,
  borders, and secondary text are derived from the live theme's
  background/foreground/accent. `Fallback()` pins the dark Tokyo-Night palette
  for a missing or unreadable `colors.toml`, so the TUI keeps its exact old
  literals when no Omarchy theme is installed. `internal/ui.buildStyles` is
  the only place lipgloss sees `theme.Colors`.
- The controller and connection callbacks fire both from `Update` and from
  session goroutines. Never call `p.Send` synchronously from a callback: it
  blocks on the program message channel and deadlocks the event loop the
  moment a navigation chord republishes a snapshot. Coalesce wake-ups onto one
  dispatcher goroutine, as `cmd/omairc-tui/main.go` does. The live theme
  watcher follows the same rule: `SetThemeWatcher` adopts the current palette
  and the shell arms one blocking `Changes()` read as a `tea.Cmd`, so the
  runtime — not a callback — delivers each `ThemeChangedMsg`.
- `internal/session` is the only product package that opens sockets or TLS;
  `internal/gate` is the dev-only PTY driver (a Unix socket plus `os/exec`),
  not product code. All timers, reconnects, the ping watchdog, the
  labeled-response timeout, and typing pacing go through `internal/session`'s
  `Clock`/`Timer` seam, so tests drive them manually with `FakeClock` and
  `LoopbackTransport`.
- `internal/controller` holds the state the shell calls: sessions, the
  reducer, selection, sidebar order, sidebar network collapse/reorder, send,
  and the Status ring buffer. `IsNetworkCollapsed`, `SetNetworkCollapsed`,
  `SetAllNetworksCollapsed`, and `MoveNetwork` live here, mirroring the Qt
  `IrcConnection` collapse home; `internal/connection.MoveNetwork` is the
  Connect-sheet roster reorder only. It reads time only from an injected
  `session.Clock`. Phase 7 lands the slash catalog: the command dispatcher
  (`commanddispatcher.go`), the reply router (`replyrouter.go`), the monitor
  coordinator (`monitorcoordinator.go`), the autoaway runtime
  (`autoawayruntime.go`), the channel-list request/model (`channellist.go`),
  and the in-memory ignore/mute/monitor/highlight/avatar stores
  (`stores.go`), all wired through the host adapters in `wiring.go`. Phase 11
  lands the bouncer playback coordinator (`playback.go`), the disk-backed
  `/pref` toggles, the open-direct restore after ISUPPORT, and the on-disk
  transcript log wiring; the session-inbox store lives on the controller. The
  controller owns `networkOrder`/`collapsedNetworks` persistence because it
  owns that state in the TUI (see `AGENTS.md` at the repo root). `seams.go`
  now names only the transcript-log alias.
- `internal/storage` is a leaf below `internal/controller` and
  `internal/connection`: filesystem and keychain integration. It imports `os`,
  `internal/irc`, and `github.com/godbus/dbus/v5`, and never imports
  `internal/controller`, `internal/connection`, `internal/session`, or
  `internal/ui`, so no import cycle forms. It owns the QSettings-compatible
  INI store (`settings.go`, byte-compatible with the Qt client's
  `omairc.conf`), the XDG paths (`paths*.go`, build-tagged per OS), the
  profile store, the open-direct and playback-time stores, the on-disk
  conversation log, and the credential store. A malformed or unreadable ini is
  never overwritten from cache (`WriteBlocked`/`ProbeSettingsIni`).
- `internal/connection` owns the Connect sheet's profile model and the
  draft/selection/apply/disconnect surface (`NetworkProfile`, `Connection`).
  It may import `internal/irc`, `internal/controller`, `internal/session`
  (the transport factory), and `internal/storage` (the profile and credential
  stores), and must not import `net`, `os`, or `crypto/tls`; sockets stay in
  `internal/session`. Persistence goes through the injected stores; `New`
  starts in memory until the shell installs them.
- The controller and connection callbacks fire both from `Update` and from
  session goroutines. Never call `p.Send` synchronously from a callback: it
  blocks on the program message channel and deadlocks the event loop the
  moment a navigation chord republishes a snapshot. Coalesce wake-ups onto one
  dispatcher goroutine, as `cmd/omairc-tui/main.go` does.
- Phase 2 adds **zero external Go dependencies**: everything is stdlib
  (`crypto/pbkdf2` ships in Go 1.24+ and the module is `go 1.25`). Do not add
  a `require` until the phase that first imports it.

## Pinned stack

Pin these exact versions. A dependency is added to `go.mod` at its pinned
version the moment its package is first imported — never earlier, because
`go mod tidy` strips unused requires:

- `charm.land/bubbletea/v2` v2.0.9 — imported in Phase 4.
- `charm.land/lipgloss/v2` v2.0.3 — imported in Phase 4.
- `charm.land/bubbles/v2` v2.1.0 — imported in Phase 4.
- `github.com/creack/pty` v1.1.24 — imported in Phase 4 by `internal/gate`
  (Unix PTY, with Windows/ConPTY support).
- `github.com/ergochat/irc-go` (`ircmsg`, `ircreader`, `ircfmt`, `ircutils`)
  — still not imported. The wire layer stays a hand-port of `src/irc/` so the
  byte-for-byte behavior and the mirrored test matrices stay the contract.
  Phase 10 hand-ported the emphasis half of `IrcTextFormatter` as well
  (`internal/irc/textformat.go`, consistent with `plaintext.go`), so `ircfmt`
  was not needed; a later `ircreader` import is the remaining candidate.

Update this list and the import that pulls the dependency in the same change.
Do not pre-add requires to `go.mod`; tidy will revert them.

## Target platforms

Omarchy/Linux is the only target until those platforms land. Keep the core
platform-neutral and defer OS specifics behind `//go:build` files in
`internal/`, never inline in `internal/irc`:

- macOS: extend the `tui.yml` matrix with `macos-latest`, bundle the app.
- Windows: extend the matrix with `windows-latest`, emit
  `omairc-tui$(go env GOEXE)` from `tui/bin/build`, and drive Windows Terminal
  only (no legacy conhost).
- Platform behavior (notifications, clipboard, image raster, storage paths,
  keychain) lives behind build tags in `internal/`, mirroring how `src/irc/`
  stays portable while `Backend` owns desktop integration. D-Bus notifications
  are implemented in `internal/notify/notify_linux.go`; osascript and Windows
  Toast remain no-op stubs behind the same `Notifier`. `internal/storage` ships
  Linux XDG paths (`paths_linux.go`) and a Linux Secret Service credential
  store (`secretservice.go`) now; macOS `GenericConfigLocation`/Keychain and
  Windows `%LOCALAPPDATA%`/Credential Manager slot into the build-tagged
  `paths_other.go`/`secretservice_other.go` seams later without touching
  `internal/irc`, `internal/controller`, or `internal/ui`.

Nothing in Phase 0 hardcodes a Linux-only assumption into `internal/irc`.

## Shared contract (must not drift from Qt)

The four invariants are ported verbatim, not re-derived:

- `IrcConversationCause` insertion rules from `ircConversationCauseInserts`.
  QuietSend and InboundSelf never invent; UserOpen invents DMs; ChannelState
  invents channels; InboundOther invents channels and non-service DMs;
  Restore invents non-service DMs after ISUPPORT. Do not invent a second
  policy.
- `IrcServerFeatures` parses ISUPPORT. Never hardcode CHANTYPES, CHANMODES,
  or PREFIX. Case mapping, `parseNamesToken`, `prefixChanges`, `rankPriority`,
  and `memberLabel` must match the C++ core.
- `IrcSecretPolicy` redaction for Status and error previews fails closed.
  Port `redactWireLine` / `redactPreviewLine` / `redactMessage` exactly; do
  not enumerate one more well-formed bypass.
- `orderedMembers` sorts by PREFIX rank then nick, in the core. The view
  never sorts, and the member panel and the CLI names order must match.
- Title parity: `internal/irc.RosterDisplayName` mirrors
  `IrcConnection::rosterDisplayName`, and `internal/ui/title.go`'s `Title`
  mirrors `OmaircWindow.qml`'s `conversationTitleText` / `statusTitleText`.
  The terminal title is emitted as OSC 2 from the Bubble Tea v2
  `View.WindowTitle` field.

Phase 2 implements all four: `ConversationCauseInserts` and `TargetLooksLikeService`
in `internal/irc/conversation.go`, `orderedMembers`/`OrderedMembers` in the
reducer, and `RedactWireLine`/`RedactPreviewLine`/`RedactMessage`/`AllowsTranscript`
in `internal/irc/secretpolicy.go` (redaction is applied at Status
classification time, so the session may emit raw lines but a `StatusEntry`
never stores an unredacted secret). The session reuses Phase 1's
`CapabilityNegotiation` for CAP LS/REQ/ACK/NAK and SASL; do not duplicate
capability policy in the session or the controller.

`internal/ui` mirrors `src/qml/` plus `OmaircWindow.qml` as a thin shell over
the Go controller. Keep the window-growth discipline: extract file-based
components instead of growing one god-object, and keep protocol policy out of
the view.

## Feature map

`.cursor/skills/verify-omairc/features/*.md` is the single source of truth for
behavior. `control-omairc` (Qt driver) and `control-omairc-tui` (driver over a
PTY) play the same `desktop-recipe` fences. Do not compile the map into the
binary or add a command that dumps it.

## Parity gate

`tui/bin/control-omairc-tui` (wrapped by
`.cursor/skills/verify-omairc-tui/control-omairc-tui`) replays the same
`desktop-recipe` fences as `control-omairc`, but over a fixed-size PTY instead
of Xvfb and xdotool. It reconstructs a text grid from the VT stream, reads the
OSC 2 title, writes key bytes, renders PNG evidence, and rejects pixel-click
verbs: the TUI is keyboard-first and has no pointer path. Evidence goes under
`test-artifacts/verify-tui/`. The internal skill lives at
`.cursor/skills/verify-omairc-tui/`. Phase 5 adds the `jump`, `status`,
`connect`, and `compare` verbs (the fence's `test-artifacts/verify/` paths are
remapped to `test-artifacts/verify-tui/`), plus the Kitty CSI-u bytes for
`Ctrl+\``, `Ctrl+,`, `Ctrl+Enter`, `Ctrl+Tab`, and `Ctrl+Shift+Delete`, whose
legacy control bytes collide with other keys. Phase 6 adds the `nick-jump`
verb and the CSI-u bytes for `ctrl+shift+s`, `ctrl+shift+p`, `ctrl+shift+k`,
`ctrl+shift+a`, `ctrl+shift+o`, and `ctrl+shift+m`, the xterm modifier forms
`alt+shift+Left`/`Right`/`Up`/`Down` and `ctrl+alt+shift+Left`/`Right`, and
`ctrl+home`, `ctrl+end`, `shift+Page_Up`, and `shift+Page_Down`. `ctrl+slash`
is sent as a CSI-u sequence too, because the decoder does not map the legacy
`0x1F` byte to `ctrl+/`. `key`, `type`, and `send` block until the child has
repainted and the PTY has gone quiet, so a following `screenshot` or `compare`
is deterministic instead of racing the redraw. Phase 7 adds the `composer`
verb (a no-op focus, so the shared `slash-complete` fence runs) and the
`slash-commands` / `slash-complete` recipes; the slash catalog, the completer,
and the `/list` overlay live in `internal/controller` and `internal/ui`.
Phase 8 adds the `toggle-members` verb and the `toggle-members`,
`member-presence`, `typing`, and `open-direct-message` recipes; the member
chrome (`ONLINE - N` heading, presence dots, away dimming, status lines, bot
mark, typing ellipsis) and the DM transcript typing footer live in
`internal/ui`, over presence and typing state in `internal/controller`.
Phase 9 adds the session-inbox store, the `internal/notify` desktop notifier
(D-Bus on Linux, no-op stubs elsewhere), the `Ctrl+Shift+A` sheet, the identity
footer badge, and terminal-focus gating via `tea.View.ReportFocus`.
Phase 10 adds the hand-ported emphasis renderer (`internal/irc/textformat.go`,
wired through `internal/ui/transcript.go`), the URL allowlist and the real
`Ctrl+Shift+O` link-sheet source (`internal/ui/urllinks.go`), the
`internal/openurl` seam behind `openAllowedUrl`, and the sidebar
direct-message avatar glyph: an identicon by default, or a half-block
truecolor raster when `internal/avatar` has the peer image cached and the
terminal advertises 24-bit color.
Phase 11 adds `internal/storage` (the QSettings-compatible INI store, the XDG
paths, the profile store, the open-direct and playback-time stores, the on-disk
JSONL conversation log, and the Linux Secret Service credential store behind
the `CredentialStore` interface), the controller's disk-backed `/pref` toggles,
open-direct restore after ISUPPORT, the `PlaybackCoordinator`, and the shell's
store wiring plus the `connectOnStartup` activation after the first render. The
Connect sheet surfaces persistence and credential status.

The transcript carries the reducer's `New messages` boundary: `internal/ui`
reads the mark through `Controller.UnreadMarkRow` (which mirrors
`MessageListModel::unreadMarkRow`, including its needs-a-row-above guard) and
attaches `UnreadMark.qml`'s accent-mixed rule to the marked row's block, so the
rendered row count keeps matching the message count find and copy index by.
`Open conversations at unread` places the viewport like
`placeTranscriptAfterSelect`: Status follows the end, a conversation lands on its
mark, and re-selecting the same conversation keeps the reader's place. `Alt+U`
jumps to the first row that arrived while the reader was scrolled up, else to
the newest row, with a `↓ new` marker in the transcript header. Because the
runtime only repaints on a message, `Controller.Publish` calls `OnViewChanged`
for any non-empty notify; the shell wires it to the same coalescing wake-up as
the selection and status callbacks, so live chat and membership changes reach
the screen.

## Chords

- The full chord map lives in `internal/ui/keys.go`. `Ctrl+Q` is the only quit
  chord; `Ctrl+C` copies via OSC 52 (`tea.SetClipboard`), not quit.
- `Alt+A` walks to the next unread conversation; `Alt+U` jumps to the first new
  message in the current transcript.

## Version

The version comes only from `version.pri` through `bin/version`. No Go file
hardcodes a version; `internal/version.Value` is injected at link time by
`tui/bin/build`, and `bin/check-conventions` gates both rules.

A release bump touches `version.pri`, `CHANGELOG.md`, and
`tests/test_derive_build_versions.py` once, and both binaries pick it up
atomically. No TUI file needs editing for a version bump.

## CI and review

- Base every pull request on `master`, or on another `tui-phase-*` branch when
  stacking phases in order; CI rejects any other base.
- Run `tui/bin/test` (conventions, vet, tests, build, CLI contract) and the
  root `bin/test` before opening a pull request. `tui/bin/test` is also wired
  into root `bin/test` and skipped when `go` is absent.
- `.github/workflows/tui.yml` is the Linux gate. Do not add a notification
  sink or bot.
- A pull request whose whole diff is TUI-only is gated by `tui.yml` alone:
  `test.yml` ignores `tui/**`, so the Qt app is not rebuilt and the C++ suite
  does not run for the Go port. That holds because `tui/bin/test` runs the
  shared `bin/check-conventions`, which carries the TUI checks (the
  `internal/irc` platform-neutrality rule). Path filters are evaluated over the
  entire pull-request diff, not the last commit, so a pull request that also
  changes the shared `bin/check-conventions` or `bin/test` still runs the Qt
  workflow. That is intended: changing the shared gate must validate it. Root
  `bin/test` exports `OMAIRC_CONVENTIONS_RAN` so the gate runs the shared
  checks only once.
