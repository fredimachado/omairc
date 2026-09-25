#include <QCoreApplication>
#ifdef Q_OS_LINUX
#include <QFile>
#endif
#include <QRegularExpression>
#include <QSettings>
#include <QTemporaryDir>
#include <QTest>

#include "ircnetworkprofile.h"
#include "ircprofilestore.h"
#include "testsettings.h"

#include <memory>

class ProfileTest : public QObject
{
    Q_OBJECT

private slots:
    void init();
    void createMintsElevenCharNetworkId();
    void suggestedPrefillsLiberachat();
    void validateRefusesIncompleteAndUnsendable();
    void validateRefusesUnsendableAccountAndBouncerNetwork();
    void saslAccountFallsBackToNickAndAppendsBouncerNetwork();
    void storeRoundTripsFieldsWithoutPassword();
    void storeRoundTripsAutojoinKeys();
    void parseAutojoinDoesNotInventChannelFromFollowingToken();
    void pickIconColorTakesTheOnlyFreeSlot();
    void ensureIconColorAssignsOnce();
    void missingIconColorDefaultsToNone();
    void missingNameDefaultsToHost();
    void emptyNameOmitsNameKey();
    void storeRoundTripsAvatarUrl();
    void missingAvatarUrlDefaultsToEmpty();
    void emptyAvatarUrlOmitsAvatarUrlKey();
    void resolvedNameFallsBackToHost();
    void missingConnectOnStartupDefaultsToFalse();
    void missingSecretSavedDefaultsToFalse();
    void missingNickServSavedDefaultsToFalse();
    void usernameAndRealnameStayAsTyped();
    void storeRemoveDropsTheNetworkGroup();
#ifdef Q_OS_LINUX
    void storeReadOnlyIniReturnsAccessError();
    void storeRemoveMissingFileIsAbsent();
    void storeRemoveMissingEqualsWithoutGroupReturnsFormatError();
    void storeRemoveMissingEqualsWithGroupReturnsFormatError();
    void storeRemoveMissingEqualsInPreferencesReturnsFormatError();
    void storeSaveMissingEqualsReturnsFormatError();
    void storeSaveAfterRemoveMissingEqualsReturnsFormatError();
#endif

private:
#ifdef Q_OS_LINUX
    QString settingsFile() const;
#endif

    std::unique_ptr<QTemporaryDir> m_dir;
};

void ProfileTest::init()
{
    m_dir = std::make_unique<QTemporaryDir>();
    QVERIFY(m_dir->isValid());
    TestSettings::isolate(m_dir->path());
    QCoreApplication::setOrganizationName(QStringLiteral("omairc"));
    QCoreApplication::setApplicationName(QStringLiteral("omairc"));
}

#ifdef Q_OS_LINUX
QString ProfileTest::settingsFile() const
{
    QSettings settings;
    return settings.fileName();
}
#endif

void ProfileTest::createMintsElevenCharNetworkId()
{
    const QString id = IrcNetworkProfile::create().networkId;
    QCOMPARE(id.size(), 11);
    QVERIFY(QRegularExpression(QStringLiteral("^[A-Za-z0-9_-]{11}$")).match(id).hasMatch());
    QVERIFY(IrcNetworkProfile::create().networkId != id);
}

void ProfileTest::suggestedPrefillsLiberachat()
{
    QCOMPARE(IrcNetworkProfile::create().iconColor, IrcNetworkProfile::noIconColor);
    const IrcNetworkProfile profile = IrcNetworkProfile::suggested();
    QVERIFY(!profile.networkId.isEmpty());
    QCOMPARE(profile.iconColor, IrcNetworkProfile::noIconColor);
    QCOMPARE(profile.host, QStringLiteral("irc.libera.chat"));
    QCOMPARE(profile.name, QStringLiteral("irc.libera.chat"));
    QCOMPARE(profile.port, quint16(6697));
    QVERIFY(profile.tlsEnabled);
    QCOMPARE(profile.autojoinChannels, QStringList{QStringLiteral("#omarchy")});
    QVERIFY(profile.nick.isEmpty());
    QVERIFY(profile.username.isEmpty());
    QVERIFY(profile.realname.isEmpty());
    QVERIFY(!profile.isComplete());
}

