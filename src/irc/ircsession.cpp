#include "ircsession.h"

#include "ircchannelmode.h"
#include "irccommandbuilder.h"
#include "irccasemapping.h"
#include "ircjointarget.h"
#include "ircparser.h"
#include "ircprefixnick.h"
#include "ircpresence.h"
#include "ircservicenick.h"
#include "ircsecretpolicy.h"
#include "irctcp.h"
#include "irctyping.h"
#include "ircwiretext.h"

#include <QByteArray>
#include <QDateTime>
#include <QMetaType>
#include <QNetworkInformation>
#include <QTimer>
#include <QtGlobal>

#include <algorithm>
#include <limits>
#include <string>
#include <string_view>
#include <type_traits>

namespace
{
constexpr char kPingWatchdogToken[] = "omairc-watchdog";
constexpr qsizetype kCtcpPingPayloadMaxBytes = 32;
constexpr qint64 kCtcpReplyIntervalMs = 5000;

QString saslSecret(const IrcSessionConfig &config)
{
    return config.nickServPassword.isEmpty() ? config.password
                                             : config.nickServPassword;
}

QString ctcpReplyHost(const IrcMessage &message)
{
    if (!message.prefix)
        return {};
    if (!message.prefix->host.empty())
        return ircWireText(message.prefix->host);
    if (message.prefix->nick.empty())
        return ircWireText(message.prefix->raw);
    return {};
}

QString previewWire(std::string_view bytes, std::size_t byteCount, QStringView channelTypes)
{
    std::string display;
    std::string spaced;
    std::string stripped;
    display.reserve(bytes.size());
    spaced.reserve(bytes.size());
    stripped.reserve(bytes.size());
    for (unsigned char c : bytes) {
        if (c == '\t' || (c >= 0x20 && c < 0x7F)) {
            display.push_back(char(c));
            spaced.push_back(char(c));
            stripped.push_back(char(c));
        } else {
            display.push_back('?');
            spaced.push_back(' ');
        }
    }
    QString preview = ircWireText(display);
    const QString strippedText = ircWireText(stripped);
    if (const auto safe =
            IrcSecretPolicy::redactPreviewLine(QStringView(strippedText), channelTypes)) {
        preview = *safe;
    } else {
        const QString spacedText = ircWireText(spaced);
        if (const auto safeSpaced =
                IrcSecretPolicy::redactPreviewLine(QStringView(spacedText), channelTypes)) {
            preview = *safeSpaced;
        }
    }
    if (byteCount > bytes.size())
        preview += QChar(0x2026);
    return preview;
}

QString describeMalformed(const char *kind, IrcError error,
                          std::string_view preview, std::size_t byteCount,
                          QStringView channelTypes)
{
    QString text = QStringLiteral("Malformed IRC %1: %2 (%3 bytes)")
                       .arg(QLatin1String(kind),
                            QString::fromLatin1(ircErrorName(error)),
                            QString::number(byteCount));
    const QString shown = previewWire(preview, byteCount, channelTypes);
    if (shown.isEmpty())
        return text;
    return text + QStringLiteral(". Preview: ") + shown;
}

QByteArray builtLine(const IrcBuildResult &result)
{
    if (!result)
        return {};
    return QByteArray(result.value->data(), qsizetype(result.value->size()));
}

std::string utf8(const QString &value)
{
    const QByteArray bytes = value.toUtf8();
    return std::string(bytes.constData(), std::size_t(bytes.size()));
}

QString parameter(const IrcMessage &message, std::size_t index)
{
    if (index >= message.parameters.size())
        return {};
    return ircWireText(message.parameters[index]);
}

// Some servers omit user@host on a self JOIN or a NICK change. The bare token
// is ambiguous with a server name, so only compare it against a nick we
// already know. Never treat it as an arbitrary sender.
QString claimedNick(const IrcMessage &message)
{
    if (!message.prefix)
        return {};
    if (!message.prefix->nick.empty())
        return ircWireText(message.prefix->nick);
    return ircWireText(message.prefix->raw);
}

int parameterIndex(const IrcMessage &message, const QString &value)
{
    for (std::size_t index = 0; index < message.parameters.size(); ++index) {
        if (parameter(message, index).compare(value, Qt::CaseInsensitive) == 0)
            return int(index);
    }
    return -1;
}

QStringList capabilityTokens(const IrcMessage &message, int subcommandIndex)
{
    if (message.parameters.size() <= std::size_t(subcommandIndex + 1))
        return {};
    return ircWireText(message.parameters.back())
        .split(QLatin1Char(' '), Qt::SkipEmptyParts);
}

QString tagValue(const IrcMessage& message, const char *name)
{
    for (const IrcTag& tag : message.tags) {
        if (tag.name == name && tag.value)
            return ircWireText(*tag.value);
    }
    return {};
}

QByteArray wireLine(const QString &command)
{
    return command.toUtf8() + QByteArrayLiteral("\r\n");
}

bool isValidPrivmsgTarget(const QString &target)
{
    if (target.isEmpty() || target.startsWith(QLatin1Char(':')))
        return false;
    for (const QChar character : target) {
        if (character.isSpace() || character.category() == QChar::Other_Control)
            return false;
    }
    return true;
}

bool loadReachabilityBackend()
{
    using Feature = QNetworkInformation::Feature;
    if (QNetworkInformation::loadBackendByFeatures(Feature::Reachability))
        return true;
    return QNetworkInformation::loadDefaultBackend();
}

bool isReachable(QNetworkInformation::Reachability reachability)
{
    switch (reachability) {
    case QNetworkInformation::Reachability::Local:
    case QNetworkInformation::Reachability::Site:
    case QNetworkInformation::Reachability::Online:
        return true;
    case QNetworkInformation::Reachability::Unknown:
    case QNetworkInformation::Reachability::Disconnected:
        return false;
    }
    return false;
}

class QtReachabilitySource : public IrcReachabilitySource
{
public:
    explicit QtReachabilitySource(QObject *parent = nullptr)
        : IrcReachabilitySource(parent)
    {
        if (!loadReachabilityBackend())
            return;
        QNetworkInformation *information = QNetworkInformation::instance();
        if (!information)
            return;
        connect(information,
                &QNetworkInformation::reachabilityChanged,
                this,
                [this](QNetworkInformation::Reachability reachability) {
            if (isReachable(reachability))
                emit reachable();
        });
    }
};
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

IrcReachabilitySource::IrcReachabilitySource(QObject *parent)
    : QObject(parent)
{
}

IrcSession::IrcSession(const IrcSessionConfig &config,
                       IrcTransport *transport,
                       IrcReconnectTimer *reconnectTimer,
                       IrcReconnectTimer *capabilityTimer,
                       QObject *parent,
                       IrcReconnectTimer *pingTimer,
                       IrcReachabilitySource *reachability)
    : QObject(parent)
    , m_config(config)
    , m_nick(config.nick)
    , m_transport(transport)
    , m_reconnectTimer(reconnectTimer)
    , m_capabilityTimer(capabilityTimer)
    , m_pingTimer(pingTimer)
    , m_reachability(reachability)
    , m_capabilities(!saslSecret(config).isEmpty())
{
    qRegisterMetaType<IrcHistoryBatch>();
    Q_ASSERT(m_transport);
    if (!m_transport->parent())
        m_transport->setParent(this);

    if (!m_reconnectTimer) {
        m_reconnectTimer = new IrcReconnectTimer(this);
    } else if (!m_reconnectTimer->parent()) {
        m_reconnectTimer->setParent(this);
    }

    if (!m_capabilityTimer) {
        m_capabilityTimer = new IrcReconnectTimer(this);
    } else if (!m_capabilityTimer->parent()) {
        m_capabilityTimer->setParent(this);
    }

    if (!m_pingTimer) {
        m_pingTimer = new IrcReconnectTimer(this);
    } else if (!m_pingTimer->parent()) {
        m_pingTimer->setParent(this);
    }

    if (!m_reachability) {
        m_reachability = new QtReachabilitySource(this);
    } else if (!m_reachability->parent()) {
        m_reachability->setParent(this);
    }

    connect(m_capabilityTimer, &IrcReconnectTimer::fired, this, [this] {
        const IrcCapabilitySet abandoned = m_capabilities.abandonOutstanding();
        if (abandoned.contains(IrcCapability::Sasl))
            m_saslPending = false;
        publishCapabilities();
        endCapabilityNegotiation();
    });

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
        fail(ErrorKind::Network,
             QStringLiteral("Connection closed by the server"),
             true);
    });
    connect(m_reconnectTimer, &IrcReconnectTimer::fired,
            this, &IrcSession::beginReconnectAttempt);
    connect(m_reachability, &IrcReachabilitySource::reachable,
            this, &IrcSession::beginReconnectAttempt);
    connect(m_pingTimer, &IrcReconnectTimer::fired, this, &IrcSession::onPingWatchdogFired);
}

