#include "ircplaybackcoordinator.h"

#include "irceventreducer.h"
#include "irceventtranslator.h"
#include "ircprefixnick.h"
#include "ircservicenick.h"
#include "ircsession.h"
#include "irctcp.h"
#include "ircwiretext.h"

#include <QDateTime>

#include <algorithm>
#include <string>

namespace
{
std::string utf8(const QString& value)
{
    const QByteArray bytes = value.toUtf8();
    return std::string(bytes.constData(), std::size_t(bytes.size()));
}

QString parameter(const IrcMessage& message, std::size_t index)
{
    if (index >= message.parameters.size())
        return {};
    return ircWireText(message.parameters[index]);
}

std::optional<QDateTime> serverTimeOf(const IrcMessage& message)
{
    for (const IrcTag& tag : message.tags) {
        if (tag.name != "time" || !tag.value)
            continue;
        const QString raw = ircWireText(*tag.value);
        if (raw.isEmpty())
            return std::nullopt;
        QDateTime parsed = QDateTime::fromString(raw, Qt::ISODateWithMs);
        if (!parsed.isValid())
            parsed = QDateTime::fromString(raw, Qt::ISODate);
        if (!parsed.isValid())
            return std::nullopt;
        return parsed.toUTC();
    }
    return std::nullopt;
}
}

IrcPlaybackCoordinator::IrcPlaybackCoordinator(IrcPlaybackTimeStore& times,
                                               IrcEventReducer& reducer)
    : m_times(times)
    , m_reducer(reducer)
{
}

void IrcPlaybackCoordinator::onRegistered(const QString& networkId,
                                          const QStringList& autojoinChannels)
{
    m_playbackSnapshot.insert(networkId, m_times.targets(networkId));
    m_zncAutojoin.insert(networkId, autojoinChannels);
    m_zncJoinedChannels.remove(networkId);
}

void IrcPlaybackCoordinator::onLeftRegistration(const QString& networkId)
{
    m_zncPlaybackSent.remove(networkId);
    m_playbackSnapshot.remove(networkId);
    m_zncAutojoin.remove(networkId);
    m_zncJoinedChannels.remove(networkId);
}

void IrcPlaybackCoordinator::dropSnapshot(const QString& networkId)
{
    m_playbackSnapshot.remove(networkId);
}

void IrcPlaybackCoordinator::rekey(const QString& networkId,
                                   const QString& oldTarget,
                                   const QString& newTarget)
{
    auto found = m_playbackSnapshot.find(networkId);
    if (found == m_playbackSnapshot.end() || oldTarget.isEmpty() || newTarget.isEmpty())
        return;
    const IrcCaseMapping& mapping =
        m_reducer.serverFeatures(networkId).caseMapping();
    QVector<IrcPlaybackTargetTime>& rows = found.value();
    int oldIndex = -1;
    for (int index = 0; index < rows.size(); ++index) {
        if (mapping.equals(utf8(rows.at(index).target), utf8(oldTarget))) {
            oldIndex = index;
            break;
        }
    }
    if (oldIndex < 0)
        return;
    const QDateTime when = rows.at(oldIndex).when;
    if (mapping.equals(utf8(oldTarget), utf8(newTarget))) {
        rows[oldIndex].target = newTarget;
        return;
    }
    rows.removeAt(oldIndex);
    for (IrcPlaybackTargetTime& row : rows) {
        if (!mapping.equals(utf8(row.target), utf8(newTarget)))
            continue;
        if (when.isValid() && (!row.when.isValid() || when > row.when))
            row.when = when;
        return;
    }
    rows.append(IrcPlaybackTargetTime{newTarget, when});
}

void IrcPlaybackCoordinator::noteKeptReplay(
    const std::function<bool(const QString& networkId)>& zncPlaybackCap)
{
    // PLAY's lower bound is exclusive. Only a replay line that landed, or
    // matched one already there, may move it. A held, dropped, or never-joined
    // channel batch stays eligible for the next PLAY.
    const std::vector<IrcKeptReplay> kept = m_reducer.takeKeptReplay();
    for (const IrcKeptReplay& line : kept) {
        // A line spliced before this target's first PLAY must not move the
        // exclusive bound that request is about to send, and must not be
        // stored for the next attach either.
        if (!mayNotePlaybackTime(line.networkId, line.target,
                                 zncPlaybackCap(line.networkId))) {
            continue;
        }
        m_times.note(line.networkId, line.target, line.serverTime,
                     m_reducer.serverFeatures(line.networkId).caseMapping());
    }
}

