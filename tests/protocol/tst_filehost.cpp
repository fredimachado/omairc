#include <QBuffer>
#include <QFile>
#include <QHostAddress>
#include <QImage>
#include <QMimeData>
#include <QNetworkAccessManager>
#include <QSignalSpy>
#include <QTcpServer>
#include <QTcpSocket>
#include <QTemporaryDir>
#include <QTemporaryFile>
#include <QTest>
#include <QUrl>

#include "ircfilehost.h"
#include "ircfilelink.h"

namespace
{
bool requestComplete(const QByteArray &raw)
{
    const int headerEnd = raw.indexOf("\r\n\r\n");
    if (headerEnd < 0)
        return false;
    const QByteArray headers = raw.left(headerEnd).toLower();
    const QByteArray needle = "content-length:";
    const int at = headers.indexOf(needle);
    if (at < 0)
        return true;
    int end = headers.indexOf("\r\n", at);
    if (end < 0)
        end = headers.size();
    bool ok = false;
    const int length = headers.mid(at + needle.size(), end - at - needle.size())
                           .trimmed()
                           .toInt(&ok);
    return ok && raw.size() >= headerEnd + 4 + length;
}

struct UploadCatcher
{
    QByteArray captured;
    int connections = 0;
    int status = 201;
    QByteArray location = "/upload/hoh5eFThae4e.txt";
    QByteArray payload;
};

void armServer(QTcpServer &server, UploadCatcher &caught)
{
    QObject::connect(&server, &QTcpServer::newConnection, &server, [&server, &caught] {
        ++caught.connections;
        QTcpSocket *socket = server.nextPendingConnection();
        auto *buffer = new QByteArray;
        QObject::connect(socket, &QTcpSocket::readyRead, socket, [socket, buffer, &caught] {
            buffer->append(socket->readAll());
            if (!requestComplete(*buffer))
                return;
            caught.captured += *buffer;
            QByteArray response = "HTTP/1.1 " + QByteArray::number(caught.status) + " X\r\n";
            if (!caught.location.isEmpty())
                response += "Location: " + caught.location + "\r\n";
            response += "Content-Length: " + QByteArray::number(caught.payload.size()) + "\r\n";
            response += "Connection: close\r\n\r\n";
            response += caught.payload;
            socket->write(response);
            socket->disconnectFromHost();
        });
        QObject::connect(socket, &QTcpSocket::disconnected, socket, [socket, buffer] {
            delete buffer;
            socket->deleteLater();
        });
    });
}

QString writeNote(QTemporaryDir &dir, const QByteArray &body)
{
    const QString path = dir.filePath(QStringLiteral("note.txt"));
    QFile file(path);
    file.open(QIODevice::WriteOnly);
    file.write(body);
    file.close();
    return path;
}
}

class FileHostTest : public QObject
{
    Q_OBJECT

private slots:
    void resolvesARelativeLocation();
    void refusesANonWebLocation();
    void basicAuthorizationMatchesTheSpecVector();
    void authStaysOnTheConnectedHost();
    void uploadPostsTheFileAndResolvesLocation();
    void uploadOmitsAuthForADifferentHost();
    void uploadDoesNotFollowRedirectsOrLeakTheSecret();
    void uploadRefusesCleartextWhenTheConnectionIsEncrypted();
    void uploadRejectsEmptyDirectoryAndHugeFiles();
    void clipboardFilesAndImages();
};

void FileHostTest::resolvesARelativeLocation()
{
    const auto link = absoluteFileLink("https://irc.example.org/upload",
                                       "/upload/hoh5eFThae4e.jpeg");
    QVERIFY(link.has_value());
    QCOMPARE(QString::fromStdString(*link),
             QStringLiteral("https://irc.example.org/upload/hoh5eFThae4e.jpeg"));

    const auto absolute = absoluteFileLink("https://irc.example.org/upload",
                                           "https://cdn.example/a.png");
    QVERIFY(absolute.has_value());
    QCOMPARE(QString::fromStdString(*absolute), QStringLiteral("https://cdn.example/a.png"));

    const auto http = absoluteFileLink("https://irc.example.org/upload",
                                       "http://cdn.example/a.png");
    QVERIFY(http.has_value());
    QCOMPARE(QString::fromStdString(*http), QStringLiteral("http://cdn.example/a.png"));

    const auto fragment = absoluteFileLink("https://irc.example.org/upload", "/a.png#part");
    QVERIFY(fragment.has_value());
    QCOMPARE(QString::fromStdString(*fragment),
             QStringLiteral("https://irc.example.org/a.png"));
    QVERIFY(!absoluteFileLink("", "/a").has_value());
    QVERIFY(!absoluteFileLink("https://irc.example.org/upload", "").has_value());
}