IrcSession::~IrcSession()
{
    m_expectedDisconnect = true;
    m_reconnectAfterDisconnect = false;
    m_reconnectTimer->cancel();
    m_capabilityTimer->cancel();
    cancelPingWatchdog();
    m_transport->shutdown();
}

QString IrcSession::networkId() const
{
    return m_config.networkId;
}

QString IrcSession::host() const
{
    return m_config.host;
}

quint16 IrcSession::port() const
{
    return m_config.port;
}

bool IrcSession::tlsEnabled() const
{
    return m_config.tlsEnabled;
}

QString IrcSession::nick() const
{
    return m_nick;
}

QString IrcSession::channelTypes() const
{
    return m_channelTypes;
}

void IrcSession::setIgnoreFilter(IgnoreFilter filter)
{
    m_ignoreFilter = std::move(filter);
}

IrcSession::State IrcSession::state() const
{
    return m_state;
}

int IrcSession::reconnectAttempt() const
{
    return m_reconnectAttempt;
}

IrcCapabilitySet IrcSession::capabilities() const
{
    return m_capabilities.enabled();
}

bool IrcSession::historyPending() const
{
    return !m_historyPending.isEmpty();
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
    m_reportedRetryErrors = 0;
    resetForConnection();
    setState(State::Connecting);
    m_transport->connectToHost(m_config.host, m_config.port, m_config.tlsEnabled);
}

