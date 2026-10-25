//! Live ircd tests: controller + real net transport against Ergo, Solanum, ngIRCd.
//!
//! Ignored by default. `bin/test-live` exports `OMAIRC_LIVE_*` and runs
//! `cargo test --test live -- --ignored`.

mod caps;
mod classic;
mod helpers;
mod invention;
mod members;
mod presence;
mod reconnect;
mod register;
mod restore;
