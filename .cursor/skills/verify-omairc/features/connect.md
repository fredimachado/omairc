# Connect

Connect is the first-run sheet that asks for a network profile before the compiled window can show a live conversation.

## Sub-features

- `connect-first-run` shows the Connect overlay when no complete profile is saved.
- `connect-defaults` prefills Host `irc.libera.chat`, Port `6697`, TLS on, and Autojoin `#omarchy`.
- `connect-nick-required` shows `Nick is required` and keeps Apply muted while Nick is empty.
- `connect-required` keeps the sheet up on first run; Escape and an outside click do not dismiss it.

## How to get to it (user POV)

- Launch Omairc with no saved complete profile. The sheet is already open.
- After a profile exists, press `Ctrl+,` or click the small `edit` control in the sidebar header to open it again. The network name opens Status.

## Driving it with control-omairc

Preconditions:

- A default compiled launch (`control-omairc launch`) is titled `irc.libera.chat Status` and shows Connect. That is the desktop entry point.
- `control-omairc launch --mock` skips Connect and shows the prototype sidebar. Do not start this recipe there.
- When Xvfb tools are missing, use `control-omairc doctor-qml` then `control-omairc qml-suite`. Do not send input to the user's live window.

- **First-run sheet.** After an isolated launch, run `control-omairc title` then `control-omairc screenshot --feature connect --name first-run`. The title is `irc.libera.chat Status`. The overlay heading is `Connect`. Host is `irc.libera.chat`, port is `6697`, TLS is on, Autojoin is `#omarchy`, Nick is empty, the problem line is `Nick is required`, and Apply is muted.
- **Offscreen suite.** When Xvfb tools are missing, run `control-omairc doctor-qml` then `control-omairc qml-suite`. `bin/test` opens a window with a fake incomplete profile and writes `test-artifacts/connection-sheet.png`. `qml-suite` copies it to `test-artifacts/verify/connect/first-run.png`. The image must show `Connect`, `irc.libera.chat`, `Nick is required`, and muted Apply. This does not prove the compiled-window first-run path.

## Gotchas

- First run cannot be dismissed. Escape and a click outside the card only work after a complete profile already exists.
- Discard on first run restores the suggested Libera defaults. It does not close the sheet.
- Apply on the compiled window starts a real IRC session. That is not this feature's proof, and it does not restore the mock `#omarchy` sidebar.
- Mock conversation recipes need `control-omairc launch --mock` or `qml-suite` (`irc` left null). They do not start from this sheet.
- `qml-suite` overwrites `test-artifacts/verify/connect/first-run.png`. If this run also captured a compiled first-run, keep that file as `compiled-first-run.png` before running the suite.
