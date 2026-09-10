# Slash complete

Slash complete shows matching commands above the composer after `/` plus a non-space character, then Tab inserts the canonical verb with a trailing space. Bare `/`, `//`, and a space after the verb keep the list closed.

## Sub-features

- `slash-open` shows the list for `/j` above the composer.
- `slash-tab` inserts `/join ` with Tab and closes the list.
- `slash-escape` dismisses the list with Escape and leaves the typed text.
- `slash-move` moves the highlighted row with Up and Down.

## How to get to it (user POV)

- Focus the composer and type `/` plus a letter, for example `/j`.
- Press Tab to insert the highlighted command.
- Press Escape to hide the list.
- Press Up or Down to move the highlight.

## Driving it with control-omairc

Preconditions:

- The compiled list needs `slashCommands` bound (`control-omairc launch --mock`). Default Connect also binds it, but the composer sits under the sheet.
- `qml-suite` covers open, Tab, Escape, Up/Down, and history walk past a bare `/j` with a JS stand-in. Existing nick-complete and history cases omit `slashCommands` and stay on today's Tab / Up / Down paths.
- There is no chord for this feature. Do not look for it on the `Ctrl+/` sheet. Tab there still says nick complete.

- **List for `/j`.** Run `control-omairc launch --mock`, then `control-omairc doctor`, `control-omairc focus-composer`, `control-omairc type --text "/j"`, and `control-omairc screenshot --feature slash-complete --name after-slash-j`. The compiled list sits above the composer with one row, `/join`. Down and Up wrap on that row and do not change the highlight.
- **Tab insert.** From `/j`, type `o` if Escape already dismissed the list, then run `control-omairc key --key Tab` and `control-omairc screenshot --feature slash-complete --name after-tab`. The composer is `/join ` and the list is gone.
- **Offscreen suite.** Run `control-omairc doctor-qml` then `control-omairc qml-suite`. That run covers `test_slashCompleteListAppearsForSlashJ`, `test_slashCompleteTabInsertsCanonicalVerb`, `test_slashCompleteEscapeDismisses`, `test_slashCompleteUpDownMoveSelection`, and `test_slashCompleteHistoryUpWalksPastBareCommand`. The Up/Down test uses a two-row JS fake (`/join` and `/nick`). It does not prove the compiled C++ catalog.

## Gotchas

- `//` and `///x` stay ordinary chat (`/` plus the rest). The list must not open.
- `/` alone and `/join #omarchy` (space after the verb) keep the list closed.
- Tab still nick-completes `mi` when the composer is not a slash query.
- Escape dismisses the list before the shortcuts sheet, Connect, or Status.
- Enter on an exact name or alias (`/close`, `/j`) sends. Enter on a partial (`/jo`) inserts `/join `.
- On Status, `/me` is not in the catalog. The list can open on `/mode` because `mode` starts with `me`. Conversation `/me` still ranks `/me` first.
- Recalling a sent line keeps the list closed so Up/Down keep walking history. The list can open again after you leave history browse.
- Compiled `/j` ranks only `/join` (the `j` alias). Down and Up wrap on that one row. The qml-suite Up/Down case uses a two-row fake and is not this compiled list.
