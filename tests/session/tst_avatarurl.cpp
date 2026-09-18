#include <QCryptographicHash>
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
    void rejectsNonStandardPort();
    void rejectsIpv4MappedLoopback();
    void rejectsZeroNetwork();
    void rejectsTrailingDotLocalhost();
    void rejectsLinkLocal();
    void quantizesFetchPixelSize();
    void selectsSafeAddressAndPinsUrl();
    void metadataValueAcceptsHttpsUrl();
    void metadataValueBuildsGravatarFromEmail();
    void metadataValueStripsMailto();
    void metadataValueRejectsUnsafeInput();
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

void AvatarUrlTest::rejectsNonStandardPort()
{
    QVERIFY(!ircAvatarUrlIsSafe(
        ircResolvedAvatarUrl(QStringLiteral("https://example.com:8443/a.png"), 32)));
    QVERIFY(ircAvatarUrlIsSafe(
        ircResolvedAvatarUrl(QStringLiteral("https://example.com/a.png"), 32)));
}

void AvatarUrlTest::rejectsIpv4MappedLoopback()
{
    QVERIFY(!ircAvatarUrlIsSafe(
        ircResolvedAvatarUrl(QStringLiteral("https://[::ffff:127.0.0.1]/a.png"), 32)));
    QVERIFY(ircHostAddressIsUnsafe(
        QHostAddress(QStringLiteral("::ffff:127.0.0.1"))));
}

void AvatarUrlTest::rejectsZeroNetwork()
{
    QVERIFY(!ircAvatarUrlIsSafe(
        ircResolvedAvatarUrl(QStringLiteral("https://0.0.0.0/a.png"), 32)));
    QVERIFY(ircHostAddressIsUnsafe(QHostAddress(QStringLiteral("0.0.0.0"))));
    QVERIFY(ircHostAddressIsUnsafe(QHostAddress(QStringLiteral("0.1.2.3"))));
}

void AvatarUrlTest::rejectsTrailingDotLocalhost()
{
    QVERIFY(!ircAvatarUrlIsSafe(
        ircResolvedAvatarUrl(QStringLiteral("https://localhost./a.png"), 32)));
    QVERIFY(ircAvatarUrlIsSafe(
        ircResolvedAvatarUrl(QStringLiteral("https://example.com./a.png"), 32)));
}

void AvatarUrlTest::rejectsLinkLocal()
{
    QVERIFY(!ircAvatarUrlIsSafe(
        ircResolvedAvatarUrl(QStringLiteral("https://169.254.12.34/a.png"), 32)));
    QVERIFY(ircHostAddressIsUnsafe(QHostAddress(QStringLiteral("169.254.12.34"))));
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

void AvatarUrlTest::selectsSafeAddressAndPinsUrl()
{
    QCOMPARE(ircSelectSafeAvatarAddress({}), QHostAddress());
    QCOMPARE(ircSelectSafeAvatarAddress({QHostAddress(QStringLiteral("127.0.0.1"))}),
             QHostAddress());
    QCOMPARE(ircSelectSafeAvatarAddress(
                 {QHostAddress(QStringLiteral("10.0.0.1")),
                  QHostAddress(QStringLiteral("93.184.216.34"))}),
             QHostAddress(QStringLiteral("93.184.216.34")));

    const QUrl hostUrl(QStringLiteral("https://cdn.example/a.png"));
    const QUrl pinned = ircAvatarUrlPinnedToAddress(
        hostUrl, QHostAddress(QStringLiteral("93.184.216.34")));
    QCOMPARE(pinned, QUrl(QStringLiteral("https://93.184.216.34/a.png")));
    QVERIFY(ircAvatarUrlIsSafe(pinned));

    QVERIFY(!ircAvatarUrlPinnedToAddress(hostUrl, QHostAddress()).isValid());
}

void AvatarUrlTest::metadataValueAcceptsHttpsUrl()
{
    const QString url = QStringLiteral("https://cdn.example.com/a.png");
    QCOMPARE(ircAvatarMetadataValue(url), url);
    QCOMPARE(ircAvatarMetadataValue(QStringLiteral("  %1  ").arg(url)), url);
}

void AvatarUrlTest::metadataValueBuildsGravatarFromEmail()
{
    const QByteArray hash =
        QCryptographicHash::hash(QByteArrayLiteral("me@example.com"),
                                 QCryptographicHash::Sha256)
            .toHex();
    const QString expected =
        QStringLiteral("https://www.gravatar.com/avatar/%1?s={size}&d=404")
            .arg(QString::fromLatin1(hash));
    QCOMPARE(ircAvatarMetadataValue(QStringLiteral("Me@Example.COM")), expected);
}

void AvatarUrlTest::metadataValueStripsMailto()
{
    const QByteArray hash =
        QCryptographicHash::hash(QByteArrayLiteral("me@example.com"),
                                 QCryptographicHash::Sha256)
            .toHex();
    const QString expected =
        QStringLiteral("https://www.gravatar.com/avatar/%1?s={size}&d=404")
            .arg(QString::fromLatin1(hash));
    QCOMPARE(ircAvatarMetadataValue(QStringLiteral("mailto:Me@Example.COM")), expected);
}

void AvatarUrlTest::metadataValueRejectsUnsafeInput()
{
    QVERIFY(ircAvatarMetadataValue(QString()).isEmpty());
    QVERIFY(ircAvatarMetadataValue(QStringLiteral("   ")).isEmpty());
    QVERIFY(ircAvatarMetadataValue(QStringLiteral("http://example.com/a.png")).isEmpty());
    QVERIFY(ircAvatarMetadataValue(QStringLiteral("https://localhost/a.png")).isEmpty());
    QVERIFY(ircAvatarMetadataValue(QStringLiteral("not-an-email")).isEmpty());
    QVERIFY(ircAvatarMetadataValue(QStringLiteral("missing-domain@host")).isEmpty());
}

int runAvatarUrlTests(int argc, char **argv)
{
    AvatarUrlTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_avatarurl.moc"
