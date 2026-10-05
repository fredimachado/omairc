#include "ircopendirect.h"

#include <QByteArray>
#include <QSettings>

#include <string>

namespace
{
const auto openDirectsGroup = QStringLiteral("openDirects");
const auto dismissedDirectsGroup = QStringLiteral("dismissedDirects");
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

void IrcOpenDirectStore::setEphemeral(bool ephemeral)
{
    m_ephemeral = ephemeral;
    if (ephemeral) {
        m_cache.clear();
        m_dismissed.clear();
    }
}

QStringList IrcOpenDirectStore::load(const QString& group,
                                     const QString& networkId,
                                     const QHash<QString, QStringList>& cache) const
{
    if (networkId.isEmpty())
        return {};
    if (m_ephemeral)
        return cache.value(networkId);
    QSettings settings;
    settings.beginGroup(group);
    settings.beginGroup(networkId);
    return settings.value(targetsKey).toStringList();
}

void IrcOpenDirectStore::save(const QString& group,
                              const QString& networkId,
                              const QStringList& targets,
                              QHash<QString, QStringList>& cache)
{
    if (networkId.isEmpty())
        return;
    if (m_ephemeral) {
        if (targets.isEmpty())
            cache.remove(networkId);
        else
            cache.insert(networkId, targets);
        return;
    }
    QSettings settings;
    settings.beginGroup(group);
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

const QStringList& IrcOpenDirectStore::cached(const QString& group,
                                              QHash<QString, QStringList>& cache,
                                              const QString& networkId) const
{
    const auto found = cache.constFind(networkId);
    if (found != cache.cend())
        return found.value();
    return cache.insert(networkId, load(group, networkId, cache)).value();
}

QStringList IrcOpenDirectStore::targets(const QString& networkId) const
{
    if (networkId.isEmpty())
        return {};
    return cached(openDirectsGroup, m_cache, networkId);
}

QStringList IrcOpenDirectStore::listed(const QString& networkId,
                                       const IrcCaseMapping& mapping) const
{
    QStringList unique;
    for (const QString& target : targets(networkId)) {
        if (!listContains(unique, target, mapping))
            unique.append(target);
    }
    return unique;
}

bool IrcOpenDirectStore::addTo(const QString& group,
                               QHash<QString, QStringList>& cache,
                               const QString& networkId,
                               const QString& target,
                               const IrcCaseMapping& mapping)
{
    if (networkId.isEmpty() || target.isEmpty())
        return false;
    QStringList current = cached(group, cache, networkId);
    if (listContains(current, target, mapping))
        return false;
    current.append(target);
    cache.insert(networkId, current);
    save(group, networkId, current, cache);
    return true;
}

bool IrcOpenDirectStore::removeFrom(const QString& group,
                                    QHash<QString, QStringList>& cache,
                                    const QString& networkId,
                                    const QString& target,
                                    const IrcCaseMapping& mapping)
{
    if (networkId.isEmpty() || target.isEmpty())
        return false;
    QStringList current = cached(group, cache, networkId);
    bool changed = false;
    for (int i = current.size() - 1; i >= 0; --i) {
        if (targetEquals(mapping, current.at(i), target)) {
            current.removeAt(i);
            changed = true;
        }
    }
    if (!changed)
        return false;
    cache.insert(networkId, current);
    save(group, networkId, current, cache);
    return true;
}

bool IrcOpenDirectStore::rekeyIn(const QString& group,
                                 QHash<QString, QStringList>& cache,
                                 const QString& networkId,
                                 const QString& oldTarget,
                                 const QString& newTarget,
                                 const IrcCaseMapping& mapping)
{
    if (networkId.isEmpty() || oldTarget.isEmpty() || newTarget.isEmpty())
        return false;
    QStringList current = cached(group, cache, networkId);
    const int oldIndex = indexOfTarget(current, oldTarget, mapping);
    if (oldIndex < 0)
        return false;
    if (targetEquals(mapping, current.at(oldIndex), newTarget)) {
        if (current.at(oldIndex) == newTarget)
            return false;
        current[oldIndex] = newTarget;
    } else {
        current.removeAt(oldIndex);
        if (!listContains(current, newTarget, mapping))
            current.append(newTarget);
    }
    cache.insert(networkId, current);
    save(group, networkId, current, cache);
    return true;
}

bool IrcOpenDirectStore::add(const QString& networkId,
                             const QString& target,
                             const IrcCaseMapping& mapping)
{
    return addTo(openDirectsGroup, m_cache, networkId, target, mapping);
}

bool IrcOpenDirectStore::remove(const QString& networkId,
                                const QString& target,
                                const IrcCaseMapping& mapping)
{
    return removeFrom(openDirectsGroup, m_cache, networkId, target, mapping);
}

bool IrcOpenDirectStore::rekey(const QString& networkId,
                               const QString& oldTarget,
                               const QString& newTarget,
                               const IrcCaseMapping& mapping)
{
    return rekeyIn(openDirectsGroup, m_cache, networkId, oldTarget, newTarget, mapping);
}

bool IrcOpenDirectStore::isDismissed(const QString& networkId,
                                     const QString& target,
                                     const IrcCaseMapping& mapping) const
{
    if (networkId.isEmpty() || target.isEmpty())
        return false;
    return listContains(cached(dismissedDirectsGroup, m_dismissed, networkId),
                        target, mapping);
}

QStringList IrcOpenDirectStore::dismissedListed(const QString& networkId,
                                                const IrcCaseMapping& mapping) const
{
    QStringList unique;
    if (networkId.isEmpty())
        return unique;
    for (const QString& target : cached(dismissedDirectsGroup, m_dismissed, networkId)) {
        if (!listContains(unique, target, mapping))
            unique.append(target);
    }
    return unique;
}

bool IrcOpenDirectStore::dismiss(const QString& networkId,
                                 const QString& target,
                                 const IrcCaseMapping& mapping)
{
    return addTo(dismissedDirectsGroup, m_dismissed, networkId, target, mapping);
}

bool IrcOpenDirectStore::undismiss(const QString& networkId,
                                   const QString& target,
                                   const IrcCaseMapping& mapping)
{
    return removeFrom(dismissedDirectsGroup, m_dismissed, networkId, target, mapping);
}

bool IrcOpenDirectStore::rekeyDismissed(const QString& networkId,
                                        const QString& oldTarget,
                                        const QString& newTarget,
                                        const IrcCaseMapping& mapping)
{
    return rekeyIn(dismissedDirectsGroup, m_dismissed, networkId,
                   oldTarget, newTarget, mapping);
}

void IrcOpenDirectStore::forget(const QString& networkId)
{
    if (networkId.isEmpty())
        return;
    m_cache.remove(networkId);
    m_dismissed.remove(networkId);
    save(openDirectsGroup, networkId, {}, m_cache);
    save(dismissedDirectsGroup, networkId, {}, m_dismissed);
}
