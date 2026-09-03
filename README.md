# Omairc

A dead-simple IRC client for Omarchy, built with Qt Quick and C++.

![Omairc interface](omairc.png)

Omairc currently runs as an interactive visual prototype. Networks, channels,
people, and conversations are mocked locally; it does not open a socket or
connect to an IRC server yet.

## Prototype interactions

- Switch between channels and direct messages from the sidebar.
- Click a channel member to open or create a local direct message.
- Send local messages with `Enter`.
- In channels, toggle the member list with `Ctrl+Shift+M`.
- Focus the message composer with `Ctrl+L`.
- Quit with `Ctrl+Q`.

Colors follow the current Omarchy theme
(`~/.local/state/omarchy/current/theme/colors.toml`) and update live. Text
follows the desktop text size.

## Build

```sh
bin/build
./build/omairc
```

## Requirements

- Qt 6: `qt6-base`, `qt6-declarative`
- `xdg-desktop-portal` and a portal backend

The iA Writer Mono font is bundled under the SIL Open Font License 1.1; see
`fonts/OFL.txt`.
