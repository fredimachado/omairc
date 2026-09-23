---
name: redact-secrets
description: Use when touching Status lines, error previews, or src/irc/ircsecretpolicy.cpp. Redaction fails closed; do not add another well-formed bypass.
metadata:
  internal: true
---

# Redact secrets

`IrcSecretPolicy::redactWireLine`, `redactPreviewLine`, and `redactMessage` are declared in `src/irc/ircsecretpolicy.h` and defined in `src/irc/ircsecretpolicy.cpp`. They fail closed. `redactWireLine` returns `std::nullopt` when the line does not tokenize or does not match a mask. `secretMapIsWellFormed()` in `ircsecretpolicy.cpp` is a `static_assert` on `kSecretMap`.

When a redaction case is unclear, drop the line (or keep the existing masked form such as `PASS ***`). Do not add another well-formed bypass to make one fixture pass. Extend the existing tests instead of a parallel redactor.

Status recording goes through `IrcStatusEntry` in `src/irc/ircstatusentry.cpp`: `buildDefaultIncoming` calls `redactMessage`, and `outgoing` calls `redactWireLine`. Error previews go through `IrcSecretPolicy::redactPreviewLine` in `previewWire` (`src/irc/ircsession.cpp`), which `describeMalformed` uses for protocol-error text.

Extend `SessionTest::malformedInputSurfacesProtocolError`, `SessionTest::overlongFrameLogsPreviewWithoutSecrets`, `SessionTest::configuredPasswordNeverAppearsInStatusEntries`, and `SessionTest::keyedJoinIsRedactedInStatusEntries` in `tests/session/tst_session.cpp`.
