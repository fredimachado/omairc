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
    , m_nam(new QNetworkAccessManager(this))
{
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
        return QStringLiteral("image://") + kProviderId + QLatin1Char('/') + key;
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
    return QStringLiteral("image://") + kProviderId + QLatin1Char('/') + key;
}

QImage IrcAvatarStore::requestImage(const QString& id, QSize *size,
                                    const QSize& requestedSize)
{
    const QImage image = cached(id);
    if (image.isNull())
        return {};
    const QImage framed = circled(image, requestedSize);
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
        found->tlsHost.clear();
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
    if (fetch.tlsHost.isEmpty()) {
        if (!ircAvatarUrlIsSafe(fetch.url)) {
            dropFetch(fetch.key);
            return;
        }
    } else {
        // URL host is the pinned IP; original hostname is only for TLS/Host.
        const QHostAddress pinned(fetch.url.host());
        if (pinned.isNull() || ircHostAddressIsUnsafe(pinned)) {
            dropFetch(fetch.key);
            return;
        }
        if (fetch.url.scheme().compare(QLatin1String("https"), Qt::CaseInsensitive)
            != 0) {
            dropFetch(fetch.key);
            return;
        }
        const int port = fetch.url.port(-1);
        if (port != -1 && port != 443) {
            dropFetch(fetch.key);
            return;
        }
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
    if (!fetch.tlsHost.isEmpty()) {
        // Connect to the pinned IP in the URL; verify/SNI against the original
        // hostname so QNAM never gets a second chance to resolve it.
        request.setPeerVerifyName(fetch.tlsHost);
        QByteArray hostHeader = fetch.tlsHost.toUtf8();
        const int port = fetch.url.port(-1);
        if (port != -1 && port != 443)
            hostHeader += ':' + QByteArray::number(port);
        request.setRawHeader("Host", hostHeader);
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
    found->tlsHost = fetch.tlsHost;
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
    const QString originalHost = found->url.host();
    const QUrl pinnedUrl = ircAvatarUrlPinnedToAddress(found->url, pinned);
    if (!pinnedUrl.isValid()) {
        dropFetch(key);
        return;
    }
    found->tlsHost = originalHost;
    found->url = pinnedUrl;
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
        next.tlsHost.clear();
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
