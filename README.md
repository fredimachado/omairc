# Omairc

A simple IRC client for Omarchy, built with Qt Quick and C++. The same binary is the window and a local CLI.

![Omairc in demo mode](omairc.gif)

## Features

- Several networks in one window. Each keeps its own channels, direct messages, nick, and Status console.
- Connect sheet on first launch. Libera Chat, TLS on, autojoin `#omarchy`. NickServ uses SASL PLAIN when the server offers it, otherwise `IDENTIFY`.
- Member list and people count on channels. Click a nick to open a direct message. `Ctrl+W` closes it.
- Avatars, a status line, and a bot mark when the network supports them.
- Keyboard first. `Ctrl+/` lists the shortcuts.
- Slash commands from the composer, including `/list`, `/pref`, `/status`, `/avatar`, `/autoaway`, and CTCP.
- Notification for a mention or direct message while the window is unfocused.
- Colors follow the Omarchy theme and update live. Text follows the desktop size.
- Backlog on join, and recent local lines after a restart.
- Local CLI on the running window: `connections`, `status`, `send`, `conversations`, `read`, `names`, `raise`.
- A second launch raises the existing window.

## Limits

- SASL is PLAIN only. Connect has no separate SASL account or bouncer-network fields.
- No DCC, file transfer, voice, video, plugins, or scripts.
- No Soju or ZNC history-sync protocol.
- The local CLI controls the running window. It is not a second IRC client.

## Install

On Omarchy or another Arch-based system:

```sh
curl -fsSL https://raw.githubusercontent.com/fredimachado/omairc/master/install.sh | sh
```

That adds the `[omairc]` pacman repository and installs the latest release. Re-running upgrades. Read [`install.sh`](install.sh) first if you prefer. The same script is a release asset:

```sh
curl -fsSL https://github.com/fredimachado/omairc/releases/latest/download/install.sh | sh
```

Later upgrades:

```sh
sudo pacman -Sy omairc
```

On Omarchy you can also run `omarchy pkg add omairc`.

From source:

```sh
git clone https://github.com/fredimachado/omairc.git
cd omairc
./bin/install
```

Run that as your user. `makepkg` calls `sudo` when it needs to.

Or install a [release package](https://github.com/fredimachado/omairc/releases/latest) directly:

```sh
sudo pacman -U omairc-*.pkg.tar.zst
```

Depends on `qt6-base`, `qt6-declarative`, `qt6-svg`, `qt6-wayland`, `qtkeychain-qt6`, and `xdg-desktop-portal`.

Packagers stage with qmake `INSTALL_ROOT`:

```sh
qmake6 PREFIX=/usr
make
make INSTALL_ROOT="$pkgdir" install
```

Default `PREFIX` is `/usr/local`. `version.pri` is the only version string. Tag a release as `v` plus that value.

Omairc is MIT. See `LICENSE`. IRC protocol code in `src/irc/` is LGPL-3.0-or-later. The bundled iA Writer Mono font is OFL-1.1.

## Agent skill

```sh
npx skills add fredimachado/omairc/skills -g
```

Installs the local CLI skill. The window must already be running.

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

`--demo-server` skips Connect. `--help` and `--version` exit without a window. Quote channel targets. Control commands need a running window. With one connection, `--network` may be omitted.

## Test

```sh
bin/test
bin/test-san
bin/test-install
bin/test-desktop
bin/test-live
```

`bin/test` is the default gate. CI also runs `bin/test-san` and `bin/test-live`.

`bin/test-desktop` needs Xvfb, Xauthority, xdotool, and ImageMagick. `bin/test-live` needs Docker.

## Windows

A native Windows build works. It is not the focus.

Each version tag publishes `omairc-*-windows-x64.zip` and `omairc-*-windows-x64-setup.exe` on the [GitHub release](https://github.com/fredimachado/omairc/releases/latest). The installer is per-user by default. Unzip and run `omairc.exe` for a portable tree.

```bat
bin\build.bat
build\release\omairc.exe
build\release\omairc.exe --demo-server
build\release\omairc.exe --help
bin\package-windows.bat
```

`bin\build.bat` finds a Qt 6 kit, builds into `build\release\`, and runs `windeployqt`. QtKeychain must be installed for that kit. `bin\package-windows.bat` needs Inno Setup 6.

The same exe is the local CLI. Start the window first. `--help` and control commands print from cmd or PowerShell.

Portal text scale, desktop notifications, and the Omarchy theme watch are Linux-only. Profiles use the Qt app config location.

## macOS

A native macOS build works. It is not the focus.

```sh
brew tap fredimachado/omairc https://github.com/fredimachado/omairc
brew install --cask fredimachado/omairc/omairc
```

The tap URL is required because this repository is not `homebrew-omairc`. That installs the app and puts `omairc` on `PATH`. The window still has to be running. `brew uninstall --cask --zap omairc` wipes profiles and logs.

Once Homebrew carries the cask:

```sh
brew install --cask omairc
```

Each version tag also publishes `omairc-*-macos-arm64.zip` and `omairc-*-macos-x64.zip` on the [GitHub release](https://github.com/fredimachado/omairc/releases/latest).

From the repo:

```sh
brew install qt@6 qtkeychain cmake
export PATH="$(brew --prefix qt@6)/bin:$PATH"
bin/build-macos
open build/omairc.app
```

Passwords use the macOS Keychain. Profiles land in `~/Library/Preferences/omairc`.
