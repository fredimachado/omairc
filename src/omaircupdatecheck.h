#pragma once

#include <QByteArray>
#include <QCryptographicHash>
#include <QFile>
#include <QObject>
#include <QPointer>
#include <QString>
#include <QTemporaryDir>
#include <QUrl>

#include <memory>
#include <optional>

class QNetworkAccessManager;
class QNetworkReply;

struct OmaircGithubRelease {
    QString version;
    QString tag;
    QString htmlUrl;
    QString setupUrl;
    QString setupSha256;
    qint64 setupSize = 0;
};

QString omaircGithubRepoUrl();
QUrl omaircGithubLatestReleaseApiUrl();
bool omaircGithubReleaseUrlIsSafe(const QUrl &url);
QString omaircExpectedWindowsSetupName(const QString &version);
bool omaircWindowsSetupRequestIsSafe(const QUrl &url, const QString &tag,
                                      const QString &version);
bool omaircWindowsSetupRedirectIsSafe(const QUrl &url);
bool omaircInstallLocationMatches(const QString &installLocation, const QString &appDir);
int omaircCompareVersions(const QString &left, const QString &right);
std::optional<OmaircGithubRelease> omaircParseGithubRelease(const QByteArray &json);
void omaircRegisterUpdateCheck();

class OmaircUpdateCheck : public QObject {
    Q_OBJECT
    Q_PROPERTY(QString status READ status NOTIFY statusChanged)
    Q_PROPERTY(QString message READ message NOTIFY messageChanged)
    Q_PROPERTY(QString latestVersion READ latestVersion NOTIFY latestChanged)
    Q_PROPERTY(QString latestUrl READ latestUrl NOTIFY latestChanged)
    Q_PROPERTY(QString currentVersion READ currentVersion WRITE setCurrentVersion
               NOTIFY currentVersionChanged)
    Q_PROPERTY(QString repoUrl READ repoUrl CONSTANT)
    Q_PROPERTY(bool installedCopy READ installedCopy WRITE setInstalledCopy
               NOTIFY installedCopyChanged)
    Q_PROPERTY(bool installerUpdateAvailable READ installerUpdateAvailable
               NOTIFY installerUpdateAvailableChanged)
    Q_PROPERTY(QString installerPath READ installerPath NOTIFY installerPathChanged)
    Q_PROPERTY(int launchAttempts READ launchAttempts NOTIFY launchAttemptsChanged)

public:
    explicit OmaircUpdateCheck(QObject *parent = nullptr);
    ~OmaircUpdateCheck() override;

    QString status() const { return m_status; }
    QString message() const;
    QString latestVersion() const { return m_latestVersion; }
    QString latestUrl() const { return m_latestUrl; }
    QString currentVersion() const { return m_currentVersion; }
    void setCurrentVersion(const QString &version);
    QString repoUrl() const { return omaircGithubRepoUrl(); }
    bool installedCopy() const { return m_installedCopy; }
    void setInstalledCopy(bool installed);
    bool installerUpdateAvailable() const;
    QString installerPath() const { return m_installerPath; }
    int launchAttempts() const { return m_launchAttempts; }

    void setNetworkAccessManager(QNetworkAccessManager *nam);
    void setUpdatesDirectory(const QString &directory);

    Q_INVOKABLE void check();
    Q_INVOKABLE void cancel();
    Q_INVOKABLE void download();
    Q_INVOKABLE bool launchInstaller();
    Q_INVOKABLE void applyGithubPayload(const QString &body, int httpStatus);
    Q_INVOKABLE void stageInstallerDownload(const QString &body);
    Q_INVOKABLE void suppressInstallerLaunch();

signals:
    void statusChanged();
    void messageChanged();
    void latestChanged();
    void currentVersionChanged();
    void installedCopyChanged();
    void installerUpdateAvailableChanged();
    void installerPathChanged();
    void launchAttemptsChanged();

private:
    QNetworkAccessManager *ensureNam();
    void abortReply();
    void setStatus(const QString &status);
    void finishFailed();
    void finishDownloadFailed();
    void finishWithBody(const QByteArray &body, int httpStatus);
    void forgetDownloadedInstaller();
    void clearSetup();
    void publishInstallerAvailability();
    QString updatesDirectory() const;
    bool beginPartial();
    void discardPartial();
    bool acceptChunk(const QByteArray &chunk);
    void commitDownloadedInstaller(const QUrl &finalUrl);
    void startNetworkDownload();
    bool downloadedInstallerMatches() const;

    QString m_status = QStringLiteral("idle");
    QString m_currentVersion;
    QString m_latestVersion;
    QString m_latestUrl;
    QString m_setupUrl;
    QString m_setupSha256;
    qint64 m_setupSize = 0;
    bool m_installedCopy = false;
    bool m_publishedInstallerAvailable = false;
    bool m_hasStagedBody = false;
    bool m_downloadRejected = false;
    QByteArray m_stagedBody;
    std::unique_ptr<QTemporaryDir> m_stagedDirectory;
    QString m_updatesDirectory;
    QString m_installerPath;
    QString m_partialPath;
    QFile m_partialFile;
    QCryptographicHash m_downloadHash{QCryptographicHash::Sha256};
    QNetworkAccessManager *m_nam = nullptr;
    QPointer<QNetworkReply> m_reply;
    int m_launchAttempts = 0;
    bool m_launchProcess = true;
    qint64 m_bytesWritten = 0;
};