void ProfileTest::validateRefusesIncompleteAndUnsendable()
{
    IrcNetworkProfile profile = IrcNetworkProfile::suggested();
    QCOMPARE(profile.validate(), IrcNetworkProfile::Problem::MissingNick);

    profile.nick = QStringLiteral("omairc");
    QVERIFY(profile.isComplete());

    QCOMPARE(IrcNetworkProfile::parseAutojoin(QStringLiteral("omarchy, #desktop")),
             QStringList({QStringLiteral("omarchy"), QStringLiteral("#desktop")}));
    profile.autojoinChannels = IrcNetworkProfile::parseAutojoin(
        QStringLiteral("omarchy &local"));
    QCOMPARE(profile.normalized().autojoinChannels,
             QStringList({QStringLiteral("#omarchy"), QStringLiteral("&local")}));
    QVERIFY(profile.isComplete());

    profile.autojoinChannels = {QStringLiteral("+plus")};
    QCOMPARE(profile.normalized().autojoinChannels,
             QStringList({QStringLiteral("#+plus")}));

    profile.host.clear();
    QCOMPARE(profile.validate(), IrcNetworkProfile::Problem::MissingHost);

    profile.host = QStringLiteral("irc.libera.chat");
    profile.port = 0;
    QCOMPARE(profile.validate(), IrcNetworkProfile::Problem::InvalidPort);

    profile.port = 6697;
    profile.nick = QStringLiteral("bad nick");
    QCOMPARE(profile.validate(), IrcNetworkProfile::Problem::UnsendableIdentity);

    profile.nick = QStringLiteral("omairc");
    profile.username = QStringLiteral("bad user");
    QCOMPARE(profile.validate(), IrcNetworkProfile::Problem::UnsendableIdentity);

    profile.username.clear();
    profile.autojoinChannels = {QStringLiteral("#omarchy") + QChar(u'\0') + QStringLiteral("x")};
    QCOMPARE(profile.validate(), IrcNetworkProfile::Problem::UnsendableChannel);
}

void ProfileTest::validateRefusesUnsendableAccountAndBouncerNetwork()
{
    IrcNetworkProfile profile = IrcNetworkProfile::suggested();
    profile.nick = QStringLiteral("omairc");
    QVERIFY(profile.account.isEmpty());
    QVERIFY(profile.bouncerNetwork.isEmpty());
    QCOMPARE(profile.validate(), IrcNetworkProfile::Problem::None);
    QVERIFY(profile.isComplete());

    profile.account = QStringLiteral("joe/libera");
    QCOMPARE(profile.validate(), IrcNetworkProfile::Problem::UnsendableAccount);
    QCOMPARE(IrcNetworkProfile::problemText(profile.validate()),
             QStringLiteral("Account cannot contain a space or a slash"));

    profile.account = QStringLiteral("joe libera");
    QCOMPARE(profile.validate(), IrcNetworkProfile::Problem::UnsendableAccount);

    profile.account = QStringLiteral("joe");
    profile.bouncerNetwork = QStringLiteral("libera/extra");
    QCOMPARE(profile.validate(), IrcNetworkProfile::Problem::UnsendableBouncerNetwork);
    QCOMPARE(IrcNetworkProfile::problemText(profile.validate()),
             QStringLiteral("Bouncer network cannot contain a space or a slash"));

    profile.bouncerNetwork = QStringLiteral("libera chat");
    QCOMPARE(profile.validate(), IrcNetworkProfile::Problem::UnsendableBouncerNetwork);

    profile.bouncerNetwork = QStringLiteral("libera");
    QVERIFY(profile.isComplete());
}

void ProfileTest::saslAccountFallsBackToNickAndAppendsBouncerNetwork()
{
    IrcNetworkProfile profile = IrcNetworkProfile::create();
    profile.host = QStringLiteral("irc.example.net");
    profile.nick = QStringLiteral("omairc");
    QCOMPARE(profile.saslAccount(), QStringLiteral("omairc"));

    profile.account = QStringLiteral("joe");
    QCOMPARE(profile.saslAccount(), QStringLiteral("joe"));

    profile.bouncerNetwork = QStringLiteral("libera");
    QCOMPARE(profile.saslAccount(), QStringLiteral("joe/libera"));

    profile.account.clear();
    QCOMPARE(profile.saslAccount(), QStringLiteral("omairc/libera"));

    profile.account = QStringLiteral("  joe  ");
    profile.bouncerNetwork = QStringLiteral("  libera  ");
    QCOMPARE(profile.saslAccount(), QStringLiteral("joe/libera"));
}

