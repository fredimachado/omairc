// Package session owns the IRC session's I/O boundary for the omairc TUI.
//
// It is the only package in this module allowed to import net, crypto/tls, or
// crypto/rand. Everything above it — internal/irc and the future reducer and
// Bubble Tea shell — speaks in frames and events, never in sockets, and never
// reads the wall clock directly. Keeping the platform and crypto edges here is
// what lets internal/irc stay aligned with `client/src/irc/` and stay
// cross-platform by construction.
//
// This package mirrors the Rust transport and SCRAM files:
//
//   - Transport, Sink, and ConnectionState mirror IrcTransport in
//     `client/src/session/transport.rs`.
//   - NetTransport mirrors `client/src/net/transport.rs`: async
//     dial, buffer-until-encrypted write ordering, and distinct TLS failures.
//     It imports net and crypto/tls.
//   - LoopbackTransport mirrors `client/src/session/loopback.rs` and the
//     session transport tests (`client/src/session/tests/transport.rs`,
//     `transport_test.go`). Delivery is synchronous so tests are deterministic.
//   - SASLScram mirrors `client/src/session/saslscram.rs`, a SCRAM-SHA-256
//     client per RFC 5802 / RFC 7677. It imports crypto/sha256, crypto/hmac,
//     crypto/pbkdf2, and crypto/rand.
//   - Clock is the timer seam (mirroring the QTimer injection in
//     `client/src/session/session.rs`). RealClock is production; FakeClock lets tests drive
//     reconnect backoff, capability timeouts, the ping watchdog, the 45000ms
//     labeled-response timeout, and the typing refresh without sleeping.
//
// Nothing here frames or parses IRC; that is internal/irc. Nothing here decides
// policy; that is the session state machine built on top in a later wave. This
// package moves bytes and runs crypto, and reports what happened through Sink.
package session
