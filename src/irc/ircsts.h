#pragma once

#include <QString>
#include <QStringList>

#include <optional>

struct IrcStsAdvertisement
{
    std::optional<quint16> port;
    std::optional<qint64> durationSeconds;
};

struct IrcStsCached
{
    quint16 port = 0;
    qint64 durationSeconds = 0;
    qint64 expiryEpochSeconds = 0;
};

std::optional<IrcStsAdvertisement> parseIrcStsAdvertisement(const QStringList &tokens);

class IrcStsStore
{
public:
    std::optional<IrcStsCached> lookup(const QString &host);
    void save(const QString &host, quint16 port, qint64 durationSeconds);
    void clear(const QString &host);
    QString filePath() const;

private:
    QString rootDir() const;
    void restrictPermissions() const;
};
