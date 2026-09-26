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
  snapshots), `internal/demo/` (the `IrcDemoServer` seed harness behind
  `--demo-server`, mirrors `src/irc/ircdemoserver.cpp`), `internal/ui/`
  (Bubble Tea shell, mirrors `src/qml/` plus `OmaircWindow.qml`),
  `internal/gate/` (the PTY parity driver: VT grid, OSC title capture, key
  writer, screenshot), `internal/version/` (injected build version), and
  `bin/` (gate scripts).
- `tui/bin/omairc-tui` is a build artifact and is gitignored.

## Package boundaries

- `internal/irc` is the portable core. It never imports `net`, `os`, or
  `crypto/tls`; sockets, the filesystem, and TLS live in `internal/session`.
  `bin/check-conventions` gates this mechanically (test files may read
  testdata with `os`).
- `internal/session` is the only product package that opens sockets or TLS;
  `internal/gate` is the dev-only PTY driver (a Unix socket plus `os/exec`),
  not product code. All timers, reconnects, the ping watchdog, the
  labeled-response timeout, and typing pacing go through `internal/session`'s
  `Clock`/`Timer` seam, so tests drive them manually with `FakeClock` and
  `LoopbackTransport`.
- `internal/controller` holds the state the shell calls: sessions, the
  reducer, selection, sidebar order, send, and the Status ring buffer. It
  reads time only from an injected `session.Clock`. Later subsystems
  (command dispatcher, playback, persistence, monitor, ignore/mute,
  highlight, autoaway, avatars, inbox store, channel list) sit behind the
  nil-able seams in `internal/controller/seams.go` and land in their own
  phases.
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
  — not imported in Phase 1: the wire layer is a hand-port of `src/irc/` so
  the byte-for-byte behavior and the mirrored test matrices stay the contract.
  Its first import is expected in Phase 10 (`ircfmt`) and later (`ircreader`).

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
- Platform behavior (D-Bus vs osascript vs Toast notifications, clipboard,
  image raster) lives behind build tags in `internal/`, mirroring how
  `src/irc/` stays portable while `Backend` owns desktop integration.

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
`.cursor/skills/verify-omairc-tui/`.

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
