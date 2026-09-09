# Mention notify

Mention notify is the desktop notification Omairc sends when a mention arrives and the window is not focused. The summary is the author. The body is the mention text with IRC formatting stripped. A focused window stays quiet.

## Sub-features

- `notify-unfocused` records a desktop notification for a mention while the window is inactive.
- `notify-focused` sends nothing when the window is already focused.
- `notify-plain` strips IRC formatting from the notification body (`hey \x02fred` becomes `hey fred`).

## How to get to it (user POV)

- Leave the Omairc window unfocused during a live session. Be mentioned in a channel or direct message.
- Keep the window focused. The same mention must not notify.

## Driving it with control-omairc

Preconditions:

- Live `notify-unfocused` needs a completed Connect, a registered session, and an unfocused window. Do not Apply on the compiled first-run window for this proof.
- `control-omairc launch` without `--mock` is first-run Connect titled `irc.libera.chat Status`. That window has no session and no mentions.
- `--mock` has no live mentions. There is no compiled-window mock recipe.

- **Offscreen suite.** Run `control-omairc doctor-qml` then `control-omairc qml-suite`. `test_unfocusedMentionNotifiesOnce` calls `notifyMentionIfUnfocused(false, "alice", "hey \x02fred")` and expects `lastNotification` author `alice` / body `hey fred`, then the focused call leaves `lastNotification` null. There is no compiled-window screenshot for that path.
- **Live desktop mention.** Do not claim this on first-run Connect or `--mock`. It is `verified-unreachable` until a session has completed Connect and the isolated window is unfocused. The attempted compiled route is `control-omairc launch` (title `irc.libera.chat Status`, no traffic).

## Gotchas

- Unread badges and `Alt+A` are keyboard / switch-conversation. This feature is only the desktop notification.
- The identity footer does not follow mentions.
- The suite sets `suppressDesktopNotification` so it does not call the session bus. A live compiled mention would.
- Do not Apply on the compiled window to "get a mention". That starts a real network.
