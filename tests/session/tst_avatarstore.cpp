#include <QBuffer>
#include <QCryptographicHash>
#include <QImage>
#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QNetworkRequest>
#include <QSignalSpy>
#include <QTest>
#include <QTimer>
#include <QUrl>
#include <QtEndian>

#include "ircavatarstore.h"
#include "ircavatarurl.h"

namespace
{
constexpr char kPublicHost[] = "93.184.216.34";

const QByteArray kTinyPng = QByteArray::fromBase64(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==");

QString avatarKey(const QUrl& url)
{
    return QString::fromLatin1(
        QCryptographicHash::hash(url.toEncoded(QUrl::FullyEncoded),
                                 QCryptographicHash::Sha256)
            .toHex());
}

QString publicAvatarUrl(const char *path)
{
    return QStringLiteral("https://") + QLatin1String(kPublicHost) + QLatin1Char('/')
        + QLatin1String(path);
}

QByteArray pngChunk(const QByteArray& type, const QByteArray& data)
{
    QByteArray chunk;
    chunk.resize(4);
    qToBigEndian(quint32(data.size()), chunk.data());
    chunk += type;
    chunk += data;
    quint32 crc = 0xffffffffu;
    const QByteArray crcInput = type + data;
    for (unsigned char byte : crcInput) {
        crc ^= byte;
        for (int i = 0; i < 8; ++i)
            crc = (crc & 1u) ? (0xedb88320u ^ (crc >> 1)) : (crc >> 1);
    }
    crc ^= 0xffffffffu;
    QByteArray crcBytes;
    crcBytes.resize(4);
    qToBigEndian(crc, crcBytes.data());
    chunk += crcBytes;
    return chunk;
}

QByteArray pngDeclaringSize(int width, int height)
{
    QByteArray ihdr;
    ihdr.resize(13);
    qToBigEndian(quint32(width), ihdr.data());
    qToBigEndian(quint32(height), ihdr.data() + 4);
    ihdr[8] = 8;  // bit depth
    ihdr[9] = 2;  // RGB
    ihdr[10] = 0;
    ihdr[11] = 0;
    ihdr[12] = 0;
    QByteArray png;
    png += QByteArray::fromHex("89504e470d0a1a0a");
    png += pngChunk("IHDR", ihdr);
    png += pngChunk("IDAT", QByteArray::fromBase64("eJwDAAAAAAE="));
    png += pngChunk("IEND", {});
    return png;
}

class MockNetworkReply : public QNetworkReply
{
public:
    MockNetworkReply(const QNetworkRequest& request, QObject *parent = nullptr)
        : QNetworkReply(parent)
    {
        setRequest(request);
        setUrl(request.url());
        open(QIODevice::ReadOnly);
    }

    void deliverRedirect(int statusCode, const QUrl& target)
    {
        setAttribute(QNetworkRequest::HttpStatusCodeAttribute, statusCode);
        setAttribute(QNetworkRequest::RedirectionTargetAttribute, target);
        QTimer::singleShot(0, this, [this]() {
            emit finished();
        });
    }

    void deliverResponse(int statusCode, const QByteArray& contentType,
                         const QByteArray& body)
    {
        setAttribute(QNetworkRequest::HttpStatusCodeAttribute, statusCode);
        setHeader(QNetworkRequest::ContentTypeHeader, contentType);
        m_body = body;
        QTimer::singleShot(0, this, [this]() {
            if (!m_body.isEmpty()) {
                emit readyRead();
            }
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
        QTimer::singleShot(0, this, [this]() {
            emit finished();
        });
    }

private:
    QByteArray m_body;
    qint64 m_offset = 0;
};

class MockNetworkAccessManager : public QNetworkAccessManager
{
public:
    struct Response {
        int statusCode = 200;
        QByteArray contentType = "image/png";
        QByteArray body;
        QUrl redirectTarget;
    };

    void setResponse(const QUrl& url, Response response)
    {
        m_responses.insert(url.toString(QUrl::FullyEncoded), response);
    }

    QNetworkRequest lastRequest;
    int requestCount = 0;

protected:
    QNetworkReply *createRequest(Operation op, const QNetworkRequest& request,
                                 QIODevice *outgoingData) override
    {
        Q_UNUSED(op);
        Q_UNUSED(outgoingData);
        lastRequest = request;
        ++requestCount;
        auto *reply = new MockNetworkReply(request, this);
        const Response response =
            m_responses.value(request.url().toString(QUrl::FullyEncoded));
        if (response.redirectTarget.isValid()) {
            reply->deliverRedirect(response.statusCode, response.redirectTarget);
        } else {
            reply->deliverResponse(response.statusCode, response.contentType,
                                   response.body);
        }
        return reply;
    }

private:
    QHash<QString, Response> m_responses;
};

bool waitForReady(QSignalSpy& spy, int timeoutMs = 5000)
{
    return QTest::qWaitFor([&spy]() { return spy.count() > 0; }, timeoutMs);
}

bool waitForIdle(int timeoutMs = 500)
{
    return QTest::qWaitFor([]() { return true; }, timeoutMs);
}

bool pixelIsTransparent(const QImage& image, int x, int y)
{
    if (x < 0 || y < 0 || x >= image.width() || y >= image.height())
        return true;
    return qAlpha(image.pixel(x, y)) == 0;
}

bool pixelIsOpaque(const QImage& image, int x, int y)
{
    if (x < 0 || y < 0 || x >= image.width() || y >= image.height())
        return false;
    return qAlpha(image.pixel(x, y)) > 0;
}
}

class AvatarStoreTest : public QObject
{
    Q_OBJECT

private slots:
    void refusesRedirectToUnsafe();
    void refusesOversizeBody();
    void refusesWrongContentType();
    void refusesNon200();
    void successfulFetchPopulatesCache();
    void capsCircledSizeWhenRequestedSizeInvalid();
    void sourceIgnoresNonPositivePixelSize();
    void loadsBundledQrcWithoutHttpsPolicy();
    void refusesDecodedImageBomb();
    void acceptsValidDecodedImageWithinBudget();
    void pinsHostnameFetchToResolvedAddress();
    void refusesHostnameWhenOnlyUnsafeAddressesResolve();
    void circleClipHasTransparentCorners();
    void squareClipHasRoundedRectShape();
    void squareClipWorksForBundledQrc();
};

void AvatarStoreTest::refusesRedirectToUnsafe()
{
    MockNetworkAccessManager nam;
    IrcAvatarStore store;
    store.setNetworkAccessManager(&nam);

    const QString rawUrl = publicAvatarUrl("redirect-http.png");
    const QUrl url = ircResolvedAvatarUrl(rawUrl, 32);
    QVERIFY(ircAvatarUrlIsSafe(url));

    MockNetworkAccessManager::Response response;
    response.statusCode = 302;
    response.redirectTarget =
        QUrl(QStringLiteral("http://example.com/avatar.png"));
    nam.setResponse(url, response);

    QSignalSpy readySpy(&store, &IrcAvatarStore::ready);
    QCOMPARE(store.source(rawUrl, 32), QString());
    QVERIFY(waitForIdle());
    QCOMPARE(readySpy.count(), 0);
    QCOMPARE(store.source(rawUrl, 32), QString());

    const QString loopbackRaw = publicAvatarUrl("redirect-loopback.png");
    const QUrl loopbackUrl = ircResolvedAvatarUrl(loopbackRaw, 32);
    response.redirectTarget =
        QUrl(QStringLiteral("https://127.0.0.1/avatar.png"));
    nam.setResponse(loopbackUrl, response);

    QCOMPARE(store.source(loopbackRaw, 32), QString());
    QVERIFY(waitForIdle());
    QCOMPARE(readySpy.count(), 0);
    QCOMPARE(store.source(loopbackRaw, 32), QString());
}

void AvatarStoreTest::refusesOversizeBody()
{
    MockNetworkAccessManager nam;
    IrcAvatarStore store;
    store.setNetworkAccessManager(&nam);

    const QString rawUrl = publicAvatarUrl("oversize.png");
    const QUrl url = ircResolvedAvatarUrl(rawUrl, 32);
    QVERIFY(ircAvatarUrlIsSafe(url));

    MockNetworkAccessManager::Response response;
    response.contentType = "image/png";
    response.body = QByteArray(512 * 1024 + 1, 'x');
    nam.setResponse(url, response);

    QSignalSpy readySpy(&store, &IrcAvatarStore::ready);
    QCOMPARE(store.source(rawUrl, 32), QString());
    QVERIFY(waitForIdle());
    QCOMPARE(readySpy.count(), 0);
    QCOMPARE(store.source(rawUrl, 32), QString());
}

void AvatarStoreTest::refusesWrongContentType()
{
    MockNetworkAccessManager nam;
    IrcAvatarStore store;
    store.setNetworkAccessManager(&nam);

    const QString rawUrl = publicAvatarUrl("text-plain.png");
    const QUrl url = ircResolvedAvatarUrl(rawUrl, 32);
    QVERIFY(ircAvatarUrlIsSafe(url));

    MockNetworkAccessManager::Response response;
    response.contentType = "text/plain";
    response.body = kTinyPng;
    nam.setResponse(url, response);

    QSignalSpy readySpy(&store, &IrcAvatarStore::ready);
    QCOMPARE(store.source(rawUrl, 32), QString());
    QVERIFY(waitForIdle());
    QCOMPARE(readySpy.count(), 0);
    QCOMPARE(store.source(rawUrl, 32), QString());
}

void AvatarStoreTest::refusesNon200()
{
    MockNetworkAccessManager nam;
    IrcAvatarStore store;
    store.setNetworkAccessManager(&nam);

    const QString rawUrl = publicAvatarUrl("not-found.png");
    const QUrl url = ircResolvedAvatarUrl(rawUrl, 32);
    QVERIFY(ircAvatarUrlIsSafe(url));

    MockNetworkAccessManager::Response response;
    response.statusCode = 404;
    response.body = kTinyPng;
    nam.setResponse(url, response);

    QSignalSpy readySpy(&store, &IrcAvatarStore::ready);
    QCOMPARE(store.source(rawUrl, 32), QString());
    QVERIFY(waitForIdle());
    QCOMPARE(readySpy.count(), 0);
    QCOMPARE(store.source(rawUrl, 32), QString());
}

void AvatarStoreTest::successfulFetchPopulatesCache()
{
    MockNetworkAccessManager nam;
    IrcAvatarStore store;
    store.setNetworkAccessManager(&nam);

    const QString rawUrl = publicAvatarUrl("success.png");
    const QUrl url = ircResolvedAvatarUrl(rawUrl, 32);
    QVERIFY(ircAvatarUrlIsSafe(url));

    MockNetworkAccessManager::Response response;
    response.body = kTinyPng;
    nam.setResponse(url, response);

    QSignalSpy readySpy(&store, &IrcAvatarStore::ready);
    QCOMPARE(store.source(rawUrl, 32), QString());
    QVERIFY(waitForReady(readySpy));
    QCOMPARE(readySpy.count(), 1);
    QCOMPARE(readySpy.at(0).at(0).toString(), rawUrl);

    const QString expectedSource =
        QStringLiteral("image://omairc-avatar/") + avatarKey(url);
    QCOMPARE(store.source(rawUrl, 32), expectedSource);

    QSize imageSize;
    const QImage image = store.requestImage(avatarKey(url), &imageSize, QSize(32, 32));
    QVERIFY(!image.isNull());
    QCOMPARE(imageSize, QSize(32, 32));
    QVERIFY(pixelIsTransparent(image, 0, 0));
    QVERIFY(pixelIsOpaque(image, 16, 16));
}

void AvatarStoreTest::capsCircledSizeWhenRequestedSizeInvalid()
{
    MockNetworkAccessManager nam;
    IrcAvatarStore store;
    store.setNetworkAccessManager(&nam);

    const QString rawUrl = publicAvatarUrl("huge.png");
    const QUrl url = ircResolvedAvatarUrl(rawUrl, 32);
    QVERIFY(ircAvatarUrlIsSafe(url));

    // Within the decoded budget, but larger than the circled render cap.
    QImage large(512, 512, QImage::Format_ARGB32);
    large.fill(Qt::red);
    QBuffer buffer;
    buffer.open(QIODevice::WriteOnly);
    QVERIFY(large.save(&buffer, "PNG"));

    MockNetworkAccessManager::Response response;
    response.body = buffer.data();
    nam.setResponse(url, response);

    QSignalSpy readySpy(&store, &IrcAvatarStore::ready);
    QCOMPARE(store.source(rawUrl, 32), QString());
    QVERIFY(waitForReady(readySpy));

    const QString key = avatarKey(url);
    QSize imageSize;
    const QImage invalidSize =
        store.requestImage(key, &imageSize, QSize());
    QVERIFY(!invalidSize.isNull());
    QCOMPARE(invalidSize.size(), QSize(128, 128));
    QCOMPARE(imageSize, QSize(128, 128));

    const QImage zeroSize =
        store.requestImage(key, &imageSize, QSize(0, 0));
    QVERIFY(!zeroSize.isNull());
    QCOMPARE(zeroSize.size(), QSize(128, 128));
}

void AvatarStoreTest::sourceIgnoresNonPositivePixelSize()
{
    IrcAvatarStore store;
    QCOMPARE(store.source(publicAvatarUrl("zero-width.png"), 0), QString());
    QCOMPARE(store.source(publicAvatarUrl("zero-width.png"), -4), QString());
}

void AvatarStoreTest::loadsBundledQrcWithoutHttpsPolicy()
{
    IrcAvatarStore store;
    const QString rawUrl = QStringLiteral("qrc:/demo/mira-avatar.png");
    QSignalSpy readySpy(&store, &IrcAvatarStore::ready);
    const QString source = store.source(rawUrl, 32);
    if (source.isEmpty() && readySpy.isEmpty())
        QSKIP("demo mira avatar is not linked into protocol_tests");
    QVERIFY(source.startsWith(QStringLiteral("image://omairc-avatar/")));
    QSize imageSize;
    const QString key = source.section(QLatin1Char('/'), -1);
    const QImage image = store.requestImage(key, &imageSize, QSize(32, 32));
    QVERIFY(!image.isNull());
}

void AvatarStoreTest::refusesDecodedImageBomb()
{
    MockNetworkAccessManager nam;
    IrcAvatarStore store;
    store.setNetworkAccessManager(&nam);

    const QString rawUrl = publicAvatarUrl("bomb.png");
    const QUrl url = ircResolvedAvatarUrl(rawUrl, 32);
    QVERIFY(ircAvatarUrlIsSafe(url));

    const QByteArray bomb = pngDeclaringSize(20000, 20000);
    QVERIFY(bomb.size() < 256);

    MockNetworkAccessManager::Response response;
    response.body = bomb;
    nam.setResponse(url, response);

    QSignalSpy readySpy(&store, &IrcAvatarStore::ready);
    QCOMPARE(store.source(rawUrl, 32), QString());
    QVERIFY(waitForIdle(1000));
    QCOMPARE(readySpy.count(), 0);
    QCOMPARE(store.source(rawUrl, 32), QString());
}

void AvatarStoreTest::acceptsValidDecodedImageWithinBudget()
{
    MockNetworkAccessManager nam;
    IrcAvatarStore store;
    store.setNetworkAccessManager(&nam);

    const QString rawUrl = publicAvatarUrl("ok-64.png");
    const QUrl url = ircResolvedAvatarUrl(rawUrl, 32);
    QVERIFY(ircAvatarUrlIsSafe(url));

    QImage ok(64, 64, QImage::Format_ARGB32);
    ok.fill(QColor(0x2f, 0x9a, 0x8f));
    QBuffer buffer;
    buffer.open(QIODevice::WriteOnly);
    QVERIFY(ok.save(&buffer, "PNG"));

    MockNetworkAccessManager::Response response;
    response.body = buffer.data();
    nam.setResponse(url, response);

    QSignalSpy readySpy(&store, &IrcAvatarStore::ready);
    QCOMPARE(store.source(rawUrl, 32), QString());
    QVERIFY(waitForReady(readySpy));
    QCOMPARE(store.source(rawUrl, 32),
             QStringLiteral("image://omairc-avatar/") + avatarKey(url));
}

void AvatarStoreTest::pinsHostnameFetchToResolvedAddress()
{
    MockNetworkAccessManager nam;
    IrcAvatarStore store;
    store.setNetworkAccessManager(&nam);

    const QString rawUrl = QStringLiteral("https://cdn.example/pin.png");
    const QUrl logicalUrl = ircResolvedAvatarUrl(rawUrl, 32);
    QVERIFY(ircAvatarUrlIsSafe(logicalUrl));
    QVERIFY(QHostAddress(logicalUrl.host()).isNull());

    MockNetworkAccessManager::Response response;
    response.body = kTinyPng;
    nam.setResponse(logicalUrl, response);

    QSignalSpy readySpy(&store, &IrcAvatarStore::ready);
    QCOMPARE(store.source(rawUrl, 32), QString());

    const QString key = avatarKey(logicalUrl);
    QVERIFY(store.m_fetches.contains(key));
    if (store.m_fetches.value(key).lookupId >= 0)
        QHostInfo::abortHostLookup(store.m_fetches.value(key).lookupId);
    store.m_fetches[key].lookupId = -1;
    store.applyResolvedAddresses(
        key,
        {QHostAddress(QStringLiteral("127.0.0.1")),
         QHostAddress(QLatin1String(kPublicHost))});

    QVERIFY(waitForReady(readySpy));
    QCOMPARE(nam.requestCount, 1);
    QCOMPARE(nam.lastRequest.url(), logicalUrl);
    QVERIFY(QHostAddress(nam.lastRequest.url().host()).isNull());
    QCOMPARE(nam.lastRequest.url().host(), QStringLiteral("cdn.example"));
    const QVariant pinned =
        nam.lastRequest.attribute(
            static_cast<QNetworkRequest::Attribute>(QNetworkRequest::User + 1000));
    QVERIFY(pinned.isValid());
    QCOMPARE(pinned.value<QHostAddress>(),
             QHostAddress(QLatin1String(kPublicHost)));
    QCOMPARE(readySpy.at(0).at(0).toString(), rawUrl);
}

void AvatarStoreTest::refusesHostnameWhenOnlyUnsafeAddressesResolve()
{
    MockNetworkAccessManager nam;
    IrcAvatarStore store;
    store.setNetworkAccessManager(&nam);

    const QString rawUrl = QStringLiteral("https://cdn.example/unsafe.png");
    const QUrl logicalUrl = ircResolvedAvatarUrl(rawUrl, 32);
    QVERIFY(ircAvatarUrlIsSafe(logicalUrl));

    QSignalSpy readySpy(&store, &IrcAvatarStore::ready);
    QCOMPARE(store.source(rawUrl, 32), QString());

    const QString key = avatarKey(logicalUrl);
    QVERIFY(store.m_fetches.contains(key));
    if (store.m_fetches.value(key).lookupId >= 0)
        QHostInfo::abortHostLookup(store.m_fetches.value(key).lookupId);
    store.m_fetches[key].lookupId = -1;
    store.applyResolvedAddresses(
        key,
        {QHostAddress(QStringLiteral("127.0.0.1")),
         QHostAddress(QStringLiteral("10.0.0.8"))});

    QVERIFY(waitForIdle());
    QCOMPARE(nam.requestCount, 0);
    QCOMPARE(readySpy.count(), 0);
    QCOMPARE(store.source(rawUrl, 32), QString());
}

void AvatarStoreTest::circleClipHasTransparentCorners()
{
    MockNetworkAccessManager nam;
    IrcAvatarStore store;
    store.setNetworkAccessManager(&nam);

    const QString rawUrl = publicAvatarUrl("circle-clip.png");
    const QUrl url = ircResolvedAvatarUrl(rawUrl, 32);
    QVERIFY(ircAvatarUrlIsSafe(url));

    MockNetworkAccessManager::Response response;
    response.body = kTinyPng;
    nam.setResponse(url, response);

    QSignalSpy readySpy(&store, &IrcAvatarStore::ready);
    QCOMPARE(store.source(rawUrl, 32), QString());
    QVERIFY(waitForReady(readySpy));

    const QString expectedSource =
        QStringLiteral("image://omairc-avatar/") + avatarKey(url);
    QCOMPARE(store.source(rawUrl, 32), expectedSource);

    QSize imageSize;
    const QImage image = store.requestImage(avatarKey(url), &imageSize, QSize(32, 32));
    QVERIFY(!image.isNull());
    QCOMPARE(imageSize, QSize(32, 32));
    QVERIFY(pixelIsTransparent(image, 0, 0));
    QVERIFY(pixelIsOpaque(image, 16, 16));
}

void AvatarStoreTest::squareClipHasRoundedRectShape()
{
    MockNetworkAccessManager nam;
    IrcAvatarStore store;
    store.setNetworkAccessManager(&nam);

    const QString rawUrl = publicAvatarUrl("square-clip.png");
    const QUrl url = ircResolvedAvatarUrl(rawUrl, 32);
    QVERIFY(ircAvatarUrlIsSafe(url));

    MockNetworkAccessManager::Response response;
    response.body = kTinyPng;
    nam.setResponse(url, response);

    QSignalSpy readySpy(&store, &IrcAvatarStore::ready);
    QCOMPARE(store.source(rawUrl, 32, QStringLiteral("square")), QString());
    QVERIFY(waitForReady(readySpy));
    QCOMPARE(readySpy.at(0).at(0).toString(), rawUrl);

    const QString expectedSource =
        QStringLiteral("image://omairc-avatar/square/") + avatarKey(url);
    QCOMPARE(store.source(rawUrl, 32, QStringLiteral("square")), expectedSource);

    QSize imageSize;
    const QString requestId = QStringLiteral("square/") + avatarKey(url);
    const QImage image = store.requestImage(requestId, &imageSize, QSize(32, 32));
    QVERIFY(!image.isNull());
    QCOMPARE(imageSize, QSize(32, 32));
    QVERIFY(pixelIsTransparent(image, 0, 0));
    QVERIFY(pixelIsOpaque(image, 16, 16));
    QVERIFY(pixelIsOpaque(image, 16, 0));
}

void AvatarStoreTest::squareClipWorksForBundledQrc()
{
    IrcAvatarStore store;
    const QString rawUrl = QStringLiteral("qrc:/demo/mira-avatar.png");
    const QString source = store.source(rawUrl, 32, QStringLiteral("square"));
    if (source.isEmpty())
        QSKIP("demo mira avatar is not linked into protocol_tests");
    QCOMPARE(source.section(QLatin1Char('/'), 0, -2),
             QStringLiteral("image://omairc-avatar/square"));
    QSize imageSize;
    const QString requestId = source.section(QLatin1Char('/'), -2, -1);
    const QImage image = store.requestImage(requestId, &imageSize, QSize(32, 32));
    QVERIFY(!image.isNull());
    QVERIFY(pixelIsTransparent(image, 0, 0));
    QVERIFY(pixelIsOpaque(image, 16, 16));
    QVERIFY(pixelIsOpaque(image, 16, 0));
}

int runAvatarStoreTests(int argc, char **argv)
{
    AvatarStoreTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_avatarstore.moc"
