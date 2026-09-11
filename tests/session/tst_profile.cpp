#include <QCoreApplication>
#include <QFile>
#include <QSettings>
#include <QTemporaryDir>
#include <QTest>

#include "ircnetworkprofile.h"
#include "ircprofilestore.h"

#include <memory>

class ProfileTest : public QObject
{
    Q_OBJECT

private slots:
    void init();
    void suggestedPrefillsLiberachat();
    void validateRefusesIncompleteAndUnsendable();
    void validateRefusesUnsendableAccountAndBouncerNetwork();
    void saslAccountFallsBackToNickAndAppendsBouncerNetwork();
    void storeRoundTripsFieldsWithoutPassword();
    void missingConnectOnStartupDefaultsToFalse();
    void missingSecretSavedDefaultsToFalse();
    void missingNickServSavedDefaultsToFalse();
    void usernameAndRealnameStayAsTyped();
    void storeRemoveDropsTheNetworkGroup();

private:
    QString settingsFile() const;

    std::unique_ptr<QTemporaryDir> m_dir;
};

void ProfileTest::init()
{
    m_dir = std::make_unique<QTemporaryDir>();
    QVERIFY(m_dir->isValid());
    qputenv("XDG_CONFIG_HOME", m_dir->path().toUtf8());
    QSettings::setPath(QSettings::NativeFormat, QSettings::UserScope, m_dir->path());
    QCoreApplication::setOrganizationName(QStringLiteral("omairc"));
    QCoreApplication::setApplicationName(QStringLiteral("omairc"));
}

QString ProfileTest::settingsFile() const
{
    QSettings settings;
    return settings.fileName();
}

void ProfileTest::suggestedPrefillsLiberachat()
{
    const IrcNetworkProfile profile = IrcNetworkProfile::suggested();
    QVERIFY(!profile.networkId.isEmpty());
    QCOMPARE(profile.host, QStringLiteral("irc.libera.chat"));
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

    IrcProfileStore store;
    store.save(profile);

    const QList<IrcNetworkProfile> loaded = IrcProfileStore().profiles();
    QCOMPARE(loaded.size(), 1);
    QCOMPARE(loaded.first().networkId, profile.networkId);
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
    QCOMPARE(loaded.first(), profile);
    QCOMPARE(loaded.first().saslAccount(), QStringLiteral("joe/libera"));

    QFile file(settingsFile());
    QVERIFY(file.exists());
    QVERIFY(settingsFile().endsWith(QStringLiteral("/omairc/omairc.conf")));
    QVERIFY(file.open(QIODevice::ReadOnly | QIODevice::Text));
    const QString contents = QString::fromUtf8(file.readAll());
    QVERIFY(contents.contains(profile.networkId));
    QVERIFY(contents.contains(profile.host));
    QVERIFY(contents.contains(QLatin1String("networks")));
    QVERIFY(contents.contains(QLatin1String("secretSaved")));
    QVERIFY(contents.contains(QLatin1String("nickServSaved")));
    QVERIFY(contents.contains(QLatin1String("account=joe")));
    QVERIFY(contents.contains(QLatin1String("bouncerNetwork=libera")));
    QVERIFY(!contents.contains(QLatin1String("password"), Qt::CaseInsensitive));
    QVERIFY(!contents.contains(QLatin1String("nick-secret")));

    IrcNetworkProfile unnamed;
    unnamed.host = QStringLiteral("irc.example.net");
    unnamed.nick = QStringLiteral("omairc");
    store.save(unnamed);
    QCOMPARE(IrcProfileStore().profiles().size(), 1);
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

    QVERIFY(store.remove(drop.networkId));
    QVERIFY(!store.remove(drop.networkId));

    const QList<IrcNetworkProfile> loaded = IrcProfileStore().profiles();
    QCOMPARE(loaded.size(), 1);
    QCOMPARE(loaded.first().networkId, keep.networkId);
    QCOMPARE(loaded.first().host, QStringLiteral("irc.example.net"));

    QFile file(settingsFile());
    QVERIFY(file.open(QIODevice::ReadOnly | QIODevice::Text));
    const QString contents = QString::fromUtf8(file.readAll());
    QVERIFY(contents.contains(keep.networkId));
    QVERIFY(!contents.contains(drop.networkId));
    QVERIFY(!contents.contains(QLatin1String("irc.oftc.net")));
}

int runProfileTests(int argc, char **argv)
{
    ProfileTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_profile.moc"
