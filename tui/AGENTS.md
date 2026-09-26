# omairc-tui contributor notes

These rules extend the repository root `AGENTS.md` for the Go terminal client.
When the two disagree about the shared contract, the root file wins.

## Module

- `tui/` is a standalone Go module: `github.com/fredimachado/omairc/tui`.
  The repo root is not a Go module, so every `go` invocation must run with
  `tui/` as the working directory or `go` walks up and fails.
- The `go` directive is `go 1.25`, the minimum `charm.land/bubbletea/v2`
  v2.0.9 declares.
- The module path is domain-qualified so Phase 12's `go install` for Linux
  works without a rename.
- Layout: `cmd/omairc-tui/` (flags and `main`), `internal/irc/` (portable
  core, mirrors `src/irc/`), `internal/ui/` (Bubble Tea shell, mirrors
  `src/qml/` plus `OmaircWindow.qml`), `internal/version/` (injected build
  version), and `bin/` (gate scripts).
- `tui/bin/omairc-tui` is a build artifact and is gitignored.

## Pinned stack

Pin these exact versions. A dependency is added to `go.mod` at its pinned
version the moment its package is first imported — never earlier, because
`go mod tidy` strips unused requires:

- `charm.land/bubbletea/v2` v2.0.9 — imported in Phase 4.
- `charm.land/lipgloss/v2` v2.0.3 — imported in Phase 4.
- `charm.land/bubbles/v2` v2.1.0 — imported in Phase 4.
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

`internal/ui` mirrors `src/qml/` plus `OmaircWindow.qml` as a thin shell over
the Go controller. Keep the window-growth discipline: extract file-based
components instead of growing one god-object, and keep protocol policy out of
the view.

## Feature map

`.cursor/skills/verify-omairc/features/*.md` is the single source of truth for
behavior. `control-omairc` (Qt driver) and the future `control-omairc-tui`
(driver over a PTY) play the same `desktop-recipe` fences. Do not compile the
map into the binary or add a command that dumps it.

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
- Run `tui/bin/test` (vet, tests, build, CLI contract) and the root `bin/test`
  before opening a pull request. `tui/bin/test` is also wired into root
  `bin/test` and skipped when `go` is absent.
- `.github/workflows/tui.yml` is the Linux gate. Do not add a notification
  sink or bot.
