#include "ircchannelmode.h"

#include <QByteArray>
#include <QLatin1Char>

#include <string>

namespace
{
std::string utf8(const QString& value)
{
    const QByteArray bytes = value.toUtf8();
    return std::string(bytes.constData(), std::size_t(bytes.size()));
}

bool containsForbidden(const QString& token)
{
    return token.contains(QLatin1Char('\r'))
        || token.contains(QLatin1Char('\n'))
        || token.contains(QChar('\0'));
}
}

std::optional<IrcChannelModeRequest> IrcChannelModeRequest::parse(
    const QString& argument,
    const IrcServerFeatures& features)
{
    const QStringList tokens = argument.split(QLatin1Char(' '), Qt::SkipEmptyParts);
    if (tokens.isEmpty())
        return std::nullopt;

    for (const QString& token : tokens) {
        if (containsForbidden(token))
            return std::nullopt;
    }

    const QString& channel = tokens.front();
    if (!features.isChannel(utf8(channel)))
        return std::nullopt;

    if (tokens.size() == 1)
        return IrcChannelModeRequest(Query{channel});

    return IrcChannelModeRequest(
        Change{channel, tokens.at(1), tokens.mid(2)});
}

const QString& IrcChannelModeRequest::channel() const
{
    return visit([](const auto& payload) -> const QString& {
        return payload.channel;
    });
}
