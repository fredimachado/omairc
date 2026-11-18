# Search history

`Ctrl+Shift+F` opens a modal sheet that searches the history of every conversation and opens the one the user picks, scrolled to the matching line. Results stream in as the user types: the lines each conversation holds in memory answer first, then the on-disk logs of the conversations on known networks are read off the UI thread. Hits are newest first, capped at 200 with a note when more matched. A line matches as `Ctrl+F` matches a row (case-insensitive, over the author and the plain text with mIRC codes stripped).

## Sub-features

- `search-open` toggles with `Ctrl+Shift+F`. The same chord closes it. Connect blocks the chord, as it blocks every sheet.
- `search-type` searches after a 150 ms pause. The match is case-insensitive over the author nick and the plain text. A blank field lists nothing and shows `Type to search every conversation`. The footer reads `N results`, `Searching…` while the logs are read, or `200+ results, refine your search`.
- `search-walk` Up and Down move the highlight through the hits and wrap. The list scrolls to keep it in view.
- `search-jump` Enter closes the sheet, selects the hit's conversation, and lands on the line (by msgid when it has one, else by author, kind, body and time). A line older than the transcript the conversation has loaded opens the conversation as it opens without a search, with `That message is older than the loaded history.` in place of the topic line. No history is fetched for it.
- `search-close` Escape closes the sheet and returns focus to the composer.

## How to get to it (user POV)

- Press `Ctrl+Shift+F` from any conversation or from Status. It is listed in the `Ctrl+/` shortcuts sheet as `search history`.
- Type a word. Press Down to move to another hit and Enter to open it.

## Driving it with control-omairc

Preconditions:

- `launch --demo-server`. The demo keeps nothing on disk, so only the in-memory source answers: anna's direct `fred: The prototype already feels at home. Nice work.` and mira's `looks good, fred.` in `#ricing` are searchable. The disk source is covered by the unit tests, not by a launch.
- **No executable recipe yet.** `run search-history` fails closed. This file has no `desktop-recipe` fence. Do not add an empty fence.
- The Xvfb desktop driver needs lavapipe. The desktop proof is the in-process `#[gpui::test]` in `client/src/ui/tests/search.rs`. The TUI proof is `control-omairc-tui` (`key --key ctrl+shift+f`, `type --text prototype`, `key --key down`, `key --key enter`).

- **What a user sees.** `Ctrl+Shift+F` opens a sheet with a field and a list `network  conversation  time  author  text`. Typing `prototype` lists anna's line. Enter closes the sheet, the window title becomes `anna - Omairc` (the TUI's OSC title) and the line is on screen. Escape on an open sheet returns focus to the composer.

## Gotchas

- Logs only hold lines the transcript policy admitted. A credential is never stored and never found, and a hand-edited log that holds one is filtered the same way when it is read.
- Only conversations the client has open on a known network are searched. A closed direct message, or a network that was removed, is not. Muted conversations are searched.
- Joins, parts and other event lines are not searched, only messages, notices and `/me` lines.
- The query is never written to the application log.
- A hit older than the loaded transcript does not fetch CHATHISTORY. It opens the conversation and says so.
- The search sheet is not the in-conversation `Ctrl+F` find. Find stays on the transcript on screen.
- `Home` and `End` edit the field's caret; only Up and Down walk the hits.
- Results from disk are bounded by the 200-hit cap, so a very old line is found only while fewer than 200 newer lines match the same query.