void ProfileTest::storeRoundTripsFieldsWithoutPassword()
{
    IrcNetworkProfile profile = IrcNetworkProfile::create();
    profile.name = QStringLiteral("Example Net");
    profile.host = QStringLiteral("irc.example.net");
    profile.port = 6697;
    profile.tlsEnabled = true;
    profile.connectOnStartup = true;
    profile.secretSaved = true;
    profile.nickServSaved = true;
    profile.nick = QStringLiteral("omairc");
    profile.username = QStringLiteral("omaircuser");
    profile.realname = QStringLiteral("Omairc User");
    profile.account = QStringLiteral("joe");
    profile.bouncerNetwork = QStringLiteral("libera");
    profile.autojoinChannels = {QStringLiteral("#omarchy"), QStringLiteral("#desktop")};
    profile.iconColor = 3;

    IrcProfileStore store;
    store.save(profile);

    const QList<IrcNetworkProfile> loaded = IrcProfileStore().profiles();
    QCOMPARE(loaded.size(), 1);
    QCOMPARE(loaded.first().networkId, profile.networkId);
    QCOMPARE(loaded.first().name, profile.name);
    QCOMPARE(loaded.first().host, profile.host);
    QCOMPARE(loaded.first().port, profile.port);
    QCOMPARE(loaded.first().tlsEnabled, profile.tlsEnabled);
    QCOMPARE(loaded.first().connectOnStartup, profile.connectOnStartup);
    QCOMPARE(loaded.first().secretSaved, true);
    QCOMPARE(loaded.first().nickServSaved, true);
    QCOMPARE(loaded.first().nick, profile.nick);
    QCOMPARE(loaded.first().username, profile.username);
    QCOMPARE(loaded.first().realname, profile.realname);
    QCOMPARE(loaded.first().account, profile.account);
    QCOMPARE(loaded.first().bouncerNetwork, profile.bouncerNetwork);
    QCOMPARE(loaded.first().autojoinChannels, profile.autojoinChannels);
    QCOMPARE(loaded.first().iconColor, 3);
    QCOMPARE(loaded.first(), profile);
    QCOMPARE(loaded.first().saslAccount(), QStringLiteral("joe/libera"));

#ifdef Q_OS_LINUX
    QFile file(settingsFile());
    QVERIFY(file.exists());
    QVERIFY(settingsFile().endsWith(QStringLiteral("/omairc/omairc.conf")));
    QVERIFY(file.open(QIODevice::ReadOnly | QIODevice::Text));
    const QString contents = QString::fromUtf8(file.readAll());
    QVERIFY(contents.contains(profile.networkId));
    QVERIFY(contents.contains(profile.name));
    QVERIFY(contents.contains(profile.host));
    QVERIFY(contents.contains(QLatin1String("networks")));
    QVERIFY(contents.contains(QLatin1String("secretSaved")));
    QVERIFY(contents.contains(QLatin1String("nickServSaved")));
    QVERIFY(contents.contains(QLatin1String("account=joe")));
    QVERIFY(contents.contains(QLatin1String("bouncerNetwork=libera")));
    QVERIFY(!contents.contains(QLatin1String("password"), Qt::CaseInsensitive));
    QVERIFY(!contents.contains(QLatin1String("nick-secret")));
#endif

    QSettings stored;
    QVERIFY(!stored.allKeys().isEmpty());
    for (const QString &key : stored.allKeys()) {
        QVERIFY(!key.contains(QLatin1String("password"), Qt::CaseInsensitive));
        QVERIFY(!key.contains(QLatin1String("nick-secret")));
        const QString value = stored.value(key).toString();
        QVERIFY(!value.contains(QLatin1String("password"), Qt::CaseInsensitive));
        QVERIFY(!value.contains(QLatin1String("nick-secret")));
    }

    IrcNetworkProfile unnamed;
    unnamed.host = QStringLiteral("irc.example.net");
    unnamed.nick = QStringLiteral("omairc");
    store.save(unnamed);
    QCOMPARE(IrcProfileStore().profiles().size(), 1);
}

