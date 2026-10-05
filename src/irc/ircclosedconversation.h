#pragma once

#include "irccasemapping.h"

#include <QHash>
#include <QString>
#include <QStringList>

// Conversations the user closed. Catch-up must not create them again. A live
// query line, or an explicit open, removes the entry when the row is inserted.
// A closed channel stays closed until the user joins it.
// Group "closedConversations/<networkId>", key "targets", shared with the
// terminal client.
class IrcClosedConversationStore
{
public:
    bool contains(const QString& networkId,
                  const QString& target,
                  const IrcCaseMapping& mapping) const;
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
