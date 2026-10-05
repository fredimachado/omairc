#include "fileuploadcatcher.h"

#include <QFile>
#include <QHostAddress>
#include <QList>
#include <QSslCertificate>
#include <QSslKey>
#include <QSslSocket>

namespace
{
QSslCertificate loadCertificate(const char *name)
{
    QFile file(QStringLiteral(TEST_CERT_DIR "/") + QString::fromUtf8(name));
    if (!file.open(QIODevice::ReadOnly))
        return QSslCertificate();
    return QSslCertificate(file.readAll(), QSsl::Pem);
}

QSslKey loadKey(const char *name)
{
    QFile file(QStringLiteral(TEST_CERT_DIR "/") + QString::fromUtf8(name));
    if (!file.open(QIODevice::ReadOnly))
        return {};
    return QSslKey(file.readAll(), QSsl::Rsa, QSsl::Pem);
}

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
}

FileUploadCatcher::FileUploadCatcher(QObject *parent)
    : QObject(parent)
{
    connect(&m_server, &QSslServer::pendingConnectionAvailable, this, [this] {
        takeConnection();
    });
    connect(&m_server, &QSslServer::errorOccurred, this,
            [this](QSslSocket *, QAbstractSocket::SocketError) {
        if (!m_server.errorString().isEmpty())
            setLastError(m_server.errorString());
    });
}

FileUploadCatcher::~FileUploadCatcher()
{
    m_server.close();
    if (m_replacedSsl)
        QSslConfiguration::setDefaultConfiguration(m_previousSsl);
}

bool FileUploadCatcher::listen()
{
    const QSslCertificate certificate = loadCertificate("localhost-cert.pem");
    const QSslCertificate ca = loadCertificate("localhost-ca-cert.pem");
    const QSslKey key = loadKey("localhost-key.pem");
    if (certificate.isNull() || ca.isNull() || key.isNull()) {
        setLastError(QStringLiteral("missing test certificate"));
        return false;
    }

    QSslConfiguration serverConfiguration = QSslConfiguration::defaultConfiguration();
    serverConfiguration.setLocalCertificate(certificate);
    serverConfiguration.setLocalCertificateChain(QList<QSslCertificate>{certificate, ca});
    serverConfiguration.setPrivateKey(key);
    serverConfiguration.setPeerVerifyMode(QSslSocket::VerifyNone);
    m_server.setSslConfiguration(serverConfiguration);
    m_server.setHandshakeTimeout(15000);
    if (!m_server.listen(QHostAddress::LocalHost, 0)) {
        setLastError(m_server.errorString());
        return false;
    }

    if (!m_replacedSsl) {
        m_previousSsl = QSslConfiguration::defaultConfiguration();
        m_replacedSsl = true;
    }
    QSslConfiguration client = m_previousSsl;
    QList<QSslCertificate> authorities = client.caCertificates();
    authorities.append(ca);
    client.setCaCertificates(authorities);
    QSslConfiguration::setDefaultConfiguration(client);
    emit endpointChanged();
    return true;
}

int FileUploadCatcher::uploads() const
{
    return m_uploads;
}

QString FileUploadCatcher::endpoint() const
{
    if (!m_server.isListening())
        return {};
    return QStringLiteral("https://127.0.0.1:%1/upload").arg(m_server.serverPort());
}

QString FileUploadCatcher::lastError() const
{
    return m_lastError;
}

bool FileUploadCatcher::refuse() const
{
    return m_refuse;
}

void FileUploadCatcher::setRefuse(bool refuse)
{
    if (m_refuse == refuse)
        return;
    m_refuse = refuse;
    emit refuseChanged();
}

void FileUploadCatcher::setLastError(const QString &error)
{
    if (m_lastError == error)
        return;
    m_lastError = error;
    emit lastErrorChanged();
}

void FileUploadCatcher::takeConnection()
{
    while (QSslSocket *socket = qobject_cast<QSslSocket *>(m_server.nextPendingConnection())) {
        m_buffers.insert(socket, {});
        connect(socket, &QSslSocket::readyRead, this, [this, socket] {
            readSocket(socket);
        });
        connect(socket, &QSslSocket::disconnected, socket, [this, socket] {
            m_buffers.remove(socket);
            socket->deleteLater();
        });
        if (socket->bytesAvailable() > 0)
            readSocket(socket);
    }
}

void FileUploadCatcher::readSocket(QSslSocket *socket)
{
    if (!socket || !m_buffers.contains(socket))
        return;
    QByteArray &buffer = m_buffers[socket];
    buffer.append(socket->readAll());
    if (!requestComplete(buffer))
        return;
    ++m_uploads;
    emit uploadsChanged();
    QByteArray response;
    if (m_refuse) {
        response = "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    } else {
        response = "HTTP/1.1 201 Created\r\nLocation: /files/note.txt\r\n"
                   "Content-Length: 0\r\nConnection: close\r\n\r\n";
    }
    socket->write(response);
    socket->disconnectFromHost();
    m_buffers.remove(socket);
}
