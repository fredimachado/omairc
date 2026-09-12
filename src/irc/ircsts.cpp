#include "ircsts.h"

#include <QCryptographicHash>
#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QSettings>
#include <QStandardPaths>

namespace
{
const auto portKey = QStringLiteral("port");
const auto durationKey = QStringLiteral("duration");
const auto expiryKey = QStringLiteral("expiry");

QString tokenName(const QString &token)
{
    return token.section(QLatin1Char('='), 0, 0);
}

QString tokenValue(const QString &token)
{
    return token.contains(QLatin1Char('='))
        ? token.section(QLatin1Char('='), 1)
        : QString{};
}

IrcStsAdvertisement parseStsValue(const QString &value)
{
    IrcStsAdvertisement advertisement;
    const QStringList parts = value.split(QLatin1Char(','), Qt::SkipEmptyParts);
    for (const QString &part : parts) {
        const QString key = tokenName(part).toCaseFolded();
        const QString raw = tokenValue(part);
        if (key == QLatin1String("port")) {
            if (advertisement.port)
                continue;
            bool ok = false;
            const uint port = raw.toUInt(&ok);
            if (ok && port >= 1 && port <= 65535)
                advertisement.port = quint16(port);
            continue;
        }
        if (key == QLatin1String("duration")) {
            if (advertisement.durationSeconds)
                continue;
            bool ok = false;
            const qint64 duration = raw.toLongLong(&ok);
            if (ok && duration >= 0)
                advertisement.durationSeconds = duration;
        }
    }
    return advertisement;
}

QString groupKey(const QString &host)
{
    const QByteArray folded = host.trimmed().toCaseFolded().toUtf8();
    if (folded.isEmpty())
        return {};
    return QString::fromLatin1(
        QCryptographicHash::hash(folded, QCryptographicHash::Sha256).toHex());
}
}

std::optional<IrcStsAdvertisement> parseIrcStsAdvertisement(const QStringList &tokens)
{
    std::optional<IrcStsAdvertisement> advertisement;
    for (const QString &token : tokens) {
        if (tokenName(token).compare(QLatin1String("sts"), Qt::CaseInsensitive) != 0)
            continue;
        advertisement = parseStsValue(tokenValue(token));
    }
    return advertisement;
}

QString IrcStsStore::rootDir() const
{
    return QStandardPaths::writableLocation(QStandardPaths::GenericConfigLocation)
        + QLatin1String("/omairc");
}

QString IrcStsStore::filePath() const
{
    return rootDir() + QLatin1String("/sts");
}

void IrcStsStore::restrictPermissions() const
{
    QFile file(filePath());
    if (!file.exists())
        return;
    file.setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner);
}

std::optional<IrcStsCached> IrcStsStore::lookup(const QString &host)
{
    const QString key = groupKey(host);
    if (key.isEmpty())
        return std::nullopt;

    QSettings settings(filePath(), QSettings::IniFormat);
    settings.beginGroup(key);
    if (!settings.contains(portKey) || !settings.contains(expiryKey)) {
        settings.endGroup();
        return std::nullopt;
    }
    const uint port = settings.value(portKey).toUInt();
    const qint64 duration = settings.value(durationKey).toLongLong();
    const qint64 expiry = settings.value(expiryKey).toLongLong();
    settings.endGroup();

    if (port < 1 || port > 65535) {
        clear(host);
        return std::nullopt;
    }
    if (expiry <= QDateTime::currentSecsSinceEpoch()) {
        clear(host);
        return std::nullopt;
    }

    IrcStsCached cached;
    cached.port = quint16(port);
    cached.durationSeconds = duration;
    cached.expiryEpochSeconds = expiry;
    return cached;
}

void IrcStsStore::save(const QString &host, quint16 port, qint64 durationSeconds)
{
    if (durationSeconds == 0) {
        clear(host);
        return;
    }
    const QString key = groupKey(host);
    if (key.isEmpty() || port < 1)
        return;

    QDir().mkpath(rootDir());
    QSettings settings(filePath(), QSettings::IniFormat);
    settings.beginGroup(key);
    settings.setValue(portKey, port);
    settings.setValue(durationKey, QString::number(durationSeconds));
    settings.setValue(expiryKey,
                      QString::number(QDateTime::currentSecsSinceEpoch() + durationSeconds));
    settings.endGroup();
    settings.sync();
    restrictPermissions();
}

void IrcStsStore::clear(const QString &host)
{
    const QString key = groupKey(host);
    if (key.isEmpty())
        return;

    QSettings settings(filePath(), QSettings::IniFormat);
    settings.remove(key);
    settings.sync();
    restrictPermissions();
}