void IrcPlaybackCoordinator::notePlaybackClock(const QString& networkId,
                                               const IrcMessage& message,
                                               const QString& currentNick,
                                               bool zncPlaybackCap)
{
    if (networkId.isEmpty())
        return;
    if (ircWireText(message.command).compare(QLatin1String("PRIVMSG"), Qt::CaseInsensitive) != 0
        || message.parameters.size() < 2) {
        return;
    }
    const std::optional<QDateTime> when = serverTimeOf(message);
    if (!when)
        return;
    const auto ctcp = parseCtcpRequest(parameter(message, 1));
    if (ctcp && ctcp->command != QStringLiteral("ACTION"))
        return;

    const IrcServerFeatures& features = m_reducer.serverFeatures(networkId);
    const QString wireTarget = parameter(message, 0);
    if (!ircConversationFor(networkId, wireTarget, message, currentNick, features))
        return;

    const QString sender = ircPrefixNick(message);
    const bool channel = features.isChannel(utf8(wireTarget));
    const bool self =
        features.caseMapping().equals(utf8(sender), utf8(currentNick));
    const QString displayTarget = channel
        ? wireTarget
        : (self ? wireTarget : sender);
    if (displayTarget.isEmpty())
        return;
    if (!channel
        && (!ircNickIsRoutable(displayTarget)
            || ircTargetLooksLikeService(displayTarget, features))) {
        return;
    }
    // A self echo whose query or channel does not exist yet is InboundSelf,
    // so the reducer drops it. Each target's PLAY bound is exclusive, so
    // noting that line would skip bouncer history that was never kept.
    // An admitted line, or one that matches an existing row, may still move
    // the clock.
    if (self && !m_reducer.find(m_reducer.conversationKey(networkId, displayTarget)))
        return;
    // The first PLAY for this target reads the registration snapshot. A
    // line before that request is not stored, or the next attach skips the
    // same buffer. PLAY * 0 opens every target; a per-target PLAY opens
    // only that one.
    if (!mayNotePlaybackTime(networkId, displayTarget, zncPlaybackCap))
        return;
    m_times.note(networkId, displayTarget, *when, features.caseMapping());
}

void IrcPlaybackCoordinator::trimBouncerBatch(const QString& networkId,
                                              IrcHistoryEvent& event)
{
    const auto sent = m_zncPlaybackSent.constFind(networkId);
    if (sent == m_zncPlaybackSent.cend() || !sent->queries)
        return;
    const std::optional<QDateTime> bound = playbackSnapshotTime(networkId, event.target);
    if (!bound)
        return;
    const qint64 boundMs = bound->toUTC().toMSecsSinceEpoch();
    // PLAY * 0 answers every buffer, including ones a per-target
    // PLAY just resumed. ZNC's lower bound is exclusive against
    // microsecond buffer times, and FormatServerTime prints the
    // tag with %E3S. cctz truncates those three digits, so a line
    // still inside the exclusive bound can share the saved
    // millisecond. Drop anything older. An equal-millisecond line
    // with no msgid is the same on the wire as the one already
    // read, so it stays dropped: keeping it would restore a
    // cleared line, and dropping it can hide a new one. An
        // nonempty msgid is kept even when this query was not
        // restored. An inbound-only direct is absent after a cold
        // start, and that absence does not mean the id was seen.
        // The reducer opens the query only when an id is not
        // already in the transcript log, then hydrates and skips
        // an id it stored. A batch of ids the log already holds
        // does not put a closed direct back. /clear leaves those
        // ids in place while the conversation still exists.
    event.lines.erase(
        std::remove_if(
            event.lines.begin(), event.lines.end(),
            [boundMs](const IrcReplayLine& line) {
                if (!line.serverTime || !line.serverTime->isValid())
                    return false;
                const qint64 lineMs =
                    line.serverTime->toUTC().toMSecsSinceEpoch();
                if (lineMs > boundMs)
                    return false;
                if (lineMs < boundMs)
                    return true;
                return line.msgid.isEmpty();
            }),
        event.lines.end());
}

