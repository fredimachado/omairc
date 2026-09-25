#pragma once

#include "ircevent.h"
#include "ircmessage.h"
#include "ircplaybacktime.h"

#include <QDateTime>
#include <QHash>
#include <QStringList>
#include <QSet>
#include <QVector>

#include <functional>
#include <optional>

class IrcEventReducer;
class IrcSession;

// Owns the per-connection playback state the controller used to keep: the
// registration snapshot, the ZNC PLAY requests already sent this
// connection, and the autojoin and joined-channel books. The controller
// forwards session events and passes the lookups it still owns (ZNC
// playback capability, MOTD-seen, open directs) as arguments; this type
// decides which PLAY requests go out and how a bouncer batch is trimmed.
class IrcPlaybackCoordinator
{
public:
    IrcPlaybackCoordinator(IrcPlaybackTimeStore& times, IrcEventReducer& reducer);

    // Lifetime: onRegistered takes a new snapshot from the persisted
    // clocks and records this connection's autojoin. onLeftRegistration
    // clears everything connection-scoped, because a reconnect must replay
    // as if the connection never happened. dropSnapshot clears only the
    // registration snapshot: forgetting a network drops the stamps the
    // next attach would resume from, but the three ZNC books stay because
    // the profile (and its ignore, mute, and highlight lists) survives.
    void onRegistered(const QString& networkId, const QStringList& autojoinChannels);
    void onLeftRegistration(const QString& networkId);
    void dropSnapshot(const QString& networkId);

    void rekey(const QString& networkId,
               const QString& oldTarget,
               const QString& newTarget);
    // Consumes the reducer's kept replay lines and stamps the persisted
    // clocks. zncPlaybackCap(networkId) reports whether the network
    // advertised znc.in/playback.
    void noteKeptReplay(
        const std::function<bool(const QString& networkId)>& zncPlaybackCap);
    void notePlaybackClock(const QString& networkId,
                           const IrcMessage& message,
                           const QString& currentNick,
                           bool zncPlaybackCap);
    // Drops wildcard PLAY lines older than the registration stamp from a
    // bouncer batch. The controller applies the event afterwards.
    void trimBouncerBatch(const QString& networkId, IrcHistoryEvent& event);
    void noteJoinedChannel(const QString& networkId, const QString& channel);
    void request(IrcSession *session,
                 bool zncPlaybackCap,
                 bool motdSeen,
                 const QStringList& restoredDirects,
                 const std::function<bool(const QString& target)>& persistableDirect);
    void requestChannelPlayback(IrcSession *session,
                                const QString& channel,
                                bool zncPlaybackCap,
                                bool motdSeen);

private:
    bool sendZncPlayback(IrcSession *session,
                         const QString& target,
                         const QString& from);
    bool zncPlaybackCovers(const QString& networkId,
                           const QString& normalizedTarget) const;
    std::optional<QDateTime> playbackSnapshotTime(const QString& networkId,
                                                  const QString& target) const;
    bool mayNotePlaybackTime(const QString& networkId,
                             const QString& target,
                             bool zncPlaybackCap) const;

    IrcPlaybackTimeStore& m_times;
    IrcEventReducer& m_reducer;
    // Stamps saved at numeric 001, before this connection's traffic. The
    // first PLAY for each target reads this. Later live lines update
    // m_times for the next attach and do not rewrite it.
    QHash<QString, QVector<IrcPlaybackTargetTime>> m_playbackSnapshot;
    // PLAY lines already written this connection. `all` is `PLAY * 0` on a
    // first attach with no saved stamps, which opens the clock for every
    // target. `queries` is `PLAY * 0` after per-target requests on later
    // attaches, which discovers offline query buffers. `targets` is each
    // per-target PLAY, including a channel PLAY, so lines after that request
    // may move the clock. A channel PLAY still does not stop a self-JOIN
    // retry: the module drops a channel that is not on, and the retry stops
    // only once a playback batch was kept.
    struct ZncPlaybackSent {
        bool all = false;
        // PLAY * 0 after per-target requests discovers query buffers that
        // appeared on ZNC while Omairc was offline.
        bool queries = false;
        QSet<QString> targets;
    };
    QHash<QString, ZncPlaybackSent> m_zncPlaybackSent;
    QHash<QString, QStringList> m_zncAutojoin;
    QHash<QString, QStringList> m_zncJoinedChannels;
};
