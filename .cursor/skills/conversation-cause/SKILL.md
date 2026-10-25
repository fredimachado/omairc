---
name: conversation-cause
description: Use when touching ensureConversation, conversation creation, or /msg in client/src/irc or tui/internal/irc. Insert rules live in the shared cause predicate; do not invent a second policy.
metadata:
  internal: true
---

# Conversation cause

`IrcConversationCause` and the insert predicate (`ircConversationCauseInserts` /
`ConversationCauseInserts`) decide whether `ensureConversation` inserts.
Implementations live in `client/src/irc/` (Rust desktop) and
`tui/internal/irc/` (Go TUI). Service targets use the shared
`ircTargetLooksLikeService` / `targetLooksLikeService` helper.

| Cause | Inserts |
|---|---|
| `QuietSend`, `InboundSelf` | never |
| `UserOpen` | DMs only |
| `ChannelState` | channels only |
| `InboundOther` | channels and non-service DMs |
| `Restore` | non-service DMs, and only after ISUPPORT (`376` / `422`) |

`Restore` is applied from the controller's reopen-directs path on numeric `376`
or `422`, and when the reopen-directs preference is turned on while the network
is already registered.

Persist a query only when the user opened it, sent in it, or a self-authored line arrives. A live message, notice, or action from someone else also persists a query when that line recreates one the user had closed. The controller records that case before `apply`, because the reducer inserts the row during `apply`. A first inbound that was never closed stays unpersisted. Remembering a query also drops its closed mark.

A closed channel is not inserted by `ensureConversation`. The in-memory mark is cleared on welcome. The persisted mark is not, so a self JOIN the user did not ask for still leaves the channel closed after a restart. `clearClosed` drops both marks, and an explicit join calls it before the row is inserted.

- User open: open/reveal direct message and query dispatch use `UserOpen`, then remember the open direct.
- Composer `/msg` is quiet send: `ensureConversation` with `QuietSend` without remembering the direct. Service `/msg` variants use the same quiet path.
- Local CLI send remembers the direct, then `ensureConversation` with `QuietSend`.
- Sending in the open conversation remembers the direct.
- Self-authored lines: the controller remembers when the author of a message, notice, or action matches the current nick; the reducer still uses `InboundSelf` for that author so the line does not insert a missing conversation.

`echoIfPresent` skips a local echo when the conversation is missing, so `/msg` to a new nick stays closed. With `echo-message`, the server echo is the self-authored message event that persists the target.

Any change to this path updates the same matrix in **both** trees:

- Rust: controller and reducer tests under `client/` (search for
  `conversationCreateMatrix`, quiet send, NickServ, and reopen-direct cases).
- Go: `tui/internal/controller/controller_test.go` and reducer tests in
  `tui/internal/irc/`.

Do not add a new test file for a single cause; extend the existing matrix.
