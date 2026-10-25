---
name: Rust client cutover gaps
overview: "Close every gap between the Qt client and the Rust GPUI client (client/) so Qt can be removed: window-state behavior, settings and credential migration, Windows and macOS build/packaging/CI, test layers, and the repository's Qt-centric gates and docs. Work lands as small slices off main, one PR each, in the order below."
todos:
  - id: m0-migration-fixes
    content: "M0 (done in this branch, needs Linux proof): Linux keyring reads QtKeychain items; Windows registry import merges into the TUI INI with a one-time marker; QStringList @Variant and @@ decoding"
    status: in_progress
  - id: m0-tui-keyring
    content: "M0b: port the dual-attribute Secret Service lookup to tui/internal/storage/secretservice.go (same contract)"
    status: completed
  - id: m1-windows-ci
    content: "M1: Windows + macOS cargo build/test in CI, cfg(unix) hygiene, product-feature builds"
    status: completed
  - id: m2-minimize
    content: "M2: minimized/hidden window suspension (transcriptSuspended port)"
    status: completed
  - id: m3-windows-packaging
    content: "M3: Windows build script, static CRT, Inno installer from cargo output, Qt-to-Rust upgrade path"
    status: completed
  - id: m4-macos-packaging
    content: "M4: macOS .app bundle, signing, notarization, cask from cargo output"
    status: completed
  - id: m5-live-tests
    content: "M5: Rust live tests against Ergo, Solanum, ngIRCd (bin/test-live parity)"
    status: completed
  - id: m6-ui-tests
    content: "M6: GPUI view tests for the QML-only behaviors (scroll, typing row, focus, sheets) - first wave + checklist landed; URL click / file-drop / transcript-nick / middle-click close pending (wave 2: real pointer through the view)"
    status: in_progress
  - id: m7-poc-driver
    content: "M7: XI2 click verbs in control-omairc-poc + non-blocking verify-poc CI (bin/verify-poc-recipes)"
    status: completed
  - id: m8-smaller
    content: "M8: dark-mode portal check on Linux, RTL visual proof, accessibility tracking"
    status: completed
  - id: m9-cutover
    content: "M9: rename poc/ to client/, retarget gates/docs/AGENTS.md, delete Qt"
    status: completed
isProject: false
---

# Rust client cutover gaps

The Rust client already matches Qt in commands, chords, protocol, storage
formats, CLI, and most platform code. What is left is listed below in the
order it should land. Each milestone is one or more slices off `main`, each
with `bin/test` green, and with `cargo test` green on Linux, Windows, and macOS
once M1 lands.

Order rationale: M0 protects user data and must ship before any Rust build
reaches a Windows or Linux user who has Qt settings. M1 comes next because
every later Windows/macOS milestone needs CI to prove it. M2 is the only
user-visible behavior gap. M3–M4 make Rust installable outside Arch. M5–M7
replace the Qt test layers. M9 is the cutover itself.

## M0 — Settings and credentials survive the switch

### Findings

Checked against the Qt sources and QtKeychain upstream
(`qtkeychain/libsecret.cpp`, `keychain_win.cpp`, `keychain_apple.mm`):

| Data | Linux | macOS | Windows |
|---|---|---|---|
| Settings file | same `~/.config/omairc/omairc.conf`, read in place | same `~/Library/Preferences/omairc/omairc.ini` | Qt uses `HKCU\Software\omairc\omairc`; Rust imports it into `%LOCALAPPDATA%\omairc\omairc.conf` |
| Keys and value types | every Qt group/key name present in Rust (`networks/*`, `preferences/*`, `openDirects`, `dismissedDirects`, `closedConversations`, `highlights`, `ignores`, `mutes`, `monitors`, `playbackTimes`, `scrollPlaces`, `window/geometry` `@Rect`, `window/maximized`) | same | same, via the import |
| STS cache | `omairc/sts` beside settings | same | `%LOCALAPPDATA%\omairc\sts` |
| Transcripts / CLI cursors / log | `$XDG_STATE_HOME/omairc/{logs,cli-cursors,omairc.log}` | `~/Library/Preferences/State/omairc/…` | `%LOCALAPPDATA%\State\omairc\…` |
| Saved passwords | **broken before M0**: QtKeychain/libsecret stores `user`/`server`/`type`; Rust looked up `service`/`username` | match (`kSecAttrService=omairc`, `kSecAttrAccount=<key>`) | match (`<key>@omairc`, UTF-8 blob) |

