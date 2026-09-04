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

## Driving it with control-omairc

Preconditions:

- Mock conversation UI is showing (`#omarchy - Omairc`, members visible). That is `qml-suite` (`irc` left null), not a fresh compiled launch.
- A fresh compiled window is titled `irc.libera.chat Status` with Connect. Do not start this recipe there.
- For a desktop instance, the window is the isolated 1180x760 default, so the panel can appear (`width >= 980`).

- **Baseline panel.** Confirm the panel is open. Run `control-omairc screenshot --feature toggle-members --name members-open`. The right column shows `ONLINE - 12` and nicks including `anna`, and the people control reads `12 PEOPLE`.
- **Shortcut hide.** Press `Ctrl+Shift+M`. Run `control-omairc key --key ctrl+shift+m` then `control-omairc screenshot --feature toggle-members --name members-hidden`. The right column is gone. Run `control-omairc compare --before test-artifacts/verify/toggle-members/members-open.png --after test-artifacts/verify/toggle-members/members-hidden.png`.
- **Shortcut show.** Press `Ctrl+Shift+M` again. Run `control-omairc key --key ctrl+shift+m`. The member column returns with `ONLINE - 12`.
- **People control.** Click the people control. Run `control-omairc click-people`. The panel hides again. Click it once more. Run `control-omairc click-people`. The panel returns.
- **Channel count.** Switch to `#desktop` with the panel open. Run `control-omairc click-conversation --name "#desktop"` then `control-omairc wait-title --exact "#desktop - Omairc"`. The people control and `ONLINE -` heading both use `8`.
- **Direct message.** Open `anna`. Run `control-omairc click-conversation --name anna` then `control-omairc wait-title --exact "anna - Omairc"`. The people control is absent and `Ctrl+Shift+M` does not open a member column. Run `control-omairc key --key ctrl+shift+m` then `control-omairc screenshot --feature toggle-members --name dm-no-members`.
- **Proof.** Return to `#omarchy` with the panel visible. Run `control-omairc click-conversation --name "#omarchy"`, `control-omairc wait-title --exact "#omarchy - Omairc"`, and `control-omairc screenshot --feature toggle-members --name after-toggle`. The artifact shows `#omarchy`, `12 PEOPLE`, and `ONLINE - 12`.
- **Offscreen suite.** When Xvfb tools are missing, run `control-omairc doctor-qml` then `control-omairc qml-suite`. `bin/test` hides the panel with `Ctrl+Shift+M` on `#omarchy` and writes `test-artifacts/toggle-members.png`. `qml-suite` copies it to `test-artifacts/verify/toggle-members/members-hidden.png`. The right column is gone; `12 PEOPLE` remains. This does not prove the compiled-window shortcut path.

## Gotchas

- `Ctrl+Shift+M` is disabled on direct messages. A no-op there is correct, not a broken shortcut.
- The panel also stays hidden when the window is narrower than 980 CSS pixels. Isolated launch stays at 1180 wide so this does not apply unless geometry isolation failed.
- The people control label is the count plus ` PEOPLE`, not the words Hide/Show. Those names exist only as the control's accessible name.
- Clicking `fred` in the member list does nothing. That is not a toggle.
