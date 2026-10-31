# Open links

Open links lets a user click an `http` or `https` URL in a conversation transcript or the Status log. The pointer becomes a hand over an allowed URL. `file:`, `javascript:`, and other schemes do nothing.

## Sub-features

- `links-https` opens an `https://` URL from a message body.
- `links-http` opens an `http://` URL from a Status line.
- `links-allowlist` ignores `file:`, `javascript:`, and text that is not on an allowed URL.

## How to get to it (user POV)

- Click an `http` or `https` URL in a chat line.
- Click an `http` or `https` URL in Status.
- Press `Ctrl+Shift+O` to open the link sheet. It lists `http`/`https` URLs from the current transcript (conversation or Status), newest message first and last-in-text first within a row. Type to filter. Up/Down wrap the list and reveal the source row. Enter opens the URL or joins an INVITE channel. Escape dismisses the sheet and returns to the composer without changing the draft. The chord is a no-op while Connect is visible. While the sheet is open, `Alt+Down` does not walk conversations and `Ctrl+/` does not open the shortcuts sheet on top.
- Click ordinary text, or a `file:` / `javascript:` URL. Nothing opens.

## Driving it with control-omairc

Preconditions:

- `control-omairc launch` without `--demo-server` is first-run Connect titled `chat.freenode.net Status` with no transcript URLs. Do not start here.
- A click on a real URL opens the system handler. Do not use that click as proof on the isolated display.

- **No executable recipe yet.** `run open-links` fails closed. This file has no `desktop-recipe` fence. Do not add an empty fence.
- **What a user sees.** A click on `https://example.com` in a message body opens that URL. A click on `http://example.com` in Status opens that URL. `file:` and `javascript:` stay closed. Trailing `.,;:!?` is stripped, and a trailing `)` or `]` is stripped only while it is unmatched. `Ctrl+Shift+O` lists the current transcript's `http`/`https` URLs, filters as you type, wraps with Up/Down, opens or joins on Enter, and dismisses on Escape without changing the draft. Connect blocks the chord. While the sheet is open, `Alt+Down` does not walk conversations and `Ctrl+/` does not open the shortcuts sheet.

## Gotchas

- Only `http` and `https` with `://` and non-empty text after `://` are allowed. A hostname is not validated separately, so `https:///foo` would still pass. Trailing `.,;:!?` is stripped. Trailing `)` or `]` is stripped only while the close count exceeds the open count inside the match, so balanced parens in paths like `IRC_(protocol)` stay intact.
- Sending `https://example.com` from the compiled composer and clicking it is not this feature's proof. That opens the system URL handler.
- Selectable message text is not this feature. Selecting a body must not open a URL.