Problems found and fixed in M0:

1. **Linux passwords saved by Qt were invisible.** QtKeychain's libsecret
   backend (the one Omarchy/Hyprland, GNOME, and Xfce use) writes items with
   `user=<key name>`, `server=omairc`, `type=plaintext`, schema
   `org.qt.keychain`. The Rust store (and the TUI it was ported from) searched
   `service=omairc` + `username=<key name>`, which never matches those items.
   Every saved server and NickServ password would have read as missing after
   the switch, while `secretSaved=true` stayed in the profile.
2. **Windows registry import was skipped when the TUI had run.** The TUI on
   Windows keeps its settings in `%LOCALAPPDATA%\omairc\omairc.conf`, the same
   file Rust imports into. The import treated "file exists" as "already
   imported", so a user who had ever saved anything in the TUI lost every Qt
   network and preference.
3. **Edge-case string lists.** Qt writes a list item that starts with `@` as
   `@@…`, and a list with an empty item as UTF-16 `@Variant(<QDataStream>)` in a
   `REG_BINARY`. Rust kept `@@` and stored the variant as one opaque string.

### Changes (this branch)

- `client/src/storage/credentialstore/secretattrs.rs` (new, compiles on every
  platform so its tests run everywhere): the two attribute sets, the written
  set (both sets + `type=plaintext` + `xdg:schema=org.qt.keychain`), and base64
  decoding for QtKeychain binary-mode items.
- `client/src/storage/credentialstore/linux.rs`:
  - read searches both sets and prefers the dual-set item;
  - write creates one dual-set item (readable by Qt after a rollback and by
    the TUI), then deletes older single-set copies so none is read stale;
  - delete removes both sets.
- `client/src/storage/registryimport.rs`:
  - import runs until the INI holds `migration/qtRegistryImported=true`, not
    only when the INI is missing;
  - it merges: a key the INI holds wins; a string list the INI holds keeps
    its order and gains the registry's missing items (`networkOrder`,
    `collapsedNetworks`, `highlights/words`, …); everything else is added;
  - a malformed or read-only INI is left untouched (the existing fail-closed
    `sync`);
  - `@@` list items and `@Variant` `QStringList` values decode as Qt reads
    them.
- `client/src/model/theme.rs`: the symlink test is `#[cfg(unix)]`, so `cargo test`
  compiles on Windows. The full suite (2,189 unit tests plus the gates) passes
  on Windows with `--features product` storage tests included.

### Still to prove (blocking M0 done)

- [ ] `cargo check`/`cargo test` on Linux. `linux.rs` cannot be type-checked
  from Windows: `ring` needs a Linux C toolchain.
- [ ] End-to-end on a Linux session with gnome-keyring:
  1. save a password with the Qt client (QtKeychain 0.17);
  2. start the Rust product build: the Connect sheet shows it saved and
     connects;
  3. change it in Rust, then start Qt: Qt reads the new value;
  4. confirm `secret-tool search --all server omairc` shows one item per key.
- [x] `#[ignore]`d test beside `the_secret_service_round_trips_a_secret`
  that seeds a QtKeychain-shaped item through oo7
  (`a_qtkeychain_shaped_secret_is_readable` in `linux.rs`).
- [ ] Windows end-to-end: a machine with Qt registry settings plus a TUI
  `omairc.conf` gets both sets of networks after first Rust start; a second
  start changes nothing.

### Known limits (document, do not fix)

- **KDE Plasma:** QtKeychain uses KWallet's own API there, not the Secret
  Service, so Qt-saved passwords are not visible to Rust. The user re-enters
  them once. Revisit only if a Plasma user reports it.
- **macOS keychain prompt:** an item created by the Qt app is trusted for that
  app's code signature. Keep `CFBundleIdentifier` `app.omairc.Omairc` and the
  same Developer ID team for the Rust bundle (M4). Otherwise the first read
  shows the "wants to use your confidential information" prompt once.
- **Avatar cache** is not migrated; it refills.
- Rollback on Windows: Rust never writes the registry, so a downgrade to Qt
  sees the pre-switch settings, not the changes made in Rust.

