#pragma once

#include "irccasemapping.h"

#include <QDateTime>
#include <QHash>
#include <QString>
#include <QVector>

#include <optional>

// Newest parseable server-time per target, per network. PLAY asks for each
// target from its own stamp; newest() is the latest among them. The stored
// value stays a UTC millisecond instant.
struct IrcPlaybackTargetTime
{
    QString target;
    QDateTime when;
};

class IrcPlaybackTimeStore
{
public:
    std::optional<QDateTime> newest(const QString& networkId) const;
    QVector<IrcPlaybackTargetTime> targets(const QString& networkId) const;
    std::optional<QDateTime> noted(const QString& networkId,
                                   const QString& target,
                                   const IrcCaseMapping& mapping) const;
    bool note(const QString& networkId,
              const QString& target,
              const QDateTime& when,
              const IrcCaseMapping& mapping);
    bool rekey(const QString& networkId,
               const QString& oldTarget,
               const QString& newTarget,
               const IrcCaseMapping& mapping);
    void forget(const QString& networkId);
    void setEphemeral(bool ephemeral);

private:
    struct Entry
    {
        QString target;
        qint64 epochMs = 0;
    };

    static int indexOfTarget(const QVector<Entry>& rows,
                             const QString& target,
                             const IrcCaseMapping& mapping);
    QVector<Entry> entries(const QString& networkId) const;
    QVector<Entry> load(const QString& networkId) const;
    void save(const QString& networkId, const QVector<Entry>& rows);

    bool m_ephemeral = false;
    mutable QHash<QString, QVector<Entry>> m_cache;
};

// ZNC `PLAY` from-stamp: Unix seconds with a millisecond fraction, or "0".
QString ircPlaybackPlayStamp(const std::optional<QDateTime>& when);
