#include "irccontroller.h"

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
#include "ircplaybacktime.h"
#include "ircprefixnick.h"
#include "ircpresence.h"
#include "ircservicenick.h"
#include "ircjointarget.h"
#include "ircviewnotify.h"
#include "irctcp.h"
#include "irctyping.h"
#include "ircwiretext.h"

#include <QByteArray>
#include <QDateTime>
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

QString openConversationsAtUnreadKey()
{
    return QStringLiteral("openConversationsAtUnread");
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

bool servicesAccountMatches(const QString& stored, const QString& incoming)
{
    const auto canonical = [](const QString& value) {
        return (value.isEmpty() || value == QLatin1String("*")) ? QString() : value;
    };
    return canonical(stored) == canonical(incoming);
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
    , m_playback(m_playbackTimes, m_reducer)
    , m_replies(m_reducer, IrcReplyRouter::Host{
          [this](const IrcWhoisTranscriptEvent& event) { apply(event); },
          [this](const IrcStatusEntry& entry) { m_console.record(entry); },
          [this]() -> std::optional<IrcConversationKey> { return m_selected; },
          [this](const QString& networkId) { return m_currentNicks.value(networkId); },
          [this](const QString& networkId, const QString& url) {
              persistProfileAvatarUrl(networkId, url);
          },
          [this](const QString& networkId) { return m_capabilities.value(networkId); },
          [this](IrcComposerSurface surface) { return queryNetworkId(surface); },
          [this](IrcComposerSurface surface) { return sessionFor(surface); },
          [this]() { return selectedSession(); },
          [this]() { return selectedTarget(); },
          [this]() { return selectedIsCloseableDirect(); },
          [this]() { return !m_sessions.networkIds().isEmpty(); },
      })
    , m_monitorCoord(m_reducer, m_monitors, m_mutes, IrcMonitorCoordinator::Host{
          [this](const IrcStatusEntry& entry) { m_console.record(entry); },
          [this](const QString& networkId, const QString& display,
                 const QString& body, bool newlyOnline) {
              emit monitorArrived(display, body, networkId, display);
              if (newlyOnline) {
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
          },
          [this](IrcComposerSurface surface) { return queryNetworkId(surface); },
          [this](IrcComposerSurface surface) { return sessionFor(surface); },
          [this](const QString& networkId) {
              return m_sessions.findSession(networkId);
          },
          [this]() -> std::optional<IrcConversationKey> { return m_selected; },
      })
    , m_commands(m_reducer, m_ignores, m_mutes, m_highlights,
                 IrcCommandDispatcher::Host{
          [this](IrcComposerSurface surface) { return queryNetworkId(surface); },
          [this](IrcComposerSurface surface) { return sessionFor(surface); },
          [this](const QString& networkId) {
              return m_sessions.findSession(networkId);
          },
          [this]() { return selectedSession(); },
          [this]() { return selectedTarget(); },
          [this]() { return isChannel(); },
          [this]() { return selectedIsCloseableDirect(); },
          [this]() -> std::optional<IrcConversationKey> { return m_selected; },
          [this]() { return !m_sessions.networkIds().isEmpty(); },
          [this]() { return m_console.networkId(); },
          [this](IrcMessageKind kind, const QString& body) {
              echoLocal(kind, body);
          },
          [this](const QString& networkId, const QString& channel) {
              openJoinedChannel(networkId, channel);
          },
          [this](const QString& networkId, const QString& channel) {
              return dismissChannel(networkId, channel);
          },
          [this]() { dropSelectedDirectAndReselect(); },
          [this](IrcComposerSurface surface) { return clearSurface(surface); },
          [this](const QString& networkId, const QString& target) {
              rememberOpenDirect(networkId, target);
          },
          [this](const QString& networkId, const QString& target) {
              noteNickDelivery(networkId, target);
          },
          [this]() { m_autoawayRuntime.noteLocalActivity(); },
          [this](IrcSession *session) { unawayAfterChat(session); },
          [this](const QString& networkId) { noteManualAway(networkId); },
          [this](const QString& networkId) { noteAwayCleared(networkId); },
          [this]() { m_typingTarget.clear(); },
          [this](const QString& body) { return sendSelectedMessage(body); },
          [this](IrcSession *session, const QString& target, const QString& body,
                 IrcCommandDispatcher::QuietWire wire) {
              echoIfPresent(session, target, body, wire);
          },
          [this](const QString& networkId, const QString& target, bool muted) {
              return applyMute(networkId, target, muted);
          },
          [this](const QString& networkId) { syncHighlightWords(networkId); },
          [this]() { m_conversations.reload(); },
          [this](const QString& networkId, const QString& target) {
              selectConversation(networkId, target);
          },
          [this](const IrcWhoisTranscriptEvent& event) { apply(event); },
          [this](const IrcStatusEntry& entry) { m_console.record(entry); },
          [this](const IrcCommand& command, IrcComposerSurface surface) {
              return dispatchList(command, surface);
          },
          [this](const IrcCommand& command, IrcComposerSurface surface) {
              return m_autoawayRuntime.dispatchAutoaway(command, surface);
          },
          [this](const IrcCommand& command, IrcComposerSurface surface) {
              return m_replies.dispatchWhois(command, surface);
          },
          [this](const IrcCommand& command, IrcComposerSurface surface) {
              return m_replies.dispatchCtcp(command, surface);
          },
          [this](const IrcCommand& command, IrcComposerSurface surface) {
              return m_replies.dispatchStatus(command, surface);
          },
          [this](const IrcCommand& command, IrcComposerSurface surface) {
              return m_replies.dispatchAvatar(command, surface);
          },
          [this](const IrcCommand& command, IrcComposerSurface surface) {
              return m_monitorCoord.dispatchMonitor(command, surface);
          },
          [this](IrcPrefName name) {
              switch (name) {
              case IrcPrefName::Directs:
                  return reopenDirectMessages();
              case IrcPrefName::Avatars:
                  return loadPeerAvatars();
              case IrcPrefName::Unread:
                  return openConversationsAtUnread();
              }
              return false;
          },
          [this](IrcPrefName name, bool enabled) {
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
          },
      })
    , m_autoawayRuntime(IrcAutoawayRuntime::Host{
          [this](const QString& networkId) {
              return m_sessions.findSession(networkId);
          },
          [this]() { return m_sessions.networkIds(); },
          [this](const QString& networkId) {
              return m_reducer.selfAway(networkId);
          },
          [this](const QString& networkId) { m_unawaySent.insert(networkId); },
          [this](const QString& networkId, const QString& text) {
              m_console.record(IrcStatusEntry::outcome(networkId, text));
          },
          [this](IrcComposerSurface surface) { return queryNetworkId(surface); },
          [this]() { return m_console.networkId(); },
          [this]() -> std::optional<IrcConversationKey> { return m_selected; },
          [this](const IrcEvent& event) { apply(event); },
      })
{
    connect(&m_channelLists, &IrcChannelListRequest::rowChanged, this,
            [this](const QString& networkId, const IrcChannelListRow& row, bool replaced) {
        if (m_channelList.networkId() == networkId && !replaced)
            m_channelList.appendRow(row);
    });
    connect(&m_channelLists, &IrcChannelListRequest::timedOut, this,
            [this](const QString& networkId, const QString& text) {
        if (m_channelList.networkId() == networkId)
            m_channelList.fail(text);
        m_console.record(IrcStatusEntry::outcome(networkId, text));
    });
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
    loadStoredPreferences();
}

void IrcController::setTranscriptRoot(const QString &root)
{
    m_transcripts.setRoot(root);
}

void IrcController::setEphemeral(bool ephemeral)
{
    m_ephemeral = ephemeral;
    if (ephemeral)
        m_reducer.setConversationLog(nullptr);
    m_openDirects.setEphemeral(ephemeral);
    m_playbackTimes.setEphemeral(ephemeral);
    m_autoawayRuntime.setEphemeral(ephemeral);
}

void IrcController::loadStoredPreferences()
{
    if (m_ephemeral)
        return;
    m_reopenDirectMessages = loadReopenDirectMessages();
    m_loadPeerAvatars = loadLoadPeerAvatars();
    m_openConversationsAtUnread = loadOpenConversationsAtUnread();
    m_autoawayRuntime.loadStored();
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
        m_playback.onRegistered(networkId, session->autojoinChannels());
        m_currentNicks[networkId] = session->nick();
        m_reducer.setServerFeatures(networkId, IrcServerFeatures{});
        m_monitorCoord.forgetPresence(networkId);
        emit serverFeaturesChanged();
        apply(IrcWelcomeEvent{networkId, session->nick()});
        m_autoawayRuntime.onSessionRegistered(session);
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
            m_playback.onLeftRegistration(session->networkId());
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
    m_replies.forget(networkId);
    forgetChannelList(networkId);
    const QString previousId = identityNetworkId();
    const bool previousAway = selfAway();
    m_reducer.apply(IrcSelfAwayEvent{networkId, false});
    m_unawaySent.remove(networkId);
    m_autoawayRuntime.forgetNetwork(networkId);
    m_openDirectsMotdSeen.remove(networkId);
    m_monitorCoord.forgetPresence(networkId);
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
    m_replies.forget(networkId);
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
    m_commands.forgetNetwork(networkId);
    m_unawaySent.remove(networkId);
    m_autoawayRuntime.forgetNetwork(networkId);
    m_openDirectsMotdSeen.remove(networkId);
    m_monitorCoord.forgetPresence(networkId);
    m_currentNicks.remove(networkId);
    m_capabilities.remove(networkId);
    m_appliedProfileAvatars.remove(networkId);
    m_lastErrors.remove(networkId);
    m_ignores.forget(networkId);
    m_monitors.forget(networkId);
    m_mutes.forget(networkId);
    m_openDirects.forget(networkId);
    m_playbackTimes.forget(networkId);
    m_playback.dropSnapshot(networkId);
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

int IrcController::peerAccountEpoch() const
{
    return m_peerAccountEpoch;
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
    if (!m_ephemeral)
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
    if (!m_ephemeral)
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
    if (!m_ephemeral)
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
    m_channelLists.setIdleTimeoutMs(milliseconds);
}

#ifdef OMAIRC_TEST
void IrcController::fireChannelListIdleTimeoutForTest(const QString& networkId)
{
    m_channelLists.fireIdleTimeout(networkId);
}
#endif

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
        m_commands.takeCancelledSelfJoin(key);
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

QString IrcController::peerAccount(const QString& networkId,
                                   const QString& nick) const
{
    return m_reducer.displayAccount(networkId, nick);
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
        requestZncPlayback(session);
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

void IrcController::dismissInboxItem(int row)
{
    if (row < 0 || row >= m_inbox.count())
        return;

    m_inbox.consumeAt(row);
    syncInbox();
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
    m_reducer.markSelected(key);
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
    echoIfPresent(session, target, text, IrcCommandDispatcher::QuietWire::Privmsg);
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
    return m_commands.dispatch(command, surface);
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
    // Held self-only query batches splice into directs this restore just
    // opened. Release after restore, and do not drop them first.
    const bool spliced = m_reducer.releasePendingQueryPlayback(networkId);
    noteKeptReplay();
    if (spliced) {
        IrcViewNotify notify;
        notify.conversations = true;
        notify.messages = true;
        publish(notify);
    }
    if (IrcSession *session = m_sessions.findSession(networkId))
        requestZncPlayback(session);
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

void IrcController::syncHighlightWords(const QString& networkId)
{
    m_reducer.setHighlightWords(networkId, m_highlights.words(networkId));
    m_messages.notifyMentioned();
}

void IrcController::noteLocalActivity()
{
    m_autoawayRuntime.noteLocalActivity();
}

void IrcController::noteManualAway(const QString& networkId)
{
    m_autoawayRuntime.noteManualAway(networkId);
}

void IrcController::noteAwayCleared(const QString& networkId)
{
    m_autoawayRuntime.noteAwayCleared(networkId);
}

#ifdef OMAIRC_TEST
void IrcController::fireAutoawayIdleForTest()
{
    m_autoawayRuntime.fireAutoawayIdleForTest();
}

void IrcController::fireAutoawayGraceForTest()
{
    m_autoawayRuntime.fireAutoawayGraceForTest();
}

int IrcController::autoawayIdleIntervalMsForTest() const
{
    return m_autoawayRuntime.autoawayIdleIntervalMsForTest();
}

bool IrcController::autoawayIdleIsActiveForTest() const
{
    return m_autoawayRuntime.autoawayIdleIsActiveForTest();
}

int IrcController::autoawayGraceIntervalMsForTest() const
{
    return m_autoawayRuntime.autoawayGraceIntervalMsForTest();
}

bool IrcController::autoawayGraceIsActiveForTest() const
{
    return m_autoawayRuntime.autoawayGraceIsActiveForTest();
}
#endif

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
    const auto *found = m_channelLists.state(networkId);
    const bool sameMask = found && IrcChannelListRequest::sameMask(found->mask, mask);
    const bool refresh = m_channelListPresented
        && m_channelList.networkId() == networkId
        && sameMask;
    const auto request = m_channelLists.request(networkId, mask, refresh);
    found = m_channelLists.state(networkId);
    if (request == IrcChannelListRequest::Request::Loading) {
        m_channelList.show(networkId, found->mask, found->rows, false, true, false,
                           found->error);
        emit channelListRequested();
        return IrcCommandOutcome::Sent;
    }
    if (request == IrcChannelListRequest::Request::Cached) {
        m_channelList.show(networkId, found->mask, found->rows, true, false, true);
        emit channelListRequested();
        return IrcCommandOutcome::Sent;
    }

    m_channelList.beginLoad(networkId, mask);
    if (!session->list(mask)) {
        m_channelLists.cancelStart(networkId);
        m_channelList.clear();
        return IrcCommandOutcome::Refused;
    }
    emit channelListRequested();
    return IrcCommandOutcome::Sent;
}

void IrcController::noteNickDelivery(const QString& networkId, const QString& target)
{
    m_replies.noteNickDelivery(networkId, target);
}

void IrcController::handleStatusEntry(const IrcStatusEntry& entry)
{
    m_replies.routeStatusEntry(entry);
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

void IrcController::onRequestLabelFinished(const QString& networkId,
                                           const QString& requestLabel)
{
    m_replies.requestLabelFinished(networkId, requestLabel);
}

void IrcController::forgetChannelList(const QString& networkId)
{
    if (networkId.isEmpty())
        return;
    m_channelLists.forget(networkId);
    if (m_channelList.networkId() == networkId)
        m_channelList.clear();
}

bool IrcController::beginChannelListLoad(IrcSession *session,
                                         const QString& networkId,
                                         const QString& mask)
{
    if (!session)
        return false;
    m_channelLists.request(networkId, mask, true);
    m_channelList.beginLoad(networkId, mask);
    if (!session->list(mask)) {
        m_channelLists.cancelStart(networkId);
        m_channelList.clear();
        return false;
    }
    return true;
}

void IrcController::applyListRow(const QString& networkId, IrcChannelListRow row)
{
    m_channelLists.row(networkId, std::move(row));
}

void IrcController::finishChannelList(const QString& networkId)
{
    const IrcChannelListRequest::FinishResult result = m_channelLists.finish(networkId);
    if (!result.completed)
        return;
    const auto *cache = m_channelLists.state(networkId);
    if (!cache || cache->loading)
        return;
    if (result.pendingMask) {
        IrcSession *session = m_sessions.findSession(networkId);
        if (session && session->state() == IrcSession::State::Registered) {
            if (beginChannelListLoad(session, networkId, *result.pendingMask))
                emit channelListRequested();
            return;
        }
    }
    if (m_channelList.networkId() == networkId)
        m_channelList.show(networkId, cache->mask, cache->rows, true, false, false);
}

bool IrcController::failChannelList(const QString& networkId, const QString& text)
{
    if (!m_channelLists.fail(networkId, text)) return false;
    if (m_channelList.networkId() == networkId)
        m_channelList.fail(text);
    return true;
}

bool IrcController::failChannelListFromNumeric(const QString& networkId,
                                               const IrcMessage& message)
{
    const auto *found = m_channelLists.state(networkId);
    if (!found || !found->loading)
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
                                  IrcCommandDispatcher::QuietWire wire)
{
    if (wire == IrcCommandDispatcher::QuietWire::Privmsg
        && session->capabilities().contains(IrcCapability::EchoMessage)) {
        return;
    }
    const IrcConversationKey key =
        m_reducer.conversationKey(session->networkId(), target);
    if (!m_reducer.find(key))
        return;
    const QDateTime now = QDateTime::currentDateTimeUtc();
    const QString nick = session->nick();
    if (wire == IrcCommandDispatcher::QuietWire::Notice) {
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
    if (const auto *account = std::get_if<IrcAccountEvent>(&event)) {
        const QString stored =
            m_reducer.nickPresence(account->networkId, account->nick).account;
        if (servicesAccountMatches(stored, account->account))
            return;
    }
    const QString previousId = identityNetworkId();
    const bool previousAway = selfAway();
    const bool typingOnly = std::holds_alternative<IrcTypingEvent>(event);
    const bool selfAwayOnly = std::holds_alternative<IrcSelfAwayEvent>(event);
    if (const auto *welcome = std::get_if<IrcWelcomeEvent>(&event)) {
        m_unawaySent.remove(welcome->networkId);
        m_autoawayRuntime.forgetNetwork(welcome->networkId);
        m_replies.forget(welcome->networkId);
    } else if (const auto *selfAway = std::get_if<IrcSelfAwayEvent>(&event)) {
        m_unawaySent.remove(selfAway->networkId);
    }
    m_reducer.apply(event);
    noteKeptReplay();
    if (const auto *metadata = std::get_if<IrcMemberMetadataEvent>(&event)) {
        ++m_peerMetadataEpoch;
        emit peerMetadataChanged();
        m_replies.routeOwnMetadataReply(metadata->networkId,
                                 metadata->nick,
                                 metadata->key,
                                 metadata->value);
    }
    const bool accountsMoved = std::holds_alternative<IrcAccountEvent>(event)
        || std::holds_alternative<IrcWelcomeEvent>(event)
        || std::holds_alternative<IrcPartEvent>(event)
        || std::holds_alternative<IrcQuitEvent>(event)
        || std::holds_alternative<IrcKickEvent>(event)
        || std::holds_alternative<IrcNickEvent>(event)
        || (std::holds_alternative<IrcJoinEvent>(event)
            && std::get<IrcJoinEvent>(event).account.has_value());
    if (accountsMoved) {
        ++m_peerAccountEpoch;
        emit peerAccountChanged();
    }
    if (const auto *nick = std::get_if<IrcNickEvent>(&event)) {
        const IrcCaseMapping& mapping =
            m_reducer.serverFeatures(nick->networkId).caseMapping();
        m_openDirects.rekey(nick->networkId, nick->oldNick, nick->newNick, mapping);
        m_playbackTimes.rekey(nick->networkId, nick->oldNick, nick->newNick, mapping);
        m_playback.rekey(nick->networkId, nick->oldNick, nick->newNick);
    } else if (const auto *message = std::get_if<IrcMessageEvent>(&event)) {
        if (m_reducer.serverFeatures(message->conversation.networkId)
                .caseMapping()
                .equals(utf8(message->author),
                        utf8(m_currentNicks.value(message->conversation.networkId)))) {
            const IrcConversationState *existing =
                m_reducer.find(message->conversation);
            if (existing && !existing->isChannel())
                rememberOpenDirect(message->conversation.networkId, message->target);
        }
    } else if (const auto *notice = std::get_if<IrcNoticeEvent>(&event)) {
        if (m_reducer.serverFeatures(notice->conversation.networkId)
                .caseMapping()
                .equals(utf8(notice->author),
                        utf8(m_currentNicks.value(notice->conversation.networkId)))) {
            const IrcConversationState *existing =
                m_reducer.find(notice->conversation);
            if (existing && !existing->isChannel())
                rememberOpenDirect(notice->conversation.networkId, notice->target);
        }
    } else if (const auto *action = std::get_if<IrcActionEvent>(&event)) {
        if (m_reducer.serverFeatures(action->conversation.networkId)
                .caseMapping()
                .equals(utf8(action->author),
                        utf8(m_currentNicks.value(action->conversation.networkId)))) {
            const IrcConversationState *existing =
                m_reducer.find(action->conversation);
            if (existing && !existing->isChannel())
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
    notePlaybackClock(networkId, message);
    if (message.command == "FAIL")
        m_replies.routeOwnMetadataFail(networkId, message);
    if (message.command == "005") {
        IrcServerFeatures features = m_reducer.serverFeatures(networkId);
        if (message.parameters.size() > 2) {
            std::vector<std::string> tokens(
                message.parameters.begin() + 1, message.parameters.end() - 1);
            features.applyTokens(tokens);
            m_reducer.setServerFeatures(networkId, features);
            emit serverFeaturesChanged();
            m_monitorCoord.subscribeMonitors(networkId);
        }
        return;
    }
    if (message.command == "730") {
        m_monitorCoord.handleMonitorPresence(networkId, message, true);
        return;
    }
    if (message.command == "731") {
        m_monitorCoord.handleMonitorPresence(networkId, message, false);
        return;
    }
    if (message.command == "734") {
        m_monitorCoord.handleMonitorListFull(networkId, message);
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
        m_channelLists.activity(networkId);
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
            if (selfJoin && m_commands.takeCancelledSelfJoin(joinKey)) {
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
                    m_playback.noteJoinedChannel(join->networkId, join->channel);
                    requestZncChannelPlayback(session, join->channel);
                }
            }
        }
    }
}

void IrcController::handleHistoryBatch(const QString& networkId,
                                       const IrcHistoryBatch& batch)
{
    std::optional<IrcHistoryEvent> event = IrcEventTranslator::translateHistory(
        networkId, m_currentNicks.value(networkId),
        m_reducer.serverFeatures(networkId), batch);
    if (!event)
        return;
    if (event->kind == IrcHistoryKind::BouncerPlayback) {
        m_playback.trimBouncerBatch(networkId, *event);
        if (event->lines.empty())
            return;
    }
    apply(*event);
}

void IrcController::noteKeptReplay()
{
    m_playback.noteKeptReplay([this](const QString& networkId) {
        return m_capabilities.value(networkId)
            .contains(IrcCapability::ZncPlayback);
    });
    // A kept self line means the user replied in that query. Remember it
    // only when the direct exists; a dropped self-only batch must not.
    for (const IrcRememberedQuery& query : m_reducer.takeRememberedQueries()) {
        if (!m_reducer.find(m_reducer.conversationKey(query.networkId, query.target)))
            continue;
        rememberOpenDirect(query.networkId, query.target);
    }
}

void IrcController::notePlaybackClock(const QString& networkId, const IrcMessage& message)
{
    m_playback.notePlaybackClock(networkId, message,
                                 m_currentNicks.value(networkId),
                                 m_capabilities.value(networkId)
                                     .contains(IrcCapability::ZncPlayback));
}

void IrcController::requestZncPlayback(IrcSession *session)
{
    if (!session)
        return;
    const QString networkId = session->networkId();
    const IrcServerFeatures& features = m_reducer.serverFeatures(networkId);
    QStringList restoredDirects = m_openDirects.listed(networkId,
                                                       features.caseMapping());
    m_playback.request(
        session,
        m_capabilities.value(networkId).contains(IrcCapability::ZncPlayback),
        m_openDirectsMotdSeen.contains(networkId),
        restoredDirects,
        [this, networkId](const QString& target) {
            return persistableDirectTarget(networkId, target);
        });
}

void IrcController::requestZncChannelPlayback(IrcSession *session,
                                              const QString& channel)
{
    if (!session)
        return;
    m_playback.requestChannelPlayback(
        session, channel,
        m_capabilities.value(session->networkId())
            .contains(IrcCapability::ZncPlayback),
        m_openDirectsMotdSeen.contains(session->networkId()));
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
