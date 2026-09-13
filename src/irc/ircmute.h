#pragma once

#include "irccasemapping.h"

#include <QHash>
#include <QString>
#include <QStringList>

class IrcMuteStore
{
public:
    QStringList targets(const QString& networkId) const;
    QStringList listed(const QString& networkId, const IrcCaseMapping& mapping) const;
    bool contains(const QString& networkId,
                  const QString& target,
                  const IrcCaseMapping& mapping) const;
    bool add(const QString& networkId,
             const QString& target,
             const IrcCaseMapping& mapping);
    bool remove(const QString& networkId,
                const QString& target,
                const IrcCaseMapping& mapping);
    void forget(const QString& networkId);

private:
    QStringList load(const QString& networkId) const;
    void save(const QString& networkId, const QStringList& targets);
    const QStringList& cached(const QString& networkId) const;

    mutable QHash<QString, QStringList> m_cache;
};
