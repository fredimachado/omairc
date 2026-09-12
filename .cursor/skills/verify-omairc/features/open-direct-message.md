# Open a direct message

Open a direct message lets a user click another person in a channel member list to jump to that conversation, creating a sidebar row when it is not already listed, with its own local transcript.

## Sub-features

- `dm-open-seeded` opens the existing `anna` conversation from her member row.
- `dm-create` adds a new DIRECT MESSAGES row when the nick is not already listed.
- `dm-history` shows that new conversation's own transcript, not the channel's.
- `dm-unread` clears anna's unread badge when her conversation is opened.
- `dm-self` ignores a click on `fred`.

## How to get to it (user POV)

- In a channel with the member panel open, click a member who is not `fred`.
- Press `Ctrl+Shift+P`, move with arrows, and press Enter on a member who is not `fred`.
- Click an existing DIRECT MESSAGES row (`anna` or `dax`).

## Driving it with control-omairc

Preconditions:

- Seeded conversation UI is showing (`#omarchy · irc.example · fred - Omairc`, members visible, no `mira` sidebar row). Use `control-omairc launch --demo-server`, or `qml-suite` (seeded `IrcController`).
- `control-omairc launch` without `--demo-server` is first-run Connect titled `irc.libera.chat Status`. Do not start this recipe there.
- For a desktop instance, the member panel is visible (`ONLINE - 12`) and this launch has not already created a `mira` sidebar row.

- **Open seeded anna.** Click `anna` in the member list. Run `control-omairc click-member --name anna` then `control-omairc wait-title --exact "anna - Omairc"`. The topic is `Direct message with anna`, the member panel is gone, and anna's unread badge is gone.
- **Return to channel.** Choose `#omarchy`. Run `control-omairc click-conversation --name "#omarchy"` then `control-omairc wait-title --exact "#omarchy - Omairc"`.
- **Create mira.** Click `mira` in the member list. Run `control-omairc click-member --name mira` then `control-omairc wait-title --exact "mira - Omairc"`. A `mira` row appears under DIRECT MESSAGES and the transcript is the empty start line `This is the beginning of your conversation with mira.`
- **Own history.** Send a line in mira's conversation. Run `control-omairc send --text "Hi mira from verify"`. Switch to `#omarchy` and back to mira. Run `control-omairc click-conversation --name "#omarchy"`, `control-omairc wait-title --exact "#omarchy - Omairc"`, `control-omairc click-member --name mira`, and `control-omairc wait-title --exact "mira - Omairc"`. The mira transcript still has `Hi mira from verify` and `#omarchy` does not.
- **Ignore self.** Return to `#omarchy` and click `fred`. Run `control-omairc click-conversation --name "#omarchy"`, `control-omairc wait-title --exact "#omarchy · irc.example · fred - Omairc"`, and `control-omairc click-member --name fred`. The title stays `#omarchy · irc.example · fred - Omairc`.
- **Proof.** Capture the created mira conversation. Run `control-omairc screenshot --feature open-direct-message --name mira-dm`. The screenshot shows title identity `mira`, a `mira` sidebar row, no member panel, and `Hi mira from verify`.
- **Offscreen suite.** When Xvfb tools are missing, run `control-omairc doctor-qml` then `control-omairc qml-suite`. `bin/test` clicks `mira` and writes `test-artifacts/open-direct-message.png`. `qml-suite` copies it to `test-artifacts/verify/open-direct-message/mira-dm.png`. The image must show `mira` selected, the beginning line, and no member panel. This does not prove the compiled-window `click-member` path.

## Gotchas

- Seeded DMs are only `anna` and `dax`. `mira` and the other nicks are created on first member click. `click-conversation` cannot target a created row; return to `mira` with `click-member` from a channel.
- Clicking `fred` is ignored. Do not treat a still-open channel as a failed DM open unless the click was on someone else.
- Opening a DM hides the member panel because the conversation is no longer a channel. Re-open a channel before proving another member click.
- `click-member` coordinates assume the panel is visible on the right of the 1180-wide window. If the panel is closed, open it with `Ctrl+Shift+M` first.
- Anna starts with unread `1` and a mention badge. Opening her conversation from either the sidebar or the member list clears it. Prove the badge after the open.
