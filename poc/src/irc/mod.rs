//! The portable IRC core. It mirrors `tui/internal/irc/` (Go) and `src/irc/`
//! (C++). Nothing here touches a socket, the filesystem, a thread, a process or
//! gpui; `tests/irc_portability.rs` enforces that. Each submodule names its own
//! Go and C++ source in its header.

pub mod avatarstore;
pub mod avatarurl;
pub mod autoaway;
pub mod capability;
pub mod capabilitynegotiation;
pub mod casemapping;
pub mod channelmode;
pub mod channelname;
pub mod command;
pub mod commandbuilder;
pub mod conversation;
#[cfg(test)]
mod corpus;
pub mod ctcp;
pub mod event;
pub mod framer;
pub mod inbox;
pub mod jointarget;
pub mod membershipnoise;
pub mod message;
pub mod parser;
pub mod plaintext;
pub mod prefixnick;
pub mod presence;
pub mod pref;
pub mod readmarker;
pub mod reducer;
pub mod secretpolicy;
pub mod serverfeatures;
pub mod servicenick;
pub mod slashcomplete;
pub mod statusentry;
pub mod storagepath;
pub mod textformat;
pub mod translator;
pub mod typing;
pub mod viewnotify;
pub mod world;
pub mod wiretext;
