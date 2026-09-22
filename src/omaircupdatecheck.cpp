#include "omaircupdatecheck.h"
#include "omaircpaths.h"
#include "omaircversion.h"

#include <QCoreApplication>
#include <QDir>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QProcess>
#include <QVector>
#include <QtQml>

#ifdef Q_OS_WIN
#  include <QSettings>
#endif

namespace {
constexpr int kUpdateCheckTimeoutMs = 10000;
constexpr int kInstallerTransferTimeoutMs = 60000;
constexpr int kMaximumReleaseBytes = 256 * 1024;
constexpr qint64 kMaximumSetupBytes = 80LL * 1024 * 1024;
const auto kIdle = QStringLiteral("idle");
const auto kChecking = QStringLiteral("checking");
const auto kUpToDate = QStringLiteral("upToDate");
const auto kUpdateAvailable = QStringLiteral("updateAvailable");
const auto kDownloading = QStringLiteral("downloading");
const auto kReadyToRestart = QStringLiteral("readyToRestart");
const auto kDownloadFailed = QStringLiteral("downloadFailed");
const auto kFailed = QStringLiteral("failed");
const auto kWindowsUninstallKey =
    QStringLiteral("{37400F83-8332-4043-8DFF-1F69E51F6555}_is1");

bool releaseTokenIsSafe(const QString &token)
{
    if (token.isEmpty() || token.size() > 64)
        return false;
    for (const QChar ch : token) {
        if (ch.isDigit() || ch.isLetter())
            continue;
        if (ch == QLatin1Char('.') || ch == QLatin1Char('_') || ch == QLatin1Char('-'))
            continue;
        return false;
    }
    return true;
}

QString normalizeInstallDir(QString path)
{
    path = QDir::cleanPath(path.trimmed());
    while (path.endsWith(QLatin1Char('/')) || path.endsWith(QLatin1Char('\\')))
        path.chop(1);
    return path;
}

std::optional<QString> parseSha256Digest(const QString &digest)
{
    const auto prefix = QLatin1String("sha256:");
    if (!digest.startsWith(prefix))
        return std::nullopt;
    const QString hex = digest.mid(prefix.size()).toLower();
    if (hex.size() != 64)
        return std::nullopt;
    for (const QChar ch : hex) {
        if (ch.isDigit())
            continue;
        if (ch < QLatin1Char('a') || ch > QLatin1Char('f'))
            return std::nullopt;
    }
    return hex;
}

bool runningCopyIsInstalled()
{
#ifdef Q_OS_WIN
    const QString appDir = QCoreApplication::applicationDirPath();
    const QStringList roots = {
        QStringLiteral("HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\"),
        QStringLiteral("HKEY_LOCAL_MACHINE\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\"),
        QStringLiteral("HKEY_LOCAL_MACHINE\\Software\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\"),
    };
    for (const QString &root : roots) {
        QSettings settings(root + kWindowsUninstallKey, QSettings::NativeFormat);
        if (omaircInstallLocationMatches(
                settings.value(QStringLiteral("InstallLocation")).toString(), appDir))
            return true;
    }
    return false;
#else
    return false;
#endif
}

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

QString omaircExpectedWindowsSetupName(const QString &version)
{
    return QStringLiteral("omairc-%1-windows-x64-setup.exe").arg(version);
}

bool omaircWindowsSetupRequestIsSafe(const QUrl &url, const QString &tag,
                                      const QString &version)
{
    if (!releaseTokenIsSafe(tag) || !releaseTokenIsSafe(version))
        return false;
    if (!url.isValid() || url.scheme() != QLatin1String("https"))
        return false;
    if (url.host().compare(QLatin1String("github.com"), Qt::CaseInsensitive) != 0)
        return false;
    if (url.hasQuery() || url.hasFragment() || !url.userInfo().isEmpty())
        return false;
    const QString path = QStringLiteral("/fredimachado/omairc/releases/download/%1/%2")
                             .arg(tag, omaircExpectedWindowsSetupName(version));
    return url.path() == path;
}

bool omaircWindowsSetupRedirectIsSafe(const QUrl &url)
{
    if (!url.isValid() || url.scheme() != QLatin1String("https") || !url.userInfo().isEmpty())
        return false;
    const QString host = url.host().toLower();
    if (host == QLatin1String("github.com")
        || host == QLatin1String("githubusercontent.com"))
        return true;
    return host.endsWith(QLatin1String(".github.com"))
        || host.endsWith(QLatin1String(".githubusercontent.com"));
}

bool omaircInstallLocationMatches(const QString &installLocation, const QString &appDir)
{
    const QString left = normalizeInstallDir(installLocation);
    const QString right = normalizeInstallDir(appDir);
    if (left.isEmpty() || right.isEmpty())
        return false;
#ifdef Q_OS_WIN
    return left.compare(right, Qt::CaseInsensitive) == 0;
#else
    return left == right;
#endif
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

    OmaircGithubRelease release{version, tag, htmlUrl};
    if (!releaseTokenIsSafe(tag) || !releaseTokenIsSafe(version))
        return release;

    const QString expectedName = omaircExpectedWindowsSetupName(version);
    const QJsonArray assets = object.value(QStringLiteral("assets")).toArray();
    int matches = 0;
    QString setupUrl;
    QString setupSha256;
    qint64 setupSize = 0;
    for (const QJsonValue &value : assets) {
        if (!value.isObject())
            continue;
        const QJsonObject asset = value.toObject();
        if (asset.value(QStringLiteral("name")).toString() != expectedName)
            continue;
        const QUrl downloadUrl(asset.value(QStringLiteral("browser_download_url"))
                                   .toString()
                                   .trimmed());
        const std::optional<QString> digest = parseSha256Digest(
            asset.value(QStringLiteral("digest")).toString().trimmed());
        const QJsonValue sizeValue = asset.value(QStringLiteral("size"));
        if (!sizeValue.isDouble() || !digest)
            continue;
        const double rawSize = sizeValue.toDouble();
        const qint64 size = static_cast<qint64>(rawSize);
        if (size <= 0 || size > kMaximumSetupBytes
            || static_cast<double>(size) != rawSize)
            continue;
        if (!omaircWindowsSetupRequestIsSafe(downloadUrl, tag, version))
            continue;
        ++matches;
        setupUrl = downloadUrl.toString(QUrl::FullyEncoded);
        setupSha256 = *digest;
        setupSize = size;
    }
    if (matches == 1) {
        release.setupUrl = setupUrl;
        release.setupSha256 = setupSha256;
        release.setupSize = setupSize;
    }
    return release;
}

void omaircRegisterUpdateCheck()
{
    static bool registered = false;
    if (registered)
        return;
    registered = true;
    qmlRegisterType<OmaircUpdateCheck>("Omairc.App", 1, 0, "UpdateCheck");
}

static void registerOmaircUpdateCheckAtStartup()
{
    omaircRegisterUpdateCheck();
}

Q_COREAPP_STARTUP_FUNCTION(registerOmaircUpdateCheckAtStartup)

OmaircUpdateCheck::OmaircUpdateCheck(QObject *parent)
    : QObject(parent)
    , m_currentVersion(QStringLiteral(OMAIRC_VERSION))
    , m_installedCopy(runningCopyIsInstalled())
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
    if (m_status == kDownloading) {
        if (m_setupSize <= 0 || m_bytesWritten <= 0)
            return QStringLiteral("Downloading update…");
        const int percent = qBound(
            0, static_cast<int>((m_bytesWritten * 100) / m_setupSize), 99);
        return QStringLiteral("Downloading update… %1%").arg(percent);
    }
    if (m_status == kReadyToRestart)
        return QStringLiteral("Restart to update");
    if (m_status == kDownloadFailed)
        return QStringLiteral("Could not download the update.");
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

void OmaircUpdateCheck::setInstalledCopy(bool installed)
{
    if (m_installedCopy == installed)
        return;
    m_installedCopy = installed;
    emit installedCopyChanged();
    publishInstallerAvailability();
}

void OmaircUpdateCheck::setUpdatesDirectory(const QString &directory)
{
    m_updatesDirectory = directory;
}

bool OmaircUpdateCheck::installerUpdateAvailable() const
{
    return m_installedCopy && m_setupSize > 0 && !m_setupUrl.isEmpty()
        && m_setupSha256.size() == 64;
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
    discardPartial();
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

void OmaircUpdateCheck::forgetDownloadedInstaller()
{
    if (m_installerPath.isEmpty())
        return;
    QFile::remove(m_installerPath);
    m_installerPath.clear();
    emit installerPathChanged();
}

void OmaircUpdateCheck::clearSetup()
{
    m_setupUrl.clear();
    m_setupSha256.clear();
    m_setupSize = 0;
    forgetDownloadedInstaller();
    publishInstallerAvailability();
}

void OmaircUpdateCheck::publishInstallerAvailability()
{
    const bool available = installerUpdateAvailable();
    if (m_publishedInstallerAvailable == available)
        return;
    m_publishedInstallerAvailable = available;
    emit installerUpdateAvailableChanged();
}

void OmaircUpdateCheck::finishFailed()
{
    m_latestVersion.clear();
    m_latestUrl.clear();
    emit latestChanged();
    clearSetup();
    setStatus(kFailed);
}

void OmaircUpdateCheck::finishDownloadFailed()
{
    discardPartial();
    forgetDownloadedInstaller();
    setStatus(kDownloadFailed);
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
    forgetDownloadedInstaller();
    m_setupUrl = release->setupUrl;
    m_setupSha256 = release->setupSha256;
    m_setupSize = release->setupSize;
    emit latestChanged();
    publishInstallerAvailability();
    if (omaircCompareVersions(m_currentVersion, release->version) >= 0)
        setStatus(kUpToDate);
    else
        setStatus(kUpdateAvailable);
}

void OmaircUpdateCheck::cancel()
{
    if (m_status != kChecking)
        return;
    abortReply();
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
    m_hasStagedBody = false;
    m_stagedBody.clear();
    m_latestVersion.clear();
    m_latestUrl.clear();
    clearSetup();
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

QString OmaircUpdateCheck::updatesDirectory() const
{
    if (!m_updatesDirectory.isEmpty())
        return m_updatesDirectory;
    QString root = omaircStateRoot();
    if (root.isEmpty())
        root = QDir::tempPath();
    return QDir(root).filePath(QStringLiteral("omairc/updates"));
}

void OmaircUpdateCheck::discardPartial()
{
    if (m_partialFile.isOpen())
        m_partialFile.close();
    if (m_partialPath.isEmpty())
        return;
    QFile::remove(m_partialPath);
    m_partialPath.clear();
}

bool OmaircUpdateCheck::beginPartial()
{
    discardPartial();
    m_downloadHash.reset();
    m_bytesWritten = 0;
    m_downloadRejected = false;
    const QString dir = updatesDirectory();
    if (dir.isEmpty() || !QDir().mkpath(dir))
        return false;
    const QString name = omaircExpectedWindowsSetupName(m_latestVersion);
    if (!releaseTokenIsSafe(m_latestVersion))
        return false;
    m_partialPath = QDir(dir).filePath(name + QStringLiteral(".partial"));
    QFile::remove(m_partialPath);
    m_partialFile.setFileName(m_partialPath);
    if (!m_partialFile.open(QIODevice::WriteOnly)) {
        m_partialPath.clear();
        return false;
    }
    return true;
}

bool OmaircUpdateCheck::acceptChunk(const QByteArray &chunk)
{
    if (m_downloadRejected)
        return false;
    if (chunk.isEmpty())
        return true;
    if (m_setupSize <= 0 || m_bytesWritten > m_setupSize - chunk.size()) {
        m_downloadRejected = true;
        return false;
    }
    if (!m_partialFile.isOpen() || m_partialFile.write(chunk) != chunk.size()) {
        m_downloadRejected = true;
        return false;
    }
    m_downloadHash.addData(chunk);
    m_bytesWritten += chunk.size();
    emit messageChanged();
    return true;
}

void OmaircUpdateCheck::commitDownloadedInstaller(const QUrl &finalUrl)
{
    if (m_partialFile.isOpen())
        m_partialFile.close();
    const QString digest = QString::fromLatin1(m_downloadHash.result().toHex());
    if (m_downloadRejected || m_bytesWritten != m_setupSize
        || digest.compare(m_setupSha256, Qt::CaseInsensitive) != 0
        || (!finalUrl.isEmpty() && !omaircWindowsSetupRedirectIsSafe(finalUrl))) {
        finishDownloadFailed();
        return;
    }
    const QString finalPath = QDir(updatesDirectory()).filePath(
        omaircExpectedWindowsSetupName(m_latestVersion));
    if (QFileInfo::exists(finalPath))
        QFile::remove(finalPath);
    if (!m_partialFile.rename(finalPath)) {
        finishDownloadFailed();
        return;
    }
    m_partialPath.clear();
    const QFileDevice::Permissions permissions =
        QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ExeOwner;
    if (!QFile::setPermissions(finalPath, permissions)) {
        QFile::remove(finalPath);
        finishDownloadFailed();
        return;
    }
    m_installerPath = finalPath;
    emit installerPathChanged();
    setStatus(kReadyToRestart);
}

void OmaircUpdateCheck::startNetworkDownload()
{
    QNetworkRequest request{QUrl(m_setupUrl)};
    request.setHeader(QNetworkRequest::UserAgentHeader,
                      QStringLiteral("Omairc/%1 (+%2)")
                          .arg(m_currentVersion, omaircGithubRepoUrl()));
    request.setAttribute(QNetworkRequest::RedirectPolicyAttribute,
                         QNetworkRequest::NoLessSafeRedirectPolicy);
    request.setTransferTimeout(kInstallerTransferTimeoutMs);

    m_reply = ensureNam()->get(request);
    connect(m_reply, &QNetworkReply::readyRead, this, [this]() {
        if (!m_reply)
            return;
        if (acceptChunk(m_reply->readAll()))
            return;
        abortReply();
        if (m_status == kDownloading)
            finishDownloadFailed();
    });
    connect(m_reply, &QNetworkReply::finished, this, [this]() {
        QNetworkReply *reply = m_reply;
        m_reply.clear();
        if (!reply)
            return;
        reply->deleteLater();
        if (m_status != kDownloading)
            return;
        if (reply->error() == QNetworkReply::OperationCanceledError)
            return;
        if (reply->bytesAvailable() > 0 && !acceptChunk(reply->readAll())) {
            finishDownloadFailed();
            return;
        }
        const int httpStatus =
            reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
        if (reply->error() != QNetworkReply::NoError || httpStatus != 200) {
            finishDownloadFailed();
            return;
        }
        commitDownloadedInstaller(reply->url());
    });
}

void OmaircUpdateCheck::suppressInstallerLaunch()
{
    m_launchProcess = false;
}

void OmaircUpdateCheck::stageInstallerDownload(const QString &body)
{
    m_stagedBody = body.toUtf8();
    m_hasStagedBody = true;
    if (!m_updatesDirectory.isEmpty())
        return;
    m_stagedDirectory = std::make_unique<QTemporaryDir>();
    if (m_stagedDirectory->isValid())
        m_updatesDirectory = m_stagedDirectory->path();
}

void OmaircUpdateCheck::download()
{
    if (!installerUpdateAvailable())
        return;
    if (m_status != kUpdateAvailable && m_status != kDownloadFailed)
        return;

    if (m_reply) {
        QNetworkReply *reply = m_reply;
        m_reply.clear();
        reply->disconnect(this);
        reply->abort();
        reply->deleteLater();
    }

    setStatus(kDownloading);
    if (!beginPartial()) {
        finishDownloadFailed();
        return;
    }
    if (m_hasStagedBody) {
        const QByteArray body = m_stagedBody;
        m_hasStagedBody = false;
        m_stagedBody.clear();
        if (!acceptChunk(body)) {
            finishDownloadFailed();
            return;
        }
        commitDownloadedInstaller(QUrl());
        return;
    }
    startNetworkDownload();
}

bool OmaircUpdateCheck::downloadedInstallerMatches() const
{
    if (m_installerPath.isEmpty() || m_setupSha256.size() != 64 || m_setupSize <= 0)
        return false;
    QFile file(m_installerPath);
    if (!file.open(QIODevice::ReadOnly))
        return false;
    if (file.size() != m_setupSize)
        return false;
    QCryptographicHash hash(QCryptographicHash::Sha256);
    if (!hash.addData(&file))
        return false;
    return QString::fromLatin1(hash.result().toHex()).compare(
               m_setupSha256, Qt::CaseInsensitive) == 0;
}

bool OmaircUpdateCheck::launchInstaller()
{
    if (m_status != kReadyToRestart)
        return false;
    if (!downloadedInstallerMatches()) {
        finishDownloadFailed();
        return false;
    }
    ++m_launchAttempts;
    emit launchAttemptsChanged();
    if (!m_launchProcess)
        return true;
#ifdef Q_OS_WIN
    // Unsigned setup: show the Inno wizard. Silent flags would hide a SmartScreen or install failure.
    const QString workingDirectory = QFileInfo(m_installerPath).absolutePath();
    return QProcess::startDetached(m_installerPath, {}, workingDirectory);
#else
    return true;
#endif
}
