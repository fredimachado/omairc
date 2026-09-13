#include "ircmute.h"

#include <QByteArray>
#include <QSettings>

#include <string>

namespace
{
const auto mutesGroup = QStringLiteral("mutes");
const auto targetsKey = QStringLiteral("targets");

std::string utf8(const QString& value)
{
    const QByteArray bytes = value.toUtf8();
    return std::string(bytes.constData(), std::size_t(bytes.size()));
}

bool targetEquals(const IrcCaseMapping& mapping, const QString& left, const QString& right)
{
    return mapping.equals(utf8(left), utf8(right));
}

int indexOfTarget(const QStringList& targets,
                  const QString& target,
                  const IrcCaseMapping& mapping)
{
    for (int i = 0; i < targets.size(); ++i) {
        if (targetEquals(mapping, targets.at(i), target))
            return i;
    }
    return -1;
}

bool listContains(const QStringList& targets,
                  const QString& target,
                  const IrcCaseMapping& mapping)
{
    return indexOfTarget(targets, target, mapping) >= 0;
}
}

QStringList IrcMuteStore::load(const QString& networkId) const
{
    if (networkId.isEmpty())
        return {};
    QSettings settings;
    settings.beginGroup(mutesGroup);
    settings.beginGroup(networkId);
    return settings.value(targetsKey).toStringList();
}

void IrcMuteStore::save(const QString& networkId, const QStringList& targets)
{
    if (networkId.isEmpty())
        return;
    QSettings settings;
    settings.beginGroup(mutesGroup);
    if (targets.isEmpty()) {
        settings.remove(networkId);
    } else {
        settings.beginGroup(networkId);
        settings.setValue(targetsKey, targets);
        settings.endGroup();
    }
    settings.endGroup();
    settings.sync();
}

const QStringList& IrcMuteStore::cached(const QString& networkId) const
{
    const auto found = m_cache.constFind(networkId);
    if (found != m_cache.cend())
        return found.value();
    return m_cache.insert(networkId, load(networkId)).value();
}

QStringList IrcMuteStore::targets(const QString& networkId) const
{
    if (networkId.isEmpty())
        return {};
    return cached(networkId);
}

QStringList IrcMuteStore::listed(const QString& networkId,
                                 const IrcCaseMapping& mapping) const
{
    QStringList unique;
    for (const QString& target : targets(networkId)) {
        if (!listContains(unique, target, mapping))
            unique.append(target);
    }
    return unique;
}

bool IrcMuteStore::contains(const QString& networkId,
                            const QString& target,
                            const IrcCaseMapping& mapping) const
{
    if (networkId.isEmpty() || target.isEmpty())
        return false;
    return listContains(cached(networkId), target, mapping);
}

bool IrcMuteStore::add(const QString& networkId,
                       const QString& target,
                       const IrcCaseMapping& mapping)
{
    if (networkId.isEmpty() || target.isEmpty())
        return false;
    QStringList current = cached(networkId);
    if (listContains(current, target, mapping))
        return false;
    current.append(target);
    m_cache.insert(networkId, current);
    save(networkId, current);
    return true;
}

bool IrcMuteStore::remove(const QString& networkId,
                          const QString& target,
                          const IrcCaseMapping& mapping)
{
    if (networkId.isEmpty() || target.isEmpty())
        return false;
    QStringList current = cached(networkId);
    bool changed = false;
    for (int i = current.size() - 1; i >= 0; --i) {
        if (targetEquals(mapping, current.at(i), target)) {
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

void IrcMuteStore::forget(const QString& networkId)
{
    if (networkId.isEmpty())
        return;
    m_cache.remove(networkId);
    save(networkId, {});
}