void ProfileTest::storeRoundTripsAutojoinKeys()
{
    IrcNetworkProfile profile = IrcNetworkProfile::create();
    profile.host = QStringLiteral("irc.example.net");
    profile.nick = QStringLiteral("omairc");
    profile.autojoinChannels = {QStringLiteral("#omarchy"), QStringLiteral("#private")};
    profile.autojoinKeys.insert(QStringLiteral("#private"), QStringLiteral("hunter2"));

    IrcProfileStore().save(profile);
    const IrcNetworkProfile loaded = IrcProfileStore().profiles().first();
    QCOMPARE(loaded.autojoinChannels, profile.autojoinChannels);
    QCOMPARE(loaded.autojoinKeys.keys(), QStringList({QStringLiteral("#private")}));
    if (loaded.autojoinKeys.value(QStringLiteral("#private"))
        != QStringLiteral("hunter2")) {
        QFAIL("stored channel key mismatch");
    }

    profile.autojoinChannels = {QStringLiteral("#omarchy")};
    QVERIFY(profile.normalized().autojoinKeys.isEmpty());
}

void ProfileTest::parseAutojoinDoesNotInventChannelFromFollowingToken()
{
    QCOMPARE(IrcNetworkProfile::parseAutojoin(QStringLiteral("#chan secret")),
             QStringList({QStringLiteral("#chan")}));
    QCOMPARE(IrcNetworkProfile::parseAutojoin(QStringLiteral("#chan,secret")),
             QStringList({QStringLiteral("#chan")}));
    QCOMPARE(IrcNetworkProfile::parseAutojoin(QStringLiteral("#omarchy #desktop")),
             QStringList({QStringLiteral("#omarchy"), QStringLiteral("#desktop")}));
    QCOMPARE(IrcNetworkProfile::parseAutojoin(QStringLiteral("omarchy, lab")),
             QStringList({QStringLiteral("omarchy"), QStringLiteral("lab")}));

    IrcNetworkProfile profile = IrcNetworkProfile::suggested();
    profile.nick = QStringLiteral("omairc");
    profile.autojoinChannels = IrcNetworkProfile::parseAutojoin(
        QStringLiteral("#chan secret"));
    QCOMPARE(profile.normalized().autojoinChannels,
             QStringList({QStringLiteral("#chan")}));
    QVERIFY(!profile.normalized().autojoinChannels.contains(QStringLiteral("#secret")));
}

void ProfileTest::pickIconColorTakesTheOnlyFreeSlot()
{
    QCOMPARE(IrcNetworkProfile::pickIconColor({0, 1, 2, 3}), 4);
    const int wrapped = IrcNetworkProfile::pickIconColor({0, 1, 2, 3, 4});
    QVERIFY(wrapped >= 0 && wrapped < IrcNetworkProfile::iconColorCount);
}

void ProfileTest::ensureIconColorAssignsOnce()
{
    IrcNetworkProfile profile = IrcNetworkProfile::create();
    QCOMPARE(profile.iconColor, IrcNetworkProfile::noIconColor);

    QVERIFY(profile.ensureIconColor({}));
    const int first = profile.iconColor;
    QVERIFY(first >= 0 && first < IrcNetworkProfile::iconColorCount);
    QVERIFY(!profile.ensureIconColor({first}));
    QCOMPARE(profile.iconColor, first);

    IrcNetworkProfile next = IrcNetworkProfile::create();
    QVERIFY(next.ensureIconColor({first}));
    QVERIFY(next.iconColor != first);

    IrcNetworkProfile invalid;
    invalid.iconColor = 99;
    QVERIFY(invalid.ensureIconColor({0, 1, 2, 3}));
    QCOMPARE(invalid.iconColor, 4);
}

void ProfileTest::missingIconColorDefaultsToNone()
{
    IrcNetworkProfile profile = IrcNetworkProfile::create();
    profile.host = QStringLiteral("irc.example.net");
    profile.nick = QStringLiteral("omairc");
    profile.iconColor = 2;
    IrcProfileStore().save(profile);

    QSettings settings;
    settings.beginGroup(QStringLiteral("networks"));
    settings.beginGroup(profile.networkId);
    settings.remove(QStringLiteral("iconColor"));
    settings.endGroup();
    settings.endGroup();
    settings.sync();

    QCOMPARE(IrcProfileStore().profiles().first().iconColor,
             IrcNetworkProfile::noIconColor);
}

