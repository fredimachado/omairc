//! Live ircd tests: controller + real net transport against Ergo, Solanum, ngIRCd.
//! Ignored by default. `bin/test-live` exports `OMAIRC_LIVE_*` and runs

mod caps;
mod channel;
mod classic;
mod helpers;
mod invention;
mod members;
mod multiline;
mod presence;
mod reconnect;
mod replies;
mod register;
mod restore;
