# About

Clicking the app version on the right of the sidebar identity footer opens a modal About sheet. It is a calm homage to mIRC's About window: the Omairc name and version, the app logo, a short IRC description, an open-source note with a GitHub link, Check for Updates, OK, and a copyright footer. It is not a conversation and it does not change Connect, Status, or the selected channel.

## Sub-features

- `about-open` opens the sheet from the footer version on every screen, including first-run Connect.
- `about-content` shows About Omairc, the running version, the bundled logo, the open-source line, a GitHub source link, Check for Updates, OK, and `Copyright © 2026 Fredi Machado`. It does not show a license agreement, privacy policy, or third-party licenses.
- `about-updates` asks GitHub's latest release. Up to date, a newer version (as a release link), and a failed check each print one status line. The GitHub link and an available release open in the desktop browser.
- `about-dismiss` closes with Escape, OK, or a left-click on the dimmer. Right-click and clicks on the card do not dismiss it. Escape closes About before Connect or Status. Window chords stay blocked while it is open.
- `about-modal` covers the whole window with the same 50% opaque dimmer as Connect. Sidebar, transcript, and members stay visible behind it.

## How to get to it (user POV)

- Click the muted version number at the bottom-right of the sidebar footer.

## Driving it with control-omairc

Preconditions:

- A default compiled launch and `--demo-server` both show the version on the footer. The About sheet is the same either way.
- Named click `click-version` assumes the isolated 1180x760 window at textScale 1.0.

- **Open from the footer.** After `control-omairc launch --demo-server`, run `control-omairc screenshot --feature about --name before-about`, then `control-omairc click-version`, then `control-omairc screenshot --feature about --name after-open`. The sheet shows Omairc, the version, the logo, the open-source GitHub line, Check for Updates, OK, and the copyright footer. Run `control-omairc key --key Escape`. The sheet is gone and the conversation stays `#omarchy`.
- **Offscreen suite.** Run `control-omairc doctor-qml` then `control-omairc qml-suite`. `bin/test` clicks `selfVersionHit`, asserts the About labels, the GitHub link (`lastOpenedUrl`), Check for Updates against a fixture GitHub payload, a left-click on the dimmer dismissing the sheet, and Escape on first-run Connect. `qml-suite` copies `about-sheet.png` to `test-artifacts/verify/about/about-sheet.png`.

## Gotchas

- The nick, presence mark, and `available` / `away` / `offline` words are not the About entry point. Only the version on the right opens the sheet.
- Check for Updates talks to `api.github.com`. The offscreen suite injects a JSON payload and does not treat a live GitHub round-trip as proof.
- A compiled-window click on View the source on GitHub would call `Qt.openUrlExternally`. Do not use that as proof on the isolated display.
- Escape closes About before Connect. On first-run Connect, the second Escape does not dismiss the required sheet.
- Connect covers the window. A click on the footer version still opens About (the overlay and card forward that hit) and does not dismiss Connect. Conversation and member clicks stay blocked.
- About uses the same 50% dimmer as Connect. Only a left-click on that dimmer dismisses it; a right-click or a click on the About card does not.
