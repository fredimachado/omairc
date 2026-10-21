//! The portable IRC core. It mirrors `tui/internal/irc/` (Go) and `src/irc/`
//! (C++). Nothing here touches a socket, the filesystem, a thread, a process or
//! gpui; `tests/irc_portability.rs` enforces that. Each submodule names its own
//! Go and C++ source in its header.

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
pub mod command;
pub mod commandbuilder;
#[cfg(test)]
mod corpus;
pub mod ctcp;
pub mod event;
#[allow(dead_code)]
pub mod framer;
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
pub mod presence;
pub mod pref;
pub mod readmarker;
pub mod reducer;
pub mod secretpolicy;
#[allow(dead_code)]
pub mod serverfeatures;
pub mod servicenick;
pub mod slashcomplete;
pub mod statusentry;
pub mod textformat;
pub mod translator;
pub mod typing;
pub mod viewnotify;
pub mod wiretext;
