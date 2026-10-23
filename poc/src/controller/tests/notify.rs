//! Mention and monitor arrivals queued for the shell (`take_arrivals`).

use crate::controller::notify::Arrival;

use super::fixture::{Fixture, OMARCHY};

fn mention_arrivals(fixture: &mut Fixture) -> Vec<crate::irc::event::MentionArrival> {
    fixture
        .controller
        .take_arrivals()
        .into_iter()
        .filter_map(|arrival| match arrival {
            Arrival::Mention(mention) => Some(mention),
            Arrival::Monitor { .. } => None,
        })
        .collect()
}

fn monitor_arrivals(fixture: &mut Fixture) -> Vec<Arrival> {
    fixture
        .controller
        .take_arrivals()
        .into_iter()
        .filter(|arrival| matches!(arrival, Arrival::Monitor { .. }))
        .collect()
}

#[test]
fn mention_arrival_fires_once_per_mention() {
    let mut fixture = Fixture::seeded();
    fixture.deliver(OMARCHY, ":kai!u@h PRIVMSG #help :fred, ping?");
    let mentions = mention_arrivals(&mut fixture);
    assert_eq!(mentions.len(), 1);
    assert!(mention_arrivals(&mut fixture).is_empty());
}

#[test]
fn muted_monitor_does_not_queue_an_arrival() {
    let mut fixture = Fixture::seeded();
    fixture.controller.monitor_add(OMARCHY, "ghost");
    fixture.controller.apply_mute(OMARCHY, "ghost", true);
    fixture.deliver(OMARCHY, ":server 730 fred :ghost!u@h");
    assert!(monitor_arrivals(&mut fixture).is_empty());
}

#[test]
fn monitor_presence_change_queues_an_arrival() {
    let mut fixture = Fixture::seeded();
    fixture.controller.monitor_add(OMARCHY, "ghost");
    fixture.deliver(OMARCHY, ":server 730 fred :ghost!u@h");
    let arrivals = monitor_arrivals(&mut fixture);
    assert_eq!(arrivals.len(), 1);
    let Arrival::Monitor {
        display,
        body,
        network_id,
    } = &arrivals[0]
    else {
        panic!("expected monitor arrival");
    };
    assert_eq!(display, "ghost");
    assert_eq!(body, "is online");
    assert_eq!(network_id, OMARCHY);
}
