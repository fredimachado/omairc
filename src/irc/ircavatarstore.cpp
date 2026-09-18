#include "ircavatarstore.h"

#include "ircavatarurl.h"

#include <QBuffer>
#include <QCryptographicHash>
#include <QImageReader>
#include <QMutexLocker>
#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QPainter>
#include <QPainterPath>
#include <QQmlEngine>
#include <QSslConfiguration>
#include <QSslError>
#include <QSslSocket>
#include <QTimer>
#include <QtGlobal>

namespace
{
constexpr int kMaximumAvatarBytes = 512 * 1024;
constexpr int kFetchTimeoutMs = 10000;
constexpr int kMaximumCacheEntries = 64;
constexpr int kMaximumCircledEdge = 128;
constexpr int kMaximumRedirects = 3;
// Decoded budget is independent of the circled render size: a tiny compressed
// payload can still declare a multi-gigapixel IHDR.
constexpr int kMaximumDecodedEdge = 1024;
constexpr qint64 kMaximumDecodedPixels = 1024 * 1024;
constexpr int kMaximumDecodedAllocationMb = 16;
const auto kProviderId = QStringLiteral("omairc-avatar");
const auto kPinnedAddressAttribute =
    static_cast<QNetworkRequest::Attribute>(QNetworkRequest::User + 1000);

QByteArray avatarRequestPath(const QUrl& url)
{
    QByteArray path = url.path(QUrl::FullyEncoded).toUtf8();
    if (path.isEmpty())
        path = "/";
    const QByteArray query = url.query(QUrl::FullyEncoded).toUtf8();
    if (!query.isEmpty())
        path += '?' + query;
    return path;
}

QByteArray avatarHostHeader(const QUrl& url)
{
    QByteArray host = url.host().toUtf8();
    const int port = url.port(-1);
    if (port != -1 && port != 443)
        host += ':' + QByteArray::number(port);
    return host;
}

QByteArray headerValue(const QByteArray& headers, const QByteArray& name)
{
    const QByteArray needle = name.toLower() + ": ";
    int offset = 0;
    while (offset < headers.size()) {
        const int lineEnd = headers.indexOf("\r\n", offset);
        const int lineLength =
            lineEnd < 0 ? headers.size() - offset : lineEnd - offset;
        const QByteArray line = headers.mid(offset, lineLength);
        if (line.isEmpty())
            break;
        if (line.startsWith(needle))
            return line.mid(needle.size());
        if (lineEnd < 0)
            break;
        offset = lineEnd + 2;
    }
    return {};
}

class PinnedHttpsReply : public QNetworkReply
{
public:
    PinnedHttpsReply(const QNetworkRequest& request, const QHostAddress& pinned,
                     QObject *parent)
        : QNetworkReply(parent)
        , m_request(request)
        , m_pinned(pinned)
    {
        setRequest(request);
        setUrl(request.url());
        open(QIODevice::ReadOnly);
        QTimer::singleShot(0, this, [this]() { start(); });
    }

protected:
    qint64 readData(char *data, qint64 maxSize) override
    {
        if (m_readOffset >= m_body.size())
            return -1;
        const qint64 chunk = qMin(maxSize, m_body.size() - m_readOffset);
        memcpy(data, m_body.constData() + m_readOffset, chunk);
        m_readOffset += chunk;
        return chunk;
    }