### M0b — same fix in the TUI

`tui/internal/storage/secretservice.go` has the same `service`/`username`
lookup, so a Linux TUI user also cannot read Qt-saved passwords. AGENTS.md
requires the credential contract to stay identical, so port the dual-set
read, write, and delete there, with Go tests that mirror `secretattrs.rs`.

## M1 — Cross-platform CI for the Rust client

**Done on this branch.** Today only Linux ran `cargo test` (inside `bin/test`).
About 3k lines of Windows-only code and the macOS code never compiled in CI.

- `.github/workflows/windows.yml`: `rust` job on `windows-2022` (same image as
  the Qt job): release product build, both test suites, smoke
  (`--version` / `--help` / `OMAIRC_SMOKE=1 --demo-server`). A `changes` job
  skips the Qt job on poc-only diffs.
- `.github/workflows/macos.yml`: the same on `macos-15` and `macos-15-intel`.
- `test.yml`: `cargo test --locked --features product`, clippy with
  `-D warnings` (style/complexity/pedantic/nursery/perf allowed until a cleanup
  slice; unused_imports and correctness stay denied), `Swatinem/rust-cache`.
- `client/tests/os_cfg_hygiene.rs`: no `std::os::unix` / `std::os::windows` outside
  a matching cfg (or a `windows.rs` / `linux.rs` / `macos.rs` module file).
- Path filters include `client/**`, `version.pri`, and the workflow files.

## M2 — Minimized and hidden window counts as away

**Done on this branch.** gpui 0.2.2 has no minimize API, so
`desktop/windowvisibility` reads the native handle (Windows `IsIconic` /
`IsWindowVisible`, macOS `NSWindow.isMiniaturized` / `isVisible`, X11
`_NET_WM_STATE_HIDDEN`; Wayland always `Visible`). The shell keeps
`TranscriptActivity` (`window_active` + `suspended`) and pushes
`active && !suspended` to the controller. Pure tests in
`controller/tests/suspension.rs` and `windowvisibility` cover the Qt cases;
feature-map Gotchas note the minimize rule and the Wayland/Hyprland limit.

## M3 — Windows build and installer from cargo

- `bin\build-rust.bat` (or extend `bin\build.bat` with `OMAIRC_CLIENT=rust`):
  locate `vcvars64.bat` the same way, run
  `cargo build --release --locked --features product`, stage
  `client\target\release\omairc.exe` plus `LICENSE`, `OFL.txt`, `COPYING-LGPL`
  into `build\release`.
- C runtime: add `client/.cargo/config.toml` with
  `[target.x86_64-pc-windows-msvc] rustflags = ["-C", "target-feature=+crt-static"]`
  so the exe needs no `vcruntime140.dll`. Verify with `dumpbin /dependents`
  in CI; fail if `VCRUNTIME` or `MSVCP` appears.
