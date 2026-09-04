#include "ircprofilestore.h"

#include <QSettings>

namespace
{
const auto networksGroup = QStringLiteral("networks");
const auto hostKey = QStringLiteral("host");
const auto portKey = QStringLiteral("port");
const auto tlsKey = QStringLiteral("tls");
const auto nickKey = QStringLiteral("nick");
const auto usernameKey = QStringLiteral("username");
const auto realnameKey = QStringLiteral("realname");
const auto autojoinKey = QStringLiteral("autojoin");
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
        profile.nick = settings.value(nickKey).toString();
        profile.username = settings.value(usernameKey).toString();
        profile.realname = settings.value(realnameKey).toString();
        profile.autojoinChannels = settings.value(autojoinKey).toStringList();
        settings.endGroup();
        result.append(profile);
    }
    settings.endGroup();
    return result;
}

void IrcProfileStore::save(const IrcNetworkProfile &profile)
{
    QSettings settings;
    settings.beginGroup(networksGroup);
    settings.beginGroup(profile.networkId);
    settings.remove(QString());
    settings.setValue(hostKey, profile.host);
    settings.setValue(portKey, profile.port);
    settings.setValue(tlsKey, profile.tlsEnabled);
    settings.setValue(nickKey, profile.nick);
    settings.setValue(usernameKey, profile.username);
    settings.setValue(realnameKey, profile.realname);
    settings.setValue(autojoinKey, profile.autojoinChannels);
    settings.endGroup();
    settings.endGroup();
    settings.sync();
}
