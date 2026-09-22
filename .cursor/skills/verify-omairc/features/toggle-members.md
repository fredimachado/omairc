# Toggle members

Toggle members lets a user hide or show the channel member list from the header people control or `Ctrl+Shift+M`, and keeps that list off on direct messages.

## Sub-features

- `members-visible-default` shows the member panel on a channel in the default-width window.
- `members-shortcut` toggles the panel with `Ctrl+Shift+M`.
- `members-people-control` toggles the panel from the header people control.
- `members-count` keeps the header count and `ONLINE - N` heading on the same channel count.
- `members-hidden-on-dm` removes the people control and panel in a direct message.

## How to get to it (user POV)

- Click the header people control (`12 PEOPLE` on `#omarchy`).
- Press `Ctrl+Shift+M` while a channel is open.
- Press `Ctrl+Shift+P` to focus the list (and reopen it if it was hidden). That focus path is keyboard.

## Driving it with control-omairc

Preconditions:

- Seeded conversation UI is showing (`#omarchy · irc.example · fred - Omairc`, members visible). Use `control-omairc launch --demo-server`. When Xvfb tools are missing, `qml-suite` (seeded `IrcController`) is the fallback and is not compiled-window proof.
- `control-omairc launch` without `--demo-server` is first-run Connect titled `irc.libera.chat Status`. Do not start this recipe there.
- For a desktop instance, the window is the isolated 1180x760 default, so the panel can appear (`width >= 980`).

```desktop-recipe
launch --demo-server
wait-title --exact "#omarchy · irc.example · fred - Omairc"
screenshot --feature toggle-members --name members-open
toggle-members
screenshot --feature toggle-members --name members-hidden
compare --before test-artifacts/verify/toggle-members/members-open.png --after test-artifacts/verify/toggle-members/members-hidden.png
toggle-members
jump --query "#desktop"
wait-title --exact "#desktop - Omairc"
jump --query anna
wait-title --exact "anna - Omairc"
toggle-members
screenshot --feature toggle-members --name dm-no-members
jump --query "#omarchy"
wait-title --exact "#omarchy · irc.example · fred - Omairc"
screenshot --feature toggle-members --name after-toggle
```

- **Baseline panel.** Capture the open panel. Run `control-omairc screenshot --feature toggle-members --name members-open`. The right column shows `ONLINE - 12` and nicks including `anna`, and the people control reads `12 PEOPLE`. `compare` against `members-hidden` runs only after both grabs are real frames.
- **Shortcut hide.** Press `Ctrl+Shift+M`. Run `control-omairc toggle-members` then `control-omairc screenshot --feature toggle-members --name members-hidden`. The right column is gone. Run `control-omairc compare --before test-artifacts/verify/toggle-members/members-open.png --after test-artifacts/verify/toggle-members/members-hidden.png`. `compare` must report a pixel change.
- **Shortcut show.** Press `Ctrl+Shift+M` again. Run `control-omairc toggle-members`. The member column returns with `ONLINE - 12`.
- **Channel count.** Press `Ctrl+K` and jump to `#desktop`. Run `control-omairc jump --query "#desktop"` then `control-omairc wait-title --exact "#desktop - Omairc"`. The people control and `ONLINE -` heading both use `8`.
- **Direct message.** Jump to `anna`. Run `control-omairc jump --query anna` then `control-omairc wait-title --exact "anna - Omairc"`. The people control is absent. Press `Ctrl+Shift+M`. Run `control-omairc toggle-members` then `control-omairc screenshot --feature toggle-members --name dm-no-members`. The chord is a no-op. No member column appears.
- **Proof.** Jump back to `#omarchy`. The panel was shown again on the channel before the DM, and the DM chord did not flip it, so the column is visible. Run `control-omairc jump --query "#omarchy"`, `control-omairc wait-title --exact "#omarchy · irc.example · fred - Omairc"`, and `control-omairc screenshot --feature toggle-members --name after-toggle`. The artifact shows `#omarchy`, `12 PEOPLE`, and `ONLINE - 12`. The title is disambiguated because both networks have `#omarchy`.
- **Mouse path.** `control-omairc click-people` hides the panel while the column is open. After it hides, `control-omairc click-people --hidden` shows it. There is no chord for that control. It is not this recipe.
- **Offscreen suite.** When Xvfb tools are missing, run `control-omairc doctor-qml` then `control-omairc qml-suite`. `bin/test` hides the panel with `Ctrl+Shift+M` on `#omarchy` and writes `test-artifacts/toggle-members.png`. `qml-suite` copies it to `test-artifacts/verify/toggle-members/members-hidden.png`. The right column is gone; `12 PEOPLE` remains. This is not compiled-window proof.

## Gotchas

- `run toggle-members` skips `launch` when `doctor` is already healthy and `demo=yes`. It does not reset the member panel, unread badges, or sent lines. A hidden panel survives. Recipes that need the default open panel on a fresh demo need `cleanup` before `run`.
- `screenshot` retries a near-solid grab and fails the run if the frame stays flat. The open-panel shot is the first grab. Compare it to `members-hidden` only after that retry has a real frame.
- `Ctrl+Shift+M` is disabled on direct messages. A no-op there is correct, not a broken shortcut.
- `control-omairc toggle-members` sends `Control_L+Shift_L+m`. xdotool's `ctrl+shift+m` token does not reach the Qt shortcut on the isolated Xvfb.
- The panel also stays hidden when the window is narrower than 980 CSS pixels. Isolated launch stays at 1180 wide so this does not apply unless geometry isolation failed.
- The people control label is the count plus ` PEOPLE`, not the words Hide/Show. Those names exist only as the control's accessible name.
- `click-people` aims at `908,36` while the member column is open. After the column hides, use `click-people --hidden` (`1124,36`). The default click misses the shifted control.
- Clicking `fred` in the member list does nothing. That is not a toggle.
- Presence dots, away dimming, and status lines are member-presence, not this toggle.
