#include "ircprofilestore.h"

#include <QFileInfo>
#include <QSettings>

namespace
{
const auto networksGroup = QStringLiteral("networks");
const auto nameKey = QStringLiteral("name");
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
const auto autojoinKeyChannelsKey = QStringLiteral("autojoinKeyChannels");
const auto autojoinKeyValuesKey = QStringLiteral("autojoinKeyValues");
const auto iconColorKey = QStringLiteral("iconColor");
const auto avatarUrlKey = QStringLiteral("avatarUrl");

QMap<QString, QString> loadAutojoinKeys(const QStringList &channels,
                                        const QStringList &names,
                                        const QStringList &values)
{
    QMap<QString, QString> keys;
    if (names.size() != values.size())
        return keys;
    for (const QString &channel : channels) {
        for (int index = 0; index < names.size(); ++index) {
            if (names.at(index).isEmpty() || values.at(index).isEmpty())
                continue;
            if (QString::compare(channel, names.at(index), Qt::CaseInsensitive) != 0)
                continue;
            keys.insert(channel, values.at(index));
            break;
        }
    }
    return keys;
}

bool existingSettingsFileIsNotWritable(const QString &path)
{
    const QFileInfo info(path);
    return info.exists() && !info.isWritable();
}

bool missingSettingsFile(const QSettings &settings)
{
    // NativeFormat is the registry on Windows. fileName() is an
    // \HKEY_... location, so QFileInfo::exists() is false and every
    // remove would take the missing-file shortcut. On macOS and Linux,
    // NativeFormat is a plist or ini file.
#ifdef Q_OS_WIN
    if (settings.format() == QSettings::NativeFormat)
        return false;
#endif
    return !QFileInfo::exists(settings.fileName());
}

IrcProfileStore::Status statusFrom(QSettings::Status status)
{
    switch (status) {
    case QSettings::NoError:
        return IrcProfileStore::Status::Written;
    case QSettings::AccessError:
        return IrcProfileStore::Status::AccessError;
    case QSettings::FormatError:
        return IrcProfileStore::Status::FormatError;
    }
    return IrcProfileStore::Status::AccessError;
}
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
        profile.name = settings.value(nameKey).toString();
        if (profile.name.trimmed().isEmpty())
            profile.name = profile.host;
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
        profile.autojoinKeys = loadAutojoinKeys(
            profile.autojoinChannels,
            settings.value(autojoinKeyChannelsKey).toStringList(),
            settings.value(autojoinKeyValuesKey).toStringList());
        bool iconOk = false;
        const int iconColor = settings.value(iconColorKey).toInt(&iconOk);
        profile.iconColor = (iconOk
                             && iconColor >= 0
                             && iconColor < IrcNetworkProfile::iconColorCount)
            ? iconColor : IrcNetworkProfile::noIconColor;
        profile.avatarUrl = settings.value(avatarUrlKey).toString();
        settings.endGroup();
        result.append(profile);
    }
    settings.endGroup();
    return result;
}

IrcProfileStore::Status IrcProfileStore::save(const IrcNetworkProfile &profile)
{
    if (profile.networkId.isEmpty())
        return Status::Absent;

    QSettings settings;
    // Pending keys survive a failed sync() in the process-wide QSettings
    // cache, so refuse before mutating a file that cannot accept the write.
    if (existingSettingsFileIsNotWritable(settings.fileName()))
        return Status::AccessError;

    settings.beginGroup(networksGroup);
    settings.beginGroup(profile.networkId);
    settings.remove(QString());
    if (!profile.name.trimmed().isEmpty())
        settings.setValue(nameKey, profile.name);
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
    if (!profile.autojoinKeys.isEmpty()) {
        settings.setValue(autojoinKeyChannelsKey, profile.autojoinKeys.keys());
        settings.setValue(autojoinKeyValuesKey, profile.autojoinKeys.values());
    }
    if (profile.iconColor >= 0 && profile.iconColor < IrcNetworkProfile::iconColorCount)
        settings.setValue(iconColorKey, profile.iconColor);
    if (!profile.avatarUrl.isEmpty())
        settings.setValue(avatarUrlKey, profile.avatarUrl);
    settings.endGroup();
    settings.endGroup();
    settings.sync();
    return statusFrom(settings.status());
}

IrcProfileStore::Status IrcProfileStore::remove(const QString &networkId)
{
    if (networkId.isEmpty())
        return Status::Absent;

    QSettings settings;
    // A missing ini or plist has no group to delete. sync() would try to
    // create it and report AccessError when that directory cannot be created.
    // QSettings's constructor can already set that status; ignore it unless
    // a cached group was actually removed.
    if (missingSettingsFile(settings)) {
        settings.beginGroup(networksGroup);
        const bool present = settings.childGroups().contains(networkId);
        if (present)
            settings.remove(networkId);
        settings.endGroup();
        if (present && settings.status() != QSettings::NoError)
            return statusFrom(settings.status());
        return present ? Status::Written : Status::Absent;
    }

    settings.sync();
    switch (settings.status()) {
    case QSettings::AccessError:
        return Status::AccessError;
    case QSettings::FormatError:
        return Status::FormatError;
    case QSettings::NoError:
        break;
    }

    // childGroups() with an empty prefix parses every section. A missing
    // '=' outside [networks] stays NoError until that parse. Return before
    // remove() so a later sync() cannot write the merged map back.
    settings.childGroups();
    switch (settings.status()) {
    case QSettings::AccessError:
        return Status::AccessError;
    case QSettings::FormatError:
        return Status::FormatError;
    case QSettings::NoError:
        break;
    }

    settings.beginGroup(networksGroup);
    // sync() leaves a value line with no '=' as NoError. childGroups()
    // parses those lines and is what sets FormatError. Return that status
    // before remove() and before a missing group is Absent, so the merged
    // map is not written back over the file.
    const QStringList groups = settings.childGroups();
    switch (settings.status()) {
    case QSettings::AccessError:
        settings.endGroup();
        return Status::AccessError;
    case QSettings::FormatError:
        settings.endGroup();
        return Status::FormatError;
    case QSettings::NoError:
        break;
    }
    if (!groups.contains(networkId)) {
        settings.endGroup();
        return Status::Absent;
    }
    if (existingSettingsFileIsNotWritable(settings.fileName())) {
        settings.endGroup();
        return Status::AccessError;
    }
    settings.remove(networkId);
    settings.endGroup();
    if (settings.status() != QSettings::NoError)
        return statusFrom(settings.status());
    settings.sync();
    return statusFrom(settings.status());
}
