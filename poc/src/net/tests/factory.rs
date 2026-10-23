//! The live factory's transports. `NetTransports::open` registers each transport on the
//! hub, so the results of its own connection reach it, and nothing reaches the others.

use super::harness::{record, Rig};
use super::peer::Mode;
use crate::controller::transports::TransportFactory;
use crate::net::{NetOptions, NetTransports};
use crate::session::transport::ConnectionState;

#[test]
fn an_opened_transport_reaches_its_peer() {
    let rig = Rig::new(NetOptions::default());
    let factory = NetTransports::new(rig.hub.clone());
    let transport = factory
        .open("omarchy")
        .expect("the live factory opens a transport");
    assert_eq!(transport.state(), ConnectionState::Idle);
    let (sink, _keep) = record(transport.as_ref());

    transport.connect("127.0.0.1", rig.peer.port(), false);
    rig.peer.accept(Mode::Plain).expect("the peer accepts");
    rig.settle(&|| transport.state() == ConnectionState::Connecting);

    assert_eq!(transport.state(), ConnectionState::Connected);
    assert_eq!(*sink.connected.borrow(), 1);
}

#[test]
fn each_open_is_its_own_connection() {
    let rig = Rig::new(NetOptions::default());
    let factory = NetTransports::new(rig.hub.clone());
    let first = factory.open("omarchy").expect("the first transport opens");
    let second = factory.open("oftc").expect("the second transport opens");
    let (first_sink, _first_keep) = record(first.as_ref());
    let (second_sink, _second_keep) = record(second.as_ref());

    first.connect("127.0.0.1", rig.peer.port(), false);
    rig.peer.accept(Mode::Plain).expect("the peer accepts");
    rig.settle(&|| first.state() == ConnectionState::Connecting);

    assert_eq!(first.state(), ConnectionState::Connected);
    assert_eq!(*first_sink.connected.borrow(), 1);
    assert_eq!(second.state(), ConnectionState::Idle);
    assert_eq!(*second_sink.connected.borrow(), 0);
}
