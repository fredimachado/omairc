#pragma once

#include <QByteArray>
#include <QObject>
#include <QPointer>
#include <QString>
#include <QUrl>

#include <optional>

class QNetworkAccessManager;
class QNetworkReply;

struct OmaircGithubRelease {
    QString version;
    QString tag;
    QString htmlUrl;
};

QString omaircGithubRepoUrl();
QUrl omaircGithubLatestReleaseApiUrl();
bool omaircGithubReleaseUrlIsSafe(const QUrl &url);
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

    void setNetworkAccessManager(QNetworkAccessManager *nam);

    Q_INVOKABLE void check();
    Q_INVOKABLE void cancel();
    Q_INVOKABLE void applyGithubPayload(const QString &body, int httpStatus);

signals:
    void statusChanged();
    void messageChanged();
    void latestChanged();
    void currentVersionChanged();

private:
    QNetworkAccessManager *ensureNam();
    void abortReply();
    void setStatus(const QString &status);
    void finishFailed();
    void finishWithBody(const QByteArray &body, int httpStatus);

    QString m_status = QStringLiteral("idle");
    QString m_currentVersion;
    QString m_latestVersion;
    QString m_latestUrl;
    QNetworkAccessManager *m_nam = nullptr;
    QPointer<QNetworkReply> m_reply;
};
