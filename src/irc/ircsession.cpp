#include "ircsession.h"

#include "irccommandbuilder.h"
#include "ircparser.h"

#include <QByteArray>
#include <QtGlobal>

#include <algorithm>
#include <limits>
#include <string>

namespace
{
QByteArray builtLine(const IrcBuildResult &result)
{
    if (!result)
        return {};
    return QByteArray(result.value->data(), qsizetype(result.value->size()));
}

QString parameter(const IrcMessage &message, std::size_t index)
{
    if (index >= message.parameters.size())
        return {};
    return QString::fromStdString(message.parameters[index]);
}

int parameterIndex(const IrcMessage &message, const QString &value)
{
    for (std::size_t index = 0; index < message.parameters.size(); ++index) {
        if (parameter(message, index).compare(value, Qt::CaseInsensitive) == 0)
            return int(index);
    }
    return -1;
}
}

IrcReconnectTimer::IrcReconnectTimer(QObject *parent)
    : QObject(parent)
{
    m_timer.setSingleShot(true);
    connect(&m_timer, &QTimer::timeout, this, &IrcReconnectTimer::fired);
}

void IrcReconnectTimer::start(int delayMilliseconds)
{
    m_timer.start(delayMilliseconds);
}

void IrcReconnectTimer::cancel()
{
    m_timer.stop();
}

IrcSession::IrcSession(const IrcSessionConfig &config,
                       IrcTransport *transport,
                       IrcReconnectTimer *reconnectTimer,
                       QObject *parent)
    : QObject(parent)
    , m_config(config)
    , m_transport(transport)
    , m_reconnectTimer(reconnectTimer)
{
    Q_ASSERT(m_transport);
    if (!m_transport->parent())
        m_transport->setParent(this);

    if (!m_reconnectTimer) {
        m_reconnectTimer = new IrcReconnectTimer(this);
    } else if (!m_reconnectTimer->parent()) {
        m_reconnectTimer->setParent(this);
    }

    connect(m_transport, &IrcTransport::connected, this, [this] {
        if (!m_config.tlsEnabled && m_state == State::Connecting)
            beginCapabilityNegotiation();
    });
    connect(m_transport, &IrcTransport::encrypted, this, [this] {
        if (m_config.tlsEnabled && m_state == State::Connecting)
            beginCapabilityNegotiation();
    });
    connect(m_transport, &IrcTransport::bytesReceived,
            this, &IrcSession::handleBytes);
    connect(m_transport, &IrcTransport::errorOccurred, this, [this](const QString &message) {
        if (m_expectedDisconnect)
            return;
        const bool tlsFailure = message.contains(QLatin1String("TLS"), Qt::CaseInsensitive)
            || message.contains(QLatin1String("certificate"), Qt::CaseInsensitive);
        fail(tlsFailure ? ErrorKind::Tls : ErrorKind::Network, message, true);
    });
    connect(m_transport, &IrcTransport::disconnected, this, [this] {
        if (m_reconnectAfterDisconnect) {
            m_reconnectAfterDisconnect = false;
            scheduleReconnect();
            return;
        }
        if (m_expectedDisconnect) {
            if (m_state == State::Closing)
                setState(State::Idle);
            return;
        }
        emit errorOccurred(m_config.networkId,
                           ErrorKind::Network,
                           QStringLiteral("Connection closed by the server"));
        scheduleReconnect();
    });
    connect(m_reconnectTimer, &IrcReconnectTimer::fired, this, [this] {
        if (m_state != State::Reconnecting)
            return;
        resetForConnection();
        setState(State::Connecting);
        m_transport->connectToHost(m_config.host, m_config.port, m_config.tlsEnabled);
    });
}

IrcSession::~IrcSession()
{
    m_expectedDisconnect = true;
    m_reconnectAfterDisconnect = false;
    m_reconnectTimer->cancel();
    m_transport->shutdown();
}

QString IrcSession::networkId() const
{
    return m_config.networkId;
}

QString IrcSession::nick() const
{
    return m_config.nick;
}

IrcSession::State IrcSession::state() const
{
    return m_state;
}

int IrcSession::reconnectAttempt() const
{
    return m_reconnectAttempt;
}

