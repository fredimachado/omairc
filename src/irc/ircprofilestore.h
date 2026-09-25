#pragma once

#include "ircnetworkprofile.h"

#include <QList>

class IrcProfileStore
{
public:
    enum class Status {
        Written,
        Absent,
        AccessError,
        FormatError,
    };

    IrcProfileStore();
    QList<IrcNetworkProfile> profiles() const;
    Status save(const IrcNetworkProfile &profile);
    Status remove(const QString &networkId);
};
