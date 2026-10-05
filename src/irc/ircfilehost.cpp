#include "ircfilehost.h"

#include "ircfilelink.h"

#include <QFile>
#include <QFileInfo>
#include <QMimeDatabase>
#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QUrl>

#if defined(QT_GUI_LIB)
#include <QBuffer>
#include <QImage>
#include <QMimeData>
#endif

#include <optional>

namespace
{
constexpr int kTransferTimeoutMs = 60000;

QString safeFileName(const QString &name)
{
    const QString base = QFileInfo(name).fileName();
    QString out;
    out.reserve(base.size());
    for (const QChar character : base) {
        const ushort code = character.unicode();
        if (code < 0x20 || code == '"' || code == '\\' || code == '/' || code == ';')
            out += QLatin1Char('_');
        else
            out += character;
    }
    if (out.isEmpty())
        return QStringLiteral("file");
    if (out.size() > 180)
        return out.right(180);
    return out;
}

QString contentTypeFor(const QString &path, const QString &overrideType)
{
    if (!overrideType.isEmpty())
        return overrideType;
    const QString detected = QMimeDatabase().mimeTypeForFile(path).name();
    if (detected.isEmpty())
        return QStringLiteral("application/octet-stream");
    return detected;
}

bool regularFile(const QString &path)
{
    const QFileInfo info(path);
    return info.exists() && info.isFile() && !info.isDir();
}

QString failureText(const QString &message)
{
    if (message.isEmpty())
        return QStringLiteral("Could not upload the file.");
    return message;
}
}

IrcFileUploader::IrcFileUploader(QObject *parent)
    : QObject(parent)
    , m_nam(new QNetworkAccessManager(this))
{
}

void IrcFileUploader::setNetworkAccessManager(QNetworkAccessManager *nam)
{
    if (!nam || nam == m_nam)
        return;
    if (m_nam && m_nam->parent() == this)
        delete m_nam;
    m_nam = nam;
}

bool IrcFileUploader::busy() const
{
    return m_sending || !m_queue.isEmpty();
}

void IrcFileUploader::enqueue(IrcFileUploadJob job)
{
    m_queue.append(std::move(job));
    if (!m_sending)
        startNext();
}

bool IrcFileUploader::readJob(IrcFileUploadJob &job, QString *message) const
{
    if (!job.fromBytes) {
        if (!regularFile(job.path)) {
            *message = QStringLiteral("That is not a file.");
            return false;
        }
        const QFileInfo info(job.path);
        if (info.size() <= 0) {
            *message = QStringLiteral("The file is empty.");
            return false;
        }
        if (info.size() > kMaximumBytes) {
            *message = QStringLiteral("The file is too large.");
            return false;
        }
        job.fileName = safeFileName(info.fileName());
        job.contentType = contentTypeFor(job.path, job.contentType);
        return true;
    }
    job.fileName = safeFileName(job.fileName);
    if (job.contentType.isEmpty())
        job.contentType = QStringLiteral("application/octet-stream");
    if (job.body.isEmpty()) {
        *message = QStringLiteral("The file is empty.");
        return false;
    }
    if (job.body.size() > kMaximumBytes) {
        *message = QStringLiteral("The file is too large.");
        return false;
    }
    return true;
}

