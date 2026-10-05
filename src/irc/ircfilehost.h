#pragma once

#include <QByteArray>
#include <QObject>
#include <QString>
#include <QStringList>

#include <optional>

class QMimeData;
class QNetworkAccessManager;
class QNetworkReply;

// One file the composer asked to upload. path is a local filesystem path.
// body is used instead when fromBytes is set (a pasted image has no path).
struct IrcFileUploadJob
{
    QString endpoint;
    QString path;
    QByteArray body;
    QString fileName;
    QString contentType;
    QString user;
    QString secret;
    QString serverHost;
    bool serverEncrypted = true;
    bool fromBytes = false;
};

// ircClipboardFiles describes a clipboard that should become an upload.
// paths are local files. png is set when the clipboard is an image and
// not ordinary text. A null return means the paste should stay text.
struct IrcClipboardOffer
{
    QStringList paths;
    QByteArray png;
};

class IrcFileUploader : public QObject
{
    Q_OBJECT

public:
    static constexpr qint64 kMaximumBytes = 32LL * 1024LL * 1024LL;

    explicit IrcFileUploader(QObject *parent = nullptr);

    void setNetworkAccessManager(QNetworkAccessManager *nam);
    void enqueue(IrcFileUploadJob job);
    bool busy() const;

signals:
    void linkReady(const QString &url);
    void failed(const QString &message);

private:
    void startNext();
    void finishCurrent(const QString &url, const QString &message);
    bool readJob(IrcFileUploadJob &job, QString *message) const;

    QNetworkAccessManager *m_nam = nullptr;
    QNetworkReply *m_reply = nullptr;
    QList<IrcFileUploadJob> m_queue;
    bool m_sending = false;
};

// ircClipboardOffer returns an upload when the clipboard holds files or an
// image. Ordinary text returns a null optional so the composer pastes it.
// The desktop build defines it. A gui-less test target does not.
#if defined(QT_GUI_LIB)
std::optional<IrcClipboardOffer> ircClipboardOffer(const QMimeData *mime);
#endif
