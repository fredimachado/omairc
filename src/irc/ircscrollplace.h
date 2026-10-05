#pragma once

#include "irccasemapping.h"

#include <QHash>
#include <QString>
#include <QVector>

#include <optional>

// Where one conversation's transcript was last left. followEnd means the
// viewport was pinned to the bottom. Otherwise the anchor is the first visible
// message: its msgid when the server gave one, else the author, kind, body,
// and server time. Both clients persist this in the scrollPlaces group so a
// restart opens the same line.
struct IrcScrollPlace
{
    bool followEnd = true;
    QString msgid;
    QString author;
    QString kind;
    QString body;
    // UTC milliseconds. hasTime is false when the message had no timestamp.
    qint64 epochMs = 0;
    bool hasTime = false;
};

class IrcScrollPlaceStore
{
public:
    std::optional<IrcScrollPlace> place(const QString& networkId,
                                        const QString& target,
                                        const IrcCaseMapping& mapping) const;
    void remember(const QString& networkId,
                  const QString& target,
                  const IrcScrollPlace& place,
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
        IrcScrollPlace place;
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
