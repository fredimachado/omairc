# Omairc

A dead-simple IRC client for Omarchy, built with Qt Quick and C++.

![Omairc interface](omairc.png)

Omairc currently runs as an interactive visual prototype. Networks, channels,
people, and conversations can be supplied by the IRC subsystem through
`IrcController`. The transport supports plain TCP and TLS.

## First-release IRC scope

The first release supports one configured network at a time. TLS is enabled by
default. A network profile contains the nick, username, real name, and optional
server password. IRCv3 capability negotiation and optional SASL PLAIN
authentication are supported.

Sessions support joining and leaving channels, channel and direct messages,
`/me` actions, topics, member lists, nick changes, joins, parts, quits, kicks,
basic mode events, actionable connection errors, and controlled reconnection.

The first release does not include:

- Multiple simultaneous networks.
- DCC, file transfer, voice, or video.
- Bouncer-specific history synchronization.
- Plugins or scripts.
- A comprehensive raw-command interface.
- Persistent local message history.

## Configuration

The first launch opens a connection sheet with Liberachat defaults. Enter a
nick, change the host if you want a different network, and type a server
password only if that network needs one. Apply starts the session.

Omairc remembers host, port, TLS, nick, username, real name, and autojoin in
`$XDG_CONFIG_HOME/omairc/omairc.conf` under `networks/<id>/`. It does not write
the password. Type the password again on the next launch if the server asks for
it. Click the sidebar network name to reopen the sheet.

Automated tests still inject an `IrcSession` through `IrcController::addSession()`
and do not construct `IrcConnection`.

## Keyboard controls

- Walk channels then direct messages with `Alt+Down` and `Alt+Up`. Click a sidebar row to jump.
- Jump to the next unread with `Alt+A`, mentions first.
- Click a channel member, or press Enter on a focused member, to open or create a direct message.
- Send messages with `Enter`.
- Scroll the transcript with `Page Up` and `Page Down`. The composer stays focused.
- Complete a nick with `Tab` in the composer.
- Recall sent lines with `Up` and `Down` in the composer.
- Toggle Status with `Ctrl+``.
- Open Connect with `Ctrl+,`.
- Toggle the channel member list with `Ctrl+Shift+M`.
- Focus the channel member list with `Ctrl+Shift+P`. The panel reopens if it was hidden.
- Focus the message composer with `Ctrl+L`.
- List these shortcuts with `Ctrl+/`.
- Dismiss Status, Connect, or the shortcut list with Escape.
- Quit with `Ctrl+Q`.

Colors follow the current Omarchy theme
(`~/.local/state/omarchy/current/theme/colors.toml`) and update live. Text
follows the desktop text size.

## Build

```sh
bin/build
./build/omairc
```

## Test

```sh
bin/test
```

`bin/test` builds and runs the C++ protocol, session, model, and loopback
transport suites. It also runs the UI suite.

The UI suite loads the production QML in an isolated temporary environment,
renders it with Qt's offscreen software backend, and controls it with real
mouse and keyboard events. It verifies channel switching, message sending,
keyboard shortcuts, and opening direct messages from the member list.
Screenshots from each workflow are written to `test-artifacts/` for visual
inspection by people or AI agents.

For a black-box check of the compiled executable, install the free
`xorg-server-xvfb`, `xorg-xauth`, `xdotool`, and `imagemagick` packages, then
run:

```sh
bin/test-desktop
```

This starts an access-controlled virtual display, launches the real
`build/omairc` executable with temporary settings and no host desktop portal,
and drives it using external mouse and keyboard events. It never sends input
to the active desktop. The runner checks visible changes between screenshots;
logs and screenshots are written to `test-artifacts/desktop/`.

## Requirements

- Qt 6: `qt6-base`, `qt6-declarative`
- `xdg-desktop-portal` and a portal backend

The iA Writer Mono font is bundled under the SIL Open Font License 1.1; see
`fonts/OFL.txt`.
