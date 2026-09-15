# Slash commands

Slash commands let a live session send catalog verbs from the same single-line composer.

## Sub-features

- `slash-live-dispatch` sends `/me`, `/join`, `/part`, `/nick`, `/disconnect` (`/quit`), `/clear`, `/close`, `/query`, `/msg`, `/topic`, `/notice`, `/away`, `/back`, `/whois`, `/ping`, `/time`, `/version`, `/mode`, `/kick`, `/ignore`, `/unignore`, `/ignored`, `/mute`, `/unmute`, `/muted`, `/highlight`, `/unhighlight`, `/highlights`, `/op`, `/deop`, `/voice`, `/devoice`, `/ban`, `/invite`, `/ns`, `/cs`, `/raw` (`/quote`), and `/help` when `irc` is bound. `/disconnect` and `/quit` idle the focused network. `/q` is unknown. `/msg` and `/ns`/`/cs` send without opening or selecting a DM. `/join` also matches `/j`, accepts comma-separated channels, and supports an optional key after whitespace. Empty `/join` joins the latest inbound invite. Clicking the channel token on a Status `INVITE` line joins that channel. `/part` also matches `/leave`. `/ignore` hides private messages, notices, and invites from that nick. Channel text stays visible. `/ignored` lists the nicks on Status. `/mute` quiets a channel or DM. Chat still arrives. Mentions do not notify or badge. `/muted` lists the targets on Status. Status `/mute` without a target is refused. `/help` prints the catalog in the active conversation. A Status `/help` stays on Status.
- `slash-whois-transcript` copies the same formatted WHOIS lines Status already shows into the conversation that issued `/whois`, as left-aligned `whois` rows in the chat column. Joins stay centered event captions. Status still logs them. A Status `/whois` stays Status-only. Empty `/whois` on a channel refuses with `Name a nick`.
- `slash-ctcp-query` sends CTCP `PING`, `TIME`, and `VERSION` to a nick. Empty `/ping`, `/time`, or `/version` on a direct message uses that peer. Empty form on a channel refuses with `Name a nick`. Channel targets are refused. Replies copy into the asking conversation as `whois` rows. A Status query stays Status-only. Unsolicited CTCP replies stay on Status.
- `slash-scope` keeps conversation-only verbs off Status (`/me`, `/close`, `/topic`, `/op`, `/deop`, `/voice`, `/devoice`, `/ban`) and refuses empty `/whois`, `/ping`, `/time`, or `/version` on a channel. `/ns` and `/cs` work from either surface.

## How to get to it (user POV)

- In a live conversation or Status, type a catalog verb and press Enter.
- On `--demo-server`, catalog verbs run against the seeded controller. `/close` on a channel is rejected and stays in the composer. A partial needle such as `/j` Tab-inserts instead of sending.
- Press `Ctrl+W` to close a DM without typing `/close`. That chord is keyboard.

## Driving it with control-omairc

Preconditions:

- Seeded dispatch needs `control-omairc launch --demo-server` or `qml-suite` (seeded `IrcController`).
- Live network dispatch needs a completed Connect and a registered session. Do not Apply on the compiled first-run window for this proof.
- `control-omairc launch` without `--demo-server` is first-run Connect titled `irc.libera.chat Status`. That window has no session.

- **Channel `/close` refuses.** On `#omarchy`, run `control-omairc send --text "/close"`. Use the full verb. The composer still holds `/close`. DIRECT MESSAGES still has `anna` and `dax`.
- **Offscreen suite.** When Xvfb tools are missing, run `control-omairc doctor-qml` then `control-omairc qml-suite`. `test_typedCloseStaysChatLine` is that seeded path. There is no compiled-window screenshot for a real-network catalog.
- **Live catalog.** Do not claim `/away`, `/whois`, `/topic`, `/query`, `/msg`, `/notice`, `/part`, `/kick`, `/close`, `/ignore`, `/unignore`, `/ignored`, `/mute`, `/unmute`, or `/muted` on first-run Connect. It is `verified-unreachable` until a session has completed Connect. The attempted compiled route is `control-omairc launch` (title `irc.libera.chat Status`, Apply would start a real network).

## Gotchas

- Live `/me` is an ACTION even without a trailing space.
- `Ctrl+W` closes a DM. Typed `/close` on a channel is rejected.
- `//away` and the other doubled slashes stay Say of the rest of the line.
- Status and conversation share one composer. Conversation scope errors land in the network subtitle (`lastError`), not as chat. Status `submit` failures log on Status.
- Compiled `--demo-server` still binds slash-complete. Enter on a partial needle that is not an alias (such as `/c`) inserts the canonical verb. A token that equals a verb alias (such as `/j`, which aliases `join`) sends instead.
