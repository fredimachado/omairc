#include "ircservicenick.h"

#include <QString>

namespace
{
bool looksLikeChannel(QStringView target)
{
    if (target.isEmpty())
        return false;
    const QChar mark = target.front();
    return mark == QLatin1Char('#') || mark == QLatin1Char('&')
        || mark == QLatin1Char('+') || mark == QLatin1Char('!')
        || mark == QLatin1Char('~');
}
}

bool ircIsServiceIdentity(QStringView nick, QStringView host)
{
    if (!nick.isEmpty() && !looksLikeChannel(nick)
        && nick.endsWith(QLatin1String("serv"), Qt::CaseInsensitive)) {
        return true;
    }
    if (host.isEmpty())
        return false;
    const QString folded = host.toString().toCaseFolded();
    return folded == QLatin1String("services")
        || folded.startsWith(QLatin1String("services."));
}
