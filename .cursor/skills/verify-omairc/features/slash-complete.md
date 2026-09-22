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

- The compiled list needs `slashCommands` bound (`control-omairc launch --demo-server`). Default Connect also binds it, but the composer sits under the sheet. Do not start this recipe there.
- `qml-suite` covers open, Tab, Escape, Up/Down, and history walk past a bare `/j` with a JS stand-in. Existing nick-complete and history cases omit `slashCommands` and stay on today's Tab / Up / Down paths. That suite is not compiled-window proof.
- There is no chord for this feature. Do not look for it on the `Ctrl+/` sheet. Tab there still says nick complete.

```desktop-recipe
launch --demo-server
wait-title --exact "#omarchy · irc.example · fred - Omairc"
composer
type --text "/j"
screenshot --feature slash-complete --name after-slash-j
key --key Escape
wait-title --exact "#omarchy · irc.example · fred - Omairc"
```

- **List for `/j`.** Press `Ctrl+L` and type `/j`. Run `control-omairc composer`, `control-omairc type --text "/j"`, and `control-omairc screenshot --feature slash-complete --name after-slash-j`. The compiled list sits above the composer with one row, `/join`. The composer shows `/j`.
- **Escape.** Press Escape. Run `control-omairc key --key Escape` then `control-omairc wait-title --exact "#omarchy · irc.example · fred - Omairc"`. The list closes and `/j` stays in the composer. The conversation does not change.
- **Tab insert.** From `/j`, `control-omairc key --key Tab` inserts `/join ` and closes the list. That step is not in this fence, because the recipe dismisses with Escape first. Prove Tab on a composer that still holds `/j`.
- **Offscreen suite.** Run `control-omairc doctor-qml` then `control-omairc qml-suite`. That run covers `test_slashCompleteListAppearsForSlashJ`, `test_slashCompleteTabInsertsCanonicalVerb`, `test_slashCompleteEscapeDismisses`, `test_slashCompleteUpDownMoveSelection`, and `test_slashCompleteHistoryUpWalksPastBareCommand`. The Up/Down test uses a two-row JS fake (`/join` and `/nick`). This is not compiled-window proof.

## Gotchas

- `run slash-complete` skips `launch` when `doctor` is already healthy and `demo=yes`. It does not clear the composer. A leftover `/j` or `/close` changes what `type` appends. Recipes that need an empty composer need `cleanup` before `run`.
- `//` and `///x` stay ordinary chat (`/` plus the rest). The list must not open.
- `/` alone and `/join #omarchy` (space after the verb) keep the list closed.
- Tab still nick-completes `mi` when the composer is not a slash query.
- Escape dismisses the list before the shortcuts sheet, Connect, or Status.
- Enter on an exact name or alias (`/close`, `/j`) sends. Enter on a partial (`/jo`) inserts `/join `.
- On Status, `/me` is not in the catalog. The list can open on `/mode` because `mode` starts with `me`. Conversation `/me` still ranks `/me` first. Status `/t` opens `/time`. Conversation `/t` still ranks `/topic` first.
- Recalling a sent line keeps the list closed so Up/Down keep walking history. The list can open again after you leave history browse.
- Compiled `/j` ranks only `/join` (the `j` alias). Down and Up wrap on that one row. The qml-suite Up/Down case uses a two-row fake and is not this compiled list.
- `/q` ranks `/query`. `/quit` inserts `/disconnect `. `/q` is not a quit alias.
