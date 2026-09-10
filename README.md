# Omairc

A dead-simple IRC client for Omarchy, built with Qt Quick and C++.

![Omairc interface](omairc.png)

## Connect

The first launch opens a connection sheet with Libera Chat defaults. Enter a
nick, change the host if you want a different network, and type a server
password only if that network needs one. Apply starts the session. TLS is on
by default. Omairc does not connect on its own by default; enable Connect
automatically on startup when you want a session on launch.

Omairc remembers host, port, TLS, nick, username, real name, and autojoin in
`$XDG_CONFIG_HOME/omairc/`. Connect automatically on startup is off by
default and can be enabled in the Connect sheet. Omairc does not write the
password. Type the password again when you Apply if the server needs it.
Press `Ctrl+,` or click `edit` beside the network name to reopen the sheet.
The network name itself opens Status.

Press `Ctrl+/` for keyboard shortcuts.

Colors follow the current Omarchy theme and update live. Text follows the
desktop text size.

## Limits

- One network at a time.
- One process. A second launch raises the existing window.
- Local CLI control of the running client over the same runtime socket
  (`connections`, `status`, `send`, `raise`). This is not a second IRC client.
- No saved message history.
- No DCC, file transfer, voice, or video.
- No plugins or scripts.
- No bouncer-specific history synchronization.

## Build

```sh
bin/build
./build/omairc
./build/omairc --mock
./build/omairc --help
./build/omairc --version
./build/omairc connections
./build/omairc status
./build/omairc send --help
./build/omairc send --network <id> '#channel' hello
./build/omairc raise
```

`--mock` skips Connect and any saved profile, and opens the bundled prototype
conversations instead. Mock mode also skips the single-process guard so tests
can run more than one window.

`--help` lists the GUI flags and control commands. `<command> --help` prints
that command's usage. Both write plain text and exit without a window or a
socket. After the send target, `--help` and `--version` are message text.

`--version` prints `omairc 0.1.0` and exits without opening a window.

While a normal Omairc window is running, the same binary can talk to it over
`$XDG_RUNTIME_DIR/omairc.sock` (TempLocation fallback). Control commands print
JSON on stdout and exit. They do not start a window. If nothing is listening,
they exit non-zero with a clear error.

`connections` (alias `list`) returns each session's stable `id` (`networkId`),
host, port, tls, nick, state, and whether it is the UI-selected network.
`status` and `send` take optional `--network <id>`. With exactly one
connection, `--network` may be omitted. With zero or more than one, omit is an
error and the message points at `connections`.

`send` delivers to an explicit channel or nick on that network without
changing the UI selection. `raise` activates the existing window (same effect
as a plain second launch).

Set `OMAIRC_ALLOW_MULTI=1` to allow more than one normal process while
debugging.

## Install

On Omarchy or another Arch-based system, clone the repository and run the
installer:

```sh
git clone https://github.com/fredimachado/omairc.git
cd omairc
./bin/install
```

The installer builds a pacman package from the checkout and installs its
dependencies. Run it as your regular user; `makepkg` invokes `sudo` when
needed.

Or download `omairc-*-x86_64.pkg.tar.zst` from the
[latest release](https://github.com/fredimachado/omairc/releases/latest) and
install it:

```sh
sudo pacman -U omairc-*.pkg.tar.zst
```

To receive Omairc updates through pacman, add the release repository:

```ini
[omairc]
SigLevel = Optional TrustAll
Server = https://github.com/fredimachado/omairc/releases/latest/download
```

```sh
sudo pacman -Sy omairc
```

On Omarchy:

```sh
omarchy pkg add omairc
```

That package depends on `qt6-base`, `qt6-declarative`, `qt6-svg`,
`qt6-wayland`, and `xdg-desktop-portal`.

Packagers stage with qmake `INSTALL_ROOT`, not `DESTDIR`:

```sh
qmake6 PREFIX=/usr
make
make INSTALL_ROOT="$pkgdir" install
```

Default `PREFIX` is `/usr/local`. Packagers pass `PREFIX=/usr`. Prove the
staged tree with `bin/test-install`. That command does not write `./build`.
Install also ships bash completion at
`share/bash-completion/completions/omairc`.

Installed license texts are MIT (`LICENSE`), LGPL-3.0-or-later
(`COPYING-LGPL`, from `src/irc/COPYING`), and OFL-1.1 (`OFL.txt`).

## Test

```sh
bin/test
bin/test-desktop
bin/test-live
bin/test-install
```

`bin/test-desktop` is optional. On Arch/Omarchy it needs `xorg-server-xvfb`,
`xorg-xauth`, `xdotool`, and `imagemagick`.

`bin/test-live` is optional and needs Docker. It starts Ergo, Solanum, and
ngIRCd on loopback and is not part of `bin/test`.

## Requirements

- Qt 6: `qt6-base`, `qt6-declarative`, `qt6-svg`, `qt6-wayland`
- `xdg-desktop-portal` and a portal backend

Omairc is MIT. See `LICENSE`. The IRC protocol code in `src/irc/` is
LGPL-3.0-or-later. See `src/irc/COPYING`. The bundled iA Writer Mono font is
OFL-1.1. See `fonts/OFL.txt`.
