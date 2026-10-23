//! The portable IRC core. It mirrors `tui/internal/irc/` (Go) and `src/irc/`
//! (C++). Nothing here touches a socket, the filesystem, a thread, a process or
//! gpui; `tests/irc_portability.rs` enforces that. Each submodule names its own
//! Go and C++ source in its header.

pub mod avatarstore;
pub mod avatarurl;
// Phase 4: the controller's auto-away runtime and the slash dispatcher's /autoaway
// read these. The allow comes off with the dispatcher's first use.
#[allow(dead_code)]
pub mod autoaway;
pub mod capability;
pub mod capabilitynegotiation;
// Phase 1 (protocol core) modules. Their consumers arrive in Phases 2-3, so the
// dead-code allow stays here until the session and reducer call them.
pub mod casemapping;
#[allow(dead_code)]
pub mod channelmode;
pub mod channelname;
pub mod command;
pub mod commandbuilder;
pub mod conversation;
#[cfg(test)]
mod corpus;
pub mod ctcp;
// The event taxonomy, the translator and the World shapes have no consumer until
// the Phase 3 reducer and controller land. The allows come off then.
pub mod event;
#[allow(dead_code)]
pub mod framer;
// Phase 3 leaf modules. Their reducer and controller consumers (the inbound
// typing hints, the presence store, the Status recorder, the inbox) land in the
// next Phase 3 steps, so the dead-code allow stays here until they call in.
pub mod inbox;
#[allow(dead_code)]
pub mod jointarget;
pub mod membershipnoise;
pub mod message;
pub mod parser;
pub mod plaintext;
pub mod prefixnick;
pub mod presence;
pub mod pref;
pub mod readmarker;
#[allow(dead_code, unused_imports)]
pub mod reducer;
pub mod secretpolicy;
#[allow(dead_code)]
pub mod serverfeatures;
pub mod servicenick;
pub mod slashcomplete;
pub mod statusentry;
pub mod textformat;
#[allow(dead_code)]
pub mod translator;
pub mod typing;
pub mod viewnotify;
pub mod world;
pub mod wiretext;
