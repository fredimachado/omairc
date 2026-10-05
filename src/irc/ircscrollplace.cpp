#include "ircscrollplace.h"

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QSettings>

#include <string>

namespace
{
const auto scrollPlacesGroup = QStringLiteral("scrollPlaces");
const auto placesKey = QStringLiteral("places");

std::string utf8(const QString& value)
{
    const QByteArray bytes = value.toUtf8();
    return std::string(bytes.constData(), std::size_t(bytes.size()));
}

bool targetEquals(const IrcCaseMapping& mapping, const QString& left, const QString& right)
{
    return mapping.equals(utf8(left), utf8(right));
}

QJsonObject placeJson(const QString& target, const IrcScrollPlace& place)
{
    QJsonObject object;
    object.insert(QStringLiteral("follow"), place.followEnd);
    object.insert(QStringLiteral("target"), target);
    if (place.followEnd)
        return object;
    if (!place.msgid.isEmpty()) {
        object.insert(QStringLiteral("msgid"), place.msgid);
        return object;
    }
    object.insert(QStringLiteral("author"), place.author);
    object.insert(QStringLiteral("body"), place.body);
    object.insert(QStringLiteral("kind"), place.kind);
    if (place.hasTime)
        object.insert(QStringLiteral("ms"), QString::number(place.epochMs));
    return object;
}

std::optional<IrcScrollPlace> placeFromJson(const QJsonObject& object, QString *target)
{
    const QString storedTarget = object.value(QStringLiteral("target")).toString();
    if (storedTarget.isEmpty() || !object.contains(QStringLiteral("follow")))
        return std::nullopt;
    IrcScrollPlace place;
    place.followEnd = object.value(QStringLiteral("follow")).toBool();
    if (!place.followEnd) {
        place.msgid = object.value(QStringLiteral("msgid")).toString();
        if (place.msgid.isEmpty()) {
            place.author = object.value(QStringLiteral("author")).toString();
            place.body = object.value(QStringLiteral("body")).toString();
            place.kind = object.value(QStringLiteral("kind")).toString();
            const QString msText = object.value(QStringLiteral("ms")).toString();
            if (!msText.isEmpty()) {
                bool ok = false;
                const qint64 ms = msText.toLongLong(&ok);
                if (!ok)
                    return std::nullopt;
                place.epochMs = ms;
                place.hasTime = true;
            }
        }
    }
    *target = storedTarget;
    return place;
}
}

void IrcScrollPlaceStore::setEphemeral(bool ephemeral)
{
    m_ephemeral = ephemeral;
    if (ephemeral)
        m_cache.clear();
}

std::optional<IrcScrollPlace> IrcScrollPlaceStore::place(
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
    return rows.at(index).place;
}

void IrcScrollPlaceStore::remember(const QString& networkId,
                                  const QString& target,
                                  const IrcScrollPlace& place,
                                  const IrcCaseMapping& mapping)
{
    if (networkId.isEmpty() || target.isEmpty())
        return;
    QVector<Entry> rows = entries(networkId);
    const int index = indexOfTarget(rows, target, mapping);
    if (index >= 0)
        rows[index] = Entry{target, place};
    else
        rows.append(Entry{target, place});
    m_cache.insert(networkId, rows);
    save(networkId, rows);
}

bool IrcScrollPlaceStore::rekey(const QString& networkId,
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
    const IrcScrollPlace kept = rows.at(oldIndex).place;
    if (targetEquals(mapping, rows.at(oldIndex).target, newTarget)) {
        if (rows.at(oldIndex).target == newTarget)
            return false;
        rows[oldIndex].target = newTarget;
    } else {
        rows.removeAt(oldIndex);
        const int existing = indexOfTarget(rows, newTarget, mapping);
        if (existing >= 0)
            rows[existing] = Entry{newTarget, kept};
        else
            rows.append(Entry{newTarget, kept});
    }
    m_cache.insert(networkId, rows);
    save(networkId, rows);
    return true;
}

void IrcScrollPlaceStore::forget(const QString& networkId)
{
    if (networkId.isEmpty())
        return;
    m_cache.remove(networkId);
    save(networkId, {});
}

int IrcScrollPlaceStore::indexOfTarget(const QVector<Entry>& rows,
                                      const QString& target,
                                      const IrcCaseMapping& mapping)
{
    for (int i = 0; i < rows.size(); ++i) {
        if (targetEquals(mapping, rows.at(i).target, target))
            return i;
    }
    return -1;
}

QVector<IrcScrollPlaceStore::Entry> IrcScrollPlaceStore::entries(
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

QVector<IrcScrollPlaceStore::Entry> IrcScrollPlaceStore::load(
    const QString& networkId) const
{
    if (networkId.isEmpty())
        return {};
    if (m_ephemeral)
        return m_cache.value(networkId);

    QSettings settings;
    settings.beginGroup(scrollPlacesGroup);
    settings.beginGroup(networkId);
    const QString stored = settings.value(placesKey).toString();
    const QJsonDocument document = QJsonDocument::fromJson(stored.toUtf8());
    if (!document.isArray())
        return {};

    QVector<Entry> rows;
    for (const QJsonValue& value : document.array()) {
        if (!value.isObject())
            continue;
        QString target;
        const std::optional<IrcScrollPlace> place = placeFromJson(value.toObject(), &target);
        if (!place)
            continue;
        rows.append(Entry{target, *place});
    }
    return rows;
}

void IrcScrollPlaceStore::save(const QString& networkId, const QVector<Entry>& rows)
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
    settings.beginGroup(scrollPlacesGroup);
    if (rows.isEmpty()) {
        settings.remove(networkId);
    } else {
        QJsonArray array;
        for (const Entry& row : rows)
            array.append(placeJson(row.target, row.place));
        settings.beginGroup(networkId);
        settings.setValue(placesKey,
                          QString::fromUtf8(QJsonDocument(array).toJson(
                              QJsonDocument::Compact)));
        settings.endGroup();
    }
    settings.endGroup();
    settings.sync();
}
