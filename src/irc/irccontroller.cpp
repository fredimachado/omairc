#include "irccontroller.h"

#include "irceventtranslator.h"

#include <QByteArray>
#include <QDateTime>

namespace
{
QString parameter(const IrcMessage& message, std::size_t index)
{
    if (index >= message.parameters.size())
        return {};
    return QString::fromUtf8(message.parameters[index].data(),
                             qsizetype(message.parameters[index].size()));
}

std::string utf8(const QString& value)
{
    const QByteArray bytes = value.toUtf8();
    return std::string(bytes.constData(), std::size_t(bytes.size()));
}

QString stateText(IrcSession::State state)
{
    switch (state) {
    case IrcSession::State::Idle:
        return QStringLiteral("Offline");
    case IrcSession::State::Connecting:
    case IrcSession::State::CapLs:
    case IrcSession::State::CapReq:
    case IrcSession::State::Sasl:
    case IrcSession::State::Registering:
        return QStringLiteral("Connecting");
    case IrcSession::State::Registered:
        return QStringLiteral("Connected");
    case IrcSession::State::Closing:
        return QStringLiteral("Disconnecting");
    case IrcSession::State::Reconnecting:
        return QStringLiteral("Reconnecting");
    case IrcSession::State::Failed:
        return QStringLiteral("Offline");
    }
    return QStringLiteral("Offline");
}
}

IrcController::IrcController(QObject *parent)
    : QObject(parent)
    , m_conversations(m_reducer)
    , m_messages(m_reducer)
    , m_members(m_reducer)
{
}

IrcSession *IrcController::addSession(const IrcSessionConfig& config,
                                      IrcTransport *transport,
                                      IrcReconnectTimer *reconnectTimer)
{
    IrcSession *session = m_sessions.createSession(config, transport, reconnectTimer);
    if (!session)
        return nullptr;

    m_currentNicks.insert(config.networkId, config.nick);
    connect(session, &IrcSession::registered, this,
            [this, session](const QString& networkId) {
        m_currentNicks[networkId] = session->nick();
        apply(IrcWelcomeEvent{networkId, session->nick()});
        updateStatus(session);
    });
    connect(session, &IrcSession::messageReceived,
            this, &IrcController::handleMessage);
    connect(session, &IrcSession::stateChanged, this,
            [this, session](IrcSession::State) { updateStatus(session); });
    connect(session, &IrcSession::errorOccurred, this,
            [this](const QString&, IrcSession::ErrorKind, const QString& message) {
        m_lastError = message;
        emit statusChanged();
    });
    return session;
}

QAbstractItemModel *IrcController::conversations()
{
    return &m_conversations;
}

QAbstractItemModel *IrcController::messages()
{
    return &m_messages;
}

QAbstractItemModel *IrcController::members()
{
    return &m_members;
}

QString IrcController::selectedNetworkId() const
{
    return m_selected ? m_selected->networkId : QString{};
}

QString IrcController::selectedTarget() const
{
    return m_selectedTarget;
}

QString IrcController::topic() const
{
    if (!m_selected)
        return {};
    const IrcConversationState *conversation = m_reducer.find(*m_selected);
    if (!conversation)
        return isChannel() ? QString{} : QStringLiteral("Direct message");
    const IrcChannelState *channel = conversation->channel();
    return channel ? channel->topic
                   : QStringLiteral("Direct message with %1").arg(conversation->target);
}

bool IrcController::isChannel() const
{
    if (!m_selected)
        return false;
    return m_reducer.serverFeatures(m_selected->networkId)
        .isChannel(utf8(m_selectedTarget));
}

int IrcController::peopleCount() const
{
    if (!m_selected)
        return 0;
    const IrcConversationState *conversation = m_reducer.find(*m_selected);
    return conversation ? conversation->peopleCount() : 0;
}

QString IrcController::connectionStatus() const
{
    return m_connectionStatus;
}

QString IrcController::lastError() const
{
    return m_lastError;
}

QString IrcController::currentNick() const
{
    return m_selected ? m_currentNicks.value(m_selected->networkId) : QString{};
}

bool IrcController::start(const QString& networkId)
{
    IrcSession *session = m_sessions.findSession(networkId);
    if (!session) {
        m_lastError = QStringLiteral("That network is not configured");
        emit statusChanged();
        return false;
    }
    m_lastError.clear();
    const bool started = m_sessions.activateSession(networkId);
    updateStatus(session);
    return started;
}

void IrcController::selectConversation(const QString& networkId,
                                       const QString& target)
{
    if (networkId.isEmpty() || target.isEmpty())
        return;
    const IrcConversationKey key = m_reducer.conversationKey(networkId, target);
    m_selected = key;
    m_selectedTarget = target;
    m_conversations.select(key);
    m_messages.select(key);
    m_members.select(key);
    if (IrcSession *session = m_sessions.findSession(networkId))
        updateStatus(session);
    emit selectionChanged();
}

