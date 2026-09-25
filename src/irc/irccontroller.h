#pragma once

#include "channellistmodel.h"
#include "conversationlistmodel.h"
#include "ircconversationlog.h"
#include "ircchannellistrequest.h"
#include "irceventreducer.h"
#include "irchighlight.h"
#include "ircignore.h"
#include "ircinbox.h"
#include "ircinboxmodel.h"
#include "ircmonitor.h"
#include "irccommanddispatcher.h"
#include "ircmonitorcoordinator.h"
#include "ircautoawayruntime.h"
#include "ircmute.h"
#include "ircopendirect.h"
#include "ircplaybackcoordinator.h"
#include "ircplaybacktime.h"
#include "ircreplyrouter.h"
#include "ircsessionmanager.h"
#include "ircstatusconsole.h"
#include "ircstatusentry.h"
#include "memberlistmodel.h"
#include "messagelistmodel.h"

#include <QHash>
#include <QObject>
#include <QSet>
#include <QStringList>
#include <QTimer>
#include <QVariantMap>
#include <QVector>

#include <QDateTime>
#include <functional>
#include <optional>
#include <set>
#include <variant>

struct IrcViewNotify;

class IrcController : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QAbstractItemModel* conversations READ conversations CONSTANT)
    Q_PROPERTY(QAbstractItemModel* messages READ messages CONSTANT)
    Q_PROPERTY(QAbstractItemModel* members READ members CONSTANT)
    Q_PROPERTY(QAbstractItemModel* channelList READ channelList CONSTANT)
    Q_PROPERTY(QString selectedNetworkId READ selectedNetworkId NOTIFY selectionChanged)
    Q_PROPERTY(QString selectedTarget READ selectedTarget NOTIFY selectionChanged)
    Q_PROPERTY(QString selectedConversationId READ selectedConversationId NOTIFY selectionChanged)
    Q_PROPERTY(QString focusedNetworkId READ focusedNetworkId NOTIFY selectionChanged)
    Q_PROPERTY(QString topic READ topic NOTIFY selectionChanged)
    Q_PROPERTY(bool isChannel READ isChannel NOTIFY selectionChanged)
    Q_PROPERTY(int peopleCount READ peopleCount NOTIFY selectionChanged)
    Q_PROPERTY(QString connectionStatus READ connectionStatus NOTIFY statusChanged)
    Q_PROPERTY(QString lastError READ lastError NOTIFY statusChanged)
    Q_PROPERTY(int conversationEpoch READ conversationEpoch NOTIFY conversationStateChanged)
    Q_PROPERTY(int peerMetadataEpoch READ peerMetadataEpoch NOTIFY peerMetadataChanged)
    Q_PROPERTY(int peerAccountEpoch READ peerAccountEpoch NOTIFY peerAccountChanged)
    Q_PROPERTY(QString currentNick READ currentNick NOTIFY selectionChanged)
    Q_PROPERTY(bool selfAway READ selfAway NOTIFY selfAwayChanged)
    Q_PROPERTY(bool hasAwayPresence READ hasAwayPresence NOTIFY capabilitiesChanged)
    Q_PROPERTY(bool hasMemberStatus READ hasMemberStatus NOTIFY capabilitiesChanged)
    Q_PROPERTY(bool hasTyping READ hasTyping NOTIFY capabilitiesChanged)
    Q_PROPERTY(QStringList typingNicks READ typingNicks NOTIFY typingChanged)
    Q_PROPERTY(bool reopenDirectMessages READ reopenDirectMessages WRITE setReopenDirectMessages NOTIFY reopenDirectMessagesChanged)
    Q_PROPERTY(bool loadPeerAvatars READ loadPeerAvatars WRITE setLoadPeerAvatars NOTIFY loadPeerAvatarsChanged)
    Q_PROPERTY(bool openConversationsAtUnread READ openConversationsAtUnread WRITE setOpenConversationsAtUnread NOTIFY openConversationsAtUnreadChanged)
    Q_PROPERTY(IrcStatusConsole* console READ console CONSTANT)
    Q_PROPERTY(QAbstractItemModel* inbox READ inbox NOTIFY inboxChanged)
    Q_PROPERTY(int inboxCount READ inboxCount NOTIFY inboxChanged)

