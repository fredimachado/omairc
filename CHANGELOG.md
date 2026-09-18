# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- IRCv3 `draft/ICON` shows the server icon in the sidebar network square when the URL is safe and loads; otherwise the initial letter and palette color stay. `--demo-server` ships a bundled `qrc` icon for the omarchy seed network.
- IRCv3 `draft/metadata-2` now covers peer `avatar`, `status`, and `bot`, plus WHOIS extras (`display-name`, `pronouns`, `homepage`, `color`). Member, nick picker, transcript, and DM circles load an HTTPS avatar when it is safe; a bad URL keeps the initial. Bots get a small mark next to the nick. Standing `status` stays separate from AWAY. `--demo-server` ships bundled `qrc` avatars for `mira`, `anna`, and `kai` so glyphs reach `Image.Ready` without outbound HTTPS.
- `/status [text|clear]` sets or clears the local user's standing metadata without reconnecting. Empty `/status` echoes the current value. Networks that do not grant metadata say so on Status or the asking transcript. Success waits for the server's `761` / `766` reply; legacy numerics and `FAIL METADATA` codes (`KEY_NO_PERMISSION`, `VALUE_INVALID`, `RATE_LIMITED`, and kin) surface the failed set/clear on Status or the asking transcript instead of an optimistic echo. Capability values honor `max-subs` (status-first subscriptions) and `max-value-bytes` (including an explicit `0`, which refuses non-empty text locally) with a 512-byte client ceiling.
- Preferences → **Show peer avatars** (default on) gates automatic avatar fetches. Turn it off to keep avatar hosts from seeing your IP on busy channels; the HTTPS URL policy still fails closed either way.
- Native macOS build and CI artifacts (`omairc-*-macos-arm64.zip` and `omairc-*-macos-x64.zip`). `bin/build-macos` and `bin/package-macos` produce an unsigned `.app` bundle; GitHub Actions uploads both zips on pull requests and attaches them to version tags.
- `omairc conversations` includes `topic` on channel rows.
- The Windows build embeds the app icon in `omairc.exe`. Explorer, the title bar, and the taskbar use it instead of the default executable mark.
- A per-user Inno Setup installer (`omairc-*-windows-x64-setup.exe`) on each version tag, next to the portable zip. `bin\package-windows.bat` compiles it from the windeployqt tree. Start Menu and user PATH are created; config is left alone on uninstall.

### Changed

- `omairc send` returns exit 2 with `"uncertain": true` after a successful local-socket write of the send request when the CLI does not get a readable reply, so agents do not double-send on a lost ack.
- CLI JSON rows omit empty strings and `false` flags so agent payloads stay small. Missing means that default. Numbers such as `unread` stay. Top-level `"ok": false` on errors is unchanged.
- The Windows portable tree ships `msvcp140` and `vcruntime140` next to `omairc.exe` so a user-mode install does not need `vc_redist`. `bin\build.bat` copies those DLLs from `VCToolsRedistDir`. It does not pass `windeployqt --compiler-runtime`: on MSVC that switch only adds an unused `vc_redist.x64.exe`.

### Fixed

- Intel macOS builds skip the Apple Silicon `-include arm_acle.h` workaround, so Qt 6.8 compiles on `macos-15-intel` instead of failing with "ACLE intrinsics support not enabled."
- Hostname image fetches keep the hostname on the HTTP request while still pinning TCP to one pre-validated address, and parse response headers case-insensitively, so Cloudflare (Unreal `draft/ICON` favicon) is not 403'd.
- Avatar fetches refuse decompression bombs whose declared or decoded dimensions exceed a fixed budget, and hostname HTTPS GETs pin to one pre-validated address so QNAM cannot re-resolve (DNS rebinding).
- Closing the last direct message refreshes `connectionStatus` when focus falls back to another network, so the identity footer and Status header no longer keep a stale Connected mark next to the new nick.
- The identity footer shows `offline` with a muted mark when the focused network is not Connected, matching the sidebar network status. It still shows `away` or `available` only while Connected.
- Opening the Windows window from Explorer or the Start Menu no longer flashes a console. `--help` and the local CLI still print in cmd or PowerShell by attaching to the parent terminal.
- Windows uninstall removes the install directory from PATH before the files, so a silent uninstall cannot leave the entry behind.
- `bin\build.bat` finds `vswhere.exe` when `cl` is not already on PATH. Delayed expansion cannot read `%ProgramFiles(x86)%`.
- The Windows installer and `LICENSE` copyright name Fredi Machado.
- The Windows installer treats expanded and unexpanded PATH entries as the same directory, so a `%LOCALAPPDATA%\Programs\Omairc` entry is not duplicated on install and is removed on uninstall.

