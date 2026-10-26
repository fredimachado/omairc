//! Fixtures and the Connect sheet's tests. The tests are grouped by what they pin, one
//! child module each under `tests/`, and the fixtures here are shared by all of them.

use super::*;

mod apply;
mod credentials;
mod fields;
mod focus;
mod rail;
mod stale;

fn seeded() -> (Vec<String>, ConnectState) {
    let state = ConnectState::seed(&demo_seeds());
    let populated = vec!["omarchy".to_string(), "oftc".to_string()];
    (populated, state)
}

fn stored_ids(state: &ConnectState) -> Vec<String> {
    state.saved().iter().map(|p| p.network_id.clone()).collect()
}

fn stored(id: &str, host: &str, nick: &str) -> StoredProfile {
    StoredProfile {
        name: host.to_owned(),
        host: host.to_owned(),
        nick: nick.to_owned(),
        autojoin_channels: vec!["#omarchy".to_owned()],
        ..StoredProfile::new(id)
    }
}