    void abort() override
    {
        if (m_aborted)
            return;
        m_aborted = true;
        m_timer.stop();
        m_socket.abort();
        setError(QNetworkReply::OperationCanceledError,
                 QStringLiteral("Avatar fetch aborted"));
        emit finished();
    }

private:
    void start()
    {
        const QUrl url = m_request.url();
        m_hostname = url.host();
        m_port = quint16(url.port(443));

        m_timer.setSingleShot(true);
        m_timer.setInterval(kFetchTimeoutMs);
        connect(&m_timer, &QTimer::timeout, this, [this]() {
            if (m_aborted || m_finished)
                return;
            m_socket.abort();
            fail(QNetworkReply::TimeoutError, QStringLiteral("Avatar fetch timed out"));
        });

        connect(&m_socket, &QSslSocket::encrypted, this, [this]() {
            sendRequest();
        });
        connect(&m_socket, &QSslSocket::readyRead, this, [this]() {
            consumeSocket();
        });
        connect(&m_socket, &QSslSocket::disconnected, this, [this]() {
            if (m_aborted || m_finished)
                return;
            if (!m_headersParsed)
                fail(QNetworkReply::ProtocolFailure,
                     QStringLiteral("Avatar fetch ended before headers"));
            else if (m_contentLength < 0)
                completeBody();
        });
        connect(&m_socket, &QSslSocket::sslErrors, this,
                [this](const QList<QSslError>& errors) {
            Q_UNUSED(errors);
            if (m_aborted || m_finished)
                return;
            m_socket.abort();
            fail(QNetworkReply::SslHandshakeFailedError,
                 QStringLiteral("Avatar fetch TLS verification failed"));
        });
        connect(&m_socket, &QAbstractSocket::errorOccurred, this,
                [this](QAbstractSocket::SocketError error) {
            if (m_aborted || m_finished || error == QAbstractSocket::RemoteHostClosedError)
                return;
            fail(QNetworkReply::HostNotFoundError, m_socket.errorString());
        });

        QSslConfiguration ssl = QSslConfiguration::defaultConfiguration();
        ssl.setAllowedNextProtocols({QByteArrayLiteral("http/1.1")});
        m_socket.setSslConfiguration(ssl);
        m_socket.setPeerVerifyName(m_hostname);
        m_timer.start();
        m_socket.connectToHostEncrypted(m_pinned.toString(), m_port, m_hostname);
    }

    void sendRequest()
    {
        QByteArray request;
        request += "GET ";
        request += avatarRequestPath(m_request.url());
        request += " HTTP/1.1\r\n";
        request += "Host: ";
        request += avatarHostHeader(m_request.url());
        request += "\r\n";
        const QByteArray accept = m_request.rawHeader("Accept");
        if (!accept.isEmpty()) {
            request += "Accept: ";
            request += accept;
            request += "\r\n";
        }
        const QByteArray userAgent = m_request.rawHeader("User-Agent");
        if (!userAgent.isEmpty()) {
            request += "User-Agent: ";
            request += userAgent;
            request += "\r\n";
        }
        request += "Connection: close\r\n\r\n";
        m_socket.write(request);
    }

    void consumeSocket()
    {
        m_buffer += m_socket.readAll();
        if (!m_headersParsed) {
            const int headerEnd = m_buffer.indexOf("\r\n\r\n");
            if (headerEnd < 0)
                return;
            if (!parseHeaders(m_buffer.left(headerEnd)))
                return;
            m_buffer.remove(0, headerEnd + 4);
            m_headersParsed = true;
            const int status =
                attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
            const QVariant redirect =
                attribute(QNetworkRequest::RedirectionTargetAttribute);
            if (redirect.isValid() && status >= 300 && status < 400) {
                completeBody();
                return;
            }
        }
        if (m_contentLength >= 0) {
            if (m_buffer.size() >= m_contentLength)
                completeBody();
            return;
        }
        if (m_socket.state() == QAbstractSocket::UnconnectedState)
            completeBody();
    }

    bool parseHeaders(const QByteArray& headerBlock)
    {
        const int lineEnd = headerBlock.indexOf("\r\n");
        if (lineEnd < 0)
            return false;
        const QList<QByteArray> statusParts = headerBlock.left(lineEnd).split(' ');
        if (statusParts.size() < 2) {
            fail(QNetworkReply::ProtocolFailure,
                 QStringLiteral("Avatar fetch returned malformed status"));
            return false;
        }
        bool ok = false;
        const int statusCode = statusParts.at(1).toInt(&ok);
        if (!ok) {
            fail(QNetworkReply::ProtocolFailure,
                 QStringLiteral("Avatar fetch returned malformed status"));
            return false;
        }
        setAttribute(QNetworkRequest::HttpStatusCodeAttribute, statusCode);
        const QByteArray contentType = headerValue(headerBlock, "Content-Type");
        if (!contentType.isEmpty())
            setHeader(QNetworkRequest::ContentTypeHeader, contentType);
        const QByteArray location = headerValue(headerBlock, "Location");
        if (!location.isEmpty()) {
            setAttribute(QNetworkRequest::RedirectionTargetAttribute,
                         QUrl::fromEncoded(location));
        }
        const QByteArray contentLength = headerValue(headerBlock, "Content-Length");
        if (!contentLength.isEmpty()) {
            m_contentLength = contentLength.toLongLong();
            if (m_contentLength < 0) {
                fail(QNetworkReply::ProtocolFailure,
                     QStringLiteral("Avatar fetch returned invalid Content-Length"));
                return false;
            }
        }
        return true;
    }

