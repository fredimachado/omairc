---
name: Bottom-anchored transcript
overview: "Make transcripts and Status that are shorter than the viewport sit on the composer and grow upward, in both the Qt and TUI clients. The existing behaviors stay unchanged: following the end, scrolling up to detach, the unseen jump, saved scroll places, and Page Up history loading."
todos:
  - id: issue-branch
    content: Find or open a GitHub issue for bottom-anchored transcripts and branch off main
    status: pending
  - id: qt-tests
    content: "Add failing Qt tests: short transcript/Status sits at bottom, grows upward, resize keeps bottom, Page Up history anchor on short list"
    status: pending
  - id: qt-impl
    content: "TranscriptList: dynamic topMargin, topContentY/endContentY/viewportAtTop/firstRenderedIndex, onTopMarginChanged; use firstRenderedIndex in anchor snapshots"
    status: pending
  - id: window-wiring
    content: "OmaircWindow scrollTranscript: use list.viewportAtTop() and list.firstRenderedIndex()"
    status: pending
  - id: tui-tests
    content: Add failing TUI tests for bottom-anchored short transcript and Status
    status: pending
  - id: tui-impl
    content: Add padAbove helper and use it in transcriptView; fix any tests that assumed top alignment
    status: pending
  - id: feature-map
    content: Extend keyboard-scroll in features/keyboard.md with the bottom-gravity behavior
    status: pending
  - id: validate
    content: Run bin/test (incl. tui/bin/test), visually verify Qt demo and TUI PTY, open PR against main
    status: pending
isProject: false
---

# Bottom-anchored transcript

## What exists vs. what is missing

These behaviors already exist and are tested:
- Following the end, so new rows push older ones up.
- Detaching when you scroll up, and keeping your place while rows arrive.
- The `Alt+U` / unseen jump.
- Per-conversation saved places.
- Page Up at the top loading older history.

In Qt this lives in [src/qml/TranscriptList.qml](src/qml/TranscriptList.qml). In the TUI it lives in [tui/internal/ui/transcript.go](tui/internal/ui/transcript.go) and [tui/internal/ui/scroll.go](tui/internal/ui/scroll.go).

The gap is short content. When rows do not fill the viewport, both clients draw them from the top. The Qt `ListView` starts items at `originY`. The TUI pads blank lines below the rows:

```168:168:tui/internal/ui/transcript.go
	rows := fitLines(body[start:end], viewport, false)
```

The goal is mIRC/Slack gravity: the newest row always sits just above the composer, and the empty space is above the oldest row.

## Qt: dynamic top margin on `TranscriptList`

Use a top margin that fills the slack. Do not use `verticalLayoutDirection: ListView.BottomToTop`. That option would put row 0 (the oldest) at the bottom and invert every `positionViewAtIndex` / `indexAt` path. The margin approach keeps the model order, the full-height wheel and selection area, the `UnseenJumpButton` anchoring, and every `findChild` handle.

Changes in [src/qml/TranscriptList.qml](src/qml/TranscriptList.qml), shared by `messageList` and `consoleList`:

```qml
topMargin: Math.max(0, height - contentHeight)

function topContentY() {
    return originY - topMargin;
}

function endContentY() {
    return topContentY() + Math.max(0, topMargin + contentHeight + bottomMargin - height);
}

function viewportAtTop() {
    return contentY <= topContentY() + 2;
}

function firstRenderedIndex() {
    var x = Math.max(1, width / 2);
    var y = Math.max(contentY, originY);
    var index = indexAt(x, y + 1);
    return index >= 0 ? index : indexAt(x, y + 8);
}

onTopMarginChanged: if (stick === stickFollowing) stickToEnd()
```

