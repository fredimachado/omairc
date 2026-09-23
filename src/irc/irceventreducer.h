#pragma once

#include "ircinbox.h"
#include "ircevent.h"
#include "ircpresence.h"
#include "ircserverfeatures.h"
#include "irctyping.h"

#include <QDateTime>
#include <QStringList>
#include <QVector>

#include <map>
#include <optional>
#include <set>
#include <variant>
#include <vector>

enum class IrcMessageKind
{
    Message,
    Notice,
    Action,
    Event,
    Error,
    Whois,
};

enum class IrcOrigin { Live, Replay };

struct IrcReducedMessage
{
    QString author;
    QString body;
    QDateTime timestamp;
    IrcMessageKind kind = IrcMessageKind::Message;
    bool collapsible = false;
    IrcOrigin origin = IrcOrigin::Live;
    IrcMsgId msgid{};
    qint64 sequence = 0;
};

struct IrcMemberState
{
    QString displayNick;
    IrcPrefixSet ranks;
};

struct IrcMemberView
{
    QString nick;
    QString label;
    IrcPrefixSet ranks;
    std::optional<IrcAway> away;
    QString status;
    QString avatar;
    bool bot = false;
    QString displayName;
    QString pronouns;
    QString homepage;
    QString color;
    // Empty when unknown, logged out, or the same as the nick under the
    // network case mapping. Callers must not compare the raw account again.
    QString account;

    bool isAway() const noexcept;
};

// What we know about a nick's presence on a network from the same facts that
// feed member rows: membership in a joined channel plus the per-network away
// facts. Unknown means no shared channel proves the nick is online, so a direct
// message row must not paint it as available.
enum class IrcPeerPresence
{
    Unknown,
    Online,
    Away,
};

// One channel member in panel order. `priority` is the index of the member's
// highest rank in the server's PREFIX order, so a lower value is a higher
// privilege and members without a rank come last.
struct IrcOrderedMember
{
    QString nick;
    int priority = 0;

    friend bool operator==(const IrcOrderedMember& left,
                           const IrcOrderedMember& right)
    {
        return left.priority == right.priority && left.nick == right.nick;
    }

    friend bool operator<(const IrcOrderedMember& left,
                          const IrcOrderedMember& right)
    {
        if (left.priority != right.priority)
            return left.priority < right.priority;
        return left.nick < right.nick;
    }
};

struct IrcTranscriptAnchor
{
    qint64 sequence = 0;
};

struct IrcChannelState
{
    std::map<QString, IrcMemberState> members;
    QString topic;
    bool joined = false;
    bool namesSyncing = false;
    QDateTime namesSyncStarted;
    std::optional<IrcTranscriptAnchor> historyAnchor;
};

struct IrcDirectMessageState
{
};

struct IrcConversationState
{
    IrcConversationKey key;
    QString target;
    std::vector<IrcReducedMessage> messages;
    std::variant<IrcChannelState, IrcDirectMessageState> detail;
    std::map<QString, IrcTypingHint> typing;
    int unread = 0;
    int mentions = 0;
    std::optional<qint64> unreadMark;
    bool muted = false;
    int trimmed = 0;
    std::set<IrcMsgId> messageIds;
    int spliceEpoch = 0;
    qint64 nextSequence = 0;

    bool isChannel() const noexcept;
    const IrcChannelState *channel() const noexcept;
    IrcChannelState *channel() noexcept;
    int peopleCount() const noexcept;
};

struct IrcMentionArrival
{
    QString author;
    QString body;
    QString networkId;
    QString target;
    IrcMsgId msgid{};
};

struct IrcInboxArrival
{
    IrcInboxKind kind;
    QString actor;
    QString body;
    QString networkId;
    QString target;
    IrcMsgId msgid{};
};

class IrcConversationLog;

// A replay line the reducer actually kept: inserted, or already present so
// author/body/kind/time or msgid dedup skipped it. `serverTime` is the raw
// time tag, never the translator's wall-clock fallback.
struct IrcKeptReplay
{
    QString networkId;
    QString target;
    QDateTime serverTime;
};

// A bouncer query whose splice kept a line from the user, including a nick
// they have since changed. The controller remembers that direct so the next
// cold start can restore it before PLAY.
struct IrcRememberedQuery
{
    QString networkId;
    QString target;
};

enum class IrcConversationCause {
    UserOpen,
    ChannelState,
    InboundOther,
    InboundSelf,
    QuietSend,
    Restore,
};

