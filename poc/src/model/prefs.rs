//! Preferences as the sheets and the shell read them.
//!
//! The catalog, the value types and the in-memory `Preferences` are protocol vocabulary and
//! live in `irc/pref` and `irc/membershipnoise` (Qt `src/irc/ircpref.cpp` and
//! `src/irc/ircmembershipnoise.h`). They are re-exported here so the sheets keep their paths.
//! The values are kept by the controller and saved in the settings file (`storage/preferences`),
//! so this module does no I/O.

pub use crate::irc::pref::{PrefName, PrefValue, Preferences};