void IrcPlaybackCoordinator::noteJoinedChannel(const QString& networkId,
                                               const QString& channel)
{
    if (networkId.isEmpty() || channel.isEmpty())
        return;
    const IrcCaseMapping& mapping =
        m_reducer.serverFeatures(networkId).caseMapping();
    QStringList& channels = m_zncJoinedChannels[networkId];
    for (const QString& existing : channels) {
        if (mapping.equals(utf8(existing), utf8(channel)))
            return;
    }
    channels.append(channel);
}

bool IrcPlaybackCoordinator::sendZncPlayback(IrcSession *session,
                                             const QString& target,
                                             const QString& from)
{
    // ZNC dispatches the IRC command ZNC to the named module. PRIVMSG
    // *status :*playback is an unknown status command, so OnModCommand
    // never runs. PRIVMSG *playback would open a query on other clients.
    return session->sendRaw(
        QStringLiteral("ZNC *playback PLAY %1 %2").arg(target, from));
}

bool IrcPlaybackCoordinator::zncPlaybackCovers(const QString& networkId,
                                               const QString& normalizedTarget) const
{
    const auto found = m_zncPlaybackSent.constFind(networkId);
    if (found == m_zncPlaybackSent.cend())
        return false;
    return found.value().all || found.value().queries
        || found.value().targets.contains(normalizedTarget);
}

std::optional<QDateTime> IrcPlaybackCoordinator::playbackSnapshotTime(
    const QString& networkId,
    const QString& target) const
{
    const auto found = m_playbackSnapshot.constFind(networkId);
    if (found == m_playbackSnapshot.cend() || target.isEmpty())
        return std::nullopt;
    const IrcCaseMapping& mapping =
        m_reducer.serverFeatures(networkId).caseMapping();
    for (const IrcPlaybackTargetTime& row : found.value()) {
        if (mapping.equals(utf8(row.target), utf8(target)))
            return row.when;
    }
    return std::nullopt;
}

bool IrcPlaybackCoordinator::mayNotePlaybackTime(const QString& networkId,
                                                 const QString& target,
                                                 bool zncPlaybackCap) const
{
    if (networkId.isEmpty() || target.isEmpty())
        return false;
    // No playback request is coming. A line the user actually saw is the
    // stamp a later playback-capable attach should resume from.
    if (!zncPlaybackCap)
        return true;
    return zncPlaybackCovers(
        networkId,
        m_reducer.conversationKey(networkId, target).normalizedTarget);
}

