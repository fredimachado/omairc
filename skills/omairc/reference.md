# CLI JSON

Every control command prints one compact JSON object on stdout.

Success has `"ok": true`. Failure has `"ok": false` and `"error"`.
`--help` and `--version` are plain text, not JSON.

Row fields omit empty strings and `false` flags. Treat a missing flag as
`false` and a missing string as empty. Numbers such as `unread` and `port`
stay even when zero. Top-level `"ok": false` on errors is kept.

## Envelope

```json
{"ok":true}
{"ok":false,"error":"Omairc is not running (no local socket). Start the client first."}
{"ok":false,"uncertain":true,"error":"No response from Omairc after send; the message may already have been delivered"}
```

`send` and `raise` succeed with `{"ok":true}` only.

`send` may return `"uncertain": true` (exit 2) after a successful local-socket
write of the send request when the CLI does not get a readable reply. That is
not proof the window ran `sendToTarget`. Do not retry `send`. Confirm with
`read --last` on that target using own nick and the same text; a peer can send
identical text. If present, stop; if absent, send once.

Zero connections: `"No connections are available."`
More than one connection without `--network`: `"Multiple connections are available; specify a network id."`
Unknown `--network`: `"Unknown network id '<id>'."`

## `connections`

```json
{"ok":true,"connections":[{"id":"…","host":"irc.libera.chat","port":6697,"tls":true,"nick":"fred","state":"Connected","selected":true}]}
```

`lastError` is present only when that connection has one.
`tls` and `selected` appear only when true.

`state`: `Offline`, `Connecting`, `Connected`, `Disconnecting`, `Reconnecting`.

## `status`

```json
{"ok":true,"status":{"id":"…","host":"…","port":6697,"tls":true,"nick":"…","state":"Connected","selected":true}}
```

Same object shape as one `connections` row.

## `conversations`

```json
{"ok":true,"conversations":[{"target":"#omarchy","channel":true,"topic":"A cozy corner for Omarchy users and builders.","unread":0}]}
```

`unread` and `mention` are GUI badges. This snapshot does not clear them.
`channel` and `mention` appear only when true. Channel rows may include
`topic` when the server has sent one. Direct message rows omit both.
Status is not in this list.

## `names`

```json
{"ok":true,"members":[{"nick":"fred","label":"~fred","status":"building Omairc"},{"nick":"anna","label":"&anna","away":true,"status":"writing docs"}]}
```

`label` includes the server `PREFIX` rank. `status` is the IRCv3
metadata status when the server grants `draft/metadata-2`.
`away` appears only when true; `status` only when non-empty.

Rows are in member-panel order: highest `PREFIX` rank first (`~`, `&`, `@`,
`%`, `+`, then people without a rank), and case-mapped nick order inside each
rank. This is the panel order, not the server's `NAMES` order.

## `read`

```json
{"ok":true,"messages":[{"network":"…","target":"#omarchy","sender":"anna","timestamp":"2026-09-14T03:00:00.000Z","message":"hello","kind":"message"}]}
```

`kind` is `message`, `notice`, or `action`. Join and part lines stay out.
`msgid` is present when the server tagged the line.
`mention` appears only when true.
`"truncated": true` when the 100-line cap dropped older lines.

## Paths (macOS)

| Item | Location |
|---|---|
| Built binary | `./build/omairc.app/Contents/MacOS/omairc` |
| CLI socket | `omairc.sock` under Qt `RuntimeLocation` (typically `$TMPDIR`) |
| Config / profiles | `~/Library/Application Support/omairc/` (or `$XDG_CONFIG_HOME/omairc/` when set) |
| State / logs / CLI cursors | `~/Library/Application Support/omairc/` state tree (or `$XDG_STATE_HOME/omairc/` when set) |
| Secrets | macOS Keychain via QtKeychain |
