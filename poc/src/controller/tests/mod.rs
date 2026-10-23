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
mod console;
mod fixture;
mod inbox;
mod keychain;
mod left_channel;
mod lifecycle;
mod matrices;
mod metadata;
mod monitor;
mod networks;
mod parity;
mod peers;
mod persistence;
mod replies;
mod scrollplace;
mod selection;
mod shell;
mod sends;
mod source;
mod stores;
