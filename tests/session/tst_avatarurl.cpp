#include <QHostAddress>
#include <QTest>
#include <QUrl>

#include "ircavatarurl.h"

class AvatarUrlTest : public QObject
{
    Q_OBJECT

private slots:
    void substitutesSizePlaceholder();
    void acceptsHttps();
    void rejectsHttp();
    void rejectsLocalhost();
    void rejectsLoopbackIp();
    void rejectsPrivateTen();
    void rejectsFile();
    void rejectsEmpty();
    void rejectsUserinfoAndLocalSuffix();
    void quantizesFetchPixelSize();
};

void AvatarUrlTest::substitutesSizePlaceholder()
{
    const QUrl sized = ircResolvedAvatarUrl(
        QStringLiteral("https://example.com/avatars/{size}/mira.png"), 34);
    QCOMPARE(sized, QUrl(QStringLiteral("https://example.com/avatars/34/mira.png")));
    QVERIFY(ircAvatarUrlIsSafe(sized));

    const QUrl floor = ircResolvedAvatarUrl(
        QStringLiteral("https://example.com/avatars/{size}/mira.png"), 8);
    QCOMPARE(floor, QUrl(QStringLiteral("https://example.com/avatars/16/mira.png")));
    QVERIFY(ircAvatarUrlIsSafe(floor));
}

void AvatarUrlTest::acceptsHttps()
{
    QVERIFY(ircAvatarUrlIsSafe(
        ircResolvedAvatarUrl(QStringLiteral("https://cdn.example.com/a.png"), 32)));
}

void AvatarUrlTest::rejectsHttp()
{
    QVERIFY(!ircAvatarUrlIsSafe(
        ircResolvedAvatarUrl(QStringLiteral("http://example.com/a.png"), 32)));
}

void AvatarUrlTest::rejectsLocalhost()
{
    QVERIFY(!ircAvatarUrlIsSafe(
        ircResolvedAvatarUrl(QStringLiteral("https://localhost/a.png"), 32)));
    QVERIFY(!ircAvatarUrlIsSafe(
        ircResolvedAvatarUrl(QStringLiteral("https://LOCALHOST/a.png"), 32)));
}

void AvatarUrlTest::rejectsLoopbackIp()
{
    QVERIFY(!ircAvatarUrlIsSafe(
        ircResolvedAvatarUrl(QStringLiteral("https://127.0.0.1/a.png"), 32)));
    QVERIFY(!ircAvatarUrlIsSafe(
        ircResolvedAvatarUrl(QStringLiteral("https://[::1]/a.png"), 32)));
}

void AvatarUrlTest::rejectsPrivateTen()
{
    QVERIFY(!ircAvatarUrlIsSafe(
        ircResolvedAvatarUrl(QStringLiteral("https://10.0.0.4/a.png"), 32)));
    QVERIFY(!ircAvatarUrlIsSafe(
        ircResolvedAvatarUrl(QStringLiteral("https://192.168.1.10/a.png"), 32)));
}

void AvatarUrlTest::rejectsFile()
{
    QVERIFY(!ircAvatarUrlIsSafe(
        ircResolvedAvatarUrl(QStringLiteral("file:///tmp/a.png"), 32)));
    QVERIFY(!ircAvatarUrlIsSafe(
        ircResolvedAvatarUrl(QStringLiteral("data:image/png;base64,aaaa"), 32)));
}

void AvatarUrlTest::rejectsEmpty()
{
    QVERIFY(ircResolvedAvatarUrl(QString(), 32).isEmpty());
    QVERIFY(!ircAvatarUrlIsSafe(QUrl()));
    QVERIFY(!ircAvatarUrlIsSafe(ircResolvedAvatarUrl(QStringLiteral("   "), 32)));
}

void AvatarUrlTest::rejectsUserinfoAndLocalSuffix()
{
    QVERIFY(!ircAvatarUrlIsSafe(
        ircResolvedAvatarUrl(QStringLiteral("https://user:pass@example.com/a.png"), 32)));
    QVERIFY(!ircAvatarUrlIsSafe(
        ircResolvedAvatarUrl(QStringLiteral("https://printer.local/a.png"), 32)));
    QVERIFY(ircHostAddressIsUnsafe(QHostAddress(QStringLiteral("100.64.0.1"))));
}

void AvatarUrlTest::quantizesFetchPixelSize()
{
    QCOMPARE(ircAvatarFetchPixelSize(0), 0);
    QCOMPARE(ircAvatarFetchPixelSize(-1), 0);
    QCOMPARE(ircAvatarFetchPixelSize(22), 32);
    QCOMPARE(ircAvatarFetchPixelSize(34), 32);
    QCOMPARE(ircAvatarFetchPixelSize(48), 32);
    QCOMPARE(ircAvatarFetchPixelSize(49), 64);
    QCOMPARE(ircAvatarFetchPixelSize(64), 64);
}

int runAvatarUrlTests(int argc, char **argv)
{
    AvatarUrlTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_avatarurl.moc"
