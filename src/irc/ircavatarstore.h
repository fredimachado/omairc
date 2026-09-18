#pragma once

#include <QHash>
#include <QHostAddress>
#include <QHostInfo>
#include <QImage>
#include <QList>
#include <QMutex>
#include <QObject>
#include <QQuickImageProvider>
#include <QString>
#include <QUrl>

class QNetworkAccessManager;
class QNetworkReply;
class QQmlEngine;
class AvatarStoreTest;

class IrcAvatarStore : public QQuickImageProvider
{
    Q_OBJECT

public:
    explicit IrcAvatarStore(QObject *parent = nullptr);
    ~IrcAvatarStore() override;

    Q_INVOKABLE QString source(const QString& rawUrl, int pixelSize);
    Q_INVOKABLE QString source(const QString& rawUrl, int pixelSize,
                               const QString& clip);
    void setNetworkAccessManager(QNetworkAccessManager *nam);

    QImage requestImage(const QString& id, QSize *size,
                        const QSize& requestedSize) override;

signals:
    void ready(const QString& rawUrl);

private:
    friend class AvatarStoreTest;

    struct Fetch {
        QString rawUrl;
        QUrl url;
        QString key;
        QString tlsHost;
        int redirects = 0;
        int lookupId = -1;
        QNetworkReply *reply = nullptr;
        QByteArray body;
    };

    static QString keyForUrl(const QUrl& url);
    static QImage circled(const QImage& source, const QSize& requestedSize);
    static QImage roundedRect(const QImage& source, const QSize& requestedSize);
    static QImage loadBoundedImage(const QByteArray& body);
    static QImage loadBoundedImageFromFile(const QString& path);

    void scheduleFetch(const QString& rawUrl, const QUrl& url, const QString& key);
    void lookupThenGet(Fetch fetch);
    void startGet(Fetch fetch);
    void finishLookup(const QString& key, const QHostInfo& info);
    void applyResolvedAddresses(const QString& key,
                                const QList<QHostAddress>& addresses);
    void finishReply(const QString& key);
    void dropFetch(const QString& key);
    void remember(const QString& key, const QImage& image);
    QImage cached(const QString& key) const;

    QNetworkAccessManager *m_nam = nullptr;
    mutable QMutex m_mutex;
    mutable QHash<QString, QImage> m_images;
    mutable QList<QString> m_order;
    QHash<QString, Fetch> m_fetches;
};

IrcAvatarStore *ircInstallAvatarStore(QQmlEngine *engine);
