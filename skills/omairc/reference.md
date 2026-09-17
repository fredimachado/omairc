# CLI JSON

Every control command prints one compact JSON object on stdout.

Success has `"ok": true`. Failure has `"ok": false` and `"error"`.
`--help` and `--version` are plain text, not JSON.

## Envelope

```json
{"ok":true}
{"ok":false,"error":"Omairc is not running (no local socket). Start the client first."}
```

`send` and `raise` succeed with `{"ok":true}` only.

Zero connections: `"No connections are available."`
More than one connection without `--network`: `"Multiple connections are available; specify a network id."`
Unknown `--network`: `"Unknown network id '<id>'."`

## `connections`

```json
{"ok":true,"connections":[{"id":"…","host":"irc.libera.chat","port":6697,"tls":true,"nick":"fred","state":"Connected","selected":true}]}
```

`lastError` is present only when that connection has one.

`state`: `Offline`, `Connecting`, `Connected`, `Disconnecting`, `Reconnecting`.

## `status`

```json
{"ok":true,"status":{"id":"…","host":"…","port":6697,"tls":true,"nick":"…","state":"Connected","selected":true}}
```

Same object shape as one `connections` row.

## `conversations`

```json
{"ok":true,"conversations":[{"target":"#omarchy","channel":true,"topic":"A cozy corner for Omarchy users and builders.","unread":0,"mention":false}]}
```

`unread` and `mention` are GUI badges. This snapshot does not clear them.
Channel rows include `topic` (empty when the server has not sent one). Direct
message rows omit it. Status is not in this list.

## `names`

```json
{"ok":true,"members":[{"nick":"fred","label":"~fred","away":false,"status":"building Omairc"},{"nick":"anna","label":"&anna","away":false,"status":"writing docs"}]}
```

`label` includes the server `PREFIX` rank. `status` is the IRCv3
metadata status when the server grants `draft/metadata-2`.

Rows are in member-panel order: highest `PREFIX` rank first (`~`, `&`, `@`,
`%`, `+`, then people without a rank), and case-mapped nick order inside each
rank. This is the panel order, not the server's `NAMES` order.

## `read`

```json
{"ok":true,"messages":[{"network":"…","target":"#omarchy","sender":"anna","timestamp":"2026-09-14T03:00:00.000Z","message":"hello","kind":"message","mention":false}]}
```

`kind` is `message`, `notice`, or `action`. Join and part lines stay out.
`msgid` is present when the server tagged the line.
`"truncated": true` when the 100-line cap dropped older lines.