    void completeBody()
    {
        if (m_finished)
            return;
        if (m_contentLength >= 0)
            m_body = m_buffer.left(int(m_contentLength));
        else
            m_body = m_buffer;
        m_finished = true;
        m_timer.stop();
        m_socket.disconnect(this);
        m_socket.abort();
        setError(QNetworkReply::NoError, {});
        emit readyRead();
        emit finished();
    }

    void fail(QNetworkReply::NetworkError error, const QString& message)
    {
        if (m_aborted || m_finished)
            return;
        m_finished = true;
        m_timer.stop();
        m_socket.disconnect(this);
        m_socket.abort();
        setError(error, message);
        emit finished();
    }

    QNetworkRequest m_request;
    QHostAddress m_pinned;
    QSslSocket m_socket;
    QTimer m_timer;
    QString m_hostname;
    quint16 m_port = 443;
    QByteArray m_buffer;
    QByteArray m_body;
    qint64 m_readOffset = 0;
    qint64 m_contentLength = -1;
    bool m_headersParsed = false;
    bool m_aborted = false;
    bool m_finished = false;
};

class PinnedNetworkAccessManager : public QNetworkAccessManager
{
protected:
    QNetworkReply *createRequest(Operation op, const QNetworkRequest& request,
                                 QIODevice *outgoingData) override
    {
        const QVariant pinned =
            request.attribute(kPinnedAddressAttribute);
        if (op == GetOperation && pinned.isValid()) {
            const QHostAddress address = pinned.value<QHostAddress>();
            if (!address.isNull())
                return new PinnedHttpsReply(request, address, this);
        }
        return QNetworkAccessManager::createRequest(op, request, outgoingData);
    }
};

bool contentTypeIsImage(const QByteArray& type)
{
    const QByteArray lowered = type.trimmed().toLower();
    return lowered.startsWith("image/");
}

QUrl redirectTarget(const QNetworkReply *reply)
{
    const QVariant target =
        reply->attribute(QNetworkRequest::RedirectionTargetAttribute);
    if (!target.isValid())
        return {};
    const QUrl relative = target.toUrl();
    if (!relative.isValid())
        return {};
    return reply->url().resolved(relative);
}

bool decodedSizeWithinBudget(const QSize& size)
{
    if (!size.isValid() || size.width() <= 0 || size.height() <= 0)
        return false;
    if (size.width() > kMaximumDecodedEdge || size.height() > kMaximumDecodedEdge)
        return false;
    const qint64 pixels = qint64(size.width()) * qint64(size.height());
    if (pixels <= 0 || pixels > kMaximumDecodedPixels)
        return false;
    return true;
}

QImage readBoundedImage(QImageReader& reader)
{
    reader.setAutoTransform(true);
    reader.setAllocationLimit(kMaximumDecodedAllocationMb);
    const QSize size = reader.size();
    if (size.isValid() && !decodedSizeWithinBudget(size))
        return {};
    QImage image = reader.read();
    if (image.isNull())
        return {};
    if (!decodedSizeWithinBudget(image.size()))
        return {};
    return image;
}
}

IrcAvatarStore::IrcAvatarStore(QObject *parent)
    : QQuickImageProvider(QQuickImageProvider::Image)
    , m_nam(new PinnedNetworkAccessManager)
{
    m_nam->setParent(this);
    setParent(parent);
}

void IrcAvatarStore::setNetworkAccessManager(QNetworkAccessManager *nam)
{
    if (!nam || nam == m_nam)
        return;
    if (m_nam && m_nam->parent() == this)
        delete m_nam;
    m_nam = nam;
}

IrcAvatarStore::~IrcAvatarStore()
{
    const QList<Fetch> pending = m_fetches.values();
    m_fetches.clear();
    for (const Fetch& fetch : pending) {
        if (fetch.lookupId >= 0)
            QHostInfo::abortHostLookup(fetch.lookupId);
        if (fetch.reply) {
            fetch.reply->disconnect(this);
            fetch.reply->abort();
            fetch.reply->deleteLater();
        }
    }
}

QString IrcAvatarStore::keyForUrl(const QUrl& url)
{
    const QByteArray encoded = url.toEncoded(QUrl::FullyEncoded);
    return QString::fromLatin1(
        QCryptographicHash::hash(encoded, QCryptographicHash::Sha256).toHex());
}

