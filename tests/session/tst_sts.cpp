#include <QCoreApplication>
#include <QDateTime>
#include <QFile>
#include <QFileDevice>
#include <QSettings>
#include <QTemporaryDir>
#include <QTest>

#include "ircsts.h"

#include <memory>

class StsTest : public QObject
{
    Q_OBJECT

private slots:
    void init();
    void parseReadsPortAndDurationAndIgnoresUnknownKeys();
    void parseRejectsInvalidPortAndNegativeDuration();
    void parseTakesTheLastStsToken();
    void storeRoundTripsHostCaseAndOwnerOnlyFile();
    void storeKeepsHostsThatOnlyDifferByUnsafeCharacters();
    void storeDurationZeroDeletesTheEntry();
    void storeLookupDropsExpiredEntries();

private:
    std::unique_ptr<QTemporaryDir> m_dir;
};

void StsTest::init()
{
    m_dir = std::make_unique<QTemporaryDir>();
    QVERIFY(m_dir->isValid());
    qputenv("XDG_CONFIG_HOME", m_dir->path().toUtf8());
}

void StsTest::parseReadsPortAndDurationAndIgnoresUnknownKeys()
{
    const auto advertisement = parseIrcStsAdvertisement(
        QStringList{QStringLiteral("multi-prefix"),
                    QStringLiteral("sts=unknown,port=6697,duration=60,foo=bar")});
    QVERIFY(advertisement);
    QCOMPARE(*advertisement->port, quint16(6697));
    QCOMPARE(*advertisement->durationSeconds, qint64(60));
}

void StsTest::parseRejectsInvalidPortAndNegativeDuration()
{
    const auto badPort = parseIrcStsAdvertisement(
        QStringList{QStringLiteral("sts=port=0,duration=60")});
    QVERIFY(badPort);
    QVERIFY(!badPort->port);
    QCOMPARE(*badPort->durationSeconds, qint64(60));

    const auto missingPort = parseIrcStsAdvertisement(
        QStringList{QStringLiteral("sts=duration=86400")});
    QVERIFY(missingPort);
    QVERIFY(!missingPort->port);
    QCOMPARE(*missingPort->durationSeconds, qint64(86400));

    const auto negative = parseIrcStsAdvertisement(
        QStringList{QStringLiteral("sts=port=6697,duration=-1")});
    QVERIFY(negative);
    QCOMPARE(*negative->port, quint16(6697));
    QVERIFY(!negative->durationSeconds);

    QVERIFY(!parseIrcStsAdvertisement(
        QStringList{QStringLiteral("multi-prefix"), QStringLiteral("sasl=PLAIN")}));
}

void StsTest::parseTakesTheLastStsToken()
{
    const auto advertisement = parseIrcStsAdvertisement(
        QStringList{QStringLiteral("sts=port=6667,duration=1"),
                    QStringLiteral("sts=port=6697,duration=60")});
    QVERIFY(advertisement);
    QCOMPARE(*advertisement->port, quint16(6697));
    QCOMPARE(*advertisement->durationSeconds, qint64(60));
}

void StsTest::storeRoundTripsHostCaseAndOwnerOnlyFile()
{
    IrcStsStore store;
    store.save(QStringLiteral("IRC.Example"), 6697, 60);

    const std::optional<IrcStsCached> cached = store.lookup(QStringLiteral("irc.example"));
    QVERIFY(cached);
    QCOMPARE(cached->port, quint16(6697));
    QCOMPARE(cached->durationSeconds, qint64(60));
    QVERIFY(cached->expiryEpochSeconds > QDateTime::currentSecsSinceEpoch());

    QCOMPARE(store.filePath(), m_dir->path() + QLatin1String("/omairc/sts"));
    QFile file(store.filePath());
    QVERIFY(file.exists());
    const QFileDevice::Permissions permissions = file.permissions();
    QVERIFY(permissions & QFileDevice::ReadOwner);
    QVERIFY(permissions & QFileDevice::WriteOwner);
    QVERIFY(!(permissions & QFileDevice::ReadGroup));
    QVERIFY(!(permissions & QFileDevice::ReadOther));
    QVERIFY(!(permissions & QFileDevice::WriteGroup));
    QVERIFY(!(permissions & QFileDevice::WriteOther));
}

void StsTest::storeKeepsHostsThatOnlyDifferByUnsafeCharacters()
{
    IrcStsStore store;
    store.save(QStringLiteral("a/b"), 6697, 60);
    store.save(QStringLiteral("a_b"), 6698, 60);
    const std::optional<IrcStsCached> slash = store.lookup(QStringLiteral("a/b"));
    const std::optional<IrcStsCached> underscore = store.lookup(QStringLiteral("a_b"));
    QVERIFY(slash);
    QVERIFY(underscore);
    QCOMPARE(slash->port, quint16(6697));
    QCOMPARE(underscore->port, quint16(6698));
}

void StsTest::storeDurationZeroDeletesTheEntry()
{
    IrcStsStore store;
    store.save(QStringLiteral("irc.example"), 6697, 60);
    QVERIFY(store.lookup(QStringLiteral("irc.example")));
    store.save(QStringLiteral("irc.example"), 6697, 0);
    QVERIFY(!store.lookup(QStringLiteral("irc.example")));
}

void StsTest::storeLookupDropsExpiredEntries()
{
    IrcStsStore store;
    store.save(QStringLiteral("irc.example"), 6697, 60);

    QSettings settings(store.filePath(), QSettings::IniFormat);
    const QStringList groups = settings.childGroups();
    QCOMPARE(groups.size(), 1);
    settings.beginGroup(groups.first());
    settings.setValue(QStringLiteral("expiry"), 1);
    settings.endGroup();
    settings.sync();

    QVERIFY(!store.lookup(QStringLiteral("irc.example")));
}

int runStsTests(int argc, char **argv)
{
    StsTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_sts.moc"
