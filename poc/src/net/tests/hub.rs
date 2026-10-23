//! The hub's task table and its closes. A worker whose connection ends on its own leaves
//! the table, a finish notice from an epoch the connection has left does not, and stopping
//! the hub drains what is already queued.

use std::time::{Duration, Instant};

use super::harness::{awaiting_peer, record, Rig};
use super::peer::Mode;
use crate::net::wire::ConnId;
use crate::net::NetOptions;
use crate::session::transport::{ConnectionState, Transport};

/// Delivers the hub's results until `done` holds. The results come first, so the events a
/// condition follows have reached the transport before it is checked. Panics if `done`
/// does not hold within five seconds.
fn settle_until(rig: &Rig, done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        rig.settle(&|| false);
        if done() {
            return;
        }
        assert!(Instant::now() < deadline, "the condition never held");
    }
}

/// A connection whose peer closes the socket ends its worker on its own. The worker's
/// finish notice removes the task, and the UI sees the clean disconnect.
#[test]
fn a_connection_that_ends_leaves_the_hub() {
    let rig = Rig::new(NetOptions::default());
    let transport = rig.hub.register();
    let (sink, _keep) = record(transport.as_ref());

    transport.connect("127.0.0.1", rig.peer.port(), false);
    rig.peer.accept(Mode::Plain).expect("the peer accepts");
    rig.settle(&|| awaiting_peer(&transport, false));
    assert_eq!(rig.hub.open_tasks(), 1);

    rig.peer.close();
    settle_until(&rig, || rig.hub.open_tasks() == 0);

    assert_eq!(transport.state(), ConnectionState::Disconnected);
    assert_eq!(*sink.disconnected.borrow(), 1);
}

/// A finish notice from an epoch the connection has left must not remove the task the
/// connection runs now. The first transport the hub registers is `ConnId(0)`, and its
/// first connect is epoch 1, so epoch 0 is stale.
#[test]
fn a_finish_from_an_earlier_epoch_leaves_the_current_task() {
    let rig = Rig::new(NetOptions::default());
    let transport = rig.hub.register();

    transport.connect("127.0.0.1", rig.peer.port(), false);
    rig.peer.accept(Mode::Plain).expect("the peer accepts");
    rig.settle(&|| awaiting_peer(&transport, false));
    assert_eq!(transport.state(), ConnectionState::Connected);
    assert_eq!(rig.hub.open_tasks(), 1);

    rig.hub.finish(ConnId(0), 0);

    assert_eq!(rig.hub.open_tasks(), 1);
    assert_eq!(transport.state(), ConnectionState::Connected);
}

/// Stopping the hub closes its connections the way `shutdown` does, so a frame queued
/// just before the hub drops still reaches the peer. Dropping the hub joins the net
/// thread, so the peer's read below sees the frame and then the close.
#[test]
fn a_frame_queued_before_the_hub_stops_reaches_the_peer() {
    let rig = Rig::new(NetOptions::default());
    let transport = rig.hub.register();

    transport.connect("127.0.0.1", rig.peer.port(), false);
    rig.peer.accept(Mode::Plain).expect("the peer accepts");
    rig.settle(&|| awaiting_peer(&transport, false));
    assert_eq!(transport.state(), ConnectionState::Connected);

    transport.write(b"QUIT :bye\r\n");
    drop(rig.hub);

    assert_eq!(rig.peer.received(), b"QUIT :bye\r\n");
}