## [0.6.0] - 2026-09-17

### Added

- A native Windows build. `bin\build.bat` and `bin\build.ps1` produce a portable tree next to `omairc.exe`. CI uploads that tree as an artifact; each version tag also publishes `omairc-*-windows-x64.zip` on the GitHub release. Portal text scale, desktop notifications, and the Omarchy theme watch stay Linux-only.
- `install.sh` adds the omairc pacman repository from the latest GitHub release and installs the package. Safe to re-run. Served from master and as a release asset for a version-pinned URL.

### Changed

- `/join` opens the last named channel as soon as the JOIN is sent, including `/j` and a Status submit. Multiple names land on the last one.
- Our CTCP `VERSION` reply no longer reveals the build number. It answers `https://omairc.app` instead.
- The scalable app icon puts the Omarchy mark behind the hash.

### Fixed

- `/query` clears the composer and focuses the new direct message. The command no longer stays as a draft on the source buffer or the new query. A refused `/query` stays in the composer, including on Status.
- A successful `/join` is consumed from the composer before the window switches, so it cannot remain as a draft on the previous conversation.
- A failed `/join` copies the server's reason into the selected channel transcript as well as Status.
- `/away` and `/back` mark our own nick away or online on every channel member row when `306` / `305` land, even when the server does not echo our own away-notify. The identity footer already had that fact; the panel and the nick picker now match it.
- A direct-message sidebar row's presence dot follows shared-channel membership and away facts (online, away, or unknown) instead of a hard-coded green mark. A peer who quits no longer looks available. The dot stays hidden when `away-notify` is off, like member rows.
- Windows CLI output works: the Windows build uses the console subsystem so `--help`, `--version`, and control commands print in a terminal or into a redirect, then `FreeConsole()` drops that console when opening the window. The local server uses a named pipe (`omairc`) instead of a drive-letter path that `QLocalServer` cannot listen on.

## [0.5.0] - 2026-09-16

### Added

- The sidebar identity footer shows the running app version on the right, next to the nick.
- `/ping [nick]`, `/time [nick]`, and `/version [nick]` send those CTCP queries. Empty form uses the open direct message. Replies copy into the asking conversation, like `/whois`. Status stays the log. Channel targets are refused. `--demo-server` answers those queries so the transcript row shows up without a live peer.
- Direct messages the user opened or replied to come back on the next connect. A query the user never touched stays session-local. Preferences has a global `Reopen direct messages on startup` toggle, on by default. Turning it off stops restore but still records engaged targets under `[openDirects]` in `omairc.conf`, so turning it back on can reopen them. `Ctrl+W` drops a query from that set. NickServ and other service queries are not restored. Replayed history stays muted, with no unread badge or notification. Restore waits until after ISUPPORT (`376` / `422`).
- Jump to a channel nick with `Ctrl+Shift+K`. Type filters the current channel's members, Up/Down highlight, Enter opens or creates that DM, Escape dismisses. The `ONLINE` heading opens the same sheet. Disabled on direct messages and Status.

### Changed

