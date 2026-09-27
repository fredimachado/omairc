// Package irc is the portable IRC core for omairc-tui.
//
// It mirrors src/irc/ semantics exactly and stays platform-neutral: no socket,
// clock, filesystem, or OS call belongs here. Phase 1 ships the wire layer —
// message model and error taxonomy, parser, framer, case mapping, ISUPPORT
// server features, capability negotiation, the outbound command builder, CTCP,
// prefix-nick recovery, and wire-text decoding.
//
// Invariants that must not drift from the Qt core:
//
//   - IrcServerFeatures parses ISUPPORT. Never hardcode CHANTYPES, CHANMODES,
//     or PREFIX; ask the feature set. Case mapping, ParseNamesToken,
//     PrefixChanges, RankPriority, and MemberLabel match the C++ core.
//   - WireText decodes invalid UTF-8 as Latin-1 instead of injecting U+FFFD.
//   - The framer is faithful byte-for-byte, including the trailing-'\r'
//     retention while a bad frame is discarded.
//   - IrcConversationCause insertion rules are ported verbatim from
//     ircConversationCauseInserts when the reducer lands; do not invent a
//     second policy.
//   - IrcSecretPolicy redaction for Status and error previews fails closed
//     when that phase lands.
//   - orderedMembers sorts by PREFIX rank then nick, in the core, never in the
//     view.
//
// Later phases fill the session, reducer, and demo server; they build on this
// package rather than re-parsing the wire.
package irc
