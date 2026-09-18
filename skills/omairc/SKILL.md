---
name: omairc
description: >-
  Talks to a running Omairc IRC window over the local CLI (JSON on stdout).
  Use when sending or reading IRC, listing connections, conversations, or
  nicks, raising the window, or when the user mentions omairc, IRC, channels,
  or direct messages on this machine.
---

# Omairc CLI

Omairc is a desktop IRC client. While the window is running, the same
`omairc` binary prints one JSON line and exits. It is control of that
window, not a second IRC client.

JSON field lists: [reference.md](reference.md). Row fields omit empty
strings and `false` flags; treat absence as that default.

## Prerequisite

The GUI must already be running. If stdout is

```json
{"ok":false,"error":"Omairc is not running (no local socket). Start the client first."}
```

stop. Do not launch a window unless the user asked you to.

`--help` and `--version` print plain text and do not need a window.
Control commands need a window, print JSON on stdout, and use exit 0
only when `"ok": true`. If `send` prints `"uncertain": true` (exit 2), do
not retry `send`. Confirm with `read --last` on that target (own nick and
the same text). If present, stop; if absent, send once.

## Binary

Use `omairc` from `PATH`. In this source tree, `./build/omairc` after
`bin/build` on Linux and Windows. On macOS, qmake builds an app bundle:
`./build/omairc.app/Contents/MacOS/omairc`. Do not speak the local socket
yourself.

Before inventing flags, run `omairc --help` and `omairc <command> --help`.

## Workflow

1. `omairc connections` (alias `list`).
2. If there is not exactly one row, pass `--network <id>` on later
   commands. Omit `--network` only when there is exactly one connection.
3. `omairc conversations` and `omairc names '#channel'` to see targets.
4. `omairc send` / `omairc read` against a target that already exists.

`send`, `read`, `names`, and `conversations` do not change the selected
conversation. Do not `raise` unless the user wants the window.

## Commands

| Command | Role |
|---|---|
| `connections` / `list` | Networks: `id`, `host`, `port`, `tls`, `nick`, `state`, `selected` |
| `status [--network ID]` | One connection |
| `conversations [--network ID]` | Channels and DMs with GUI `unread`/`mention` badges and channel `topic` (does not clear badges) |
| `names [--network ID] TARGET` | Joined-channel member panel snapshot |
| `send [--network ID] TARGET TEXT...` | PRIVMSG without changing UI selection |
| `read [--network ID] [TARGET] [--last N\|--since DURATION\|--unread]` | Chat snapshot (`message`, `notice`, `action`) |
| `raise` | Activate the existing window |

`read` with no target is every channel and DM on that network. A named
target that is not in the window is an error. `read` does not invent
conversations.

`names` is the member panel, not a live NAMES round-trip. Rows come back in
panel order: highest server `PREFIX` rank first, then nick. A DM, Status,
or a channel that is not joined is an error.

Connection `state` values: `Offline`, `Connecting`, `Connected`,
`Disconnecting`, `Reconnecting`.

## Shell

Quote channel targets. `#` starts a shell comment. Quote send text when
it contains spaces. After the send target, `--help` and `--version` are
message text. Use `--` when the target or text looks like a flag.

```sh
omairc connections
omairc status
omairc send '#channel' hello
omairc send '#channel' 'hello there'
omairc send --network abc '#channel' hello
omairc send '#channel' --help
omairc send -- '#channel' --version
omairc conversations
omairc names '#channel'
omairc names -- --dash-nick
omairc read --last 20
omairc read '#channel' --since 5m
omairc read --unread
omairc raise
```

## `read` windows

Give exactly one of `--last`, `--since`, or `--unread`. They do not
combine. Default is `--last 50`. `--last` over 100 is an error.
`--since` and `--unread` keep the newest 100 and set `"truncated": true`
when they drop older lines.

`--since` examples: `5m`, `1h`. Units are `s`, `m`, `h`, `d`.

`--unread` uses a CLI cursor under `omairc/cli-cursors/` in the platform
state location (`$XDG_STATE_HOME` overrides when set). It does not change
the selected conversation or the GUI unread and mention counts. Two agents
on this machine share one cursor per network, or per network and target.

## Local socket and secrets

The running window listens on a filesystem socket named `omairc.sock` under
Qt `RuntimeLocation` (on macOS this is usually under `$TMPDIR`, not your
home directory). Network passwords use the platform secret store (Keychain
on macOS, libsecret on Linux, Credential Manager on Windows).

## Do not

- Type slash commands (`/msg`, `/join`, …) through `send`. Use `send`
  for channel or nick text.
- Drive the GUI with xdotool or click recipes.
- Invent conversations, nicks, or network ids.
- Scrape the window. Parse the JSON line.
- Pass `--network` with the host name. Use `connections` `id`.
