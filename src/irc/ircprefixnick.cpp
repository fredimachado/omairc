#include "ircprefixnick.h"

#include "ircwiretext.h"

QString ircPrefixNick(const IrcMessage& message)
{
    if (!message.prefix)
        return {};
    if (!message.prefix->nick.empty())
        return ircWireText(message.prefix->nick);
    const QString raw = ircWireText(message.prefix->raw);
    if (raw.contains(QLatin1Char('.')))
        return {};
    return raw;
}
