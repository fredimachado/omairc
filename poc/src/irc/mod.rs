//! The portable IRC core. It mirrors `tui/internal/irc/` (Go) and `src/irc/`
//! (C++). Nothing here touches a socket, the filesystem, a thread, a process or
//! gpui; `tests/irc_portability.rs` enforces that. Each submodule names its own
//! Go and C++ source in its header.

// Phase 7 consumers (the avatar fetch and the presence photo) arrive with the UI,
// so the avatar policy and store carry the dead-code allow until then.
#[allow(dead_code)]
pub mod avatarstore;
#[allow(dead_code)]
pub mod avatarurl;
pub mod capability;
pub mod capabilitynegotiation;
// Phase 1 (protocol core) modules. Their consumers arrive in Phases 2-3, so the
// dead-code allow stays here until the session and reducer call them.
#[allow(dead_code)]
pub mod casemapping;
#[allow(dead_code)]
pub mod channelmode;
#[allow(dead_code)]
pub mod channelname;
// The slash catalog's dispatcher (the controller's CommandDispatcher) is the consumer of
// the rest of the parser and the outcome set. It lands in the next step, so the allow
// stays until it calls in.
#[allow(dead_code)]
pub mod command;
pub mod commandbuilder;
#[allow(dead_code)]
pub mod conversation;
#[cfg(test)]
mod corpus;
pub mod ctcp;
// The event taxonomy, the translator and the World shapes have no consumer until
// the Phase 3 reducer and controller land. The allows come off then.
#[allow(dead_code)]
pub mod event;
#[allow(dead_code)]
pub mod framer;
// Phase 3 leaf modules. Their reducer and controller consumers (the inbound
// typing hints, the presence store, the Status recorder, the inbox) land in the
// next Phase 3 steps, so the dead-code allow stays here until they call in.
#[allow(dead_code)]
pub mod inbox;
#[allow(dead_code)]
pub mod jointarget;
pub mod membershipnoise;
#[allow(dead_code)]
pub mod message;
#[allow(dead_code)]
pub mod parser;
pub mod plaintext;
pub mod prefixnick;
#[allow(dead_code)]
pub mod presence;
#[allow(dead_code)]
pub mod pref;
pub mod readmarker;
#[allow(dead_code, unused_imports)]
pub mod reducer;
pub mod secretpolicy;
#[allow(dead_code)]
pub mod serverfeatures;
pub mod servicenick;
// The rows the popup does not read yet (`has_label`, `needle`) are the dispatcher's and
// the tests', so the allow stays until the dispatcher calls in.
#[allow(dead_code)]
pub mod slashcomplete;
#[allow(dead_code)]
pub mod statusentry;
pub mod textformat;
#[allow(dead_code)]
pub mod translator;
pub mod typing;
pub mod viewnotify;
#[allow(dead_code)]
pub mod world;
pub mod wiretext;
