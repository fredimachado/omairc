# Omairc

A dead-simple IRC client for Omarchy, built with Qt Quick and C++.

![Omairc interface](omairc.png)

## Connect

The first launch opens a connection sheet with Libera Chat defaults. Enter a
nick, change the host if you want a different network, and type a server
password only if that network needs one. Apply starts the session. TLS is on
by default.

Omairc remembers host, port, TLS, nick, username, real name, and autojoin in
`$XDG_CONFIG_HOME/omairc/` on Linux, or `HKCU\Software\omairc\omairc` on
Windows. It does not write the password. Type the password again on the next
launch if the server asks for it. Press `Ctrl+,` or click `edit` beside the
network name to reopen the sheet. The network name itself opens Status.

Press `Ctrl+/` for keyboard shortcuts.

Colors follow the current Omarchy theme and update live. When that theme file
is missing, Omairc uses a built-in dark or light palette. Text follows the
desktop text size on Linux. On Windows the text scale stays 1.0 and the window
icon stays empty.

## Limits

- One network at a time.
- No saved message history.
- No DCC, file transfer, voice, or video.
- No plugins or scripts.
- No bouncer-specific history synchronization.

## Build

Linux:

```sh
bin/build
./build/omairc
./build/omairc --mock
```

`--mock` skips Connect and any saved profile, and opens the bundled prototype
conversations instead.

Windows uses a Qt 6 kit with Quick, Quick Controls 2, and Network. DBus is not
linked. Open `omairc.pro` in Qt Creator, or from a shadow directory:

```bat
qmake ..\omairc.pro
nmake
rem or jom, or mingw32-make
```

Run `omairc.exe --mock` and `omairc.exe` from that directory. Qt Creator puts
Qt on `PATH`. For a copied tree, run `windeployqt omairc.exe`, then copy
OpenSSL next to the exe (`libssl-3-x64.dll` / `libcrypto-3-x64.dll` from the
Qt OpenSSL tools, matching the kit) so `tls/qopensslbackend.dll` can load.
Libera Chat on 6697 needs that when the kit is OpenSSL-backed (typical MinGW,
and also when Schannel is absent).

## Test

Linux:

```sh
bin/test
bin/test-desktop
bin/test-live
```

`bin/test-desktop` is optional and Linux-only. On Arch/Omarchy it needs
`xorg-server-xvfb`, `xorg-xauth`, `xdotool`, and `imagemagick`.

`bin/test-live` is optional and needs Docker. It starts Ergo, Solanum, and
ngIRCd on loopback and is not part of `bin/test`.

On Windows, `qmake tests/tests.pro` plus nmake, jom, or mingw32-make builds
and runs `protocol_tests`. The UI suite stays on the Linux `bin/test` path.

## Requirements

Linux:

- Qt 6: `qt6-base`, `qt6-declarative`
- `xdg-desktop-portal` and a portal backend

Windows:

- A Qt 6 kit with Quick, Quick Controls 2, and Network. DBus is not used.

The iA Writer Mono font is bundled under the SIL Open Font License 1.1; see
`fonts/OFL.txt`.
