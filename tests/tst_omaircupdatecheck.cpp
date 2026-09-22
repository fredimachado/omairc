#include <QByteArray>
#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QTest>
#include <QTimer>
#include <QUrl>

#include "omaircupdatecheck.h"
#include "omaircversion.h"

namespace {

class MockNetworkReply : public QNetworkReply
{
public:
    MockNetworkReply(const QNetworkRequest &request, QObject *parent = nullptr)
        : QNetworkReply(parent)
    {
        setRequest(request);
        setUrl(request.url());
        open(QIODevice::ReadOnly);
    }

    void deliver(int statusCode, const QByteArray &body,
                 QNetworkReply::NetworkError error = QNetworkReply::NoError,
                 const QUrl &reportedUrl = QUrl())
    {
        setAttribute(QNetworkRequest::HttpStatusCodeAttribute, statusCode);
        setHeader(QNetworkRequest::ContentTypeHeader,
                  QByteArrayLiteral("application/octet-stream"));
        if (reportedUrl.isValid())
            setUrl(reportedUrl);
        if (error != QNetworkReply::NoError)
            setError(error, QStringLiteral("mock"));
        m_body = body;
        QTimer::singleShot(0, this, [this]() {
            if (!m_body.isEmpty())
                emit readyRead();
            emit finished();
        });
    }

protected:
    qint64 readData(char *data, qint64 maxSize) override
    {
        if (m_offset >= m_body.size())
            return -1;
        const qint64 chunk = qMin(maxSize, m_body.size() - m_offset);
        memcpy(data, m_body.constData() + m_offset, chunk);
        m_offset += chunk;
        return chunk;
    }

    void abort() override
    {
        setError(QNetworkReply::OperationCanceledError,
                 QStringLiteral("Mock reply aborted"));
        QTimer::singleShot(0, this, [this]() { emit finished(); });
    }

private:
    QByteArray m_body;
    qint64 m_offset = 0;
};

class MockNetworkAccessManager : public QNetworkAccessManager
{
public:
    int statusCode = 200;
    QByteArray body = QByteArrayLiteral(
        "{\"tag_name\":\"v9.9.9\",\"html_url\":\"https://github.com/fredimachado/omairc/releases/tag/v9.9.9\"}");
    QNetworkReply::NetworkError error = QNetworkReply::NoError;
    QUrl reportedUrl;
    QNetworkRequest lastRequest;
    int requestCount = 0;

protected:
    QNetworkReply *createRequest(Operation op, const QNetworkRequest &request,
                                 QIODevice *outgoingData) override
    {
        Q_UNUSED(op);
        Q_UNUSED(outgoingData);
        lastRequest = request;
        requestCount += 1;
        auto *reply = new MockNetworkReply(request, this);
        reply->deliver(statusCode, body, error, reportedUrl);
        return reply;
    }
};

QByteArray releaseJson(const QByteArray &tag, const QByteArray &htmlUrl)
{
    return QByteArrayLiteral("{\"tag_name\":\"") + tag
        + QByteArrayLiteral("\",\"html_url\":\"") + htmlUrl
        + QByteArrayLiteral("\"}");
}

}

class OmaircUpdateCheckTest : public QObject
{
    Q_OBJECT

private slots:
    void githubRepoUrl();
    void releaseUrlSafety();
    void compareVersions();
    void parseRelease();
    void applyPayload();
    void checkUsesGithubLatestAndCompares();
    void checkNetworkErrorFailsClosed();
    void windowsSetupAsset();
    void installedCopyDownloadsVerifiedSetup();
    void downloadRejectsBadHash();
    void downloadRejectsOversizedBody();
    void downloadRejectsUnsafeRedirect();
    void cancelDoesNotAbortInstallerDownload();
    void downloadWithoutInstalledCopyDoesNothing();
};

void OmaircUpdateCheckTest::githubRepoUrl()
{
    QCOMPARE(omaircGithubRepoUrl(),
             QStringLiteral("https://github.com/fredimachado/omairc"));
    QCOMPARE(omaircGithubLatestReleaseApiUrl(),
             QUrl(QStringLiteral(
                 "https://api.github.com/repos/fredimachado/omairc/releases/latest")));
}

