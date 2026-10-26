# Omairc contributor notes

## Product boundary

- Keep Omairc simple, keyboard-friendly, and visually native to Omarchy.
- Conversations are live through the IRC controller (`client/src/controller/`).
  `--demo-server` seeds that controller in-process and skips Connect. UI and
  integration tests use the demo seed or a small fake controller.
- Prefer a small, calm interface over adding controls for hypothetical future
  features.

## Architecture

- The desktop client is a Rust GPUI application in `client/`. Cargo's default
  features include `product` (install identity: `omairc` folder and IPC). The
  `dev` feature uses a separate `omairc-poc` config/state tree so local
  `cargo run` / `cargo test --features dev` never claim a real install.
- `client/src/app.rs` owns the GPUI application shell: window chrome, theme
  appearance, text scale, shortcuts, overlay open/close, conversation selection,
  composer, slash dispatch, and navigation helpers. Presentation lives in
  `client/src/ui/`.
- Seed the demo world from the in-process demo server, not from ad hoc UI list
  models.
- Keep `client/src/desktop/` limited to desktop integration: Omarchy theme
  colors, live theme watching, text scale, window geometry, notifications, and
  platform paths.
- Keep IRC networking, protocol, session, and model code in `client/src/irc/`.
  IRC formatting stays in the IRC layer; nick colors stay presentation
  (`client/src/ui/` style helpers). Do not move URL policy or invite-channel
  parsing into unrelated modules.
- Keep system light/dark mode and portal text-scale detection in
  `client/src/desktop/`.
- Use the bundled `iA Writer Mono S` font for all custom interface text.
- The public CLI skill lives in `skills/omairc/`. Install it with
  `npx skills add fredimachado/omairc/skills -g`. Desktop proof uses
  `.cursor/skills/verify-omairc-poc` (`control-omairc-poc`: isolated Xvfb +
  xdotool, clears `WAYLAND_DISPLAY`, requires lavapipe; pointer verbs use
  XTEST/XI2 clicks). `bin/verify-poc-recipes` and the non-blocking
  `verify-poc` job in `test.yml` drive the feature map. Load
  `.cursor/skills/conversation-cause` when touching `ensureConversation`,
  conversation creation, or `/msg`, and `.cursor/skills/redact-secrets` when
  touching Status lines, error previews, or secret redaction in
  `client/src/irc/` and `tui/internal/irc/`.

`.cursor/skills/verify-omairc/features/*.md` is the map of how a user
reaches a behavior: shortcuts, preconditions, and observables.
`control-omairc-poc` drives that map against the Rust GPUI window;
`control-omairc-tui` replays the same fences against the TUI. Proof of each
GUI stays that window: title, screenshots, and chords. Do not compile the map
into the binary, the product CLI, or a Rust catalog.
Do not add debug queries or a debug command so an agent can read the map from a
running window. The product CLI is the existing `omairc` commands that control
a user's running window: `connections`, `status`, `send`, `read`, `names`, and
`raise`. It is not a test harness. Do not add a command that dumps the map.
`bin/test` stays the offscreen gate.

Agents looking for the legacy `verify-omairc` desktop driver should use
`.cursor/skills/verify-omairc-poc` instead; the feature map remains under
`.cursor/skills/verify-omairc/features/`.

## Historical Qt parity

Intentional differences between the retired Qt client and this Rust desktop
client are no longer maintained here. See `CHANGELOG.md` (Known regressions /
release notes) for the historical list.

## Desktop UI conventions

- Dynamic conversation navigation must use an unambiguous conversation identity
  in models and tests. Confirm rendered labels in the live window or GPUI
  tests; a clean compile does not prove visible text.
- Sidebar and network lists are model-driven from the controller, not by
  walking view children.
- Collapse the server list by width, not by hiding it with zero size.
- The Connect sheet is tabbed: `Connection` holds the per-network profile and
  `Preferences` holds global settings such as reopening direct messages on
  startup. `/pref` reads and sets those toggles from the composer
  (`directs`, `avatars`, `unread`). Open conversations at unread starts on.
  Only queries the user opened or replied to are restored, and restore waits
  until after ISUPPORT (`376` / `422`). Keep the `Ctrl+/` shortcut hint in its
  rail so new users find the shortcuts sheet.