public:
    explicit IrcController(QObject *parent = nullptr);
    ~IrcController() override = default;

    void setTranscriptRoot(const QString &root);
    using ProfileAvatarUrlPersist =
        std::function<void(const QString &networkId, const QString &url)>;
    using ProfileAvatarUrlLookup = std::function<QString(const QString &networkId)>;
    void setProfileAvatarUrlCallbacks(ProfileAvatarUrlPersist persist,
                                      ProfileAvatarUrlLookup lookup);
    void setEphemeral(bool ephemeral);
    void loadStoredPreferences();
    IrcSession *addSession(const IrcSessionConfig& config,
                           IrcTransport *transport,
                           IrcReconnectTimer *reconnectTimer = nullptr,
                           IrcReconnectTimer *labelTimer = nullptr);
    bool discardSession(const QString &networkId);
    void forgetNetworkState(const QString &networkId);
    void setNetworkOrder(const QStringList &networkOrder);

    QAbstractItemModel *conversations();
    QAbstractItemModel *messages();
    QAbstractItemModel *members();
    QAbstractItemModel *channelList();
    QString selectedNetworkId() const;
    QString selectedTarget() const;
    QString selectedConversationId() const;
    QString focusedNetworkId() const;
    QString topic() const;
    bool isChannel() const;
    int peopleCount() const;
    QString connectionStatus() const;
    QString lastError() const;
    int conversationEpoch() const;
    int peerMetadataEpoch() const;
    int peerAccountEpoch() const;
    QString lastErrorForNetwork(const QString& networkId) const;
    Q_INVOKABLE QString lastErrorFor(const QString& networkId) const;
    Q_INVOKABLE QString connectionStatusFor(const QString& networkId) const;
    Q_INVOKABLE int unreadCountFor(const QString& networkId) const;
    Q_INVOKABLE bool mentionFor(const QString& networkId) const;
    QString currentNick() const;
    bool selfAway() const;

    bool hasAwayPresence() const;
    bool hasMemberStatus() const;
    bool hasTyping() const;
    QStringList typingNicks() const;
    bool reopenDirectMessages() const;
    void setReopenDirectMessages(bool enabled);
    bool loadPeerAvatars() const;
    void setLoadPeerAvatars(bool enabled);
    bool openConversationsAtUnread() const;
    void setOpenConversationsAtUnread(bool enabled);
    IrcStatusConsole *console();
    QAbstractItemModel *inbox();
    int inboxCount() const;
    Q_INVOKABLE void activateInboxItem(int row);
    Q_INVOKABLE void dismissInboxItem(int row);
    const IrcServerFeatures &serverFeatures(const QString &networkId) const;
    Q_INVOKABLE QString networkIconUrl(const QString &networkId) const;

    Q_INVOKABLE bool start(const QString& networkId);
    Q_INVOKABLE void selectConversation(const QString& networkId,
                                        const QString& target);
    Q_INVOKABLE void selectConversationById(const QString& conversationId);
    Q_INVOKABLE void openStatus(const QString& networkId);
    Q_INVOKABLE void openDirectMessage(const QString& nick);
    Q_INVOKABLE void revealConversation(const QString& networkId,
                                        const QString& target);
    Q_INVOKABLE void closeDirectMessage();
    Q_INVOKABLE bool sendMessage(const QString& text);
    Q_INVOKABLE bool nickIsTyping(const QString& nick) const;
    Q_INVOKABLE void notifyComposerText(const QString& text);
    Q_INVOKABLE void setWindowActive(bool active);
    void noteLocalActivity();
#ifdef OMAIRC_TEST
    void fireAutoawayIdleForTest();
    void fireAutoawayGraceForTest();
    int autoawayIdleIntervalMsForTest() const;
    bool autoawayIdleIsActiveForTest() const;
    int autoawayGraceIntervalMsForTest() const;
    bool autoawayGraceIsActiveForTest() const;
#endif
    Q_INVOKABLE void setChannelListPresented(bool presented);
    Q_INVOKABLE bool joinListedChannel(const QString& channel);
    void setChannelListIdleTimeoutMs(int milliseconds);
#ifdef OMAIRC_TEST
    void fireChannelListIdleTimeoutForTest(const QString& networkId);
