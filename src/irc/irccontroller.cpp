#include "irccontroller.h"

#include "ircchannelmode.h"
#include "irccommand.h"
#include "irceventtranslator.h"
#include "irchighlight.h"
#include "ircignore.h"
#include "ircmute.h"
#include "ircopendirect.h"
#include "ircpresence.h"
#include "ircjointarget.h"
#include "ircviewnotify.h"
#include "irctcp.h"
#include "irctyping.h"
#include "ircwiretext.h"

#include <QByteArray>
#include <QDateTime>
#include <QSettings>
#include <QVariantMap>

#include <algorithm>
#include <string>
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

bool ignoreNickIsUsable(const QString& nick, const IrcServerFeatures& features)
{
    if (nick.isEmpty())
        return false;
    if (features.isChannel(utf8(nick)))
        return false;
    if (nick.contains(QLatin1Char('!')) || nick.contains(QLatin1Char('@'))
        || nick.contains(QLatin1Char('*')) || nick.contains(QLatin1Char(','))) {
        return false;
    }
    return true;
}

bool muteTargetIsUsable(const QString& target, const IrcServerFeatures& features)
{
    if (target.isEmpty())
        return false;
    if (features.isChannel(utf8(target)))
        return true;
    return ignoreNickIsUsable(target, features);
}

QString reopenDirectMessagesKey()
{
    return QStringLiteral("reopenDirectMessages");
}

QString loadPeerAvatarsKey()
{
    return QStringLiteral("loadPeerAvatars");
}

bool loadReopenDirectMessages()
{
    QSettings settings;
    settings.beginGroup(QStringLiteral("preferences"));
    return settings.value(reopenDirectMessagesKey(), true).toBool();
}

void saveReopenDirectMessages(bool enabled)
{
    QSettings settings;
    settings.beginGroup(QStringLiteral("preferences"));
    settings.setValue(reopenDirectMessagesKey(), enabled);
    settings.endGroup();
    settings.sync();
}

bool loadLoadPeerAvatars()
{
    QSettings settings;
    settings.beginGroup(QStringLiteral("preferences"));
    // Default on: HTTPS policy still fails closed. Turn off to keep avatar
    // hosts from seeing your IP when you join a busy channel.
    return settings.value(loadPeerAvatarsKey(), true).toBool();
}

void saveLoadPeerAvatars(bool enabled)
{
    QSettings settings;
    settings.beginGroup(QStringLiteral("preferences"));
    settings.setValue(loadPeerAvatarsKey(), enabled);
    settings.endGroup();
    settings.sync();
}

QString stateText(IrcSession::State state)
{
    switch (state) {
    case IrcSession::State::Idle:
        return QStringLiteral("Offline");
    case IrcSession::State::Connecting:
    case IrcSession::State::StsUpgrading:
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
        m_conversations.invalidateTyping();
        armTypingRefresh();
    });
    connect(&m_console, &IrcStatusConsole::openChanged, this, [this] {
        emit selectionChanged();
        emit statusChanged();
    });
    connect(&m_console, &IrcStatusConsole::networkChanged, this, [this] {
        emit selectionChanged();
        emit statusChanged();
    });
    connect(&m_console, &IrcStatusConsole::alertsChanged, this,
            &IrcController::statusChanged);
    m_reducer.setConversationLog(&m_transcripts);
    m_reopenDirectMessages = loadReopenDirectMessages();
    m_loadPeerAvatars = loadLoadPeerAvatars();
}

void IrcController::setTranscriptRoot(const QString &root)
{
    m_transcripts.setRoot(root);
}

IrcSession *IrcController::addSession(const IrcSessionConfig& config,
                                      IrcTransport *transport,
                                      IrcReconnectTimer *reconnectTimer)
{
    IrcSession *session = m_sessions.createSession(config, transport, reconnectTimer);
    if (!session)
        return nullptr;

    m_currentNicks.insert(config.networkId, config.nick);
    hydrateMutes(config.networkId);
    session->setIgnoreFilter(
        [this, networkId = config.networkId](const IrcMessage& message,
                                             const QString& selfNick) {
            return ircIgnoreDropsInbound(
                message, selfNick, m_ignores.nicks(networkId),
                m_reducer.serverFeatures(networkId));
        });
    syncHighlightWords(config.networkId);
    connect(session, &IrcSession::registered, this,
            [this, session](const QString& networkId) {
        m_currentNicks[networkId] = session->nick();
        apply(IrcWelcomeEvent{networkId, session->nick()});
        m_openDirectsMotdSeen.remove(networkId);
        updateStatus(session);
    });
    connect(session, &IrcSession::messageReceived,
            this, &IrcController::handleMessage);
    connect(session, &IrcSession::historyBatchReceived,
            this, &IrcController::handleHistoryBatch);
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
    // Log every status line before WHOIS/CTCP routing so nothing is dropped when
    // routing defers or ignores an entry.
    m_console.observe(session);
    connect(session, &IrcSession::statusEntry,
            this, &IrcController::handleStatusEntry);
    return session;
}

bool IrcController::discardSession(const QString &networkId)
{
    if (!m_sessions.findSession(networkId))
        return false;
    forgetWhoisWatches(networkId);
    forgetCtcpWatches(networkId);
    forgetStatusWatch(networkId);
    const QString previousId = identityNetworkId();
    const bool previousAway = selfAway();
    m_reducer.apply(IrcSelfAwayEvent{networkId, false});
    m_unawaySent.remove(networkId);
    m_openDirectsMotdSeen.remove(networkId);
    m_currentNicks.remove(networkId);
    m_capabilities.remove(networkId);
    m_console.forget(networkId);
    emit capabilitiesChanged();
    const bool discarded = m_sessions.discardSession(networkId);
    notifySelfAwayIfChanged(previousId, previousAway);
    emit statusChanged();
    return discarded;
}

void IrcController::forgetNetworkState(const QString &networkId)
{
    if (networkId.isEmpty())
        return;
    forgetWhoisWatches(networkId);
    forgetCtcpWatches(networkId);
    forgetStatusWatch(networkId);
    const QString previousId = identityNetworkId();
    const bool previousAway = selfAway();
    m_reducer.forgetNetwork(networkId);
    m_unawaySent.remove(networkId);
    m_openDirectsMotdSeen.remove(networkId);
    m_currentNicks.remove(networkId);
    m_capabilities.remove(networkId);
    m_lastErrors.remove(networkId);
    m_ignores.forget(networkId);
    m_mutes.forget(networkId);
    m_openDirects.forget(networkId);
    m_highlights.forget(networkId);
    if (m_selected && m_selected->networkId == networkId)
        clearConversationSelection();
    reloadModels();
    emit capabilitiesChanged();
    emit statusChanged();
    notifySelfAwayIfChanged(previousId, previousAway);
}

