#include "qtirctransport.h"

#include <QSslCertificate>
#include <QSslError>
#include <QStringList>

QtIrcTransport::QtIrcTransport(QObject *parent)
    : IrcTransport(parent)
{
    connect(&m_socket, &QSslSocket::connected, this, &QtIrcTransport::onConnected);
    connect(&m_socket, &QSslSocket::encrypted, this, &QtIrcTransport::onEncrypted);
    connect(&m_socket, &QSslSocket::disconnected, this, &QtIrcTransport::onDisconnected);
    connect(&m_socket, &QSslSocket::readyRead, this, &QtIrcTransport::onReadyRead);
    connect(&m_socket, &QAbstractSocket::errorOccurred, this, &QtIrcTransport::onSocketError);
    connect(&m_socket, &QSslSocket::sslErrors, this, &QtIrcTransport::onSslErrors);
}

QtIrcTransport::QtIrcTransport(const QSslConfiguration &sslConfiguration,
                               QObject *parent)
    : QtIrcTransport(parent)
{
    m_socket.setSslConfiguration(sslConfiguration);
}

void QtIrcTransport::connectToHost(const QString &host, quint16 port, bool tlsEnabled)
{
    if (m_state == ConnectionState::Connecting
        || m_state == ConnectionState::Connected
        || m_state == ConnectionState::Encrypted
        || m_state == ConnectionState::Closing) {
        return;
    }

    m_pendingWrites.clear();
    m_tlsEnabled = tlsEnabled;
    setState(ConnectionState::Connecting);

    if (tlsEnabled)
        m_socket.connectToHostEncrypted(host, port);
    else
        m_socket.connectToHost(host, port);
}

void QtIrcTransport::write(const QByteArray &frame)
{
    if (frame.isEmpty())
        return;

    if (isOpen())
        m_socket.write(frame);
    else
        m_pendingWrites.append(frame);
}

void QtIrcTransport::shutdown()
{
    if (isFinished() || m_state == ConnectionState::Closing)
        return;

    setState(ConnectionState::Closing);
    m_pendingWrites.clear();

    if (m_socket.state() == QAbstractSocket::UnconnectedState) {
        setState(ConnectionState::Disconnected);
        emit disconnected();
        return;
    }

    m_socket.abort();
    if (m_state == ConnectionState::Closing) {
        setState(ConnectionState::Disconnected);
        emit disconnected();
    }
}

IrcTransport::ConnectionState QtIrcTransport::connectionState() const
{
    return m_state;
}

bool QtIrcTransport::isOpen() const
{
    return m_state == ConnectionState::Connected || m_state == ConnectionState::Encrypted;
}

bool QtIrcTransport::isFinished() const
{
    return m_state == ConnectionState::Idle
        || m_state == ConnectionState::Disconnected
        || m_state == ConnectionState::Failed;
}

void QtIrcTransport::setState(ConnectionState state)
{
    m_state = state;
}

void QtIrcTransport::fail(const QString &message)
{
    if (m_state == ConnectionState::Failed)
        return;

    setState(ConnectionState::Failed);
    m_pendingWrites.clear();
    emit errorOccurred(message);

    if (m_socket.state() != QAbstractSocket::UnconnectedState)
        m_socket.abort();
}

void QtIrcTransport::flushPendingWrites()
{
    const QByteArrayList pending = m_pendingWrites;
    m_pendingWrites.clear();
    for (const QByteArray &frame : pending)
        m_socket.write(frame);
}

void QtIrcTransport::onConnected()
{
    if (m_state != ConnectionState::Connecting)
        return;

    setState(ConnectionState::Connected);
    emit connected();

    if (!m_tlsEnabled)
        flushPendingWrites();
}

void QtIrcTransport::onEncrypted()
{
    if (m_state != ConnectionState::Connected && m_state != ConnectionState::Connecting)
        return;

    if (m_state == ConnectionState::Connecting) {
        setState(ConnectionState::Connected);
        emit connected();
    }

    setState(ConnectionState::Encrypted);
    emit encrypted();
    flushPendingWrites();
}

void QtIrcTransport::onDisconnected()
{
    if (m_state == ConnectionState::Failed || m_state == ConnectionState::Disconnected)
        return;

    setState(ConnectionState::Disconnected);
    m_pendingWrites.clear();
    emit disconnected();
}

void QtIrcTransport::onReadyRead()
{
    const QByteArray bytes = m_socket.readAll();
    if (!bytes.isEmpty())
        emit bytesReceived(bytes);
}

void QtIrcTransport::onSocketError(QAbstractSocket::SocketError error)
{
    if (m_state == ConnectionState::Failed || m_state == ConnectionState::Closing)
        return;

    if (error == QAbstractSocket::RemoteHostClosedError)
        return;

    fail(m_socket.errorString());
}

void QtIrcTransport::onSslErrors(const QList<QSslError> &errors)
{
    QStringList details;
    details.reserve(errors.size());
    for (const QSslError &error : errors) {
        QString detail = error.errorString();
        const QSslCertificate certificate = error.certificate();
        if (!certificate.isNull()) {
            const QStringList commonName = certificate.subjectInfo(QSslCertificate::CommonName);
            if (!commonName.isEmpty())
                detail += QStringLiteral(" (CN=%1)").arg(commonName.join(QLatin1Char(',')));
        }
        details.append(detail);
    }

    fail(QStringLiteral("TLS certificate error: %1").arg(details.join(QStringLiteral("; "))));
}
