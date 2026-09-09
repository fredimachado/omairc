# Member presence

Member presence is what each channel member row shows besides the nick: a presence dot, an optional status line, and a dimmed away state. Mock rows always show this chrome. A live session gates the dots and away dimming on `away-notify`, and gates status lines on both `draft/metadata-2` and `batch`.

## Sub-features

- `presence-dot` shows a green mark when the person is available and an amber mark when they are away.
- `presence-status` shows a status line under the nick when that person has one (`writing docs`, `on #desktop`, `making tea`, `new here`, `building Omairc` on the mock list).
- `presence-away` dims the nick and avatar for away members. On `#omarchy` those mock nicks are `teo`, `lena`, `sam`, `ivy`, and `max`.
- `presence-caps` hides the dots and away dimming without `away-notify`, and hides status lines unless both `draft/metadata-2` and `batch` are on. Those are independent.
- `presence-prefix` shows live rank glyphs (`@`, `+`, and the rest) on the member label. Mock labels have no glyphs.

## How to get to it (user POV)

- Open a channel with the member panel visible.
- Read the member rows. Scroll to reach away nicks on `#omarchy`.

## Driving it with control-omairc

Preconditions:

- Mock conversation UI is showing (`#omarchy - Omairc` or `#desktop - Omairc`, members visible). Use `control-omairc launch --mock`, or `qml-suite` (`irc` left null).
- `control-omairc launch` without `--mock` is first-run Connect titled `irc.libera.chat Status` with no member list. Do not start this recipe there.
- `presence-prefix` and live `presence-caps` need a completed Connect and a real session. That is not this recipe.

- **Channel chrome.** From the suite `#desktop` grab, confirm `ONLINE - 8` and rows for `anna` / `writing docs`, `dax` / `on #desktop`, and `teo` with the amber away mark. Run `control-omairc doctor-qml` then `control-omairc qml-suite`. `qml-suite` copies `test-artifacts/switch-channel.png` to `test-artifacts/verify/member-presence/after-desktop.png`.
- **Capability gate.** The same suite run covers `presence-caps` in `test_memberPresenceChromeFollowsCapabilities`. Turning `away-notify` off hides dots and dimming. Turning `draft/metadata-2` or `batch` off hides status lines. There is no compiled-window screenshot for that path.
- **PREFIX ranks.** Do not claim this on mock rows or on first-run Connect. It is `verified-unreachable` until a live session has completed Connect and received `PREFIX` / `NAMES`. The attempted compiled route is `control-omairc launch` (title `irc.libera.chat Status`, no members).

## Gotchas

- The heading stays `ONLINE - N` even when some people are away. That count is the people feature, not presence.
- Showing or hiding the whole panel is toggle-members. This feature is what the rows contain while the panel is open.
- Clicking a member still opens a DM. That is open-direct-message, not presence.
- Live rank glyphs replace the bare nick in the label (`@mira`). Mock rows stay `mira`.
- Bouncing ellipsis beside a nick is typing, not presence.
