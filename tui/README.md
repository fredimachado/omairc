# Omairc TUI

The Go terminal client. Same behavior, chords, and window title as the Qt app.

![Omairc TUI in demo mode](omairc-tui.gif)

## Install

Arch, after the omairc pacman repository is configured. `install.sh` installs the Qt client only:

```sh
sudo pacman -S omairc-tui
```

Go 1.25 or newer. `@vX.Y.Z` is the module version; the git tag on that same commit is `tui/vX.Y.Z`:

```sh
go install github.com/fredimachado/omairc/tui/cmd/omairc-tui@vX.Y.Z
```

Homebrew:

```sh
brew tap fredimachado/omairc https://github.com/fredimachado/omairc
brew install fredimachado/omairc/omairc-tui
```

Scoop, or unzip `omairc-tui-*-windows-amd64.zip` from the GitHub release and run `omairc-tui.exe` in Windows Terminal:

```sh
scoop bucket add omairc https://github.com/fredimachado/omairc
scoop install omairc-tui
```

## Build

```sh
tui/bin/build
tui/bin/omairc-tui
tui/bin/omairc-tui --demo-server
```

`--demo-server` seeds the two-network demo world and skips Connect.

## Test

```sh
tui/bin/test
```

Contributor notes are in [AGENTS.md](AGENTS.md).