void IrcPlaybackCoordinator::request(IrcSession *session,
                                     bool zncPlaybackCap,
                                     bool motdSeen,
                                     const QStringList& restoredDirects,
                                     const std::function<bool(const QString& target)>& persistableDirect)
{
    if (!session || session->state() != IrcSession::State::Registered)
        return;
    const QString networkId = session->networkId();
    // Open queries are restored at MOTD end. PLAY before that uses a stamp
    // that can skip self-only lines whose direct does not exist yet.
    if (!motdSeen)
        return;
    if (!zncPlaybackCap)
        return;
    const auto sent = m_zncPlaybackSent.constFind(networkId);
    if (sent != m_zncPlaybackSent.cend() && sent.value().all)
        return;

    const IrcServerFeatures& features = m_reducer.serverFeatures(networkId);
    const IrcCaseMapping& mapping = features.caseMapping();
    // Registration snapshot, taken at numeric 001. Traffic on this
    // connection must not replace the stamp this first PLAY sends.
    const QVector<IrcPlaybackTargetTime> stored = m_playbackSnapshot.value(networkId);

    // Nothing stored yet: one PLAY * 0 fetches every buffer, including
    // queries. Autojoin must not replace that with per-channel PLAY. The
    // module drops a channel PLAY until that channel is on, and PLAY * 0
    // does not mark the channel covered.
    if (stored.isEmpty()) {
        const bool alreadySpecific =
            sent != m_zncPlaybackSent.cend() && !sent.value().targets.isEmpty();
        if (!alreadySpecific) {
            if (!sendZncPlayback(session, QStringLiteral("*"), QStringLiteral("0")))
                return;
            m_zncPlaybackSent[networkId].all = true;
        }
    } else {
        for (const IrcPlaybackTargetTime& row : stored) {
            const QString normalized =
                m_reducer.conversationKey(networkId, row.target).normalizedTarget;
            if (zncPlaybackCovers(networkId, normalized))
                continue;
            if (!sendZncPlayback(session, row.target, ircPlaybackPlayStamp(row.when)))
                return;
            m_zncPlaybackSent[networkId].targets.insert(normalized);
        }
        // A restored direct with no stamp is absent from the store, so the
        // loop above skips it once any other target has one. Ask from the
        // beginning. A nick the user never opened is not listed.
        for (const QString& nick : restoredDirects) {
            if (!persistableDirect(nick))
                continue;
            if (playbackSnapshotTime(networkId, nick))
                continue;
            const IrcConversationKey key =
                m_reducer.conversationKey(networkId, nick);
            if (!m_reducer.find(key))
                continue;
            if (zncPlaybackCovers(networkId, key.normalizedTarget))
                continue;
            if (!sendZncPlayback(session, nick, QStringLiteral("0")))
                return;
            m_zncPlaybackSent[networkId].targets.insert(key.normalizedTarget);
        }
        // Snapshot from registration, before this connection's JOIN echoes.
        // A channel joined before the MOTD is not autojoin for this request.
        for (const QString& channel : m_zncAutojoin.value(networkId)) {
            if (channel.isEmpty() || !features.isChannel(utf8(channel)))
                continue;
            if (playbackSnapshotTime(networkId, channel))
                continue;
            const QString normalized =
                m_reducer.conversationKey(networkId, channel).normalizedTarget;
            if (zncPlaybackCovers(networkId, normalized))
                continue;
            if (!sendZncPlayback(session, channel, QStringLiteral("0")))
                return;
            m_zncPlaybackSent[networkId].targets.insert(normalized);
        }
        // Offline query buffers are absent from the snapshot, restored
        // directs, and autojoin. With znc.in/playback enabled the module
        // suppresses automatic delivery, so PLAY * 0 discovers them.
        // Known targets already got per-target PLAY with their resume
        // bounds. handleHistoryBatch drops wildcard lines older than the
        // registration stamp, and an equal-millisecond line that has no
        // msgid. A distinct msgid at that millisecond is kept.
        if (!m_zncPlaybackSent[networkId].queries) {
            if (!sendZncPlayback(session, QStringLiteral("*"), QStringLiteral("0")))
                return;
            m_zncPlaybackSent[networkId].queries = true;
        }
    }
    // A channel joined before this request still needs its own PLAY when
    // no playback batch for it has been kept. PLAY * 0 does not cover that
    // retry: a channel that was not on yet never answered.
    const QStringList joined = m_zncJoinedChannels.value(networkId);
    for (const QString& channel : joined)
        requestChannelPlayback(session, channel, zncPlaybackCap, motdSeen);
}

void IrcPlaybackCoordinator::requestChannelPlayback(IrcSession *session,
                                                    const QString& channel,
                                                    bool zncPlaybackCap,
                                                    bool motdSeen)
{
    if (!session || channel.isEmpty()
        || session->state() != IrcSession::State::Registered) {
        return;
    }
    const QString networkId = session->networkId();
    if (!motdSeen)
        return;
    if (!zncPlaybackCap)
        return;
    const IrcServerFeatures& features = m_reducer.serverFeatures(networkId);
    if (!features.isChannel(utf8(channel)))
        return;
    // Covered only after a playback batch was spliced or deduped. A PLAY
    // written at MOTD does not count, and neither does PLAY * 0: the module
    // emits nothing for a channel that is not on.
    if (m_reducer.playbackBatchKept(networkId, channel))
        return;
    // A self-JOIN retry still uses the registration snapshot, not a live
    // line from earlier in this connection.
    const QString from = ircPlaybackPlayStamp(
        playbackSnapshotTime(networkId, channel));
    if (!sendZncPlayback(session, channel, from))
        return;
    m_zncPlaybackSent[networkId].targets.insert(
        m_reducer.conversationKey(networkId, channel).normalizedTarget);
}