- Connect is tabbed. Connection holds the per-network profile. Preferences holds global settings such as reopening direct messages. Enter walks to the next field. `Ctrl+Enter` applies. The network rail owns Up/Down/Home/End and shows a `Ctrl+/` shortcut hint. Server password (PASS) and NickServ password are labeled apart, with the SASL fallback in the NickServ help. Closing the sheet returns focus to the composer.
- Direct-message typing sits in the transcript, in the slot the peer's next line will occupy. It groups with that peer's last message in the same minute; otherwise it shows their avatar. Channel typing stays on the member row.
- The member panel and `omairc names` list nicks by the server's `PREFIX` rank, highest first, then nick. Tab nick completion stays alphabetical.

## [0.4.0] - 2026-09-14

### Added

- `omairc read`, `omairc names`, and `omairc conversations` snapshot the running window as JSON. An agent can fetch recent chat, a joined-channel member list, and the channel and DM list without changing the UI selection. `read --unread` is a CLI cursor under `$XDG_STATE_HOME/omairc/cli-cursors/` and does not clear GUI unread or mention badges. `--since` and `--unread` keep the newest 100 lines and set `"truncated": true` when they drop older ones. The unread cursor tie-breaks same-millisecond lines with a per-conversation sequence so empty `msgid` tags do not drop later arrivals.
- Conversation transcripts persist under `$XDG_STATE_HOME/omairc/logs/{networkId}/{target}`. Join and opening a DM reload the tail as muted backlog. Lines that `IrcSecretPolicy` redacts are not written.
- Live transcripts insert a centered date mark when the local calendar day changes. The label is `Today`, `Yesterday`, or the locale short date. Status stays a raw console.
- Chat bodies render bold, italic, and underline. Topics, Status, find, and notifications stay stripped plain text.
- Click a chat or action author or avatar to open or create a DM, same as the member panel. Self, events, WHOIS, and grouped follow-up rows stay inert.
- Activating a desktop notification raises the window and opens that conversation. A missing DM is created the same way a member click is.
- `/highlight <word>`, `/unhighlight <word>`, and `/highlights`. Each network keeps its own list, persisted next to ignore. A whole-word hit increments mentions and notifies like a nick mention. Clearing the network drops the list.
- `/mute [target]`, `/unmute [target]`, and `/muted`. Each network keeps its own list by conversation key. Chat still arrives. Mentions do not notify, increment, or badge. `Alt+A` skips muted mentions. Opening the buffer does not unmute. `/close` and clearing the network drop the flag. Status `/mute` without a target is refused.
- `/disconnect [reason]` stops the focused network. `/quit` is the same command. Connect shows Disconnect next to Apply while that network is live or reconnecting. The profile, sidebar, and other networks stay.
- Empty `/join` joins the latest inbound invite for that network. Clicking the channel token on the Status `INVITE` line does the same. A new invite replaces the stored one. Ignore still drops invites from ignored nicks.
- A successful keyed self JOIN stores the key beside the Autojoin names. Welcome sends `JOIN #chan key`. Part and kick drop both. The Connect field stays names only.

### Changed

- New networks mint an 11-character base64url id instead of a UUID.

### Fixed

- The Ctrl+K jump field no longer overlaps its hint with the focused border. The hint stays inside the field as "Jump to conversation…" until you type.

## [0.3.0] - 2026-09-12

### Added

- IRCv3 `server-time`. Omairc requests the capability whenever the server advertises it, so a server that gates the `time` message tag behind it now sends real timestamps instead of folding them into the message body.
- Bouncer buffer replay on attach. A `znc.in/playback` batch now lands as backlog in muted text instead of as live traffic, so reattaching after a day away no longer raises unread counts or announces yesterday's mentions as new desktop notifications. A channel buffer splices above the join line. A query buffer opens its direct message only when the backlog holds a line somebody else wrote, so an outbound `/msg` that the bouncer kept in a buffer still opens nothing. Replay needs only the `batch` capability, which is what a bouncer offers.

### Changed

- `--demo-server` seeds the real `IrcController` in-process. `--mock` and the
  QML prototype sidebar are gone. `OmaircWindow` requires `irc`; a null
  binding no longer loads mock conversations.
