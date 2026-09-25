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

- Proof is the offscreen suite. `qml-suite` clicks `urlHit` on `messageBody` and on a Status line, and asserts the allowlist without opening a system handler.
- A compiled window would call `Qt.openUrlExternally` on a real click. Do not use that as proof on the isolated display.
- `control-omairc launch` without `--demo-server` is first-run Connect titled `irc.libera.chat Status` with no transcript URLs. Do not start this recipe there.

- **No compiled-window recipe.** `control-omairc run open-links` fails closed. This file has no `desktop-recipe` fence. Do not add an empty fence or a `qml-suite` fence.
- **Message and Status clicks.** Run `control-omairc doctor-qml` then `control-omairc qml-suite`. `test_messageBodyClickOpensHttpsUrl` appends `read https://example.com thanks` and clicks the hit; `lastOpenedUrl` is `https://example.com`. `test_consoleBodyClickOpensHttpUrl` does the same for `http://example.com` on Status. `test_httpUrlAllowlist` keeps `file:` and `javascript:` closed. `test_httpUrlAtTrimCases` covers balanced parenthesis and bracket trim. `test_ctrlShiftOLinkSheet` covers the link sheet filter, wrap, Enter, Escape, Status INVITE join, and overlay guards. `qml-suite` is not compiled-window proof.

## Gotchas

- Only `http` and `https` with `://` and non-empty text after `://` are allowed. A hostname is not validated separately, so `https:///foo` would still pass. Trailing `.,;:!?` is stripped. Trailing `)` or `]` is stripped only while the close count exceeds the open count inside the match, so balanced parens in paths like `IRC_(protocol)` stay intact.
- The body stays `PlainText`. Do not look for rich-text anchors.
- Sending `https://example.com` from the compiled composer and clicking it is not this feature's proof. That opens the system URL handler.
- Selectable message text is not this feature. The suite still checks that a body can be selected without opening a URL.
