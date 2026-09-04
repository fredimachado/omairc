#include "ircnetworklog.h"

IrcNetworkLog::IrcNetworkLog(QObject *parent)
    : QObject(parent)
{
}

void IrcNetworkLog::append(const IrcStatusEntry& entry)
{
    if (entry.networkId().isEmpty())
        return;

    NetworkLog& log = m_logs[entry.networkId()];
    log.entries.push_back(entry);
    ++log.nextSequence;
    emit appended(entry.networkId(), int(log.entries.size()) - 1);

    if (int(log.entries.size()) > kMaxEntriesPerNetwork) {
        const int removed = int(log.entries.size()) - kMaxEntriesPerNetwork;
        for (int index = 0; index < removed; ++index)
            log.entries.pop_front();
        emit trimmed(entry.networkId(), removed);
    }
}

void IrcNetworkLog::clear(const QString& networkId)
{
    const auto found = m_logs.find(networkId);
    if (found == m_logs.end() || found->entries.empty())
        return;
    found->entries.clear();
    found->lastSeen = found->nextSequence - 1;
    emit cleared(networkId);
}

void IrcNetworkLog::forget(const QString& networkId)
{
    if (!m_logs.contains(networkId))
        return;
    m_logs.remove(networkId);
    emit cleared(networkId);
}

void IrcNetworkLog::markSeen(const QString& networkId)
{
    const auto found = m_logs.find(networkId);
    if (found == m_logs.end())
        return;
    found->lastSeen = found->nextSequence - 1;
}

int IrcNetworkLog::count(const QString& networkId) const
{
    const auto found = m_logs.constFind(networkId);
    return found == m_logs.cend() ? 0 : int(found->entries.size());
}

const IrcStatusEntry& IrcNetworkLog::at(const QString& networkId, int row) const
{
    const auto found = m_logs.constFind(networkId);
    Q_ASSERT(found != m_logs.cend());
    Q_ASSERT(row >= 0 && row < int(found->entries.size()));
    return found->entries[std::size_t(row)];
}

int IrcNetworkLog::alertsSinceSeen(const QString& networkId) const
{
    const auto found = m_logs.constFind(networkId);
    if (found == m_logs.cend() || found->entries.empty())
        return 0;

    const quint64 firstSequence = found->nextSequence - quint64(found->entries.size());
    int alerts = 0;
    for (std::size_t index = 0; index < found->entries.size(); ++index) {
        if (firstSequence + index <= found->lastSeen)
            continue;
        if (found->entries[index].severity() == IrcLogSeverity::Alert)
            ++alerts;
    }
    return alerts;
}
