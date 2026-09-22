#include "irccontroller.h"

#include "ircautoaway.h"
#include "ircpref.h"
#include "ircavatarurl.h"
#include "ircchannelmode.h"
#include "irccommand.h"
#include "irceventtranslator.h"
#include "irchighlight.h"
#include "ircignore.h"
#include "ircmonitor.h"
#include "ircmute.h"
#include "ircnetworkprofile.h"
#include "ircopendirect.h"
#include "ircpresence.h"
#include "ircjointarget.h"
#include "ircviewnotify.h"
#include "irctcp.h"
#include "irctyping.h"
#include "ircwiretext.h"

#include <QByteArray>
#include <QCoreApplication>
#include <QDateTime>
#include <QEvent>
#ifdef QT_GUI_LIB
#include <QGuiApplication>
#endif
#include <QSettings>
#include <QTimer>
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

bool isOwnMetadataClearAlias(const QString& argument)
{
    return firstToken(argument).compare(QStringLiteral("clear"), Qt::CaseInsensitive) == 0
        && restAfterFirstToken(argument).isEmpty();
}

QString ownMetadataClearedMessage(const QString& metadataKey)
{
    if (metadataKey.compare(IrcMetadata::statusKey(), Qt::CaseInsensitive) == 0)
        return QStringLiteral("Standing status cleared.");
    if (metadataKey.compare(IrcMetadata::avatarKey(), Qt::CaseInsensitive) == 0)
        return QStringLiteral("Avatar cleared.");
    return {};
}

QString ownMetadataSetMessage(const QString& metadataKey, const QString& value)
{
    if (metadataKey.compare(IrcMetadata::statusKey(), Qt::CaseInsensitive) == 0)
        return QStringLiteral("Standing status set to %1.").arg(value);
    if (metadataKey.compare(IrcMetadata::avatarKey(), Qt::CaseInsensitive) == 0)
        return QStringLiteral("Avatar set to %1.").arg(value);
    return {};
}

QString ownMetadataClearFailMessage(const QString& metadataKey, const QString& reason)
{
    if (metadataKey.compare(IrcMetadata::statusKey(), Qt::CaseInsensitive) == 0)
        return QStringLiteral("Could not clear standing status: %1").arg(reason);
    if (metadataKey.compare(IrcMetadata::avatarKey(), Qt::CaseInsensitive) == 0)
        return QStringLiteral("Could not clear avatar: %1").arg(reason);
    return {};
}

QString ownMetadataSetFailMessage(const QString& metadataKey, const QString& reason)
{
    if (metadataKey.compare(IrcMetadata::statusKey(), Qt::CaseInsensitive) == 0)
        return QStringLiteral("Could not set standing status: %1").arg(reason);
    if (metadataKey.compare(IrcMetadata::avatarKey(), Qt::CaseInsensitive) == 0)
        return QStringLiteral("Could not set avatar: %1").arg(reason);
    return {};
}

QString ownMetadataNoCapMessage(const QString& metadataKey)
{
    if (metadataKey.compare(IrcMetadata::statusKey(), Qt::CaseInsensitive) == 0)
        return QStringLiteral("This network does not support standing status.");
    if (metadataKey.compare(IrcMetadata::avatarKey(), Qt::CaseInsensitive) == 0)
        return QStringLiteral("This network does not support avatars.");
    return {};
}

QString ownMetadataNoValueMessage(const QString& metadataKey)
{
    if (metadataKey.compare(IrcMetadata::statusKey(), Qt::CaseInsensitive) == 0)
        return QStringLiteral("This network does not allow standing status text.");
    if (metadataKey.compare(IrcMetadata::avatarKey(), Qt::CaseInsensitive) == 0)
        return QStringLiteral("This network does not allow avatar URLs.");
    return {};
}

QString ownMetadataInspectEmptyMessage(const QString& metadataKey)
{
    if (metadataKey.compare(IrcMetadata::statusKey(), Qt::CaseInsensitive) == 0) {
        return QStringLiteral(
            "No standing status. Use /status <text> or /status clear.");
    }
    if (metadataKey.compare(IrcMetadata::avatarKey(), Qt::CaseInsensitive) == 0) {
        return QStringLiteral(
            "No standing avatar. Use /avatar <url|email> or /avatar clear.");
    }
    return {};
}

