# Insert a file link

Paste, drop, or pick a file and a normal web link is inserted into the draft. The control appears only when the server offers a file host. The link is an ordinary web address.

## Sub-features

- `file-link-hidden` keeps the file control hidden when the server offers no file host. The seeded demo does not offer one, so the composer shows SEND and no FILE button.
- `file-link-pick` inserts a web link from a picked file when a file host is offered. Desktop uses the FILE button or `Ctrl+Shift+U`. The terminal client uses `Ctrl+Shift+U`.
- `file-link-paste` inserts a web link when the clipboard is a local file. The desktop client also accepts a dropped file. The terminal client has no drop target.

## How to get to it (user POV)

- On a server that offers a file host, press `Ctrl+Shift+U` and choose a file. The composer gains an `http` or `https` link. Enter still sends.
- On the desktop client, click FILE, paste a file, or drop a file on the conversation.
- On the terminal client, paste a file path or use `Ctrl+Shift+U`.
- On the seeded demo, the FILE button is absent because that server offers no file host.

## Driving it with control-omairc

Preconditions:

- Seeded conversation UI is showing. Use `control-omairc launch --demo-server`.
- The demo server does not offer a file host. This recipe checks that the composer is the normal send field. It does not upload a file.

```desktop-recipe
launch --demo-server
wait-title --exact "#omarchy · irc.example · fred - Omairc"
screenshot --feature file-link --name composer-without-file-host
```

- **Seeded composer.** Launch the demo and wait for `#omarchy · irc.example · fred - Omairc`. Run `control-omairc launch --demo-server` and `control-omairc wait-title --exact "#omarchy · irc.example · fred - Omairc"`.
- **No file control.** Capture the composer. Run `control-omairc screenshot --feature file-link --name composer-without-file-host`. The shot shows SEND. FILE is not visible.

## Gotchas

- The file control is absent until the server offers a file host. The seeded demo never shows FILE.
- The inserted text is an ordinary `http` or `https` address. Nothing is sent until Enter.
- The terminal client has no drop target. Paste and `Ctrl+Shift+U` are the two ways in.
- A failure is a fixed Status line: `Could not upload the file.`, `The file is too large.`, `The file is empty.`, or `That is not a file.`
