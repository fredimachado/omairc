#pragma once

#include <QDateTime>
#include <QString>

#include <optional>

struct OmaircCliCursor
{
    QDateTime timestamp;
    QString msgid;
    qint64 sequence = 0;
};

class OmaircCliCursorStore
{
public:
    static QString defaultRoot();

    std::optional<OmaircCliCursor> load(const QString &networkId,
                                        const QString &target) const;
    bool save(const QString &networkId,
              const QString &target,
              const OmaircCliCursor &cursor) const;

private:
    QString pathFor(const QString &networkId, const QString &target) const;
};