- The Connect sheet is a window-level modal. Sidebar servers, conversations,
  and the member panel cannot be clicked, focused, or walked while it is open.
  The dimmer is partly transparent so those columns stay visible. `Ctrl+/` still
  opens the shortcuts overlay on top of it.
- The Connect sheet is keyboard-first. `Enter` walks to the next field and
  `Ctrl+Enter` applies, so a stray keypress cannot commit a half-typed profile.
  `Ctrl+Enter` is one window-level shortcut, not a branch inside each control.
  The network rail owns `Up`/`Down`/`Home`/`End` and scrolls the focused row into
  view. Focusable rows must stay reachable by `Tab` as well. Closing the sheet
  returns focus to the composer.
- Channel switching must update the topic, message model, people count, and
  member list together.
- Show the people count, member toggle, and member panel only for channels.
  Clicking another user in the member panel must open or create a direct message
  with its own local message history.
- Order member rows by the server's `PREFIX` rank, highest first, then nick.
  Sort through `orderedMembers` in the IRC reducer, never in the view layer,
  and keep the panel and `omairc names` in that same order.
- Keep the composer single-line. `Enter` sends the message.

## Visual conventions

- Derive UI colors from Omarchy theme background, foreground, accent, and
  selection; do not introduce a separate fixed palette for structural UI.
- Derive secondary surfaces and muted text with the shared style helpers so new
  Omarchy themes continue to work.
- Scale dimensions and text through the desktop text-scale path and UI scale
  helpers.
- Keep conversation counts consistent between the header and member panel.

## Review lessons

`bin/check-conventions` already gates the mechanical repeats. These still
need judgment.

- Inventing a conversation requires `IrcConversationCause` on
  `ensureConversation`. QuietSend and InboundSelf never invent; UserOpen
  invents DMs; ChannelState invents channels; InboundOther invents
  channels and non-service DMs; Restore invents non-service DMs after
  ISUPPORT so engaged queries survive a restart. Persist only when the
  user opened the query, sent in it, or a self-authored line arrives.
  Cover `/msg` with `echo-message`, incoming NickServ PRIVMSG, and
  incoming human PRIVMSG in the same test matrix.
- Do not hard-code CHANTYPES, CHANMODES, or PREFIX. Call
  `IrcServerFeatures`.
- Fail closed when redacting secrets for Status or error previews. Do not
  enumerate one more well-formed bypass.

## Correction loop

When a review comment or a failing test shows a bad pattern, fix it in
this order:

1. Make the bad pattern impossible in the type, the model, or the call.
   `IrcConversationCause` and `IrcServerFeatures` are the examples, or an
   API that cannot express the mistake.
2. Else add or extend a check in `bin/check-conventions`.
3. Else add or adjust one of the internal playbooks:
   `.cursor/skills/conversation-cause`, `.cursor/skills/redact-secrets`.
4. A sentence in `AGENTS.md` is the last place, and only for judgment the
   three layers cannot encode.

Pick a GitHub issue, branch off `main`, run `bin/test`, and open the
pull request against `main`. Also run `bin/test-live` when it touches
real IRCd behavior. CI rejects a pull request whose base is not `main`, a
`tui-phase-*` branch, or a `slice/rust-cutover-*` stack base. No new
bot, workflow, or notification sink.

## Release versioning

`version.pri` is the single `VERSION` for `--version`, CTCP `VERSION`,
tests, and packaging. Cargo needs a literal `version` in
`client/Cargo.toml` (and a matching `client/Cargo.lock` entry); those
stay checked copies. Prefer `bin/set-version X.Y.Z` to rewrite
`version.pri`, `client/Cargo.toml`, and the lock together.
`bin/check-conventions` fails when they drift. A release bump touches at
least these files:

- `version.pri` - set `VERSION` (or run `bin/set-version`).
- `client/Cargo.toml` and `client/Cargo.lock` - keep in lockstep with
  `version.pri` (letter suffixes become SemVer pre-releases, e.g.
  `1.0.7alpha` → `1.0.7-alpha`).