inline bool ircConversationCauseInserts(IrcConversationCause cause,
                                        bool targetIsChannel,
                                        bool targetLooksLikeService) noexcept
{
    switch (cause) {
    case IrcConversationCause::UserOpen:
        return !targetIsChannel;
    case IrcConversationCause::ChannelState:
        return targetIsChannel;
    case IrcConversationCause::InboundOther:
        return targetIsChannel || !targetLooksLikeService;
    case IrcConversationCause::Restore:
        return !targetIsChannel && !targetLooksLikeService;
    case IrcConversationCause::InboundSelf:
    case IrcConversationCause::QuietSend:
        return false;
    }
    return false;
}

bool ircTargetLooksLikeService(const QString& target,
                               const IrcServerFeatures& features);

class IrcEventReducer
{
public:
    using Store = std::map<IrcConversationKey, IrcConversationState>;

    void setServerFeatures(const QString& networkId,
                           const IrcServerFeatures& features);
    const IrcServerFeatures& serverFeatures(const QString& networkId) const;
    void setHighlightWords(const QString& networkId, const QStringList& words);

    IrcConversationKey conversationKey(const QString& networkId,
                                       const QString& target) const;
    void markSelected(const IrcConversationKey& key);
    bool markRead(const IrcConversationKey& key);
    void setWindowActive(bool active);
    void clearSelection();
    std::optional<IrcConversationKey> selected() const;

    static constexpr qint64 kStaleNamesSyncMs = 30000;
    static constexpr int kMaxMessages = 2000;

    void setConversationLog(IrcConversationLog *log);

    void apply(const IrcEvent& event);
    std::vector<IrcKeptReplay> takeKeptReplay();
    std::vector<IrcRememberedQuery> takeRememberedQueries();
    // MOTD-end restore has finished for this connection. Splice held query
    // batches into directs that now exist. A self-only batch whose query
    // still does not exist is a finished drop and does not move the clock.
    bool releasePendingQueryPlayback(const QString& networkId);
    // A znc.in/playback batch for this channel was spliced, or deduped onto
    // a row already in the transcript, on the current connection.
    bool playbackBatchKept(const QString& networkId, const QString& target) const;
    bool releaseStaleNamesSync(const std::optional<IrcConversationKey>& key,
                               const QDateTime& now);
    bool dropDirectMessage(const IrcConversationKey& key);
    bool dropChannel(const IrcConversationKey& key);
    void forgetNetwork(const QString& networkId);
    void clearMessages(const IrcConversationKey& key);
    void setMuted(const IrcConversationKey& key, bool muted);
    IrcConversationState *ensureConversation(const IrcConversationKey& key,
                                             const QString& displayTarget,
                                             IrcConversationCause cause);

    const Store& conversations() const noexcept;
    const IrcConversationState *find(const IrcConversationKey& key) const noexcept;

    std::optional<IrcMemberView> memberView(const IrcConversationKey& key,
                                            const QString& normalizedNick) const;
    IrcNickPresence nickPresence(const QString& networkId,
                                 const QString& nick) const;
    // Display label: empty unless the stored account adds information.
    QString displayAccount(const QString& networkId, const QString& nick) const;
    IrcPeerPresence peerPresence(const QString& networkId,
                                 const QString& normalizedNick) const;
    QVector<IrcOrderedMember> orderedMembers(const IrcConversationKey& key) const;
    bool mentions(const QString& networkId, const QString& body) const;
    // Transcript wash: other-authored Message/Action whose body is a nick
    // or /highlight hit. Not classifyChatLine, which also tags every DM.
    bool isTranscriptHighlight(const QString& networkId,
                               const QString& author,
                               IrcMessageKind kind,
                               const QString& body) const;

    QStringList typingNicks(const IrcConversationKey& key,
                            const QDateTime& now) const;
    bool directPeerIsTyping(const IrcConversationKey& key,
                            const QDateTime& now) const;
    void clearTypingFacts(const QString& networkId);

    void clearPresenceFacts(const QString& networkId, bool away, bool metadata);

    bool selfAway(const QString& networkId) const noexcept;