- `endContentY()` now accounts for the margin. In the short state, start and end coincide, so the rows rest on the bottom.
- `onTopMarginChanged` covers the binding-order race with `onContentHeightChanged`. The margin can update after the height handler has already read a stale value.
- `rememberDetachedViewportSequence()` and `snapshotAnchor()` switch from `indexAt(..., contentY + 1)` to `firstRenderedIndex()`. In the short state `contentY + 1` lands in the empty margin and returns -1. That would leave Page Up history paging with no anchor, so loaded history would jump.
- `typingTranscript` is the list footer and is part of `contentHeight`, so the typing dots also sit on the bottom.

Window wiring in [src/OmaircWindow.qml](src/OmaircWindow.qml) `scrollTranscript()` replaces lines and adds none:
- Replace `first === 0 && list.contentY <= 2` with `first === 0 && list.viewportAtTop()`.
- Compute `first` with `list.firstRenderedIndex()`.

The old check assumed the top of the list is `contentY == 0`. That is no longer true with a margin. It was also fragile on long lists, because `originY` drifts as variable-height rows lay out.

## TUI: pad above instead of below

- In [tui/internal/ui/model.go](tui/internal/ui/model.go), add `padAbove(lines []string, limit int) []string` next to `fitLines`. It prepends blanks until the slice reaches `limit` and keeps the last `limit` lines when the input is longer.
- In `transcriptView`, use it for the row block, so short transcripts and Status hug the composer under the pinned header.
- The scroll math is unchanged. `transcriptWindow`, `pageTranscript`, and `firstVisibleMessageRow` already index from the tail, and `start` clamps to 0.

## Tests first, to reproduce the gap

Write these tests before the fix so they fail first, then make them pass.

Qt, in [tests/ui/tst_omairc.qml](tests/ui/tst_omairc.qml):
- `test_shortTranscriptSitsAtBottom`:
  - Open seeded `#help`, which is already short (see `test_shortTranscriptHidesScrollbar`).
  - The bottom edge of the last delegate, mapped into the list, is within 1px of `list.height`.
  - The first row's top is greater than 0.
  - Do the same for Status (`consoleList`) with a few lines.
- `test_shortTranscriptGrowsUpward`:
  - Use `shortTranscriptComponent` and append rows.
  - After each append, the last row's bottom stays at `list.height` and the first row's top moves up.
  - Once the content overflows, `topMargin === 0`, the list stays pinned, and the scrollbar thumb appears.
- `test_shortTranscriptResizeKeepsBottom`: changing the list height in either direction keeps the last row on the bottom.
- `test_shortTranscriptPageUpAnchorsHistory`: Page Up on a short transcript still calls `requestOlderTranscriptHistory`, and after `prependLiveReplay` the previously first row is still in view. This uses the anchor path that `firstRenderedIndex()` fixes.
- Re-run the existing following, detached, unseen, reveal, and remembered-place suites. They must stay green unchanged.

TUI, in `tui/internal/ui`:
- A test where a seeded short conversation, rendered in a tall terminal, shows blank lines between the header and the first row, and the last row sits on the last viewport line.
- A test where Status shows the same layout.
- Fix any existing assertion that encoded top alignment.

## Feature map

In [.cursor/skills/verify-omairc/features/keyboard.md](.cursor/skills/verify-omairc/features/keyboard.md), extend `keyboard-scroll`: "A transcript or Status shorter than the window rests on the composer; new rows push older ones up." The same text holds for both clients.

## Validation

- `bin/test` covers conventions, the C++ suite, the offscreen QML tests, and `tui/bin/test` when `go` is present.
- Visual check:
  - Run `./build/omairc --demo-server` (on Windows, `bin\build.bat` then `build\release\omairc.exe --demo-server`).
  - Open `#help` and an empty DM, send a few lines, and resize the window.
  - Check the screenshots under `test-artifacts/` for a pixel-exact bottom edge above the composer's 12px gap.
- Drive the TUI through `.cursor/skills/verify-omairc-tui` in a tall PTY.
- Branch off `main` from a GitHub issue and open the pull request against `main`.