#include "ircnetworkprofile.h"

#include "irccasemapping.h"
#include "irccommandbuilder.h"
#include "ircserverfeatures.h"

#include <QByteArray>
#include <QRandomGenerator>
#include <QRegularExpression>

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
    const IrcServerFeatures features;
    if (features.isChannel(utf8(channel)))
        return channel;
    return QLatin1Char('#') + channel;
}

bool tokenIsChannel(const QString &token)
{
    if (token.isEmpty())
        return false;
    return IrcServerFeatures().isChannel(utf8(token));
}

bool keepAutojoinToken(const QString &token, bool previousWasChannel)
{
    if (token.isEmpty())
        return false;
    if (tokenIsChannel(token))
        return true;
    return !previousWasChannel;
}

QMap<QString, QString> retainAutojoinKeys(const QMap<QString, QString> &keys,
                                          const QStringList &channels)
{
    QMap<QString, QString> retained;
    const IrcCaseMapping mapping;
    for (const QString &channel : channels) {
        for (auto it = keys.constBegin(); it != keys.constEnd(); ++it) {
            if (mapping.equals(utf8(it.key()), utf8(channel))) {
                retained.insert(channel, it.value());
                break;
            }
        }
    }
    return retained;
}

bool fitsOneLoginField(const QString &value)
{
    for (const QChar mark : value) {
        if (mark.isSpace() || mark == QChar(u'\0') || mark == QLatin1Char('/'))
            return false;
    }
    return true;
}

QStringList splitAutojoin(const QStringList &channels)
{
    QStringList result;
    const QRegularExpression separators(QStringLiteral("[,\\s]+"));
    bool previousWasChannel = false;
    for (const QString &entry : channels) {
        const QStringList parts = entry.split(separators, Qt::SkipEmptyParts);
        for (const QString &part : parts) {
            const QString token = part.trimmed();
            if (!keepAutojoinToken(token, previousWasChannel))
                continue;
            const QString channel = prefixChannel(token);
            if (channel.isEmpty())
                continue;
            result.append(channel);
            previousWasChannel = tokenIsChannel(token);
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
    bool previousWasChannel = false;
    for (const QString &part : parts) {
        const QString token = part.trimmed();
        if (!keepAutojoinToken(token, previousWasChannel))
            continue;
        result.append(token);
        previousWasChannel = tokenIsChannel(token);
    }
    return result;
}

IrcNetworkProfile IrcNetworkProfile::create()
{
    IrcNetworkProfile profile;
    const quint64 bits = QRandomGenerator::system()->generate64();
    const QByteArray bytes(reinterpret_cast<const char *>(&bits), 8);
    profile.networkId = QString::fromLatin1(
        bytes.toBase64(QByteArray::Base64UrlEncoding | QByteArray::OmitTrailingEquals));
    return profile;
}

IrcNetworkProfile IrcNetworkProfile::suggested()
{
    IrcNetworkProfile profile = create();
    profile.host = QStringLiteral("irc.libera.chat");
    profile.name = profile.host;
    profile.port = 6697;
    profile.tlsEnabled = true;
    profile.autojoinChannels = {QStringLiteral("#omarchy")};
    return profile;
}

int IrcNetworkProfile::pickIconColor(const QList<int> &used)
{
    QList<int> free;
    for (int slot = 0; slot < iconColorCount; ++slot) {
        if (!used.contains(slot))
            free.append(slot);
    }
    if (free.isEmpty())
        return int(QRandomGenerator::global()->bounded(iconColorCount));
    if (free.size() == 1)
        return free.first();
    return free.at(int(QRandomGenerator::global()->bounded(free.size())));
}

bool IrcNetworkProfile::ensureIconColor(const QList<int> &used)
{
    if (iconColor >= 0 && iconColor < iconColorCount)
        return false;
    iconColor = pickIconColor(used);
    return true;
}

IrcNetworkProfile IrcNetworkProfile::normalized() const
{
    IrcNetworkProfile profile = *this;
    profile.name = name.trimmed();
    profile.host = host.trimmed();
    profile.nick = nick.trimmed();
    profile.username = username.trimmed();
    profile.realname = realname.trimmed();
    profile.account = account.trimmed();
    profile.bouncerNetwork = bouncerNetwork.trimmed();
    profile.autojoinChannels = splitAutojoin(autojoinChannels);
    profile.autojoinKeys = retainAutojoinKeys(autojoinKeys, profile.autojoinChannels);
    return profile;
}

QString IrcNetworkProfile::resolvedName() const
{
    const QString trimmedName = name.trimmed();
    if (!trimmedName.isEmpty())
        return trimmedName;
    return host.trimmed();
}

QString IrcNetworkProfile::saslAccount() const
{
    const IrcNetworkProfile profile = normalized();
    const QString name = profile.account.isEmpty() ? profile.nick : profile.account;
    if (profile.bouncerNetwork.isEmpty())
        return name;
    return name + QLatin1Char('/') + profile.bouncerNetwork;
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
    if (!fitsOneLoginField(profile.account))
        return Problem::UnsendableAccount;
    if (!fitsOneLoginField(profile.bouncerNetwork))
        return Problem::UnsendableBouncerNetwork;
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
    case Problem::UnsendableAccount:
        return QStringLiteral("Account cannot contain a space or a slash");
    case Problem::UnsendableBouncerNetwork:
        return QStringLiteral("Bouncer network cannot contain a space or a slash");
    case Problem::UnsendableChannel:
        return QStringLiteral("An autojoin channel cannot be sent");
    }
    return {};
}

bool operator==(const IrcNetworkProfile &left, const IrcNetworkProfile &right)
{
    return left.networkId == right.networkId
        && left.name == right.name
        && left.host == right.host
        && left.port == right.port
        && left.tlsEnabled == right.tlsEnabled
        && left.connectOnStartup == right.connectOnStartup
        && left.secretSaved == right.secretSaved
        && left.nickServSaved == right.nickServSaved
        && left.nick == right.nick
        && left.username == right.username
        && left.realname == right.realname
        && left.account == right.account
        && left.bouncerNetwork == right.bouncerNetwork
        && left.autojoinChannels == right.autojoinChannels
        && left.autojoinKeys == right.autojoinKeys
        && left.iconColor == right.iconColor;
}

bool operator!=(const IrcNetworkProfile &left, const IrcNetworkProfile &right)
{
    return !(left == right);
}