void OmaircUpdateCheckTest::releaseUrlSafety()
{
    QVERIFY(omaircGithubReleaseUrlIsSafe(
        QUrl(QStringLiteral("https://github.com/fredimachado/omairc"))));
    QVERIFY(omaircGithubReleaseUrlIsSafe(
        QUrl(QStringLiteral(
            "https://github.com/fredimachado/omairc/releases/tag/v0.7.0"))));
    QVERIFY(!omaircGithubReleaseUrlIsSafe(
        QUrl(QStringLiteral("http://github.com/fredimachado/omairc"))));
    QVERIFY(!omaircGithubReleaseUrlIsSafe(
        QUrl(QStringLiteral("https://evil.example/fredimachado/omairc"))));
    QVERIFY(!omaircGithubReleaseUrlIsSafe(
        QUrl(QStringLiteral("https://github.com/other/omairc"))));
    QVERIFY(!omaircGithubReleaseUrlIsSafe(QUrl()));
}

void OmaircUpdateCheckTest::compareVersions()
{
    QCOMPARE(omaircCompareVersions(QStringLiteral("0.7.0"),
                                   QStringLiteral("0.7.0")),
             0);
    QCOMPARE(omaircCompareVersions(QStringLiteral("v0.7.0"),
                                   QStringLiteral("0.7.0")),
             0);
    QVERIFY(omaircCompareVersions(QStringLiteral("0.7.0alpha"),
                                  QStringLiteral("0.7.0")) < 0);
    QVERIFY(omaircCompareVersions(QStringLiteral("0.7.0"),
                                  QStringLiteral("0.7.0alpha")) > 0);
    QVERIFY(omaircCompareVersions(QStringLiteral("0.7.0alpha"),
                                  QStringLiteral("0.7.0beta")) < 0);
    QVERIFY(omaircCompareVersions(QStringLiteral("0.7.0"),
                                  QStringLiteral("0.8.0")) < 0);
    QVERIFY(omaircCompareVersions(QStringLiteral("0.8.0"),
                                  QStringLiteral("0.7.9")) > 0);
    QVERIFY(omaircCompareVersions(QStringLiteral("0.7"),
                                  QStringLiteral("0.7.0")) == 0);
}

void OmaircUpdateCheckTest::parseRelease()
{
    const auto parsed = omaircParseGithubRelease(
        releaseJson("v0.8.0",
                    "https://github.com/fredimachado/omairc/releases/tag/v0.8.0"));
    QVERIFY(parsed.has_value());
    QCOMPARE(parsed->version, QStringLiteral("0.8.0"));
    QCOMPARE(parsed->tag, QStringLiteral("v0.8.0"));
    QCOMPARE(parsed->htmlUrl,
             QStringLiteral(
                 "https://github.com/fredimachado/omairc/releases/tag/v0.8.0"));
    QCOMPARE(parsed->setupUrl, QString());
    QCOMPARE(parsed->setupSize, 0);

    const auto fallback = omaircParseGithubRelease(
        QByteArrayLiteral("{\"tag_name\":\"v1.0.0\"}"));
    QVERIFY(fallback.has_value());
    QCOMPARE(fallback->htmlUrl,
             QStringLiteral("https://github.com/fredimachado/omairc/releases/latest"));

    QVERIFY(!omaircParseGithubRelease(QByteArrayLiteral("{}")));
    QVERIFY(!omaircParseGithubRelease(QByteArrayLiteral("not-json")));
    QVERIFY(!omaircParseGithubRelease(
        releaseJson("v0.8.0", "https://evil.example/releases/v0.8.0")));
}

void OmaircUpdateCheckTest::applyPayload()
{
    OmaircUpdateCheck checker;
    QCOMPARE(checker.status(), QStringLiteral("idle"));
    QCOMPARE(checker.currentVersion(), QStringLiteral(OMAIRC_VERSION));
    QCOMPARE(checker.repoUrl(), omaircGithubRepoUrl());

    checker.setCurrentVersion(QStringLiteral("0.1.0"));
    checker.applyGithubPayload(
        QString::fromUtf8(releaseJson(
            "v9.9.9",
            "https://github.com/fredimachado/omairc/releases/tag/v9.9.9")),
        200);
    QCOMPARE(checker.status(), QStringLiteral("updateAvailable"));
    QCOMPARE(checker.latestVersion(), QStringLiteral("9.9.9"));
    QCOMPARE(checker.message(), QStringLiteral("Version 9.9.9 is available."));
    QCOMPARE(checker.latestUrl(),
             QStringLiteral(
                 "https://github.com/fredimachado/omairc/releases/tag/v9.9.9"));

    checker.setCurrentVersion(QStringLiteral("9.9.9"));
    checker.applyGithubPayload(
        QString::fromUtf8(releaseJson(
            "v9.9.9",
            "https://github.com/fredimachado/omairc/releases/tag/v9.9.9")),
        200);
    QCOMPARE(checker.status(), QStringLiteral("upToDate"));
    QCOMPARE(checker.message(), QStringLiteral("Omairc is up to date."));

    checker.applyGithubPayload(QStringLiteral("{}"), 200);
    QCOMPARE(checker.status(), QStringLiteral("failed"));
    QCOMPARE(checker.message(), QStringLiteral("Could not check for updates."));

    checker.applyGithubPayload(QStringLiteral("{\"tag_name\":\"v1.0.0\"}"), 404);
    QCOMPARE(checker.status(), QStringLiteral("failed"));
}

