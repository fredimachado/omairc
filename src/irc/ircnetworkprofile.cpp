#include "ircnetworkprofile.h"

#include "irccommandbuilder.h"

#include <QRegularExpression>
#include <QUuid>

namespace
{
std::string utf8(const QString &value)
{
    const QByteArray bytes = value.toUtf8();
    return std::string(bytes.constData(), std::size_t(bytes.size()));
}

QString prefixChannel(QString channel)
{
    channel = channel.trimmed();
    if (channel.isEmpty())
        return {};
    const QChar mark = channel.front();
    if (mark == QLatin1Char('#') || mark == QLatin1Char('&')
        || mark == QLatin1Char('+') || mark == QLatin1Char('!')) {
        return channel;
    }
    return QLatin1Char('#') + channel;
}

QStringList splitAutojoin(const QStringList &channels)
{
    QStringList result;
    const QRegularExpression separators(QStringLiteral("[,\\s]+"));
    for (const QString &entry : channels) {
        const QStringList parts = entry.split(separators, Qt::SkipEmptyParts);
        for (const QString &part : parts) {
            const QString channel = prefixChannel(part);
            if (!channel.isEmpty())
                result.append(channel);
        }
    }
    return result;
}
}

QStringList IrcNetworkProfile::parseAutojoin(const QString &channels)
{
    QStringList result;
    const QRegularExpression separators(QStringLiteral("[,\\s]+"));
    const QStringList parts = channels.split(separators, Qt::SkipEmptyParts);
    for (const QString &part : parts)
        result.append(part.trimmed());
    return result;
}

IrcNetworkProfile IrcNetworkProfile::create()
{
    IrcNetworkProfile profile;
    profile.networkId = QUuid::createUuid().toString(QUuid::WithoutBraces);
    return profile;
}

IrcNetworkProfile IrcNetworkProfile::suggested()
{
    IrcNetworkProfile profile = create();
    profile.host = QStringLiteral("irc.libera.chat");
    profile.port = 6697;
    profile.tlsEnabled = true;
    profile.autojoinChannels = {QStringLiteral("#omarchy")};
    return profile;
}

IrcNetworkProfile IrcNetworkProfile::normalized() const
{
    IrcNetworkProfile profile = *this;
    profile.host = host.trimmed();
    profile.nick = nick.trimmed();
    profile.username = username.trimmed();
    profile.realname = realname.trimmed();
    profile.autojoinChannels = splitAutojoin(autojoinChannels);
    return profile;
}

IrcNetworkProfile::Problem IrcNetworkProfile::validate() const
{
    const IrcNetworkProfile profile = normalized();
    if (profile.host.isEmpty())
        return Problem::MissingHost;
    if (profile.port == 0)
        return Problem::InvalidPort;
    if (profile.nick.isEmpty())
        return Problem::MissingNick;
    if (!IrcCommandBuilder::nick(utf8(profile.nick)))
        return Problem::UnsendableIdentity;
    if (!profile.username.isEmpty() || !profile.realname.isEmpty()) {
        const QString userField = profile.username.isEmpty() ? profile.nick
                                                             : profile.username;
        const QString realField = profile.realname.isEmpty() ? profile.nick
                                                             : profile.realname;
        if (!IrcCommandBuilder::user(utf8(userField), utf8(realField)))
            return Problem::UnsendableIdentity;
    }
    for (const QString &channel : profile.autojoinChannels) {
        if (!IrcCommandBuilder::join(utf8(channel)))
            return Problem::UnsendableChannel;
    }
    return Problem::None;
}

bool IrcNetworkProfile::isComplete() const
{
    return validate() == Problem::None;
}

QString IrcNetworkProfile::problemText(Problem problem)
{
    switch (problem) {
    case Problem::None:
        return {};
    case Problem::MissingHost:
        return QStringLiteral("Host is required");
    case Problem::MissingNick:
        return QStringLiteral("Nick is required");
    case Problem::InvalidPort:
        return QStringLiteral("Port is required");
    case Problem::UnsendableIdentity:
        return QStringLiteral("Nick, username, or real name cannot be sent");
    case Problem::UnsendableChannel:
        return QStringLiteral("An autojoin channel cannot be sent");
    }
    return {};
}

bool operator==(const IrcNetworkProfile &left, const IrcNetworkProfile &right)
{
    return left.networkId == right.networkId
        && left.host == right.host
        && left.port == right.port
        && left.tlsEnabled == right.tlsEnabled
        && left.connectOnStartup == right.connectOnStartup
        && left.secretSaved == right.secretSaved
        && left.nick == right.nick
        && left.username == right.username
        && left.realname == right.realname
        && left.autojoinChannels == right.autojoinChannels;
}

bool operator!=(const IrcNetworkProfile &left, const IrcNetworkProfile &right)
{
    return !(left == right);
}