void IrcSession::start()
{
    if (m_state != State::Idle && m_state != State::Failed)
        return;
    if (m_config.networkId.isEmpty() || m_config.host.isEmpty() || m_config.port == 0
        || m_config.nick.isEmpty() || m_config.username.isEmpty()
        || m_config.realname.isEmpty()) {
        fail(ErrorKind::Registration,
             QStringLiteral("IRC session configuration is incomplete"),
             false);
        return;
    }

    m_expectedDisconnect = false;
    m_reconnectAfterDisconnect = false;
    m_reconnectAttempt = 0;
    resetForConnection();
    setState(State::Connecting);
    m_transport->connectToHost(m_config.host, m_config.port, m_config.tlsEnabled);
}

void IrcSession::stop()
{
    m_expectedDisconnect = true;
    m_reconnectAfterDisconnect = false;
    m_reconnectTimer->cancel();
    m_reconnectAttempt = 0;

    if (m_state == State::Idle)
        return;
    if (m_state == State::Reconnecting || m_state == State::Failed) {
        setState(State::Idle);
        return;
    }

    setState(State::Closing);
    m_transport->shutdown();
    if (m_transport->connectionState() == IrcTransport::ConnectionState::Idle
        || m_transport->connectionState() == IrcTransport::ConnectionState::Disconnected
        || m_transport->connectionState() == IrcTransport::ConnectionState::Failed) {
        setState(State::Idle);
    }
}

void IrcSession::cancelReconnect()
{
    if (m_state != State::Reconnecting)
        return;
    m_expectedDisconnect = true;
    m_reconnectAfterDisconnect = false;
    m_reconnectTimer->cancel();
    m_reconnectAttempt = 0;
    setState(State::Idle);
}

bool IrcSession::sendPrivmsg(const QString& target, const QString& body)
{
    if (target.isEmpty() || body.isEmpty())
        return false;
    return sendCommand(QStringLiteral("PRIVMSG %1 :%2").arg(target, body));
}

bool IrcSession::sendAction(const QString& target, const QString& body)
{
    if (target.isEmpty() || body.isEmpty())
        return false;
    return sendPrivmsg(target, QChar(1) + QStringLiteral("ACTION ") + body + QChar(1));
}

bool IrcSession::join(const QString& channel)
{
    return !channel.isEmpty()
        && sendCommand(QStringLiteral("JOIN %1").arg(channel));
}

bool IrcSession::part(const QString& channel)
{
    return !channel.isEmpty()
        && sendCommand(QStringLiteral("PART %1").arg(channel));
}

bool IrcSession::changeNick(const QString& nick)
{
    return !nick.isEmpty()
        && sendCommand(QStringLiteral("NICK %1").arg(nick));
}

bool IrcSession::quit(const QString& reason)
{
    const bool sent = sendCommand(
        reason.isEmpty() ? QStringLiteral("QUIT")
                         : QStringLiteral("QUIT :%1").arg(reason));
    if (sent)
        stop();
    return sent;
}

void IrcSession::setState(State state)
{
    if (m_state == state)
        return;
    m_state = state;
    emit stateChanged(state);
}

void IrcSession::beginCapabilityNegotiation()
{
    setState(State::CapLs);
    sendLine(QByteArrayLiteral("CAP LS 302\r\n"));
}

void IrcSession::sendRegistration()
{
    if (m_registrationSent)
        return;

    // SASL uses the same in-memory secret. Do not also send PASS.
    const QByteArray pass = (m_config.password.isEmpty() || m_saslRequested)
        ? QByteArray{}
        : builtLine(IrcCommandBuilder::pass(m_config.password.toStdString()));
    const QByteArray nick = builtLine(IrcCommandBuilder::nick(m_config.nick.toStdString()));
    const QByteArray user = builtLine(IrcCommandBuilder::user(
        m_config.username.toStdString(), m_config.realname.toStdString()));
    if ((!m_config.password.isEmpty() && !m_saslRequested && pass.isEmpty())
        || nick.isEmpty() || user.isEmpty()) {
        fail(ErrorKind::Registration,
             QStringLiteral("IRC registration fields contain invalid characters"),
             false);
        return;
    }

    if (!pass.isEmpty())
        sendLine(pass);
    sendLine(nick);
    sendLine(user);
    m_registrationSent = true;
    setState(State::Registering);
}

void IrcSession::sendLine(const QByteArray &line)
{
    if (!line.isEmpty())
        m_transport->write(line);
}

bool IrcSession::sendCommand(const QString& command)
{
    if (m_state != State::Registered)
        return false;
    const QByteArray line = builtLine(IrcCommandBuilder::line(command.toStdString()));
    if (line.isEmpty())
        return false;
    sendLine(line);
    return true;
}

