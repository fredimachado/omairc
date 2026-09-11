# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Connect **NickServ** field beside Password. Password is connection `PASS` only, except the one-field SASL compatibility row. NickServ is SASL PLAIN when the server offers it, otherwise `IDENTIFY` after welcome and before autojoin. Both secrets use the existing Secret Service store. Forget and startup focus are independent.
- `/ignore <nick>`, `/unignore <nick>`, and `/ignored`. Each network keeps its own list. Private messages, notices, and invites from those nicks stay off Status and do not open a direct message. Channel text stays visible. Clearing the network drops its list.
- IRCv3 CHATHISTORY on channel join. After a successful self JOIN, Omairc sends `CHATHISTORY LATEST` when the server advertised `chathistory` or `draft/chathistory` plus `batch`. Replayed lines keep their original times and stay unread. The body uses muted text. A server that never offers the cap still starts empty.
- Desktop notification for a direct message when the window is unfocused, even when the body has no nick.
- Walk sidebar network headers with `Alt+Left` / `Alt+Right`. Enter opens that network's Status. `Ctrl+,` opens Connect for the focused header, including a network with no conversations.
- IRCv3 `server-time`. Omairc requests the capability whenever the server advertises it, so a server that gates the `time` message tag behind it now sends real timestamps instead of folding them into the message body.
- Optional Account and Bouncer network fields in the Connect sheet. SASL PLAIN logs in with the account when it is set and falls back to the nick, and a bouncer network is appended after a slash so the bouncer attaches the right upstream network. Neither field is a secret, so both are saved with the profile.

### Changed

- A network drop keeps the session reconnecting with the existing backoff until `/quit` or Connect stops it. Status stays reconnecting.

### Fixed

- Channel topics and join, part, quit, and nick event rows render as plain text. Server-controlled markup no longer becomes rich text.
- QtKeychain `OtherError` is a storage error, not "unavailable".
- Connect automatically on startup does not start a network whose saved secret cannot be read. The password field is focused instead.

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

[Unreleased]: https://github.com/fredimachado/omairc/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/fredimachado/omairc/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/fredimachado/omairc/releases/tag/v0.1.0