void ProfileTest::missingNameDefaultsToHost()
{
    IrcNetworkProfile profile = IrcNetworkProfile::create();
    profile.host = QStringLiteral("irc.example.net");
    profile.nick = QStringLiteral("omairc");
    IrcProfileStore().save(profile);

    QSettings settings;
    settings.beginGroup(QStringLiteral("networks"));
    settings.beginGroup(profile.networkId);
    settings.remove(QStringLiteral("name"));
    settings.endGroup();
    settings.endGroup();
    settings.sync();

    const IrcNetworkProfile loaded = IrcProfileStore().profiles().first();
    QCOMPARE(loaded.host, QStringLiteral("irc.example.net"));
    QCOMPARE(loaded.name, QStringLiteral("irc.example.net"));
    QCOMPARE(loaded.resolvedName(), QStringLiteral("irc.example.net"));

    settings.beginGroup(QStringLiteral("networks"));
    settings.beginGroup(profile.networkId);
    settings.setValue(QStringLiteral("name"), QStringLiteral("   "));
    settings.endGroup();
    settings.endGroup();
    settings.sync();

    const IrcNetworkProfile blank = IrcProfileStore().profiles().first();
    QCOMPARE(blank.name, QStringLiteral("irc.example.net"));
}

void ProfileTest::storeRoundTripsAvatarUrl()
{
    IrcNetworkProfile profile = IrcNetworkProfile::create();
    profile.host = QStringLiteral("irc.example.net");
    profile.nick = QStringLiteral("omairc");
    profile.avatarUrl = QStringLiteral("https://example.com/a.png");

    IrcProfileStore().save(profile);
    const IrcNetworkProfile loaded = IrcProfileStore().profiles().first();
    QCOMPARE(loaded.avatarUrl, QStringLiteral("https://example.com/a.png"));
    QCOMPARE(loaded.networkId, profile.networkId);
    QCOMPARE(loaded.host, profile.host);
}

void ProfileTest::missingAvatarUrlDefaultsToEmpty()
{
    IrcNetworkProfile profile = IrcNetworkProfile::create();
    profile.host = QStringLiteral("irc.example.net");
    profile.nick = QStringLiteral("omairc");
    IrcProfileStore().save(profile);

    QSettings settings;
    settings.beginGroup(QStringLiteral("networks"));
    settings.beginGroup(profile.networkId);
    settings.remove(QStringLiteral("avatarUrl"));
    settings.endGroup();
    settings.endGroup();
    settings.sync();

    QVERIFY(IrcProfileStore().profiles().first().avatarUrl.isEmpty());
}

void ProfileTest::emptyAvatarUrlOmitsAvatarUrlKey()
{
    IrcNetworkProfile profile = IrcNetworkProfile::create();
    profile.host = QStringLiteral("irc.example.net");
    profile.nick = QStringLiteral("omairc");
    QVERIFY(profile.avatarUrl.isEmpty());
    IrcProfileStore().save(profile);

    QSettings settings;
    settings.beginGroup(QStringLiteral("networks"));
    settings.beginGroup(profile.networkId);
    QVERIFY(!settings.contains(QStringLiteral("avatarUrl")));
    settings.endGroup();
    settings.endGroup();
}

void ProfileTest::emptyNameOmitsNameKey()
{
    IrcNetworkProfile profile = IrcNetworkProfile::create();
    profile.host = QStringLiteral("irc.example.net");
    profile.nick = QStringLiteral("omairc");
    QVERIFY(profile.name.isEmpty());
    IrcProfileStore().save(profile);

    QSettings settings;
    settings.beginGroup(QStringLiteral("networks"));
    settings.beginGroup(profile.networkId);
    QVERIFY(!settings.contains(QStringLiteral("name")));
    QCOMPARE(settings.value(QStringLiteral("host")).toString(),
             QStringLiteral("irc.example.net"));
    settings.endGroup();
    settings.endGroup();

    profile.name = QStringLiteral("   ");
    IrcProfileStore().save(profile);
    settings.beginGroup(QStringLiteral("networks"));
    settings.beginGroup(profile.networkId);
    QVERIFY(!settings.contains(QStringLiteral("name")));
    settings.endGroup();
    settings.endGroup();
}

void ProfileTest::resolvedNameFallsBackToHost()
{
    IrcNetworkProfile profile;
    profile.host = QStringLiteral("irc.example.net");
    QVERIFY(profile.name.isEmpty());
    QCOMPARE(profile.resolvedName(), QStringLiteral("irc.example.net"));

    profile.name = QStringLiteral("  Example  ");
    QCOMPARE(profile.resolvedName(), QStringLiteral("Example"));
    QCOMPARE(IrcNetworkProfile::resolvedName(QStringLiteral("  Example  "),
                                             QStringLiteral("irc.example.net")),
             QStringLiteral("Example"));
    QCOMPARE(IrcNetworkProfile::resolvedName(QString(),
                                             QStringLiteral("irc.example.net")),
             QStringLiteral("irc.example.net"));
    QCOMPARE(profile.normalized().name, QStringLiteral("Example"));
}