- `packaging/windows/omairc.iss`: keep `AppId`, `MyAppUserModelID`
  (`FrediMachado.Omairc`), and the toast activator CLSID unchanged so the Start
  Menu shortcut, toasts, and the Rust updater's installed-copy detection keep
  working. Add `[InstallDelete]` for every Qt file the Qt build shipped
  (`Qt6*.dll`, `platforms\`, `imageformats\`, `qml\`, `qt6keychain.dll`,
  `vc_redist*.exe`, `msvcp140*.dll`, `vcruntime140*.dll`, …) so an upgrade over
  Qt does not leave a stale tree.
- Updater: the Qt updater downloads `omairc-<v>-windows-x64-setup.exe`. The
  first Rust release keeps that asset name, so installed Qt copies upgrade
  themselves. Add a test fixture asserting the Rust updater accepts the same
  JSON.
- `windows.yml` smoke-installs the setup, runs `omairc.exe --version`, checks
  the shortcut AUMID, uninstalls, and checks `%LOCALAPPDATA%\omairc` survives.
  On tags/dispatch the rust job uploads the setup artifact, builds the
  portable zip, and publishes both to the GitHub Release; the Qt job no
  longer packages or uploads.
- Ship order: one release where the setup contains Rust, with M0 already in.

## M4 — macOS app bundle from cargo

**Done on this branch.** Rust owns tag/dispatch release zips (notarize + cask
artifacts). Qt `macos` CI still compiles and runs portable tests when the `qt`
path filter matches; it no longer packages, notarizes, or publishes.

- `bin/build-macos-rust`: `cargo build --release --locked --features product`
  for the host triple (`aarch64-apple-darwin` / `x86_64-apple-darwin`).
  Assembles `build/omairc.app` from `packaging/macos/Info.plist`
  (`CFBundleIdentifier` stays `app.omairc.Omairc`), `omairc.icns`,
  `Contents/MacOS/omairc`. No `qt.conf`, no frameworks. Signs through
  `bin/sign-macos` (hardened runtime + `Omairc.entitlements`) and runs
  `bin/check-macos-bundle`.
- Reuse `bin/package-macos`, `bin/notarize-macos`, `bin/check-macos-syspolicy`
  against the rust-built app (`OMAIRC_CLIENT=rust` for error text). On
  tags/dispatch the rust job uploads cask zip names, notarizes when secrets
  exist, and publishes GitHub Release assets. `macos-zips` / `bump-cask`
  consume rust artifacts.
- `Casks/omairc.rb`: unchanged zip names / PATH helper / zap paths.
- `macos.yml` rust job: assemble, assert layout + Mach-O arch, smoke the
  bundle with `OMAIRC_SMOKE=1`, prove `bin/package-macos` zip names. Developer
  ID is optional on pull requests; v* tags fail closed without signing secrets.

## M5 — Live tests against real IRC daemons

**Done on this branch (code + CI wiring; green against daemons needs Linux+Docker).**

Rust's network tests use a scripted peer. Qt's `bin/test-live` proves
interoperability with Ergo, Solanum, and ngIRCd. The Rust suite reuses the same
compose/certs/env and drives the controller through the real `net` transport
(no GPUI).

- Library surface: `client/src/lib.rs` (binary calls `omairc::run()`),
  `NetOptions` / `client_config_trusting_pem` for `live.pem`,
  `Controller::connect_session` / `set_transcript_root`, session received-log
  and `force_transport_shutdown`.
- Harness: `client/src/live/` reads `OMAIRC_LIVE_*` the same way as Qt
  `liveharness`.
- Cases: `client/tests/live/` (20 `#[ignore]`d tests) covering register plain/TLS,
  capability flags, ISUPPORT, classic traffic, nick in use / length, WHO
  snapshot, peer away/typing/metadata, chat history on join, SASL PLAIN,
  conversation invention matrix, folded nick collision, ASCII DM restore,
  join multiple channels, member prefix ranks, reconnect autojoin, and a
  controller-level dual-session same-nick DM isolation proof (one Controller,
  two `connect_session`s against two daemons; Status and transcript asserts).
- Remaining vs Qt live-UI: fuller dual-network UI proofs
  (`inboundSameNickDirectsStayIsolated` screenshots,
  `bouncerQueryReplayRendersDirectMessageInSidebar`, and the other QML-only
  live-UI cases) stay for M6 or a follow-up once the window is under test.
- `bin/test-live` runs `cargo test --test live --locked -- --ignored
  --test-threads=1` after the Qt suite when `cargo` is on `PATH`.
- CI `live` job installs the stable Rust toolchain and rust-cache.

## M6 — GPUI view tests

Rust covers the logic (2,189 tests) but has 3 `#[gpui::test]`. Qt's 349
offscreen QML tests also pin view behavior. Port the ones whose behavior lives
in `ui/`, not `model/`:

- Transcript placement: `shortTranscriptSitsAtBottom`, `…GrowsUpward`,
  `…ResizeKeepsBottom`, `historyPrependKeepsAnchorMessageY`,
  `historySpliceKeepsTheReaderOnTheSameMessage`, `cappedTrimKeepsDetachedPlace`,
  `detachedArrivalKeepsViewportAndJumpsToFirstUnseen`,
  `followingAppendStaysPinnedThroughLayout`.
- Typing row: `typingIndicatorRidesTheTranscriptEnd`,
  `…MovesBelowMyOwnMessage`, `…KeepsScrollWhileReadingHistory`.
- Focus and sheets: `connectionSheetCloseRestoresComposerFocus`,
  `aboutSheetLeftClickDimmerDismisses`, `aboutSheetIgnoresRightClickAndCardClick`,
  `connectionSheetBlocksSidebarAndMembers`, `memberListPageDownMovesByVisiblePage`.
- Pointer (wave 2 - real hits through the view, not shell-API stubs):
  `messageBodyClickOpensHttpsUrl`, `openDirectMessageFromTranscriptNick`,
  `droppedFileUploadsAndInsertsTheLink`, middle-click close.
- Harness: one `client/src/ui/tests/fixture.rs` that builds the shell over the
  seeded demo controller in a `TestAppContext` with a fixed window size, plus
  helpers for key chords, clicks at element bounds, and reading
  `ListState` offsets. Keep each file under the 800-line gate.
- Track progress with a checklist mapping each `tst_omairc.qml` test to its
  Rust test or to "covered in model/ by X". That list is the cutover evidence.

## M7 — Feature-map recipes on the Rust client

- Done: `control-omairc-poc` `click-*` verbs use `xdotool mousemove --sync`
  then `xdotool click --window 0` through the `Virtual core XTEST pointer`
  (`xinput`), so GPUI sees XI2 ButtonPress. Named pixels stay on the
  1180×760 layout. `ctrl+shift+slash` maps About if a recipe needs the
  keyboard escape hatch; shared `about.md` / `send-message.md` fences keep
  `click-version` / `click-send`.
- Done: `bin/verify-poc-recipes` runs every `desktop-recipe` fence under
  Xvfb + lavapipe. `test.yml` job `verify-poc` is non-blocking
  (`continue-on-error: true`) and uploads `test-artifacts/verify-poc/`.
  Make that job blocking once green is stable.

## M8 — Smaller items

- **Dark mode on Linux:** Done (document). GPUI already watches portal
  `color-scheme` via ashpd; app follows `observe_window_appearance`. No
  second subscription next to `textscale.rs`. Hyprland flip not proven on
  Windows / CI Xvfb (no session bus); host check noted in `client/NOTES.md`.
- **RTL:** Done. Spike closed; lavapipe in CI verify-poc; no upstream issue
  (optional screenshot recipe kept in NOTES).
- **Accessibility:** Done. NOTES list kept; Unreleased CHANGELOG draft +
  NOTES cutover draft for M9.
- **`bin/test-install`:** Done. Cargo product release + PKGBUILD seven
  paths; keep `COPYING-LGPL` while `src/irc/` remains.
- **`bin/test-desktop`:** Done. Drives release+product via
  `control-omairc-poc` (`OMAIRC_POC_PROFILE` / `OMAIRC_POC_FEATURES`).

## M9 — Cutover

1. Rename `client/` to the product crate (`client/` or the repo root). Make
   `product` the default feature, and add a `dev` feature for the separate
   `omairc-poc` folder. Set the crate version from `version.pri` in
   `build.rs` (already exported) and assert it in a test.
2. `bin/test`: drop the qmake/QML steps and run cargo test, clippy, and the
   gates.
3. `bin/check-conventions`: remove the QML/C++ gates. Move the remaining
   shared-contract checks (`IrcConversationCause`, `IrcServerFeatures`,
   `IrcSecretPolicy`, `orderedMembers`, feature map) to compare the Rust
   crate with `tui/`.
4. `AGENTS.md`: rewrite Architecture, QML, build and Windows sections for
   the Rust client. Keep the review lessons and correction loop. Update the
   "Intentional Qt↔Rust diffs" section into release notes.
5. Remove `src/`, `tests/` (except `tests/live/` infrastructure),
   `omairc.pro`, `qtkeychain.pri`, `bin/install-qtkeychain`, the Qt pin scripts,
   and the QML skills. Update `README.md`, `skills/omairc/`, and the
   `verify-omairc` skill to drive the Rust client.
6. `CHANGELOG.md`: one "Changed" entry for the new client and one
   "Known regressions" entry (screen readers, KWallet passwords, Wayland
   minimize).

## Validation for every slice

- `bin/test` (and `tui/bin/test` for M0b).
- `cargo test --locked` and `cargo test --locked --features product` on Linux,
  Windows, macOS (M1 onward).
- `bin/test-live` for M5 and anything touching session code.
- Feature-map recipe runs on `control-omairc-poc` for UI slices (M7 onward).
