---
name: redact-secrets
description: Use when touching Status lines, error previews, or secret redaction in client/src/irc or tui/internal/irc/secretpolicy.go. Redaction fails closed; do not add another well-formed bypass.
metadata:
  internal: true
---

# Redact secrets

`IrcSecretPolicy` wire, preview, and message redaction are implemented in
`client/src/irc/` (Rust desktop) and `tui/internal/irc/secretpolicy.go` (Go
TUI). They fail closed. Wire redaction returns no replacement when the line
does not tokenize or does not match a mask. The secret map must stay well-formed
(static assert / unit test in each tree).

When a redaction case is unclear, drop the line (or keep the existing masked form such as `PASS ***`). Do not add another well-formed bypass to make one fixture pass. Extend the existing tests instead of a parallel redactor.

Status recording applies `redactMessage` / wire redaction on incoming and outgoing Status lines. Error previews apply preview redaction on malformed wire and overlong frames.

Extend the session/status tests in **both** implementations:

- Rust: search `client/` for malformed input, overlong frame, configured password, and keyed join redaction cases.
- Go: `tui/internal/session/` and `tui/internal/irc/` tests covering the same fixtures.

Keep `kSecretMap` / `secretMap` rows aligned across Rust and Go when you add a mask.
