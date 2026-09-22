#include "ircpref.h"

#include <QStringList>

const QVector<IrcPrefSpec>& ircPrefCatalog()
{
    static const QVector<IrcPrefSpec> rows = {
        {IrcPrefName::Directs, QStringLiteral("directs"),
         QStringLiteral("Reopen direct messages on startup")},
        {IrcPrefName::Avatars, QStringLiteral("avatars"),
         QStringLiteral("Show peer avatars")},
        {IrcPrefName::Unread, QStringLiteral("unread"),
         QStringLiteral("Open conversations at unread")},
    };
    return rows;
}

const IrcPrefSpec *ircPrefFind(const QString& token)
{
    const QString folded = token.toLower();
    for (const IrcPrefSpec& spec : ircPrefCatalog()) {
        if (spec.token == folded)
            return &spec;
    }
    return nullptr;
}

QString ircPrefUsage()
{
    return QStringLiteral("/pref [directs|avatars|unread] [on|off]");
}

QString ircPrefAvatarNote()
{
    return QStringLiteral(
        "Turn off to keep avatar hosts from seeing your IP on busy channels.");
}

QString ircFormatPrefState(IrcPrefName name, bool enabled)
{
    const IrcPrefSpec *spec = nullptr;
    for (const IrcPrefSpec& row : ircPrefCatalog()) {
        if (row.name == name) {
            spec = &row;
            break;
        }
    }
    const QString label = spec ? spec->label : QString();
    return QStringLiteral("%1: %2")
        .arg(label, enabled ? QStringLiteral("on") : QStringLiteral("off"));
}

QString ircFormatPrefQuery(IrcPrefName name, bool enabled)
{
    QString text = ircFormatPrefState(name, enabled);
    if (name == IrcPrefName::Avatars)
        text += QLatin1Char('\n') + ircPrefAvatarNote();
    return text;
}

QString ircFormatPrefList(bool directs, bool avatars, bool unread)
{
    return ircFormatPrefState(IrcPrefName::Directs, directs) + QLatin1Char('\n')
        + ircFormatPrefState(IrcPrefName::Avatars, avatars) + QLatin1Char('\n')
        + ircFormatPrefState(IrcPrefName::Unread, unread);
}

IrcPrefRequest ircParsePrefArgument(const QString& argument)
{
    IrcPrefRequest request;
    const QStringList parts =
        argument.trimmed().split(QLatin1Char(' '), Qt::SkipEmptyParts);
    if (parts.isEmpty()) {
        request.kind = IrcPrefKind::QueryAll;
        return request;
    }
    if (parts.size() > 2)
        return request;

    const IrcPrefSpec *spec = ircPrefFind(parts.at(0));
    if (!spec)
        return request;
    request.name = spec->name;
    if (parts.size() == 1) {
        request.kind = IrcPrefKind::QueryOne;
        return request;
    }

    const QString value = parts.at(1).toLower();
    if (value == QLatin1String("on")) {
        request.kind = IrcPrefKind::Set;
        request.enabled = true;
        return request;
    }
    if (value == QLatin1String("off")) {
        request.kind = IrcPrefKind::Set;
        request.enabled = false;
        return request;
    }
    return request;
}