#endif
    Q_INVOKABLE QVariantMap peerMetadata(const QString& networkId,
                                         const QString& nick) const;
    // Empty when the services account is unknown, logged out, or the same
    // as the nick under the network case mapping.
    Q_INVOKABLE QString peerAccount(const QString& networkId,
                                    const QString& nick) const;

    QStringList networkIds() const;
    IrcSession *session(const QString &networkId) const;
    bool sendToTarget(const QString &networkId,
                      const QString &target,
                      const QString &text);

    struct CliMessage {
        QString networkId;
        QString target;
        QString sender;
        QDateTime timestamp;
        QString message;
        QString kind;
        QString msgid;
        qint64 sequence = 0;
        bool mention = false;
    };

    struct CliMember {
        QString nick;
        QString label;
        bool away = false;
        QString status;
    };

    struct CliConversation {
        QString target;
        bool channel = false;
        QString topic;
        int unread = 0;
        bool mention = false;
    };

    struct CliReadQuery {
        enum class Mode { Last, Since, After } mode = Mode::Last;
        int last = 50;
        QDateTime sinceUtc;
        QDateTime afterUtc;
        QString afterMsgid;
        qint64 afterSequence = 0;
    };

    struct CliMessageSnapshot {
        QVector<CliMessage> lines;
        bool truncated = false;
    };

    std::variant<CliMessageSnapshot, QString> snapshotMessages(
        const QString &networkId,
        const QString &target,
        const CliReadQuery &query) const;
    std::variant<QVector<CliMember>, QString> snapshotMembers(
        const QString &networkId,
        const QString &target) const;
    std::variant<QVector<CliConversation>, QString> snapshotConversations(
        const QString &networkId) const;

signals:
    void selectionChanged();
    void statusChanged();
    void conversationStateChanged();
    void peerMetadataChanged();
    void peerAccountChanged();
    void selfAwayChanged();

    void capabilitiesChanged();
    void serverFeaturesChanged();
    void typingChanged();
    void reopenDirectMessagesChanged();
    void loadPeerAvatarsChanged();
    void openConversationsAtUnreadChanged();
    void errorOccurred(const QString &networkId,
                       IrcSession::ErrorKind kind,
                       const QString &message);
    void mentionArrived(const QString &author, const QString &body,
                       const QString &networkId, const QString &target,
                       const QString &msgid);
    void monitorArrived(const QString &author, const QString &body,
                        const QString &networkId, const QString &target);
    void inboxChanged();
    void channelListRequested();

