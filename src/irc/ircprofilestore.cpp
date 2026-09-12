#include "ircprofilestore.h"

#include <QSettings>

namespace
{
const auto networksGroup = QStringLiteral("networks");
const auto hostKey = QStringLiteral("host");
const auto portKey = QStringLiteral("port");
const auto tlsKey = QStringLiteral("tls");
const auto connectOnStartupKey = QStringLiteral("connectOnStartup");
const auto secretSavedKey = QStringLiteral("secretSaved");
const auto nickServSavedKey = QStringLiteral("nickServSaved");
const auto nickKey = QStringLiteral("nick");
const auto usernameKey = QStringLiteral("username");
const auto realnameKey = QStringLiteral("realname");
const auto accountKey = QStringLiteral("account");
const auto bouncerNetworkKey = QStringLiteral("bouncerNetwork");
const auto autojoinKey = QStringLiteral("autojoin");
const auto iconColorKey = QStringLiteral("iconColor");
}

IrcProfileStore::IrcProfileStore() = default;

QList<IrcNetworkProfile> IrcProfileStore::profiles() const
{
    QSettings settings;
    QList<IrcNetworkProfile> result;
    settings.beginGroup(networksGroup);
    const QStringList ids = settings.childGroups();
    for (const QString &id : ids) {
        settings.beginGroup(id);
        IrcNetworkProfile profile;
        profile.networkId = id;
        profile.host = settings.value(hostKey).toString();
        profile.port = quint16(settings.value(portKey, 6697).toUInt());
        profile.tlsEnabled = settings.value(tlsKey, true).toBool();
        profile.connectOnStartup = settings.value(connectOnStartupKey, false).toBool();
        profile.secretSaved = settings.value(secretSavedKey, false).toBool();
        profile.nickServSaved = settings.value(nickServSavedKey, false).toBool();
        profile.nick = settings.value(nickKey).toString();
        profile.username = settings.value(usernameKey).toString();
        profile.realname = settings.value(realnameKey).toString();
        profile.account = settings.value(accountKey).toString();
        profile.bouncerNetwork = settings.value(bouncerNetworkKey).toString();
        profile.autojoinChannels = settings.value(autojoinKey).toStringList();
        bool iconOk = false;
        const int iconColor = settings.value(iconColorKey).toInt(&iconOk);
        profile.iconColor = (iconOk
                             && iconColor >= 0
                             && iconColor < IrcNetworkProfile::iconColorCount)
            ? iconColor : IrcNetworkProfile::noIconColor;
        settings.endGroup();
        result.append(profile);
    }
    settings.endGroup();
    return result;
}

void IrcProfileStore::save(const IrcNetworkProfile &profile)
{
    if (profile.networkId.isEmpty())
        return;

    QSettings settings;
    settings.beginGroup(networksGroup);
    settings.beginGroup(profile.networkId);
    settings.remove(QString());
    settings.setValue(hostKey, profile.host);
    settings.setValue(portKey, profile.port);
    settings.setValue(tlsKey, profile.tlsEnabled);
    settings.setValue(connectOnStartupKey, profile.connectOnStartup);
    settings.setValue(secretSavedKey, profile.secretSaved);
    settings.setValue(nickServSavedKey, profile.nickServSaved);
    settings.setValue(nickKey, profile.nick);
    settings.setValue(usernameKey, profile.username);
    settings.setValue(realnameKey, profile.realname);
    settings.setValue(accountKey, profile.account);
    settings.setValue(bouncerNetworkKey, profile.bouncerNetwork);
    settings.setValue(autojoinKey, profile.autojoinChannels);
    if (profile.iconColor >= 0 && profile.iconColor < IrcNetworkProfile::iconColorCount)
        settings.setValue(iconColorKey, profile.iconColor);
    settings.endGroup();
    settings.endGroup();
    settings.sync();
}

bool IrcProfileStore::remove(const QString &networkId)
{
    if (networkId.isEmpty())
        return false;

    QSettings settings;
    settings.beginGroup(networksGroup);
    if (!settings.childGroups().contains(networkId)) {
        settings.endGroup();
        return false;
    }
    settings.remove(networkId);
    settings.endGroup();
    settings.sync();
    return true;
}
