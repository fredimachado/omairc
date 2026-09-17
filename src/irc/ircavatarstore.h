#pragma once

#include <QHash>
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

class IrcAvatarStore : public QQuickImageProvider
{
    Q_OBJECT

public:
    explicit IrcAvatarStore(QObject *parent = nullptr);
    ~IrcAvatarStore() override;

    Q_INVOKABLE QString source(const QString& rawUrl, int pixelSize) const;
    void put(const QUrl& url, const QImage& image);
    void setNetworkAccessManager(QNetworkAccessManager *nam);

    QImage requestImage(const QString& id, QSize *size,
                        const QSize& requestedSize) override;

signals:
    void ready(const QString& rawUrl);

private:
    struct Fetch {
        QString rawUrl;
        QUrl url;
        QString key;
        int redirects = 0;
        int lookupId = -1;
        QNetworkReply *reply = nullptr;
        QByteArray body;
    };

    static QString keyForUrl(const QUrl& url);
    static QImage circled(const QImage& source, const QSize& requestedSize);

    void scheduleFetch(const QString& rawUrl, const QUrl& url, const QString& key) const;
    void lookupThenGet(Fetch fetch) const;
    void startGet(Fetch fetch) const;
    void finishLookup(const QString& key, const QHostInfo& info) const;
    void finishReply(const QString& key) const;
    void dropFetch(const QString& key) const;
    void remember(const QString& key, const QImage& image) const;
    QImage cached(const QString& key) const;

    QNetworkAccessManager *m_nam = nullptr;
    mutable QMutex m_mutex;
    mutable QHash<QString, QImage> m_images;
    mutable QList<QString> m_order;
    mutable QHash<QString, Fetch> m_fetches;
};

IrcAvatarStore *ircInstallAvatarStore(QQmlEngine *engine);