private:
    void apply(const IrcEvent& event);
    void adoptReducerSelection();
    void publish(const IrcViewNotify& notify);
    void handleCapabilities(const QString& networkId,
                            IrcCapabilitySet capabilities);
    void echoLocal(IrcMessageKind kind, const QString& body);
    void handleMessage(const QString& networkId, const IrcMessage& message);
    void handleHistoryBatch(const QString& networkId, const IrcHistoryBatch& batch);
    void notePlaybackClock(const QString& networkId, const IrcMessage& message);
    void noteKeptReplay();
    // Packs the capability, MOTD-seen, and open-direct lookups the
    // coordinator takes as arguments at the call.
    void requestZncPlayback(IrcSession *session);
    void requestZncChannelPlayback(IrcSession *session, const QString& channel);
    void reloadModels();
    IrcCommandOutcome dispatch(const IrcCommand& command,
                               IrcComposerSurface surface);
    QString queryNetworkId(IrcComposerSurface surface) const;
    IrcSession *sessionFor(IrcComposerSurface surface) const;
    IrcCommandOutcome sendSelectedMessage(const QString& body);
    void hydrateMutes(const QString& networkId);
    bool persistableDirectTarget(const QString& networkId,
                                 const QString& target) const;
    void rememberOpenDirect(const QString& networkId, const QString& target);
    void forgetOpenDirect(const QString& networkId, const QString& target);
    void restoreOpenDirects(const QString& networkId);
    void noteOpenDirectsMotd(const QString& networkId);
    bool applyMute(const QString& networkId,
                   const QString& target,
                   bool muted);
    void syncHighlightWords(const QString& networkId);
    IrcCommandOutcome dispatchList(const IrcCommand& command,
                                   IrcComposerSurface surface);
    void noteNickDelivery(const QString& networkId, const QString& target);
    void noteManualAway(const QString& networkId);
    void noteAwayCleared(const QString& networkId);
    void handleStatusEntry(const IrcStatusEntry& entry);
    void onRequestLabelFinished(const QString& networkId, const QString& requestLabel);
    void forgetChannelList(const QString& networkId);
    bool beginChannelListLoad(IrcSession *session,
                              const QString& networkId,
                              const QString& mask);
    void applyListRow(const QString& networkId, IrcChannelListRow row);
    void finishChannelList(const QString& networkId);
    bool failChannelList(const QString& networkId, const QString& text);
    bool failChannelListFromNumeric(const QString& networkId,
                                    const IrcMessage& message);
    QString profileAvatarUrlForNetwork(const QString& networkId) const;
    void persistProfileAvatarUrl(const QString& networkId, const QString& url);
    void applyProfileAvatarOnConnect(IrcSession *session);

    void unawayAfterChat(IrcSession *session);
    void echoIfPresent(IrcSession *session,
                       const QString& target,
                       const QString& body,
                       IrcCommandDispatcher::QuietWire wire);
    IrcCommandOutcome clearSurface(IrcComposerSurface surface);
    bool report(IrcCommandOutcome outcome, const IrcCommand& command);
    bool selectedIsCloseableDirect() const;
    void dropConversationAndReselect(const IrcConversationKey& key,
                                     bool forgetDirect);
    void dropSelectedDirectAndReselect();
    bool dismissChannel(const QString& networkId, const QString& channel);
    void clearConversationSelection();
    void openJoinedChannel(const QString& networkId, const QString& channel);
    IrcSession *selectedSession() const;
    QString identityNetworkId() const;
    void notifySelfAwayIfChanged(const QString& previousId, bool previousAway);
    void updateStatus(IrcSession *session);
    // connectionStatus() follows focusedNetworkId(), so focus changes must
    // notify even when the newly focused network has no live session.
    void notifyFocusedConnectionStatus();
    void setLastError(const QString& networkId, const QString& message);
    QString errorNetworkId(IrcComposerSurface surface) const;
    void armTypingRefresh();
    void appendInbox(IrcInboxItem item);
    void syncInbox();

    IrcSessionManager m_sessions;
    IrcStatusConsole m_console;
    IrcConversationLog m_transcripts;
    IrcEventReducer m_reducer;
    IrcIgnoreStore m_ignores;
    IrcMonitorStore m_monitors;
    IrcMuteStore m_mutes;
    IrcOpenDirectStore m_openDirects;
    IrcPlaybackTimeStore m_playbackTimes;
    IrcPlaybackCoordinator m_playback;
    IrcReplyRouter m_replies;
    IrcMonitorCoordinator m_monitorCoord;
    IrcCommandDispatcher m_commands;
    IrcAutoawayRuntime m_autoawayRuntime;
    IrcHighlightStore m_highlights;
    IrcInbox m_inbox;
    IrcInboxModel m_inboxModel;
    ConversationListModel m_conversations;
    MessageListModel m_messages;
    MemberListModel m_members;
    ChannelListModel m_channelList;
    IrcChannelListRequest m_channelLists;
    bool m_channelListPresented = false;
    QHash<QString, QString> m_currentNicks;
    QHash<QString, QString> m_lastErrors;
    QHash<QString, IrcCapabilitySet> m_capabilities;
    QSet<QString> m_unawaySent;
    QSet<QString> m_openDirectsMotdSeen;
    std::optional<IrcConversationKey> m_selected;
    QString m_selectedTarget;
    QStringList m_networkOrder;
    QString m_connectionStatus = QStringLiteral("Offline");
    int m_conversationEpoch = 0;
    int m_peerMetadataEpoch = 0;
    int m_peerAccountEpoch = 0;
    bool m_ephemeral = false;
    bool m_reopenDirectMessages = true;
    bool m_loadPeerAvatars = true;
    bool m_openConversationsAtUnread = true;
    QTimer m_typingRefresh;
    QString m_composerDraft;
    QString m_typingTarget;
    QSet<QString> m_appliedProfileAvatars;
    ProfileAvatarUrlPersist m_profileAvatarUrlPersist;
    ProfileAvatarUrlLookup m_profileAvatarUrlLookup;
};