void FileHostTest::refusesANonWebLocation()
{
    QVERIFY(!absoluteFileLink("https://irc.example.org/upload", "javascript:alert(1)").has_value());
    QVERIFY(!absoluteFileLink("https://irc.example.org/upload",
                              "https://user:pw@cdn.example/a.png").has_value());
}

void FileHostTest::basicAuthorizationMatchesTheSpecVector()
{
    QCOMPARE(QString::fromStdString(basicAuthorizationValue("seunghye", "no")),
             QStringLiteral("Basic c2V1bmdoeWU6bm8="));
    QVERIFY(basicAuthorizationValue("", "no").empty());
    QVERIFY(basicAuthorizationValue("seunghye", "").empty());
    QVERIFY(basicAuthorizationValue("bad\nname", "no").empty());
}

void FileHostTest::authStaysOnTheConnectedHost()
{
    QVERIFY(fileHostSendsBasicAuth("irc.example", "https://irc.example/upload", true));
    QVERIFY(!fileHostSendsBasicAuth("irc.example", "http://irc.example/upload", true));
    QVERIFY(fileHostSendsBasicAuth("irc.example", "http://IRC.EXAMPLE./upload", false));
    QVERIFY(!fileHostSendsBasicAuth("irc.example", "https://uploads.example/upload", true));
    QVERIFY(!fileHostSendsBasicAuth("irc.example", "https://user:pw@irc.example/upload", true));
    QVERIFY(!fileHostSendsBasicAuth("", "https://irc.example/upload", true));
}

void FileHostTest::uploadPostsTheFileAndResolvesLocation()
{
    QTcpServer server;
    QVERIFY(server.listen(QHostAddress::LocalHost, 0));
    UploadCatcher caught;
    armServer(server, caught);
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString path = writeNote(dir, "hello file");

    QNetworkAccessManager nam;
    IrcFileUploader uploader;
    uploader.setNetworkAccessManager(&nam);
    QSignalSpy ready(&uploader, &IrcFileUploader::linkReady);
    IrcFileUploadJob job;
    job.endpoint = QStringLiteral("http://127.0.0.1:%1/upload").arg(server.serverPort());
    job.path = path;
    job.user = QStringLiteral("seunghye");
    job.secret = QStringLiteral("no");
    job.serverHost = QStringLiteral("127.0.0.1");
    job.serverEncrypted = false;
    uploader.enqueue(std::move(job));

    QTRY_COMPARE(ready.size(), 1);
    QCOMPARE(ready.at(0).at(0).toString(),
             QStringLiteral("http://127.0.0.1:%1/upload/hoh5eFThae4e.txt")
                 .arg(server.serverPort()));
    // Qt writes header names in lowercase.
    QVERIFY(caught.captured.contains("authorization: Basic c2V1bmdoeWU6bm8="));
    QVERIFY(caught.captured.contains("content-type: text/plain"));
    QVERIFY(caught.captured.contains("content-disposition: attachment; filename=\"note.txt\""));
    QVERIFY(caught.captured.contains("hello file"));
}

void FileHostTest::uploadOmitsAuthForADifferentHost()
{
    QTcpServer server;
    QVERIFY(server.listen(QHostAddress::LocalHost, 0));
    UploadCatcher caught;
    armServer(server, caught);
    QTemporaryDir dir;
    QVERIFY(dir.isValid());

    QNetworkAccessManager nam;
    IrcFileUploader uploader;
    uploader.setNetworkAccessManager(&nam);
    QSignalSpy ready(&uploader, &IrcFileUploader::linkReady);
    IrcFileUploadJob job;
    job.endpoint = QStringLiteral("http://127.0.0.1:%1/upload").arg(server.serverPort());
    job.path = writeNote(dir, "x");
    job.user = QStringLiteral("alice");
    job.secret = QStringLiteral("s3cret-token");
    job.serverHost = QStringLiteral("irc.example");
    job.serverEncrypted = false;
    uploader.enqueue(std::move(job));

    QTRY_COMPARE(ready.size(), 1);
    QVERIFY(!caught.captured.toLower().contains("authorization:"));
    QVERIFY(!caught.captured.contains("s3cret-token"));
}

