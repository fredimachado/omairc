---
name: conversation-cause
description: Use when touching ensureConversation, conversation creation, or /msg. Insert rules live in ircConversationCauseInserts; do not invent a second policy.
metadata:
  internal: true
---

# Conversation cause

`IrcConversationCause` and `ircConversationCauseInserts` in `src/irc/irceventreducer.h` decide whether `IrcEventReducer::ensureConversation` inserts. Service targets use `ircTargetLooksLikeService` (declared in that header, defined in `src/irc/irceventreducer.cpp`).

| Cause | Inserts |
|---|---|
| `QuietSend`, `InboundSelf` | never |
| `UserOpen` | DMs only |
| `ChannelState` | channels only |
| `InboundOther` | channels and non-service DMs |
| `Restore` | non-service DMs, and only after ISUPPORT (`376` / `422`) |

`Restore` is applied from `IrcController::restoreOpenDirects`. `noteOpenDirectsMotd` calls it on numeric `376` or `422` inside `IrcController::handleMessage`. Turning reopen-directs on also calls `restoreOpenDirects` from `setReopenDirectMessages` when that network is already `Registered`.

Persist a query only when the user opened it, sent in it, or a self-authored line arrives. That path is `IrcController::rememberOpenDirect`, which returns immediately unless `persistableDirectTarget` (not a channel, not `ircTargetLooksLikeService`).

- User open: `openDirectMessage`, `revealConversation` (only when the conversation is missing), and `dispatchQuery` use `UserOpen`, then `rememberOpenDirect`.
- Composer `/msg` is `dispatchQuietSend` (`quietSendFor` maps `Verb::Msg`). It calls `ensureConversation` with `QuietSend` and does not call `rememberOpenDirect`. `dispatchServiceMsg` (`/ns`, `/cs`, `/znc`) uses that same function.
- Local CLI send is `sendToTarget`: `rememberOpenDirect`, then `ensureConversation` with `QuietSend`.
- Sending in the open conversation: `sendSelectedMessage` and the `/me` branch of `dispatch` call `rememberOpenDirect`.
- Self-authored lines: `IrcController::apply` calls `rememberOpenDirect` when the author of an `IrcMessageEvent`, `IrcNoticeEvent`, or `IrcActionEvent` matches the current nick. `appendChat` still uses `InboundSelf` for that author, so the line does not insert a missing conversation.

`echoIfPresent` skips a local echo when the conversation is missing, so `/msg` to a new nick stays closed. With `echo-message`, the server echo is the self-authored `IrcMessageEvent` that persists the target.

Any change to this path updates the same matrix in `tests/session/tst_controller.cpp`. Do not add a new test file for a single cause. Existing cases: `ControllerTest::zncQuietSendDoesNotOpenDirect`, `ControllerTest::msgEchoDoesNotOpenMissingDirect` (`/msg` with `echo-message`), `ControllerTest::incomingNickservPrivmsgDoesNotOpenDirect`, and `ControllerTest::routableNicksOpenDirectsAndPseudoClientsDoNot` (incoming human PRIVMSG opens a DM; pseudo-clients do not). `ControllerTest::conversationCreateMatrix` covers `/msg` with `echo-message`, NickServ, a human PRIVMSG, and `/query` in one test. Reducer checks are `ReducerTest::conversationCauseInsertTable` and `ReducerTest::ensureConversationHonorsCause` in `tests/session/tst_reducer.cpp`.
