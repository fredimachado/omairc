#include "irccontroller.h"

#include "ircchannelmode.h"
#include "irccommand.h"
#include "irceventtranslator.h"
#include "ircnetworkprofile.h"
#include "ircviewnotify.h"
#include "irctyping.h"
#include "ircwiretext.h"

#include <QByteArray>
#include <QDateTime>
#include <variant>

namespace
{
QString firstToken(const QString& argument)
{
    const int space = argument.indexOf(QLatin1Char(' '));
    return space < 0 ? argument : argument.left(space);
}

QString restAfterFirstToken(const QString& argument)
{
    const int space = argument.indexOf(QLatin1Char(' '));
    return space < 0 ? QString() : argument.mid(space + 1).trimmed();
}

QString parameter(const IrcMessage& message, std::size_t index)
{
    if (index >= message.parameters.size())
        return {};
    return ircWireText(message.parameters[index]);
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
    , m_console(m_sessions, [this](const IrcCommand& command) {
        return dispatch(command, IrcComposerSurface::Status);
    })
    , m_conversations(m_reducer)
    , m_messages(m_reducer)
    , m_members(m_reducer)
{
    m_typingRefresh.setSingleShot(true);
    connect(&m_typingRefresh, &QTimer::timeout, this, [this] {
        emit typingChanged();
        armTypingRefresh();
    });
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
    connect(session, &IrcSession::capabilitiesChanged,
            this, &IrcController::handleCapabilities);
    connect(session, &IrcSession::stateChanged, this,
            [this, session](IrcSession::State) { updateStatus(session); });
    connect(session, &IrcSession::errorOccurred, this,
            [this](const QString& networkId, IrcSession::ErrorKind kind, const QString& message) {
        setLastError(networkId, message);
        emit statusChanged();
        emit errorOccurred(networkId, kind, message);
    });
    m_console.observe(session);
    return session;
}

bool IrcController::discardSession(const QString &networkId)
{
    if (!m_sessions.findSession(networkId))
        return false;
    const QString previousId = identityNetworkId();
    const bool previousAway = selfAway();
    m_reducer.apply(IrcSelfAwayEvent{networkId, false});
    m_unawaySent.remove(networkId);
    m_currentNicks.remove(networkId);
    m_capabilities.remove(networkId);
    m_console.forget(networkId);
    emit capabilitiesChanged();
    const bool discarded = m_sessions.discardSession(networkId);
    notifySelfAwayIfChanged(previousId, previousAway);
    return discarded;
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
    if (m_selected) {
        if (const IrcConversationState *conversation = m_reducer.find(*m_selected))
            return conversation->target;
    }
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
        .isChannel(utf8(selectedTarget()));
}

int IrcController::peopleCount() const
{
    if (!m_selected)
        return 0;
    return m_members.rowCount();
}

QString IrcController::connectionStatus() const
{
    return m_connectionStatus;
}

QString IrcController::lastError() const
{
    return lastErrorForNetwork(identityNetworkId());
}

QString IrcController::lastErrorForNetwork(const QString& networkId) const
{
    return m_lastErrors.value(networkId);
}

QString IrcController::currentNick() const
{
    return m_selected ? m_currentNicks.value(m_selected->networkId) : QString{};
}

bool IrcController::selfAway() const
{
    const QString networkId = identityNetworkId();
    return !networkId.isEmpty() && m_reducer.selfAway(networkId);
}

bool IrcController::hasAwayPresence() const
{
    return m_selected
        && m_capabilities.value(m_selected->networkId)
               .contains(IrcCapability::AwayNotify);
}

bool IrcController::hasMemberStatus() const
{
    if (!m_selected)
        return false;
    const IrcCapabilitySet capabilities = m_capabilities.value(m_selected->networkId);
    return capabilities.contains(IrcCapability::MemberMetadata)
        && capabilities.contains(IrcCapability::Batch);
}

bool IrcController::hasTyping() const
{
    return m_selected
        && m_capabilities.value(m_selected->networkId)
               .contains(IrcCapability::MessageTags);
}

QStringList IrcController::typingNicks() const
{
    if (!m_selected)
        return {};
    return m_reducer.typingNicks(*m_selected, QDateTime::currentDateTimeUtc());
}

bool IrcController::nickIsTyping(const QString& nick) const
{
    if (!m_selected || nick.isEmpty())
        return false;
    const auto& features = m_reducer.serverFeatures(m_selected->networkId);
    for (const QString& typing : typingNicks()) {
        if (features.caseMapping().equals(utf8(typing), utf8(nick)))
            return true;
    }
    return false;
}

void IrcController::notifyComposerText(const QString& text)
{
    m_composerDraft = text;
    if (m_console.isOpen())
        return;
    IrcSession *session = selectedSession();
    if (!session || !m_selected || !hasTyping())
        return;

    const IrcCommand command = IrcCommand::parse(text);
    const QString target = selectedTarget();
    if (command.isLiveMessage()) {
        session->sendTyping(target, IrcTypingPhase::Active);
        m_typingTarget = target;
        return;
    }
    if (m_typingTarget != target)
        return;
    session->sendTyping(target, IrcTypingPhase::Done);
    m_typingTarget.clear();
}

void IrcController::handleCapabilities(const QString& networkId,
                                       IrcCapabilitySet capabilities)
{
    const IrcCapabilitySet previous = m_capabilities.value(networkId);
    m_capabilities.insert(networkId, capabilities);

    const auto dropped = [&](IrcCapability capability) {
        return previous.contains(capability) && !capabilities.contains(capability);
    };
    const bool awayDropped = dropped(IrcCapability::AwayNotify);
    const bool statusDropped = dropped(IrcCapability::MemberMetadata)
        || dropped(IrcCapability::Batch);
    if (awayDropped || statusDropped) {
        m_reducer.clearPresenceFacts(networkId, awayDropped, statusDropped);
        reloadModels();
    }
    if (dropped(IrcCapability::MessageTags)) {
        m_reducer.clearTypingFacts(networkId);
        emit typingChanged();
        armTypingRefresh();
    }
    emit capabilitiesChanged();
}

IrcStatusConsole *IrcController::console()
{
    return &m_console;
}

const IrcServerFeatures &IrcController::serverFeatures(const QString &networkId) const
{
    return m_reducer.serverFeatures(networkId);
}

bool IrcController::start(const QString& networkId)
{
    IrcSession *session = m_sessions.findSession(networkId);
    if (!session) {
        setLastError(networkId, QStringLiteral("That network is not configured"));
        emit statusChanged();
        return false;
    }
    const QString previousId = identityNetworkId();
    const bool previousAway = selfAway();
    setLastError(networkId, {});
    m_console.setNetwork(networkId);
    const bool started = m_sessions.activateSession(networkId);
    updateStatus(session);
    notifySelfAwayIfChanged(previousId, previousAway);
    return started;
}

void IrcController::selectConversation(const QString& networkId,
                                       const QString& target)
{
    if (networkId.isEmpty() || target.isEmpty())
        return;
    m_console.setOpen(false);
    const QString previousId = identityNetworkId();
    const bool previousAway = selfAway();
    const IrcConversationKey key = m_reducer.conversationKey(networkId, target);
    const bool changed = !m_selected
        || m_selected->networkId != networkId
        || m_selected->normalizedTarget != key.normalizedTarget;
    if (changed && !m_typingTarget.isEmpty()) {
        if (IrcSession *previous = selectedSession())
            previous->sendTyping(m_typingTarget, IrcTypingPhase::Done);
        m_typingTarget.clear();
    }
    m_selected = key;
    m_selectedTarget = target;
    m_conversations.select(key);
    m_messages.select(key);
    m_members.select(key);
    if (IrcSession *session = m_sessions.findSession(networkId))
        updateStatus(session);
    emit selectionChanged();
    emit capabilitiesChanged();
    notifyComposerText(m_composerDraft);
    emit typingChanged();
    armTypingRefresh();
    notifySelfAwayIfChanged(previousId, previousAway);
}

void IrcController::openDirectMessage(const QString& nick)
{
    if (!m_selected || nick.isEmpty())
        return;
    const IrcConversationKey key =
        m_reducer.conversationKey(m_selected->networkId, nick);
    m_reducer.ensureConversation(key, nick);
    m_conversations.reload();
    selectConversation(m_selected->networkId, nick);
}

void IrcController::closeDirectMessage()
{
    if (!selectedIsCloseableDirect())
        return;
    const QString networkId = identityNetworkId();
    dropSelectedDirectAndReselect();
    if (lastErrorForNetwork(networkId).isEmpty())
        return;
    setLastError(networkId, {});
    emit statusChanged();
}

bool IrcController::selectedIsCloseableDirect() const
{
    if (!m_selected)
        return false;
    const IrcConversationState *conversation = m_reducer.find(*m_selected);
    return !conversation || !conversation->isChannel();
}

void IrcController::dropSelectedDirectAndReselect()
{
    if (!m_selected)
        return;
    const IrcConversationKey dropping = *m_selected;
    const QVector<IrcConversationKey> ordered = ircSidebarOrder(m_reducer);
    const std::optional<IrcConversationKey> next =
        ircNeighborAfterDrop(ordered, dropping);
    QString nextNetworkId;
    QString nextTarget;
    if (next) {
        nextNetworkId = next->networkId;
        const IrcConversationState *neighbor = m_reducer.find(*next);
        nextTarget = neighbor ? neighbor->target : next->normalizedTarget;
    }
    m_reducer.dropDirectMessage(dropping);
    reloadModels();
    if (!nextTarget.isEmpty())
        selectConversation(nextNetworkId, nextTarget);
    else
        clearConversationSelection();
}

void IrcController::clearConversationSelection()
{
    const QString previousId = identityNetworkId();
    const bool previousAway = selfAway();
    if (!m_typingTarget.isEmpty()) {
        if (IrcSession *previous = selectedSession())
            previous->sendTyping(m_typingTarget, IrcTypingPhase::Done);
        m_typingTarget.clear();
    }
    m_selected.reset();
    m_selectedTarget.clear();
    m_reducer.clearSelection();
    m_messages.clearSelection();
    m_members.clearSelection();
    emit selectionChanged();
    emit capabilitiesChanged();
    emit typingChanged();
    notifySelfAwayIfChanged(previousId, previousAway);
}

bool IrcController::sendMessage(const QString& text)
{
    const IrcCommand command = IrcCommand::parse(text);
    if (command.verb == IrcCommand::Verb::Empty)
        return false;
    return report(dispatch(command, IrcComposerSurface::Conversation), command);
}

QStringList IrcController::networkIds() const
{
    return m_sessions.networkIds();
}

IrcSession *IrcController::session(const QString &networkId) const
{
    return m_sessions.findSession(networkId);
}

bool IrcController::sendToTarget(const QString &networkId,
                                 const QString &target,
                                 const QString &text)
{
    // Explicit target path for local IPC/CLI. Does not change the UI selection.
    if (networkId.isEmpty() || target.isEmpty() || text.isEmpty()) {
        setLastError(networkId, QStringLiteral("Missing network, target, or text"));
        emit statusChanged();
        return false;
    }

    IrcSession *session = m_sessions.findSession(networkId);
    if (!session) {
        setLastError(networkId, QStringLiteral("That network is not configured"));
        emit statusChanged();
        return false;
    }
    if (session->state() != IrcSession::State::Registered) {
        setLastError(networkId, QStringLiteral("Not connected"));
        emit statusChanged();
        return false;
    }

    const bool sent = session->sendPrivmsg(target, text);
    if (!sent) {
        setLastError(networkId, QStringLiteral("Failed to send message"));
        emit statusChanged();
        return false;
    }

    echoIfPresent(session, target, text, QuietWire::Privmsg);
    unawayAfterChat(session);
    setLastError(networkId, {});
    emit statusChanged();
    return true;
}

IrcCommandOutcome IrcController::dispatch(const IrcCommand& command,
                                          IrcComposerSurface surface)
{
    if (command.verb == IrcCommand::Verb::Empty)
        return IrcCommandOutcome::Sent;
    if (command.verb == IrcCommand::Verb::Unknown)
        return IrcCommandOutcome::Unsupported;
    if (!command.allowedOn(surface))
        return IrcCommandOutcome::WrongScope;

    if (command.verb == IrcCommand::Verb::Say)
        return sendSelectedMessage(command.argument);

    if (command.verb == IrcCommand::Verb::Action) {
        IrcSession *session = selectedSession();
        if (!session || !m_selected)
            return IrcCommandOutcome::WrongScope;
        const bool sent = session->sendAction(selectedTarget(), command.argument);
        if (sent) {
            echoLocal(IrcMessageKind::Action, command.argument);
            m_typingTarget.clear();
            unawayAfterChat(session);
        }
        return sent ? IrcCommandOutcome::Sent : IrcCommandOutcome::Refused;
    }

    if (command.verb == IrcCommand::Verb::Query)
        return dispatchQuery(command, surface);

    if (quietSendFor(command.verb))
        return dispatchQuietSend(command, surface);

    if (command.verb == IrcCommand::Verb::Mode)
        return dispatchMode(command, surface);

    if (command.verb == IrcCommand::Verb::Whois)
        return dispatchWhois(command, surface);

    if (command.verb == IrcCommand::Verb::Clear)
        return clearSurface(surface);

    if (command.verb == IrcCommand::Verb::Close) {
        if (!selectedIsCloseableDirect())
            return IrcCommandOutcome::WrongScope;
        dropSelectedDirectAndReselect();
        return IrcCommandOutcome::Sent;
    }

    if (command.verb == IrcCommand::Verb::Topic)
        return setSelectedTopic(command.argument);

    IrcSession *active = m_console.boundSession();
    if (!active || active->state() != IrcSession::State::Registered)
        return IrcCommandOutcome::NotConnected;

    bool sent = false;
    switch (command.verb) {
    case IrcCommand::Verb::Join: {
        const QStringList channels =
            IrcNetworkProfile::canonicalizeChannels(command.argument);
        sent = !channels.isEmpty();
        for (const QString &channel : channels)
            sent = active->join(channel) && sent;
        break;
    }
    case IrcCommand::Verb::Part: {
        QString channel = firstToken(command.argument);
        if (channel.isEmpty()) {
            IrcSession *selected = selectedSession();
            if (m_selected && selected
                && selected->state() != IrcSession::State::Registered)
                return IrcCommandOutcome::NotConnected;
            if (m_selected && !isChannel())
                return IrcCommandOutcome::WrongScope;
            if (!selected)
                break;
            if (selected->state() != IrcSession::State::Registered)
                return IrcCommandOutcome::NotConnected;
            channel = selectedTarget();
            sent = !channel.isEmpty() && selected->part(channel);
            break;
        }
        sent = !channel.isEmpty() && active->part(channel);
        break;
    }
    case IrcCommand::Verb::Kick: {
        const QString first = firstToken(command.argument);
        if (first.isEmpty())
            return IrcCommandOutcome::Refused;
        const QString networkId = queryNetworkId(surface);
        if (m_reducer.serverFeatures(networkId).isChannel(utf8(first))) {
            const QString afterChannel = restAfterFirstToken(command.argument);
            const QString nick = firstToken(afterChannel);
            if (nick.isEmpty())
                return IrcCommandOutcome::Refused;
            sent = active->kick(first, nick, restAfterFirstToken(afterChannel));
            break;
        }
        IrcSession *selected = selectedSession();
        if (m_selected && selected
            && selected->state() != IrcSession::State::Registered)
            return IrcCommandOutcome::NotConnected;
        if (m_selected && !isChannel())
            return IrcCommandOutcome::WrongScope;
        if (!selected)
            break;
        if (selected->state() != IrcSession::State::Registered)
            return IrcCommandOutcome::NotConnected;
        const QString channel = selectedTarget();
        sent = !channel.isEmpty()
            && selected->kick(channel, first, restAfterFirstToken(command.argument));
        break;
    }
    case IrcCommand::Verb::Nick:
        sent = !command.argument.isEmpty() && active->changeNick(command.argument);
        break;
    case IrcCommand::Verb::Quit:
        sent = active->quit(command.argument);
        break;
    case IrcCommand::Verb::Away:
        sent = active->setAway(command.argument);
        break;
    case IrcCommand::Verb::Back:
        sent = active->clearAway();
        break;
    default:
        return IrcCommandOutcome::Unsupported;
    }
    return sent ? IrcCommandOutcome::Sent : IrcCommandOutcome::Refused;
}

QString IrcController::queryNetworkId(IrcComposerSurface surface) const
{
    if (surface == IrcComposerSurface::Status)
        return m_console.networkId();
    return m_selected ? m_selected->networkId : QString{};
}

IrcCommandOutcome IrcController::sendSelectedMessage(const QString& body)
{
    IrcSession *session = selectedSession();
    if (!session || !m_selected)
        return IrcCommandOutcome::WrongScope;
    const bool sent = session->sendPrivmsg(selectedTarget(), body);
    if (sent) {
        echoLocal(IrcMessageKind::Message, body);
        m_typingTarget.clear();
        unawayAfterChat(session);
    }
    return sent ? IrcCommandOutcome::Sent : IrcCommandOutcome::Refused;
}

IrcCommandOutcome IrcController::setSelectedTopic(const QString& topic)
{
    if (!m_selected || !isChannel())
        return IrcCommandOutcome::WrongScope;
    if (topic.isEmpty())
        return IrcCommandOutcome::Sent;
    IrcSession *session = selectedSession();
    if (!session || session->state() != IrcSession::State::Registered)
        return IrcCommandOutcome::NotConnected;
    return session->setTopic(selectedTarget(), topic)
        ? IrcCommandOutcome::Sent
        : IrcCommandOutcome::Refused;
}

IrcCommandOutcome IrcController::dispatchQuery(const IrcCommand& command,
                                               IrcComposerSurface surface)
{
    const QString nick = firstToken(command.argument);
    const QString rest = restAfterFirstToken(command.argument);
    if (nick.isEmpty())
        return IrcCommandOutcome::Refused;

    const QString networkId = queryNetworkId(surface);
    if (networkId.isEmpty()) {
        if (surface == IrcComposerSurface::Conversation)
            return IrcCommandOutcome::WrongScope;
        return IrcCommandOutcome::Refused;
    }

    if (m_reducer.serverFeatures(networkId).isChannel(utf8(nick)))
        return IrcCommandOutcome::Refused;

    if (!rest.isEmpty()) {
        IrcSession *session = m_sessions.findSession(networkId);
        if (!session || session->state() != IrcSession::State::Registered)
            return IrcCommandOutcome::NotConnected;
    }

    const IrcConversationKey key = m_reducer.conversationKey(networkId, nick);
    m_reducer.ensureConversation(key, nick);
    m_conversations.reload();
    selectConversation(networkId, nick);
    if (rest.isEmpty())
        return IrcCommandOutcome::Sent;
    return sendSelectedMessage(rest);
}

std::optional<IrcController::QuietSend>
IrcController::quietSendFor(IrcCommand::Verb verb)
{
    switch (verb) {
    case IrcCommand::Verb::Msg:
        return QuietSend{QuietWire::Privmsg, QuietTarget::Nick};
    case IrcCommand::Verb::Notice:
        return QuietSend{QuietWire::Notice, QuietTarget::Any};
    default:
        return std::nullopt;
    }
}

IrcCommandOutcome IrcController::dispatchQuietSend(const IrcCommand& command,
                                                   IrcComposerSurface surface)
{
    const std::optional<QuietSend> spec = quietSendFor(command.verb);
    if (!spec)
        return IrcCommandOutcome::Unsupported;

    const QString target = firstToken(command.argument);
    const QString body = restAfterFirstToken(command.argument);
    if (target.isEmpty() || body.isEmpty())
        return IrcCommandOutcome::Refused;

    const QString networkId = queryNetworkId(surface);
    if (networkId.isEmpty()) {
        if (surface == IrcComposerSurface::Conversation)
            return IrcCommandOutcome::WrongScope;
        return IrcCommandOutcome::Refused;
    }

    if (spec->target == QuietTarget::Nick
        && m_reducer.serverFeatures(networkId).isChannel(utf8(target))) {
        return IrcCommandOutcome::Refused;
    }

    IrcSession *session = m_sessions.findSession(networkId);
    if (!session || session->state() != IrcSession::State::Registered)
        return IrcCommandOutcome::NotConnected;

    const bool sent = spec->wire == QuietWire::Privmsg
        ? session->sendPrivmsg(target, body)
        : session->sendNotice(target, body);
    if (!sent)
        return IrcCommandOutcome::Refused;

    echoIfPresent(session, target, body, spec->wire);
    if (spec->wire == QuietWire::Privmsg)
        unawayAfterChat(session);
    return IrcCommandOutcome::Sent;
}

IrcCommandOutcome IrcController::dispatchMode(const IrcCommand& command,
                                              IrcComposerSurface surface)
{
    if (command.argument.isEmpty())
        return IrcCommandOutcome::Refused;

    const QString networkId = queryNetworkId(surface);
    if (networkId.isEmpty()) {
        if (surface == IrcComposerSurface::Conversation)
            return IrcCommandOutcome::WrongScope;
        return IrcCommandOutcome::Refused;
    }

    const std::optional<IrcChannelModeRequest> request =
        IrcChannelModeRequest::parse(command.argument, serverFeatures(networkId));
    if (!request)
        return IrcCommandOutcome::Refused;

    IrcSession *session = m_sessions.findSession(networkId);
    if (!session || session->state() != IrcSession::State::Registered)
        return IrcCommandOutcome::NotConnected;
    return session->sendChannelMode(*request) ? IrcCommandOutcome::Sent
                                              : IrcCommandOutcome::Refused;
}

IrcCommandOutcome IrcController::dispatchWhois(const IrcCommand& command,
                                               IrcComposerSurface surface)
{
    QString nick = firstToken(command.argument);
    IrcSession *session = nullptr;
    if (nick.isEmpty()) {
        if (selectedIsCloseableDirect()) {
            nick = selectedTarget();
            session = selectedSession();
        } else if (m_selected) {
            return IrcCommandOutcome::WrongScope;
        } else {
            return IrcCommandOutcome::Refused;
        }
    } else {
        const QString networkId = queryNetworkId(surface);
        if (networkId.isEmpty()) {
            if (surface == IrcComposerSurface::Conversation)
                return IrcCommandOutcome::WrongScope;
            return IrcCommandOutcome::Refused;
        }
        session = m_sessions.findSession(networkId);
    }
    if (nick.isEmpty())
        return IrcCommandOutcome::Refused;
    if (!session || session->state() != IrcSession::State::Registered)
        return IrcCommandOutcome::NotConnected;
    return session->whois(nick) ? IrcCommandOutcome::Sent
                                : IrcCommandOutcome::Refused;
}

void IrcController::echoIfPresent(IrcSession *session,
                                  const QString& target,
                                  const QString& body,
                                  QuietWire wire)
{
    const IrcConversationKey key =
        m_reducer.conversationKey(session->networkId(), target);
    if (!m_reducer.find(key) && !(m_selected && *m_selected == key))
        return;
    const QDateTime now = QDateTime::currentDateTimeUtc();
    const QString nick = session->nick();
    if (wire == QuietWire::Notice) {
        apply(IrcNoticeEvent{key, nick, body, now, target});
        return;
    }
    apply(IrcMessageEvent{key, nick, body, now, target});
}

void IrcController::unawayAfterChat(IrcSession *session)
{
    const QString networkId = session->networkId();
    if (m_reducer.selfAway(networkId) && !m_unawaySent.contains(networkId)) {
        if (session->clearAway())
            m_unawaySent.insert(networkId);
    }
}

IrcCommandOutcome IrcController::clearSurface(IrcComposerSurface surface)
{
    if (surface == IrcComposerSurface::Status)
        return m_console.clearLog() ? IrcCommandOutcome::Sent
                                    : IrcCommandOutcome::Refused;
    if (!m_selected)
        return IrcCommandOutcome::WrongScope;
    m_reducer.clearMessages(*m_selected);
    m_messages.reload();
    return IrcCommandOutcome::Sent;
}

bool IrcController::report(IrcCommandOutcome outcome, const IrcCommand& command)
{
    const QString networkId = errorNetworkId(
        m_console.isOpen() ? IrcComposerSurface::Status
                           : IrcComposerSurface::Conversation);
    setLastError(networkId, outcome == IrcCommandOutcome::Sent
        ? QString{}
        : ircCommandOutcomeText(outcome, command));
    emit statusChanged();
    return outcome == IrcCommandOutcome::Sent;
}

void IrcController::echoLocal(IrcMessageKind kind, const QString& body)
{
    if (!m_selected || body.isEmpty())
        return;
    const QDateTime now = QDateTime::currentDateTimeUtc();
    const QString nick = currentNick();
    if (kind == IrcMessageKind::Action) {
        apply(IrcActionEvent{*m_selected, nick, body, now, selectedTarget()});
        return;
    }
    apply(IrcMessageEvent{*m_selected, nick, body, now, selectedTarget()});
}

void IrcController::adoptReducerSelection()
{
    const std::optional<IrcConversationKey> key = m_reducer.selected();
    if (!key)
        return;
    m_selected = *key;
    m_messages.setSelected(*key);
    m_members.setSelected(*key);
}

void IrcController::apply(const IrcEvent& event)
{
    const QString previousId = identityNetworkId();
    const bool previousAway = selfAway();
    const bool typingOnly = std::holds_alternative<IrcTypingEvent>(event);
    const bool selfAwayOnly = std::holds_alternative<IrcSelfAwayEvent>(event);
    if (const auto *welcome = std::get_if<IrcWelcomeEvent>(&event))
        m_unawaySent.remove(welcome->networkId);
    else if (const auto *selfAway = std::get_if<IrcSelfAwayEvent>(&event))
        m_unawaySent.remove(selfAway->networkId);
    m_reducer.apply(event);
    if (selfAwayOnly) {
        notifySelfAwayIfChanged(previousId, previousAway);
        return;
    }
    if (!m_selected && !m_reducer.conversations().empty() && !typingOnly) {
        const IrcConversationState& conversation =
            m_reducer.conversations().begin()->second;
        m_selected = conversation.key;
        m_selectedTarget = conversation.target;
        m_reducer.markSelected(conversation.key);
        m_messages.setSelected(conversation.key);
        m_members.setSelected(conversation.key);
    }
    adoptReducerSelection();
    const bool releasedStale = m_reducer.releaseStaleNamesSync(
        m_selected, QDateTime::currentDateTimeUtc());
    IrcViewNotify notify = classifyViewNotify(event, m_reducer, m_selected);
    if (releasedStale) {
        notify.conversations = true;
        notify.messages = true;
        notify.members = IrcMemberSurface::Reset;
        notify.selection = true;
    }
    publish(notify);
    if (notify.rearmTyping)
        armTypingRefresh();
    notifySelfAwayIfChanged(previousId, previousAway);
}

void IrcController::publish(const IrcViewNotify& notify)
{
    if (notify.conversations)
        m_conversations.reload();
    if (notify.messages)
        m_messages.reload();
    if (notify.members == IrcMemberSurface::Reset)
        m_members.reload();
    else if (notify.members == IrcMemberSurface::Row)
        m_members.touch(notify.nick);
    if (notify.selection)
        emit selectionChanged();
    if (notify.typing)
        emit typingChanged();
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
    if (channelNamesSyncing(m_reducer, m_selected)) {
        m_conversations.reload();
        return;
    }
    publish(IrcViewNotify::resetAll());
}

IrcSession *IrcController::selectedSession() const
{
    return m_selected ? m_sessions.findSession(m_selected->networkId) : nullptr;
}

void IrcController::setLastError(const QString& networkId, const QString& message)
{
    if (message.isEmpty())
        m_lastErrors.remove(networkId);
    else
        m_lastErrors.insert(networkId, message);
}

QString IrcController::errorNetworkId(IrcComposerSurface surface) const
{
    const QString networkId = queryNetworkId(surface);
    return networkId.isEmpty() ? identityNetworkId() : networkId;
}

QString IrcController::identityNetworkId() const
{
    if (m_selected)
        return m_selected->networkId;
    return m_console.networkId();
}

void IrcController::notifySelfAwayIfChanged(const QString& previousId, bool previousAway)
{
    if (identityNetworkId() != previousId || selfAway() != previousAway)
        emit selfAwayChanged();
}

void IrcController::armTypingRefresh()
{
    m_typingRefresh.stop();
    if (!m_selected)
        return;
    const QDateTime now = QDateTime::currentDateTimeUtc();
    if (m_reducer.typingNicks(*m_selected, now).isEmpty())
        return;
    const IrcConversationState *conversation = m_reducer.find(*m_selected);
    if (!conversation)
        return;
    QDateTime soonest;
    for (const auto& entry : conversation->typing) {
        if (!ircIsTyping(entry.second, now))
            continue;
        const QDateTime expires = ircTypingExpiresAt(entry.second);
        if (!soonest.isValid() || expires < soonest)
            soonest = expires;
    }
    if (!soonest.isValid())
        return;
    m_typingRefresh.start(int(qMax(now.msecsTo(soonest), qint64(0))));
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
