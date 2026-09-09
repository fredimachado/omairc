# Slash commands

Slash commands let a live session send catalog verbs from the same single-line composer. Mock UI only special-cases `/me `. Other leading slashes stay ordinary chat.

## Sub-features

- `slash-mock-chat` posts `/close`, `/away`, `/whois`, and the rest as a `fred` line on `--mock`.
- `slash-live-dispatch` sends `/me`, `/join`, `/part`, `/nick`, `/quit`, `/clear`, `/close`, `/query`, `/msg`, `/topic`, `/notice`, `/away`, `/back`, `/whois`, and `/mode` when `irc` is bound. `/join` takes comma-separated channels. Whitespace inside one entry is an optional key.
- `slash-scope` keeps conversation-only verbs off Status (`/me`, `/close`, `/topic`) and refuses empty `/whois` on a channel.

## How to get to it (user POV)

- In a live conversation or Status, type a catalog verb and press Enter.
- On `--mock`, type any slash except `/me ` and press Enter. The line stays in the transcript.
- Press `Ctrl+W` to close a DM without typing `/close`. That chord is keyboard.

## Driving it with control-omairc

Preconditions:

- Mock chat-line proof needs `control-omairc launch --mock` or `qml-suite` (`irc` left null).
- Live dispatch needs a completed Connect and a registered session. Do not Apply on the compiled first-run window for this proof.
- `control-omairc launch` without `--mock` is first-run Connect titled `irc.libera.chat Status`. That window has no session.

- **Mock stays chat.** On `#omarchy`, run `control-omairc send --text "/close"`. The last line is `fred` / `/close`. DIRECT MESSAGES still has `anna` and `dax`.
- **Offscreen suite.** When Xvfb tools are missing, run `control-omairc doctor-qml` then `control-omairc qml-suite`. `test_typedCloseStaysChatLine` is that mock path. There is no compiled-window screenshot for live dispatch.
- **Live catalog.** Do not claim `/away`, `/whois`, `/topic`, `/query`, `/msg`, `/notice`, `/part`, or `/close` on first-run Connect. It is `verified-unreachable` until a session has completed Connect. The attempted compiled route is `control-omairc launch` (title `irc.libera.chat Status`, Apply would start a real network).

## Gotchas

- Mock `/me ` is send-message. Live `/me` is an ACTION even without a trailing space.
- `Ctrl+W` closes a mock or live DM. Typed `/close` on `--mock` does not.
- `//away` and the other doubled slashes stay Say of the rest of the line.
- Status and conversation share one composer. Scope errors land in Status, not as chat.
