#include "ircloopbacktransport.h"

IrcLoopbackTransport::IrcLoopbackTransport(QObject *parent)
    : IrcTransport(parent)
{
}

void IrcLoopbackTransport::connectToHost(const QString &host, quint16 port, bool tlsEnabled)
{
    if (m_state == ConnectionState::Connecting
        || m_state == ConnectionState::Connected
        || m_state == ConnectionState::Encrypted
        || m_state == ConnectionState::Closing) {
        return;
    }

    m_host = host;
    m_port = port;
    m_tlsEnabled = tlsEnabled;
    m_lastError.clear();
    setState(ConnectionState::Connecting);
}

void IrcLoopbackTransport::write(const QByteArray &frame)
{
    m_written.append(frame);
    emit frameWritten(frame);
}

void IrcLoopbackTransport::shutdown()
{
    if (isFinished() || m_state == ConnectionState::Closing)
        return;

    setState(ConnectionState::Closing);
    setState(ConnectionState::Disconnected);
    emit disconnected();
}

IrcTransport::ConnectionState IrcLoopbackTransport::connectionState() const
{
    return m_state;
}

void IrcLoopbackTransport::completeConnect()
{
    if (m_state != ConnectionState::Connecting)
        return;

    setState(ConnectionState::Connected);
    emit connected();

    if (!m_tlsEnabled)
        return;

    setState(ConnectionState::Encrypted);
    emit encrypted();
}

void IrcLoopbackTransport::failConnect(const QString &message)
{
    if (m_state != ConnectionState::Connecting)
        return;

    fail(message);
}

void IrcLoopbackTransport::failTls(const QString &message)
{
    if (m_state != ConnectionState::Connecting && m_state != ConnectionState::Connected)
        return;

    fail(message);
}

void IrcLoopbackTransport::timeoutConnect()
{
    if (m_state != ConnectionState::Connecting)
        return;

    fail(QStringLiteral("Connection timed out"));
}

void IrcLoopbackTransport::injectBytes(const QByteArray &bytes)
{
    if (!isOpen() || bytes.isEmpty())
        return;

    emit bytesReceived(bytes);
}

void IrcLoopbackTransport::remoteClose()
{
    if (!isOpen())
        return;

    setState(ConnectionState::Disconnected);
    emit disconnected();
}

QByteArrayList IrcLoopbackTransport::writtenFrames() const
{
    return m_written;
}

QString IrcLoopbackTransport::lastError() const
{
    return m_lastError;
}

QString IrcLoopbackTransport::connectedHost() const
{
    return m_host;
}

quint16 IrcLoopbackTransport::connectedPort() const
{
    return m_port;
}

bool IrcLoopbackTransport::tlsRequested() const
{
    return m_tlsEnabled;
}

bool IrcLoopbackTransport::isOpen() const
{
    return m_state == ConnectionState::Connected || m_state == ConnectionState::Encrypted;
}

bool IrcLoopbackTransport::isFinished() const
{
    return m_state == ConnectionState::Idle
        || m_state == ConnectionState::Disconnected
        || m_state == ConnectionState::Failed;
}

void IrcLoopbackTransport::setState(ConnectionState state)
{
    m_state = state;
}

void IrcLoopbackTransport::fail(const QString &message)
{
    if (m_state == ConnectionState::Failed)
        return;

    m_lastError = message;
    setState(ConnectionState::Failed);
    emit errorOccurred(message);
}
