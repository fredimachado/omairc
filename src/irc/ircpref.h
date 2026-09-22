#pragma once

#include <QString>
#include <QVector>

enum class IrcPrefName { Directs, Avatars, Unread };

enum class IrcPrefKind { QueryAll, QueryOne, Set, Usage };

struct IrcPrefSpec
{
    IrcPrefName name = IrcPrefName::Directs;
    QString token;
    QString label;
};

struct IrcPrefRequest
{
    IrcPrefKind kind = IrcPrefKind::Usage;
    IrcPrefName name = IrcPrefName::Directs;
    bool enabled = false;
};

const QVector<IrcPrefSpec>& ircPrefCatalog();
const IrcPrefSpec *ircPrefFind(const QString& token);
QString ircPrefUsage();
QString ircPrefAvatarNote();
QString ircFormatPrefState(IrcPrefName name, bool enabled);
QString ircFormatPrefQuery(IrcPrefName name, bool enabled);
QString ircFormatPrefList(bool directs, bool avatars, bool unread);
IrcPrefRequest ircParsePrefArgument(const QString& argument);
