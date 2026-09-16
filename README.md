# Omairc

A dead-simple IRC client for Omarchy, built with Qt Quick and C++. Agents talk to the running window over `$XDG_RUNTIME_DIR/omairc.sock`. Humans use the same binary.

![Omairc in demo mode](omairc.gif)

## Features

- AI-friendly local CLI. While the window runs, the same binary prints one JSON line and exits: `connections`, `status`, `send`, `conversations`, `read`, `names`, `raise`. None of them change the selected conversation or clear the GUI badges.
- Several networks in one window. Each keeps its own channels, direct messages, nick, and connection state, plus a Status console that holds the handshake, AUTH, and errors.
- Connect sheet on first launch. Libera Chat defaults, TLS on, autojoin `#omarchy`. NickServ is SASL PLAIN when the server offers it, otherwise `IDENTIFY`. Nothing connects on its own unless you ask it to.
- Channels and direct messages. Member panel and people count on channels only. Click a nick to open a DM, `Ctrl+W` to close it.
- Keyboard first. `Ctrl+/` lists the shortcuts: `Ctrl+K` jump, `Ctrl+Shift+K` nick, `Alt+A` next unread, `Ctrl+F` find, Tab nick complete, Up/Down history, and drafts that stay with each conversation.
- Slash commands from the composer, with complete after `/`. The usual set plus `/ignore`, `/mute`, `/highlight`, and CTCP `/ping`, `/time`, and `/version`, which query a nick. Replies copy into the asking transcript, like `/whois`.
- Desktop notification for a mention or DM while the window is unfocused. Activating it raises the window and opens that conversation.
- Colors follow the current Omarchy theme and update live. Text follows the desktop size.
- Backlog on arrival. A joined channel asks for its last 100 lines over `CHATHISTORY`, a ZNC bouncer's `znc.in/playback` replay is folded in on attach, and a restart reloads the last 2000 lines from the local log as muted backlog.
- IRCv3 `sasl`, `sts`, `echo-message`, `server-time`, `multi-prefix`, `away-notify`, `message-tags`, `chghost`, `cap-notify`, `batch`, `chathistory`, and `znc.in/playback`. Reconnects with backoff.
- Profiles in `$XDG_CONFIG_HOME/omairc/`. Passwords and NickServ secrets go through QtKeychain into Secret Service; without that service they stay session-only and Connect says so. Logs and errors land in `$XDG_STATE_HOME/omairc/`.
- One process. A second launch raises the existing window.

## Limits

- SASL is PLAIN only, and Connect has no separate SASL account or bouncer-network fields.
- No DCC, file transfer, voice, video, plugins, or scripts.
- No Soju or ZNC history-sync protocol. Omairc asks for and accepts server history, but it does not drive a bouncer's own sync commands.
- The local CLI controls the running window. It is not a second IRC client.

## Install

On Omarchy or another Arch-based system:

```sh
git clone https://github.com/fredimachado/omairc.git
cd omairc
./bin/install
```

Run that as your regular user. `makepkg` calls `sudo` when it needs to.

Or install a [release package](https://github.com/fredimachado/omairc/releases/latest):

```sh
sudo pacman -U omairc-*.pkg.tar.zst
```

For updates through pacman, add:

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

Default `PREFIX` is `/usr/local`. Prove the staged tree with `bin/test-install`. `version.pri` is the only version string; tag a release as `v` plus that value. Arch `pkgver` cannot contain hyphens, so pre-releases use `0.4.0alpha` rather than `0.4.0-alpha`.

Omairc is MIT. See `LICENSE`. The IRC protocol code in `src/irc/` is LGPL-3.0-or-later. The bundled iA Writer Mono font is OFL-1.1.

## Agent skill

```sh
npx skills add fredimachado/omairc/skills -g
```

That installs the local CLI skill for this user (Cursor, Claude Code, Codex, and other agents the Skills CLI supports). The window must already be running. Preview with `--list`.

## Build

```sh
bin/build
./build/omairc
./build/omairc --demo-server
./build/omairc --help
./build/omairc send --network <id> '#channel' hello
./build/omairc read '#channel' --since 5m
./build/omairc read --unread
```

`--demo-server` seeds an in-process session and skips Connect. `--help` and `--version` print plain text and exit without a window; after a send target they are message text. Quote channel targets, since `#` starts a shell comment.

Control commands need a running window. With exactly one connection, `--network` may be omitted. Default `read` is `--last 50`, capped at 100; `--last`, `--since`, and `--unread` do not combine. `--unread` uses a CLI cursor under `$XDG_STATE_HOME/omairc/cli-cursors/` that agents on this machine share, separate from the GUI unread counts.

Set `OMAIRC_ALLOW_MULTI=1` if you need more than one normal process while debugging.

## Test

```sh
bin/test
bin/test-san
bin/test-install
bin/test-desktop
bin/test-live
```

`bin/test` is the default gate: conventions, build, CLI help and version, the C++ suite, and the offscreen QML tests. CI also runs `bin/test-san` (ASan and UBSan) and `bin/test-live`.

`bin/test-desktop` is optional and needs `xorg-server-xvfb`, `xorg-xauth`, `xdotool`, and `imagemagick`. `bin/test-live` is optional and needs Docker; it drives Ergo, Solanum, and ngIRCd on loopback, then the dual-network production-QML proof.
