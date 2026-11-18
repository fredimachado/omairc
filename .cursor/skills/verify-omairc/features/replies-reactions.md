# Replies and reactions

Other IRCv3 clients send replies (`+draft/reply`) and emoji reactions (`+draft/react`). Omairc shows both and sends neither. A reply is a chat line with one muted `↪ author: text` line above its body, naming the line it answers. A reaction is a small pill under a line, `👍 2`, one per emoji. Both clients draw them: the desktop window and the TUI.

Reactions are quiet. They never open a conversation, mark a conversation unread, raise a mention or notification, or touch the sidebar. A reaction to a line that is not loaded is dropped without a trace.

## Sub-features

- `reply-line` shows the muted `↪ author: text` line above a reply, in the body column. The text is the parent's body flattened to one line and cut with an ellipsis. A parent that is not in the loaded transcript reads `↪ reply`, and the line names the parent once the history that holds it arrives.
- `reaction-chips` shows `emoji count` pills under a reacted line, in first-seen order. On the desktop the tooltip over a pill lists the nicks that sent it. The TUI shows the counts only. A nick counts once per emoji, case-mapped by the server's `CASEMAPPING`.
- `reply-jump` makes a click on the desktop reply line scroll to the parent when it is loaded. A parent that is not loaded does nothing. The TUI has no pointer, so it has no jump.

## How to get to it (user POV)

- Open `#omarchy`. `dax`'s first line is a reply to `anna`'s opening question, so it has `↪ anna: Morning! Has anyone tried the new minimal install flow yet?` above its body.
- `anna`'s opening question carries `👍 2` under it, from `mira` and `sol`. `dax`'s reply carries `🎉 1`.
- Against a live server, any client that sends `+draft/reply` or `+draft/react` shows the same way once the network granted `message-tags`.

## Driving it with control-omairc

Preconditions:

- Seeded conversation UI is showing (`#omarchy · irc.example · fred - Omairc`). Use `control-omairc launch --demo-server`. If desktop tools are missing, report the unmet precondition.
- `control-omairc launch` without `--demo-server` is first-run Connect and has no transcript. Do not start this recipe there.
- The seed is the whole proof: the demo does not accept a client-sent reaction, because sending is not part of this feature.

```desktop-recipe
launch --demo-server
wait-title --exact "#omarchy · irc.example · fred - Omairc"
screenshot --feature replies-reactions --name reply-and-chips
```

- **Reply and chips.** Run `control-omairc screenshot --feature replies-reactions --name reply-and-chips`. Near the top of the transcript a muted `↪ anna: Morning! Has anyone tried…` line sits above `dax`'s first body. A `👍 2` pill sits under `anna`'s opening line and a `🎉 1` pill under `dax`'s.
- **Jump.** Click the reply line when the opening line is scrolled out of view: the opening line lands at the top of the transcript. The seeded `#omarchy` is short, so this needs a window small enough to scroll or a few more lines. There is no recipe verb for it yet, so the GPUI test `clicking_a_reply_line_scrolls_to_its_loaded_parent` carries the proof.

## Gotchas

- Reactions are in memory only. A restart drops them, and a CHATHISTORY replay does not bring them back (that needs `draft/event-playback`, which is out of scope). A reply keeps its parent through restarts: the parent msgid is in the conversation log, and the preview is rebuilt from the loaded lines, so it reads `↪ reply` until the parent loads.
- A reaction needs a target line with a msgid. Lines Omairc echoes locally without `echo-message` have none, so reactions to your own lines attach only on a network that echoes.
- A reaction whose value is longer than 64 bytes or holds a control character is ignored, as is a TAGMSG that carries both `+draft/react` and `+draft/unreact`.
- `+draft/unreact` with `+draft/reply` removes the sender's reaction. The IRCv3 react spec has no removal, so only clients that send it can take a pill away.
- Clicking a pill does nothing: sending is not part of this feature.
- The TUI draws the reply line and the pills as plain muted lines in the body column. A reaction that lands on a line above the reader moves the view by one line.