void IrcController::openDirectMessage(const QString& nick)
{
    if (!m_selected || nick.isEmpty())
        return;
    selectConversation(m_selected->networkId, nick);
}

bool IrcController::sendMessage(const QString& text)
{
    const QString input = text.trimmed();
    IrcSession *session = selectedSession();
    if (!session || !m_selected || input.isEmpty()) {
        m_lastError = QStringLiteral("Select a connected conversation first");
        emit statusChanged();
        return false;
    }

    bool sent = false;
    if (input.startsWith(QStringLiteral("/me "))) {
        const QString action = input.mid(4).trimmed();
        sent = session->sendAction(m_selectedTarget, action);
        if (sent)
            echoLocal(IrcMessageKind::Action, action);
    } else if (input.startsWith(QStringLiteral("/join "))) {
        sent = session->join(input.mid(6).trimmed());
    } else if (input == QStringLiteral("/part")) {
        sent = isChannel() && session->part(m_selectedTarget);
    } else if (input.startsWith(QStringLiteral("/nick "))) {
        sent = session->changeNick(input.mid(6).trimmed());
    } else if (input == QStringLiteral("/quit")) {
        sent = session->quit();
    } else if (input.startsWith(QStringLiteral("/quit "))) {
        sent = session->quit(input.mid(6).trimmed());
    } else if (input.startsWith(QLatin1Char('/'))) {
        m_lastError = QStringLiteral("That command is not supported");
        emit statusChanged();
        return false;
    } else {
        sent = session->sendPrivmsg(m_selectedTarget, input);
        if (sent)
            echoLocal(IrcMessageKind::Message, input);
    }

    m_lastError = sent ? QString{} : QStringLiteral("Message was not sent");
    emit statusChanged();
    return sent;
}

void IrcController::echoLocal(IrcMessageKind kind, const QString& body)
{
    if (!m_selected || body.isEmpty())
        return;
    const QDateTime now = QDateTime::currentDateTimeUtc();
    const QString nick = currentNick();
    if (kind == IrcMessageKind::Action) {
        apply(IrcActionEvent{*m_selected, nick, body, now, m_selectedTarget});
        return;
    }
    apply(IrcMessageEvent{*m_selected, nick, body, now, m_selectedTarget});
}

void IrcController::apply(const IrcEvent& event)
{
    m_reducer.apply(event);
    if (!m_selected && !m_reducer.conversations().empty()) {
        const IrcConversationState& conversation =
            m_reducer.conversations().begin()->second;
        m_selected = conversation.key;
        m_selectedTarget = conversation.target;
        m_reducer.markSelected(conversation.key);
        m_messages.select(conversation.key);
        m_members.select(conversation.key);
    }
    reloadModels();
    emit selectionChanged();
}

void IrcController::handleMessage(const QString& networkId,
                                  const IrcMessage& message)
{
    if (message.command == "005") {
        IrcServerFeatures features = m_reducer.serverFeatures(networkId);
        if (message.parameters.size() > 2) {
            std::vector<std::string> tokens(
                message.parameters.begin() + 1, message.parameters.end() - 1);
            features.applyTokens(tokens);
            m_reducer.setServerFeatures(networkId, features);
            reloadModels();
            emit selectionChanged();
        }
        return;
    }
    if (message.command == "333" && message.parameters.size() >= 3) {
        const QString channel = parameter(message, 1);
        const IrcConversationKey key = m_reducer.conversationKey(networkId, channel);
        const IrcConversationState *conversation = m_reducer.find(key);
        const IrcChannelState *state = conversation ? conversation->channel() : nullptr;
        apply(IrcTopicEvent{
            networkId, channel, state ? state->topic : QString{},
            parameter(message, 2)});
        return;
    }

    const IrcServerFeatures& features = m_reducer.serverFeatures(networkId);
    const QString currentNick = m_currentNicks.value(networkId);
    for (const IrcEvent& event :
         IrcEventTranslator::translate(networkId, currentNick, features, message)) {
        if (const auto *nick = std::get_if<IrcNickEvent>(&event)) {
            if (features.caseMapping().equals(
                    utf8(nick->oldNick), utf8(currentNick))) {
                m_currentNicks[networkId] = nick->newNick;
            }
        }
        apply(event);
    }
}

void IrcController::reloadModels()
{
    m_conversations.reload();
    m_messages.reload();
    m_members.reload();
}

IrcSession *IrcController::selectedSession() const
{
    return m_selected ? m_sessions.findSession(m_selected->networkId) : nullptr;
}

void IrcController::updateStatus(IrcSession *session)
{
    if (!session)
        return;
    if (!m_selected || m_selected->networkId == session->networkId()
        || m_connectionStatus == QStringLiteral("Offline")) {
        m_connectionStatus = stateText(session->state());
        emit statusChanged();
    }
}
