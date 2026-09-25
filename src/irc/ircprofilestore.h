#pragma once

#include "ircnetworkprofile.h"

#include <QList>

class QSettings;

class IrcProfileStore
{
public:
    enum class Status {
        Written,
        Absent,
        AccessError,
        FormatError,
    };

    // File bytes, not QSettings::status() on another object. Ok when the
    // path is not an ini, missing, or readable and well-formed.
    enum class IniProbe {
        Ok,
        Malformed,
        Unreadable,
    };

    IrcProfileStore();
    QList<IrcNetworkProfile> profiles() const;
    Status save(const IrcNetworkProfile &profile);
    Status remove(const QString &networkId);
    static IniProbe probeSettingsIni(const QSettings &settings);
};
