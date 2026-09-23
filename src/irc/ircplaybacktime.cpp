#include "ircplaybacktime.h"

#include <QByteArray>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QSettings>
#include <QTimeZone>

#include <algorithm>
#include <string>

namespace
{
const auto playbackTimesGroup = QStringLiteral("playbackTimes");
const auto timesKey = QStringLiteral("times");

std::string utf8(const QString& value)
{
    const QByteArray bytes = value.toUtf8();
    return std::string(bytes.constData(), std::size_t(bytes.size()));
}

bool targetEquals(const IrcCaseMapping& mapping, const QString& left, const QString& right)
{
    return mapping.equals(utf8(left), utf8(right));
}

QString stampText(qint64 epochMs)
{
    return QString::number(epochMs);
}

std::optional<qint64> stampMs(const QString& text)
{
    bool ok = false;
    const qint64 ms = text.toLongLong(&ok);
    if (!ok)
        return std::nullopt;
    return ms;
}
}

void IrcPlaybackTimeStore::setEphemeral(bool ephemeral)
{
    m_ephemeral = ephemeral;
    if (ephemeral)
        m_cache.clear();
}

QVector<IrcPlaybackTimeStore::Entry> IrcPlaybackTimeStore::load(
    const QString& networkId) const
{
    if (networkId.isEmpty())
        return {};
    if (m_ephemeral)
        return m_cache.value(networkId);

    QSettings settings;
    settings.beginGroup(playbackTimesGroup);
    settings.beginGroup(networkId);
    const QString stored = settings.value(timesKey).toString();
    const QJsonDocument document = QJsonDocument::fromJson(stored.toUtf8());
    if (!document.isArray())
        return {};

    QVector<Entry> rows;
    for (const QJsonValue& value : document.array()) {
        if (!value.isObject())
            continue;
        const QJsonObject object = value.toObject();
        const QString target = object.value(QStringLiteral("target")).toString();
        const std::optional<qint64> ms =
            stampMs(object.value(QStringLiteral("ms")).toString());
        if (target.isEmpty() || !ms)
            continue;
        rows.append(Entry{target, *ms});
    }
    return rows;
}

void IrcPlaybackTimeStore::save(const QString& networkId, const QVector<Entry>& rows)
{
    if (networkId.isEmpty())
        return;
    if (m_ephemeral) {
        if (rows.isEmpty())
            m_cache.remove(networkId);
        else
            m_cache.insert(networkId, rows);
        return;
    }

    QSettings settings;
    settings.beginGroup(playbackTimesGroup);
    if (rows.isEmpty()) {
        settings.remove(networkId);
    } else {
        QJsonArray array;
        for (const Entry& row : rows) {
            QJsonObject object;
            object.insert(QStringLiteral("target"), row.target);
            object.insert(QStringLiteral("ms"), stampText(row.epochMs));
            array.append(object);
        }
        settings.beginGroup(networkId);
        settings.setValue(timesKey,
                          QString::fromUtf8(QJsonDocument(array).toJson(
                              QJsonDocument::Compact)));
        settings.endGroup();
    }
    settings.endGroup();
    settings.sync();
}

QVector<IrcPlaybackTimeStore::Entry> IrcPlaybackTimeStore::entries(
    const QString& networkId) const
{
    if (networkId.isEmpty())
        return {};
    const auto found = m_cache.constFind(networkId);
    if (found != m_cache.cend())
        return found.value();
    const QVector<Entry> loaded = load(networkId);
    m_cache.insert(networkId, loaded);
    return loaded;
}

int IrcPlaybackTimeStore::indexOfTarget(const QVector<Entry>& rows,
                                       const QString& target,
                                       const IrcCaseMapping& mapping)
{
    for (int i = 0; i < rows.size(); ++i) {
        if (targetEquals(mapping, rows.at(i).target, target))
            return i;
    }
    return -1;
}