void ProfileTest::missingConnectOnStartupDefaultsToFalse()
{
    IrcNetworkProfile profile = IrcNetworkProfile::create();
    profile.host = QStringLiteral("irc.example.net");
    profile.nick = QStringLiteral("omairc");
    IrcProfileStore().save(profile);

    QSettings settings;
    settings.beginGroup(QStringLiteral("networks"));
    settings.beginGroup(profile.networkId);
    settings.remove(QStringLiteral("connectOnStartup"));
    settings.endGroup();
    settings.endGroup();
    settings.sync();

    QCOMPARE(IrcProfileStore().profiles().first().connectOnStartup, false);
}

void ProfileTest::missingSecretSavedDefaultsToFalse()
{
    IrcNetworkProfile profile = IrcNetworkProfile::create();
    profile.host = QStringLiteral("irc.example.net");
    profile.nick = QStringLiteral("omairc");
    IrcProfileStore().save(profile);

    QSettings settings;
    settings.beginGroup(QStringLiteral("networks"));
    settings.beginGroup(profile.networkId);
    settings.remove(QStringLiteral("secretSaved"));
    settings.endGroup();
    settings.endGroup();
    settings.sync();

    QCOMPARE(IrcProfileStore().profiles().first().secretSaved, false);
}

void ProfileTest::missingNickServSavedDefaultsToFalse()
{
    IrcNetworkProfile profile = IrcNetworkProfile::create();
    profile.host = QStringLiteral("irc.example.net");
    profile.nick = QStringLiteral("omairc");
    IrcProfileStore().save(profile);

    QSettings settings;
    settings.beginGroup(QStringLiteral("networks"));
    settings.beginGroup(profile.networkId);
    settings.remove(QStringLiteral("nickServSaved"));
    settings.endGroup();
    settings.endGroup();
    settings.sync();

    QCOMPARE(IrcProfileStore().profiles().first().nickServSaved, false);
}

void ProfileTest::usernameAndRealnameStayAsTyped()
{
    IrcNetworkProfile profile = IrcNetworkProfile::create();
    profile.host = QStringLiteral("irc.example.net");
    profile.nick = QStringLiteral("omairc");
    profile.username = QString();
    profile.realname = QString();
    profile.autojoinChannels = {QStringLiteral("#omarchy")};

    IrcProfileStore store;
    store.save(profile);

    const IrcNetworkProfile loaded = IrcProfileStore().profiles().first();
    QVERIFY(loaded.username.isEmpty());
    QVERIFY(loaded.realname.isEmpty());
    QVERIFY(loaded.isComplete());
}

void ProfileTest::storeRemoveDropsTheNetworkGroup()
{
    IrcNetworkProfile keep = IrcNetworkProfile::create();
    keep.host = QStringLiteral("irc.example.net");
    keep.nick = QStringLiteral("omairc");
    IrcNetworkProfile drop = IrcNetworkProfile::create();
    drop.host = QStringLiteral("irc.oftc.net");
    drop.nick = QStringLiteral("oak");

    IrcProfileStore store;
    store.save(keep);
    store.save(drop);
    QCOMPARE(IrcProfileStore().profiles().size(), 2);

    QCOMPARE(store.remove(drop.networkId), IrcProfileStore::Status::Written);
    QCOMPARE(store.remove(drop.networkId), IrcProfileStore::Status::Absent);

    const QList<IrcNetworkProfile> loaded = IrcProfileStore().profiles();
    QCOMPARE(loaded.size(), 1);
    QCOMPARE(loaded.first().networkId, keep.networkId);
    QCOMPARE(loaded.first().host, QStringLiteral("irc.example.net"));

#ifdef Q_OS_LINUX
    QFile file(settingsFile());
    QVERIFY(file.open(QIODevice::ReadOnly | QIODevice::Text));
    const QString contents = QString::fromUtf8(file.readAll());
    QVERIFY(contents.contains(keep.networkId));
    QVERIFY(!contents.contains(drop.networkId));
    QVERIFY(!contents.contains(QLatin1String("irc.oftc.net")));
#else
    QSettings stored;
    const QStringList keys = stored.allKeys();
    const QString joinedKeys = keys.join(QLatin1Char('\n'));
    QVERIFY(joinedKeys.contains(keep.networkId));
    QVERIFY(!joinedKeys.contains(drop.networkId));
    const QStringList values = [&stored]() {
        QStringList out;
        for (const QString &key : stored.allKeys())
            out.append(stored.value(key).toString());
        return out;
    }();
    QVERIFY(!values.join(QLatin1Char('\n')).contains(QLatin1String("irc.oftc.net")));
#endif
}