void OmaircUpdateCheckTest::checkUsesGithubLatestAndCompares()
{
    MockNetworkAccessManager nam;
    OmaircUpdateCheck checker;
    checker.setCurrentVersion(QStringLiteral("0.1.0"));
    checker.setNetworkAccessManager(&nam);

    QSignalSpy statusSpy(&checker, &OmaircUpdateCheck::statusChanged);
    checker.check();
    QCOMPARE(checker.status(), QStringLiteral("checking"));
    QCOMPARE(checker.message(), QStringLiteral("Checking…"));
    QVERIFY(QTest::qWaitFor([&checker]() {
        return checker.status() == QStringLiteral("updateAvailable");
    }));
    QCOMPARE(nam.requestCount, 1);
    QCOMPARE(nam.lastRequest.url(), omaircGithubLatestReleaseApiUrl());
    QCOMPARE(nam.lastRequest.rawHeader("Accept"),
             QByteArrayLiteral("application/vnd.github+json"));
    QVERIFY(nam.lastRequest.header(QNetworkRequest::UserAgentHeader)
                .toString()
                .startsWith(QStringLiteral("Omairc/0.1.0")));
    QCOMPARE(checker.latestVersion(), QStringLiteral("9.9.9"));
    QVERIFY(statusSpy.count() >= 2);
}

QByteArray setupReleaseJson(const QByteArray &downloadUrl, const QByteArray &digest,
                            const QByteArray &size,
                            const QByteArray &name = QByteArrayLiteral(
                                "omairc-9.9.9-windows-x64-setup.exe"))
{
    return QByteArrayLiteral(
               "{\"tag_name\":\"v9.9.9\",\"html_url\":\"https://github.com/fredimachado/omairc/releases/tag/v9.9.9\",\"assets\":[{\"name\":\"")
        + name
        + QByteArrayLiteral("\",\"browser_download_url\":\"")
        + downloadUrl
        + QByteArrayLiteral("\",\"size\":")
        + size
        + QByteArrayLiteral(",\"digest\":\"")
        + digest
        + QByteArrayLiteral("\"}]}");
}

const auto kSetupBody = QByteArrayLiteral("omairc-setup");
const auto kSetupDigest = QByteArrayLiteral(
    "sha256:d323de13bbb0973b891849578f74c69cb4ef85fc37ffeb05b31d835f708c0ae9");
const auto kSetupUrl = QByteArrayLiteral(
    "https://github.com/fredimachado/omairc/releases/download/v9.9.9/omairc-9.9.9-windows-x64-setup.exe");