std::optional<QDateTime> IrcPlaybackTimeStore::newest(const QString& networkId) const
{
    const QVector<Entry> rows = entries(networkId);
    if (rows.isEmpty())
        return std::nullopt;
    qint64 maxMs = rows.first().epochMs;
    for (const Entry& row : rows)
        maxMs = std::max(maxMs, row.epochMs);
    return QDateTime::fromMSecsSinceEpoch(maxMs, QTimeZone::utc());
}

QVector<IrcPlaybackTargetTime> IrcPlaybackTimeStore::targets(
    const QString& networkId) const
{
    QVector<IrcPlaybackTargetTime> stamps;
    const QVector<Entry> rows = entries(networkId);
    stamps.reserve(rows.size());
    for (const Entry& row : rows) {
        stamps.append(IrcPlaybackTargetTime{
            row.target,
            QDateTime::fromMSecsSinceEpoch(row.epochMs, QTimeZone::utc()),
        });
    }
    return stamps;
}

std::optional<QDateTime> IrcPlaybackTimeStore::noted(
    const QString& networkId,
    const QString& target,
    const IrcCaseMapping& mapping) const
{
    if (networkId.isEmpty() || target.isEmpty())
        return std::nullopt;
    const QVector<Entry> rows = entries(networkId);
    const int index = indexOfTarget(rows, target, mapping);
    if (index < 0)
        return std::nullopt;
    return QDateTime::fromMSecsSinceEpoch(rows.at(index).epochMs, QTimeZone::utc());
}

bool IrcPlaybackTimeStore::note(const QString& networkId,
                                const QString& target,
                                const QDateTime& when,
                                const IrcCaseMapping& mapping)
{
    if (networkId.isEmpty() || target.isEmpty() || !when.isValid())
        return false;
    const qint64 ms = when.toUTC().toMSecsSinceEpoch();
    QVector<Entry> rows = entries(networkId);
    const int index = indexOfTarget(rows, target, mapping);
    if (index >= 0) {
        if (ms <= rows.at(index).epochMs)
            return false;
        rows[index].epochMs = ms;
    } else {
        rows.append(Entry{target, ms});
    }
    m_cache.insert(networkId, rows);
    save(networkId, rows);
    return true;
}

bool IrcPlaybackTimeStore::rekey(const QString& networkId,
                                 const QString& oldTarget,
                                 const QString& newTarget,
                                 const IrcCaseMapping& mapping)
{
    if (networkId.isEmpty() || oldTarget.isEmpty() || newTarget.isEmpty())
        return false;
    QVector<Entry> rows = entries(networkId);
    const int oldIndex = indexOfTarget(rows, oldTarget, mapping);
    if (oldIndex < 0)
        return false;
    const qint64 ms = rows.at(oldIndex).epochMs;
    if (targetEquals(mapping, rows.at(oldIndex).target, newTarget)) {
        if (rows.at(oldIndex).target == newTarget)
            return false;
        rows[oldIndex].target = newTarget;
    } else {
        rows.removeAt(oldIndex);
        const int existing = indexOfTarget(rows, newTarget, mapping);
        if (existing >= 0) {
            if (ms > rows.at(existing).epochMs)
                rows[existing].epochMs = ms;
        } else {
            rows.append(Entry{newTarget, ms});
        }
    }
    m_cache.insert(networkId, rows);
    save(networkId, rows);
    return true;
}

void IrcPlaybackTimeStore::forget(const QString& networkId)
{
    if (networkId.isEmpty())
        return;
    m_cache.remove(networkId);
    save(networkId, {});
}

QString ircPlaybackPlayStamp(const std::optional<QDateTime>& when)
{
    if (!when || !when->isValid())
        return QStringLiteral("0");
    const qint64 ms = when->toUTC().toMSecsSinceEpoch();
    const bool negative = ms < 0;
    const qint64 magnitude = negative ? -ms : ms;
    const qint64 seconds = magnitude / 1000;
    const int fraction = int(magnitude % 1000);
    QString text = QString::number(seconds);
    text += QLatin1Char('.');
    text += QString::number(fraction).rightJustified(3, QLatin1Char('0'));
    if (negative)
        text.prepend(QLatin1Char('-'));
    return text;
}
