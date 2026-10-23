//! Newest server-time per target per network (`IrcPlaybackTimeStore`). Group
//! `playbackTimes/<networkId>`, key `times`. Gap 5 implements load, save, and note.

/// Persisted playback stamps per network. Ephemeral runs keep rows in memory only.
#[derive(Clone, Debug, Default)]
pub struct PlaybackTimeStore;
