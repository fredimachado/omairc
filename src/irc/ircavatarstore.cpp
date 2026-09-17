#include "ircavatarstore.h"

#include "ircavatarurl.h"

#include <QCryptographicHash>
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
            QImage image(trimmed);
            if (image.isNull())
                return {};
            remember(key, image);
            emit ready(trimmed);
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
    if (!ircAvatarUrlIsSafe(fetch.url)) {
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
    // Pre-connect lookup blocks literal private addresses, but QNAM resolves the
    // host again at GET time (DNS-rebinding TOCTOU). Any resolved address that
    // is not global unicast drops the fetch (fail-closed; dual-stack hosts with
    // one non-global address are rejected too).
    auto found = m_fetches.find(key);
    if (found == m_fetches.end())
        return;
    found->lookupId = -1;
    if (info.error() != QHostInfo::NoError || info.addresses().isEmpty()) {
        dropFetch(key);
        return;
    }
    for (const QHostAddress& address : info.addresses()) {
        if (ircHostAddressIsUnsafe(address)) {
            dropFetch(key);
            return;
        }
    }
    const Fetch fetch = found.value();
    startGet(fetch);
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
    QImage image;
    if (!image.loadFromData(body))
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