- A sender Omairc could not reply to no longer opens a conversation or notifies. A bouncer's `*status` and its `***` playback markers stay on the Status console, where every inbound line is already logged. A nick starting with a digit is still a person and still opens a direct message.

### Fixed

- The Connect form scrollbar no longer covers the fields, and the card width stays the same.
- `Ctrl+F` find works on live conversation and Status transcripts. Empty first press enters find and waits. The composer shows Find. Matches include author and body, or Status label and text. The current row uses the selection color.

## [0.3.0alpha] - 2026-09-11

### Added

- Passwords stored through QtKeychain in the desktop Secret Service. If that service is unavailable, the secret stays session-only and Connect says so.
- Connect **NickServ** field beside Password. Password is connection `PASS` only, except the one-field SASL compatibility row. NickServ is SASL PLAIN when the server offers it, otherwise `IDENTIFY` after welcome and before autojoin. Both secrets use the Secret Service store. Forget and startup focus are independent.
- `/ignore <nick>`, `/unignore <nick>`, and `/ignored`. Each network keeps its own list. Private messages, notices, and invites from those nicks stay off Status and do not open a direct message. Channel text stays visible. Clearing the network drops its list.
- IRCv3 CHATHISTORY on channel join. After a successful self JOIN, Omairc sends `CHATHISTORY LATEST` when the server advertised `chathistory` or `draft/chathistory` plus `batch`. Replayed lines keep their original times and stay unread. The body uses muted text. A server that never offers the cap still starts empty.
- `/whois` replies copy into the asking conversation as wrapped event rows. A Status `/whois` stays on Status.
- Desktop notification for a direct message when the window is unfocused, even when the body has no nick.
- Connect sheet is keyboard-complete. Tab reaches Add network, Forget, Discard, and Apply. Enter Applies from fields, or from a selected network row on the second press. Overflowing lists show a scrollbar.
- Jump to a channel, direct message, or Status with `Ctrl+K`. Type to filter, Up/Down to highlight, Enter to jump, Escape to dismiss. Disabled while Connect is visible.
- Walk sidebar network headers with `Alt+Left` / `Alt+Right`. Enter opens that network's Status. `Ctrl+,` opens Connect for the focused header, including a network with no conversations.

### Changed

- A network drop keeps the session reconnecting with the existing backoff until `/quit` or Connect stops it. Status stays reconnecting.

### Fixed

- Channel topics and join, part, quit, and nick event rows render as plain text. Server-controlled markup no longer becomes rich text.
- QtKeychain `OtherError` is a storage error, not "unavailable".
- Connect automatically on startup does not start a network whose saved secret cannot be read. The password field is focused instead.
- First-open Connect focuses Nick or Host so Enter Applies. Discard undoes an unstored Add.
- Chat, action, and playback clocks use local time.
- Incoming NickServ PRIVMSG stays on Status and does not open a direct message.

## [0.2.0] - 2026-09-10

### Added

