//! state (the reducer's sidebar rows, unread counts, ordered members, peer
//! labels, the rejoin through `/part` and `/join`). Those halves run in the
//! Phase 3 controller tests, which read this same seed. Here the demo's own
//! contract is checked at the wire: the bytes it seeds, the replies it sends
//! and the events a `Session` drains from them.

mod answers;
mod fixture;
mod seed;
mod session;
