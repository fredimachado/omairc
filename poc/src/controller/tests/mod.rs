//! The controller's frame-level tests. Each one drives an intent through the
//! controller, reads the frames the loopback wrote, injects the server's lines, and
//! checks the reads. The demo is the real `DemoServer`: nothing here stubs the
//! protocol. The Go cases they port are named in each file's header.

mod console;
mod fixture;
mod inbox;
mod lifecycle;
mod networks;
mod parity;
mod selection;
mod sends;
mod source;
