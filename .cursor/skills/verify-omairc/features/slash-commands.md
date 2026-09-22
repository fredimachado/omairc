# Slash commands

Slash commands let a live session send catalog verbs from the same single-line composer.

## Sub-features

- `slash-live-dispatch` sends `/me`, `/join`, `/part`, `/nick`, `/disconnect` (`/quit`), `/clear`, `/close`, `/query`, `/msg`, `/topic`, `/notice`, `/away`, `/back`, `/autoaway`, `/status`, `/avatar`, `/whois`, `/ping`, `/time`, `/version`, `/mode`, `/kick`, `/ignore`, `/unignore`, `/ignored`, `/monitor`, `/unmonitor`, `/monitored`, `/mute`, `/unmute`, `/muted`, `/highlight`, `/unhighlight`, `/highlights`, `/op`, `/deop`, `/voice`, `/devoice`, `/ban`, `/invite`, `/ns`, `/cs`, `/znc`, `/raw` (`/quote`), `/help`, and `/list` when `irc` is bound. `/disconnect` and `/quit` idle the focused network. `/q` is unknown. `/msg`, `/ns`/`/cs`, and `/znc` send without opening or selecting a DM. `/znc` quiet-sends to `*status`. `/join` also matches `/j`, accepts comma-separated channels, and supports an optional key after whitespace. A successful join opens the last named channel. A failed join still selects that empty buffer and echoes the server's reason into the channel transcript (and Status). Empty `/join` joins the latest inbound invite. Clicking the channel token on a Status `INVITE` line joins that channel. `/part` also matches `/leave`. `/ignore` hides private messages, notices, and invites from that nick. Channel text stays visible. `/ignored` lists the nicks on Status. `/monitor` watches a nick for IRCv3 `MONITOR` sign-on and sign-off. `/monitored` lists those nicks plus online/offline/unknown on Status. `/mute` quiets a channel or DM. Chat still arrives. Mentions do not notify or badge. `/muted` lists the targets on Status. Status `/mute` without a target is refused. `/status` sets standing status metadata; empty `/status` echoes the current value; sole `/status clear` unsets it. `/avatar` sets standing avatar metadata from an HTTPS URL or email (stored as a Gravatar SHA-256 URL); empty `/avatar` echoes the stored URL (clickable when HTTPS); sole `/avatar clear` unsets it. Replies copy into the selected conversation transcript when one is open. Neither is `/away`. `/autoaway` is a global client idle timer (off by default). Confirmations are local whois rows in the asking conversation, or Status when issued there. `/pref` reads and sets the global Preferences toggles (`directs`, `avatars`, `unread`). Bare `/pref` prints all three. A name prints one, and avatars adds the IP note. `on` or `off` changes that toggle. A bad name prints the usage line and changes nothing. The write still succeeds when there is no network to attach the line to. After `/pref `, completion lists the names, then `on` and `off`. When it trips, AWAY fans out to every registered network that is not already on a manual `/away`. `/help` prints the catalog in the active conversation. A Status `/help` stays on Status. `/list` (optional server mask) opens a searchable overlay of channels on the focused network; it does not print `322` into Status. A second `/list` with the same mask reuses the in-memory list until disconnect; `/list` while the overlay is open refreshes.
- `slash-list-overlay` is the `/list` picker. It streams `322` rows (name, users, topic), fuzzy-filters like `Ctrl+K`, sorts by users, and Enter/`click` joins (or focuses if already joined). Escape dismisses and returns to the composer. It is a no-op while Connect is visible. `--demo-server` answers LIST, including unjoined `#linux`. Topics strip mIRC colors and keep bold, italic, and underline.
- `slash-whois-transcript` copies the same formatted WHOIS lines Status already shows into the conversation that issued `/whois`, as left-aligned `whois` rows in the chat column. Joins stay centered event captions. Status still logs them. A Status `/whois` stays Status-only. Empty `/whois` on a channel refuses with `Name a nick`. Known metadata keys (display-name, pronouns, status, bot, homepage, color, avatar) appear as extra whois rows when already stored.
- `slash-ctcp-query` sends CTCP `PING`, `TIME`, and `VERSION` to a nick. Empty `/ping`, `/time`, or `/version` on a direct message uses that peer. Empty form on a channel refuses with `Name a nick`. Channel targets are refused. Replies copy into the asking conversation as `whois` rows. A Status query stays Status-only. Unsolicited CTCP replies stay on Status.
- `slash-scope` keeps conversation-only verbs off Status (`/me`, `/close`, `/topic`, `/op`, `/deop`, `/voice`, `/devoice`, `/ban`) and refuses empty `/whois`, `/ping`, `/time`, or `/version` on a channel. `/ns`, `/cs`, and `/znc` work from either surface.

