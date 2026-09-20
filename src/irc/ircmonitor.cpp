#include "ircmonitor.h"

#include <QByteArray>
#include <QSettings>

#include <string>

namespace
{
const auto monitorsGroup = QStringLiteral("monitors");
const auto nicksKey = QStringLiteral("nicks");

std::string utf8(const QString& value)
{
    const QByteArray bytes = value.toUtf8();
    return std::string(bytes.constData(), std::size_t(bytes.size()));
}

bool nickEquals(const IrcCaseMapping& mapping, const QString& left, const QString& right)
{
    return mapping.equals(utf8(left), utf8(right));
}

int indexOfNick(const QStringList& nicks,
                const QString& nick,
                const IrcCaseMapping& mapping)
{
    for (int i = 0; i < nicks.size(); ++i) {
        if (nickEquals(mapping, nicks.at(i), nick))
            return i;
    }
    return -1;
}

bool listContains(const QStringList& nicks,
                  const QString& nick,
                  const IrcCaseMapping& mapping)
{
    return indexOfNick(nicks, nick, mapping) >= 0;
}
}

QStringList IrcMonitorStore::load(const QString& networkId) const
{
    if (networkId.isEmpty())
        return {};
    QSettings settings;
    settings.beginGroup(monitorsGroup);
    settings.beginGroup(networkId);
    return settings.value(nicksKey).toStringList();
}

void IrcMonitorStore::save(const QString& networkId, const QStringList& nicks)
{
    if (networkId.isEmpty())
        return;
    QSettings settings;
    settings.beginGroup(monitorsGroup);
    if (nicks.isEmpty()) {
        settings.remove(networkId);
    } else {
        settings.beginGroup(networkId);
        settings.setValue(nicksKey, nicks);
        settings.endGroup();
    }
    settings.endGroup();
    settings.sync();
}

const QStringList& IrcMonitorStore::cached(const QString& networkId) const
{
    const auto found = m_cache.constFind(networkId);
    if (found != m_cache.cend())
        return found.value();
    return m_cache.insert(networkId, load(networkId)).value();
}

QStringList IrcMonitorStore::nicks(const QString& networkId) const
{
    if (networkId.isEmpty())
        return {};
    return cached(networkId);
}

QStringList IrcMonitorStore::listed(const QString& networkId,
                                    const IrcCaseMapping& mapping) const
{
    QStringList unique;
    for (const QString& nick : nicks(networkId)) {
        if (!listContains(unique, nick, mapping))
            unique.append(nick);
    }
    return unique;
}

bool IrcMonitorStore::contains(const QString& networkId,
                               const QString& nick,
                               const IrcCaseMapping& mapping) const
{
    if (networkId.isEmpty() || nick.isEmpty())
        return false;
    return listContains(cached(networkId), nick, mapping);
}

bool IrcMonitorStore::add(const QString& networkId,
                          const QString& nick,
                          const IrcCaseMapping& mapping)
{
    if (networkId.isEmpty() || nick.isEmpty())
        return false;
    QStringList current = cached(networkId);
    if (listContains(current, nick, mapping))
        return false;
    current.append(nick);
    m_cache.insert(networkId, current);
    save(networkId, current);
    return true;
}

bool IrcMonitorStore::remove(const QString& networkId,
                             const QString& nick,
                             const IrcCaseMapping& mapping)
{
    if (networkId.isEmpty() || nick.isEmpty())
        return false;
    QStringList current = cached(networkId);
    bool changed = false;
    for (int i = current.size() - 1; i >= 0; --i) {
        if (nickEquals(mapping, current.at(i), nick)) {
            current.removeAt(i);
            changed = true;
        }
    }
    if (!changed)
        return false;
    m_cache.insert(networkId, current);
    save(networkId, current);
    return true;
}

void IrcMonitorStore::forget(const QString& networkId)
{
    if (networkId.isEmpty())
        return;
    m_cache.remove(networkId);
    save(networkId, {});
}