void IrcSession::stop()
{
    m_expectedDisconnect = true;
    m_reconnectAfterDisconnect = false;
    m_reconnectTimer->cancel();
    m_capabilityTimer->cancel();
    cancelPingWatchdog();
    m_reconnectAttempt = 0;
    m_reportedRetryErrors = 0;

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

bool IrcSession::sendPrivmsg(const QString& target, const QString& body)
{
    if (!isValidPrivmsgTarget(target) || body.isEmpty())
        return false;
    const bool sent = sendCommand(QStringLiteral("PRIVMSG %1 :%2").arg(target, body));
    if (sent)
        m_typing.noteMessageSent(target);
    return sent;
}

bool IrcSession::sendNotice(const QString& target, const QString& body)
{
    if (target.isEmpty() || body.isEmpty())
        return false;
    return sendCommand(QStringLiteral("NOTICE %1 :%2").arg(target, body));
}

bool IrcSession::sendChannelMode(const IrcChannelModeRequest& request)
{
    return request.visit([this](const auto& payload) {
        using T = std::decay_t<decltype(payload)>;
        if constexpr (std::is_same_v<T, IrcChannelModeRequest::Query>) {
            return sendCommand(QStringLiteral("MODE %1").arg(payload.channel));
        } else {
            QStringList parts{QStringLiteral("MODE"), payload.channel, payload.modes};
            parts.append(payload.parameters);
            return sendCommand(parts.join(QLatin1Char(' ')));
        }
    });
}

bool IrcSession::sendAction(const QString& target, const QString& body)
{
    if (target.isEmpty() || body.isEmpty())
        return false;
    return sendPrivmsg(target, QChar(1) + QStringLiteral("ACTION ") + body + QChar(1));
}

bool IrcSession::sendTyping(const QString& target, IrcTypingPhase phase)
{
    if (m_state != State::Registered)
        return false;
    if (!capabilities().contains(IrcCapability::MessageTags))
        return false;
    const QDateTime now = QDateTime::currentDateTimeUtc();
    if (!m_typing.shouldSend(target, phase, now))
        return false;
    const QByteArray line = ircTypingTagmsg(target, phase);
    if (line.isEmpty())
        return false;
    m_transport->write(line);
    m_typing.recordSent(target, phase, now);
    return true;
}

bool IrcSession::join(const IrcJoinTarget& target)
{
    if (m_state != State::Registered)
        return false;
    const std::string channel = utf8(target.channel());
    const std::string key = target.hasKey() ? utf8(*target.key()) : std::string();
    const QByteArray line = builtLine(IrcCommandBuilder::join(channel, key));
    if (line.isEmpty())
        return false;
    sendLine(line);
    return true;
}

bool IrcSession::part(const QString& channel)
{
    return !channel.isEmpty()
        && sendCommand(QStringLiteral("PART %1").arg(channel));
}

bool IrcSession::kick(const QString& channel, const QString& nick, const QString& reason)
{
    if (channel.isEmpty() || nick.isEmpty())
        return false;
    return sendCommand(
        reason.isEmpty() ? QStringLiteral("KICK %1 %2").arg(channel, nick)
                         : QStringLiteral("KICK %1 %2 :%3").arg(channel, nick, reason));
}

bool IrcSession::setTopic(const QString& channel, const QString& topic)
{
    return !channel.isEmpty() && !topic.isEmpty()
        && sendCommand(QStringLiteral("TOPIC %1 :%2").arg(channel, topic));
}

bool IrcSession::setAway(const QString& reason)
{
    const QString trimmed = reason.trimmed();
    return sendCommand(
        trimmed.isEmpty() ? QStringLiteral("AWAY")
                         : QStringLiteral("AWAY :%1").arg(trimmed));
}

bool IrcSession::clearAway()
{
    return setAway({});
}

bool IrcSession::changeNick(const QString& nick)
{
    return !nick.isEmpty()
        && sendCommand(QStringLiteral("NICK %1").arg(nick));
}

bool IrcSession::quit(const QString& reason)
{
    if (m_state == State::Idle)
        return false;
    if (m_state == State::Registered) {
        const bool sent = sendCommand(
            reason.isEmpty() ? QStringLiteral("QUIT")
                             : QStringLiteral("QUIT :%1").arg(reason));
        if (!sent)
            return false;
    }
    stop();
    return true;
}

bool IrcSession::whois(const QString& nick)
{
    const QString trimmed = nick.trimmed();
    if (trimmed.isEmpty())
        return false;
    return sendCommand(QStringLiteral("WHOIS %1 %1").arg(trimmed));
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

void IrcSession::requestCapabilities()
{
    const IrcCapabilityNegotiation::Request request = m_capabilities.takeRequest();
    if (request.requestsSasl) {
        m_saslRequested = true;
        m_saslPending = true;
    }

    if (!request.lines.isEmpty()) {
        if (!m_registrationSent)
            setState(State::CapReq);
        for (const QString &line : request.lines)
            sendLine(wireLine(QStringLiteral("CAP REQ :%1").arg(line)));
        m_capabilityTimer->start(std::max(0, m_config.capabilityTimeoutMilliseconds));
    }

    sendRegistration();
    endCapabilityNegotiation();
}

void IrcSession::endCapabilityNegotiation()
{
    if (m_capabilityNegotiationEnded || m_state == State::Registered)
        return;
    if (!m_registrationSent || !m_capabilities.settled() || m_saslPending)
        return;

    m_capabilityNegotiationEnded = true;
    m_capabilityTimer->cancel();
    sendLine(QByteArrayLiteral("CAP END\r\n"));
}

void IrcSession::publishCapabilities()
{
    const IrcCapabilitySet enabled = m_capabilities.enabled();
    if (enabled == m_publishedCapabilities)
        return;
    m_publishedCapabilities = enabled;
    emit capabilitiesChanged(m_config.networkId, enabled);
}

void IrcSession::subscribeToMemberMetadata()
{
    const IrcCapabilitySet enabled = m_capabilities.enabled();
    if (!enabled.contains(IrcCapability::MemberMetadata)
        || !enabled.contains(IrcCapability::Batch)) {
        return;
    }
    sendLine(wireLine(QStringLiteral("METADATA * SUB %1")
                          .arg(IrcMetadata::subscribedKeys().join(QLatin1Char(' ')))));
}

void IrcSession::probeChannelAway(const QString& channel)
{
    if (channel.isEmpty()
        || !m_capabilities.enabled().contains(IrcCapability::AwayNotify)) {
        return;
    }
    sendCommand(QStringLiteral("WHO %1").arg(channel));
}

void IrcSession::handleMetadataSyncLater(const IrcMessage &message)
{
    const QString target = parameter(message, 1);
    if (target.isEmpty())
        return;
    const int retryAfterSeconds = qBound(0, parameter(message, 2).toInt(), 60);
    QTimer::singleShot(retryAfterSeconds * 1000, this, [this, target] {
        sendCommand(QStringLiteral("METADATA %1 SYNC").arg(target));
    });
}

void IrcSession::sendRegistration()
{
    if (m_registrationSent)
        return;

    const bool sendPass = !m_config.password.isEmpty()
        && (!m_saslRequested || !m_config.nickServPassword.isEmpty());
    const QByteArray pass = sendPass
        ? builtLine(IrcCommandBuilder::pass(m_config.password.toStdString()))
        : QByteArray{};
    const QByteArray nick = builtLine(IrcCommandBuilder::nick(m_config.nick.toStdString()));
    const QByteArray user = builtLine(IrcCommandBuilder::user(
        m_config.username.toStdString(), m_config.realname.toStdString()));
    if ((sendPass && pass.isEmpty())
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

bool IrcSession::tryRegistrationNickFallback()
{
    if (!m_registrationSent)
        return false;

    QString fallback;
    RegistrationNick next = RegistrationNick::Digit;
    switch (m_registrationNick) {
    case RegistrationNick::Configured:
        fallback = m_config.nick + QLatin1Char('_');
        next = RegistrationNick::Underscore;
        break;
    case RegistrationNick::Underscore:
        fallback = m_config.nick + QLatin1Char('2');
        next = RegistrationNick::Digit;
        break;
    case RegistrationNick::Digit:
        return false;
    }

    const QByteArray line = builtLine(IrcCommandBuilder::nick(fallback.toStdString()));
    if (line.isEmpty())
        return false;

    m_registrationNick = next;
    m_nick = fallback;
    sendLine(line);
    return true;
}

void IrcSession::sendLine(const QByteArray &line)
{
    if (line.isEmpty())
        return;
    if (ircStatusKeepsOutgoing(line))
        emit statusEntry(IrcStatusEntry::outgoing(m_config.networkId, line, m_channelTypes));
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
    if (m_pingWatchdog == PingWatchdog::Watching)
        armPingWatchdog();

    const IrcFrameResult result = m_framer.feed(
        std::string_view(bytes.constData(), std::size_t(bytes.size())));
    for (const IrcFrameFault& fault : result.faults) {
        emit errorOccurred(m_config.networkId,
                           ErrorKind::Protocol,
                           describeMalformed("frame", fault.error, fault.preview,
                                             fault.byteCount, m_channelTypes));
    }

    for (const std::string &frame : result.frames) {
        const IrcParseResult parsed = IrcParser::parse(frame);
        if (!parsed) {
            emit errorOccurred(m_config.networkId,
                               ErrorKind::Protocol,
                               describeMalformed("message", parsed.error, frame,
                                                 frame.size(), m_channelTypes));
            continue;
        }
        handleMessage(*parsed.value);
    }
}

bool IrcSession::allowCtcpReply(const QString &nick)
{
    const QString key = nick.toCaseFolded();
    QElapsedTimer &clock = m_ctcpReplyClock[key];
    if (clock.isValid() && clock.elapsed() < kCtcpReplyIntervalMs)
        return false;
    clock.start();
    return true;
}

void IrcSession::handleMessage(const IrcMessage &message)
{
    if (m_ignoreFilter && m_ignoreFilter(message, m_nick))
        return;

    if (ircStatusKeepsIncoming(message, m_nick, m_channelTypes))
        emit statusEntry(IrcStatusEntry::incoming(m_config.networkId, message, m_channelTypes));
    applyIsupport(message);

    if (message.command == "BATCH") {
        handleBatch(message);
        return;
    }
    if (captureInBatch(message))
        return;

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

    if (message.command == "PONG" && pongMatchesWatchdog(message)) {
        armPingWatchdog();
        return;
    }

    if (message.command == "PRIVMSG" && message.parameters.size() >= 2
        && nicksEqual(parameter(message, 0), m_nick)) {
        const auto request = parseCtcpRequest(parameter(message, 1));
        if (request && request->command != QStringLiteral("ACTION")) {
            const QString sender = ircPrefixNick(message);
            if (sender.isEmpty()
                || ircIsServiceIdentity(sender, ctcpReplyHost(message), m_channelTypes)) {
                return;
            }
            QString payload;
            if (request->command == QStringLiteral("PING")) {
                if (request->argument.toUtf8().size() > kCtcpPingPayloadMaxBytes)
                    return;
                payload = ctcpPayload({QStringLiteral("PING"), request->argument});
            } else if (request->command == QStringLiteral("TIME")) {
                payload = ctcpPayload({
                    QStringLiteral("TIME"),
                    QDateTime::currentDateTime().toString(Qt::RFC2822Date),
                });
            } else if (request->command == QStringLiteral("VERSION")) {
                payload = ctcpPayload({
                    QStringLiteral("VERSION"),
                    QStringLiteral("Omairc %1").arg(QString::fromLatin1(OMAIRC_VERSION)),
                });
            } else {
                return;
            }
            if (!allowCtcpReply(sender))
                return;
            sendNotice(sender, payload);
            return;
        }
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
        handleWelcome(message);
        return;
    }
    if (message.command == "903") {
        if (m_saslPending) {
            m_saslPending = false;
            m_saslSucceeded = true;
            endCapabilityNegotiation();
            setState(State::Registering);
        }
        return;
    }
    if (message.command == "774") {
        handleMetadataSyncLater(message);
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
        m_capabilityNegotiationEnded = true;
        m_capabilityTimer->cancel();
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
        || message.command == "436" || message.command == "437"
        || message.command == "451" || message.command == "462"
        || message.command == "465") {
        if (m_state != State::Registered) {
            if (message.command == "433" && tryRegistrationNickFallback())
                return;
            fail(ErrorKind::Registration,
                 QStringLiteral("IRC registration was refused (%1)")
                     .arg(ircWireText(message.command)),
                 false);
            return;
        }
    }
    if (message.command == "ERROR") {
        emit messageReceived(m_config.networkId, message);
        fail(ErrorKind::Network,
             message.parameters.empty()
                 ? QStringLiteral("IRC server reported an error")
                 : ircWireText(message.parameters.back()),
             true);
        return;
    }
    if (message.command == "NICK") {
        const QString oldNick = claimedNick(message);
        const QString newNick = parameter(message, 0);
        if (!oldNick.isEmpty() && !newNick.isEmpty()
            && nicksEqual(oldNick, m_nick)) {
            m_nick = newNick;
        }
    }

    if (message.command == "FAIL")
        handleChatHistoryFail(message);

    if (message.command == "366" && message.parameters.size() >= 2)
        probeChannelAway(parameter(message, 1));

    emit messageReceived(m_config.networkId, message);

    if (message.command == "JOIN" && selfPrefixed(message)) {
        const QString channel = parameter(message, 0);
        if (!channel.isEmpty()) {
            bumpHistoryGeneration(channel);
            requestChannelHistory(channel);
        }
    } else if (message.command == "PART" && selfPrefixed(message)) {
        const QString channel = parameter(message, 0);
        if (!channel.isEmpty()) {
            bumpHistoryGeneration(channel);
            forgetChannelHistory(channel);
        }
    } else if (message.command == "KICK" && selfIs(parameter(message, 1))) {
        const QString channel = parameter(message, 0);
        if (!channel.isEmpty()) {
            bumpHistoryGeneration(channel);
            forgetChannelHistory(channel);
        }
    }
}

void IrcSession::handleBatch(const IrcMessage &message)
{
    if (message.parameters.empty())
        return;
    const QString token = parameter(message, 0);
    if (token.size() < 2)
        return;
    const QString reference = token.mid(1);
    if (reference.isEmpty())
        return;
    if (token.startsWith(QLatin1Char('+'))) {
        const QString parent = tagValue(message, "batch");
        const QString type = parameter(message, 1);
        if (m_openBatches.contains(reference))
            return;
        if (!historyCapabilitiesEnabled() && isHistoryBatch(type, parent)) {
            ignoreBatch(reference);
            return;
        }
        // A history batch that answers a request we are still waiting on is
        // never crowded out. Everything else shares the open-batch budget, so
        // a server cannot make us hold state for batches we did not ask for.
        const bool solicited = isChatHistoryBatchType(type)
            && answersPendingHistory(parameter(message, 2));
        const bool overOpenCap =
            !solicited && m_openBatches.size() >= kMaxOpenBatches;
        if (m_ignoredBatches.contains(reference)
            || (!parent.isEmpty() && m_ignoredBatches.contains(parent))
            || (overOpenCap && isHistoryBatch(type, parent))) {
            ignoreBatch(reference);
            if (isChatHistoryBatchType(type))
                clearHistoryPending(parameter(message, 2));
            return;
        }
        if (overOpenCap)
            return;
        OpenBatch frame;
        frame.type = type;
        frame.parent = parent;
        const QString parentRoot = m_openBatches.contains(frame.parent)
            ? m_openBatches.value(frame.parent).replayRoot
            : QString{};
        if (!parentRoot.isEmpty())
            frame.replayRoot = parentRoot;
        else if (isChatHistoryBatchType(frame.type))
            frame.replayRoot = reference;
        if (frame.replayRoot == reference) {
            frame.collected.target = parameter(message, 2);
            frame.generation = historyGeneration(frame.collected.target);
        }
        m_openBatches.insert(reference, frame);
        return;
    }
    if (token.startsWith(QLatin1Char('-')))
        closeBatch(reference);
}

void IrcSession::closeBatch(const QString& reference)
{
    if (m_ignoredBatches.remove(reference))
        return;
    const auto found = m_openBatches.find(reference);
    if (found == m_openBatches.end())
        return;
    const OpenBatch frame = found.value();
    m_openBatches.erase(found);
    QSet<QString> children;
    auto it = m_openBatches.begin();
    while (it != m_openBatches.end()) {
        if (it.value().replayRoot == reference) {
            children.insert(it.key());
            it = m_openBatches.erase(it);
        } else {
            ++it;
        }
    }
    for (const QString& child : children)
        ignoreBatch(child);
    if (frame.replayRoot == reference && !frame.collected.target.isEmpty()) {
        const QString folded = foldChannel(frame.collected.target);
        const bool currentMembership =
            frame.generation == historyGeneration(frame.collected.target);
        if (currentMembership)
            m_historyPending.remove(folded);
        if (!currentMembership)
            return;
        emit historyBatchReceived(m_config.networkId, frame.collected);
    }
}

bool IrcSession::captureInBatch(const IrcMessage& message)
{
    const QString batch = tagValue(message, "batch");
    if (batch.isEmpty())
        return false;
    if (m_ignoredBatches.contains(batch))
        return true;
    const auto found = m_openBatches.constFind(batch);
    if (found == m_openBatches.cend())
        return false;
    if (found.value().replayRoot.isEmpty())
        return false;
    const auto root = m_openBatches.find(found.value().replayRoot);
    if (root == m_openBatches.end())
        return false;
    if (int(root.value().collected.lines.size()) < kHistoryBufferCeiling)
        root.value().collected.lines.push_back(message);
    return true;
}

bool IrcSession::isChatHistoryBatchType(const QString& type) noexcept
{
    return type.compare(QLatin1String("chathistory"), Qt::CaseInsensitive) == 0
        || type.compare(QLatin1String("draft/chathistory"), Qt::CaseInsensitive) == 0;
}

bool IrcSession::selfPrefixed(const IrcMessage& message) const
{
    return selfIs(claimedNick(message));
}

bool IrcSession::selfIs(const QString& nick) const
{
    return !nick.isEmpty() && nicksEqual(nick, m_nick);
}

void IrcSession::requestChannelHistory(const QString& channel)
{
    if (channel.isEmpty())
        return;
    const IrcCapabilitySet enabled = m_capabilities.enabled();
    if (!enabled.contains(IrcCapability::ChatHistory)
        || !enabled.contains(IrcCapability::Batch)) {
        return;
    }
    const QString key = foldChannel(channel);
    if (m_historyAsked.contains(key))
        return;
    m_historyAsked.insert(key);
    m_historyPending.insert(key, historyGeneration(channel));
    // Both placeholders are filled in one pass. Chaining arg() would let a
    // channel name containing %2 swallow the limit.
    sendCommand(QStringLiteral("CHATHISTORY LATEST %1 * %2")
                    .arg(channel, QString::number(m_historyLimit)));
}

void IrcSession::forgetChannelHistory(const QString& channel)
{
    const QString folded = foldChannel(channel);
    m_historyAsked.remove(folded);
    m_historyPending.remove(folded);
    dropHistoryBatches(channel);
}

void IrcSession::dropHistoryBatches(const QString& channel)
{
    const QString folded = foldChannel(channel);
    QSet<QString> roots;
    for (auto it = m_openBatches.constBegin(); it != m_openBatches.constEnd(); ++it) {
        if (it.value().replayRoot == it.key()
            && foldChannel(it.value().collected.target) == folded) {
            roots.insert(it.key());
        }
    }
    if (roots.isEmpty())
        return;
    QSet<QString> gone;
    auto it = m_openBatches.begin();
    while (it != m_openBatches.end()) {
        if (roots.contains(it.key()) || roots.contains(it.value().replayRoot)) {
            gone.insert(it.key());
            it = m_openBatches.erase(it);
        } else {
            ++it;
        }
    }
    for (const QString& reference : gone)
        ignoreBatch(reference);
}

void IrcSession::bumpHistoryGeneration(const QString& channel)
{
    const QString folded = foldChannel(channel);
    m_historyGeneration[folded] = historyGeneration(channel) + 1;
}

int IrcSession::historyGeneration(const QString& channel) const
{
    return m_historyGeneration.value(foldChannel(channel), 0);
}

QString IrcSession::foldChannel(const QString& channel) const
{
    return ircWireText(m_caseMapping.normalize(utf8(channel)));
}

void IrcSession::ignoreBatch(const QString& reference)
{
    if (reference.isEmpty())
        return;
    m_openBatches.remove(reference);
    if (m_ignoredBatches.contains(reference))
        return;
    // At the ceiling we stop remembering references rather than start
    // discarding tagged lines we cannot account for. A stray replay line
    // showing up live is a smaller failure than dropping live traffic.
    if (m_ignoredBatches.size() >= kMaxIgnoredBatches)
        return;
    m_ignoredBatches.insert(reference);
}

void IrcSession::clearHistoryPending(const QString& channel)
{
    m_historyPending.remove(foldChannel(channel));
}

bool IrcSession::nicksEqual(const QString& left, const QString& right) const
{
    return m_caseMapping.equals(utf8(left), utf8(right));
}

bool IrcSession::answersPendingHistory(const QString& channel) const
{
    if (channel.isEmpty())
        return false;
    const QString folded = foldChannel(channel);
    const auto pending = m_historyPending.constFind(folded);
    return pending != m_historyPending.cend()
        && pending.value() == historyGeneration(channel)
        && !hasOpenCurrentHistoryBatch(channel);
}

bool IrcSession::hasOpenCurrentHistoryBatch(const QString& channel) const
{
    const QString folded = foldChannel(channel);
    const int generation = historyGeneration(channel);
    for (auto it = m_openBatches.constBegin(); it != m_openBatches.constEnd(); ++it) {
        if (it.value().replayRoot != it.key())
            continue;
        if (foldChannel(it.value().collected.target) != folded)
            continue;
        if (it.value().generation == generation)
            return true;
    }
    return false;
}

bool IrcSession::historyCapabilitiesEnabled() const
{
    const IrcCapabilitySet enabled = m_capabilities.enabled();
    return enabled.contains(IrcCapability::ChatHistory)
        && enabled.contains(IrcCapability::Batch);
}

void IrcSession::abandonHistoryRequests()
{
    QSet<QString> targets;
    for (auto it = m_openBatches.constBegin(); it != m_openBatches.constEnd(); ++it) {
        if (it.value().replayRoot == it.key()
            && !it.value().collected.target.isEmpty()) {
            targets.insert(it.value().collected.target);
        }
    }
    for (const QString& target : targets)
        dropHistoryBatches(target);
    m_historyPending.clear();
}

bool IrcSession::isHistoryBatch(const QString& type, const QString& parent) const
{
    if (isChatHistoryBatchType(type))
        return true;
    if (parent.isEmpty())
        return false;
    if (m_ignoredBatches.contains(parent))
        return true;
    const auto found = m_openBatches.constFind(parent);
    return found != m_openBatches.cend() && !found.value().replayRoot.isEmpty();
}

void IrcSession::handleChatHistoryFail(const IrcMessage& message)
{
    if (parameter(message, 0).compare(QLatin1String("CHATHISTORY"),
                                      Qt::CaseInsensitive)
        != 0) {
        return;
    }
    if (message.parameters.size() < 4)
        return;
    // FAIL CHATHISTORY <code> <subcommand> [<target>] [<context>] :description.
    // The target's position moves with the code, so look for the request this
    // answers instead of indexing. The trailing description is skipped so a
    // channel named in prose cannot cancel a different request.
    for (std::size_t index = 2; index + 1 < message.parameters.size(); ++index) {
        const QString channel = parameter(message, index);
        if (!answersPendingHistory(channel))
            continue;
        m_historyPending.remove(foldChannel(channel));
        return;
    }
}

void IrcSession::handleCap(const IrcMessage &message)
{
    const int lsIndex = parameterIndex(message, QStringLiteral("LS"));
    if (lsIndex >= 0) {
        m_capabilityListSeen = true;
        m_capabilities.advertise(capabilityTokens(message, lsIndex));
        const bool continuation = message.parameters.size() > std::size_t(lsIndex + 1)
            && parameter(message, std::size_t(lsIndex + 1)) == QStringLiteral("*");
        if (continuation)
            return;
        requestCapabilities();
        return;
    }

    // Every other CAP subcommand answers a negotiation we started. Acting on
    // one before CAP LS would register the connection with nothing agreed.
    if (!m_capabilityListSeen)
        return;

    const int newIndex = parameterIndex(message, QStringLiteral("NEW"));
    if (newIndex >= 0) {
        m_capabilities.advertise(capabilityTokens(message, newIndex));
        requestCapabilities();
        return;
    }

    const int delIndex = parameterIndex(message, QStringLiteral("DEL"));
    if (delIndex >= 0) {
        m_capabilities.withdraw(capabilityTokens(message, delIndex));
        publishCapabilities();
        if (!historyCapabilitiesEnabled())
            abandonHistoryRequests();
        requestCapabilities();
        return;
    }

    const int ackIndex = parameterIndex(message, QStringLiteral("ACK"));
    if (ackIndex >= 0) {
        const IrcCapabilitySet granted =
            m_capabilities.acknowledge(capabilityTokens(message, ackIndex));
        publishCapabilities();
        if (granted.contains(IrcCapability::Sasl)) {
            setState(State::Sasl);
            sendLine(QByteArrayLiteral("AUTHENTICATE PLAIN\r\n"));
            return;
        }
        if (m_state == State::Registered)
            subscribeToMemberMetadata();
        endCapabilityNegotiation();
        return;
    }

    const int nakIndex = parameterIndex(message, QStringLiteral("NAK"));
    if (nakIndex >= 0) {
        const IrcCapabilitySet refused =
            m_capabilities.reject(capabilityTokens(message, nakIndex));
        if (refused.contains(IrcCapability::Sasl)) {
            m_saslPending = false;
            fail(ErrorKind::Authentication,
                 QStringLiteral("Server rejected the SASL capability"),
                 false);
            return;
        }
        requestCapabilities();
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
    const QByteArray secret = saslSecret(m_config).toUtf8();
    QByteArray plain;
    plain.reserve(nick.size() * 2 + secret.size() + 2);
    plain.append(nick);
    plain.append('\0');
    plain.append(nick);
    plain.append('\0');
    plain.append(secret);
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

void IrcSession::applyIsupport(const IrcMessage &message)
{
    if (message.command != "005" || message.parameters.size() <= 2)
        return;
    constexpr QLatin1String channelTypes("CHANTYPES=");
    constexpr QLatin1String caseMapping("CASEMAPPING=");
    constexpr QLatin1String historyLimit("CHATHISTORY=");
    const auto last = message.parameters.end() - 1;
    for (auto it = message.parameters.begin() + 1; it != last; ++it) {
        const QString token = ircWireText(*it);
        if (token.startsWith(channelTypes)) {
            m_channelTypes = token.mid(channelTypes.size());
        } else if (token.startsWith(caseMapping)) {
            const QString name = token.mid(caseMapping.size());
            if (const auto mapping = IrcCaseMapping::fromName(utf8(name)))
                m_caseMapping = *mapping;
        } else if (token.startsWith(historyLimit)) {
            bool parsed = false;
            const int maximum = token.mid(historyLimit.size()).toInt(&parsed);
            if (parsed) {
                // The token is the server's per-command maximum. Zero means it
                // does not impose one.
                m_historyLimit = maximum > 0
                    ? std::min(kHistoryLimit, maximum)
                    : kHistoryLimit;
            }
        }
    }
}

void IrcSession::handleWelcome(const IrcMessage &message)
{
    if (m_state == State::Registered || m_state == State::Failed)
        return;

    const QString assigned = parameter(message, 0);
    if (!assigned.isEmpty())
        m_nick = assigned;

    m_reconnectAttempt = 0;
    m_reportedRetryErrors = 0;
    m_capabilityTimer->cancel();
    m_capabilities.abandonOutstanding();
    setState(State::Registered);
    emit registered(m_config.networkId);
    armPingWatchdog();
    subscribeToMemberMetadata();
    if (!m_config.nickServPassword.isEmpty() && !m_saslSucceeded) {
        if (!m_config.tlsEnabled) {
            emit statusEntry(IrcStatusEntry::lifecycle(
                m_config.networkId, IrcLogSeverity::Alert,
                QStringLiteral("identify"),
                QStringLiteral("NickServ identify will be sent in clear text")));
        }
        sendPrivmsg(QStringLiteral("NickServ"),
                    QStringLiteral("IDENTIFY ") + m_config.nickServPassword);
    }
    for (const QString &channel : m_config.autojoinChannels) {
        const std::optional<IrcJoinTarget> target = IrcJoinTarget::make(channel);
        if (!target || !join(*target)) {
            emit errorOccurred(m_config.networkId,
                               ErrorKind::Protocol,
                               QStringLiteral("Invalid autojoin channel"));
        }
    }
}

void IrcSession::fail(ErrorKind kind, const QString &message, bool reconnect)
{
    cancelPingWatchdog();
    if (reconnect) {
        const quint32 bit = quint32(1) << int(kind);
        if ((m_reportedRetryErrors & bit) == 0) {
            m_reportedRetryErrors |= bit;
            emit errorOccurred(m_config.networkId, kind, message);
        }
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

    emit errorOccurred(m_config.networkId, kind, message);
    m_expectedDisconnect = true;
    m_reconnectTimer->cancel();
    setState(State::Failed);
    m_transport->shutdown();
}

void IrcSession::scheduleReconnect()
{
    if (!m_config.reconnectEnabled) {
        setState(State::Failed);
        return;
    }
    if (m_state == State::Reconnecting)
        return;

    ++m_reconnectAttempt;
    const int delay = reconnectDelay();
    setState(State::Reconnecting);
    m_reconnectTimer->start(delay);
    emit reconnectScheduled(m_config.networkId, delay, m_reconnectAttempt);
}

void IrcSession::beginReconnectAttempt()
{
    if (m_state != State::Reconnecting)
        return;
    m_reconnectTimer->cancel();
    resetForConnection();
    setState(State::Connecting);
    m_transport->connectToHost(m_config.host, m_config.port, m_config.tlsEnabled);
}

void IrcSession::resetForConnection()
{
    m_framer = IrcFramer{};
    m_openBatches.clear();
    m_ignoredBatches.clear();
    m_historyAsked.clear();
    m_historyPending.clear();
    m_historyGeneration.clear();
    m_historyLimit = kHistoryLimit;
    m_caseMapping = IrcCaseMapping{IrcCaseMapping::Kind::Rfc1459};
    m_nick = m_config.nick;
    m_registrationSent = false;
    m_registrationNick = RegistrationNick::Configured;
    m_saslRequested = false;
    m_saslPending = false;
    m_saslSucceeded = false;
    m_capabilityNegotiationEnded = false;
    m_capabilityListSeen = false;
    m_channelTypes.clear();
    m_capabilityTimer->cancel();
    cancelPingWatchdog();
    m_capabilities.reset(!saslSecret(m_config).isEmpty());
    m_typing.reset();
    publishCapabilities();
}

void IrcSession::armPingWatchdog()
{
    if (m_state != State::Registered)
        return;
    m_pingWatchdog = PingWatchdog::Watching;
    m_pingTimer->start(std::max(0, m_config.pingTimeoutMilliseconds));
}

void IrcSession::cancelPingWatchdog()
{
    m_pingWatchdog = PingWatchdog::Off;
    m_pingTimer->cancel();
}

void IrcSession::onPingWatchdogFired()
{
    if (m_state != State::Registered)
        return;
    if (m_pingWatchdog == PingWatchdog::Watching) {
        m_pingWatchdog = PingWatchdog::Probing;
        sendLine(QByteArrayLiteral("PING :") + kPingWatchdogToken + QByteArrayLiteral("\r\n"));
        m_pingTimer->start(std::max(0, m_config.pingTimeoutMilliseconds));
        return;
    }
    if (m_pingWatchdog == PingWatchdog::Probing) {
        fail(ErrorKind::Network, QStringLiteral("Ping timeout"), true);
    }
}

bool IrcSession::pongMatchesWatchdog(const IrcMessage &message) const
{
    if (m_pingWatchdog != PingWatchdog::Probing || message.parameters.empty())
        return false;
    return message.parameters.back() == kPingWatchdogToken;
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
