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
    // User close is the only writer. A stamp or a transcript is not a dismiss.
    bool isDismissed(const QString& networkId,
                     const QString& target,
                     const IrcCaseMapping& mapping) const;
    QStringList dismissedListed(const QString& networkId,
                                const IrcCaseMapping& mapping) const;
    bool dismiss(const QString& networkId,
                 const QString& target,
                 const IrcCaseMapping& mapping);
    bool undismiss(const QString& networkId,
                   const QString& target,
                   const IrcCaseMapping& mapping);
    bool rekeyDismissed(const QString& networkId,
                        const QString& oldTarget,
                        const QString& newTarget,
                        const IrcCaseMapping& mapping);
    void forget(const QString& networkId);
    void setEphemeral(bool ephemeral);

private:
    QStringList load(const QString& group,
                     const QString& networkId,
                     const QHash<QString, QStringList>& cache) const;
    void save(const QString& group,
              const QString& networkId,
              const QStringList& targets,
              QHash<QString, QStringList>& cache);
    const QStringList& cached(const QString& group,
                              QHash<QString, QStringList>& cache,
                              const QString& networkId) const;
    bool addTo(const QString& group,
               QHash<QString, QStringList>& cache,
               const QString& networkId,
               const QString& target,
               const IrcCaseMapping& mapping);
    bool removeFrom(const QString& group,
                    QHash<QString, QStringList>& cache,
                    const QString& networkId,
                    const QString& target,
                    const IrcCaseMapping& mapping);
    bool rekeyIn(const QString& group,
                 QHash<QString, QStringList>& cache,
                 const QString& networkId,
                 const QString& oldTarget,
                 const QString& newTarget,
                 const IrcCaseMapping& mapping);

    bool m_ephemeral = false;
    mutable QHash<QString, QStringList> m_cache;
    mutable QHash<QString, QStringList> m_dismissed;
};
