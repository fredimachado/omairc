#include "fakeirctransport.h"

FakeIrcTransport::FakeIrcTransport(QObject *parent)
    : IrcTransport(parent)
{
}

void FakeIrcTransport::connectToHost(const QString &host, quint16 port, bool tlsEnabled)
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

void FakeIrcTransport::write(const QByteArray &frame)
{
    m_written.append(frame);
}

void FakeIrcTransport::shutdown()
{
    if (isFinished() || m_state == ConnectionState::Closing)
        return;

    setState(ConnectionState::Closing);
    setState(ConnectionState::Disconnected);
    emit disconnected();
}

IrcTransport::ConnectionState FakeIrcTransport::connectionState() const
{
    return m_state;
}

void FakeIrcTransport::completeConnect()
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

void FakeIrcTransport::failConnect(const QString &message)
{
    if (m_state != ConnectionState::Connecting)
        return;

    fail(message);
}

void FakeIrcTransport::failTls(const QString &message)
{
    if (m_state != ConnectionState::Connecting && m_state != ConnectionState::Connected)
        return;

    fail(message);
}

void FakeIrcTransport::timeoutConnect()
{
    if (m_state != ConnectionState::Connecting)
        return;

    fail(QStringLiteral("Connection timed out"));
}

void FakeIrcTransport::injectBytes(const QByteArray &bytes)
{
    if (!isOpen() || bytes.isEmpty())
        return;

    emit bytesReceived(bytes);
}

void FakeIrcTransport::remoteClose()
{
    if (!isOpen())
        return;

    setState(ConnectionState::Disconnected);
    emit disconnected();
}

QByteArrayList FakeIrcTransport::writtenFrames() const
{
    return m_written;
}

QString FakeIrcTransport::lastError() const
{
    return m_lastError;
}

QString FakeIrcTransport::connectedHost() const
{
    return m_host;
}

quint16 FakeIrcTransport::connectedPort() const
{
    return m_port;
}

bool FakeIrcTransport::tlsRequested() const
{
    return m_tlsEnabled;
}

bool FakeIrcTransport::isOpen() const
{
    return m_state == ConnectionState::Connected || m_state == ConnectionState::Encrypted;
}

bool FakeIrcTransport::isFinished() const
{
    return m_state == ConnectionState::Idle
        || m_state == ConnectionState::Disconnected
        || m_state == ConnectionState::Failed;
}

void FakeIrcTransport::setState(ConnectionState state)
{
    m_state = state;
}

void FakeIrcTransport::fail(const QString &message)
{
    if (m_state == ConnectionState::Failed)
        return;

    m_lastError = message;
    setState(ConnectionState::Failed);
    emit errorOccurred(message);
}
