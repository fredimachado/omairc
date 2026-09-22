# Member presence

Member presence is what each channel member row shows besides the nick: a presence dot, an optional status line, and a dimmed away state. Mock rows always show this chrome. A live session gates other members' dots and away dimming on `away-notify`, and gates status lines on both `draft/metadata-2` and `batch`. Our own row is the exception: our away state comes from the `305` / `306` numerics, so it shows without `away-notify` and matches the identity footer. A direct message row in the sidebar reads the same per-network facts for its peer: the avatar dot is online when a shared channel proves them available, away when the member list would mark them away, and muted when no shared channel proves them online.

## Sub-features

- `presence-dot` shows a green mark when the person is available and an amber mark when they are away.
- `presence-dm` mirrors a direct-message peer on the sidebar avatar dot. It is green when a shared channel proves the peer available, amber when the member list would mark them away, and muted when no shared channel proves them online. It reads the same per-network facts as member rows.
- `presence-status` shows a status line under the nick when that person has one (`writing docs`, `on #desktop`, `making tea`, `new here`, `building Omairc` on the mock list). Status is not away: it never dims the nick.
- `presence-bot` shows a tiny geometric bot mark next to the nick (`member-bot-dax` on seeded `#omarchy`) when `bot` metadata is set. It sits beside the name, not on the avatar, so the presence dot stays visible. Non-bots such as `anna` keep the mark hidden.
- `presence-avatar` clips a fetched HTTPS avatar into the nick circle when the image is Ready. Unsafe URLs (http, localhost, private IPs, file, data) fail closed and keep the initial. Mira's demo `https://example.com/.../{size}/...` URL is valid HTTPS; loading it is best-effort and initials stay if the fetch fails.
- `presence-away` dims the nick and avatar for away members. On `#omarchy` those mock nicks are `teo`, `lena`, `sam`, `ivy`, and `max`.
- `presence-caps` hides other members' dots and away dimming without `away-notify`, and hides status lines unless both `draft/metadata-2` and `batch` are on. Those are independent. Our own row keeps its dot and dimming either way, because `/away` and `/back` are answered by the `306` / `305` numerics rather than by `away-notify`. A direct message dot follows the same gate as other members' rows.
- `presence-prefix` shows the live rank glyphs on the member label. The seeded demo `#omarchy` carries `~fred`, `&anna`, `@dax`, `@mira` (op and voice at once), `%kai`, and `+teo`; every other seeded channel has plain nicks.
- `presence-rank-order` sorts rows by the server's `PREFIX` ladder, highest rank first, then nick. `#omarchy` therefore reads `~fred`, `&anna`, `@dax`, `@mira`, `%kai`, `+teo`, then `ivy`, `lena`, `max`, `nora`, `sam`, `sol` in nick order.

## How to get to it (user POV)

- Open a channel with the member panel visible.
- Read the member rows. Scroll to reach away nicks on `#omarchy`.

## Driving it with control-omairc

Preconditions:

- Seeded conversation UI is showing (`#omarchy · irc.example · fred - Omairc`, members visible). Use `control-omairc launch --demo-server`. When Xvfb tools are missing, `qml-suite` (seeded `IrcController`) is the fallback and is not compiled-window proof.
- `control-omairc launch` without `--demo-server` is first-run Connect titled `irc.libera.chat Status` with no member list. Do not start this recipe there.
- The compiled demo already paints presence dots, away dimming, status lines, and PREFIX glyphs on `#omarchy`. Capability gating and a real handshake are not this fence.

```desktop-recipe
launch --demo-server
wait-title --exact "#omarchy · irc.example · fred - Omairc"
jump --query "#omarchy"
wait-title --exact "#omarchy · irc.example · fred - Omairc"
screenshot --feature member-presence --name ranked-omarchy
```

- **Ranked #omarchy.** Press `Ctrl+K` and jump to `#omarchy`. Run `control-omairc jump --query "#omarchy"` then `control-omairc wait-title --exact "#omarchy · irc.example · fred - Omairc"`, then `control-omairc screenshot --feature member-presence --name ranked-omarchy`. The member panel shows presence dots, away dimming, status lines, and rank glyphs. The rows read `~fred`, `&anna`, `@dax`, `@mira`, `%kai`, `+teo`, then `ivy`, `lena`, `max`, `nora`, `sam`, `sol`. `anna` is typing, so three dots sit beside `&anna`.
- **Capability gate and injected NAMES.** `control-omairc doctor-qml` then `control-omairc qml-suite` covers `presence-caps` in `test_memberPresenceChromeFollowsCapabilities` and the self-away exception in `test_memberPresenceShowsOurOwnAwayWithoutAwayNotify`. `test_memberRowsFollowRankOrder` injects a `353` / `366` and mode changes. That suite is not compiled-window proof. Do not invent those frames on the desktop.
- **Direct message presence.** `bin/test` runs `directMessagePresenceFollowsAwayAndOffline` by injecting `AWAY` and `QUIT`. That is not a compiled-window recipe. `nick-jump --query teo` would open the DM, and the sidebar dot is presence, but this fence stays on the channel panel.
- **Mouse path.** `control-omairc click-member` opens a DM. That is open-direct-message, not this screenshot.

## Gotchas

- `run member-presence` skips `launch` when `doctor` is already healthy and `demo=yes`. It does not reset the member panel. A hidden panel makes this screenshot miss the rows. Recipes that need the panel open on a fresh demo need `cleanup` before `run`.
- The heading stays `ONLINE - N` even when some people are away. That count is the people feature, not presence.
- Showing or hiding the whole panel is toggle-members. This feature is what the rows contain while the panel is open.
- Clicking a member still opens a DM. That is open-direct-message, not presence.
- Live rank glyphs replace the bare nick in the label (`@mira`). Seeded `#omarchy` rows carry glyphs; `#desktop`, `#ricing`, `#help`, and every OFTC channel stay plain.
- Rank, away state, and status are independent. Seeded `teo` is voiced and away; the away members are not grouped together. A standing status line is never treated as away.
- Seeded `dax` is a bot (`PacketBot`) and keeps `@dax` plus a small mark beside the nick. The avatar circle still shows an initial unless a safe HTTPS image loads.
- Our own row is the only member row that shows away chrome without `away-notify`. It tracks the `305` / `306` numerics and the identity footer, not member presence.
- A direct message dot is presence, not unread state, and a peer with no shared channel reads muted rather than green.
- Without `away-notify`, the direct message dot follows other members' rows and stays hidden.
- Bouncing ellipsis beside a nick is typing, not presence.