QString ownMetadataInspectValueMessage(const QString& metadataKey,
                                       const QString& value)
{
    if (metadataKey.compare(IrcMetadata::statusKey(), Qt::CaseInsensitive) == 0)
        return QStringLiteral("Standing status: %1").arg(value);
    if (metadataKey.compare(IrcMetadata::avatarKey(), Qt::CaseInsensitive) == 0)
        return QStringLiteral("Standing avatar: %1").arg(value);
    return {};
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

QString foldedNick(const IrcCaseMapping& mapping, const QString& nick)
{
    const std::string folded = mapping.normalize(utf8(nick));
    return QString::fromUtf8(folded.data(), qsizetype(folded.size()));
}

QString monitorTargetNick(const QString& target)
{
    const int bang = target.indexOf(QLatin1Char('!'));
    const QString nick = bang < 0 ? target : target.left(bang);
    return nick.trimmed();
}

QStringList monitorTargetNicks(const IrcMessage& message)
{
    if (message.parameters.size() < 2)
        return {};
    const QString trailing = parameter(message, message.parameters.size() - 1);
    QStringList nicks;
    for (const QString& target :
         trailing.split(QLatin1Char(','), Qt::SkipEmptyParts)) {
        const QString nick = monitorTargetNick(target);
        if (!nick.isEmpty())
            nicks.append(nick);
    }
    return nicks;
}

QString monitorListFullText(const QString& limit, const QString& targets)
{
    QString text = QStringLiteral("Monitor list is full");
    if (!limit.isEmpty())
        text += QStringLiteral(" (%1)").arg(limit);
    if (!targets.isEmpty()) {
        QString shown = targets;
        shown.replace(QLatin1Char(','), QStringLiteral(", "));
        text += QStringLiteral(": %1").arg(shown);
    }
    return text + QLatin1Char('.');
}

QString reopenDirectMessagesKey()
{
    return QStringLiteral("reopenDirectMessages");
}

QString loadPeerAvatarsKey()
{
    return QStringLiteral("loadPeerAvatars");
}

QString openConversationsAtUnreadKey()
{
    return QStringLiteral("openConversationsAtUnread");
}

QString autoawayEnabledKey()
{
    return QStringLiteral("autoawayEnabled");
}

QString autoawayTimeoutSecondsKey()
{
    return QStringLiteral("autoawayTimeoutSeconds");
}

QString autoawayDefaultReasonKey()
{
    return QStringLiteral("autoawayDefaultReason");
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

bool loadOpenConversationsAtUnread()
{
    QSettings settings;
    settings.beginGroup(QStringLiteral("preferences"));
    return settings.value(openConversationsAtUnreadKey(), true).toBool();
}

void saveOpenConversationsAtUnread(bool enabled)
{
    QSettings settings;
    settings.beginGroup(QStringLiteral("preferences"));
    settings.setValue(openConversationsAtUnreadKey(), enabled);
    settings.endGroup();
    settings.sync();
}

IrcAutoawayConfig loadAutoaway()
{
    QSettings settings;
    settings.beginGroup(QStringLiteral("preferences"));
    IrcAutoawayConfig config;
    config.enabled = settings.value(autoawayEnabledKey(), false).toBool();
    config.timeoutSeconds =
        settings.value(autoawayTimeoutSecondsKey(), 0).toInt();
    if (config.timeoutSeconds < ircAutoawayMinTimeoutSeconds)
        config.timeoutSeconds = 0;
    if (config.timeoutSeconds > ircAutoawayMaxTimeoutSeconds)
        config.timeoutSeconds = ircAutoawayMaxTimeoutSeconds;
    if (config.timeoutSeconds < ircAutoawayMinTimeoutSeconds)
        config.enabled = false;
    config.defaultReason = settings.value(autoawayDefaultReasonKey()).toString();
    return config;
}

void saveAutoawayConfig(const IrcAutoawayConfig& config)
{
    QSettings settings;
    settings.beginGroup(QStringLiteral("preferences"));
    settings.setValue(autoawayEnabledKey(), config.enabled);
    settings.setValue(autoawayTimeoutSecondsKey(), config.timeoutSeconds);
    settings.setValue(autoawayDefaultReasonKey(), config.defaultReason);
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

bool copiesLabeledStandardReply(const IrcStatusEntry& entry)
{
    const QString label = entry.label();
    if (label == QLatin1String("FAIL")
        || label == QLatin1String("WARN")
        || label == QLatin1String("NOTE")) {
        return true;
    }
    if (label.size() != 3)
        return false;
    if (!label.at(0).isDigit() || !label.at(1).isDigit() || !label.at(2).isDigit())
        return false;
    return label.at(0) == QLatin1Char('4') || label.at(0) == QLatin1Char('5');
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
    m_openConversationsAtUnread = loadOpenConversationsAtUnread();
    m_autoaway = loadAutoaway();
    m_autoawayIdle.setSingleShot(true);
    m_autoawayGrace.setSingleShot(true);
    connect(&m_autoawayIdle, &QTimer::timeout, this, &IrcController::onAutoawayIdle);
    connect(&m_autoawayGrace, &QTimer::timeout, this, &IrcController::onAutoawayGrace);
    if (QCoreApplication *app = QCoreApplication::instance())
        app->installEventFilter(this);
    armAutoawayIdle();
}

IrcController::~IrcController()
{
    if (QCoreApplication *app = QCoreApplication::instance())
        app->removeEventFilter(this);
}

void IrcController::setTranscriptRoot(const QString &root)
{
    m_transcripts.setRoot(root);
}

IrcSession *IrcController::addSession(const IrcSessionConfig& config,
                                      IrcTransport *transport,
                                      IrcReconnectTimer *reconnectTimer,
                                      IrcReconnectTimer *labelTimer)
{
    IrcSession *session = m_sessions.createSession(config, transport, reconnectTimer,
                                                   labelTimer);
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
        m_reducer.setServerFeatures(networkId, IrcServerFeatures{});
        forgetMonitorState(networkId);
        emit serverFeaturesChanged();
        apply(IrcWelcomeEvent{networkId, session->nick()});
        if (m_autoawayTripped && m_autoaway.enabled)
            markSessionAutoAway(session);
        m_openDirectsMotdSeen.remove(networkId);
        applyProfileAvatarOnConnect(session);
        updateStatus(session);
    });
    connect(session, &IrcSession::messageReceived,
            this, &IrcController::handleMessage);
    connect(session, &IrcSession::historyBatchReceived,
            this, &IrcController::handleHistoryBatch);
    connect(session, &IrcSession::capabilitiesChanged,
            this, &IrcController::handleCapabilities);
    connect(session, &IrcSession::stateChanged, this,
            [this, session](IrcSession::State state) {
        if (state != IrcSession::State::Registered) {
            forgetChannelList(session->networkId());
            m_appliedProfileAvatars.remove(session->networkId());
        }
        updateStatus(session);
    });
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
    connect(session, &IrcSession::requestLabelFinished,
            this, &IrcController::onRequestLabelFinished);
    return session;
}

bool IrcController::discardSession(const QString &networkId)
{
    if (!m_sessions.findSession(networkId))
        return false;
    forgetWhoisWatches(networkId);
    forgetCtcpWatches(networkId);
    forgetOwnMetadataWatches(networkId);
    forgetChannelList(networkId);
    const QString previousId = identityNetworkId();
    const bool previousAway = selfAway();
    m_reducer.apply(IrcSelfAwayEvent{networkId, false});
    m_unawaySent.remove(networkId);
    m_autoAwayNetworks.remove(networkId);
    m_manualAwayNetworks.remove(networkId);
    m_openDirectsMotdSeen.remove(networkId);
    forgetMonitorState(networkId);
    m_currentNicks.remove(networkId);
    m_capabilities.remove(networkId);
    m_appliedProfileAvatars.remove(networkId);
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
    forgetOwnMetadataWatches(networkId);
    forgetChannelList(networkId);
    const QString previousId = identityNetworkId();
    const bool previousAway = selfAway();
    if (IrcSession *session = m_sessions.findSession(networkId)) {
        if (session->state() == IrcSession::State::Registered
            && m_reducer.serverFeatures(networkId).monitorAdvertised()) {
            session->sendMonitor(QLatin1Char('C'));
        }
    }
    m_reducer.forgetNetwork(networkId);
    for (auto it = m_cancelledPendingJoins.begin();
         it != m_cancelledPendingJoins.end(); ) {
        if (it->networkId == networkId)
            it = m_cancelledPendingJoins.erase(it);
        else
            ++it;
    }
    m_unawaySent.remove(networkId);
    m_autoAwayNetworks.remove(networkId);
    m_manualAwayNetworks.remove(networkId);
    m_openDirectsMotdSeen.remove(networkId);
    forgetMonitorState(networkId);
    m_currentNicks.remove(networkId);
    m_capabilities.remove(networkId);
    m_appliedProfileAvatars.remove(networkId);
    m_lastErrors.remove(networkId);
    m_ignores.forget(networkId);
    m_monitors.forget(networkId);
    m_mutes.forget(networkId);
    m_openDirects.forget(networkId);
    m_highlights.forget(networkId);
    m_inbox.purgeNetwork(networkId);
    syncInbox();
    if (m_selected && m_selected->networkId == networkId)
        clearConversationSelection();
    reloadModels();
    emit capabilitiesChanged();
    emit serverFeaturesChanged();
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

QAbstractItemModel *IrcController::channelList()
{
    return &m_channelList;
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

bool IrcController::openConversationsAtUnread() const
{
    return m_openConversationsAtUnread;
}

void IrcController::setOpenConversationsAtUnread(bool enabled)
{
    if (m_openConversationsAtUnread == enabled)
        return;
    m_openConversationsAtUnread = enabled;
    saveOpenConversationsAtUnread(enabled);
    emit openConversationsAtUnreadChanged();
}

void IrcController::setWindowActive(bool active)
{
    m_reducer.setWindowActive(active);
    if (!active)
        return;
    // Focus returned: consume the unread that piled up in the open
    // conversation while the window was unfocused. The "New messages" mark
    // survives so the transcript still shows where the backlog starts.
    // Status being open does not clear m_selected, so this still consumes
    // the last conversation; that matches the pre-unfocused-mark badge.
    bool changed = false;
    if (m_selected)
        changed = m_reducer.markRead(*m_selected);
    if (!changed)
        return;
    m_conversations.reload();
    ++m_conversationEpoch;
    emit conversationStateChanged();
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

void IrcController::setChannelListPresented(bool presented)
{
    m_channelListPresented = presented;
}

void IrcController::setChannelListIdleTimeoutMs(int milliseconds)
{
    m_channelListIdleTimeoutMs = std::max(1, milliseconds);
}

bool IrcController::joinListedChannel(const QString& channel)
{
    const QString networkId = m_channelList.networkId();
    if (networkId.isEmpty())
        return false;
    IrcSession *session = m_sessions.findSession(networkId);
    if (!session || session->state() != IrcSession::State::Registered)
        return false;
    const std::optional<IrcJoinTarget> target =
        IrcJoinTarget::make(channel, std::nullopt,
                            m_reducer.serverFeatures(networkId));
    if (!target)
        return false;
    const IrcConversationKey key =
        m_reducer.conversationKey(networkId, target->channel());
    if (m_reducer.find(key)) {
        selectConversation(networkId, target->channel());
        return true;
    }
    const bool wrote = session->join(*target);
    if (wrote) {
        m_cancelledPendingJoins.erase(key);
        openJoinedChannel(networkId, target->channel());
    }
    return wrote;
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
            m_appliedProfileAvatars.remove(networkId);
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

    if (IrcSession *session = m_sessions.findSession(networkId)) {
        if (session->state() == IrcSession::State::Registered)
            applyProfileAvatarOnConnect(session);
    }
}

IrcStatusConsole *IrcController::console()
{
    return &m_console;
}

QAbstractItemModel *IrcController::inbox()
{
    return &m_inboxModel;
}

int IrcController::inboxCount() const
{
    return m_inbox.count();
}

void IrcController::appendInbox(IrcInboxItem item)
{
    m_inbox.append(std::move(item),
                    m_reducer.serverFeatures(item.networkId).caseMapping());
    syncInbox();
}

void IrcController::syncInbox()
{
    m_inboxModel.sync(m_inbox);
    emit inboxChanged();
}

void IrcController::activateInboxItem(int row)
{
    if (row < 0 || row >= m_inbox.count())
        return;

    const IrcInboxItem item = m_inbox.at(row);
    m_inbox.consumeAt(row);
    syncInbox();

    switch (item.kind) {
    case IrcInboxKind::Mention:
    case IrcInboxKind::Highlight:
    case IrcInboxKind::Direct:
        revealConversation(item.networkId, item.target);
        break;
    case IrcInboxKind::Invite: {
        IrcSession *session = m_sessions.findSession(item.networkId);
        if (!session)
            break;
        const IrcServerFeatures& features = m_reducer.serverFeatures(item.networkId);
        const std::optional<IrcJoinTarget> target =
            IrcJoinTarget::make(item.target, std::nullopt, features);
        if (!target)
            break;
        if (session->join(*target))
            openJoinedChannel(item.networkId, item.target);
        break;
    }
    case IrcInboxKind::MonitorOnline:
        revealConversation(item.networkId, item.actor);
        break;
    case IrcInboxKind::Kick:
        revealConversation(item.networkId, item.target);
        break;
    }
}

const IrcServerFeatures &IrcController::serverFeatures(const QString &networkId) const
{
    return m_reducer.serverFeatures(networkId);
}

QString IrcController::networkIconUrl(const QString &networkId) const
{
    return QString::fromStdString(
        std::string(m_reducer.serverFeatures(networkId).iconUrl()));
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
    const IrcServerFeatures& features = m_reducer.serverFeatures(networkId);
    m_inbox.consumeConversation(networkId, target, features.caseMapping());
    if (!features.isChannel(utf8(key.normalizedTarget)))
        m_inbox.consumeMonitor(networkId, target, features.caseMapping());
    syncInbox();
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
    const IrcServerFeatures& features = m_reducer.serverFeatures(networkId);
    if (!features.isChannel(utf8(key.normalizedTarget))) {
        m_inbox.consumeMonitor(networkId, target, features.caseMapping());
        syncInbox();
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

void IrcController::dropConversationAndReselect(const IrcConversationKey& key,
                                               bool forgetDirect)
{
    const IrcConversationState *conversation = m_reducer.find(key);
    const bool channel = conversation && conversation->isChannel();
    const bool wasSelected = m_selected && *m_selected == key;
    const QString displayTarget = conversation && !conversation->target.isEmpty()
        ? conversation->target
        : (wasSelected ? m_selectedTarget : key.normalizedTarget);
    QString nextNetworkId;
    QString nextTarget;
    if (wasSelected) {
        const QVector<IrcConversationKey> ordered =
            ircSidebarOrder(m_reducer, m_networkOrder);
        const std::optional<IrcConversationKey> next =
            ircNeighborAfterDrop(ordered, key);
        if (next) {
            nextNetworkId = next->networkId;
            const IrcConversationState *neighbor = m_reducer.find(*next);
            nextTarget = neighbor ? neighbor->target : next->normalizedTarget;
        }
    }
    applyMute(key.networkId, displayTarget, false);
    if (forgetDirect)
        forgetOpenDirect(key.networkId, displayTarget);
    if (channel)
        m_reducer.dropChannel(key);
    else
        m_reducer.dropDirectMessage(key);
    reloadModels();
    if (!wasSelected)
        return;
    if (!nextTarget.isEmpty())
        selectConversation(nextNetworkId, nextTarget);
    else
        clearConversationSelection();
}

void IrcController::dropSelectedDirectAndReselect()
{
    if (!m_selected)
        return;
    dropConversationAndReselect(*m_selected, true);
}

bool IrcController::dismissChannel(const QString& networkId, const QString& channel)
{
    if (networkId.isEmpty() || channel.isEmpty())
        return false;
    const IrcConversationKey key = m_reducer.conversationKey(networkId, channel);
    const IrcConversationState *conversation = m_reducer.find(key);
    if (!conversation || !conversation->isChannel())
        return false;
    dropConversationAndReselect(key, false);
    return true;
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
    noteLocalActivity();
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
            noteLocalActivity();
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
        || command.verb == IrcCommand::Verb::Cs
        || command.verb == IrcCommand::Verb::Znc) {
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

    if (command.verb == IrcCommand::Verb::Monitor
        || command.verb == IrcCommand::Verb::Unmonitor
        || command.verb == IrcCommand::Verb::Monitored) {
        return dispatchMonitor(command, surface);
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

    if (command.verb == IrcCommand::Verb::Autoaway)
        return dispatchAutoaway(command, surface);

    if (command.verb == IrcCommand::Verb::Pref)
        return dispatchPref(command, surface);

    if (command.verb == IrcCommand::Verb::List)
        return dispatchList(command, surface);

    if (command.verb == IrcCommand::Verb::Status)
        return dispatchStatus(command, surface);

    if (command.verb == IrcCommand::Verb::Avatar)
        return dispatchAvatar(command, surface);

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
        for (const IrcJoinTarget& target : *targets) {
            const bool wrote = active->join(target);
            if (wrote) {
                m_cancelledPendingJoins.erase(
                    m_reducer.conversationKey(active->networkId(),
                                              target.channel()));
            }
            sent = wrote && sent;
        }
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
            if (channel.isEmpty())
                return IrcCommandOutcome::Refused;
        }
        const IrcConversationKey key =
            m_reducer.conversationKey(active->networkId(), channel);
        const IrcConversationState *conversation = m_reducer.find(key);
        const bool joined = conversation
            && conversation->channel()
            && conversation->channel()->joined;
        if (dismissChannel(active->networkId(), channel)) {
            if (joined)
                active->part(channel);
            else
                m_cancelledPendingJoins.insert(key);
            sent = true;
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
        if (sent) {
            if (command.argument.trimmed().isEmpty())
                noteAwayCleared(active->networkId());
            else
                noteManualAway(active->networkId());
        }
        break;
    case IrcCommand::Verb::Back:
        sent = active->clearAway();
        if (sent)
            noteAwayCleared(active->networkId());
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
        noteLocalActivity();
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

QString IrcController::monitorDisplayNick(const QString& networkId,
                                          const QString& nick) const
{
    const IrcCaseMapping& mapping =
        m_reducer.serverFeatures(networkId).caseMapping();
    for (const QString& stored : m_monitors.nicks(networkId)) {
        if (mapping.equals(utf8(stored), utf8(nick)))
            return stored;
    }
    return nick;
}

bool IrcController::monitorNotifyMuted(const QString& networkId,
                                       const QString& nick) const
{
    const IrcCaseMapping& mapping =
        m_reducer.serverFeatures(networkId).caseMapping();
    if (m_mutes.contains(networkId, nick, mapping))
        return true;
    const IrcConversationState *conversation =
        m_reducer.find(m_reducer.conversationKey(networkId, nick));
    return conversation && conversation->muted;
}

void IrcController::forgetMonitorState(const QString& networkId)
{
    m_monitorSubscribed.remove(networkId);
    m_monitorPresence.remove(networkId);
}

void IrcController::subscribeMonitors(const QString& networkId)
{
    if (m_monitorSubscribed.contains(networkId))
        return;
    const IrcServerFeatures& features = m_reducer.serverFeatures(networkId);
    if (!features.monitorAdvertised())
        return;
    IrcSession *session = m_sessions.findSession(networkId);
    if (!session || session->state() != IrcSession::State::Registered)
        return;

    m_monitorSubscribed.insert(networkId);
    m_monitorPresence.remove(networkId);

    QStringList nicks = m_monitors.listed(networkId, features.caseMapping());
    if (const std::optional<std::size_t> limit = features.monitorLimit()) {
        if (nicks.size() > int(*limit))
            nicks = nicks.mid(0, int(*limit));
    }
    if (nicks.isEmpty())
        return;
    session->sendMonitor(QLatin1Char('+'), nicks);
}

void IrcController::handleMonitorPresence(const QString& networkId,
                                          const IrcMessage& message,
                                          bool online)
{
    const IrcServerFeatures& features = m_reducer.serverFeatures(networkId);
    const IrcCaseMapping& mapping = features.caseMapping();
    QHash<QString, MonitorPresence>& states = m_monitorPresence[networkId];
    for (const QString& nick : monitorTargetNicks(message)) {
        if (!m_monitors.contains(networkId, nick, mapping))
            continue;
        const QString key = foldedNick(mapping, nick);
        const MonitorPresence previous =
            states.value(key, MonitorPresence::Unknown);
        const MonitorPresence next =
            online ? MonitorPresence::Online : MonitorPresence::Offline;
        states.insert(key, next);
        if (previous == MonitorPresence::Unknown || previous == next)
            continue;
        const QString display = monitorDisplayNick(networkId, nick);
        const QString body = online ? QStringLiteral("is online")
                                    : QStringLiteral("is offline");
        m_console.record(IrcStatusEntry::outcome(
            networkId, QStringLiteral("%1 %2").arg(display, body)));
        if (monitorNotifyMuted(networkId, nick))
            continue;
        emit monitorArrived(display, body, networkId, display);
        if (online && previous != MonitorPresence::Online) {
            appendInbox({
                IrcInboxKind::MonitorOnline,
                QDateTime::currentDateTimeUtc(),
                networkId,
                display,
                display,
                body,
                IrcMsgId{},
            });
        }
    }
}

void IrcController::handleMonitorListFull(const QString& networkId,
                                          const IrcMessage& message)
{
    const QString limit = parameter(message, 1);
    const QString targets = parameter(message, 2);
    const IrcCaseMapping& mapping =
        m_reducer.serverFeatures(networkId).caseMapping();
    for (const QString& nick :
         targets.split(QLatin1Char(','), Qt::SkipEmptyParts)) {
        const QString trimmed = monitorTargetNick(nick);
        if (trimmed.isEmpty())
            continue;
        m_monitors.remove(networkId, trimmed, mapping);
        m_monitorPresence[networkId].remove(foldedNick(mapping, trimmed));
    }
    m_console.record(IrcStatusEntry::outcome(
        networkId, monitorListFullText(limit, targets)));
}

IrcCommandOutcome IrcController::dispatchMonitor(const IrcCommand& command,
                                                 IrcComposerSurface surface)
{
    const QString networkId = queryNetworkId(surface);
    if (networkId.isEmpty()) {
        if (surface == IrcComposerSurface::Conversation && !m_selected)
            return IrcCommandOutcome::WrongScope;
        return IrcCommandOutcome::Refused;
    }
    IrcSession *session = sessionFor(surface);
    if (!session || session->state() != IrcSession::State::Registered)
        return IrcCommandOutcome::NotConnected;

    const IrcServerFeatures& features = m_reducer.serverFeatures(networkId);
    const IrcCaseMapping& mapping = features.caseMapping();
    const bool advertised = features.monitorAdvertised();

    QString text;
    if (command.verb == IrcCommand::Verb::Monitored) {
        if (!command.argument.isEmpty())
            return IrcCommandOutcome::Refused;
        const QStringList nicks = m_monitors.listed(networkId, mapping);
        if (nicks.isEmpty()) {
            text = QStringLiteral("Not watching anyone");
        } else {
            QStringList parts;
            const QHash<QString, MonitorPresence> states =
                m_monitorPresence.value(networkId);
            for (const QString& nick : nicks) {
                const MonitorPresence presence =
                    states.value(foldedNick(mapping, nick),
                                 MonitorPresence::Unknown);
                const char *state = "unknown";
                if (presence == MonitorPresence::Online)
                    state = "online";
                else if (presence == MonitorPresence::Offline)
                    state = "offline";
                parts.append(QStringLiteral("%1 (%2)").arg(
                    nick, QLatin1String(state)));
            }
            text = QStringLiteral("Watching: %1").arg(
                parts.join(QStringLiteral(", ")));
        }
        m_console.record(IrcStatusEntry::outcome(networkId, text));
        return IrcCommandOutcome::Sent;
    }

    const QString nick = firstToken(command.argument);
    if (!restAfterFirstToken(command.argument).isEmpty()
        || !ignoreNickIsUsable(nick, features)) {
        return IrcCommandOutcome::Refused;
    }

    if (!advertised) {
        m_console.record(IrcStatusEntry::outcome(
            networkId,
            QStringLiteral("This network does not support MONITOR.")));
        return IrcCommandOutcome::Sent;
    }

    if (command.verb == IrcCommand::Verb::Monitor) {
        if (m_monitors.contains(networkId, nick, mapping)) {
            text = QStringLiteral("Already watching %1").arg(nick);
        } else if (const std::optional<std::size_t> limit = features.monitorLimit();
                   limit
                   && m_monitors.listed(networkId, mapping).size()
                       >= int(*limit)) {
            m_console.record(IrcStatusEntry::outcome(
                networkId, monitorListFullText(QString::number(qulonglong(*limit)),
                                               nick)));
            return IrcCommandOutcome::Sent;
        } else if (m_monitors.add(networkId, nick, mapping)) {
            if (!session->sendMonitor(QLatin1Char('+'), {nick})) {
                m_monitors.remove(networkId, nick, mapping);
                return IrcCommandOutcome::Refused;
            }
            text = QStringLiteral("Watching %1").arg(nick);
        } else {
            text = QStringLiteral("Already watching %1").arg(nick);
        }
    } else {
        const bool removed = m_monitors.remove(networkId, nick, mapping);
        if (removed) {
            m_monitorPresence[networkId].remove(foldedNick(mapping, nick));
            session->sendMonitor(QLatin1Char('-'), {nick});
            text = QStringLiteral("No longer watching %1").arg(nick);
        } else {
            text = QStringLiteral("Not watching %1").arg(nick);
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
    m_messages.notifyMentioned();
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
    if (spec->wire == QuietWire::Privmsg) {
        noteLocalActivity();
        unawayAfterChat(session);
    }
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
    QString nick;
    switch (command.verb) {
    case IrcCommand::Verb::Ns:
        nick = QStringLiteral("NickServ");
        break;
    case IrcCommand::Verb::Cs:
        nick = QStringLiteral("ChanServ");
        break;
    case IrcCommand::Verb::Znc:
        nick = QStringLiteral("*status");
        break;
    default:
        return IrcCommandOutcome::Unsupported;
    }
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

IrcCommandOutcome IrcController::dispatchAutoaway(const IrcCommand& command,
                                                  IrcComposerSurface surface)
{
    const IrcAutoawayRequest request = ircParseAutoawayArgument(command.argument);
    if (request.kind == IrcAutoawayKind::Usage)
        return echoAutoawayUsage(surface);

    bool persist = false;
    QString text;
    const QString previousReason = autoawayReason();
    switch (request.kind) {
    case IrcAutoawayKind::Query:
        text = ircFormatAutoawayQuery(m_autoaway);
        break;
    case IrcAutoawayKind::Disable:
        m_autoaway.enabled = false;
        persist = true;
        stopAutoawayTimers();
        m_autoawayTripped = false;
        clearAutoAwayNetworks();
        text = ircFormatAutoawayConfirmation(m_autoaway);
        break;
    case IrcAutoawayKind::EnableOn:
        if (m_autoaway.timeoutSeconds < ircAutoawayMinTimeoutSeconds)
            return echoAutoawayUsage(surface);
        m_autoaway.enabled = true;
        persist = true;
        if (!m_autoawayTripped)
            armAutoawayIdle();
        text = ircFormatAutoawayConfirmation(m_autoaway);
        break;
    case IrcAutoawayKind::SetTimeout:
        m_autoaway.enabled = true;
        m_autoaway.timeoutSeconds = request.timeoutSeconds;
        if (!request.text.isEmpty())
            m_autoaway.oneShotReason = request.text;
        persist = true;
        if (!m_autoawayTripped)
            armAutoawayIdle();
        text = ircFormatAutoawayConfirmation(m_autoaway);
        break;
    case IrcAutoawayKind::SetDefaultReason:
        m_autoaway.defaultReason = request.text;
        persist = true;
        text = ircFormatAutoawayConfirmation(m_autoaway);
        break;
    case IrcAutoawayKind::ClearDefaultReason:
        m_autoaway.defaultReason.clear();
        persist = true;
        text = ircFormatAutoawayConfirmation(m_autoaway);
        break;
    case IrcAutoawayKind::Usage:
        return echoAutoawayUsage(surface);
    }
    if (persist)
        saveAutoaway();
    if (m_autoawayTripped && m_autoaway.enabled
        && autoawayReason() != previousReason) {
        refreshAutoAwayReason();
    }
    return echoAutoawayFeedback(surface, text);
}

IrcCommandOutcome IrcController::echoAutoawayFeedback(IrcComposerSurface surface,
                                                      const QString& text)
{
    QString networkId = queryNetworkId(surface);
    if (networkId.isEmpty())
        networkId = m_console.networkId();
    if (networkId.isEmpty())
        return IrcCommandOutcome::Refused;

    if (surface == IrcComposerSurface::Conversation) {
        if (!m_selected)
            return IrcCommandOutcome::WrongScope;
        apply(IrcWhoisTranscriptEvent{*m_selected, text});
        return IrcCommandOutcome::Sent;
    }
    m_console.record(IrcStatusEntry::outcome(networkId, text));
    return IrcCommandOutcome::Sent;
}

IrcCommandOutcome IrcController::echoAutoawayUsage(IrcComposerSurface surface)
{
    const IrcVerbSpec *spec = IrcVerbTable::find(IrcCommand::Verb::Autoaway);
    return echoAutoawayFeedback(
        surface,
        spec ? spec->usage
             : QStringLiteral("/autoaway [off|on|duration [reason]|reason [text]]"));
}

IrcCommandOutcome IrcController::echoPrefFeedback(IrcComposerSurface surface,
                                                  const QString& text)
{
    if (surface == IrcComposerSurface::Conversation) {
        if (m_selected)
            apply(IrcWhoisTranscriptEvent{*m_selected, text});
        return IrcCommandOutcome::Sent;
    }
    if (!m_console.networkId().isEmpty())
        m_console.record(IrcStatusEntry::outcome(m_console.networkId(), text));
    return IrcCommandOutcome::Sent;
}

IrcCommandOutcome IrcController::dispatchPref(const IrcCommand& command,
                                              IrcComposerSurface surface)
{
    const IrcPrefRequest request = ircParsePrefArgument(command.argument);
    if (request.kind == IrcPrefKind::Usage) {
        const IrcVerbSpec *spec = IrcVerbTable::find(IrcCommand::Verb::Pref);
        return echoPrefFeedback(surface, spec ? spec->usage : ircPrefUsage());
    }

    auto enabledFor = [this](IrcPrefName name) {
        switch (name) {
        case IrcPrefName::Directs:
            return reopenDirectMessages();
        case IrcPrefName::Avatars:
            return loadPeerAvatars();
        case IrcPrefName::Unread:
            return openConversationsAtUnread();
        }
        return false;
    };
    auto applyPref = [this](IrcPrefName name, bool enabled) {
        switch (name) {
        case IrcPrefName::Directs:
            setReopenDirectMessages(enabled);
            break;
        case IrcPrefName::Avatars:
            setLoadPeerAvatars(enabled);
            break;
        case IrcPrefName::Unread:
            setOpenConversationsAtUnread(enabled);
            break;
        }
    };

    if (request.kind == IrcPrefKind::Set)
        applyPref(request.name, request.enabled);

    if (request.kind == IrcPrefKind::QueryAll) {
        return echoPrefFeedback(
            surface,
            ircFormatPrefList(reopenDirectMessages(), loadPeerAvatars(),
                              openConversationsAtUnread()));
    }
    if (request.kind == IrcPrefKind::QueryOne) {
        return echoPrefFeedback(
            surface, ircFormatPrefQuery(request.name, enabledFor(request.name)));
    }
    return echoPrefFeedback(
        surface, ircFormatPrefState(request.name, enabledFor(request.name)));
}

void IrcController::saveAutoaway() const
{
    saveAutoawayConfig(m_autoaway);
}

void IrcController::armAutoawayIdle()
{
    stopAutoawayTimers();
    if (!m_autoaway.enabled
        || m_autoaway.timeoutSeconds < ircAutoawayMinTimeoutSeconds
        || m_autoaway.timeoutSeconds > ircAutoawayMaxTimeoutSeconds) {
        return;
    }
    m_autoawayIdle.start(m_autoaway.timeoutSeconds * 1000);
}

void IrcController::stopAutoawayTimers()
{
    m_autoawayIdle.stop();
    m_autoawayGrace.stop();
    m_autoawayGraceArmed = false;
}

void IrcController::onAutoawayIdle()
{
    if (!m_autoaway.enabled
        || m_autoaway.timeoutSeconds < ircAutoawayMinTimeoutSeconds)
        return;
    m_autoawayIdle.stop();
    m_autoawayGraceArmed = true;
    const int graceMs = ircAutoawayGraceSeconds(m_autoaway.timeoutSeconds) * 1000;
    if (graceMs <= 0) {
        onAutoawayGrace();
        return;
    }
    m_autoawayGrace.start(graceMs);
}

void IrcController::onAutoawayGrace()
{
    if (!m_autoawayGraceArmed)
        return;
    m_autoawayGraceArmed = false;
    m_autoawayGrace.stop();
    tripAutoaway();
}

QString IrcController::autoawayReason() const
{
    return !m_autoaway.oneShotReason.isEmpty()
        ? m_autoaway.oneShotReason
        : m_autoaway.defaultReason;
}

bool IrcController::markSessionAutoAway(IrcSession *session)
{
    if (!session || session->state() != IrcSession::State::Registered)
        return false;
    if (!session->markAway(autoawayReason()))
        return false;
    m_autoAwayNetworks.insert(session->networkId());
    return true;
}

void IrcController::noteManualAway(const QString& networkId)
{
    m_autoAwayNetworks.remove(networkId);
    m_manualAwayNetworks.insert(networkId);
}

void IrcController::noteAwayCleared(const QString& networkId)
{
    const bool wasAuto = m_autoAwayNetworks.remove(networkId);
    m_manualAwayNetworks.remove(networkId);
    if (wasAuto && m_autoAwayNetworks.isEmpty())
        m_autoaway.oneShotReason.clear();
}

void IrcController::refreshAutoAwayReason()
{
    if (!m_autoawayTripped || !m_autoaway.enabled)
        return;
    const QString reason = autoawayReason();
    for (const QString& networkId : m_autoAwayNetworks) {
        IrcSession *session = m_sessions.findSession(networkId);
        if (!session || session->state() != IrcSession::State::Registered)
            continue;
        session->markAway(reason);
    }
}

void IrcController::tripAutoaway()
{
    if (!m_autoaway.enabled)
        return;
    m_autoawayTripped = true;
    for (const QString& networkId : m_sessions.networkIds()) {
        if (m_autoAwayNetworks.contains(networkId)
            || m_manualAwayNetworks.contains(networkId)
            || m_reducer.selfAway(networkId)) {
            continue;
        }
        markSessionAutoAway(m_sessions.findSession(networkId));
    }
    stopAutoawayTimers();
}

void IrcController::clearAutoAwayNetworks()
{
    m_autoawayTripped = false;
    const QSet<QString> networks = m_autoAwayNetworks;
    m_autoAwayNetworks.clear();
    for (const QString& networkId : networks) {
        if (IrcSession *session = m_sessions.findSession(networkId)) {
            if (session->state() == IrcSession::State::Registered
                && session->clearAway()) {
                m_unawaySent.insert(networkId);
            }
        }
    }
    m_autoaway.oneShotReason.clear();
}

void IrcController::noteLocalActivity()
{
    const bool wasTripped = m_autoawayTripped;
    m_autoawayTripped = false;
    if (!m_autoAwayNetworks.isEmpty())
        clearAutoAwayNetworks();
    else if (wasTripped)
        m_autoaway.oneShotReason.clear();
    if (m_autoaway.enabled)
        armAutoawayIdle();
}

#ifdef OMAIRC_TEST
void IrcController::fireAutoawayIdleForTest()
{
    onAutoawayIdle();
}

void IrcController::fireAutoawayGraceForTest()
{
    onAutoawayGrace();
}

int IrcController::autoawayIdleIntervalMsForTest() const
{
    return m_autoawayIdle.interval();
}

bool IrcController::autoawayIdleIsActiveForTest() const
{
    return m_autoawayIdle.isActive();
}

int IrcController::autoawayGraceIntervalMsForTest() const
{
    return m_autoawayGrace.interval();
}

bool IrcController::autoawayGraceIsActiveForTest() const
{
    return m_autoawayGrace.isActive();
}
#endif

bool IrcController::eventFilter(QObject *watched, QEvent *event)
{
    Q_UNUSED(watched);
    if (!m_autoaway.enabled && !m_autoawayTripped && m_autoAwayNetworks.isEmpty())
        return false;
    switch (event->type()) {
    case QEvent::KeyPress:
    case QEvent::MouseButtonPress:
    case QEvent::TouchBegin:
    case QEvent::TabletPress:
    case QEvent::Wheel:
        break;
    default:
        return false;
    }
#ifdef QT_GUI_LIB
    if (const auto *gui =
            qobject_cast<QGuiApplication *>(QCoreApplication::instance())) {
        if (gui->applicationState() != Qt::ApplicationActive)
            return false;
    }
#endif
    noteLocalActivity();
    return false;
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

IrcCommandOutcome IrcController::dispatchList(const IrcCommand& command,
                                              IrcComposerSurface surface)
{
    const QString networkId = queryNetworkId(surface);
    if (networkId.isEmpty()) {
        if (surface == IrcComposerSurface::Conversation && !m_selected)
            return IrcCommandOutcome::WrongScope;
        return IrcCommandOutcome::Refused;
    }
    IrcSession *session = m_sessions.findSession(networkId);
    if (!session || session->state() != IrcSession::State::Registered)
        return IrcCommandOutcome::NotConnected;

    const QString mask = command.argument.trimmed();
    const auto found = m_channelLists.constFind(networkId);
    const bool sameMask = found != m_channelLists.cend()
        && sameListMask(found->mask, mask);
    const bool refresh = m_channelListPresented
        && m_channelList.networkId() == networkId
        && sameMask;
    if (found != m_channelLists.cend() && found->loading) {
        ChannelListCache &cache = m_channelLists[networkId];
        if (sameMask)
            cache.pendingMask.reset();
        else
            cache.pendingMask = mask;
        m_channelList.show(networkId, cache.mask, cache.rows, false, true, false,
                           cache.error);
        emit channelListRequested();
        return IrcCommandOutcome::Sent;
    }
    if (found != m_channelLists.cend() && !refresh && found->complete && sameMask) {
        m_channelList.show(networkId, found->mask, found->rows, true, false, true);
        emit channelListRequested();
        return IrcCommandOutcome::Sent;
    }

    if (!beginChannelListLoad(session, networkId, mask))
        return IrcCommandOutcome::Refused;
    emit channelListRequested();
    return IrcCommandOutcome::Sent;
}

IrcCommandOutcome IrcController::echoMetadataCommandFeedback(
    IrcComposerSurface surface,
    const QString& networkId,
    const QString& text)
{
    if (m_selected && m_selected->networkId == networkId) {
        apply(IrcWhoisTranscriptEvent{*m_selected, text});
        return IrcCommandOutcome::Sent;
    }
    if (surface == IrcComposerSurface::Conversation)
        return IrcCommandOutcome::WrongScope;
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

    const IrcCapabilitySet capabilities = m_capabilities.value(networkId);
    if (!capabilities.contains(IrcCapability::MemberMetadata)
            || !capabilities.contains(IrcCapability::Batch)) {
        return echoMetadataCommandFeedback(
            surface, networkId, ownMetadataNoCapMessage(IrcMetadata::statusKey()));
    }

    if (command.argument.isEmpty()) {
        const QString current =
            peerMetadata(networkId, session->nick())
                .value(IrcMetadata::statusKey())
                .toString();
        if (current.isEmpty()) {
            return echoMetadataCommandFeedback(
                surface, networkId,
                ownMetadataInspectEmptyMessage(IrcMetadata::statusKey()));
        }
        return echoMetadataCommandFeedback(
            surface, networkId,
            ownMetadataInspectValueMessage(IrcMetadata::statusKey(), current));
    }

    if (isOwnMetadataClearAlias(command.argument))
        return dispatchOwnMetadataClear(session, IrcMetadata::statusKey());

    const int valueBudget = IrcMetadata::effectiveMaxValueBytes(
        session->metadataCapability().maxValueBytes);
    if (valueBudget <= 0) {
        return echoMetadataCommandFeedback(
            surface, networkId,
            ownMetadataNoValueMessage(IrcMetadata::statusKey()));
    }
    const QString clamped = IrcMetadata::clamped(command.argument, valueBudget);
    if (clamped.isEmpty())
        return IrcCommandOutcome::Refused;
    return dispatchOwnMetadataSet(session, IrcMetadata::statusKey(), clamped);
}

IrcCommandOutcome IrcController::dispatchAvatar(const IrcCommand& command,
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

    const IrcCapabilitySet capabilities = m_capabilities.value(networkId);
    if (!capabilities.contains(IrcCapability::MemberMetadata)
            || !capabilities.contains(IrcCapability::Batch)) {
        return echoMetadataCommandFeedback(
            surface, networkId, ownMetadataNoCapMessage(IrcMetadata::avatarKey()));
    }

    if (command.argument.isEmpty()) {
        const QString current =
            peerMetadata(networkId, session->nick())
                .value(IrcMetadata::avatarKey())
                .toString();
        if (current.isEmpty()) {
            return echoMetadataCommandFeedback(
                surface, networkId,
                ownMetadataInspectEmptyMessage(IrcMetadata::avatarKey()));
        }
        return echoMetadataCommandFeedback(
            surface, networkId,
            ownMetadataInspectValueMessage(IrcMetadata::avatarKey(), current));
    }

    if (isOwnMetadataClearAlias(command.argument))
        return dispatchOwnMetadataClear(session, IrcMetadata::avatarKey());

    const QString resolved = ircAvatarMetadataValue(command.argument);
    if (resolved.isEmpty()) {
        return echoMetadataCommandFeedback(
            surface, networkId,
            QStringLiteral("Avatar must be an HTTPS URL or an email address."));
    }

    const int valueBudget = IrcMetadata::effectiveMaxValueBytes(
        session->metadataCapability().maxValueBytes);
    if (valueBudget <= 0) {
        return echoMetadataCommandFeedback(
            surface, networkId,
            ownMetadataNoValueMessage(IrcMetadata::avatarKey()));
    }
    const QString clamped = IrcMetadata::clamped(resolved, valueBudget);
    if (clamped.isEmpty())
        return IrcCommandOutcome::Refused;
    return dispatchOwnMetadataSet(session, IrcMetadata::avatarKey(), clamped);
}

IrcCommandOutcome IrcController::dispatchOwnMetadataClear(IrcSession *session,
                                                        const QString& metadataKey)
{
    if (!session->clearOwnMetadata(metadataKey))
        return IrcCommandOutcome::Refused;
    armOwnMetadataWatch(session->networkId(), metadataKey,
                        IrcOwnMetadataWatch::Kind::Clear, QString{});
    return IrcCommandOutcome::Sent;
}

IrcCommandOutcome IrcController::dispatchOwnMetadataSet(IrcSession *session,
                                                        const QString& metadataKey,
                                                        const QString& value)
{
    if (value.isEmpty() || !session->setOwnMetadata(metadataKey, value))
        return IrcCommandOutcome::Refused;
    armOwnMetadataWatch(session->networkId(), metadataKey,
                        IrcOwnMetadataWatch::Kind::Set, value);
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
    case IrcCommand::Verb::Autoaway:
    case IrcCommand::Verb::Pref:
    case IrcCommand::Verb::Status:
    case IrcCommand::Verb::Avatar:
    case IrcCommand::Verb::Whois:
    case IrcCommand::Verb::Mode:
    case IrcCommand::Verb::Kick:
    case IrcCommand::Verb::Invite:
    case IrcCommand::Verb::Ignore:
    case IrcCommand::Verb::Unignore:
    case IrcCommand::Verb::Ignored:
    case IrcCommand::Verb::Monitor:
    case IrcCommand::Verb::Unmonitor:
    case IrcCommand::Verb::Monitored:
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
    case IrcCommand::Verb::Znc:
    case IrcCommand::Verb::Raw:
    case IrcCommand::Verb::Help:
    case IrcCommand::Verb::List:
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

    QString requestLabel = session.startLabeledRequest();
    if (!requestLabel.isEmpty()) {
        m_labeledWatches.insert_or_assign(
            IrcLabeledWatchKey{session.networkId(), requestLabel},
            IrcLabeledWatch{IrcLabeledWatchKind::Whois, destination});
        if (!session.whois(nick, requestLabel)) {
            m_labeledWatches.erase(IrcLabeledWatchKey{session.networkId(), requestLabel});
            session.cancelRequestLabel(requestLabel);
            return false;
        }
        return true;
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
    routeOwnMetadataError(entry);
    if (!entry.requestLabel().isEmpty()) {
        auto found = m_labeledWatches.find(
            IrcLabeledWatchKey{entry.networkId(), entry.requestLabel()});
        if (found != m_labeledWatches.end()) {
            if (found->second.kind == IrcLabeledWatchKind::Whois) {
                if (const IrcWhoisLine *line = entry.whoisLine())
                    routeLabeledWhois(entry.networkId(), entry.requestLabel(), *line);
                else
                    routeLabeledStandardReply(entry);
            } else if (found->second.kind == IrcLabeledWatchKind::Ctcp) {
                if (const IrcCtcpReplyLine *line = entry.ctcpReply())
                    routeLabeledCtcp(entry.networkId(), entry.requestLabel(), *line,
                                     entry.text());
                else
                    routeLabeledStandardReply(entry);
            }
        }
    } else {
        if (const IrcWhoisLine *line = entry.whoisLine())
            routeWhoisLine(entry.networkId(), *line);
        if (const IrcCtcpReplyLine *line = entry.ctcpReply())
            routeCtcpReply(entry.networkId(), *line, entry.text());
    }
    if (entry.label() == QStringLiteral("INVITE")) {
        IrcSession *session = m_sessions.findSession(entry.networkId());
        if (!session)
            return;
        const std::optional<IrcPendingInvite> pending = session->pendingInvite();
        if (!pending)
            return;
        appendInbox({
            IrcInboxKind::Invite,
            QDateTime::currentDateTimeUtc(),
            entry.networkId(),
            pending->nick,
            pending->channel,
            entry.text(),
            IrcMsgId{},
        });
    }
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

void IrcController::routeLabeledWhois(const QString& networkId,
                                      const QString& requestLabel,
                                      const IrcWhoisLine& line)
{
    auto found = m_labeledWatches.find(IrcLabeledWatchKey{networkId, requestLabel});
    if (found == m_labeledWatches.end()
        || found->second.kind != IrcLabeledWatchKind::Whois) {
        return;
    }

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
        m_labeledWatches.erase(found);
}

void IrcController::routeLabeledCtcp(const QString& networkId,
                                     const QString& requestLabel,
                                     const IrcCtcpReplyLine& line,
                                     const QString& text)
{
    auto found = m_labeledWatches.find(IrcLabeledWatchKey{networkId, requestLabel});
    if (found == m_labeledWatches.end()
        || found->second.kind != IrcLabeledWatchKind::Ctcp) {
        return;
    }

    const IrcCtcpDestination destination = found->second.destination;
    m_labeledWatches.erase(found);
    if (const std::optional<IrcCtcpWatchKey> key =
            ctcpWatchKey(networkId, line.nick(), line.command())) {
        m_ctcpWatches.erase(*key);
    }

    if (std::holds_alternative<IrcWhoisStatusOnly>(destination) || text.isEmpty())
        return;
    apply(IrcWhoisTranscriptEvent{
        std::get<IrcConversationKey>(destination),
        text,
    });
}

void IrcController::routeLabeledStandardReply(const IrcStatusEntry& entry)
{
    auto found = m_labeledWatches.find(
        IrcLabeledWatchKey{entry.networkId(), entry.requestLabel()});
    if (found == m_labeledWatches.end())
        return;

    if (copiesLabeledStandardReply(entry)) {
        if (const auto *conversation =
                std::get_if<IrcConversationKey>(&found->second.destination)) {
            if (!entry.text().isEmpty()) {
                apply(IrcWhoisTranscriptEvent{
                    *conversation,
                    entry.text(),
                });
            }
        }
    }

    if (entry.severity() == IrcLogSeverity::Alert)
        m_labeledWatches.erase(found);
}

void IrcController::onRequestLabelFinished(const QString& networkId,
                                           const QString& requestLabel)
{
    if (networkId.isEmpty() || requestLabel.isEmpty())
        return;
    m_labeledWatches.erase(IrcLabeledWatchKey{networkId, requestLabel});
}

void IrcController::forgetLabeledWatches(const QString& networkId,
                                         IrcLabeledWatchKind kind)
{
    if (networkId.isEmpty())
        return;
    for (auto it = m_labeledWatches.begin(); it != m_labeledWatches.end(); ) {
        if (it->first.networkId == networkId && it->second.kind == kind)
            it = m_labeledWatches.erase(it);
        else
            ++it;
    }
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
    forgetLabeledWatches(networkId, IrcLabeledWatchKind::Whois);
}

void IrcController::forgetChannelList(const QString& networkId)
{
    if (networkId.isEmpty())
        return;
    stopChannelListIdle(networkId);
    m_channelLists.remove(networkId);
    if (m_channelList.networkId() == networkId)
        m_channelList.clear();
}

bool IrcController::beginChannelListLoad(IrcSession *session,
                                         const QString& networkId,
                                         const QString& mask)
{
    if (!session)
        return false;
    ChannelListCache &cache = m_channelLists[networkId];
    cache.mask = mask;
    cache.rows.clear();
    cache.complete = false;
    cache.loading = true;
    cache.error.clear();
    cache.pendingMask.reset();
    m_channelList.beginLoad(networkId, mask);
    if (!session->list(mask)) {
        stopChannelListIdle(networkId);
        cache.loading = false;
        m_channelLists.remove(networkId);
        m_channelList.clear();
        return false;
    }
    armChannelListIdle(networkId);
    return true;
}

bool IrcController::sameListMask(const QString& left, const QString& right) const
{
    return QString::compare(left.trimmed(), right.trimmed(), Qt::CaseInsensitive) == 0;
}

void IrcController::applyListRow(const QString& networkId, IrcChannelListRow row)
{
    if (row.channel.isEmpty())
        return;
    const auto found = m_channelLists.find(networkId);
    if (found == m_channelLists.end() || !found->loading)
        return;
    ChannelListCache &cache = *found;
    armChannelListIdle(networkId);
    bool replaced = false;
    for (IrcChannelListRow &existing : cache.rows) {
        if (existing.channel.compare(row.channel, Qt::CaseInsensitive) != 0)
            continue;
        existing = row;
        replaced = true;
        break;
    }
    if (!replaced) {
        if (cache.rows.size() >= ChannelListModel::kMaxRows)
            return;
        cache.rows.append(row);
    }
    if (m_channelList.networkId() != networkId || replaced)
        return;
    m_channelList.appendRow(row);
}

void IrcController::finishChannelList(const QString& networkId)
{
    const auto found = m_channelLists.find(networkId);
    if (found == m_channelLists.end())
        return;
    ChannelListCache &cache = *found;
    if (!cache.loading) {
        cache.drainTimedOutEnd = false;
        return;
    }
    if (cache.drainTimedOutEnd) {
        cache.drainTimedOutEnd = false;
        armChannelListIdle(networkId);
        return;
    }
    stopChannelListIdle(networkId);
    const std::optional<QString> pending = cache.pendingMask;
    cache.pendingMask.reset();
    cache.loading = false;
    cache.complete = true;
    cache.error.clear();
    if (pending && !sameListMask(*pending, cache.mask)) {
        IrcSession *session = m_sessions.findSession(networkId);
        if (session && session->state() == IrcSession::State::Registered) {
            if (beginChannelListLoad(session, networkId, *pending))
                emit channelListRequested();
            return;
        }
    }
    if (m_channelList.networkId() == networkId) {
        m_channelList.show(networkId, cache.mask, cache.rows, true, false, false);
    }
}

bool IrcController::failChannelList(const QString& networkId, const QString& text)
{
    const auto found = m_channelLists.find(networkId);
    if (found == m_channelLists.end() || !found->loading)
        return false;
    stopChannelListIdle(networkId);
    ChannelListCache &cache = *found;
    cache.loading = false;
    cache.complete = false;
    cache.pendingMask.reset();
    cache.error = text;
    if (m_channelList.networkId() == networkId)
        m_channelList.fail(text);
    return true;
}

void IrcController::armChannelListIdle(const QString& networkId)
{
    QTimer *&timer = m_channelListIdleTimers[networkId];
    if (!timer) {
        timer = new QTimer(this);
        timer->setSingleShot(true);
        connect(timer, &QTimer::timeout, this, [this, networkId] {
            const QString text =
                QStringLiteral("Channel list timed out. Try /list again.");
            if (failChannelList(networkId, text)) {
                m_console.record(IrcStatusEntry::outcome(networkId, text));
                const auto found = m_channelLists.find(networkId);
                if (found != m_channelLists.end())
                    found->drainTimedOutEnd = true;
            }
        });
    }
    timer->start(m_channelListIdleTimeoutMs);
}

void IrcController::stopChannelListIdle(const QString& networkId)
{
    QTimer *timer = m_channelListIdleTimers.take(networkId);
    if (!timer)
        return;
    timer->stop();
    timer->deleteLater();
}

bool IrcController::failChannelListFromNumeric(const QString& networkId,
                                               const IrcMessage& message)
{
    const auto found = m_channelLists.constFind(networkId);
    if (found == m_channelLists.cend() || !found->loading)
        return false;
    if (message.command.size() != 3)
        return false;
    const bool tryAgain = message.command == "263";
    const bool tooMany = message.command == "416";
    const bool errorNumeric = message.command[0] == '4' || message.command[0] == '5';
    if (!tryAgain && !tooMany && !errorNumeric)
        return false;
    bool mentionsList = false;
    for (std::size_t i = 0; i < message.parameters.size(); ++i) {
        if (parameter(message, i).compare(QLatin1String("LIST"), Qt::CaseInsensitive) == 0) {
            mentionsList = true;
            break;
        }
    }
    if (tryAgain && !mentionsList)
        return false;
    if (errorNumeric && !tooMany && !mentionsList)
        return false;
    QString text = parameter(message, message.parameters.empty()
                               ? 0
                               : message.parameters.size() - 1);
    if (text.isEmpty()) {
        if (tryAgain)
            text = QStringLiteral("Server load is too heavy. Try /list again.");
        else if (tooMany)
            text = QStringLiteral("Too many channel matches. Try a narrower /list.");
        else
            text = QStringLiteral("Channel list failed. Try /list again.");
    }
    failChannelList(networkId, text);
    return true;
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

    QString requestLabel = session.startLabeledRequest();
    if (!requestLabel.isEmpty()) {
        m_labeledWatches.insert_or_assign(
            IrcLabeledWatchKey{session.networkId(), requestLabel},
            IrcLabeledWatch{IrcLabeledWatchKind::Ctcp, destination});
    }
    m_ctcpWatches.insert_or_assign(*key, IrcCtcpWatch{std::move(destination)});
    if (!session.sendCtcp(nick, command, argument, requestLabel)) {
        if (!requestLabel.isEmpty()) {
            m_labeledWatches.erase(IrcLabeledWatchKey{session.networkId(), requestLabel});
            session.cancelRequestLabel(requestLabel);
        }
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
    forgetLabeledWatches(networkId, IrcLabeledWatchKind::Ctcp);
}

void IrcController::armOwnMetadataWatch(const QString& networkId,
                                        const QString& metadataKey,
                                        IrcOwnMetadataWatch::Kind kind,
                                        const QString& value)
{
    IrcWhoisDestination destination{IrcWhoisStatusOnly{}};
    if (m_selected && m_selected->networkId == networkId)
        destination = *m_selected;
    const QString canonical = IrcMetadata::canonicalKey(metadataKey);
    m_ownMetadataWatches[networkId].insert(
        canonical, IrcOwnMetadataWatch{std::move(destination), kind, value});
}

void IrcController::forgetOwnMetadataWatches(const QString& networkId)
{
    if (networkId.isEmpty())
        return;
    m_ownMetadataWatches.remove(networkId);
}

void IrcController::echoOwnMetadataOutcome(const QString& networkId,
                                           const IrcWhoisDestination& destination,
                                           const QString& text)
{
    if (const auto *conversation = std::get_if<IrcConversationKey>(&destination)) {
        apply(IrcWhoisTranscriptEvent{*conversation, text});
        return;
    }
    if (m_selected && m_selected->networkId == networkId) {
        apply(IrcWhoisTranscriptEvent{*m_selected, text});
        return;
    }
    m_console.record(IrcStatusEntry::outcome(networkId, text));
}

void IrcController::routeOwnMetadataReply(const QString& networkId,
                                          const QString& nick,
                                          const QString& key,
                                          const QString& value)
{
    const QString canonical = IrcMetadata::canonicalKey(key);
    auto networkWatches = m_ownMetadataWatches.find(networkId);
    if (networkWatches == m_ownMetadataWatches.end())
        return;
    auto found = networkWatches->find(canonical);
    if (found == networkWatches->end())
        return;
    const QString self = m_currentNicks.value(networkId);
    if (self.isEmpty()
        || !m_reducer.serverFeatures(networkId)
                .caseMapping()
                .equals(utf8(nick), utf8(self))) {
        return;
    }
    if (found->kind == IrcOwnMetadataWatch::Kind::Clear) {
        if (!value.isEmpty())
            return;
        echoOwnMetadataOutcome(networkId, found->destination,
                               ownMetadataClearedMessage(canonical));
        if (canonical == IrcMetadata::avatarKey())
            persistProfileAvatarUrl(networkId, QString{});
    } else {
        if (value.isEmpty() || found->value != value)
            return;
        echoOwnMetadataOutcome(networkId, found->destination,
                               ownMetadataSetMessage(canonical, value));
        if (canonical == IrcMetadata::avatarKey())
            persistProfileAvatarUrl(networkId, value);
    }
    networkWatches->erase(found);
    if (networkWatches->empty())
        m_ownMetadataWatches.erase(networkWatches);
}

void IrcController::routeOwnMetadataError(const IrcStatusEntry& entry)
{
    const QString networkId = entry.networkId();
    auto networkWatches = m_ownMetadataWatches.find(networkId);
    if (networkWatches == m_ownMetadataWatches.end())
        return;
    const QString label = entry.label();
    if (label != QStringLiteral("764") && label != QStringLiteral("767")
        && label != QStringLiteral("769")) {
        return;
    }
    const QString keyToken = firstToken(entry.text());
    if (keyToken.isEmpty())
        return;
    const QString canonical = IrcMetadata::canonicalKey(keyToken);
    auto found = networkWatches->find(canonical);
    if (found == networkWatches->end())
        return;
    const QString reason = entry.text().isEmpty() ? label : entry.text();
    const QString message = found->kind == IrcOwnMetadataWatch::Kind::Clear
        ? ownMetadataClearFailMessage(canonical, reason)
        : ownMetadataSetFailMessage(canonical, reason);
    echoOwnMetadataOutcome(networkId, found->destination, message);
    networkWatches->erase(found);
    if (networkWatches->empty())
        m_ownMetadataWatches.erase(networkWatches);
}

void IrcController::routeOwnMetadataFail(const QString& networkId,
                                         const IrcMessage& message)
{
    if (parameter(message, 0).compare(QLatin1String("METADATA"), Qt::CaseInsensitive)
        != 0) {
        return;
    }
    auto networkWatches = m_ownMetadataWatches.find(networkId);
    if (networkWatches == m_ownMetadataWatches.end())
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

    QStringList failKeys;
    const std::size_t lastContext =
        message.parameters.size() > 2 ? message.parameters.size() - 1 : 2;
    for (std::size_t index = 2; index < lastContext; ++index) {
        const QString token = parameter(message, index);
        if (!IrcMetadata::isKnownKey(token))
            continue;
        const QString canonical = IrcMetadata::canonicalKey(token);
        if (!failKeys.contains(canonical))
            failKeys.append(canonical);
    }

    const QString description = message.parameters.empty()
        ? code
        : parameter(message, message.parameters.size() - 1);
    const QString reason = description.isEmpty()
        ? code
        : QStringLiteral("%1 %2").arg(code, description);

    const auto finishWatch = [&](const QString& canonical,
                                 IrcOwnMetadataWatch watch) {
        if (watch.kind == IrcOwnMetadataWatch::Kind::Clear
            && code == QLatin1String("KEY_NOT_SET")) {
            echoOwnMetadataOutcome(networkId, watch.destination,
                                   ownMetadataClearedMessage(canonical));
            if (canonical == IrcMetadata::avatarKey())
                persistProfileAvatarUrl(networkId, QString{});
            return;
        }
        const QString outcome = watch.kind == IrcOwnMetadataWatch::Kind::Clear
            ? ownMetadataClearFailMessage(canonical, reason)
            : ownMetadataSetFailMessage(canonical, reason);
        echoOwnMetadataOutcome(networkId, watch.destination, outcome);
    };

    if (failKeys.isEmpty())
        return;

    for (const QString& canonical : failKeys) {
        auto found = networkWatches->find(canonical);
        if (found == networkWatches->end())
            continue;
        const IrcOwnMetadataWatch watch = *found;
        networkWatches->erase(found);
        finishWatch(canonical, watch);
    }
    if (networkWatches->empty())
        m_ownMetadataWatches.erase(networkWatches);
}

void IrcController::setProfileAvatarUrlCallbacks(ProfileAvatarUrlPersist persist,
                                                 ProfileAvatarUrlLookup lookup)
{
    m_profileAvatarUrlPersist = std::move(persist);
    m_profileAvatarUrlLookup = std::move(lookup);
}

QString IrcController::profileAvatarUrlForNetwork(const QString& networkId) const
{
    if (m_profileAvatarUrlLookup)
        return m_profileAvatarUrlLookup(networkId);
    return {};
}

void IrcController::persistProfileAvatarUrl(const QString& networkId,
                                              const QString& url)
{
    if (networkId.isEmpty() || !m_profileAvatarUrlPersist)
        return;
    m_profileAvatarUrlPersist(networkId, url);
}

void IrcController::applyProfileAvatarOnConnect(IrcSession *session)
{
    if (!session)
        return;
    const QString networkId = session->networkId();
    if (m_appliedProfileAvatars.contains(networkId))
        return;
    const QString avatarUrl =
        ircAvatarMetadataValue(profileAvatarUrlForNetwork(networkId));
    if (avatarUrl.isEmpty())
        return;
    const IrcCapabilitySet capabilities = m_capabilities.value(networkId);
    if (!capabilities.contains(IrcCapability::MemberMetadata)
            || !capabilities.contains(IrcCapability::Batch)) {
        return;
    }
    if (!session->setOwnMetadata(IrcMetadata::avatarKey(), avatarUrl))
        return;
    m_appliedProfileAvatars.insert(networkId);
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
        if (session->clearAway()) {
            m_unawaySent.insert(networkId);
            noteAwayCleared(networkId);
        }
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
        m_autoAwayNetworks.remove(welcome->networkId);
        m_manualAwayNetworks.remove(welcome->networkId);
        forgetWhoisWatches(welcome->networkId);
        forgetCtcpWatches(welcome->networkId);
        forgetOwnMetadataWatches(welcome->networkId);
    } else if (const auto *selfAway = std::get_if<IrcSelfAwayEvent>(&event)) {
        m_unawaySent.remove(selfAway->networkId);
    }
    m_reducer.apply(event);
    if (const auto *metadata = std::get_if<IrcMemberMetadataEvent>(&event)) {
        ++m_peerMetadataEpoch;
        emit peerMetadataChanged();
        routeOwnMetadataReply(metadata->networkId,
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
    if (std::optional<IrcInboxArrival> arrival = m_reducer.takeInboxArrival()) {
        appendInbox({
            arrival->kind,
            QDateTime::currentDateTimeUtc(),
            arrival->networkId,
            arrival->actor,
            arrival->target,
            arrival->body,
            arrival->msgid,
        });
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
        routeOwnMetadataFail(networkId, message);
    if (message.command == "005") {
        IrcServerFeatures features = m_reducer.serverFeatures(networkId);
        if (message.parameters.size() > 2) {
            std::vector<std::string> tokens(
                message.parameters.begin() + 1, message.parameters.end() - 1);
            features.applyTokens(tokens);
            m_reducer.setServerFeatures(networkId, features);
            emit serverFeaturesChanged();
            subscribeMonitors(networkId);
        }
        return;
    }
    if (message.command == "730") {
        handleMonitorPresence(networkId, message, true);
        return;
    }
    if (message.command == "731") {
        handleMonitorPresence(networkId, message, false);
        return;
    }
    if (message.command == "734") {
        handleMonitorListFull(networkId, message);
        return;
    }
    if (message.command == "376" || message.command == "422")
        noteOpenDirectsMotd(networkId);
    if (message.command == "322") {
        if (message.parameters.size() >= 3) {
            bool ok = false;
            const int users = parameter(message, 2).toInt(&ok);
            applyListRow(networkId, {
                parameter(message, 1),
                ok ? users : 0,
                parameter(message, 3),
            });
        }
        return;
    }
    if (message.command == "321") {
        const auto found = m_channelLists.find(networkId);
        if (found != m_channelLists.end() && found->loading)
            armChannelListIdle(networkId);
        return;
    }
    if (message.command == "323") {
        finishChannelList(networkId);
        return;
    }
    if (failChannelListFromNumeric(networkId, message))
        return;
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
            const auto& mapping = features.caseMapping();
            const bool selfJoin =
                mapping.equals(utf8(join->nick), utf8(currentNick));
            const IrcConversationKey joinKey =
                m_reducer.conversationKey(join->networkId, join->channel);
            if (selfJoin && m_cancelledPendingJoins.erase(joinKey)) {
                if (IrcSession *session = m_sessions.findSession(join->networkId))
                    session->part(join->channel);
                dismissChannel(join->networkId, join->channel);
                continue;
            }
            if (selfJoin
                && features.isChannel(utf8(join->channel))) {
                m_inbox.consumeInvite(join->networkId, join->channel, mapping);
                syncInbox();
                if (IrcSession *session = m_sessions.findSession(join->networkId)) {
                    if (const auto pending = session->pendingInvite()) {
                        if (mapping.equals(utf8(join->channel),
                                          utf8(pending->channel)))
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
