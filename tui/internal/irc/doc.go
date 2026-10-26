// Package irc is the portable IRC core for omairc-tui.
// It mirrors `client/src/irc/` semantics exactly and stays platform-neutral: no
// socket, clock, filesystem, or OS call belongs here. Sockets, TLS, timers,
// and the filesystem live in internal/session and internal/controller, which
// inject time and wire bytes into this package.
//
// Invariants that must not drift from the desktop core:
//   - IrcServerFeatures parses ISUPPORT. Never hardcode CHANTYPES, CHANMODES
//     or PREFIX; ask the feature set. Case mapping, ParseNamesToken,
//     PrefixChanges, RankPriority, and MemberLabel match the C++ core.
//   - WireText decodes invalid UTF-8 as Latin-1 instead of injecting U+FFFD.
//   - The framer is faithful byte-for-byte, including the trailing-'\r'
//     retention while a bad frame is discarded.
//   - ConversationCause inserts verbatim from ircConversationCauseInserts:
//     UserOpen invents direct messages; ChannelState invents channels;
//     InboundOther invents channels and non-service direct messages; Restore
//     invents non-service direct messages; QuietSend and InboundSelf never
//     invent. EnsureConversation returns an existing conversation before it
//     consults the cause. Do not invent a second policy.
//   - IrcSecretPolicy redaction for Status and error previews fails closed.
//     Do not enumerate one more well-formed bypass.
//   - orderedMembers sorts by PREFIX rank then nick, in the core, never in the
//     view. The member panel and the CLI names order must match.
//   - Times are injected: the reducer and translator take an explicit
//     time.Time and never read the wall clock, so replay and tests stay
//     deterministic.
package irc
