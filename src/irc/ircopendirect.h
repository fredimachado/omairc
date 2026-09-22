#pragma once

#include "irccasemapping.h"

#include <QHash>
#include <QString>
#include <QStringList>

class IrcOpenDirectStore
{
public:
    QStringList targets(const QString& networkId) const;
    QStringList listed(const QString& networkId, const IrcCaseMapping& mapping) const;
    bool add(const QString& networkId,
             const QString& target,
             const IrcCaseMapping& mapping);
    bool remove(const QString& networkId,
                const QString& target,
                const IrcCaseMapping& mapping);
    bool rekey(const QString& networkId,
               const QString& oldTarget,
               const QString& newTarget,
               const IrcCaseMapping& mapping);
    void forget(const QString& networkId);
    void setEphemeral(bool ephemeral);

private:
    QStringList load(const QString& networkId) const;
    void save(const QString& networkId, const QStringList& targets);
    const QStringList& cached(const QString& networkId) const;

    bool m_ephemeral = false;
    mutable QHash<QString, QStringList> m_cache;
};