- `CHANGELOG.md` - move `[Unreleased]` into a dated `[x.y.z]` section and
  update the compare links at the bottom.
- `tests/test_derive_build_versions.py` - replace every hardcoded
  `1.0.x` string and `refs/tags/v1.0.x` ref with the new version.
  `bin/test` runs this suite; forgetting it fails CI immediately with
  `'1.0.4' != '1.0.3'`-style assertions.

Run `bin/test` (or at least
`python3 -m unittest -v tests.test_derive_build_versions`) before
tagging. One bump of `version.pri`, `client/Cargo.toml`,
`client/Cargo.lock`, `CHANGELOG.md`, and
`tests/test_derive_build_versions.py` ships both binaries. Tag that
commit as `v` plus the `version.pri` string, for example `v1.0.4`, and
as `tui/v1.0.4` so `go install` sees the module version. GitHub's `v*`
filter does not match `tui/v*`, so the module tag does not open a second
release. Version tags trigger release packaging, the Homebrew cask bump,
and the omairc-tui formula and Scoop manifest bumps. Do not hand-edit
`Casks/omairc.rb`, `Formula/omairc-tui.rb`, or `bucket/omairc-tui.json`
for a normal release. If a tagged commit needs a follow-up fix before
artifacts ship, commit on `main`, delete and recreate both tags on the
new tip, and `git push origin vX.Y.Z --force` (and the same for
`tui/vX.Y.Z`).

## Build and validation

`bin/test` is the default gate. It runs `bin/check-conventions`, `cargo test`
and `clippy` in `client/` (default `product` features, plus a `dev` feature
pass), version/help smoke on the built `omairc` binary, and `tui/bin/test`
when `go` is on `PATH`. CI runs `bin/test` and `bin/test-live`
on every pull request that touches anything outside `tui/**`, and rejects a pull
request whose base is not `main`, a `tui-phase-*` branch, or a
`slice/rust-cutover-*` stack base. A pull request whose whole diff is TUI-only is
gated by the TUI workflow alone: `test.yml` ignores `tui/**`, and `tui/bin/test`
runs the shared `bin/check-conventions` so the TUI checks still hold. Paths are
filtered over the whole pull-request diff, so a pull request that also changes
`bin/check-conventions` or `bin/test` still runs the desktop workflow. See
`tui/AGENTS.md`.

```sh
bin/test
bin/test-install
bin/test-desktop
bin/test-live
bin/verify-poc-recipes
```

The TUI port lives in `tui/` as its own Go module; its gate is `tui/bin/test`
(vet, tests, build, CLI contract), which root `bin/test` runs when `go` is on
`PATH`. The shared contract spans the Rust desktop client and the TUI:
`version.pri` stays the single version source, and `IrcConversationCause`,
`IrcServerFeatures`, `IrcSecretPolicy`, `orderedMembers`, and the feature map
under `.cursor/skills/verify-omairc/features/*.md` must stay aligned between
`client/`, `tui/`, and the map. TUI-scoped rules live in `tui/AGENTS.md`.

`bin/test-desktop` needs Xvfb, Xauthority, xdotool, xinput, ImageMagick,
cargo, and lavapipe (`vulkan-swrast`). It drives
`client/target/release/omairc` (`--features product`) through
`control-omairc-poc` as a black box and is not in CI. `bin/verify-poc-recipes`
replays every feature fence the same way. `bin/test-install` stages the six
packager paths from that release binary (binary, desktop file, SVG, bash
completion, LICENSE, OFL).
`bin/test-live` needs Docker. It drives Ergo, Solanum, and ngIRCd, then the
Rust live suite under `tests/live/`. It fails if `docker` or `docker compose`
is missing.

For visual changes, inspect the running window and the screenshots under
`test-artifacts/verify-poc/`.

Judge desktop claims against the application log under the platform state
location (`omairc.log` beside other state files). No CI job drives the full
desktop window for every PR, so log lines are a manual observation when needed,
not the default gate.

On Linux, build from the repo root:

```sh
cd client && cargo build --release --locked
./target/release/omairc --demo-server
```

Set `OMAIRC_BUILD_VERSION` when packaging needs an exact `--version` string
(CI and `bin/test` set it from `bin/version`).

