# Member presence

Member presence is what each channel member row shows besides the nick: a presence dot, an optional status line, and a dimmed away state. Mock rows always show this chrome. A live session gates other members' dots and away dimming on `away-notify`, and gates status lines on both `draft/metadata-2` and `batch`. Our own row is the exception: our away state comes from the `305` / `306` numerics, so it shows without `away-notify` and matches the identity footer. A direct message row in the sidebar reads the same per-network facts for its peer: the avatar dot is online when a shared channel proves them available, away when the member list would mark them away, and muted when no shared channel proves them online.

## Sub-features

- `presence-dot` shows a green mark when the person is available and an amber mark when they are away.
- `presence-dm` mirrors a direct-message peer on the sidebar avatar dot. It is green when a shared channel proves the peer available, amber when the member list would mark them away, and muted when no shared channel proves them online. It reads the same per-network facts as member rows.
- `presence-status` shows a status line under the nick when that person has one (`writing docs`, `on #desktop`, `making tea`, `new here`, `building Omairc` on the mock list).
- `presence-away` dims the nick and avatar for away members. On `#omarchy` those mock nicks are `teo`, `lena`, `sam`, `ivy`, and `max`.
- `presence-caps` hides other members' dots and away dimming without `away-notify`, and hides status lines unless both `draft/metadata-2` and `batch` are on. Those are independent. Our own row keeps its dot and dimming either way, because `/away` and `/back` are answered by the `306` / `305` numerics rather than by `away-notify`. A direct message dot follows the same gate as other members' rows.
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
- **Capability gate.** The same suite run covers `presence-caps` in `test_memberPresenceChromeFollowsCapabilities`. Turning `away-notify` off hides other members' dots and dimming. Turning `draft/metadata-2` or `batch` off hides status lines. There is no compiled-window screenshot for that path. `test_memberPresenceShowsOurOwnAwayWithoutAwayNotify` covers the self exception: with `away-notify` off, our own row still paints the amber mark while another away row stays unpainted.
- **Self away across channels.** `bin/test` runs `selfAwayShowsOnEveryChannelRow`. On `--demo-server` with auto-echo, `/away lunch` gets the `306` and marks `fred` away on `#omarchy`, and the same mark follows to `#desktop`; `/back` gets the `305` and clears both. The demo does not echo our own away-notify, so this proves the client paints its own row from the numeric.
- **Direct message presence.** `bin/test` runs `directMessagePresenceFollowsAwayAndOffline`. It opens `teo` (seeded away in `#omarchy`) as a direct message and reads `presence` `away` on the sidebar row, injects `AWAY` to read `online`, then a `QUIT` to read `offline`. On the desktop, run `control-omairc launch --demo-server`, `control-omairc click-member --name teo`, and screenshot the sidebar: the new `teo` row carries the amber dot while `anna` and `dax` stay green. The offscreen suite covers the colors in `test_liveDmPresenceFollowsAwayFacts`.
- **PREFIX ranks and rank order.** `presence-prefix` and `presence-rank-order` are visible on the seeded `#omarchy` at launch: run `control-omairc launch --demo-server`, `control-omairc doctor`, then `control-omairc screenshot --feature member-presence --name ranked-omarchy`. The rows read `~fred`, `&anna`, `@dax`, `@mira`, `%kai`, `+teo`, then the plain nicks. The suite covers the same rows in `test_memberRowsFollowRankOrder`, which keeps `test-artifacts/member-rank-order.png` and `test-artifacts/member-rank-order-demoted.png`; `qml-suite` copies them to `test-artifacts/verify/member-presence/`. That test also injects `:server 353 fred = #omarchy :teo @anna @dax +mira fred kai sol` plus a `366` to prove a fresh `NAMES` reorders, then a `-o` and a `+o` to prove a promotion or demotion reorders without a model reset. A `353` replaces the whole member set, because it starts a `NAMES` sync.
- Rank glyphs arriving from a real server handshake are a live-session path, not a seeded one. The seeded `353` is the demo's stand-in.

## Gotchas

- The heading stays `ONLINE - N` even when some people are away. That count is the people feature, not presence.
- Showing or hiding the whole panel is toggle-members. This feature is what the rows contain while the panel is open.
- Clicking a member still opens a DM. That is open-direct-message, not presence.
- Live rank glyphs replace the bare nick in the label (`@mira`). Seeded `#omarchy` rows carry glyphs; `#desktop`, `#ricing`, `#help`, and every OFTC channel stay plain.
- Rank, away state, and status are independent. Seeded `teo` is voiced and away; the away members are not grouped together.
- Our own row is the only member row that shows away chrome without `away-notify`. It tracks the `305` / `306` numerics and the identity footer, not member presence.
- A direct message dot is presence, not unread state, and a peer with no shared channel reads muted rather than green.
- Without `away-notify`, the direct message dot follows other members' rows and stays hidden.
- Bouncing ellipsis beside a nick is typing, not presence.