#ifdef Q_OS_LINUX
void ProfileTest::storeReadOnlyIniReturnsAccessError()
{
    IrcNetworkProfile profile = IrcNetworkProfile::create();
    profile.host = QStringLiteral("irc.example.net");
    profile.nick = QStringLiteral("omairc");
    QVERIFY(profile.isComplete());

    IrcProfileStore store;
    QCOMPARE(store.save(profile), IrcProfileStore::Status::Written);

    const QString path = settingsFile();
    QFile ini(path);
    QVERIFY(ini.exists());
    const QFile::Permissions original = ini.permissions();
    struct PermissionGuard
    {
        explicit PermissionGuard(QString filePath, QFile::Permissions saved)
            : filePath(std::move(filePath))
            , saved(saved)
        {
        }
        ~PermissionGuard()
        {
            if (active)
                QFile::setPermissions(filePath, saved);
        }
        void dismiss() { active = false; }

        QString filePath;
        QFile::Permissions saved;
        bool active = true;
    } guard(path, original);

    const QFile::Permissions readOnly = original
        & ~(QFile::WriteOwner | QFile::WriteUser | QFile::WriteGroup | QFile::WriteOther);
    QVERIFY(ini.setPermissions(readOnly));

    IrcNetworkProfile changed = profile;
    changed.host = QStringLiteral("irc.changed.example");
    QCOMPARE(store.save(changed), IrcProfileStore::Status::AccessError);
    QCOMPARE(store.remove(profile.networkId), IrcProfileStore::Status::AccessError);

    QVERIFY(QFile::setPermissions(path, original));
    guard.dismiss();

    const QList<IrcNetworkProfile> loaded = IrcProfileStore().profiles();
    QCOMPARE(loaded.size(), 1);
    QCOMPARE(loaded.first().networkId, profile.networkId);
    QCOMPARE(loaded.first().host, profile.host);

    QVERIFY(ini.open(QIODevice::ReadOnly | QIODevice::Text));
    const QString contents = QString::fromUtf8(ini.readAll());
    QVERIFY(contents.contains(profile.host));
    QVERIFY(!contents.contains(changed.host));
}

void ProfileTest::storeRemoveMissingFileIsAbsent()
{
    QSettings settings;
    const QString path = settings.fileName();
    QVERIFY2(!QFile::exists(path), qPrintable(path));

    QCOMPARE(IrcProfileStore().remove(QStringLiteral("missing-network")),
             IrcProfileStore::Status::Absent);
    QVERIFY(!QFile::exists(path));
}

static bool writeSettingsFile(const QString &path, const QByteArray &bytes)
{
    QFile file(path);
    if (!file.open(QIODevice::WriteOnly | QIODevice::Truncate))
        return false;
    if (file.write(bytes) != bytes.size())
        return false;
    return file.flush();
}

static QByteArray readSettingsFile(const QString &path)
{
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly))
        return {};
    return file.readAll();
}

void ProfileTest::storeRemoveMissingEqualsWithoutGroupReturnsFormatError()
{
    IrcNetworkProfile profile = IrcNetworkProfile::create();
    profile.host = QStringLiteral("irc.example.net");
    profile.nick = QStringLiteral("omairc");

    IrcProfileStore store;
    QCOMPARE(store.save(profile), IrcProfileStore::Status::Written);

    // No '=' means the network is not a parsed key, so childGroups() drops it.
    const QByteArray corrupt = QByteArrayLiteral("[networks]\n")
        + profile.networkId.toUtf8()
        + QByteArrayLiteral("\\host\n");
    const QString path = settingsFile();
    QVERIFY(writeSettingsFile(path, corrupt));

    QCOMPARE(store.remove(profile.networkId), IrcProfileStore::Status::FormatError);
    QCOMPARE(readSettingsFile(path), corrupt);
}