## Windows

`bin\build-rust.bat` builds the product client with MSVC static CRT
(`cargo build --release --locked --features product` in `client/`) and stages
`build\release\omairc.exe`. Load the Visual Studio toolchain first when `cl`
is not on `PATH` (see `vswhere` / `vcvars64.bat` in prior Qt docs; the Rust
script uses the same MSVC expectation).

`bin/test`, `bin/test-desktop`, and `bin/test-live` stay Unix scripts. `--help` and the local CLI attach to a parent Windows console with
`AttachConsole` (cmd/PowerShell, not Git Bash/mintty). Redirected stdout/stderr
stay on the pipe or file. Local CLI IPC uses a named pipe (`omairc`), not a
filesystem socket. Portal text-scale stays 1.0, desktop notifications use the
Windows toast path, and the Omarchy `colors.toml` watch does nothing when that
file is missing. Windows embeds `data/icons/omairc.ico` in the exe and the
window icon; regenerate it from the SVG with ImageMagick when the mark changes.

`bin\package-windows.bat` compiles `packaging/windows/omairc.iss` with Inno
Setup 6 into `dist\omairc-*-windows-x64-setup.exe`. It expects a fresh
`bin\build-rust.bat` tree under `build\release`. Privileges stay lowest by
default (`{autopf}` is `%LOCALAPPDATA%\Programs\Omairc`); the wizard can
elevate for all users. The installer uses `data/icons/omairc.ico`, adds a Start
Menu shortcut and user PATH, and leaves config and logs alone on uninstall.
Look for `ISCC.exe` under `%LOCALAPPDATA%\Programs\Inno Setup 6` as well as
Program Files, or set `ISCC`.

## macOS

`bin/build-macos-rust` and `bin/package-macos` produce `build/omairc.app` and
`dist/omairc-*-macos-*.zip`; CI runs `.github/workflows/macos.yml` on
`macos-15` (Apple Silicon) and `macos-15-intel` with Rust toolchains. CI
asserts the Mach-O contains `arm64` or `x86_64` to match the
`macos-arm64` / `macos-x64` artifact name. Pull requests compile, smoke, and
(on Apple Silicon) run portable tests; they do not zip or upload artifacts.
`main` is the same for packaging. Version tags and `workflow_dispatch` package
both zips, and tags attach both to the GitHub release. Intel pull requests skip
the portable test rebuild. Passwords use the macOS Keychain. Regenerate
`data/icons/omairc.icns` from the SVG with `packaging/macos/generate-icns`
when the mark changes.
CI imports a Developer ID certificate and signs with hardened runtime
(`packaging/macos/Omairc.entitlements`; library validation stays on). Version
tags and workflow_dispatch run `bin/notarize-macos` (`notarytool` + staple),
then `stapler validate`, `syspolicy_check distribution`, and
`spctl --assess --type execute --verbose=4`. Pull requests sign only and run
`syspolicy_check notary-submission`: Apple often holds the first Developer ID
uploads In Progress for hours. Tag releases fail if the signing secrets are
missing or Gatekeeper rejects the stapled app. Local `bin/build-macos-rust`
stays ad-hoc unless `CODESIGN_IDENTITY` is set. The Homebrew cask is
`Casks/omairc.rb` so this repository is a tap (`brew tap fredimachado/omairc
https://github.com/fredimachado/omairc`; the URL is required because the repo
is not `homebrew-omairc`). It ships the GitHub `macos-arm64` / `macos-x64`
zips, `livecheck` on the latest release, a PATH helper that execs
`Contents/MacOS/omairc` so `omairc connections` works from a terminal, and
`zap` of `~/Library/Preferences/omairc` plus
`~/Library/Preferences/State/omairc`. Version tags rewrite `version` and both
`sha256`s through `bin/bump-homebrew-cask` after both zips land. The bump job
fetches and rebases onto `origin/main` when that push is not a fast-forward; a
conflict re-applies the generated cask on the new tip instead of leaving the
tap on the prior version. Do not install the cask from a raw GitHub URL.
Uninstall leaves config; `--zap` is the wipe.
