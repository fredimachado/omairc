# Open a direct message

Open a direct message lets a user click another person in a channel member list to jump to that conversation, creating a sidebar row when it is not already listed, with its own local transcript.

## Sub-features

- `dm-open-seeded` opens the existing `anna` conversation from her member row.
- `dm-create` adds a new DIRECT MESSAGES row when the nick is not already listed.
- `dm-nick-sheet` opens or creates that DM from `Ctrl+Shift+K` or the `ONLINE` heading, including when the member panel is hidden.
- `dm-history` shows that new conversation's own transcript, not the channel's.
- `dm-unread` clears anna's unread badge when her conversation is opened.
- `dm-self` ignores a click on `fred`.
- `dm-transcript` opens or selects that nick from a chat or action author or avatar.

## How to get to it (user POV)

- In a channel with the member panel open, click a member who is not `fred`.
- Press `Ctrl+Shift+P`, move with arrows, and press Enter on a member who is not `fred`.
- Press `Ctrl+Shift+K` on a channel, type a nick substring, and press Enter. Click `ONLINE` in the member panel for the same sheet.
- Click an existing DIRECT MESSAGES row (`anna` or `dax`).
- Click the author or avatar on a chat or action row. Grouped follow-ups, event rows, WHOIS rows, and `fred` do nothing.

## Driving it with control-omairc

Preconditions:

- Seeded conversation UI is showing (`#omarchy · irc.example · fred - Omairc`, members visible, no `mira` sidebar row). Use `control-omairc launch --demo-server`. When Xvfb tools are missing, `qml-suite` (seeded `IrcController`) is the fallback and is not compiled-window proof.
- `control-omairc launch` without `--demo-server` is first-run Connect titled `irc.libera.chat Status`. Do not start this recipe there.
- For a desktop instance, this launch has not already created a `mira` sidebar row. `nick-jump` does not need the member panel.

```desktop-recipe
launch --demo-server
wait-title --exact "#omarchy · irc.example · fred - Omairc"
nick-jump --query anna
wait-title --exact "anna - Omairc"
jump --query "#omarchy"
wait-title --exact "#omarchy · irc.example · fred - Omairc"
nick-jump --query mira
wait-title --exact "mira - Omairc"
send --text "Hi mira from verify"
jump --query "#omarchy"
wait-title --exact "#omarchy · irc.example · fred - Omairc"
jump --query mira
wait-title --exact "mira - Omairc"
screenshot --feature open-direct-message --name mira-dm
jump --query "#omarchy"
wait-title --exact "#omarchy · irc.example · fred - Omairc"
nick-jump --query fred
wait-title --exact "#omarchy · irc.example · fred - Omairc"
```

- **Open seeded anna.** From `#omarchy`, press `Ctrl+Shift+K`, type `anna`, and press Enter. Run `control-omairc nick-jump --query anna` then `control-omairc wait-title --exact "anna - Omairc"`. The topic is `Direct message with anna`, the member panel is gone, and anna's unread badge is gone.
- **Return to channel.** Press `Ctrl+K` and jump to `#omarchy`. Run `control-omairc jump --query "#omarchy"` then `control-omairc wait-title --exact "#omarchy · irc.example · fred - Omairc"`. The query is a substring of the jump label. The first sidebar match is the omarchy network.
- **Create mira.** Press `Ctrl+Shift+K`, type `mira`, and press Enter. Run `control-omairc nick-jump --query mira` then `control-omairc wait-title --exact "mira - Omairc"`. A `mira` row appears under DIRECT MESSAGES and the transcript is the empty start line `This is the beginning of your conversation with mira.`
- **Own history.** Send a line, jump away, and jump back. Run `control-omairc send --text "Hi mira from verify"`, `control-omairc jump --query "#omarchy"`, `control-omairc wait-title --exact "#omarchy · irc.example · fred - Omairc"`, `control-omairc jump --query mira`, and `control-omairc wait-title --exact "mira - Omairc"`. The mira transcript still has `Hi mira from verify` and `#omarchy` does not. `click-conversation` does not know `mira`.
- **Ignore self.** Jump to `#omarchy` and nick-jump `fred`. Run `control-omairc jump --query "#omarchy"`, `control-omairc wait-title --exact "#omarchy · irc.example · fred - Omairc"`, `control-omairc nick-jump --query fred`, and `control-omairc wait-title --exact "#omarchy · irc.example · fred - Omairc"`. The title stays on the channel. Enter on yourself does not open a DM.
- **Proof.** Capture the created mira conversation after jumping away and back. Run `control-omairc screenshot --feature open-direct-message --name mira-dm`. The screenshot shows title identity `mira`, a `mira` sidebar row, no member panel, and `Hi mira from verify`.
- **Mouse path.** `control-omairc click-member --name anna` and `control-omairc click-member --name mira` open the same conversations when the member panel is visible. `control-omairc click-member --name fred` is ignored. There is no chord that aims a pixel. It is not this recipe.
- **Offscreen suite.** When Xvfb tools are missing, run `control-omairc doctor-qml` then `control-omairc qml-suite`. `bin/test` clicks `mira` and writes `test-artifacts/open-direct-message.png`. `qml-suite` copies it to `test-artifacts/verify/open-direct-message/mira-dm.png`. The image must show `mira` selected, the beginning line, and no member panel. This is not compiled-window proof.
- **Transcript nick.** `bin/test` runs `test_openDirectMessageFromTranscriptNick`. It clicks mira's avatar and author, ignores `fred` and event or grouped rows, keeps anna's existing buffer, and leaves NickServ on Status. Desktop `control-omairc` has no transcript-nick click.

## Gotchas

- `run open-direct-message` skips `launch` when `doctor` is already healthy and `demo=yes`. It does not reset created DMs or sent lines. A `mira` row from an earlier recipe survives. Recipes that need a fresh demo with no `mira` row need `cleanup` before `run`.
- Seeded DMs are only `anna` and `dax`. `mira` is created on the first `nick-jump` or member click. Return to that row with `jump --query mira`. `click-conversation` does not know `mira`.
- Clicking `fred` is ignored. Do not treat a still-open channel as a failed DM open unless the click was on someone else.
- Opening a DM hides the member panel because the conversation is no longer a channel. Re-open a channel before proving another member click.
- `click-member` coordinates assume the panel is visible on the right of the 1180-wide window. If the panel is closed, open it with `Ctrl+Shift+M` first.
- Anna starts with unread `1` and a mention badge. Opening her conversation from either the sidebar or the member list clears it. Prove the badge after the open.
