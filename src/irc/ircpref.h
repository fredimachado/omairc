#pragma once

#include "ircmembershipnoise.h"

#include <QString>
#include <QVector>

enum class IrcPrefName { Directs, Avatars, Unread, Joins };

enum class IrcPrefKind { QueryAll, QueryOne, Set, Usage };

enum class IrcPrefValueKind { Toggle, Noise };

struct IrcPrefSpec
{
    IrcPrefName name = IrcPrefName::Directs;
    QString token;
    QString label;
    IrcPrefValueKind valueKind = IrcPrefValueKind::Toggle;
};

struct IrcPrefRequest
{
    IrcPrefKind kind = IrcPrefKind::Usage;
    IrcPrefName name = IrcPrefName::Directs;
    bool enabled = false;
    IrcMembershipNoise noise = IrcMembershipNoise::Folded;
};

const QVector<IrcPrefSpec>& ircPrefCatalog();
const IrcPrefSpec *ircPrefFind(const QString& token);
QString ircPrefUsage();
QString ircPrefAvatarNote();
QString ircFormatPrefState(IrcPrefName name, bool enabled);
QString ircFormatPrefQuery(IrcPrefName name, bool enabled);
QString ircFormatPrefNoise(IrcMembershipNoise noise);
QString ircFormatPrefList(bool directs,
                          bool avatars,
                          bool unread,
                          IrcMembershipNoise noise);
IrcPrefRequest ircParsePrefArgument(const QString& argument);