void ProfileTest::storeRemoveMissingEqualsWithGroupReturnsFormatError()
{
    IrcNetworkProfile profile = IrcNetworkProfile::create();
    profile.host = QStringLiteral("irc.example.net");
    profile.nick = QStringLiteral("omairc");

    IrcProfileStore store;
    QCOMPARE(store.save(profile), IrcProfileStore::Status::Written);

    const QString path = settingsFile();
    QByteArray corrupt = readSettingsFile(path);
    QVERIFY(corrupt.contains(profile.networkId.toUtf8()));
    QVERIFY(corrupt.contains('='));
    if (!corrupt.endsWith('\n'))
        corrupt.append('\n');
    // A value line with no '=' beside real keys: the group stays listed.
    corrupt.append(profile.networkId.toUtf8());
    corrupt.append(QByteArrayLiteral("\\port\n"));
    QVERIFY(writeSettingsFile(path, corrupt));

    QCOMPARE(store.remove(profile.networkId), IrcProfileStore::Status::FormatError);

    const QByteArray after = readSettingsFile(path);
    QCOMPARE(after, corrupt);
    QVERIFY(after.contains(profile.networkId.toUtf8()));
}

void ProfileTest::storeRemoveMissingEqualsInPreferencesReturnsFormatError()
{
    IrcNetworkProfile profile = IrcNetworkProfile::create();
    profile.host = QStringLiteral("irc.example.net");
    profile.nick = QStringLiteral("omairc");

    IrcProfileStore store;
    QCOMPARE(store.save(profile), IrcProfileStore::Status::Written);

    const QString path = settingsFile();
    QByteArray corrupt = readSettingsFile(path);
    QVERIFY(corrupt.contains(profile.networkId.toUtf8()));
    if (!corrupt.endsWith('\n'))
        corrupt.append('\n');
    // A missing '=' outside [networks] is invisible to childGroups() there.
    corrupt.append(QByteArrayLiteral("[preferences]\nnetworkOrder\n"));
    QVERIFY(writeSettingsFile(path, corrupt));

    QCOMPARE(store.remove(profile.networkId), IrcProfileStore::Status::FormatError);

    const QByteArray after = readSettingsFile(path);
    QCOMPARE(after, corrupt);
    QVERIFY(after.contains(profile.networkId.toUtf8()));
    QVERIFY(after.contains(QByteArrayLiteral("networkOrder")));
}

void ProfileTest::storeSaveMissingEqualsReturnsFormatError()
{
    IrcNetworkProfile profile = IrcNetworkProfile::create();
    profile.host = QStringLiteral("irc.example.net");
    profile.nick = QStringLiteral("omairc");

    IrcProfileStore store;
    QCOMPARE(store.save(profile), IrcProfileStore::Status::Written);

    const QByteArray corrupt = QByteArrayLiteral("[networks]\n")
        + profile.networkId.toUtf8()
        + QByteArrayLiteral("\\host\n");
    const QString path = settingsFile();
    QVERIFY(writeSettingsFile(path, corrupt));

    profile.host = QStringLiteral("irc.changed.example");
    QCOMPARE(store.save(profile), IrcProfileStore::Status::FormatError);
    QCOMPARE(readSettingsFile(path), corrupt);
}

void ProfileTest::storeSaveAfterRemoveMissingEqualsReturnsFormatError()
{
    IrcNetworkProfile profile = IrcNetworkProfile::create();
    profile.host = QStringLiteral("irc.example.net");
    profile.nick = QStringLiteral("omairc");

    IrcProfileStore store;
    QCOMPARE(store.save(profile), IrcProfileStore::Status::Written);

    const QByteArray corrupt = QByteArrayLiteral("[networks]\n")
        + profile.networkId.toUtf8()
        + QByteArrayLiteral("\\host\n");
    const QString path = settingsFile();
    QVERIFY(writeSettingsFile(path, corrupt));

    QCOMPARE(store.remove(profile.networkId), IrcProfileStore::Status::FormatError);
    QCOMPARE(readSettingsFile(path), corrupt);

    profile.host = QStringLiteral("irc.changed.example");
    QCOMPARE(store.save(profile), IrcProfileStore::Status::FormatError);
    QCOMPARE(readSettingsFile(path), corrupt);
}
#endif

int runProfileTests(int argc, char **argv)
{
    ProfileTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_profile.moc"
