#include "omaircupdatecheck.h"

#ifndef OMAIRC_VERSION
#error "Build with version.pri so OMAIRC_VERSION is defined"
#endif

#include <QJsonDocument>
#include <QJsonObject>
#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QVector>
#include <QtQml>

namespace {
constexpr int kUpdateCheckTimeoutMs = 10000;
constexpr int kMaximumReleaseBytes = 256 * 1024;
const auto kIdle = QStringLiteral("idle");
const auto kChecking = QStringLiteral("checking");
const auto kUpToDate = QStringLiteral("upToDate");
const auto kUpdateAvailable = QStringLiteral("updateAvailable");
const auto kFailed = QStringLiteral("failed");

int versionSuffixRank(const QString &suffix)
{
    if (suffix.isEmpty())
        return 100;
    if (suffix.startsWith(QLatin1String("alpha")))
        return 10;
    if (suffix.startsWith(QLatin1String("beta")))
        return 20;
    if (suffix.startsWith(QLatin1String("rc")))
        return 30;
    return 0;
}

struct VersionParts {
    QVector<int> numeric;
    QString suffix;
};

VersionParts parseVersion(QString view)
{
    view = view.trimmed();
    if (view.startsWith(QLatin1Char('v')) || view.startsWith(QLatin1Char('V')))
        view.remove(0, 1);

    VersionParts parts;
    int index = 0;
    while (index < view.size() && view.at(index).isDigit()) {
        int value = 0;
        while (index < view.size() && view.at(index).isDigit()) {
            value = value * 10 + view.at(index).digitValue();
            ++index;
        }
        parts.numeric.append(value);
        if (index < view.size() && view.at(index) == QLatin1Char('.')) {
            ++index;
            continue;
        }
        break;
    }
    QString suffix = view.mid(index).toLower();
    if (suffix.startsWith(QLatin1Char('-')) || suffix.startsWith(QLatin1Char('+')))
        suffix.remove(0, 1);
    parts.suffix = suffix;
    return parts;
}
}

QString omaircGithubRepoUrl()
{
    return QStringLiteral("https://github.com/fredimachado/omairc");
}

QUrl omaircGithubLatestReleaseApiUrl()
{
    return QUrl(QStringLiteral(
        "https://api.github.com/repos/fredimachado/omairc/releases/latest"));
}

bool omaircGithubReleaseUrlIsSafe(const QUrl &url)
{
    if (!url.isValid() || url.scheme() != QLatin1String("https"))
        return false;
    if (url.host().compare(QLatin1String("github.com"), Qt::CaseInsensitive) != 0)
        return false;
    const QString path = url.path();
    return path == QLatin1String("/fredimachado/omairc")
        || path.startsWith(QLatin1String("/fredimachado/omairc/"));
}

int omaircCompareVersions(const QString &left, const QString &right)
{
    const VersionParts a = parseVersion(left);
    const VersionParts b = parseVersion(right);
    const int count = qMax(a.numeric.size(), b.numeric.size());
    for (int index = 0; index < count; ++index) {
        const int av = index < a.numeric.size() ? a.numeric.at(index) : 0;
        const int bv = index < b.numeric.size() ? b.numeric.at(index) : 0;
        if (av != bv)
            return av < bv ? -1 : 1;
    }
    const int ar = versionSuffixRank(a.suffix);
    const int br = versionSuffixRank(b.suffix);
    if (ar != br)
        return ar < br ? -1 : 1;
    if (a.suffix == b.suffix)
        return 0;
    return a.suffix < b.suffix ? -1 : 1;
}

std::optional<OmaircGithubRelease> omaircParseGithubRelease(const QByteArray &json)
{
    QJsonParseError error;
    const QJsonDocument document = QJsonDocument::fromJson(json, &error);
    if (error.error != QJsonParseError::NoError || !document.isObject())
        return std::nullopt;

    const QJsonObject object = document.object();
    QString tag = object.value(QStringLiteral("tag_name")).toString().trimmed();
    if (tag.isEmpty())
        return std::nullopt;

    QString version = tag;
    if (version.startsWith(QLatin1Char('v')) || version.startsWith(QLatin1Char('V')))
        version.remove(0, 1);
    if (version.isEmpty())
        return std::nullopt;

    QString htmlUrl = object.value(QStringLiteral("html_url")).toString().trimmed();
    if (htmlUrl.isEmpty())
        htmlUrl = QStringLiteral("https://github.com/fredimachado/omairc/releases/latest");
    if (!omaircGithubReleaseUrlIsSafe(QUrl(htmlUrl)))
        return std::nullopt;

    return OmaircGithubRelease{version, tag, htmlUrl};
}

