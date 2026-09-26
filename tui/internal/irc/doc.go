// Package irc is the portable IRC core for omairc-tui.
//
// It mirrors src/irc/ semantics exactly and stays platform-neutral: no socket,
// clock, filesystem, or OS call belongs here. Sockets, TLS, timers, and the
// filesystem live in internal/session and internal/controller, which inject
// time and wire bytes into this package.
//
// Phase 1 shipped the wire layer: message model and error taxonomy, parser,
// framer, case mapping, ISUPPORT server features, capability negotiation, the
// outbound command builder, CTCP, prefix-nick recovery, and wire-text
// decoding. Phase 2 adds the state model that sits between the wire and the
// view:
//
//   - event.go is the sealed Event taxonomy, one exported struct per
//     IrcEvent variant with an EventKind for switching and logging.
//   - presence.go, typing.go, servicenick.go, and jointarget.go hold the
//     per-nick presence and metadata facts, the typing hint retention rules,
//     service-identity and routable-nick predicates, and join-target
//     validation.
//   - conversation.go owns ConversationID normalization, sidebar order and
//     neighbor-after-drop, and the IrcConversationCause insert predicate.
//   - reducer.go is the EventReducer store: apply, ensureConversation,
//     unread/mention bookkeeping, ordered members, the message cap, msgid
//     dedup, nick merge, and the history splice machinery.
//   - translator.go turns a parsed wire Message into Events, and a replay
//     batch into a HistoryEvent.
//   - viewnotify.go classifies an Event into the view surfaces it dirties.
//   - secretpolicy.go and statusentry.go redact secrets for Status and error
//     previews and classify which lines reach the console.
//
// Invariants that must not drift from the Qt core:
//
//   - IrcServerFeatures parses ISUPPORT. Never hardcode CHANTYPES, CHANMODES,
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
//   - ConversationLog is an interface here and is nil in Phase 2; the
//     file-backed implementation lands with persistence.
//
// Later phases fill the command layer, demo server, and UI; they build on this
// package rather than re-parsing the wire.
package irc
