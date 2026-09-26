// Package irc is the portable IRC core for omairc-tui.
//
// It mirrors src/irc/ semantics exactly and stays platform-neutral:
//
//   - IrcConversationCause insertion rules are ported verbatim from
//     ircConversationCauseInserts; do not invent a second policy.
//   - IrcServerFeatures parses ISUPPORT; never hardcode CHANTYPES, CHANMODES,
//     or PREFIX.
//   - IrcSecretPolicy redaction for Status and error previews fails closed.
//   - orderedMembers sorts by PREFIX rank then nick, in the core, never in the
//     view.
//
// Later phases fill this package; Phase 0 ships only this placeholder so the
// module compiles.
package irc