void omaircRegisterUpdateCheck()
{
    static bool registered = false;
    if (registered)
        return;
    registered = true;
    qmlRegisterType<OmaircUpdateCheck>("Omairc.App", 1, 0, "UpdateCheck");
}

OmaircUpdateCheck::OmaircUpdateCheck(QObject *parent)
    : QObject(parent)
    , m_currentVersion(QStringLiteral(OMAIRC_VERSION))
{
}

OmaircUpdateCheck::~OmaircUpdateCheck()
{
    abortReply();
}

QString OmaircUpdateCheck::message() const
{
    if (m_status == kChecking)
        return QStringLiteral("Checking…");
    if (m_status == kUpToDate)
        return QStringLiteral("Omairc is up to date.");
    if (m_status == kUpdateAvailable) {
        if (m_latestVersion.isEmpty())
            return QStringLiteral("A newer version is available.");
        return QStringLiteral("Version %1 is available.").arg(m_latestVersion);
    }
    if (m_status == kFailed)
        return QStringLiteral("Could not check for updates.");
    return {};
}

void OmaircUpdateCheck::setCurrentVersion(const QString &version)
{
    if (m_currentVersion == version)
        return;
    m_currentVersion = version;
    emit currentVersionChanged();
}

void OmaircUpdateCheck::setNetworkAccessManager(QNetworkAccessManager *nam)
{
    if (m_nam && m_nam->parent() == this && m_nam != nam)
        delete m_nam;
    m_nam = nam;
}

QNetworkAccessManager *OmaircUpdateCheck::ensureNam()
{
    if (!m_nam)
        m_nam = new QNetworkAccessManager(this);
    return m_nam;
}

void OmaircUpdateCheck::abortReply()
{
    if (!m_reply)
        return;
    QNetworkReply *reply = m_reply;
    m_reply.clear();
    reply->disconnect(this);
    reply->abort();
    reply->deleteLater();
}

void OmaircUpdateCheck::setStatus(const QString &status)
{
    if (m_status == status)
        return;
    m_status = status;
    emit statusChanged();
    emit messageChanged();
}

void OmaircUpdateCheck::finishFailed()
{
    m_latestVersion.clear();
    m_latestUrl.clear();
    emit latestChanged();
    setStatus(kFailed);
}

void OmaircUpdateCheck::finishWithBody(const QByteArray &body, int httpStatus)
{
    if (httpStatus != 200) {
        finishFailed();
        return;
    }
    const std::optional<OmaircGithubRelease> release = omaircParseGithubRelease(body);
    if (!release) {
        finishFailed();
        return;
    }
    m_latestVersion = release->version;
    m_latestUrl = release->htmlUrl;
    emit latestChanged();
    if (omaircCompareVersions(m_currentVersion, release->version) >= 0)
        setStatus(kUpToDate);
    else
        setStatus(kUpdateAvailable);
}

void OmaircUpdateCheck::cancel()
{
    abortReply();
    if (m_status == kChecking)
        setStatus(kIdle);
}

void OmaircUpdateCheck::applyGithubPayload(const QString &body, int httpStatus)
{
    abortReply();
    finishWithBody(body.toUtf8(), httpStatus);
}

void OmaircUpdateCheck::check()
{
    abortReply();
    m_latestVersion.clear();
    m_latestUrl.clear();
    emit latestChanged();
    setStatus(kChecking);

    QNetworkRequest request(omaircGithubLatestReleaseApiUrl());
    request.setRawHeader("Accept", "application/vnd.github+json");
    request.setRawHeader("X-GitHub-Api-Version", "2022-11-28");
    request.setHeader(QNetworkRequest::UserAgentHeader,
                      QStringLiteral("Omairc/%1 (+%2)")
                          .arg(m_currentVersion, omaircGithubRepoUrl()));
    request.setAttribute(QNetworkRequest::RedirectPolicyAttribute,
                         QNetworkRequest::NoLessSafeRedirectPolicy);
    request.setTransferTimeout(kUpdateCheckTimeoutMs);

    m_reply = ensureNam()->get(request);
    connect(m_reply, &QNetworkReply::finished, this, [this]() {
        QNetworkReply *reply = m_reply;
        m_reply.clear();
        if (!reply)
            return;
        reply->deleteLater();
        if (reply->error() == QNetworkReply::OperationCanceledError)
            return;
        const int httpStatus =
            reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
        if (reply->error() != QNetworkReply::NoError && httpStatus == 0) {
            finishFailed();
            return;
        }
        const QByteArray body = reply->readAll();
        if (body.size() > kMaximumReleaseBytes) {
            finishFailed();
            return;
        }
        finishWithBody(body, httpStatus);
    });
}