QImage IrcAvatarStore::circled(const QImage& source, const QSize& requestedSize)
{
    if (source.isNull())
        return {};
    int edge = 32;
    if (requestedSize.width() > 0 && requestedSize.height() > 0)
        edge = qMax(requestedSize.width(), requestedSize.height());
    else if (source.width() > 0 && source.height() > 0)
        edge = qMin(qMax(source.width(), source.height()), kMaximumCircledEdge);
    edge = qMin(edge, kMaximumCircledEdge);
    const QSize target(edge, edge);
    QImage circle(target, QImage::Format_ARGB32_Premultiplied);
    circle.fill(Qt::transparent);
    QPainter painter(&circle);
    painter.setRenderHint(QPainter::Antialiasing, true);
    painter.setRenderHint(QPainter::SmoothPixmapTransform, true);
    QPainterPath clip;
    clip.addEllipse(QRectF(QPointF(0, 0), QSizeF(target)));
    painter.setClipPath(clip);
    const qreal scale = qMax(qreal(edge) / qMax(1, source.width()),
                             qreal(edge) / qMax(1, source.height()));
    const QSize drawn(qRound(source.width() * scale),
                      qRound(source.height() * scale));
    const QPoint offset((edge - drawn.width()) / 2, (edge - drawn.height()) / 2);
    painter.drawImage(QRect(offset, drawn), source);
    return circle;
}

QImage IrcAvatarStore::roundedRect(const QImage& source, const QSize& requestedSize)
{
    if (source.isNull())
        return {};
    int edge = 32;
    if (requestedSize.width() > 0 && requestedSize.height() > 0)
        edge = qMax(requestedSize.width(), requestedSize.height());
    else if (source.width() > 0 && source.height() > 0)
        edge = qMin(qMax(source.width(), source.height()), kMaximumCircledEdge);
    edge = qMin(edge, kMaximumCircledEdge);
    const QSize target(edge, edge);
    const qreal radius = 8.0 / 28.0 * edge;
    QImage frame(target, QImage::Format_ARGB32_Premultiplied);
    frame.fill(Qt::transparent);
    QPainter painter(&frame);
    painter.setRenderHint(QPainter::Antialiasing, true);
    painter.setRenderHint(QPainter::SmoothPixmapTransform, true);
    QPainterPath clip;
    clip.addRoundedRect(QRectF(QPointF(0, 0), QSizeF(target)), radius, radius);
    painter.setClipPath(clip);
    const qreal scale = qMax(qreal(edge) / qMax(1, source.width()),
                             qreal(edge) / qMax(1, source.height()));
    const QSize drawn(qRound(source.width() * scale),
                      qRound(source.height() * scale));
    const QPoint offset((edge - drawn.width()) / 2, (edge - drawn.height()) / 2);
    painter.drawImage(QRect(offset, drawn), source);
    return frame;
}

QImage IrcAvatarStore::loadBoundedImage(const QByteArray& body)
{
    QByteArray bytes = body;
    QBuffer buffer(&bytes);
    if (!buffer.open(QIODevice::ReadOnly))
        return {};
    QImageReader reader(&buffer);
    return readBoundedImage(reader);
}

QImage IrcAvatarStore::loadBoundedImageFromFile(const QString& path)
{
    QImageReader reader(path);
    return readBoundedImage(reader);
}

QString IrcAvatarStore::source(const QString& rawUrl, int pixelSize)
{
    return source(rawUrl, pixelSize, QString());
}

QString IrcAvatarStore::source(const QString& rawUrl, int pixelSize,
                               const QString& clip)
{
    const bool squareClip =
        clip.trimmed().compare(QLatin1String("square"), Qt::CaseInsensitive) == 0;
    const auto imageIdForKey = [&](const QString& key) -> QString {
        if (squareClip)
            return QStringLiteral("image://") + kProviderId
                + QLatin1String("/square/") + key;
        return QStringLiteral("image://") + kProviderId + QLatin1Char('/') + key;
    };

    const QString trimmed = rawUrl.trimmed();
    // Bundled demo art may use qrc: without the HTTPS policy. Network URLs
    // still must pass ircAvatarUrlIsSafe.
    if (trimmed.startsWith(QLatin1String("qrc:"), Qt::CaseInsensitive)) {
        const QUrl resourceUrl(trimmed);
        if (!resourceUrl.isValid())
            return {};
        const QString key = keyForUrl(resourceUrl);
        if (cached(key).isNull()) {
            // QImage loads Qt resources as ":/…", not "qrc:/…".
            const QString resourcePath = trimmed.mid(3);
            const QImage image = loadBoundedImageFromFile(resourcePath);
            if (image.isNull())
                return {};
            remember(key, image);
            // Synchronous fill — do not emit ready() here or storeSource
            // re-enters through avatarEpoch while still evaluating.
        }
        return imageIdForKey(key);
    }

    const int fetchSize = ircAvatarFetchPixelSize(pixelSize);
    if (fetchSize <= 0)
        return {};
    const QUrl url = ircResolvedAvatarUrl(rawUrl, fetchSize);
    if (!ircAvatarUrlIsSafe(url))
        return {};
    const QString key = keyForUrl(url);
    if (cached(key).isNull()) {
        scheduleFetch(rawUrl, url, key);
        return {};
    }
    return imageIdForKey(key);
}