## How to get to it (user POV)

- In a live conversation or Status, type a catalog verb and press Enter.
- On `--demo-server`, catalog verbs run against the seeded controller. `/close` on a channel is rejected and stays in the composer. A partial needle such as `/j` Tab-inserts instead of sending. `/ping`, `/time`, and `/version` get a NOTICE reply from the target nick, copied into the asking transcript as `whois` rows. `/away` and `/back` get the `306` / `305` numerics, so the footer and the `fred` member row switch between `away` and `available` on every channel.
- Press `Ctrl+W` to close a DM without typing `/close`. That chord is keyboard.

## Driving it with control-omairc

Preconditions:

- Seeded dispatch needs `control-omairc launch --demo-server`. When Xvfb tools are missing, `qml-suite` (seeded `IrcController`) is the fallback and is not compiled-window proof.
- Live network dispatch needs a completed Connect and a registered session. Do not Apply on the compiled first-run window for this proof. Do not inject IRC frames.
- `control-omairc launch` without `--demo-server` is first-run Connect titled `irc.libera.chat Status`. That window has no session.
- The composer must be empty. `send` appends to a leftover draft.

```desktop-recipe
launch --demo-server
wait-title --exact "#omarchy · irc.example · fred - Omairc"
send --text "/join #help"
wait-title --exact "#help - Omairc"
screenshot --feature slash-commands --name after-join
send --text "/close"
wait-title --exact "#help - Omairc"
screenshot --feature slash-commands --name close-stays
```

- **`/join` opens the last channel.** On a fresh demo, run `control-omairc send --text "/join #help"` then `control-omairc wait-title --exact "#help - Omairc"`. The composer is empty. Capture it with `control-omairc screenshot --feature slash-commands --name after-join`.
- **Channel `/close` refuses.** On `#help`, run `control-omairc send --text "/close"` then `control-omairc wait-title --exact "#help - Omairc"`. The title stays `#help - Omairc`. The composer still holds `/close`. DIRECT MESSAGES still has `anna` and `dax`. Capture it with `control-omairc screenshot --feature slash-commands --name close-stays`.
- **Offscreen suite.** When Xvfb tools are missing, run `control-omairc doctor-qml` then `control-omairc qml-suite`. `test_typedCloseStaysChatLine` is the refused `/close` path. `test_typedQueryOpensDirectAndClearsComposer` is the successful `/query` path. `test_joinOpensChannelAndConsumesComposer` is the successful `/join` path. `test_listOpensOverlayAndJoins` and `test_listUsesPerNetworkCache` are `/list`. This is not compiled-window proof. There is no compiled-window screenshot for a real-network catalog.
- **Live catalog.** Do not claim `/away`, `/whois`, `/topic`, `/query`, `/msg`, `/notice`, `/part`, `/kick`, `/ignore`, or the monitor and mute verbs on first-run Connect. It is `verified-unreachable` until a session has completed Connect. The attempted compiled route is `control-omairc launch` (title `irc.libera.chat Status`, Apply would start a real network). This recipe does not Apply.

## Gotchas

- `run slash-commands` skips `launch` when `doctor` is already healthy and `demo=yes`. It does not clear the composer or the selected channel. A leftover draft makes `/join #help` append instead of send. Recipes that need an empty composer on `#omarchy` need `cleanup` before `run`.
- Live `/me` is an ACTION even without a trailing space.
- `Ctrl+W` closes a DM. Typed `/close` on a channel is rejected.
- `//away` and the other doubled slashes stay Say of the rest of the line.
- Status and conversation share one composer. Conversation scope errors land in the network subtitle (`lastError`), not as chat. Status `submit` failures log on Status.
- A successful `/join` opens the last named channel and is consumed before the switch, so it cannot remain as a draft on the previous conversation. A failed `/join` stays on the empty channel buffer and shows the server's reason there as well as on Status. A rejected command such as `/close` on a channel stays in the composer.
- Compiled `--demo-server` still binds slash-complete. Enter on a partial needle that is not an alias (such as `/c`) inserts the canonical verb. A token that equals a verb alias (such as `/j`, which aliases `join`) sends instead.