void IrcSession::handleBytes(const QByteArray &bytes)
{
    const IrcFrameResult result = m_framer.feed(
        std::string_view(bytes.constData(), std::size_t(bytes.size())));
    for (IrcError error : result.errors) {
        emit errorOccurred(m_config.networkId,
                           ErrorKind::Protocol,
                           QStringLiteral("Malformed IRC frame (%1)").arg(int(error)));
    }

    for (const std::string &frame : result.frames) {
        const IrcParseResult parsed = IrcParser::parse(frame);
        if (!parsed) {
            emit errorOccurred(m_config.networkId,
                               ErrorKind::Protocol,
                               QStringLiteral("Malformed IRC message (%1)")
                                   .arg(int(parsed.error)));
            continue;
        }
        handleMessage(*parsed.value);
    }
}

void IrcSession::handleMessage(const IrcMessage &message)
{
    if (message.command == "PING") {
        if (message.parameters.empty()) {
            emit errorOccurred(m_config.networkId,
                               ErrorKind::Protocol,
                               QStringLiteral("PING message has no token"));
            return;
        }
        const QByteArray token = QByteArray::fromStdString(message.parameters.back());
        sendLine(QByteArrayLiteral("PONG :") + token + QByteArrayLiteral("\r\n"));
        return;
    }

    if (message.command == "CAP") {
        handleCap(message);
        return;
    }
    if (message.command == "AUTHENTICATE") {
        handleAuthenticate(message);
        return;
    }
    if (message.command == "001") {
        handleWelcome();
        return;
    }
    if (message.command == "903") {
        if (m_state == State::Sasl) {
            sendLine(QByteArrayLiteral("CAP END\r\n"));
            setState(State::Registering);
        }
        return;
    }
    if (message.command == "904" || message.command == "905") {
        fail(ErrorKind::Authentication,
             QStringLiteral("SASL authentication failed"),
             false);
        return;
    }
    if (message.command == "421"
        && parameterIndex(message, QStringLiteral("CAP")) >= 0
        && m_state == State::CapLs) {
        sendRegistration();
        return;
    }
    if (message.command == "464") {
        fail(ErrorKind::Authentication,
             QStringLiteral("Server password or authentication was rejected"),
             false);
        return;
    }
    if (message.command == "432" || message.command == "433"
        || message.command == "436" || message.command == "451"
        || message.command == "462" || message.command == "465") {
        fail(ErrorKind::Registration,
             QStringLiteral("IRC registration was refused (%1)")
                 .arg(QString::fromStdString(message.command)),
             false);
        return;
    }
    if (message.command == "ERROR") {
        emit messageReceived(m_config.networkId, message);
        fail(ErrorKind::Network,
             message.parameters.empty()
                 ? QStringLiteral("IRC server reported an error")
                 : QString::fromStdString(message.parameters.back()),
             true);
        return;
    }

    emit messageReceived(m_config.networkId, message);
}

void IrcSession::handleCap(const IrcMessage &message)
{
    const int lsIndex = parameterIndex(message, QStringLiteral("LS"));
    if (lsIndex >= 0) {
        if (message.parameters.size() > std::size_t(lsIndex + 1)) {
            const QString capabilities = QString::fromStdString(message.parameters.back());
            m_advertisedCapabilities.append(
                capabilities.split(QLatin1Char(' '), Qt::SkipEmptyParts));
        }
        const bool continuation = message.parameters.size() > std::size_t(lsIndex + 1)
            && parameter(message, std::size_t(lsIndex + 1)) == QStringLiteral("*");
        if (continuation)
            return;

        const bool hasSasl = std::any_of(
            m_advertisedCapabilities.cbegin(),
            m_advertisedCapabilities.cend(),
            [](const QString &capability) {
                if (capability.section(QLatin1Char('='), 0, 0)
                        .compare(QStringLiteral("sasl"), Qt::CaseInsensitive)
                    != 0) {
                    return false;
                }
                if (!capability.contains(QLatin1Char('=')))
                    return true;
                return capability.section(QLatin1Char('='), 1)
                    .split(QLatin1Char(','), Qt::SkipEmptyParts)
                    .contains(QStringLiteral("PLAIN"), Qt::CaseInsensitive);
            });
        if (!m_config.password.isEmpty() && hasSasl) {
            m_saslRequested = true;
            setState(State::CapReq);
            sendLine(QByteArrayLiteral("CAP REQ :sasl\r\n"));
            sendRegistration();
            return;
        }

        sendRegistration();
        sendLine(QByteArrayLiteral("CAP END\r\n"));
        return;
    }

    const int ackIndex = parameterIndex(message, QStringLiteral("ACK"));
    if (ackIndex >= 0 && m_saslRequested) {
        setState(State::Sasl);
        sendLine(QByteArrayLiteral("AUTHENTICATE PLAIN\r\n"));
        return;
    }

    const int nakIndex = parameterIndex(message, QStringLiteral("NAK"));
    if (nakIndex >= 0 && m_saslRequested) {
        fail(ErrorKind::Authentication,
             QStringLiteral("Server rejected the SASL capability"),
             false);
    }
}

