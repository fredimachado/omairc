# Member presence

Member presence is what each channel member row shows besides the nick: a presence dot, an optional status line, and a dimmed away state. Mock rows always show this chrome. A live session gates the dots and away dimming on `away-notify`, and gates status lines on both `draft/metadata-2` and `batch`.

## Sub-features

- `presence-dot` shows a green mark when the person is available and an amber mark when they are away.
- `presence-status` shows a status line under the nick when that person has one (`writing docs`, `on #desktop`, `making tea`, `new here`, `building Omairc` on the mock list).
- `presence-away` dims the nick and avatar for away members. On `#omarchy` those mock nicks are `teo`, `lena`, `sam`, `ivy`, and `max`.
- `presence-caps` hides the dots and away dimming without `away-notify`, and hides status lines unless both `draft/metadata-2` and `batch` are on. Those are independent.
- `presence-prefix` shows the live rank glyphs on the member label. The seeded demo `#omarchy` carries `~fred`, `&anna`, `@dax`, `@mira` (op and voice at once), `%kai`, and `+teo`; every other seeded channel has plain nicks.
- `presence-rank-order` sorts rows by the server's `PREFIX` ladder, highest rank first, then nick. `#omarchy` therefore reads `~fred`, `&anna`, `@dax`, `@mira`, `%kai`, `+teo`, then `ivy`, `lena`, `max`, `nora`, `sam`, `sol` in nick order.

## How to get to it (user POV)

- Open a channel with the member panel visible.
- Read the member rows. Scroll to reach away nicks on `#omarchy`.

## Driving it with control-omairc

Preconditions:

- Seeded conversation UI is showing (`#omarchy · irc.example · fred - Omairc` or `#desktop - Omairc`, members visible). Use `control-omairc launch --demo-server`, or `qml-suite` (seeded `IrcController`).
- `control-omairc launch` without `--demo-server` is first-run Connect titled `irc.libera.chat Status` with no member list. Do not start this recipe there.
- `presence-prefix` and live `presence-caps` need a completed Connect and a real session. That is not this recipe.

- **Channel chrome.** From the suite `#desktop` grab, confirm `ONLINE - 8` and rows for `anna` / `writing docs`, `dax` / `on #desktop`, and `teo` with the amber away mark. Run `control-omairc doctor-qml` then `control-omairc qml-suite`. `qml-suite` copies `test-artifacts/switch-channel.png` to `test-artifacts/verify/member-presence/after-desktop.png`.
- **Capability gate.** The same suite run covers `presence-caps` in `test_memberPresenceChromeFollowsCapabilities`. Turning `away-notify` off hides dots and dimming. Turning `draft/metadata-2` or `batch` off hides status lines. There is no compiled-window screenshot for that path.
- **PREFIX ranks and rank order.** `presence-prefix` and `presence-rank-order` are visible on the seeded `#omarchy` at launch: run `control-omairc launch --demo-server`, `control-omairc doctor`, then `control-omairc screenshot --feature member-presence --name ranked-omarchy`. The rows read `~fred`, `&anna`, `@dax`, `@mira`, `%kai`, `+teo`, then the plain nicks. The suite covers the same rows in `test_memberRowsFollowRankOrder`, which keeps `test-artifacts/member-rank-order.png` and `test-artifacts/member-rank-order-demoted.png`; `qml-suite` copies them to `test-artifacts/verify/member-presence/`. That test also injects `:server 353 fred = #omarchy :teo @anna @dax +mira fred kai sol` plus a `366` to prove a fresh `NAMES` reorders, then a `-o` and a `+o` to prove a promotion or demotion reorders without a model reset. A `353` replaces the whole member set, because it starts a `NAMES` sync.
- Rank glyphs arriving from a real server handshake are a live-session path, not a seeded one. The seeded `353` is the demo's stand-in.

## Gotchas

- The heading stays `ONLINE - N` even when some people are away. That count is the people feature, not presence.
- Showing or hiding the whole panel is toggle-members. This feature is what the rows contain while the panel is open.
- Clicking a member still opens a DM. That is open-direct-message, not presence.
- Live rank glyphs replace the bare nick in the label (`@mira`). Seeded `#omarchy` rows carry glyphs; `#desktop`, `#ricing`, `#help`, and every OFTC channel stay plain.
- Rank, away state, and status are independent. Seeded `teo` is voiced and away; the away members are not grouped together.
- Bouncing ellipsis beside a nick is typing, not presence.
