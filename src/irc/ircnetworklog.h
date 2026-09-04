#pragma once

#include "ircstatusentry.h"

#include <QHash>
#include <QObject>
#include <QString>

#include <deque>

class IrcNetworkLog : public QObject
{
    Q_OBJECT
public:
    static constexpr int kMaxEntriesPerNetwork = 2000;

    explicit IrcNetworkLog(QObject *parent = nullptr);

    void append(const IrcStatusEntry& entry);
    void clear(const QString& networkId);
    void forget(const QString& networkId);
    void markSeen(const QString& networkId);

    int count(const QString& networkId) const;
    const IrcStatusEntry& at(const QString& networkId, int row) const;
    int alertsSinceSeen(const QString& networkId) const;

signals:
    void appended(const QString& networkId, int row);
    void trimmed(const QString& networkId, int removedFromFront);
    void cleared(const QString& networkId);

private:
    struct NetworkLog {
        std::deque<IrcStatusEntry> entries;
        quint64 nextSequence = 1;
        quint64 lastSeen = 0;
    };
    QHash<QString, NetworkLog> m_logs;
};
