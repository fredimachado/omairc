//! The controller's frame-level tests. Each one drives an intent through the
//! controller, reads the frames the loopback wrote, injects the server's lines, and
//! checks the reads. The demo is the real `DemoServer`: nothing here stubs the
//! protocol. The Go cases they port are named in each file's header.

mod autoaway;
mod avatars;
mod channellist;
mod chrome;
mod commands;
mod composer;
mod credentials;
mod drafts;
mod console;
mod forget_credentials;
mod fileupload;
pub(crate) mod fixture;
mod inbox;
mod keychain;
mod keychain_wait;
mod left_channel;
mod lifecycle;
mod matrices;
mod metadata;
mod monitor;
mod notify;
mod network_header;
mod networks;
mod parity;
mod peers;
mod persistence;
mod persistence_closed;
mod persistence_network_lists;
mod persistence_sts;
mod playback;
mod readmarker;
mod readmarker_viewport;
mod replies;
mod scrollplace;
mod search;
mod selection;
mod shell;
mod sends;
mod source;
mod stores;
mod suspension;
mod verify_desktop_member_lists;
