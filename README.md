# Omairc

A dead-simple IRC client for Omarchy, built with Qt Quick and C++.

![Omairc interface](omairc.png)

## Features

- Several networks in one window. Each keeps its own channels, direct messages, nick, and connection state.
- Connect sheet on first launch. Libera Chat defaults, TLS on, autojoin `#omarchy`. Password is the connection `PASS`. NickServ is SASL PLAIN when the server offers it, otherwise `IDENTIFY` after welcome. Apply starts the session. Nothing connects on its own unless you turn on Connect automatically on startup.
- `Ctrl+,` or `edit` beside a network name reopens Connect. The network name opens Status.
- Status console per network. Handshake, errors, and AUTH stay there, not in DIRECT MESSAGES.
- Channels and DMs. Member panel and people count on channels only. Click a nick to open or create a DM. `Ctrl+W` or `/close` closes a DM.
- Presence dots and away dimming when the server grants `away-notify`. Status lines need `draft/metadata-2` and `batch`. Typing ellipsis needs `message-tags`. Member ranks follow the server `PREFIX`.
- Keyboard first. `Ctrl+/` lists the shortcuts. Walk conversations and networks, `Ctrl+K` jump, `Alt+A` next unread, `Ctrl+F` find, Tab nick complete, Up/Down history, drafts that stay with each conversation.
- Slash commands from the same single-line composer, with complete after `/`. Catalog is `/me`, `/join` (`/j`), `/part` (`/leave`), `/nick`, `/quit`, `/clear`, `/close`, `/query`, `/msg`, `/topic`, `/notice`, `/away`, `/back`, `/whois`, `/mode`, `/kick`, `/invite`, `/ignore`, `/unignore`, `/ignored`, `/op`, `/deop`, `/voice`, `/devoice`, `/ban`, `/ns`, `/cs`, `/raw` (`/quote`), and `/help`.
- `/ignore` hides private messages, notices, and invites from that nick and persists per network. Channel text stays visible.
- Desktop notification for an unfocused mention or DM. Focused window stays quiet.
- Clickable `http` and `https` links in chat and Status. Other schemes do nothing.
- Colors follow the current Omarchy theme and update live. Text follows the desktop size.
- Profiles in `$XDG_CONFIG_HOME/omairc/`. Passwords and NickServ secrets go through QtKeychain into Secret Service. If that service is missing, those secrets stay session-only and Connect says so. If a saved secret cannot be returned, Connect automatically on startup leaves that network disconnected. Errors go to `$XDG_STATE_HOME/omairc/omairc.log`.
- One process. A second launch raises the existing window. While that window is running, the same binary can send `connections`, `status`, `send`, and `raise` over `$XDG_RUNTIME_DIR/omairc.sock`.
- IRCv3 `sasl`, `sts`, `echo-message`, `server-time`, `multi-prefix`, `chghost`, `cap-notify`, `batch`, `chathistory`, and `znc.in/playback`. Reconnects with backoff. Answers CTCP VERSION.

## Limits

- Network sections stay expanded. Scroll the sidebar when they overflow.
- No saved message history. A server that advertises `chathistory` and `batch` can fill a joined channel with its last 100 lines. A bouncer that volunteers `znc.in/playback` can replay on attach. There is no Soju or ZNC history-sync protocol.
- Connect has no separate SASL account or bouncer-network fields. NickServ is the services password.
- SASL is PLAIN only.
- No DCC, file transfer, voice, or video.
- No plugins or scripts.
- The local CLI is control of the running window, not a second IRC client.

## Install

On Omarchy or another Arch-based system:

```sh
git clone https://github.com/fredimachado/omairc.git
cd omairc
./bin/install
```

Run that as your regular user. `makepkg` calls `sudo` when it needs to.

Or install a release package:

```sh
sudo pacman -U omairc-*.pkg.tar.zst
```

[Latest release](https://github.com/fredimachado/omairc/releases/latest).

To get updates through pacman, add:

```ini
[omairc]
SigLevel = Optional TrustAll
Server = https://github.com/fredimachado/omairc/releases/latest/download
```

```sh
sudo pacman -Sy omairc
```

On Omarchy you can also run `omarchy pkg add omairc`.

Depends on `qt6-base`, `qt6-declarative`, `qt6-svg`, `qt6-wayland`, `qtkeychain-qt6`, and `xdg-desktop-portal`.

Packagers stage with qmake `INSTALL_ROOT`, not `DESTDIR`:

```sh
qmake6 PREFIX=/usr
make
make INSTALL_ROOT="$pkgdir" install
```

Default `PREFIX` is `/usr/local`. Prove the staged tree with `bin/test-install`. That command does not write `./build`. Install also ships bash completion at `share/bash-completion/completions/omairc`.

`version.pri` is the only version string. Tag a release as `v` plus that value. Arch `pkgver` cannot contain hyphens, so pre-releases use `0.4.0alpha` rather than `0.4.0-alpha`.

Omairc is MIT. See `LICENSE`. The IRC protocol code in `src/irc/` is LGPL-3.0-or-later. The bundled iA Writer Mono font is OFL-1.1.

## Build

```sh
bin/build
./build/omairc
./build/omairc --demo-server
./build/omairc --help
./build/omairc --version
./build/omairc connections
./build/omairc status
./build/omairc send --help
./build/omairc send --network <id> '#channel' hello
./build/omairc raise
```

`--demo-server` seeds an in-process session and skips Connect. `--help` and `--version` print plain text and exit without a window. After the send target, `--help` and `--version` are message text.

Control commands need a running window. They print JSON on stdout and exit. With exactly one connection, `--network` may be omitted. With zero or more than one, omit is an error and the message points at `connections`. `send` does not change the UI selection.

Set `OMAIRC_ALLOW_MULTI=1` if you need more than one normal process while debugging.

## Test

```sh
bin/test
bin/test-san
bin/test-install
bin/test-desktop
bin/test-live
```

`bin/test` is the default gate. It checks conventions, builds, checks CLI help and version, then runs the C++ suite and the offscreen QML tests. CI also runs `bin/test-san` and `bin/test-live`.

`bin/test-san` rebuilds the C++ suite with ASan and UBSan.

`bin/test-desktop` is optional. On Arch or Omarchy it needs `xorg-server-xvfb`, `xorg-xauth`, `xdotool`, and `imagemagick`. It launches `./build/omairc --demo-server` as a black box.

`bin/test-live` is optional and needs Docker. It starts Ergo, Solanum, and ngIRCd on loopback, runs the protocol suite, then the dual-network production-QML proof.
