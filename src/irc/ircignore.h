#pragma once

#include "irccasemapping.h"
#include "ircmessage.h"
#include "ircserverfeatures.h"

#include <QHash>
#include <QString>
#include <QStringList>

class IrcIgnoreStore
{
public:
    QStringList nicks(const QString& networkId) const;
    QStringList listed(const QString& networkId, const IrcCaseMapping& mapping) const;
    bool contains(const QString& networkId,
                  const QString& nick,
                  const IrcCaseMapping& mapping) const;
    bool add(const QString& networkId,
             const QString& nick,
             const IrcCaseMapping& mapping);
    bool remove(const QString& networkId,
                const QString& nick,
                const IrcCaseMapping& mapping);
    void forget(const QString& networkId);

private:
    QStringList load(const QString& networkId) const;
    void save(const QString& networkId, const QStringList& nicks);
    const QStringList& cached(const QString& networkId) const;

    mutable QHash<QString, QStringList> m_cache;
};

bool ircIgnoreDropsInbound(const IrcMessage& message,
                           const QString& selfNick,
                           const QStringList& nicks,
                           const IrcServerFeatures& features);