void IrcSession::handleAuthenticate(const IrcMessage &message)
{
    if (m_state != State::Sasl || message.parameters.empty())
        return;
    if (message.parameters.front() == "*") {
        fail(ErrorKind::Authentication,
             QStringLiteral("Server aborted SASL authentication"),
             false);
        return;
    }
    if (message.parameters.front() != "+")
        return;

    const QByteArray nick = m_config.nick.toUtf8();
    QByteArray plain;
    plain.reserve(nick.size() * 2 + m_config.password.toUtf8().size() + 2);
    plain.append(nick);
    plain.append('\0');
    plain.append(nick);
    plain.append('\0');
    plain.append(m_config.password.toUtf8());
    const QByteArray encoded = plain.toBase64();
    if (encoded.size() > 400) {
        fail(ErrorKind::Authentication,
             QStringLiteral("SASL PLAIN credentials exceed one IRC payload"),
             false);
        return;
    }
    sendLine(QByteArrayLiteral("AUTHENTICATE ") + encoded
             + QByteArrayLiteral("\r\n"));
}

void IrcSession::handleWelcome()
{
    if (m_state == State::Registered)
        return;

    m_reconnectAttempt = 0;
    setState(State::Registered);
    emit registered(m_config.networkId);
    for (const QString &channel : m_config.autojoinChannels) {
        const IrcBuildResult join = IrcCommandBuilder::line(
            QStringLiteral("JOIN %1").arg(channel).toStdString());
        if (!join) {
            emit errorOccurred(m_config.networkId,
                               ErrorKind::Protocol,
                               QStringLiteral("Invalid autojoin channel"));
            continue;
        }
        sendLine(builtLine(join));
    }
}

void IrcSession::fail(ErrorKind kind, const QString &message, bool reconnect)
{
    emit errorOccurred(m_config.networkId, kind, message);
    if (reconnect) {
        const IrcTransport::ConnectionState transportState = m_transport->connectionState();
        if (transportState == IrcTransport::ConnectionState::Connecting
            || transportState == IrcTransport::ConnectionState::Connected
            || transportState == IrcTransport::ConnectionState::Encrypted
            || transportState == IrcTransport::ConnectionState::Closing) {
            m_reconnectAfterDisconnect = true;
            m_transport->shutdown();
            return;
        }
        scheduleReconnect();
        return;
    }

    m_expectedDisconnect = true;
    m_reconnectTimer->cancel();
    setState(State::Failed);
    m_transport->shutdown();
}

void IrcSession::scheduleReconnect()
{
    if (!m_config.reconnectEnabled
        || m_reconnectAttempt >= std::max(0, m_config.reconnectMaximumAttempts)) {
        setState(State::Failed);
        return;
    }

    ++m_reconnectAttempt;
    const int delay = reconnectDelay();
    setState(State::Reconnecting);
    m_reconnectTimer->start(delay);
    emit reconnectScheduled(m_config.networkId, delay, m_reconnectAttempt);
}

void IrcSession::resetForConnection()
{
    m_framer = IrcFramer{};
    m_registrationSent = false;
    m_saslRequested = false;
    m_advertisedCapabilities.clear();
}

int IrcSession::reconnectDelay() const
{
    const int base = std::max(0, m_config.reconnectBaseDelayMilliseconds);
    const int maximum = std::max(base, m_config.reconnectMaximumDelayMilliseconds);
    qint64 delay = base;
    for (int step = 1; step < m_reconnectAttempt && delay < maximum; ++step)
        delay = std::min<qint64>(qint64(maximum), delay * 2);
    return int(std::min<qint64>(delay, std::numeric_limits<int>::max()));
}
