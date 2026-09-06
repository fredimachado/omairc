# Omairc

A dead-simple IRC client for Omarchy, built with Qt Quick and C++.

![Omairc interface](omairc.png)

## Connect

The first launch opens a connection sheet with Libera Chat defaults. Enter a
nick, change the host if you want a different network, and type a server
password only if that network needs one. Apply starts the session. TLS is on
by default.

Omairc remembers host, port, TLS, nick, username, real name, and autojoin in
`$XDG_CONFIG_HOME/omairc/`. It does not write the password. Type the password
again on the next launch if the server asks for it. Press `Ctrl+,` or click
`edit` beside the network name to reopen the sheet. The network name itself
opens Status.

Press `Ctrl+/` for keyboard shortcuts.

Colors follow the current Omarchy theme and update live. Text follows the
desktop text size.

## Limits

- One network at a time.
- No saved message history.
- No DCC, file transfer, voice, or video.
- No plugins or scripts.
- No bouncer-specific history synchronization.

## Build

```sh
bin/build
./build/omairc
./build/omairc --mock
./build/omairc --version
```

`--mock` skips Connect and any saved profile, and opens the bundled prototype
conversations instead.

`--version` prints `omairc 0.1.0` and exits without opening a window.

## Install

On Arch, once the AUR package is published:

```sh
yay -S omairc
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
