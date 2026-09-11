#include "ircservicenick.h"

#include <QString>

namespace
{
bool canStartNick(QChar mark)
{
    if ((mark >= QLatin1Char('A') && mark <= QLatin1Char('Z'))
        || (mark >= QLatin1Char('a') && mark <= QLatin1Char('z'))) {
        return true;
    }
    return mark == QLatin1Char('-') || mark == QLatin1Char('[')
        || mark == QLatin1Char(']') || mark == QLatin1Char('\\')
        || mark == QLatin1Char('`') || mark == QLatin1Char('_')
        || mark == QLatin1Char('^') || mark == QLatin1Char('{')
        || mark == QLatin1Char('|') || mark == QLatin1Char('}');
}

bool looksLikeChannel(QStringView target, QStringView channelTypes)
{
    if (target.isEmpty())
        return false;
    const QChar mark = target.front();
    if (!channelTypes.isEmpty())
        return channelTypes.contains(mark);
    return !canStartNick(mark);
}
}

bool ircIsServiceIdentity(QStringView nick, QStringView host, QStringView channelTypes)
{
    if (!nick.isEmpty() && !looksLikeChannel(nick, channelTypes)
        && nick.endsWith(QLatin1String("serv"), Qt::CaseInsensitive)) {
        return true;
    }
    if (host.isEmpty())
        return false;
    const QString folded = host.toString().toCaseFolded();
    return folded == QLatin1String("services")
        || folded.startsWith(QLatin1String("services."));
}

bool ircNickIsRoutable(QStringView nick) noexcept
{
    if (nick.isEmpty())
        return false;
    for (const QChar mark : nick) {
        if (mark.unicode() < 0x20 || QStringView(u" ,*?!@.$:").contains(mark))
            return false;
    }
    return true;
}
