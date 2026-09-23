# Mention notify

Mention notify is the desktop notification Omairc sends when a mention or a direct message arrives and the window is not focused. The summary is the author. The body is the text with IRC formatting stripped. A focused window stays quiet. A direct message that does not contain the nick still notifies while unfocused.

## Sub-features

- `notify-unfocused` records a desktop notification for a mention while the window is inactive.
- `notify-dm-unfocused` records a desktop notification for a direct message that does not contain the nick while the window is inactive.
- `notify-focused` sends nothing when the window is already focused.
- `notify-muted` sends nothing for a muted channel or direct message. Chat still arrives. Mentions stay at 0.
- `notify-plain` strips IRC formatting from the notification body (`hey \x02fred` becomes `hey fred`).
- `notify-activate` opens the notified conversation when the notification is activated (body click or its `Open` action): Omairc is raised, the conversation is revealed, and the exact message is shown with the composer focused.

## How to get to it (user POV)

- Leave the Omairc window unfocused during a live session. Be mentioned in a channel, or receive a direct message, including one that does not contain the nick.
- Keep the window focused. The same mention or direct message must not notify.
- Activate the notification (click its body, or its `Open` action). Omairc raises and opens that conversation at the exact message.

## Driving it with control-omairc

Preconditions:

- Live `notify-unfocused` needs a completed Connect, a registered session, and an unfocused window. Do not Apply on the compiled first-run window for this proof.
- `control-omairc launch` without `--demo-server` is first-run Connect titled `irc.libera.chat Status`. That window has no session and no mentions.
- `--demo-server` has no new inbound mentions after launch. There is no compiled-window mention recipe there.

- **No compiled-window recipe.** `control-omairc run mention-notify` fails closed. This file has no `desktop-recipe` fence. Do not add an empty fence or a `qml-suite` fence.
- **Offscreen suite.** Run `control-omairc doctor-qml` then `control-omairc qml-suite`. `test_unfocusedMentionNotifiesOnce` calls `notifyMentionIfUnfocused(false, "alice", "hey \x02fred")` and expects `lastNotification` author `alice` / body `hey fred`, then the focused call leaves `lastNotification` null, then `notifyMentionIfUnfocused(false, "alice", "hello")` expects author `alice` / body `hello`. `test_notificationActivateOpensLiveChannelMention` and `test_notificationActivateOpensDirectMessage` cover the activation path: `activateNotifiedConversation` switches to the mention's channel or DM and reveals the exact `msgid` row. `qml-suite` is not compiled-window proof.
- **Live desktop mention.** Do not claim this on first-run Connect or `--demo-server`. It is `verified-unreachable` until a session has completed Connect and the isolated window is unfocused. The attempted compiled route is `control-omairc launch` (title `irc.libera.chat Status`, no traffic).

## Gotchas

- Unread badges and `Alt+A` are keyboard / switch-conversation. This feature is only the desktop notification.
- The mentions badge stays nick-only. An unfocused direct message without the nick still notifies.
- The identity footer shows a separate inbox mark and count for the session waiting list. That inbox is not the mentions badge and does not follow mention counts.
- The suite sets `suppressDesktopNotification` so it does not call the session bus. A live compiled mention would.
- Do not Apply on the compiled window to "get a mention". That starts a real network.
