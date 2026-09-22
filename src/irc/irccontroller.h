#pragma once

#include "channellistmodel.h"
#include "conversationlistmodel.h"
#include "ircautoaway.h"
#include "ircconversationlog.h"
#include "irceventreducer.h"
#include "irchighlight.h"
#include "ircignore.h"
#include "ircinbox.h"
#include "ircinboxmodel.h"
#include "ircmonitor.h"
#include "ircmute.h"
#include "ircnetworkprofile.h"
#include "ircopendirect.h"
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
#include <map>
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
    ~IrcController() override;

    void setTranscriptRoot(const QString &root);
    using ProfileAvatarUrlPersist =
        std::function<void(const QString &networkId, const QString &url)>;
    using ProfileAvatarUrlLookup = std::function<QString(const QString &networkId)>;
    void setProfileAvatarUrlCallbacks(ProfileAvatarUrlPersist persist,
                                      ProfileAvatarUrlLookup lookup);
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
    Q_INVOKABLE QVariantMap peerMetadata(const QString& networkId,
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
    enum class QuietWire { Privmsg, Notice };
    enum class QuietTarget { Nick, Any };
    struct QuietSend {
        QuietWire wire;
        QuietTarget target;
    };
    static std::optional<QuietSend> quietSendFor(IrcCommand::Verb verb);

    struct IrcWhoisWatchKey
    {
        QString networkId;
        QString normalizedNick;

        friend bool operator<(const IrcWhoisWatchKey& left,
                              const IrcWhoisWatchKey& right)
        {
            if (left.networkId != right.networkId)
                return left.networkId < right.networkId;
            return left.normalizedNick < right.normalizedNick;
        }
    };
    struct IrcWhoisStatusOnly {};
    using IrcWhoisDestination = std::variant<IrcWhoisStatusOnly, IrcConversationKey>;
    struct IrcWhoisWatch
    {
        IrcWhoisDestination destination;
        bool failedIsAmbiguous = false;
        bool metadataEmitted = false;
    };
    struct IrcLabeledWatchKey
    {
        QString networkId;
        QString requestLabel;

        friend bool operator<(const IrcLabeledWatchKey& left,
                              const IrcLabeledWatchKey& right)
        {
            if (left.networkId != right.networkId)
                return left.networkId < right.networkId;
            return left.requestLabel < right.requestLabel;
        }
    };
    enum class IrcLabeledWatchKind { Whois, Ctcp };
    struct IrcLabeledWatch
    {
        IrcLabeledWatchKind kind = IrcLabeledWatchKind::Whois;
        IrcWhoisDestination destination;
        bool metadataEmitted = false;
    };

    void apply(const IrcEvent& event);
    void adoptReducerSelection();
    void publish(const IrcViewNotify& notify);
    void handleCapabilities(const QString& networkId,
                            IrcCapabilitySet capabilities);
    void echoLocal(IrcMessageKind kind, const QString& body);
    void handleMessage(const QString& networkId, const IrcMessage& message);
    void handleHistoryBatch(const QString& networkId, const IrcHistoryBatch& batch);
    void reloadModels();
    IrcCommandOutcome dispatch(const IrcCommand& command,
                               IrcComposerSurface surface);
    QString queryNetworkId(IrcComposerSurface surface) const;
    IrcSession *sessionFor(IrcComposerSurface surface) const;
    IrcCommandOutcome sendSelectedMessage(const QString& body);
    IrcCommandOutcome setSelectedTopic(const QString& topic);
    IrcCommandOutcome dispatchQuery(const IrcCommand& command,
                                    IrcComposerSurface surface);
    IrcCommandOutcome dispatchQuietSend(const IrcCommand& command,
                                        IrcComposerSurface surface);
    IrcCommandOutcome dispatchMode(const IrcCommand& command,
                                   IrcComposerSurface surface);
    IrcCommandOutcome dispatchWhois(const IrcCommand& command,
                                    IrcComposerSurface surface);
    IrcCommandOutcome dispatchCtcp(const IrcCommand& command,
                                   IrcComposerSurface surface);
    IrcCommandOutcome dispatchIgnore(const IrcCommand& command,
                                     IrcComposerSurface surface);
    IrcCommandOutcome dispatchMonitor(const IrcCommand& command,
                                      IrcComposerSurface surface);
    void subscribeMonitors(const QString& networkId);
    void forgetMonitorState(const QString& networkId);
    QString monitorDisplayNick(const QString& networkId,
                               const QString& nick) const;
    bool monitorNotifyMuted(const QString& networkId,
                            const QString& nick) const;
    void handleMonitorPresence(const QString& networkId,
                               const IrcMessage& message,
                               bool online);
    void handleMonitorListFull(const QString& networkId,
                               const IrcMessage& message);
    IrcCommandOutcome dispatchMute(const IrcCommand& command,
                                     IrcComposerSurface surface);
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
    IrcCommandOutcome dispatchHighlight(const IrcCommand& command,
                                        IrcComposerSurface surface);
    IrcCommandOutcome dispatchAutoaway(const IrcCommand& command,
                                       IrcComposerSurface surface);
    IrcCommandOutcome dispatchPref(const IrcCommand& command,
                                   IrcComposerSurface surface);
    IrcCommandOutcome echoAutoawayFeedback(IrcComposerSurface surface,
                                           const QString& text);
    IrcCommandOutcome echoPrefFeedback(IrcComposerSurface surface,
                                       const QString& text);
    IrcCommandOutcome echoAutoawayUsage(IrcComposerSurface surface);
    void saveAutoaway() const;
    void armAutoawayIdle();
    void stopAutoawayTimers();
    void onAutoawayIdle();
    void onAutoawayGrace();
    void tripAutoaway();
    void clearAutoAwayNetworks();
    QString autoawayReason() const;
    bool markSessionAutoAway(IrcSession *session);
    void noteManualAway(const QString& networkId);
    void noteAwayCleared(const QString& networkId);
    void refreshAutoAwayReason();
    void syncHighlightWords(const QString& networkId);
    IrcCommandOutcome dispatchChannelModeWrapper(const IrcCommand& command,
                                                IrcComposerSurface surface);
    IrcCommandOutcome dispatchServiceMsg(const IrcCommand& command,
                                         IrcComposerSurface surface);
    IrcCommandOutcome dispatchRaw(const IrcCommand& command,
                                  IrcComposerSurface surface);
    IrcCommandOutcome dispatchHelp(IrcComposerSurface surface);
    IrcCommandOutcome dispatchList(const IrcCommand& command,
                                   IrcComposerSurface surface);
    IrcCommandOutcome dispatchStatus(const IrcCommand& command,
                                     IrcComposerSurface surface);
    IrcCommandOutcome dispatchAvatar(const IrcCommand& command,
                                     IrcComposerSurface surface);
    std::optional<IrcWhoisWatchKey> whoisWatchKey(const QString& networkId,
                                                  const QString& nick) const;
    bool sendWhois(IrcSession& session,
                   const QString& nick,
                   IrcWhoisDestination destination);
    void noteNickDelivery(const QString& networkId, const QString& target);
    void handleStatusEntry(const IrcStatusEntry& entry);
    void routeWhoisLine(const QString& networkId, const IrcWhoisLine& line);
    void routeLabeledWhois(const QString& networkId,
                           const QString& requestLabel,
                           const IrcWhoisLine& line);
    void routeLabeledCtcp(const QString& networkId,
                          const QString& requestLabel,
                          const IrcCtcpReplyLine& line,
                          const QString& text);
    void routeLabeledStandardReply(const IrcStatusEntry& entry);
    void onRequestLabelFinished(const QString& networkId, const QString& requestLabel);
    void forgetLabeledWatches(const QString& networkId, IrcLabeledWatchKind kind);
    QStringList whoisMetadataLines(const QString& networkId,
                                   const QString& nick) const;
    void forgetWhoisWatches(const QString& networkId);
    void forgetChannelList(const QString& networkId);
    bool beginChannelListLoad(IrcSession *session,
                              const QString& networkId,
                              const QString& mask);
    void applyListRow(const QString& networkId, IrcChannelListRow row);
    void finishChannelList(const QString& networkId);
    bool failChannelList(const QString& networkId, const QString& text);
    void armChannelListIdle(const QString& networkId);
    void stopChannelListIdle(const QString& networkId);
    bool failChannelListFromNumeric(const QString& networkId,
                                    const IrcMessage& message);
    bool sameListMask(const QString& left, const QString& right) const;
    struct IrcCtcpWatchKey
    {
        QString networkId;
        QString normalizedNick;
        QString command;

        friend bool operator<(const IrcCtcpWatchKey& left,
                              const IrcCtcpWatchKey& right)
        {
            if (left.networkId != right.networkId)
                return left.networkId < right.networkId;
            if (left.normalizedNick != right.normalizedNick)
                return left.normalizedNick < right.normalizedNick;
            return left.command < right.command;
        }
    };
    using IrcCtcpDestination = IrcWhoisDestination;
    struct IrcCtcpWatch
    {
        IrcCtcpDestination destination;
    };
    std::optional<IrcCtcpWatchKey> ctcpWatchKey(const QString& networkId,
                                                const QString& nick,
                                                const QString& command) const;
    QString ctcpQueryName(IrcCommand::Verb verb) const;
    bool sendCtcpQuery(IrcSession& session,
                       const QString& nick,
                       const QString& command,
                       const QString& argument,
                       IrcCtcpDestination destination);
    void routeCtcpReply(const QString& networkId,
                        const IrcCtcpReplyLine& line,
                        const QString& text);
    void forgetCtcpWatches(const QString& networkId);
    struct IrcOwnMetadataWatch
    {
        IrcWhoisDestination destination;
        enum class Kind { Set, Clear };
        Kind kind = Kind::Set;
        QString value;
    };
    IrcCommandOutcome dispatchOwnMetadataClear(IrcSession *session,
                                               const QString& metadataKey);
    IrcCommandOutcome dispatchOwnMetadataSet(IrcSession *session,
                                             const QString& metadataKey,
                                             const QString& value);
    IrcCommandOutcome echoMetadataCommandFeedback(IrcComposerSurface surface,
                                                 const QString& networkId,
                                                 const QString& text);
    void armOwnMetadataWatch(const QString& networkId,
                             const QString& metadataKey,
                             IrcOwnMetadataWatch::Kind kind,
                             const QString& value);
    void forgetOwnMetadataWatches(const QString& networkId);
    void echoOwnMetadataOutcome(const QString& networkId,
                                const IrcWhoisDestination& destination,
                                const QString& text);
    void routeOwnMetadataReply(const QString& networkId,
                               const QString& nick,
                               const QString& key,
                               const QString& value);
    void routeOwnMetadataError(const IrcStatusEntry& entry);
    void routeOwnMetadataFail(const QString& networkId,
                              const IrcMessage& message);
    QString profileAvatarUrlForNetwork(const QString& networkId) const;
    void persistProfileAvatarUrl(const QString& networkId, const QString& url);
    void applyProfileAvatarOnConnect(IrcSession *session);
    void echoIfPresent(IrcSession *session,
                       const QString& target,
                       const QString& body,
                       QuietWire wire);
    void unawayAfterChat(IrcSession *session);
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
    bool eventFilter(QObject *watched, QEvent *event) override;

    IrcSessionManager m_sessions;
    IrcStatusConsole m_console;
    IrcConversationLog m_transcripts;
    IrcEventReducer m_reducer;
    IrcIgnoreStore m_ignores;
    IrcMonitorStore m_monitors;
    IrcMuteStore m_mutes;
    IrcOpenDirectStore m_openDirects;
    IrcHighlightStore m_highlights;
    IrcInbox m_inbox;
    IrcInboxModel m_inboxModel;
    ConversationListModel m_conversations;
    MessageListModel m_messages;
    MemberListModel m_members;
    ChannelListModel m_channelList;
    struct ChannelListCache {
        QString mask;
        QVector<IrcChannelListRow> rows;
        bool complete = false;
        bool loading = false;
        QString error;
        std::optional<QString> pendingMask;
        // After idle timeout, a 323 from the unanswered LIST may still arrive
        // after a retry's 321/322. Drain that one end; do not finish the retry.
        bool drainTimedOutEnd = false;
    };
    QHash<QString, ChannelListCache> m_channelLists;
    QHash<QString, QTimer *> m_channelListIdleTimers;
    int m_channelListIdleTimeoutMs = 30000;
    bool m_channelListPresented = false;
    QHash<QString, QString> m_currentNicks;
    QHash<QString, QString> m_lastErrors;
    QHash<QString, IrcCapabilitySet> m_capabilities;
    QSet<QString> m_unawaySent;
    QSet<QString> m_openDirectsMotdSeen;
    QSet<QString> m_monitorSubscribed;
    enum class MonitorPresence { Unknown, Online, Offline };
    QHash<QString, QHash<QString, MonitorPresence>> m_monitorPresence;
    std::optional<IrcConversationKey> m_selected;
    QString m_selectedTarget;
    QStringList m_networkOrder;
    QString m_connectionStatus = QStringLiteral("Offline");
    int m_conversationEpoch = 0;
    int m_peerMetadataEpoch = 0;
    bool m_reopenDirectMessages = true;
    bool m_loadPeerAvatars = true;
    bool m_openConversationsAtUnread = true;
    IrcAutoawayConfig m_autoaway;
    QTimer m_autoawayIdle;
    QTimer m_autoawayGrace;
    bool m_autoawayGraceArmed = false;
    bool m_autoawayTripped = false;
    QSet<QString> m_autoAwayNetworks;
    QSet<QString> m_manualAwayNetworks;
    QTimer m_typingRefresh;
    QString m_composerDraft;
    QString m_typingTarget;
    std::map<IrcWhoisWatchKey, IrcWhoisWatch> m_whoisWatches;
    std::map<IrcCtcpWatchKey, IrcCtcpWatch> m_ctcpWatches;
    std::map<IrcLabeledWatchKey, IrcLabeledWatch> m_labeledWatches;
    QHash<QString, QHash<QString, IrcOwnMetadataWatch>> m_ownMetadataWatches;
    QSet<QString> m_appliedProfileAvatars;
    ProfileAvatarUrlPersist m_profileAvatarUrlPersist;
    ProfileAvatarUrlLookup m_profileAvatarUrlLookup;
    std::set<IrcConversationKey> m_cancelledPendingJoins;
};