void OmaircUpdateCheckTest::windowsSetupAsset()
{
    QCOMPARE(omaircExpectedWindowsSetupName(QStringLiteral("9.9.9")),
             QStringLiteral("omairc-9.9.9-windows-x64-setup.exe"));
    const QUrl setupUrl(QString::fromUtf8(kSetupUrl));
    QVERIFY(omaircWindowsSetupRequestIsSafe(setupUrl, QStringLiteral("v9.9.9"),
                                            QStringLiteral("9.9.9")));
    QVERIFY(!omaircWindowsSetupRequestIsSafe(
        QUrl(QStringLiteral("https://evil.example/omairc-9.9.9-windows-x64-setup.exe")),
        QStringLiteral("v9.9.9"), QStringLiteral("9.9.9")));
    QVERIFY(!omaircWindowsSetupRequestIsSafe(
        QUrl(QString::fromUtf8(kSetupUrl + "?raw=1")),
        QStringLiteral("v9.9.9"), QStringLiteral("9.9.9")));
    QVERIFY(omaircWindowsSetupRedirectIsSafe(
        QUrl(QStringLiteral("https://objects.githubusercontent.com/releases/setup.exe"))));
    QVERIFY(!omaircWindowsSetupRedirectIsSafe(
        QUrl(QStringLiteral("https://evil.example/setup.exe"))));
    QVERIFY(omaircInstallLocationMatches(
        QStringLiteral("C:/Users/Fredi/AppData/Local/Programs/Omairc/"),
        QStringLiteral("C:/Users/Fredi/AppData/Local/Programs/Omairc")));
    QVERIFY(!omaircInstallLocationMatches(QStringLiteral("C:/elsewhere"),
                                          QStringLiteral("C:/Omairc")));

    const auto parsed = omaircParseGithubRelease(
        setupReleaseJson(kSetupUrl, kSetupDigest, QByteArrayLiteral("12")));
    QVERIFY(parsed.has_value());
    QCOMPARE(parsed->setupUrl, QString::fromUtf8(kSetupUrl));
    QCOMPARE(parsed->setupSha256,
             QStringLiteral(
                 "d323de13bbb0973b891849578f74c69cb4ef85fc37ffeb05b31d835f708c0ae9"));
    QCOMPARE(parsed->setupSize, 12);

    const auto unsafe = omaircParseGithubRelease(setupReleaseJson(
        QByteArrayLiteral("https://evil.example/omairc-9.9.9-windows-x64-setup.exe"),
        kSetupDigest, QByteArrayLiteral("12")));
    QVERIFY(unsafe.has_value());
    QCOMPARE(unsafe->setupUrl, QString());

    const auto missingDigest = omaircParseGithubRelease(setupReleaseJson(
        kSetupUrl, QByteArrayLiteral("md5:abcd"), QByteArrayLiteral("12")));
    QVERIFY(missingDigest.has_value());
    QCOMPARE(missingDigest->setupUrl, QString());

    QByteArray duplicated = setupReleaseJson(kSetupUrl, kSetupDigest, QByteArrayLiteral("12"));
    duplicated.insert(duplicated.lastIndexOf(']'),
                      QByteArrayLiteral(
                          ",{\"name\":\"omairc-9.9.9-windows-x64-setup.exe\",\"browser_download_url\":\"")
                          + kSetupUrl
                          + QByteArrayLiteral("\",\"size\":12,\"digest\":\"")
                          + kSetupDigest
                          + QByteArrayLiteral("\"}"));
    const auto duplicate = omaircParseGithubRelease(duplicated);
    QVERIFY(duplicate.has_value());
    QCOMPARE(duplicate->setupUrl, QString());
}

void OmaircUpdateCheckTest::installedCopyDownloadsVerifiedSetup()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    MockNetworkAccessManager nam;
    nam.body = kSetupBody;
    OmaircUpdateCheck checker;
    checker.setCurrentVersion(QStringLiteral("0.1.0"));
    checker.setInstalledCopy(true);
    checker.setUpdatesDirectory(dir.path());
    checker.setNetworkAccessManager(&nam);
    checker.applyGithubPayload(
        QString::fromUtf8(setupReleaseJson(kSetupUrl, kSetupDigest, QByteArrayLiteral("12"))),
        200);
    QVERIFY(checker.installerUpdateAvailable());
    QCOMPARE(checker.status(), QStringLiteral("updateAvailable"));

    checker.download();
    QCOMPARE(checker.status(), QStringLiteral("downloading"));
    QVERIFY(QTest::qWaitFor([&checker]() {
        return checker.status() == QStringLiteral("readyToRestart");
    }));
    QCOMPARE(checker.message(), QStringLiteral("Restart to update"));
    QCOMPARE(nam.requestCount, 1);
    QCOMPARE(nam.lastRequest.url(), QUrl(QString::fromUtf8(kSetupUrl)));
    const QString path = checker.installerPath();
    QVERIFY(path.endsWith(QStringLiteral("omairc-9.9.9-windows-x64-setup.exe")));
    QVERIFY(QFile::exists(path));
    QFile file(path);
    QVERIFY(file.open(QIODevice::ReadOnly));
    QCOMPARE(file.readAll(), kSetupBody);

    checker.suppressInstallerLaunch();
    QVERIFY(checker.launchInstaller());
    QCOMPARE(checker.launchAttempts(), 1);
    QCOMPARE(checker.status(), QStringLiteral("readyToRestart"));
}

void OmaircUpdateCheckTest::downloadRejectsBadHash()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    MockNetworkAccessManager nam;
    nam.body = QByteArrayLiteral("nope");
    OmaircUpdateCheck checker;
    checker.setCurrentVersion(QStringLiteral("0.1.0"));
    checker.setInstalledCopy(true);
    checker.setUpdatesDirectory(dir.path());
    checker.setNetworkAccessManager(&nam);
    checker.applyGithubPayload(
        QString::fromUtf8(setupReleaseJson(kSetupUrl, kSetupDigest, QByteArrayLiteral("4"))),
        200);
    checker.download();
    QVERIFY(QTest::qWaitFor([&checker]() {
        return checker.status() == QStringLiteral("downloadFailed");
    }));
    QCOMPARE(checker.message(), QStringLiteral("Could not download the update."));
    const QString setupPath = QDir(dir.path()).filePath(
        QStringLiteral("omairc-9.9.9-windows-x64-setup.exe"));
    QVERIFY(!QFile::exists(setupPath));
    QVERIFY(!QFile::exists(setupPath + QStringLiteral(".partial")));
    QCOMPARE(checker.installerPath(), QString());
}