    std::optional<IrcMentionArrival> takeMentionArrival();
    std::optional<IrcInboxArrival> takeInboxArrival();

private:
    IrcConversationState *findMutable(const IrcConversationKey& key) noexcept;
    QString normalize(const QString& networkId, const QString& identifier) const;
    bool equals(const QString& networkId,
                const QString& left,
                const QString& right) const;
    bool isSelf(const QString& networkId, const QString& nick) const;
    bool isMention(const QString& networkId, const QString& body) const;
    bool isNickMention(const QString& networkId, const QString& body) const;
    bool isHighlightHit(const QString& networkId, const QString& body) const;
    void appendChat(const IrcConversationKey& key,
                    const QString& displayTarget,
                    const QString& author,
                    const QString& body,
                    const QDateTime& timestamp,
                    IrcMessageKind kind,
                    const IrcMsgId& msgid);
    void noteChatArrival(IrcConversationState& conversation,
                         const IrcConversationKey& key,
                         const QString& author,
                         const QString& body,
                         IrcMessageKind kind,
                         const IrcMsgId& msgid,
                         qint64 sequence,
                         IrcOrigin origin,
                         const IrcHistoryEvent *history = nullptr);
    void appendEvent(IrcConversationState& conversation,
                     const QString& body,
                     bool collapsible = false);
    void appendWhois(IrcConversationState& conversation, const QString& body);
    void appendError(IrcConversationState& conversation, const QString& body);
    void hydrateFromLog(IrcConversationState& conversation);
    void persistMessage(const IrcConversationState& conversation,
                        const IrcReducedMessage& message);
    void capMessages(IrcConversationState& conversation);
    std::optional<std::size_t> peekSpliceIndex(
        const IrcConversationState& conversation) const;
    std::optional<std::size_t> takeSpliceIndex(IrcConversationState& conversation);
    enum class HistoryAnchorUse { Consume, Keep };
    bool bouncerQueryOwnLine(const IrcHistoryEvent& event,
                             const QString& author) const;
    bool replayFromPeer(const IrcHistoryEvent& event) const;
    bool absorbPendingQueryPlayback(const IrcHistoryEvent& event);
    void spliceHistory(IrcConversationState& conversation,
                       const IrcHistoryEvent& event,
                       HistoryAnchorUse anchorUse);
    void holdPendingPlayback(const IrcHistoryEvent& event);
    void dropKeptPlayback(const QString& networkId);
    void releasePendingPlayback(IrcConversationState& conversation);
    void dropPendingPlayback(const QString& networkId);

    void reduce(const IrcWelcomeEvent& event);
    void reduce(const IrcMessageEvent& event);
    void reduce(const IrcNoticeEvent& event);
    void reduce(const IrcActionEvent& event);
    void reduce(const IrcJoinEvent& event);
    void reduce(const IrcPartEvent& event);
    void reduce(const IrcQuitEvent& event);
    void reduce(const IrcNickEvent& event);
    void reduce(const IrcKickEvent& event);
    void reduce(const IrcTopicEvent& event);
    void reduce(const IrcNamesEvent& event);
    void reduce(const IrcModeEvent& event);
    void reduce(const IrcAwayEvent& event);
    void reduce(const IrcSelfAwayEvent& event);
    void reduce(const IrcMemberMetadataEvent& event);
    void reduce(const IrcAccountEvent& event);
    void reduce(const IrcTypingEvent& event);
    void reduce(const IrcHistoryEvent& event);
    void reduce(const IrcWhoisTranscriptEvent& event);
    void reduce(const IrcChannelErrorEvent& event);

    void clearTyping(IrcConversationState& conversation,
                     const QString& normalizedNick);
    void clearTypingEverywhere(const QString& networkId,
                               const QString& normalizedNick);
    void rekeyTyping(const QString& networkId,
                     const QString& oldNormalized,
                     const QString& newNormalized,
                     const QString& newDisplay);
    void pruneExpiredTyping(IrcConversationState& conversation,
                            const QDateTime& now);

    void forgetUnseen(const QString& networkId, const QStringList& normalizedNicks);
    bool isVisible(const QString& networkId, const QString& normalizedNick) const;

    Store m_conversations;
    std::map<QString, IrcServerFeatures> m_features;
    std::map<QString, QString> m_currentNicks;
    std::map<QString, QStringList> m_highlightWords;
    std::map<QString, IrcNetworkPresence> m_presence;
    std::set<QString> m_selfAway;
    std::optional<IrcConversationKey> m_selected;
    bool m_windowActive = true;
    std::optional<IrcMentionArrival> m_mentionArrival;
    std::optional<IrcInboxArrival> m_inboxArrival;
    std::set<IrcConversationKey> m_mutedKeys;
    IrcConversationLog *m_log = nullptr;
    std::map<IrcConversationKey, IrcHistoryEvent> m_pendingPlayback;
    std::set<IrcConversationKey> m_keptPlaybackChannels;
    std::vector<IrcKeptReplay> m_keptReplay;
    std::vector<IrcRememberedQuery> m_rememberedQueries;
    // Set on welcome, cleared when open-direct restore finishes. A missing
    // query is not a finished drop while this connection is still waiting.
    std::set<QString> m_queryRestorePending;
};