void FileHostTest::uploadDoesNotFollowRedirectsOrLeakTheSecret()
{
    QTcpServer server;
    QVERIFY(server.listen(QHostAddress::LocalHost, 0));
    UploadCatcher caught;
    caught.status = 302;
    caught.location = "http://127.0.0.1:" + QByteArray::number(server.serverPort()) + "/stolen";
    caught.payload = "s3cret-token";
    armServer(server, caught);
    QTemporaryDir dir;
    QVERIFY(dir.isValid());

    QNetworkAccessManager nam;
    IrcFileUploader uploader;
    uploader.setNetworkAccessManager(&nam);
    QSignalSpy failed(&uploader, &IrcFileUploader::failed);
    IrcFileUploadJob job;
    job.endpoint = QStringLiteral("http://127.0.0.1:%1/upload").arg(server.serverPort());
    job.path = writeNote(dir, "x");
    job.user = QStringLiteral("alice");
    job.secret = QStringLiteral("s3cret-token");
    job.serverHost = QStringLiteral("127.0.0.1");
    job.serverEncrypted = false;
    uploader.enqueue(std::move(job));

    QTRY_COMPARE(failed.size(), 1);
    const QString message = failed.at(0).at(0).toString();
    QCOMPARE(message, QStringLiteral("Could not upload the file."));
    QVERIFY(!message.contains(QStringLiteral("s3cret-token")));
    QVERIFY(!message.contains(QStringLiteral("http://")));
    QCOMPARE(caught.connections, 1);
}

void FileHostTest::uploadRefusesCleartextWhenTheConnectionIsEncrypted()
{
    QTcpServer server;
    QVERIFY(server.listen(QHostAddress::LocalHost, 0));
    UploadCatcher caught;
    armServer(server, caught);
    QTemporaryDir dir;
    QVERIFY(dir.isValid());

    QNetworkAccessManager nam;
    IrcFileUploader uploader;
    uploader.setNetworkAccessManager(&nam);
    QSignalSpy failed(&uploader, &IrcFileUploader::failed);
    IrcFileUploadJob job;
    job.endpoint = QStringLiteral("http://127.0.0.1:%1/upload").arg(server.serverPort());
    job.path = writeNote(dir, "x");
    job.serverEncrypted = true;
    uploader.enqueue(std::move(job));

    QCOMPARE(failed.size(), 1);
    QCOMPARE(failed.at(0).at(0).toString(), QStringLiteral("Could not upload the file."));
    QCOMPARE(caught.connections, 0);
}

void FileHostTest::uploadRejectsEmptyDirectoryAndHugeFiles()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString emptyPath = writeNote(dir, "");
    QNetworkAccessManager nam;
    IrcFileUploader uploader;
    uploader.setNetworkAccessManager(&nam);

    QSignalSpy failed(&uploader, &IrcFileUploader::failed);
    IrcFileUploadJob empty;
    empty.endpoint = QStringLiteral("https://irc.example/upload");
    empty.path = emptyPath;
    empty.serverEncrypted = true;
    uploader.enqueue(std::move(empty));
    QCOMPARE(failed.size(), 1);
    QCOMPARE(failed.at(0).at(0).toString(), QStringLiteral("The file is empty."));

    IrcFileUploadJob directory;
    directory.endpoint = QStringLiteral("https://irc.example/upload");
    directory.path = dir.path();
    directory.serverEncrypted = true;
    uploader.enqueue(std::move(directory));
    QCOMPARE(failed.size(), 2);
    QCOMPARE(failed.at(1).at(0).toString(), QStringLiteral("That is not a file."));

    QTemporaryFile huge(dir.filePath(QStringLiteral("huge-XXXXXX.bin")));
    QVERIFY(huge.open());
    QVERIFY(huge.resize(IrcFileUploader::kMaximumBytes + 1));
    huge.close();
    IrcFileUploadJob tooLarge;
    tooLarge.endpoint = QStringLiteral("https://irc.example/upload");
    tooLarge.path = huge.fileName();
    tooLarge.serverEncrypted = true;
    uploader.enqueue(std::move(tooLarge));
    QCOMPARE(failed.size(), 3);
    QCOMPARE(failed.at(2).at(0).toString(), QStringLiteral("The file is too large."));
}

void FileHostTest::clipboardFilesAndImages()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString path = writeNote(dir, "x");

    QMimeData files;
    files.setUrls({QUrl::fromLocalFile(path)});
    const auto fileOffer = ircClipboardOffer(&files);
    QVERIFY(fileOffer.has_value());
    QCOMPARE(fileOffer->paths, QStringList{path});
    QVERIFY(fileOffer->png.isEmpty());

    QMimeData text;
    text.setText(QStringLiteral("hello"));
    QVERIFY(!ircClipboardOffer(&text).has_value());

    QImage image(2, 2, QImage::Format_ARGB32);
    image.fill(Qt::red);
    QMimeData picture;
    picture.setImageData(image);
    const auto imageOffer = ircClipboardOffer(&picture);
    QVERIFY(imageOffer.has_value());
    QVERIFY(imageOffer->paths.isEmpty());
    QVERIFY(!imageOffer->png.isEmpty());

    picture.setText(QStringLiteral("caption"));
    QVERIFY(!ircClipboardOffer(&picture).has_value());
}

int runFileHostTests(int argc, char **argv)
{
    FileHostTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_filehost.moc"
