#pragma once

#include "ircnetworkprofile.h"

#include <QList>

class IrcProfileStore
{
public:
    IrcProfileStore();
    QList<IrcNetworkProfile> profiles() const;
    void save(const IrcNetworkProfile &profile);
    bool remove(const QString &networkId);
};