void IrcController::setNetworkOrder(const QStringList &networkOrder)
{
    if (m_networkOrder == networkOrder)
        return;
    m_networkOrder = networkOrder;
    m_conversations.setNetworkOrder(networkOrder);
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

QString IrcController::selectedConversationId() const
{
    return m_selected ? ircConversationId(*m_selected) : QString{};
}

QString IrcController::focusedNetworkId() const
{
    if (m_console.isOpen())
        return m_console.networkId();
    if (m_selected)
        return m_selected->networkId;
    return m_console.networkId();
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
    return connectionStatusFor(focusedNetworkId());
}

QString IrcController::lastError() const
{
    return lastErrorForNetwork(identityNetworkId());
}

int IrcController::conversationEpoch() const
{
    return m_conversationEpoch;
}

int IrcController::peerMetadataEpoch() const
{
    return m_peerMetadataEpoch;
}

QString IrcController::lastErrorForNetwork(const QString& networkId) const
{
    return m_lastErrors.value(networkId);
}

QString IrcController::lastErrorFor(const QString& networkId) const
{
    return lastErrorForNetwork(networkId);
}

QString IrcController::connectionStatusFor(const QString& networkId) const
{
    if (IrcSession *session = m_sessions.findSession(networkId))
        return stateText(session->state());
    return networkId.isEmpty() ? m_connectionStatus : QStringLiteral("Offline");
}

int IrcController::unreadCountFor(const QString& networkId) const
{
    if (networkId.isEmpty())
        return 0;
    int total = 0;
    for (const auto& entry : m_reducer.conversations()) {
        if (entry.first.networkId == networkId)
            total += entry.second.unread;
    }
    return total;
}

bool IrcController::mentionFor(const QString& networkId) const
{
    if (networkId.isEmpty())
        return false;
    for (const auto& entry : m_reducer.conversations()) {
        if (entry.first.networkId == networkId && entry.second.mentions > 0)
            return true;
    }
    return false;
}

QString IrcController::currentNick() const
{
    return m_currentNicks.value(focusedNetworkId());
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

bool IrcController::reopenDirectMessages() const
{
    return m_reopenDirectMessages;
}

void IrcController::setReopenDirectMessages(bool enabled)
{
    if (m_reopenDirectMessages == enabled)
        return;
    m_reopenDirectMessages = enabled;
    saveReopenDirectMessages(enabled);
    emit reopenDirectMessagesChanged();
    if (!enabled)
        return;
    for (const QString& networkId : m_sessions.networkIds()) {
        IrcSession *session = m_sessions.findSession(networkId);
        if (session && session->state() == IrcSession::State::Registered)
            restoreOpenDirects(networkId);
    }
}

bool IrcController::loadPeerAvatars() const
{
    return m_loadPeerAvatars;
}

void IrcController::setLoadPeerAvatars(bool enabled)
{
    if (m_loadPeerAvatars == enabled)
        return;
    m_loadPeerAvatars = enabled;
    saveLoadPeerAvatars(enabled);
    emit loadPeerAvatarsChanged();
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

QVariantMap IrcController::peerMetadata(const QString& networkId,
                                        const QString& nick) const
{
    QVariantMap result;
    result.insert(QStringLiteral("avatar"), QString());
    result.insert(QStringLiteral("status"), QString());
    result.insert(QStringLiteral("bot"), false);
    result.insert(QStringLiteral("displayName"), QString());
    result.insert(QStringLiteral("pronouns"), QString());
    result.insert(QStringLiteral("homepage"), QString());
    result.insert(QStringLiteral("color"), QString());
    if (networkId.isEmpty() || nick.isEmpty())
        return result;
    const IrcNickPresence facts = m_reducer.nickPresence(networkId, nick);
    result.insert(QStringLiteral("avatar"), facts.avatar());
    result.insert(QStringLiteral("status"), facts.status());
    result.insert(QStringLiteral("bot"), facts.isBot());
    result.insert(QStringLiteral("displayName"),
                  facts.metadata(IrcMetadata::displayNameKey()));
    result.insert(QStringLiteral("pronouns"),
                  facts.metadata(IrcMetadata::pronounsKey()));
    result.insert(QStringLiteral("homepage"),
                  facts.metadata(IrcMetadata::homepageKey()));
    result.insert(QStringLiteral("color"),
                  facts.metadata(IrcMetadata::colorKey()));
    return result;
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
    const bool metadataDropped = dropped(IrcCapability::MemberMetadata)
        || dropped(IrcCapability::Batch);
    if (awayDropped || metadataDropped) {
        m_reducer.clearPresenceFacts(networkId, awayDropped, metadataDropped);
        if (metadataDropped) {
            ++m_peerMetadataEpoch;
            emit peerMetadataChanged();
        }
        reloadModels();
    }
    if (dropped(IrcCapability::MessageTags)) {
        m_reducer.clearTypingFacts(networkId);
        emit typingChanged();
        m_conversations.invalidateTyping();
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
    m_console.setNetwork(networkId);
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
    notifyFocusedConnectionStatus();
    emit selectionChanged();
    emit capabilitiesChanged();
    notifyComposerText(m_composerDraft);
    emit typingChanged();
    armTypingRefresh();
    notifySelfAwayIfChanged(previousId, previousAway);
}

void IrcController::selectConversationById(const QString& conversationId)
{
    const std::optional<IrcConversationKey> key = ircParseConversationId(conversationId);
    if (!key)
        return;
    const IrcConversationState *conversation = m_reducer.find(*key);
    selectConversation(key->networkId,
                       conversation ? conversation->target : key->normalizedTarget);
}

void IrcController::openStatus(const QString& networkId)
{
    if (networkId.isEmpty())
        return;
    const QString previousId = identityNetworkId();
    const bool previousAway = selfAway();
    m_console.setNetwork(networkId);
    m_console.setOpen(true);
    notifyFocusedConnectionStatus();
    emit selectionChanged();
    notifySelfAwayIfChanged(previousId, previousAway);
}

void IrcController::openDirectMessage(const QString& nick)
{
    if (!m_selected || nick.isEmpty())
        return;
    const IrcConversationKey key =
        m_reducer.conversationKey(m_selected->networkId, nick);
    if (!m_reducer.ensureConversation(key, nick, IrcConversationCause::UserOpen))
        return;
    rememberOpenDirect(m_selected->networkId, nick);
    m_conversations.reload();
    selectConversation(m_selected->networkId, nick);
}

void IrcController::revealConversation(const QString& networkId,
                                       const QString& target)
{
    if (networkId.isEmpty() || target.isEmpty())
        return;
    const IrcConversationKey key = m_reducer.conversationKey(networkId, target);
    if (!m_reducer.find(key)) {
        if (!m_reducer.ensureConversation(key, target, IrcConversationCause::UserOpen))
            return;
        rememberOpenDirect(networkId, target);
        m_conversations.reload();
    }
    selectConversation(networkId, target);
}

void IrcController::openJoinedChannel(const QString& networkId,
                                      const QString& channel)
{
    if (networkId.isEmpty() || channel.isEmpty())
        return;
    const IrcConversationKey key = m_reducer.conversationKey(networkId, channel);
    if (!m_reducer.ensureConversation(key, channel, IrcConversationCause::ChannelState))
        return;
    m_conversations.reload();
    selectConversation(networkId, channel);
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
    const QVector<IrcConversationKey> ordered = ircSidebarOrder(m_reducer, m_networkOrder);
    const std::optional<IrcConversationKey> next =
        ircNeighborAfterDrop(ordered, dropping);
    QString nextNetworkId;
    QString nextTarget;
    if (next) {
        nextNetworkId = next->networkId;
        const IrcConversationState *neighbor = m_reducer.find(*next);
        nextTarget = neighbor ? neighbor->target : next->normalizedTarget;
    }
    applyMute(dropping.networkId, m_selectedTarget, false);
    forgetOpenDirect(dropping.networkId, m_selectedTarget);
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
    notifyFocusedConnectionStatus();
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

    rememberOpenDirect(networkId, target);
    const IrcConversationKey key = m_reducer.conversationKey(networkId, target);
    m_reducer.ensureConversation(key, target, IrcConversationCause::QuietSend);
    noteNickDelivery(networkId, target);
    echoIfPresent(session, target, text, QuietWire::Privmsg);
    unawayAfterChat(session);
    setLastError(networkId, {});
    emit statusChanged();
    return true;
}

namespace {

QString cliKind(IrcMessageKind kind)
{
    switch (kind) {
    case IrcMessageKind::Action:
        return QStringLiteral("action");
    case IrcMessageKind::Notice:
        return QStringLiteral("notice");
    case IrcMessageKind::Message:
        return QStringLiteral("message");
    case IrcMessageKind::Event:
    case IrcMessageKind::Error:
    case IrcMessageKind::Whois:
        return {};
    }
    return {};
}

qint64 lineStamp(const QDateTime &when)
{
    return when.toUTC().toMSecsSinceEpoch();
}

int compareChat(const QDateTime &leftTs,
                const QString &leftMsgid,
                qint64 leftSequence,
                const QDateTime &rightTs,
                const QString &rightMsgid,
                qint64 rightSequence)
{
    const qint64 left = lineStamp(leftTs);
    const qint64 right = lineStamp(rightTs);
    if (left != right)
        return left < right ? -1 : 1;
    const int msgidOrder = QString::compare(leftMsgid, rightMsgid);
    if (msgidOrder != 0)
        return msgidOrder;
    if (leftSequence != rightSequence)
        return leftSequence < rightSequence ? -1 : 1;
    return 0;
}

void appendChatLines(QVector<IrcController::CliMessage> &out,
                     const IrcConversationState &conversation,
                     const IrcEventReducer &reducer)
{
    for (const IrcReducedMessage &message : conversation.messages) {
        const QString kind = cliKind(message.kind);
        if (kind.isEmpty())
            continue;
        IrcController::CliMessage row;
        row.networkId = conversation.key.networkId;
        row.target = conversation.target;
        row.sender = message.author;
        row.timestamp = message.timestamp.isValid()
            ? message.timestamp.toUTC()
            : QDateTime::currentDateTimeUtc();
        row.message = message.body;
        row.kind = kind;
        row.msgid = message.msgid.value;
        row.sequence = message.sequence;
        row.mention = reducer.mentions(conversation.key.networkId, message.body);
        out.append(row);
    }
}

bool sortAndCap(QVector<IrcController::CliMessage> &lines, int cap)
{
    std::sort(lines.begin(), lines.end(),
              [](const IrcController::CliMessage &left,
                 const IrcController::CliMessage &right) {
                  return compareChat(left.timestamp, left.msgid, left.sequence,
                                     right.timestamp, right.msgid,
                                     right.sequence) < 0;
              });
    if (cap >= 0 && lines.size() > cap) {
        lines.erase(lines.begin(), lines.end() - cap);
        return true;
    }
    return false;
}

}

std::variant<IrcController::CliMessageSnapshot, QString>
IrcController::snapshotMessages(const QString &networkId,
                                const QString &target,
                                const CliReadQuery &query) const
{
    QVector<CliMessage> lines;
    if (target.isEmpty()) {
        for (const auto &entry : m_reducer.conversations()) {
            if (entry.first.networkId != networkId)
                continue;
            appendChatLines(lines, entry.second, m_reducer);
        }
    } else {
        const IrcConversationKey key = m_reducer.conversationKey(networkId, target);
        const IrcConversationState *conversation = m_reducer.find(key);
        if (!conversation)
            return QStringLiteral("No conversation for '%1'.").arg(target);
        appendChatLines(lines, *conversation, m_reducer);
    }

    if (query.mode == CliReadQuery::Mode::Since) {
        QVector<CliMessage> kept;
        for (const CliMessage &line : lines) {
            if (lineStamp(line.timestamp) >= lineStamp(query.sinceUtc))
                kept.append(line);
        }
        lines = std::move(kept);
        const bool truncated = sortAndCap(lines, 100);
        return CliMessageSnapshot{std::move(lines), truncated};
    }
    if (query.mode == CliReadQuery::Mode::After) {
        QVector<CliMessage> kept;
        for (const CliMessage &line : lines) {
            if (compareChat(line.timestamp, line.msgid, line.sequence,
                            query.afterUtc, query.afterMsgid,
                            query.afterSequence) > 0)
                kept.append(line);
        }
        lines = std::move(kept);
        const bool truncated = sortAndCap(lines, 100);
        return CliMessageSnapshot{std::move(lines), truncated};
    }

    const int last = query.last < 1 ? 50 : query.last;
    sortAndCap(lines, last);
    return CliMessageSnapshot{std::move(lines)};
}

std::variant<QVector<IrcController::CliMember>, QString>
IrcController::snapshotMembers(const QString &networkId,
                               const QString &target) const
{
    const IrcConversationKey key = m_reducer.conversationKey(networkId, target);
    const IrcConversationState *conversation = m_reducer.find(key);
    if (!conversation)
        return QStringLiteral("No conversation for '%1'.").arg(target);
    const IrcChannelState *channel = conversation->channel();
    if (!channel)
        return QStringLiteral("names is for joined channels.");
    if (!channel->joined)
        return QStringLiteral("Not joined.");

    QVector<CliMember> members;
    const QVector<IrcOrderedMember> ordered = m_reducer.orderedMembers(key);
    members.reserve(ordered.size());
    for (const IrcOrderedMember &member : ordered) {
        const std::optional<IrcMemberView> view =
            m_reducer.memberView(key, member.nick);
        if (!view)
            continue;
        CliMember row;
        row.nick = view->nick;
        row.label = view->label;
        row.away = view->isAway();
        row.status = view->status;
        members.append(row);
    }
    return members;
}

std::variant<QVector<IrcController::CliConversation>, QString>
IrcController::snapshotConversations(const QString &networkId) const
{
    QVector<CliConversation> rows;
    const QVector<IrcConversationKey> keys = ircSidebarOrder(m_reducer);
    for (const IrcConversationKey &key : keys) {
        if (key.networkId != networkId)
            continue;
        const IrcConversationState *conversation = m_reducer.find(key);
        if (!conversation)
            continue;
        CliConversation row;
        row.target = conversation->target;
        row.channel = conversation->isChannel();
        if (const IrcChannelState *channel = conversation->channel())
            row.topic = channel->topic;
        row.unread = conversation->unread;
        row.mention = conversation->mentions > 0;
        rows.append(row);
    }
    return rows;
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
            rememberOpenDirect(session->networkId(), selectedTarget());
            noteNickDelivery(session->networkId(), selectedTarget());
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

    if (command.verb == IrcCommand::Verb::Op
        || command.verb == IrcCommand::Verb::Deop
        || command.verb == IrcCommand::Verb::Voice
        || command.verb == IrcCommand::Verb::Devoice
        || command.verb == IrcCommand::Verb::Ban) {
        return dispatchChannelModeWrapper(command, surface);
    }

    if (command.verb == IrcCommand::Verb::Ns
        || command.verb == IrcCommand::Verb::Cs) {
        return dispatchServiceMsg(command, surface);
    }

    if (command.verb == IrcCommand::Verb::Raw)
        return dispatchRaw(command, surface);

    if (command.verb == IrcCommand::Verb::Whois)
        return dispatchWhois(command, surface);

    if (command.verb == IrcCommand::Verb::Ping
        || command.verb == IrcCommand::Verb::Time
        || command.verb == IrcCommand::Verb::Version) {
        return dispatchCtcp(command, surface);
    }

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

    if (command.verb == IrcCommand::Verb::Ignore
        || command.verb == IrcCommand::Verb::Unignore
        || command.verb == IrcCommand::Verb::Ignored) {
        return dispatchIgnore(command, surface);
    }

    if (command.verb == IrcCommand::Verb::Mute
        || command.verb == IrcCommand::Verb::Unmute
        || command.verb == IrcCommand::Verb::Muted) {
        return dispatchMute(command, surface);
    }

    if (command.verb == IrcCommand::Verb::Highlight
        || command.verb == IrcCommand::Verb::Unhighlight
        || command.verb == IrcCommand::Verb::Highlights) {
        return dispatchHighlight(command, surface);
    }

    if (command.verb == IrcCommand::Verb::Help)
        return dispatchHelp(surface);

    if (command.verb == IrcCommand::Verb::Status)
        return dispatchStatus(command, surface);

    IrcSession *active = sessionFor(surface);
    if (!active) {
        if (surface == IrcComposerSurface::Conversation && !m_selected
                && !m_sessions.networkIds().isEmpty())
            return IrcCommandOutcome::Refused;
        return IrcCommandOutcome::NotConnected;
    }
    if (active->state() != IrcSession::State::Registered
            && command.verb != IrcCommand::Verb::Quit)
        return IrcCommandOutcome::NotConnected;

    bool sent = false;
    switch (command.verb) {
    case IrcCommand::Verb::Join: {
        const IrcServerFeatures& features =
            m_reducer.serverFeatures(active->networkId());
        std::optional<QVector<IrcJoinTarget>> targets;
        if (command.argument.isEmpty()) {
            const std::optional<IrcPendingInvite> pending = active->pendingInvite();
            if (!pending)
                return IrcCommandOutcome::Refused;
            const std::optional<IrcJoinTarget> target =
                IrcJoinTarget::make(pending->channel, std::nullopt, features);
            if (!target)
                return IrcCommandOutcome::Refused;
            targets = QVector<IrcJoinTarget>{*target};
        } else {
            targets = ircParseJoinTargets(command.argument, features);
            if (!targets)
                return IrcCommandOutcome::Refused;
        }
        sent = true;
        for (const IrcJoinTarget& target : *targets)
            sent = active->join(target) && sent;
        if (sent)
            openJoinedChannel(active->networkId(), targets->constLast().channel());
        break;
    }
    case IrcCommand::Verb::Part: {
        QString channel = firstToken(command.argument);
        if (channel.isEmpty()) {
            if (!m_selected || !isChannel())
                return IrcCommandOutcome::WrongScope;
            if (m_selected->networkId != queryNetworkId(surface))
                return IrcCommandOutcome::Refused;
            channel = selectedTarget();
            sent = !channel.isEmpty() && active->part(channel);
            break;
        }
        sent = active->part(channel);
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
        if (!m_selected || !isChannel())
            return IrcCommandOutcome::WrongScope;
        if (m_selected->networkId != networkId)
            return IrcCommandOutcome::Refused;
        const QString channel = selectedTarget();
        sent = !channel.isEmpty()
            && active->kick(channel, first, restAfterFirstToken(command.argument));
        break;
    }
    case IrcCommand::Verb::Invite: {
        const QString nick = firstToken(command.argument);
        if (nick.isEmpty())
            return IrcCommandOutcome::WrongScope;
        const QString networkId = queryNetworkId(surface);
        const IrcServerFeatures& features = m_reducer.serverFeatures(networkId);
        if (features.isChannel(utf8(nick)))
            return IrcCommandOutcome::Refused;
        const QString rest = restAfterFirstToken(command.argument);
        QString channel;
        if (rest.isEmpty()) {
            if (surface == IrcComposerSurface::Status || !m_selected || !isChannel())
                return IrcCommandOutcome::WrongScope;
            if (m_selected->networkId != networkId)
                return IrcCommandOutcome::Refused;
            channel = selectedTarget();
        } else {
            channel = firstToken(rest);
            if (!features.isChannel(utf8(channel)) || !restAfterFirstToken(rest).isEmpty())
                return IrcCommandOutcome::Refused;
        }
        sent = !channel.isEmpty() && active->invite(nick, channel);
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

IrcSession *IrcController::sessionFor(IrcComposerSurface surface) const
{
    return m_sessions.findSession(queryNetworkId(surface));
}

IrcCommandOutcome IrcController::sendSelectedMessage(const QString& body)
{
    IrcSession *session = selectedSession();
    if (!session || !m_selected)
        return IrcCommandOutcome::WrongScope;
    const bool sent = session->sendPrivmsg(selectedTarget(), body);
    if (sent) {
        rememberOpenDirect(session->networkId(), selectedTarget());
        noteNickDelivery(session->networkId(), selectedTarget());
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

IrcCommandOutcome IrcController::dispatchIgnore(const IrcCommand& command,
                                                IrcComposerSurface surface)
{
    const QString networkId = queryNetworkId(surface);
    if (networkId.isEmpty()) {
        if (surface == IrcComposerSurface::Conversation && !m_selected)
            return IrcCommandOutcome::WrongScope;
        return IrcCommandOutcome::Refused;
    }
    IrcSession *session = sessionFor(surface);
    if (!session || session->state() == IrcSession::State::Idle
        || session->state() == IrcSession::State::Failed)
        return IrcCommandOutcome::NotConnected;

    const IrcServerFeatures& features = m_reducer.serverFeatures(networkId);
    const IrcCaseMapping& mapping = features.caseMapping();
    QString text;
    if (command.verb == IrcCommand::Verb::Ignored) {
        if (!command.argument.isEmpty())
            return IrcCommandOutcome::Refused;
        const QStringList nicks = m_ignores.listed(networkId, mapping);
        text = nicks.isEmpty()
            ? QStringLiteral("Not ignoring anyone")
            : QStringLiteral("Ignoring: %1").arg(nicks.join(QStringLiteral(", ")));
    } else {
        const QString nick = firstToken(command.argument);
        if (!restAfterFirstToken(command.argument).isEmpty()
            || !ignoreNickIsUsable(nick, features)) {
            return IrcCommandOutcome::Refused;
        }
        if (command.verb == IrcCommand::Verb::Ignore) {
            const bool added = m_ignores.add(networkId, nick, mapping);
            text = added ? QStringLiteral("Ignoring %1").arg(nick)
                         : QStringLiteral("Already ignoring %1").arg(nick);
        } else {
            const bool removed = m_ignores.remove(networkId, nick, mapping);
            text = removed ? QStringLiteral("No longer ignoring %1").arg(nick)
                           : QStringLiteral("Not ignoring %1").arg(nick);
        }
    }
    m_console.record(IrcStatusEntry::outcome(networkId, text));
    return IrcCommandOutcome::Sent;
}

void IrcController::hydrateMutes(const QString& networkId)
{
    for (const QString& target : m_mutes.targets(networkId))
        m_reducer.setMuted(m_reducer.conversationKey(networkId, target), true);
}

bool IrcController::persistableDirectTarget(const QString& networkId,
                                            const QString& target) const
{
    if (networkId.isEmpty() || target.isEmpty())
        return false;
    const IrcServerFeatures& features = m_reducer.serverFeatures(networkId);
    if (features.isChannel(utf8(target)))
        return false;
    return !ircTargetLooksLikeService(target, features);
}

void IrcController::rememberOpenDirect(const QString& networkId, const QString& target)
{
    if (!persistableDirectTarget(networkId, target))
        return;
    const IrcConversationState *conversation =
        m_reducer.find(m_reducer.conversationKey(networkId, target));
    if (conversation && conversation->isChannel())
        return;
    const QString stored =
        (conversation && !conversation->target.isEmpty()) ? conversation->target
                                                          : target;
    m_openDirects.add(networkId, stored,
                      m_reducer.serverFeatures(networkId).caseMapping());
}

void IrcController::forgetOpenDirect(const QString& networkId, const QString& target)
{
    if (networkId.isEmpty() || target.isEmpty())
        return;
    m_openDirects.remove(networkId, target,
                         m_reducer.serverFeatures(networkId).caseMapping());
}

void IrcController::noteOpenDirectsMotd(const QString& networkId)
{
    if (networkId.isEmpty() || m_openDirectsMotdSeen.contains(networkId))
        return;
    m_openDirectsMotdSeen.insert(networkId);
    restoreOpenDirects(networkId);
}

void IrcController::restoreOpenDirects(const QString& networkId)
{
    if (!m_reopenDirectMessages || networkId.isEmpty())
        return;
    bool created = false;
    const bool prune = m_openDirectsMotdSeen.contains(networkId);
    const IrcCaseMapping& mapping =
        m_reducer.serverFeatures(networkId).caseMapping();
    for (const QString& target : m_openDirects.listed(networkId, mapping)) {
        if (!persistableDirectTarget(networkId, target)) {
            if (prune)
                m_openDirects.remove(networkId, target, mapping);
            continue;
        }
        const IrcConversationKey key = m_reducer.conversationKey(networkId, target);
        if (m_reducer.find(key))
            continue;
        if (m_reducer.ensureConversation(key, target, IrcConversationCause::Restore))
            created = true;
        else if (prune)
            m_openDirects.remove(networkId, target, mapping);
    }
    if (!created)
        return;
    m_conversations.reload();
    ++m_conversationEpoch;
    emit conversationStateChanged();
}

bool IrcController::applyMute(const QString& networkId,
                              const QString& target,
                              bool muted)
{
    if (networkId.isEmpty() || target.isEmpty())
        return false;
    const IrcCaseMapping& mapping =
        m_reducer.serverFeatures(networkId).caseMapping();
    const bool changed = muted
        ? m_mutes.add(networkId, target, mapping)
        : m_mutes.remove(networkId, target, mapping);
    m_reducer.setMuted(m_reducer.conversationKey(networkId, target), muted);
    m_conversations.reload();
    ++m_conversationEpoch;
    emit conversationStateChanged();
    return changed;
}

IrcCommandOutcome IrcController::dispatchMute(const IrcCommand& command,
                                              IrcComposerSurface surface)
{
    const QString networkId = queryNetworkId(surface);
    if (networkId.isEmpty()) {
        if (surface == IrcComposerSurface::Conversation && !m_selected)
            return IrcCommandOutcome::WrongScope;
        return IrcCommandOutcome::Refused;
    }
    IrcSession *session = sessionFor(surface);
    if (!session || session->state() == IrcSession::State::Idle
        || session->state() == IrcSession::State::Failed)
        return IrcCommandOutcome::NotConnected;

    const IrcServerFeatures& features = m_reducer.serverFeatures(networkId);
    const IrcCaseMapping& mapping = features.caseMapping();
    QString text;
    if (command.verb == IrcCommand::Verb::Muted) {
        if (!command.argument.isEmpty())
            return IrcCommandOutcome::Refused;
        const QStringList targets = m_mutes.listed(networkId, mapping);
        text = targets.isEmpty()
            ? QStringLiteral("Not muting anything")
            : QStringLiteral("Muted: %1").arg(targets.join(QStringLiteral(", ")));
    } else {
        QString target = firstToken(command.argument);
        if (!restAfterFirstToken(command.argument).isEmpty())
            return IrcCommandOutcome::Refused;
        if (target.isEmpty()) {
            if (surface == IrcComposerSurface::Status || !m_selected)
                return IrcCommandOutcome::WrongScope;
            target = selectedTarget();
        }
        if (!muteTargetIsUsable(target, features))
            return IrcCommandOutcome::Refused;
        if (command.verb == IrcCommand::Verb::Mute) {
            const bool added = applyMute(networkId, target, true);
            text = added ? QStringLiteral("Muted %1").arg(target)
                         : QStringLiteral("Already muted %1").arg(target);
        } else {
            const bool removed = applyMute(networkId, target, false);
            text = removed ? QStringLiteral("No longer muted %1").arg(target)
                           : QStringLiteral("Not muted %1").arg(target);
        }
    }
    m_console.record(IrcStatusEntry::outcome(networkId, text));
    return IrcCommandOutcome::Sent;
}

IrcCommandOutcome IrcController::dispatchHighlight(const IrcCommand& command,
                                                   IrcComposerSurface surface)
{
    const QString networkId = queryNetworkId(surface);
    if (networkId.isEmpty()) {
        if (surface == IrcComposerSurface::Conversation && !m_selected)
            return IrcCommandOutcome::WrongScope;
        return IrcCommandOutcome::Refused;
    }
    IrcSession *session = sessionFor(surface);
    if (!session || session->state() == IrcSession::State::Idle
        || session->state() == IrcSession::State::Failed)
        return IrcCommandOutcome::NotConnected;

    const IrcCaseMapping& mapping =
        m_reducer.serverFeatures(networkId).caseMapping();
    QString text;
    if (command.verb == IrcCommand::Verb::Highlights) {
        if (!command.argument.isEmpty())
            return IrcCommandOutcome::Refused;
        const QStringList words = m_highlights.listed(networkId, mapping);
        text = words.isEmpty()
            ? QStringLiteral("No highlight words")
            : QStringLiteral("Highlights: %1").arg(words.join(QStringLiteral(", ")));
    } else {
        const QString word = firstToken(command.argument);
        if (word.isEmpty() || !restAfterFirstToken(command.argument).isEmpty())
            return IrcCommandOutcome::Refused;
        if (command.verb == IrcCommand::Verb::Highlight) {
            const bool added = m_highlights.add(networkId, word, mapping);
            text = added ? QStringLiteral("Highlighting %1").arg(word)
                         : QStringLiteral("Already highlighting %1").arg(word);
        } else {
            const bool removed = m_highlights.remove(networkId, word, mapping);
            text = removed ? QStringLiteral("No longer highlighting %1").arg(word)
                           : QStringLiteral("Not highlighting %1").arg(word);
        }
        syncHighlightWords(networkId);
    }
    m_console.record(IrcStatusEntry::outcome(networkId, text));
    return IrcCommandOutcome::Sent;
}

void IrcController::syncHighlightWords(const QString& networkId)
{
    m_reducer.setHighlightWords(networkId, m_highlights.words(networkId));
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
    if (!m_reducer.ensureConversation(key, nick, IrcConversationCause::UserOpen))
        return IrcCommandOutcome::Refused;
    rememberOpenDirect(networkId, nick);
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

    noteNickDelivery(networkId, target);
    const IrcConversationKey key = m_reducer.conversationKey(networkId, target);
    m_reducer.ensureConversation(key, target, IrcConversationCause::QuietSend);
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

IrcCommandOutcome IrcController::dispatchChannelModeWrapper(
    const IrcCommand& command, IrcComposerSurface surface)
{
    if (!m_selected || !isChannel())
        return IrcCommandOutcome::WrongScope;
    if (m_selected->networkId != queryNetworkId(surface))
        return IrcCommandOutcome::Refused;

    const QString token = firstToken(command.argument);
    if (token.isEmpty() || !restAfterFirstToken(command.argument).isEmpty())
        return IrcCommandOutcome::Refused;

    QString modes;
    switch (command.verb) {
    case IrcCommand::Verb::Op:
        modes = QStringLiteral("+o");
        break;
    case IrcCommand::Verb::Deop:
        modes = QStringLiteral("-o");
        break;
    case IrcCommand::Verb::Voice:
        modes = QStringLiteral("+v");
        break;
    case IrcCommand::Verb::Devoice:
        modes = QStringLiteral("-v");
        break;
    case IrcCommand::Verb::Ban:
        modes = QStringLiteral("+b");
        break;
    default:
        return IrcCommandOutcome::Unsupported;
    }

    QString parameter = token;
    if (command.verb == IrcCommand::Verb::Ban
        && !token.contains(QLatin1Char('!'))
        && !token.contains(QLatin1Char('@'))) {
        parameter = token + QStringLiteral("!*@*");
    }

    IrcCommand mode;
    mode.verb = IrcCommand::Verb::Mode;
    mode.argument = QStringLiteral("%1 %2 %3")
                        .arg(selectedTarget(), modes, parameter);
    return dispatchMode(mode, surface);
}

IrcCommandOutcome IrcController::dispatchServiceMsg(const IrcCommand& command,
                                                    IrcComposerSurface surface)
{
    const QString nick = command.verb == IrcCommand::Verb::Ns
        ? QStringLiteral("NickServ")
        : QStringLiteral("ChanServ");
    IrcCommand msg = command;
    msg.verb = IrcCommand::Verb::Msg;
    msg.argument = command.argument.isEmpty()
        ? nick
        : nick + QLatin1Char(' ') + command.argument;
    return dispatchQuietSend(msg, surface);
}

IrcCommandOutcome IrcController::dispatchRaw(const IrcCommand& command,
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

    IrcSession *session = m_sessions.findSession(networkId);
    if (!session || session->state() != IrcSession::State::Registered)
        return IrcCommandOutcome::NotConnected;
    return session->sendRaw(command.argument) ? IrcCommandOutcome::Sent
                                              : IrcCommandOutcome::Refused;
}

IrcCommandOutcome IrcController::dispatchHelp(IrcComposerSurface surface)
{
    QString networkId = queryNetworkId(surface);
    if (networkId.isEmpty())
        networkId = m_console.networkId();
    if (networkId.isEmpty())
        return IrcCommandOutcome::Refused;

    QStringList names;
    for (const IrcVerbSpec& row : IrcVerbTable::all())
        names.append(QLatin1Char('/') + row.name);
    const QString text =
        QStringLiteral("Commands: %1. Empty /join joins the latest invite.")
            .arg(names.join(QStringLiteral(", ")));
    if (surface == IrcComposerSurface::Conversation) {
        if (!m_selected)
            return IrcCommandOutcome::WrongScope;
        apply(IrcWhoisTranscriptEvent{*m_selected, text});
        return IrcCommandOutcome::Sent;
    }
    m_console.record(IrcStatusEntry::outcome(networkId, text));
    return IrcCommandOutcome::Sent;
}

IrcCommandOutcome IrcController::dispatchStatus(const IrcCommand& command,
                                                IrcComposerSurface surface)
{
    IrcSession *session = sessionFor(surface);
    if (!session) {
        if (surface == IrcComposerSurface::Conversation && !m_selected
                && !m_sessions.networkIds().isEmpty())
            return IrcCommandOutcome::Refused;
        return IrcCommandOutcome::NotConnected;
    }
    if (session->state() != IrcSession::State::Registered)
        return IrcCommandOutcome::NotConnected;

    const QString networkId = session->networkId();
    const auto echo = [&](const QString& text) -> IrcCommandOutcome {
        if (surface == IrcComposerSurface::Conversation) {
            if (!m_selected)
                return IrcCommandOutcome::WrongScope;
            apply(IrcWhoisTranscriptEvent{*m_selected, text});
            return IrcCommandOutcome::Sent;
        }
        m_console.record(IrcStatusEntry::outcome(networkId, text));
        return IrcCommandOutcome::Sent;
    };

    const IrcCapabilitySet capabilities = m_capabilities.value(networkId);
    if (!capabilities.contains(IrcCapability::MemberMetadata)
            || !capabilities.contains(IrcCapability::Batch)) {
        return echo(QStringLiteral("This network does not support standing status."));
    }

    if (command.argument.isEmpty()) {
        const QString current =
            peerMetadata(networkId, session->nick())
                .value(QStringLiteral("status"))
                .toString();
        if (current.isEmpty()) {
            return echo(QStringLiteral(
                "No standing status. Use /status <text> or /status clear."));
        }
        return echo(QStringLiteral("Standing status: %1").arg(current));
    }

    const QString token = firstToken(command.argument);
    if (token.compare(QStringLiteral("clear"), Qt::CaseInsensitive) == 0
            && restAfterFirstToken(command.argument).isEmpty()) {
        if (!session->clearOwnMetadata(IrcMetadata::statusKey()))
            return IrcCommandOutcome::Refused;
        armStatusWatch(networkId, surface, IrcStatusWatch::Kind::Clear, QString{});
        return IrcCommandOutcome::Sent;
    }

    const int valueBudget = IrcMetadata::effectiveMaxValueBytes(
        session->metadataCapability().maxValueBytes);
    if (valueBudget <= 0) {
        return echo(QStringLiteral(
            "This network does not allow standing status text."));
    }
    const QString clamped = IrcMetadata::clamped(command.argument, valueBudget);
    if (clamped.isEmpty()
            || !session->setOwnMetadata(IrcMetadata::statusKey(), clamped)) {
        return IrcCommandOutcome::Refused;
    }
    armStatusWatch(networkId, surface, IrcStatusWatch::Kind::Set, clamped);
    return IrcCommandOutcome::Sent;
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
    IrcWhoisDestination destination{IrcWhoisStatusOnly{}};
    if (surface == IrcComposerSurface::Conversation) {
        if (!m_selected)
            return IrcCommandOutcome::WrongScope;
        destination = *m_selected;
    }
    return sendWhois(*session, nick, std::move(destination))
        ? IrcCommandOutcome::Sent
        : IrcCommandOutcome::Refused;
}

QString IrcController::ctcpQueryName(IrcCommand::Verb verb) const
{
    switch (verb) {
    case IrcCommand::Verb::Ping:
        return QStringLiteral("PING");
    case IrcCommand::Verb::Time:
        return QStringLiteral("TIME");
    case IrcCommand::Verb::Version:
        return QStringLiteral("VERSION");
    case IrcCommand::Verb::Empty:
    case IrcCommand::Verb::Say:
    case IrcCommand::Verb::Action:
    case IrcCommand::Verb::Join:
    case IrcCommand::Verb::Part:
    case IrcCommand::Verb::Nick:
    case IrcCommand::Verb::Quit:
    case IrcCommand::Verb::Clear:
    case IrcCommand::Verb::Close:
    case IrcCommand::Verb::Query:
    case IrcCommand::Verb::Msg:
    case IrcCommand::Verb::Topic:
    case IrcCommand::Verb::Notice:
    case IrcCommand::Verb::Away:
    case IrcCommand::Verb::Back:
    case IrcCommand::Verb::Status:
    case IrcCommand::Verb::Whois:
    case IrcCommand::Verb::Mode:
    case IrcCommand::Verb::Kick:
    case IrcCommand::Verb::Invite:
    case IrcCommand::Verb::Ignore:
    case IrcCommand::Verb::Unignore:
    case IrcCommand::Verb::Ignored:
    case IrcCommand::Verb::Mute:
    case IrcCommand::Verb::Unmute:
    case IrcCommand::Verb::Muted:
    case IrcCommand::Verb::Highlight:
    case IrcCommand::Verb::Unhighlight:
    case IrcCommand::Verb::Highlights:
    case IrcCommand::Verb::Op:
    case IrcCommand::Verb::Deop:
    case IrcCommand::Verb::Voice:
    case IrcCommand::Verb::Devoice:
    case IrcCommand::Verb::Ban:
    case IrcCommand::Verb::Ns:
    case IrcCommand::Verb::Cs:
    case IrcCommand::Verb::Raw:
    case IrcCommand::Verb::Help:
    case IrcCommand::Verb::Unknown:
        return {};
    }
    return {};
}

IrcCommandOutcome IrcController::dispatchCtcp(const IrcCommand& command,
                                              IrcComposerSurface surface)
{
    const QString query = ctcpQueryName(command.verb);
    QString nick = firstToken(command.argument);
    if (!restAfterFirstToken(command.argument).isEmpty())
        return IrcCommandOutcome::Refused;

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
        if (m_reducer.serverFeatures(networkId).isChannel(utf8(nick)))
            return IrcCommandOutcome::Refused;
        session = m_sessions.findSession(networkId);
    }
    if (nick.isEmpty())
        return IrcCommandOutcome::Refused;
    if (!session || session->state() != IrcSession::State::Registered)
        return IrcCommandOutcome::NotConnected;

    IrcCtcpDestination destination{IrcWhoisStatusOnly{}};
    if (surface == IrcComposerSurface::Conversation) {
        if (!m_selected)
            return IrcCommandOutcome::WrongScope;
        destination = *m_selected;
    }

    QString argument;
    if (command.verb == IrcCommand::Verb::Ping)
        argument = QString::number(QDateTime::currentMSecsSinceEpoch());
    return sendCtcpQuery(*session, nick, query, argument, std::move(destination))
        ? IrcCommandOutcome::Sent
        : IrcCommandOutcome::Refused;
}

std::optional<IrcController::IrcWhoisWatchKey>
IrcController::whoisWatchKey(const QString& networkId, const QString& nick) const
{
    const QString trimmed = nick.trimmed();
    if (networkId.isEmpty() || trimmed.isEmpty())
        return std::nullopt;
    return IrcWhoisWatchKey{
        networkId,
        m_reducer.conversationKey(networkId, trimmed).normalizedTarget,
    };
}

bool IrcController::sendWhois(IrcSession& session,
                              const QString& nick,
                              IrcWhoisDestination destination)
{
    const std::optional<IrcWhoisWatchKey> key =
        whoisWatchKey(session.networkId(), nick);
    if (!key)
        return false;
    if (const auto *conversation = std::get_if<IrcConversationKey>(&destination)) {
        if (conversation->networkId != session.networkId())
            return false;
    }

    m_whoisWatches.insert_or_assign(*key, IrcWhoisWatch{std::move(destination)});
    if (!session.whois(nick)) {
        m_whoisWatches.erase(*key);
        return false;
    }
    return true;
}

void IrcController::noteNickDelivery(const QString& networkId, const QString& target)
{
    if (m_reducer.serverFeatures(networkId).isChannel(utf8(target)))
        return;
    const std::optional<IrcWhoisWatchKey> key = whoisWatchKey(networkId, target);
    if (!key)
        return;
    auto found = m_whoisWatches.find(*key);
    if (found == m_whoisWatches.end())
        return;
    found->second.failedIsAmbiguous = true;
}

void IrcController::handleStatusEntry(const IrcStatusEntry& entry)
{
    routeStatusMetadataError(entry);
    if (const IrcWhoisLine *line = entry.whoisLine())
        routeWhoisLine(entry.networkId(), *line);
    if (const IrcCtcpReplyLine *line = entry.ctcpReply())
        routeCtcpReply(entry.networkId(), *line, entry.text());
}

void IrcController::routeWhoisLine(const QString& networkId, const IrcWhoisLine& line)
{
    const std::optional<IrcWhoisWatchKey> key = whoisWatchKey(networkId, line.nick());
    if (!key)
        return;
    auto found = m_whoisWatches.find(*key);
    if (found == m_whoisWatches.end())
        return;

    if (line.progress() == IrcWhoisLine::Progress::Failed
        && found->second.failedIsAmbiguous)
        return;

    const IrcWhoisDestination destination = found->second.destination;
    const IrcConversationKey *conversation =
        std::get_if<IrcConversationKey>(&destination);
    if (conversation) {
        apply(IrcWhoisTranscriptEvent{
            *conversation,
            line.text(),
        });
    }

    if (line.progress() == IrcWhoisLine::Progress::Detail
        && !found->second.metadataEmitted) {
        found->second.metadataEmitted = true;
        for (const QString& text : whoisMetadataLines(networkId, line.nick())) {
            m_console.record(IrcStatusEntry::lifecycle(
                networkId, IrcLogSeverity::Info, QStringLiteral("whois"), text));
            if (conversation) {
                apply(IrcWhoisTranscriptEvent{
                    *conversation,
                    text,
                });
            }
        }
    }

    if (line.terminal())
        m_whoisWatches.erase(found);
}

QStringList IrcController::whoisMetadataLines(const QString& networkId,
                                              const QString& nick) const
{
    const IrcNickPresence facts = m_reducer.nickPresence(networkId, nick);
    QStringList lines;
    const auto addValue = [&](const QString& key, const QString& pattern) {
        const QString value = facts.metadata(key);
        if (value.isEmpty())
            return;
        lines.append(pattern.arg(nick, value));
    };
    addValue(IrcMetadata::displayNameKey(),
             QStringLiteral("%1 is also known as %2"));
    addValue(IrcMetadata::pronounsKey(), QStringLiteral("%1 pronouns %2"));
    addValue(IrcMetadata::statusKey(), QStringLiteral("%1 status %2"));
    if (facts.isBot()) {
        const QString software = facts.metadata(IrcMetadata::botKey());
        if (software.isEmpty())
            lines.append(QStringLiteral("%1 is a bot").arg(nick));
        else
            lines.append(QStringLiteral("%1 is a bot (%2)").arg(nick, software));
    }
    addValue(IrcMetadata::homepageKey(), QStringLiteral("%1 homepage %2"));
    addValue(IrcMetadata::colorKey(), QStringLiteral("%1 color %2"));
    addValue(IrcMetadata::avatarKey(), QStringLiteral("%1 avatar %2"));
    return lines;
}

void IrcController::forgetWhoisWatches(const QString& networkId)
{
    if (networkId.isEmpty())
        return;
    for (auto it = m_whoisWatches.begin(); it != m_whoisWatches.end(); ) {
        if (it->first.networkId == networkId)
            it = m_whoisWatches.erase(it);
        else
            ++it;
    }
}

std::optional<IrcController::IrcCtcpWatchKey>
IrcController::ctcpWatchKey(const QString& networkId,
                            const QString& nick,
                            const QString& command) const
{
    const QString trimmed = nick.trimmed();
    const QString verb = command.trimmed().toUpper();
    if (networkId.isEmpty() || trimmed.isEmpty() || verb.isEmpty())
        return std::nullopt;
    return IrcCtcpWatchKey{
        networkId,
        m_reducer.conversationKey(networkId, trimmed).normalizedTarget,
        verb,
    };
}

bool IrcController::sendCtcpQuery(IrcSession& session,
                                  const QString& nick,
                                  const QString& command,
                                  const QString& argument,
                                  IrcCtcpDestination destination)
{
    const std::optional<IrcCtcpWatchKey> key =
        ctcpWatchKey(session.networkId(), nick, command);
    if (!key)
        return false;
    if (const auto *conversation = std::get_if<IrcConversationKey>(&destination)) {
        if (conversation->networkId != session.networkId())
            return false;
    }

    m_ctcpWatches.insert_or_assign(*key, IrcCtcpWatch{std::move(destination)});
    if (!session.sendCtcp(nick, command, argument)) {
        m_ctcpWatches.erase(*key);
        return false;
    }
    return true;
}

void IrcController::routeCtcpReply(const QString& networkId,
                                   const IrcCtcpReplyLine& line,
                                   const QString& text)
{
    const std::optional<IrcCtcpWatchKey> key =
        ctcpWatchKey(networkId, line.nick(), line.command());
    if (!key)
        return;
    auto found = m_ctcpWatches.find(*key);
    if (found == m_ctcpWatches.end())
        return;

    const IrcCtcpDestination destination = found->second.destination;
    m_ctcpWatches.erase(found);

    if (std::holds_alternative<IrcWhoisStatusOnly>(destination) || text.isEmpty())
        return;
    apply(IrcWhoisTranscriptEvent{
        std::get<IrcConversationKey>(destination),
        text,
    });
}

void IrcController::forgetCtcpWatches(const QString& networkId)
{
    if (networkId.isEmpty())
        return;
    for (auto it = m_ctcpWatches.begin(); it != m_ctcpWatches.end(); ) {
        if (it->first.networkId == networkId)
            it = m_ctcpWatches.erase(it);
        else
            ++it;
    }
}

void IrcController::armStatusWatch(const QString& networkId,
                                   IrcComposerSurface surface,
                                   IrcStatusWatch::Kind kind,
                                   const QString& value)
{
    IrcWhoisDestination destination{IrcWhoisStatusOnly{}};
    if (surface == IrcComposerSurface::Conversation && m_selected)
        destination = *m_selected;
    m_statusWatches.insert(networkId, IrcStatusWatch{std::move(destination), kind, value});
}

void IrcController::forgetStatusWatch(const QString& networkId)
{
    if (networkId.isEmpty())
        return;
    m_statusWatches.remove(networkId);
}

void IrcController::echoStatusOutcome(const QString& networkId,
                                      const IrcWhoisDestination& destination,
                                      const QString& text)
{
    if (const auto *conversation = std::get_if<IrcConversationKey>(&destination)) {
        apply(IrcWhoisTranscriptEvent{*conversation, text});
        return;
    }
    m_console.record(IrcStatusEntry::outcome(networkId, text));
}

void IrcController::routeStatusMetadataReply(const QString& networkId,
                                             const QString& nick,
                                             const QString& key,
                                             const QString& value)
{
    auto found = m_statusWatches.find(networkId);
    if (found == m_statusWatches.end())
        return;
    if (key.compare(IrcMetadata::statusKey(), Qt::CaseInsensitive) != 0)
        return;
    const QString self = m_currentNicks.value(networkId);
    if (self.isEmpty()
        || !m_reducer.serverFeatures(networkId)
                .caseMapping()
                .equals(utf8(nick), utf8(self))) {
        return;
    }
    if (found->kind == IrcStatusWatch::Kind::Clear) {
        if (!value.isEmpty())
            return;
        echoStatusOutcome(networkId, found->destination,
                          QStringLiteral("Standing status cleared."));
    } else {
        if (value.isEmpty())
            return;
        echoStatusOutcome(networkId, found->destination,
                          QStringLiteral("Standing status set to %1.").arg(value));
    }
    m_statusWatches.erase(found);
}

void IrcController::routeStatusMetadataError(const IrcStatusEntry& entry)
{
    const QString networkId = entry.networkId();
    auto found = m_statusWatches.find(networkId);
    if (found == m_statusWatches.end())
        return;
    const QString label = entry.label();
    if (label != QStringLiteral("764") && label != QStringLiteral("767")
        && label != QStringLiteral("769")) {
        return;
    }
    const QString keyToken = firstToken(entry.text());
    if (!keyToken.isEmpty()
        && keyToken.compare(IrcMetadata::statusKey(), Qt::CaseInsensitive) != 0) {
        return;
    }
    const QString reason = entry.text().isEmpty() ? label : entry.text();
    const QString message = found->kind == IrcStatusWatch::Kind::Clear
        ? QStringLiteral("Could not clear standing status: %1").arg(reason)
        : QStringLiteral("Could not set standing status: %1").arg(reason);
    echoStatusOutcome(networkId, found->destination, message);
    m_statusWatches.erase(found);
}

void IrcController::routeStatusMetadataFail(const QString& networkId,
                                            const IrcMessage& message)
{
    if (parameter(message, 0).compare(QLatin1String("METADATA"), Qt::CaseInsensitive)
        != 0) {
        return;
    }
    auto found = m_statusWatches.find(networkId);
    if (found == m_statusWatches.end())
        return;

    const QString code = parameter(message, 1).toUpper();
    const bool knownFail =
        code == QLatin1String("KEY_NO_PERMISSION")
        || code == QLatin1String("VALUE_INVALID")
        || code == QLatin1String("RATE_LIMITED")
        || code == QLatin1String("KEY_NOT_SET")
        || code == QLatin1String("LIMIT_REACHED")
        || code == QLatin1String("KEY_INVALID");
    if (!knownFail)
        return;

    bool sawStatusKey = false;
    bool sawOtherKnownKey = false;
    // Skip trailing description so prose cannot invent a key.
    const std::size_t lastContext =
        message.parameters.size() > 2 ? message.parameters.size() - 1 : 2;
    for (std::size_t index = 2; index < lastContext; ++index) {
        const QString token = parameter(message, index);
        if (!IrcMetadata::isKnownKey(token))
            continue;
        if (token.compare(IrcMetadata::statusKey(), Qt::CaseInsensitive) == 0)
            sawStatusKey = true;
        else
            sawOtherKnownKey = true;
    }
    // SUB warnings for other keys must not consume a pending /status watch.
    if (sawOtherKnownKey && !sawStatusKey)
        return;

    const QString description = message.parameters.empty()
        ? code
        : parameter(message, message.parameters.size() - 1);
    const QString reason = description.isEmpty()
        ? code
        : QStringLiteral("%1 %2").arg(code, description);
    const QString outcome = found->kind == IrcStatusWatch::Kind::Clear
        ? QStringLiteral("Could not clear standing status: %1").arg(reason)
        : QStringLiteral("Could not set standing status: %1").arg(reason);
    echoStatusOutcome(networkId, found->destination, outcome);
    m_statusWatches.erase(found);
}

void IrcController::echoIfPresent(IrcSession *session,
                                  const QString& target,
                                  const QString& body,
                                  QuietWire wire)
{
    if (wire == QuietWire::Privmsg
        && session->capabilities().contains(IrcCapability::EchoMessage)) {
        return;
    }
    const IrcConversationKey key =
        m_reducer.conversationKey(session->networkId(), target);
    if (!m_reducer.find(key))
        return;
    const QDateTime now = QDateTime::currentDateTimeUtc();
    const QString nick = session->nick();
    if (wire == QuietWire::Notice) {
        apply(IrcNoticeEvent{key, nick, body, now, target, {}});
        return;
    }
    apply(IrcMessageEvent{key, nick, body, now, target, {}});
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
    if (m_capabilities.value(m_selected->networkId)
            .contains(IrcCapability::EchoMessage)) {
        return;
    }
    const QDateTime now = QDateTime::currentDateTimeUtc();
    const QString nick = currentNick();
    if (kind == IrcMessageKind::Action) {
        apply(IrcActionEvent{*m_selected, nick, body, now, selectedTarget(), {}});
        return;
    }
    apply(IrcMessageEvent{*m_selected, nick, body, now, selectedTarget(), {}});
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
    if (const auto *welcome = std::get_if<IrcWelcomeEvent>(&event)) {
        m_unawaySent.remove(welcome->networkId);
        forgetWhoisWatches(welcome->networkId);
        forgetCtcpWatches(welcome->networkId);
        forgetStatusWatch(welcome->networkId);
    } else if (const auto *selfAway = std::get_if<IrcSelfAwayEvent>(&event)) {
        m_unawaySent.remove(selfAway->networkId);
    }
    m_reducer.apply(event);
    if (const auto *metadata = std::get_if<IrcMemberMetadataEvent>(&event)) {
        ++m_peerMetadataEpoch;
        emit peerMetadataChanged();
        routeStatusMetadataReply(metadata->networkId,
                                 metadata->nick,
                                 metadata->key,
                                 metadata->value);
    }
    if (const auto *nick = std::get_if<IrcNickEvent>(&event)) {
        m_openDirects.rekey(nick->networkId, nick->oldNick, nick->newNick,
                            m_reducer.serverFeatures(nick->networkId).caseMapping());
    } else if (const auto *message = std::get_if<IrcMessageEvent>(&event)) {
        if (m_reducer.serverFeatures(message->conversation.networkId)
                .caseMapping()
                .equals(utf8(message->author),
                        utf8(m_currentNicks.value(message->conversation.networkId)))) {
            rememberOpenDirect(message->conversation.networkId, message->target);
        }
    } else if (const auto *notice = std::get_if<IrcNoticeEvent>(&event)) {
        if (m_reducer.serverFeatures(notice->conversation.networkId)
                .caseMapping()
                .equals(utf8(notice->author),
                        utf8(m_currentNicks.value(notice->conversation.networkId)))) {
            rememberOpenDirect(notice->conversation.networkId, notice->target);
        }
    } else if (const auto *action = std::get_if<IrcActionEvent>(&event)) {
        if (m_reducer.serverFeatures(action->conversation.networkId)
                .caseMapping()
                .equals(utf8(action->author),
                        utf8(m_currentNicks.value(action->conversation.networkId)))) {
            rememberOpenDirect(action->conversation.networkId, action->target);
        }
    }
    if (selfAwayOnly) {
        publish(classifyViewNotify(event, m_reducer, m_selected));
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
    if (std::optional<IrcMentionArrival> mention = m_reducer.takeMentionArrival()) {
        emit mentionArrived(mention->author, mention->body, mention->networkId,
                            mention->target, mention->msgid.value);
    }
}

void IrcController::publish(const IrcViewNotify& notify)
{
    if (notify.conversations) {
        m_conversations.reload();
        ++m_conversationEpoch;
        emit conversationStateChanged();
    }
    if (notify.messages)
        m_messages.reload();
    if (notify.members == IrcMemberSurface::Reset)
        m_members.reload();
    else if (notify.members == IrcMemberSurface::Row)
        m_members.touch(notify.nick);
    if (notify.selection)
        emit selectionChanged();
    if (notify.typing) {
        emit typingChanged();
        if (!notify.conversations)
            m_conversations.invalidateTyping();
    }
}

void IrcController::handleMessage(const QString& networkId,
                                  const IrcMessage& message)
{
    if (message.command == "FAIL")
        routeStatusMetadataFail(networkId, message);
    if (message.command == "005") {
        IrcServerFeatures features = m_reducer.serverFeatures(networkId);
        if (message.parameters.size() > 2) {
            std::vector<std::string> tokens(
                message.parameters.begin() + 1, message.parameters.end() - 1);
            features.applyTokens(tokens);
            m_reducer.setServerFeatures(networkId, features);
        }
        return;
    }
    if (message.command == "376" || message.command == "422")
        noteOpenDirectsMotd(networkId);
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
        if (const auto *join = std::get_if<IrcJoinEvent>(&event)) {
            if (IrcSession *session = m_sessions.findSession(join->networkId)) {
                if (const auto pending = session->pendingInvite()) {
                    const auto& mapping = features.caseMapping();
                    if (mapping.equals(utf8(join->nick), utf8(currentNick))
                        && mapping.equals(utf8(join->channel),
                                          utf8(pending->channel))) {
                        selectConversation(join->networkId, join->channel);
                    }
                }
            }
        }
    }
}

void IrcController::handleHistoryBatch(const QString& networkId,
                                       const IrcHistoryBatch& batch)
{
    const auto event = IrcEventTranslator::translateHistory(
        networkId, m_currentNicks.value(networkId),
        m_reducer.serverFeatures(networkId), batch);
    if (event)
        apply(*event);
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
    return focusedNetworkId();
}

void IrcController::notifySelfAwayIfChanged(const QString& previousId, bool previousAway)
{
    if (identityNetworkId() != previousId || selfAway() != previousAway)
        emit selfAwayChanged();
}

void IrcController::armTypingRefresh()
{
    m_typingRefresh.stop();
    const QDateTime now = QDateTime::currentDateTimeUtc();
    QDateTime soonest;
    for (const auto& conversation : m_reducer.conversations()) {
        for (const auto& entry : conversation.second.typing) {
            if (!ircTypingShowsIndicator(entry.second, now))
                continue;
            const QDateTime expires = ircTypingExpiresAt(entry.second);
            if (!soonest.isValid() || expires < soonest)
                soonest = expires;
        }
    }
    if (!soonest.isValid())
        return;
    m_typingRefresh.start(int(qMax(now.msecsTo(soonest), qint64(0))));
}

void IrcController::updateStatus(IrcSession *session)
{
    if (!session)
        return;
    m_connectionStatus = stateText(session->state());
    emit statusChanged();
}

void IrcController::notifyFocusedConnectionStatus()
{
    if (IrcSession *session = m_sessions.findSession(focusedNetworkId())) {
        updateStatus(session);
        return;
    }
    emit statusChanged();
}
