#pragma once

#include "ircevent.h"
#include "ircpresence.h"
#include "ircserverfeatures.h"
#include "irctyping.h"

#include <QDateTime>
#include <QStringList>

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

struct IrcReducedMessage
{
    QString author;
    QString body;
    QDateTime timestamp;
    IrcMessageKind kind = IrcMessageKind::Message;
    bool collapsible = false;
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

    bool isAway() const noexcept;
};

struct IrcChannelState
{
    std::map<QString, IrcMemberState> members;
    QString topic;
    bool joined = false;
    bool namesSyncing = false;
    QDateTime namesSyncStarted;
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
    int trimmed = 0;

    bool isChannel() const noexcept;
    const IrcChannelState *channel() const noexcept;
    IrcChannelState *channel() noexcept;
    int peopleCount() const noexcept;
};

struct IrcMentionArrival
{
    QString author;
    QString body;
};

class IrcEventReducer
{
public:
    using Store = std::map<IrcConversationKey, IrcConversationState>;

    void setServerFeatures(const QString& networkId,
                           const IrcServerFeatures& features);
    const IrcServerFeatures& serverFeatures(const QString& networkId) const;

    IrcConversationKey conversationKey(const QString& networkId,
                                       const QString& target) const;
    void markSelected(const IrcConversationKey& key);
    void clearSelection();
    std::optional<IrcConversationKey> selected() const;

    static constexpr qint64 kStaleNamesSyncMs = 30000;
    static constexpr int kMaxMessages = 2000;

    void apply(const IrcEvent& event);
    bool releaseStaleNamesSync(const std::optional<IrcConversationKey>& key,
                               const QDateTime& now);
    bool dropDirectMessage(const IrcConversationKey& key);
    void forgetNetwork(const QString& networkId);
    void clearMessages(const IrcConversationKey& key);
    IrcConversationState& ensureConversation(const IrcConversationKey& key,
                                             const QString& displayTarget);

    const Store& conversations() const noexcept;
    const IrcConversationState *find(const IrcConversationKey& key) const noexcept;

    std::optional<IrcMemberView> memberView(const IrcConversationKey& key,
                                            const QString& normalizedNick) const;

    QStringList typingNicks(const IrcConversationKey& key,
                            const QDateTime& now) const;
    bool directPeerIsTyping(const IrcConversationKey& key,
                            const QDateTime& now) const;
    void clearTypingFacts(const QString& networkId);

    void clearPresenceFacts(const QString& networkId, bool away, bool status);

    bool selfAway(const QString& networkId) const noexcept;

    std::optional<IrcMentionArrival> takeMentionArrival();

private:
    IrcConversationState *findMutable(const IrcConversationKey& key) noexcept;
    QString normalize(const QString& networkId, const QString& identifier) const;
    bool equals(const QString& networkId,
                const QString& left,
                const QString& right) const;
    bool isSelf(const QString& networkId, const QString& nick) const;
    bool isMention(const QString& networkId, const QString& body) const;
    void appendChat(const IrcConversationKey& key,
                    const QString& displayTarget,
                    const QString& author,
                    const QString& body,
                    const QDateTime& timestamp,
                    IrcMessageKind kind);
    void appendEvent(IrcConversationState& conversation,
                     const QString& body,
                     bool collapsible = false);
    void appendWhois(IrcConversationState& conversation, const QString& body);
    void capMessages(IrcConversationState& conversation);

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
    void reduce(const IrcMemberStatusEvent& event);
    void reduce(const IrcTypingEvent& event);
    void reduce(const IrcWhoisTranscriptEvent& event);

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
    std::map<QString, IrcNetworkPresence> m_presence;
    std::set<QString> m_selfAway;
    std::optional<IrcConversationKey> m_selected;
    std::optional<IrcMentionArrival> m_mentionArrival;
};
