#include "ircprofilestore.h"

#include <QCoreApplication>
#include <QFile>
#include <QFileInfo>
#include <QSettings>
#include <QStandardPaths>

#include <optional>

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

bool settingsFileMissingOnDisk()
{
#ifdef Q_OS_WIN
    QSettings probe;
    probe.setAtomicSyncRequired(true);
    return missingSettingsFile(probe);
#else
    const QString org = QCoreApplication::organizationName();
    const QString app = QCoreApplication::applicationName();
    const QString xdg = QString::fromUtf8(qgetenv("XDG_CONFIG_HOME"));
    const QString root = xdg.isEmpty()
        ? QStandardPaths::writableLocation(QStandardPaths::GenericConfigLocation)
        : xdg;
    const QString path =
#if defined(Q_OS_DARWIN)
        root + QLatin1Char('/') + org + QLatin1Char('/') + app + QLatin1String(".ini");
#else
        root + QLatin1Char('/') + org + QLatin1Char('/') + app + QLatin1String(".conf");
#endif
    return !QFileInfo::exists(path);
#endif
}

bool settingsFileIsIni(const QSettings &settings)
{
    const QSettings::Format format = settings.format();
#if defined(Q_OS_WIN) || defined(Q_OS_DARWIN)
    // Windows NativeFormat is the registry. Darwin NativeFormat is a plist.
    if (format == QSettings::NativeFormat)
        return false;
#endif
    return format <= QSettings::IniFormat;
}

bool iniSpace(char ch)
{
    return ch == ' ' || ch == '\t' || ch == '\n' || ch == '\r';
}

bool iniSpecial(unsigned char ch)
{
    return ch == '\n' || ch == '\r' || ch == '"' || ch == ';' || ch == '=' || ch == '\\';
}

// Qt 6.8 readIniLine. A leading ';' is a comment. '=' inside quotes does not
// count, and a backslash consumes the next character (including a newline).
bool nextIniLine(const QByteArray &data, qsizetype &dataPos, qsizetype &lineStart,
                 qsizetype &lineLen, qsizetype &equalsPos)
{
    const qsizetype dataLen = data.size();
    bool inQuotes = false;
    equalsPos = -1;

    lineStart = dataPos;
    while (lineStart < dataLen && iniSpace(data.at(lineStart)))
        ++lineStart;

    qsizetype i = lineStart;
    bool ended = false;
    while (i < dataLen && !ended) {
        const unsigned char ch = static_cast<unsigned char>(data.at(i));
        if (!iniSpecial(ch)) {
            ++i;
            continue;
        }

        ++i;
        if (ch == '=') {
            if (!inQuotes && equalsPos == -1)
                equalsPos = i - 1;
        } else if (ch == '\n' || ch == '\r') {
            if (i == lineStart + 1)
                ++lineStart;
            else if (!inQuotes) {
                --i;
                ended = true;
            }
        } else if (ch == '\\') {
            if (i < dataLen) {
                const char escaped = data.at(i++);
                if (i < dataLen) {
                    const char next = data.at(i);
                    if ((escaped == '\n' && next == '\r')
                        || (escaped == '\r' && next == '\n')) {
                        ++i;
                    }
                }
            }
        } else if (ch == '"') {
            inQuotes = !inQuotes;
        } else if (i == lineStart + 1) {
            while (i < dataLen) {
                const char comment = data.at(i);
                if (comment == '\n' || comment == '\r')
                    break;
                ++i;
            }
            while (i < dataLen && iniSpace(data.at(i)))
                ++i;
            lineStart = i;
        } else if (!inQuotes) {
            --i;
            ended = true;
        }
    }

    dataPos = i;
    lineLen = i - lineStart;
    return lineLen > 0;
}

// readIniFile rejects a '[' line with no ']'. readIniSection rejects a
// non-empty, non-comment line whose equalsPos stays -1. Both set FormatError.
// childGroups() and a writing sync() run that second parse, drop the bad
// line from the process-wide cache, and the next QSettings reports NoError.
bool iniBytesAreMalformed(const QByteArray &data)
{
    QByteArray bytes = data;
    if (bytes.startsWith("\xEF\xBB\xBF"))
        bytes.remove(0, 3);

    qsizetype dataPos = 0;
    qsizetype lineStart = 0;
    qsizetype lineLen = 0;
    qsizetype equalsPos = -1;
    while (nextIniLine(bytes, dataPos, lineStart, lineLen, equalsPos)) {
        if (bytes.at(lineStart) == '[') {
            const qsizetype close = bytes.indexOf(']', lineStart);
            if (close < 0 || close >= lineStart + lineLen)
                return true;
            continue;
        }
        if (equalsPos < 0 && bytes.at(lineStart) != ';')
            return true;
    }
    return false;
}

std::optional<IrcProfileStore::Status> blockedIniWrite(const QSettings &settings)
{
    switch (IrcProfileStore::probeSettingsIni(settings)) {
    case IrcProfileStore::IniProbe::Malformed:
        return IrcProfileStore::Status::FormatError;
    case IrcProfileStore::IniProbe::Unreadable:
        return IrcProfileStore::Status::AccessError;
    case IrcProfileStore::IniProbe::Ok:
        return std::nullopt;
    }
    return IrcProfileStore::Status::AccessError;
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

IrcProfileStore::IniProbe IrcProfileStore::probeSettingsIni(const QSettings &settings)
{
    // Windows NativeFormat is the registry. Skip the read there. A missing
    // file stays the missing-file shortcut in remove(), not Unreadable.
    if (!settingsFileIsIni(settings))
        return IniProbe::Ok;
    const QString path = settings.fileName();
    if (path.isEmpty() || !QFileInfo::exists(path))
        return IniProbe::Ok;
    QFile file(path);
    // Exists but cannot be opened: unreadable, not a well-formed ini.
    // QSettings::status() on a warm cache stays NoError because sync()
    // short-circuits on size and mtime, so this has to be the file bytes.
    if (!file.open(QIODevice::ReadOnly))
        return IniProbe::Unreadable;
    if (iniBytesAreMalformed(file.readAll()))
        return IniProbe::Malformed;
    return IniProbe::Ok;
}

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
    // Refuse before setValue. A writing sync() parses the ini, drops a bad
    // line from the process-wide cache, and still rewrites the file. An
    // unreadable file is AccessError for the same reason: setValue would
    // overwrite it from that cache.
    if (const std::optional<Status> blocked = blockedIniWrite(settings))
        return *blocked;
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

    // Check before constructing QSettings. ~QSettings would flush pending keys
    // from the process-wide cache and create a missing ini from cached data.
    if (settingsFileMissingOnDisk())
        return Status::Absent;

    QSettings settings;

    // childGroups() is what parses every section. Once it has run, a later
    // QSettings on this path reports NoError and save() rewrites the file.
    // Read the bytes before sync() or childGroups() so a bad line stays put.
    // Open failure is AccessError, before a missing group can be Absent.
    if (const std::optional<Status> blocked = blockedIniWrite(settings))
        return *blocked;

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