- Concurrent network connections. Add more networks from the Connect sheet. Each network keeps its own channels, direct messages, nick, Status, and connection state in the sidebar.
- Optional “Connect automatically on startup” per network (off by default).
- Source install script (`./bin/install`) that builds and installs a pacman package from the checkout.
- Single-process desktop: a second launch raises the existing window instead of starting another nick.
- Local CLI on the running client over `$XDG_RUNTIME_DIR/omairc.sock`: `connections`/`list`, `status`, `send`, and `raise`.
- `omairc --help` and `omairc <command> --help` print usage and exit without a window or a socket. Install ships bash completion for the same verbs.
- Warning and error log at `$XDG_STATE_HOME/omairc/omairc.log` (owner-only). Secondary raises and CLI help do not create the file.
- Per-conversation composer drafts, including Status. Enter still sends; Up/Down still recall sent lines.
- Find in the current buffer from the composer (`Ctrl+F`). Enter or `Ctrl+F` goes to the next match and wraps; Escape restores the draft.
- Clickable `http`/`https` links in chat and Status. `javascript:` and `file:` stay closed.
- Desktop notification for a mention when the window is unfocused.
- Typing indicator on an existing direct-message row when that peer is typing in the background.
- Multi-channel `/join` with optional keys. Bare names get a `#` prefix. Keys stay out of Status and logs.
- Status hides service IDENTIFY, OPER, and MODE keys the same way as PASS and JOIN. The wire still carries the real bytes.
- Direct CTCP `VERSION`, `PING`, and `TIME` replies (NOTICE), shown on Status. Channel broadcasts are not answered. Replies are rate-limited to once per nick per 5 seconds.
- Nick-in-use fallback during registration: on `433` before welcome, try `nick_` then `nick2` before failing the session.
- Reconnect as soon as the network is reachable again while waiting on the backoff timer.
- Idle-socket watchdog after welcome: ping a silent connection, then fail with Network and reconnect if it stays quiet.
- IRCv3 `time` tag on chat, action, and typing events when the value is valid ISO 8601.
- Latin-1 fallback for invalid UTF-8 inbound text. Outbound stays UTF-8.
- IRCv3 standard replies on Status: `FAIL` is Alert, `WARN` and `NOTE` are Info. They do not drop the session.
- Quiet IRCv3 capabilities when the server advertises them: `multi-prefix`, `chghost`, `cap-notify`, and `echo-message`.
- IRCv3 `BATCH` open and close. The `BATCH` line itself is swallowed; unknown batch types still pass inner messages through.

### Changed

- Consecutive messages from the same nick in the same minute hide the repeated avatar, name, and timestamp.
- Consecutive join, part, quit, and nick events collapse onto one transcript line. Kick and mode stay on their own rows.
- Each conversation transcript is capped at 2000 messages, oldest first out. `/clear` still empties the buffer.
- Inbound lines may exceed 512 bytes. Outbound stays at 512. A line that is still too long prints a named preview on Status.
- Event rows and the topic strip leftover mIRC codes the same way chat bodies do.
- Version lives in `version.pri` for `--version`, CTCP `VERSION`, tests, and packaging.

### Fixed

- `/msg` does not open a new direct message, including when the server echoes the line back with `echo-message`.
- CTCP requests no longer appear in conversation models; they go to Status.
- PRIVMSG targets that are empty, colon-prefixed, or contain whitespace or control characters are rejected.
- Away state, connection errors, command errors, drafts, passwords, and Status slash commands stay scoped to the focused network.
- Local IPC is same-user only, 64KiB per line, and bounded on client lifecycle and command responses. Dash-prefixed send text works. Raise does not fire twice.
- Ctrl+F jumps to the match instead of resticking the transcript to the bottom.
- GCC 16 QChar SFINAE warnings no longer fail the app build.

## [0.1.0] - 2026-09-07

First public release: a dead-simple IRC client for Omarchy.

- Connect sheet with a persisted network profile, TLS by default, and no saved password.
- Live session through `IrcController`, plus `--mock` for the bundled prototype window.
- Status console, member list with PREFIX ranks, away on the identity footer, and IRCv3 typing.
- Keyboard map, slash-command complete, selectable transcript, and follow-unseen.
- qmake Unix install tree and a GitHub Releases pacman repository.

[Unreleased]: https://github.com/fredimachado/omairc/compare/v0.6.0...HEAD
[0.6.0]: https://github.com/fredimachado/omairc/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/fredimachado/omairc/compare/v0.4.0...v0.5.0

[0.4.0]: https://github.com/fredimachado/omairc/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/fredimachado/omairc/compare/v0.3.0alpha...v0.3.0
[0.3.0alpha]: https://github.com/fredimachado/omairc/compare/v0.2.0...v0.3.0alpha
[0.2.0]: https://github.com/fredimachado/omairc/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/fredimachado/omairc/releases/tag/v0.1.0
