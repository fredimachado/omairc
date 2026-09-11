# Connect

Connect is the first-run sheet that asks for a network profile before the compiled window can show a live conversation.

## Sub-features

- `connect-first-run` shows the Connect overlay when no complete profile is saved.
- `connect-defaults` prefills Host `irc.libera.chat`, Port `6697`, TLS on, and Autojoin `#omarchy`.
- `connect-on-startup` offers an opt-in `Connect automatically on startup` toggle, off by default.
- `connect-nick-required` shows `Nick is required` and keeps Apply muted while Nick is empty.
- `connect-required` keeps the sheet up on first run; Escape and an outside click do not dismiss it.
- `connect-keyboard` tabs from the network list through the form to Apply, and Shift+Tab returns. Enter Applies when Nick is complete. Enter from Host with an empty nick keeps `Nick is required` and does not start a session. Apply, Discard, and Add network are reachable without a mouse.
- `connect-scrollbar` shows a vertical scrollbar on the form when Password is off-screen, and on the network list once it overflows. The bars stay visible without hover.

## How to get to it (user POV)

- Launch Omairc with no saved complete profile. The sheet is already open.
- After a profile exists, press `Ctrl+,` or click the small `edit` control in the sidebar header to open it again. The network name opens Status.

## Driving it with control-omairc

Preconditions:

- A default compiled launch (`control-omairc launch`) is titled `irc.libera.chat Status` and shows Connect. That is the desktop entry point.
- `control-omairc launch --mock` skips Connect and shows the prototype sidebar. Do not start this recipe there.
- When Xvfb tools are missing, use `control-omairc doctor-qml` then `control-omairc qml-suite`. Do not send input to the user's live window.

- **First-run sheet.** After an isolated launch, run `control-omairc title` then `control-omairc screenshot --feature connect --name first-run`. The title is `irc.libera.chat Status`. The overlay heading is `Connect`. Host is `irc.libera.chat`, port is `6697`, TLS is on, Autojoin is `#omarchy`, `Connect automatically on startup` is off, Nick is empty, the problem line is `Nick is required`, and Apply is muted.
- **Offscreen suite.** When Xvfb tools are missing, run `control-omairc doctor-qml` then `control-omairc qml-suite`. `bin/test` opens a window with a fake incomplete profile and writes `test-artifacts/connection-sheet.png`. `qml-suite` copies it to `test-artifacts/verify/connect/first-run.png`. The image must show `Connect`, `irc.libera.chat`, `Nick is required`, and muted Apply. The same suite covers Tab from the network list to Apply, Enter from Host keeping `Nick is required`, Enter from Nick closing a complete sheet, and scrollbars on a short window and a long network list. This does not prove the compiled-window first-run path.
- **Keyboard on first run.** After an isolated launch, run `control-omairc key --key Tab` until Apply is focused, then `control-omairc key --key shift+Tab`. The focus ring returns through Discard. With Nick empty, `control-omairc key --key Return` from Host leaves the sheet open and `Nick is required` visible. Shrink the window until Password is off-screen. The form scrollbar is visible without hovering.

## Gotchas

- First run cannot be dismissed. Escape and a click outside the card only work after a complete profile already exists.
- Discard on first run restores the suggested Libera defaults. It does not close the sheet. An incomplete saved profile still opens Connect, but the fields follow that stored draft, not `suggested()`. Isolated `control-omairc launch` uses empty XDG, so it is first-run suggested values.
- Apply on the compiled window starts a real IRC session. That is not this feature's proof, and it does not restore the mock `#omarchy` sidebar.
- Mock conversation recipes need `control-omairc launch --mock` or `qml-suite` (`irc` left null). They do not start from this sheet.
- `qml-suite` overwrites `test-artifacts/verify/connect/first-run.png`. If this run also captured a compiled first-run, keep that file as `compiled-first-run.png` before running the suite.
- NickServ sits in the Tab order after Password. Forget links appear only when a stored secret exists. First run skips Add network, Forget, and Remove.
- Enter on a network row selects that network. A second Enter or Ctrl+Return Applies. Enter on Discard, Add network, or Forget activates that control.