void IrcFileUploader::startNext()
{
    while (!m_sending && !m_queue.isEmpty() && m_nam) {
    IrcFileUploadJob job = m_queue.takeFirst();
    QString message;
    if (!readJob(job, &message)) {
        emit failed(failureText(message));
        continue;
    }

    const QUrl endpoint(job.endpoint);
    if (!endpoint.isValid() || endpoint.scheme().isEmpty()) {
        emit failed(QStringLiteral("Could not upload the file."));
        job.secret.clear();
        continue;
    }
    const bool https = endpoint.scheme().compare(QLatin1String("https"), Qt::CaseInsensitive) == 0;
    if (job.serverEncrypted && !https) {
        emit failed(QStringLiteral("Could not upload the file."));
        job.secret.clear();
        continue;
    }

    QNetworkRequest request(endpoint);
    request.setTransferTimeout(kTransferTimeoutMs);
    request.setAttribute(QNetworkRequest::RedirectPolicyAttribute,
                         QNetworkRequest::ManualRedirectPolicy);
    request.setHeader(QNetworkRequest::ContentTypeHeader, job.contentType);
    request.setRawHeader("Content-Disposition",
                         QByteArray("attachment; filename=\"")
                             + job.fileName.toUtf8()
                             + '"');
    if (fileHostSendsBasicAuth(job.serverHost.toStdString(),
                               job.endpoint.toStdString(),
                               job.serverEncrypted)) {
        const std::string header = basicAuthorizationValue(job.user.toStdString(),
                                                           job.secret.toStdString());
        if (!header.empty())
            request.setRawHeader("Authorization", QByteArray::fromStdString(header));
    }
    job.secret.clear();
    job.user.clear();

    QFile *file = nullptr;
    if (!job.fromBytes) {
        file = new QFile(job.path);
        if (!file->open(QIODevice::ReadOnly)) {
            delete file;
            emit failed(QStringLiteral("Could not upload the file."));
            continue;
        }
        request.setHeader(QNetworkRequest::ContentLengthHeader, file->size());
    } else {
        request.setHeader(QNetworkRequest::ContentLengthHeader, job.body.size());
    }

    m_sending = true;
    const QString endpointText = job.endpoint;
    if (file)
        m_reply = m_nam->post(request, file);
    else
        m_reply = m_nam->post(request, job.body);
    if (!m_reply) {
        delete file;
        m_sending = false;
        emit failed(QStringLiteral("Could not upload the file."));
        continue;
    }
    if (file)
        file->setParent(m_reply);
    connect(m_reply, &QNetworkReply::finished, this, [this, endpointText] {
        QNetworkReply *reply = m_reply;
        m_reply = nullptr;
        m_sending = false;
        QString url;
        QString message = QStringLiteral("Could not upload the file.");
        if (reply) {
            const int status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
            const QByteArray location = reply->rawHeader("Location");
            reply->deleteLater();
            if (status == 201) {
                if (const auto link = absoluteFileLink(endpointText.toStdString(),
                                                       std::string(location.constData(),
                                                                   static_cast<std::size_t>(location.size()))))
                    url = QString::fromStdString(*link);
            }
        }
        finishCurrent(url, url.isEmpty() ? message : QString());
    });
    return;
    }
}

void IrcFileUploader::finishCurrent(const QString &url, const QString &message)
{
    if (!url.isEmpty())
        emit linkReady(url);
    else
        emit failed(failureText(message));
    startNext();
}

#if defined(QT_GUI_LIB)
std::optional<IrcClipboardOffer> ircClipboardOffer(const QMimeData *mime)
{
    if (!mime)
        return std::nullopt;

    QStringList paths;
    bool sawLocal = false;
    bool sawNonRegular = false;
    bool sawMissing = false;
    const QList<QUrl> urls = mime->urls();
    for (const QUrl &url : urls) {
        if (!url.isLocalFile())
            continue;
        sawLocal = true;
        const QString path = url.toLocalFile();
        const QFileInfo info(path);
        if (!info.exists()) {
            sawMissing = true;
            continue;
        }
        if (regularFile(path))
            paths.append(path);
        else
            sawNonRegular = true;
    }
    if (sawLocal && sawNonRegular && !sawMissing)
        return IrcClipboardOffer{{}, {}, true};
    if (!paths.isEmpty())
        return IrcClipboardOffer{paths, {}, false};

    const QString text = mime->text().trimmed();
    if (mime->hasImage() && text.isEmpty()) {
        const QImage image = qvariant_cast<QImage>(mime->imageData());
        if (image.isNull())
            return std::nullopt;
        QByteArray png;
        QBuffer buffer(&png);
        buffer.open(QIODevice::WriteOnly);
        if (!image.save(&buffer, "PNG") || png.isEmpty())
            return std::nullopt;
        return IrcClipboardOffer{{}, png};
    }
    return std::nullopt;
}
#endif
