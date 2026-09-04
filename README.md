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
```

`--mock` skips Connect and any saved profile, and opens the bundled prototype
conversations instead.

## Test

```sh
bin/test
bin/test-desktop
bin/test-live
```

`bin/test-desktop` is optional. On Arch/Omarchy it needs `xorg-server-xvfb`,
`xorg-xauth`, `xdotool`, and `imagemagick`.

`bin/test-live` is optional and needs Docker. It starts Ergo, Solanum, and
ngIRCd on loopback and is not part of `bin/test`.

## Requirements

- Qt 6: `qt6-base`, `qt6-declarative`
- `xdg-desktop-portal` and a portal backend

The iA Writer Mono font is bundled under the SIL Open Font License 1.1; see
`fonts/OFL.txt`.