QImage IrcAvatarStore::requestImage(const QString& id, QSize *size,
                                    const QSize& requestedSize)
{
    QString cacheKey = id;
    bool squareClip = false;
    const QString squarePrefix = QStringLiteral("square/");
    if (cacheKey.startsWith(squarePrefix)) {
        squareClip = true;
        cacheKey = cacheKey.mid(squarePrefix.size());
    }
    const QImage image = cached(cacheKey);
    if (image.isNull())
        return {};
    const QImage framed = squareClip
        ? roundedRect(image, requestedSize)
        : circled(image, requestedSize);
    if (size)
        *size = framed.size();
    return framed;
}

void IrcAvatarStore::scheduleFetch(const QString& rawUrl, const QUrl& url,
                                   const QString& key)
{
    if (!cached(key).isNull())
        return;
    if (m_fetches.contains(key))
        return;
    Fetch fetch;
    fetch.rawUrl = rawUrl;
    fetch.url = url;
    fetch.key = key;
    m_fetches.insert(key, fetch);
    lookupThenGet(fetch);
}

void IrcAvatarStore::lookupThenGet(Fetch fetch)
{
    auto found = m_fetches.find(fetch.key);
    if (found == m_fetches.end())
        found = m_fetches.insert(fetch.key, fetch);
    else
        *found = fetch;

    const QHostAddress literal(fetch.url.host());
    if (!literal.isNull()) {
        if (ircHostAddressIsUnsafe(literal)) {
            dropFetch(fetch.key);
            return;
        }
        found->pinnedAddress = QHostAddress();
        startGet(found.value());
        return;
    }

    const int lookupId = QHostInfo::lookupHost(
        fetch.url.host(), this,
        [this, key = fetch.key](const QHostInfo& info) {
            finishLookup(key, info);
        });
    found = m_fetches.find(fetch.key);
    if (found == m_fetches.end())
        return;
    found->lookupId = lookupId;
}

void IrcAvatarStore::startGet(Fetch fetch)
{
    if (!fetch.pinnedAddress.isNull()) {
        if (ircHostAddressIsUnsafe(fetch.pinnedAddress)) {
            dropFetch(fetch.key);
            return;
        }
        if (!ircAvatarUrlIsSafe(fetch.url)) {
            dropFetch(fetch.key);
            return;
        }
        if (!QHostAddress(fetch.url.host()).isNull()) {
            dropFetch(fetch.key);
            return;
        }
    } else if (!ircAvatarUrlIsSafe(fetch.url)) {
        dropFetch(fetch.key);
        return;
    }

    QNetworkRequest request(fetch.url);
    request.setTransferTimeout(kFetchTimeoutMs);
    request.setMaximumRedirectsAllowed(0);
    request.setAttribute(QNetworkRequest::RedirectPolicyAttribute,
                         QNetworkRequest::ManualRedirectPolicy);
    request.setAttribute(QNetworkRequest::CacheLoadControlAttribute,
                         QNetworkRequest::AlwaysNetwork);
    request.setRawHeader("Accept", "image/*");
    request.setHeader(QNetworkRequest::UserAgentHeader, QStringLiteral("Omairc"));
    if (!fetch.pinnedAddress.isNull()) {
        request.setAttribute(kPinnedAddressAttribute,
                             QVariant::fromValue(fetch.pinnedAddress));
    }

    QNetworkReply *reply = m_nam->get(request);
    reply->setParent(this);
    auto found = m_fetches.find(fetch.key);
    if (found == m_fetches.end()) {
        reply->abort();
        reply->deleteLater();
        return;
    }
    found->reply = reply;
    found->lookupId = -1;
    found->url = fetch.url;
    found->pinnedAddress = fetch.pinnedAddress;
    found->redirects = fetch.redirects;
    found->rawUrl = fetch.rawUrl;
    found->body.clear();

    QObject::connect(reply, &QNetworkReply::readyRead, this, [this, key = fetch.key]() {
        auto it = m_fetches.find(key);
        if (it == m_fetches.end() || !it->reply)
            return;
        it->body += it->reply->readAll();
        if (it->body.size() > kMaximumAvatarBytes) {
            it->reply->abort();
            dropFetch(key);
        }
    });
    QObject::connect(reply, &QNetworkReply::finished, this, [this, key = fetch.key]() {
        finishReply(key);
    });
}

