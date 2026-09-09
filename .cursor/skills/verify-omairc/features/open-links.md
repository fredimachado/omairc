# Open links

Open links lets a user click an `http` or `https` URL in a conversation transcript or the Status log. The pointer becomes a hand over an allowed URL. `file:`, `javascript:`, and other schemes do nothing.

## Sub-features

- `links-https` opens an `https://` URL from a message body.
- `links-http` opens an `http://` URL from a Status line.
- `links-allowlist` ignores `file:`, `javascript:`, and text that is not on an allowed URL.

## How to get to it (user POV)

- Click an `http` or `https` URL in a chat line.
- Click an `http` or `https` URL in Status.
- Click ordinary text, or a `file:` / `javascript:` URL. Nothing opens.

## Driving it with control-omairc

Preconditions:

- Proof is the offscreen suite. `qml-suite` clicks `urlHit` on `messageBody` and on a Status line, and asserts the allowlist without opening a system handler.
- A compiled window would call `Qt.openUrlExternally` on a real click. Do not use that as proof on the isolated display.
- `control-omairc launch` without `--mock` is first-run Connect titled `irc.libera.chat Status` with no transcript URLs. Do not start this recipe there.

- **Message and Status clicks.** Run `control-omairc doctor-qml` then `control-omairc qml-suite`. `test_messageBodyClickOpensHttpsUrl` appends `read https://example.com thanks` and clicks the hit; `lastOpenedUrl` is `https://example.com`. `test_consoleBodyClickOpensHttpUrl` does the same for `http://example.com` on Status. `test_httpUrlAllowlist` keeps `file:` and `javascript:` closed. There is no compiled-window screenshot for that path.

## Gotchas

- Only `http` and `https` with `://` and a non-empty host are allowed. Trailing `.,;:!?)\]>` is stripped from the match.
- The body stays `PlainText`. Do not look for rich-text anchors.
- Sending `https://example.com` from the compiled composer and clicking it is not this feature's proof. That opens the system URL handler.
- Selectable message text is not this feature. The suite still checks that a body can be selected without opening a URL.