void OmaircUpdateCheckTest::downloadRejectsOversizedBody()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    MockNetworkAccessManager nam;
    nam.body = kSetupBody;
    OmaircUpdateCheck checker;
    checker.setCurrentVersion(QStringLiteral("0.1.0"));
    checker.setInstalledCopy(true);
    checker.setUpdatesDirectory(dir.path());
    checker.setNetworkAccessManager(&nam);
    checker.applyGithubPayload(
        QString::fromUtf8(setupReleaseJson(kSetupUrl, kSetupDigest, QByteArrayLiteral("4"))),
        200);
    checker.download();
    QVERIFY(QTest::qWaitFor([&checker]() {
        return checker.status() == QStringLiteral("downloadFailed");
    }));
    QVERIFY(!QFile::exists(QDir(dir.path()).filePath(
        QStringLiteral("omairc-9.9.9-windows-x64-setup.exe"))));
}

void OmaircUpdateCheckTest::downloadRejectsUnsafeRedirect()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    MockNetworkAccessManager nam;
    nam.body = kSetupBody;
    nam.reportedUrl = QUrl(QStringLiteral("https://evil.example/setup.exe"));
    OmaircUpdateCheck checker;
    checker.setCurrentVersion(QStringLiteral("0.1.0"));
    checker.setInstalledCopy(true);
    checker.setUpdatesDirectory(dir.path());
    checker.setNetworkAccessManager(&nam);
    checker.applyGithubPayload(
        QString::fromUtf8(setupReleaseJson(kSetupUrl, kSetupDigest, QByteArrayLiteral("12"))),
        200);
    checker.download();
    QVERIFY(QTest::qWaitFor([&checker]() {
        return checker.status() == QStringLiteral("downloadFailed");
    }));
    QVERIFY(!QFile::exists(checker.installerPath()));
}

void OmaircUpdateCheckTest::cancelDoesNotAbortInstallerDownload()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    MockNetworkAccessManager nam;
    nam.body = kSetupBody;
    OmaircUpdateCheck checker;
    checker.setCurrentVersion(QStringLiteral("0.1.0"));
    checker.setInstalledCopy(true);
    checker.setUpdatesDirectory(dir.path());
    checker.setNetworkAccessManager(&nam);
    checker.applyGithubPayload(
        QString::fromUtf8(setupReleaseJson(kSetupUrl, kSetupDigest, QByteArrayLiteral("12"))),
        200);
    checker.download();
    QCOMPARE(checker.status(), QStringLiteral("downloading"));
    checker.cancel();
    QCOMPARE(checker.status(), QStringLiteral("downloading"));
    QVERIFY(QTest::qWaitFor([&checker]() {
        return checker.status() == QStringLiteral("readyToRestart");
    }));
}

void OmaircUpdateCheckTest::downloadWithoutInstalledCopyDoesNothing()
{
    OmaircUpdateCheck checker;
    checker.setCurrentVersion(QStringLiteral("0.1.0"));
    checker.setInstalledCopy(false);
    checker.applyGithubPayload(
        QString::fromUtf8(setupReleaseJson(kSetupUrl, kSetupDigest, QByteArrayLiteral("12"))),
        200);
    QVERIFY(!checker.installerUpdateAvailable());
    checker.download();
    QCOMPARE(checker.status(), QStringLiteral("updateAvailable"));
    QCOMPARE(checker.launchAttempts(), 0);
    QVERIFY(!checker.launchInstaller());
}

void OmaircUpdateCheckTest::checkNetworkErrorFailsClosed()
{
    MockNetworkAccessManager nam;
    nam.statusCode = 0;
    nam.body.clear();
    nam.error = QNetworkReply::HostNotFoundError;
    OmaircUpdateCheck checker;
    checker.setNetworkAccessManager(&nam);
    checker.check();
    QVERIFY(QTest::qWaitFor([&checker]() {
        return checker.status() == QStringLiteral("failed");
    }));
}

int runOmaircUpdateCheckTests(int argc, char **argv)
{
    OmaircUpdateCheckTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_omaircupdatecheck.moc"