void IrcAvatarStore::finishLookup(const QString& key, const QHostInfo& info)
{
    auto found = m_fetches.find(key);
    if (found == m_fetches.end())
        return;
    found->lookupId = -1;
    if (info.error() != QHostInfo::NoError || info.addresses().isEmpty()) {
        dropFetch(key);
        return;
    }
    applyResolvedAddresses(key, info.addresses());
}

void IrcAvatarStore::applyResolvedAddresses(const QString& key,
                                           const QList<QHostAddress>& addresses)
{
    // Pin the GET to one pre-validated address so QNAM cannot re-resolve the
    // hostname (DNS-rebinding TOCTOU). Dual-stack answers may mix safe and
    // unsafe addresses; only a safe global-unicast address is used.
    auto found = m_fetches.find(key);
    if (found == m_fetches.end())
        return;
    const QHostAddress pinned = ircSelectSafeAvatarAddress(addresses);
    if (pinned.isNull()) {
        dropFetch(key);
        return;
    }
    found->pinnedAddress = pinned;
    startGet(found.value());
}

void IrcAvatarStore::finishReply(const QString& key)
{
    auto found = m_fetches.find(key);
    if (found == m_fetches.end())
        return;
    QNetworkReply *reply = found->reply;
    found->reply = nullptr;
    if (!reply) {
        dropFetch(key);
        return;
    }

    const QUrl redirected = redirectTarget(reply);
    if (redirected.isValid()) {
        if (found->redirects >= kMaximumRedirects || !ircAvatarUrlIsSafe(redirected)) {
            reply->deleteLater();
            dropFetch(key);
            return;
        }
        Fetch next = found.value();
        next.url = redirected;
        next.pinnedAddress = QHostAddress();
        next.redirects += 1;
        next.body.clear();
        reply->deleteLater();
        lookupThenGet(next);
        return;
    }

    const QByteArray body = found->body + reply->readAll();
    const QString rawUrl = found->rawUrl;
    const int status =
        reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();
    const QByteArray contentType =
        reply->header(QNetworkRequest::ContentTypeHeader).toByteArray();
    const bool ok = reply->error() == QNetworkReply::NoError
        && status == 200
        && contentTypeIsImage(contentType)
        && body.size() > 0
        && body.size() <= kMaximumAvatarBytes;
    reply->deleteLater();
    m_fetches.remove(key);
    if (!ok)
        return;
    const QImage image = loadBoundedImage(body);
    if (image.isNull())
        return;
    remember(key, image);
    emit ready(rawUrl);
}

void IrcAvatarStore::dropFetch(const QString& key)
{
    auto found = m_fetches.find(key);
    if (found == m_fetches.end())
        return;
    if (found->lookupId >= 0)
        QHostInfo::abortHostLookup(found->lookupId);
    if (found->reply) {
        found->reply->disconnect(this);
        found->reply->abort();
        found->reply->deleteLater();
    }
    m_fetches.remove(key);
}

void IrcAvatarStore::remember(const QString& key, const QImage& image)
{
    QMutexLocker locker(&m_mutex);
    m_images.insert(key, image);
    m_order.removeAll(key);
    m_order.append(key);
    while (m_order.size() > kMaximumCacheEntries) {
        const QString oldest = m_order.takeFirst();
        m_images.remove(oldest);
    }
}

QImage IrcAvatarStore::cached(const QString& key) const
{
    QMutexLocker locker(&m_mutex);
    return m_images.value(key);
}

IrcAvatarStore *ircInstallAvatarStore(QQmlEngine *engine)
{
    if (!engine)
        return nullptr;
    auto *store = new IrcAvatarStore;
    engine->addImageProvider(kProviderId, store);
    return store;
}
