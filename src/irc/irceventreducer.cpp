#include "irceventreducer.h"

#include "ircconversationlog.h"
#include "ircservicenick.h"
#include "ircwiretext.h"

#include <QByteArray>

#include <algorithm>
#include <cstddef>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

namespace
{
std::string utf8(const QString& value)
{
    const QByteArray encoded = value.toUtf8();
    return std::string(encoded.constData(), static_cast<std::size_t>(encoded.size()));
}

bool isIdentifierCharacter(QChar character)
{
    return character.isLetterOrNumber()
        || QStringLiteral("-_[]\\`^{}|~").contains(character);
}

bool containsWord(const QString& normalizedBody, const QString& normalizedWord)
{
    if (normalizedWord.isEmpty())
        return false;
    qsizetype position = normalizedBody.indexOf(normalizedWord);
    while (position >= 0) {
        const qsizetype end = position + normalizedWord.size();
        const bool leftBoundary = position == 0
            || !isIdentifierCharacter(normalizedBody.at(position - 1));
        const bool rightBoundary = end == normalizedBody.size()
            || !isIdentifierCharacter(normalizedBody.at(end));
        if (leftBoundary && rightBoundary)
            return true;
        position = normalizedBody.indexOf(normalizedWord, position + 1);
    }
    return false;
}

QString displayTarget(const IrcConversationKey& key, const QString& target)
{
    return target.isEmpty() ? key.normalizedTarget : target;
}

void startNamesSync(IrcChannelState& channel)
{
    channel.members.clear();
    channel.namesSyncing = true;
    channel.namesSyncStarted = QDateTime::currentDateTimeUtc();
}

void stopNamesSync(IrcChannelState& channel)
{
    channel.namesSyncing = false;
    channel.namesSyncStarted = {};
}

QString collapseEventBody(const QString& existing, const QString& incoming)
{
    static const QString suffixes[] = {
        QStringLiteral(" joined"),
        QStringLiteral(" left"),
        QStringLiteral(" quit"),
    };
    for (const QString& suffix : suffixes) {
        if (existing.endsWith(suffix) && incoming.endsWith(suffix)) {
            return existing.chopped(suffix.size()) + QStringLiteral(", ")
                + incoming.chopped(suffix.size()) + suffix;
        }
    }
    return existing + QStringLiteral(", ") + incoming;
}

QString kindToken(IrcMessageKind kind)
{
    switch (kind) {
    case IrcMessageKind::Action:
        return QStringLiteral("action");
    case IrcMessageKind::Event:
    case IrcMessageKind::Error:
        return QStringLiteral("event");
    case IrcMessageKind::Whois:
        return QStringLiteral("whois");
    case IrcMessageKind::Notice:
        return QStringLiteral("notice");
    case IrcMessageKind::Message:
        return QStringLiteral("message");
    }
    return {};
}

std::optional<IrcMessageKind> kindFromToken(const QString& token)
{
    if (token == QLatin1String("action"))
        return IrcMessageKind::Action;
    if (token == QLatin1String("event"))
        return IrcMessageKind::Event;
    if (token == QLatin1String("notice"))
        return IrcMessageKind::Notice;
    if (token == QLatin1String("message"))
        return IrcMessageKind::Message;
    return std::nullopt;
}

bool persistableKind(IrcMessageKind kind)
{
    switch (kind) {
    case IrcMessageKind::Message:
    case IrcMessageKind::Notice:
    case IrcMessageKind::Action:
    case IrcMessageKind::Event:
        return true;
    case IrcMessageKind::Error:
    case IrcMessageKind::Whois:
        return false;
    }
    return false;
}

enum class ChatLineReason
{
    NickMention,
    Highlight,
    DirectMessage,
};

std::optional<ChatLineReason> classifyChatLine(
    const IrcConversationState& conversation,
    IrcMessageKind kind,
    bool self,
    bool nickHit,
    bool highlightHit)
{
    if (kind != IrcMessageKind::Message && kind != IrcMessageKind::Action)
        return std::nullopt;
    if (self)
        return std::nullopt;
    if (nickHit)
        return ChatLineReason::NickMention;
    if (highlightHit)
        return ChatLineReason::Highlight;
    if (!conversation.isChannel())
        return ChatLineReason::DirectMessage;
    return std::nullopt;
}

IrcInboxKind inboxKindFor(ChatLineReason reason)
{
    switch (reason) {
    case ChatLineReason::NickMention:
        return IrcInboxKind::Mention;
    case ChatLineReason::Highlight:
        return IrcInboxKind::Highlight;
    case ChatLineReason::DirectMessage:
        return IrcInboxKind::Direct;
    }
    return IrcInboxKind::Mention;
}

void admitMessage(IrcConversationState& conversation, IrcReducedMessage message)
{
    message.sequence = conversation.nextSequence++;
    conversation.messages.push_back(std::move(message));
}
}

bool ircTargetLooksLikeService(const QString& target,
                               const IrcServerFeatures& features)
{
    const std::string_view types = features.channelTypes();
    return ircIsServiceIdentity(
        target,
        QStringView{},
        QString::fromLatin1(types.data(), qsizetype(types.size())));
}

bool IrcMemberView::isAway() const noexcept
{
    return away.has_value();
}

bool IrcConversationState::isChannel() const noexcept
{
    return std::holds_alternative<IrcChannelState>(detail);
}

const IrcChannelState *IrcConversationState::channel() const noexcept
{
    return std::get_if<IrcChannelState>(&detail);
}

IrcChannelState *IrcConversationState::channel() noexcept
{
    return std::get_if<IrcChannelState>(&detail);
}

int IrcConversationState::peopleCount() const noexcept
{
    const IrcChannelState *state = channel();
    return state ? static_cast<int>(state->members.size()) : 0;
}

void IrcEventReducer::setServerFeatures(const QString& networkId,
                                        const IrcServerFeatures& features)
{
    m_features[networkId] = features;
}

const IrcServerFeatures& IrcEventReducer::serverFeatures(
    const QString& networkId) const
{
    const auto found = m_features.find(networkId);
    if (found != m_features.end())
        return found->second;
    static const IrcServerFeatures defaults;
    return defaults;
}

void IrcEventReducer::setHighlightWords(const QString& networkId,
                                        const QStringList& words)
{
    if (networkId.isEmpty())
        return;
    if (words.isEmpty())
        m_highlightWords.erase(networkId);
    else
        m_highlightWords[networkId] = words;
}

IrcConversationKey IrcEventReducer::conversationKey(
    const QString& networkId, const QString& target) const
{
    return {networkId, normalize(networkId, target)};
}

void IrcEventReducer::markSelected(const IrcConversationKey& key)
{
    if (m_selected && *m_selected == key)
        return;
    m_selected = key;
    if (IrcConversationState *conversation = findMutable(key)) {
        if (conversation->unread == 0)
            conversation->unreadMark.reset();
        conversation->unread = 0;
        conversation->mentions = 0;
    }
}

bool IrcEventReducer::markRead(const IrcConversationKey& key)
{
    IrcConversationState *conversation = findMutable(key);
    if (!conversation || (conversation->unread == 0 && conversation->mentions == 0))
        return false;
    conversation->unread = 0;
    conversation->mentions = 0;
    // Keep unreadMark: the "New messages" boundary stays visible in an
    // already-open transcript and clears on the next selection, matching
    // markSelected's keep-on-visit rule. IrcController::setWindowActive(true)
    // is what calls this on focus-regain; the reducer setter is flag-only.
    return true;
}

void IrcEventReducer::setWindowActive(bool active)
{
    // Flag only. Consuming unread on focus-regain is
    // IrcController::setWindowActive, which calls markRead for the
    // selected conversation.
    m_windowActive = active;
}

void IrcEventReducer::clearSelection()
{
    m_selected.reset();
}

std::optional<IrcConversationKey> IrcEventReducer::selected() const
{
    return m_selected;
}

void IrcEventReducer::apply(const IrcEvent& event)
{
    m_mentionArrival.reset();
    m_inboxArrival.reset();
    std::visit([this](const auto& value) { reduce(value); }, event);
}

std::optional<IrcMentionArrival> IrcEventReducer::takeMentionArrival()
{
    std::optional<IrcMentionArrival> mention = m_mentionArrival;
    m_mentionArrival.reset();
    return mention;
}

std::optional<IrcInboxArrival> IrcEventReducer::takeInboxArrival()
{
    std::optional<IrcInboxArrival> arrival = m_inboxArrival;
    m_inboxArrival.reset();
    return arrival;
}

bool IrcEventReducer::releaseStaleNamesSync(const std::optional<IrcConversationKey>& key,
                                            const QDateTime& now)
{
    if (!key)
        return false;
    IrcConversationState *conversation = findMutable(*key);
    IrcChannelState *channel = conversation ? conversation->channel() : nullptr;
    if (!channel || !channel->namesSyncing)
        return false;
    if (channel->namesSyncStarted.msecsTo(now) < kStaleNamesSyncMs)
        return false;
    stopNamesSync(*channel);
    return true;
}

bool IrcEventReducer::dropDirectMessage(const IrcConversationKey& key)
{
    const auto found = m_conversations.find(key);
    if (found == m_conversations.end() || found->second.isChannel())
        return false;
    if (m_selected == key)
        m_selected.reset();
    m_conversations.erase(found);
    return true;
}

bool IrcEventReducer::dropChannel(const IrcConversationKey& key)
{
    const auto found = m_conversations.find(key);
    if (found == m_conversations.end() || !found->second.isChannel())
        return false;
    if (m_selected == key)
        m_selected.reset();
    m_conversations.erase(found);
    return true;
}

void IrcEventReducer::forgetNetwork(const QString& networkId)
{
    if (networkId.isEmpty())
        return;
    dropPendingPlayback(networkId);
    m_queryRestorePending.erase(networkId);
    for (auto it = m_conversations.begin(); it != m_conversations.end(); ) {
        if (it->first.networkId == networkId)
            it = m_conversations.erase(it);
        else
            ++it;
    }
    m_features.erase(networkId);
    m_currentNicks.erase(networkId);
    m_highlightWords.erase(networkId);
    m_presence.erase(networkId);
    m_selfAway.erase(networkId);
    for (auto it = m_mutedKeys.begin(); it != m_mutedKeys.end(); ) {
        if (it->networkId == networkId)
            it = m_mutedKeys.erase(it);
        else
            ++it;
    }
    if (m_selected && m_selected->networkId == networkId)
        m_selected.reset();
}

void IrcEventReducer::setConversationLog(IrcConversationLog *log)
{
    m_log = log;
}

void IrcEventReducer::hydrateFromLog(IrcConversationState& conversation)
{
    if (!m_log)
        return;
    const std::vector<IrcTranscriptLine> lines = m_log->readTail(
        conversation.key.networkId, conversation.key.normalizedTarget,
        kMaxMessages);
    for (const IrcTranscriptLine& line : lines) {
        const std::optional<IrcMessageKind> kind = kindFromToken(line.kind);
        if (!kind)
            continue;
        const IrcMsgId msgid{line.msgid};
        if (!msgid.isEmpty() && conversation.messageIds.count(msgid))
            continue;
        if (!msgid.isEmpty())
            conversation.messageIds.insert(msgid);
        admitMessage(conversation,
                     {line.author, line.body, line.timestamp, *kind, false,
                      IrcOrigin::Replay, msgid});
    }
    capMessages(conversation);
}

void IrcEventReducer::persistMessage(const IrcConversationState& conversation,
                                     const IrcReducedMessage& message)
{
    if (!m_log || !persistableKind(message.kind))
        return;
    const QString kind = kindToken(message.kind);
    if (kind.isEmpty())
        return;
    m_log->append(conversation.key.networkId, conversation.key.normalizedTarget,
                  {message.timestamp, message.author, kind, message.body,
                   message.msgid.value});
}

void IrcEventReducer::setMuted(const IrcConversationKey& key, bool muted)
{
    if (key.networkId.isEmpty() || key.normalizedTarget.isEmpty())
        return;
    if (muted)
        m_mutedKeys.insert(key);
    else
        m_mutedKeys.erase(key);
    if (IrcConversationState *conversation = findMutable(key)) {
        conversation->muted = muted;
        if (muted)
            conversation->mentions = 0;
    }
}

void IrcEventReducer::clearMessages(const IrcConversationKey& key)
{
    IrcConversationState *conversation = findMutable(key);
    if (!conversation)
        return;
    conversation->trimmed += int(conversation->messages.size());
    conversation->messages.clear();
    conversation->messageIds.clear();
    ++conversation->spliceEpoch;
    if (IrcChannelState *channel = conversation->channel())
        channel->historyAnchor.reset();
}

const IrcEventReducer::Store& IrcEventReducer::conversations() const noexcept
{
    return m_conversations;
}

const IrcConversationState *IrcEventReducer::find(
    const IrcConversationKey& key) const noexcept
{
    const auto found = m_conversations.find(key);
    return found == m_conversations.end() ? nullptr : &found->second;
}

std::optional<IrcMemberView> IrcEventReducer::memberView(
    const IrcConversationKey& key, const QString& normalizedNick) const
{
    const IrcConversationState *conversation = find(key);
    const IrcChannelState *channel = conversation ? conversation->channel() : nullptr;
    if (!channel)
        return std::nullopt;
    const auto member = channel->members.find(normalizedNick);
    if (member == channel->members.end())
        return std::nullopt;

    const auto presence = m_presence.find(key.networkId);
    const IrcNickPresence facts = presence == m_presence.end()
        ? IrcNickPresence{}
        : presence->second.lookup(normalizedNick);
    const IrcServerFeatures& features = serverFeatures(key.networkId);
    // Self-away arrives as a network-level numeric (305/306), not as member
    // presence, and a server is not required to echo our own AWAY back through
    // away-notify. Overlay it on our own row so the identity footer and the
    // self member row agree in every channel. The event carries no reason, so
    // the overlay has none.
    std::optional<IrcAway> away = facts.away;
    if (!away && selfAway(key.networkId)
        && isSelf(key.networkId, member->second.displayNick)) {
        away = IrcAway{};
    }
    return IrcMemberView{
        member->second.displayNick,
        ircWireText(features.memberLabel(
            member->second.ranks, utf8(member->second.displayNick))),
        member->second.ranks,
        away,
        facts.status(),
        facts.avatar(),
        facts.isBot(),
        facts.metadata(IrcMetadata::displayNameKey()),
        facts.metadata(IrcMetadata::pronounsKey()),
        facts.metadata(IrcMetadata::homepageKey()),
        facts.metadata(IrcMetadata::colorKey()),
        displayAccount(key.networkId, member->second.displayNick)};
}

IrcNickPresence IrcEventReducer::nickPresence(const QString& networkId,
                                              const QString& nick) const
{
    if (networkId.isEmpty() || nick.isEmpty())
        return {};
    const auto presence = m_presence.find(networkId);
    if (presence == m_presence.end())
        return {};
    return presence->second.lookup(normalize(networkId, nick));
}

QString IrcEventReducer::displayAccount(const QString& networkId,
                                        const QString& nick) const
{
    const QString account = nickPresence(networkId, nick).account;
    if (account.isEmpty() || nick.isEmpty())
        return {};
    if (serverFeatures(networkId).caseMapping().equals(utf8(account), utf8(nick)))
        return {};
    return account;
}

IrcPeerPresence IrcEventReducer::peerPresence(const QString& networkId,
                                              const QString& normalizedNick) const
{
    if (normalizedNick.isEmpty() || !isVisible(networkId, normalizedNick))
        return IrcPeerPresence::Unknown;
    const auto presence = m_presence.find(networkId);
    if (presence == m_presence.end())
        return IrcPeerPresence::Online;
    return presence->second.lookup(normalizedNick).away
        ? IrcPeerPresence::Away
        : IrcPeerPresence::Online;
}

QVector<IrcOrderedMember> IrcEventReducer::orderedMembers(
    const IrcConversationKey& key) const
{
    const IrcConversationState *conversation = find(key);
    const IrcChannelState *channel = conversation ? conversation->channel() : nullptr;
    if (!channel)
        return {};

    const IrcServerFeatures& features = serverFeatures(key.networkId);
    QVector<IrcOrderedMember> ordered;
    ordered.reserve(int(channel->members.size()));
    for (const auto& entry : channel->members)
        ordered.append({entry.first, features.rankPriority(entry.second.ranks)});
    std::sort(ordered.begin(), ordered.end());
    return ordered;
}

void IrcEventReducer::clearPresenceFacts(const QString& networkId,
                                         bool away,
                                         bool metadata)
{
    const auto presence = m_presence.find(networkId);
    if (presence == m_presence.end())
        return;
    if (away)
        presence->second.clearAway();
    if (metadata)
        presence->second.clearMetadata();
}

bool IrcEventReducer::selfAway(const QString& networkId) const noexcept
{
    return m_selfAway.count(networkId) != 0;
}

QStringList IrcEventReducer::typingNicks(const IrcConversationKey& key,
                                         const QDateTime& now) const
{
    QStringList nicks;
    const IrcConversationState *conversation = find(key);
    if (!conversation)
        return nicks;
    for (const auto& entry : conversation->typing) {
        if (isSelf(key.networkId, entry.second.displayNick))
            continue;
        if (!ircTypingShowsIndicator(entry.second, now))
            continue;
        nicks.append(entry.second.displayNick);
    }
    return nicks;
}

bool IrcEventReducer::directPeerIsTyping(const IrcConversationKey& key,
                                         const QDateTime& now) const
{
    const IrcConversationState *conversation = find(key);
    if (!conversation || conversation->isChannel())
        return false;
    const auto found = conversation->typing.find(key.normalizedTarget);
    if (found == conversation->typing.end())
        return false;
    return ircTypingShowsIndicator(found->second, now);
}

void IrcEventReducer::clearTypingFacts(const QString& networkId)
{
    for (auto& entry : m_conversations) {
        if (entry.second.key.networkId == networkId)
            entry.second.typing.clear();
    }
}

void IrcEventReducer::clearTyping(IrcConversationState& conversation,
                                  const QString& normalizedNick)
{
    conversation.typing.erase(normalizedNick);
}

void IrcEventReducer::clearTypingEverywhere(const QString& networkId,
                                            const QString& normalizedNick)
{
    for (auto& entry : m_conversations) {
        if (entry.second.key.networkId == networkId)
            clearTyping(entry.second, normalizedNick);
    }
}

void IrcEventReducer::rekeyTyping(const QString& networkId,
                                  const QString& oldNormalized,
                                  const QString& newNormalized,
                                  const QString& newDisplay)
{
    for (auto& entry : m_conversations) {
        if (entry.second.key.networkId != networkId)
            continue;
        auto& typing = entry.second.typing;
        const auto found = typing.find(oldNormalized);
        if (found == typing.end())
            continue;
        IrcTypingHint hint = found->second;
        hint.displayNick = newDisplay;
        typing.erase(found);
        typing.insert_or_assign(newNormalized, std::move(hint));
    }
}

void IrcEventReducer::pruneExpiredTyping(IrcConversationState& conversation,
                                         const QDateTime& now)
{
    for (auto it = conversation.typing.begin(); it != conversation.typing.end(); ) {
        if (!ircTypingHintRetained(it->second, now))
            it = conversation.typing.erase(it);
        else
            ++it;
    }
}

bool IrcEventReducer::isVisible(const QString& networkId,
                                const QString& normalizedNick) const
{
    for (const auto& entry : m_conversations) {
        const IrcConversationState& conversation = entry.second;
        if (conversation.key.networkId != networkId)
            continue;
        const IrcChannelState *channel = conversation.channel();
        if (channel && channel->members.count(normalizedNick) != 0)
            return true;
    }
    return false;
}

void IrcEventReducer::forgetUnseen(const QString& networkId,
                                   const QStringList& normalizedNicks)
{
    const auto presence = m_presence.find(networkId);
    if (presence == m_presence.end())
        return;
    for (const QString& nick : normalizedNicks) {
        if (presence->second.knows(nick) && !isVisible(networkId, nick))
            presence->second.forget(nick);
    }
}

IrcConversationState *IrcEventReducer::ensureConversation(
    const IrcConversationKey& key,
    const QString& displayTarget,
    IrcConversationCause cause)
{
    if (IrcConversationState *found = findMutable(key))
        return found;

    const IrcServerFeatures& features = serverFeatures(key.networkId);
    const bool targetIsChannel = features.isChannel(utf8(key.normalizedTarget));
    const bool targetLooksLikeService = ircTargetLooksLikeService(displayTarget, features);
    if (!ircConversationCauseInserts(cause, targetIsChannel, targetLooksLikeService))
        return nullptr;

    IrcConversationState conversation;
    conversation.key = key;
    conversation.target = displayTarget;
    conversation.muted = m_mutedKeys.count(key) > 0;
    if (targetIsChannel)
        conversation.detail = IrcChannelState{};
    else
        conversation.detail = IrcDirectMessageState{};
    IrcConversationState *created =
        &m_conversations.emplace(key, std::move(conversation)).first->second;
    hydrateFromLog(*created);
    return created;
}

IrcConversationState *IrcEventReducer::findMutable(
    const IrcConversationKey& key) noexcept
{
    const auto found = m_conversations.find(key);
    return found == m_conversations.end() ? nullptr : &found->second;
}

QString IrcEventReducer::normalize(const QString& networkId,
                                   const QString& identifier) const
{
    return ircWireText(serverFeatures(networkId).caseMapping().normalize(
        utf8(identifier)));
}

bool IrcEventReducer::equals(const QString& networkId,
                             const QString& left,
                             const QString& right) const
{
    return serverFeatures(networkId).caseMapping().equals(utf8(left), utf8(right));
}

bool IrcEventReducer::isSelf(const QString& networkId, const QString& nick) const
{
    const auto current = m_currentNicks.find(networkId);
    return current != m_currentNicks.end()
        && equals(networkId, current->second, nick);
}

bool IrcEventReducer::mentions(const QString& networkId,
                               const QString& body) const
{
    return isMention(networkId, body);
}

bool IrcEventReducer::isTranscriptHighlight(const QString& networkId,
                                            const QString& author,
                                            IrcMessageKind kind,
                                            const QString& body) const
{
    // Kind and self match classifyChatLine, but that helper also returns
    // DirectMessage for every query without a body hit. Regular DMs do not
    // wash; only a nick or /highlight word in the body does.
    if (kind != IrcMessageKind::Message && kind != IrcMessageKind::Action)
        return false;
    if (isSelf(networkId, author))
        return false;
    return mentions(networkId, body);
}

bool IrcEventReducer::isNickMention(const QString& networkId,
                                    const QString& body) const
{
    const QString normalizedBody = normalize(networkId, body);
    const auto current = m_currentNicks.find(networkId);
    return current != m_currentNicks.end()
        && containsWord(normalizedBody, normalize(networkId, current->second));
}

bool IrcEventReducer::isHighlightHit(const QString& networkId,
                                     const QString& body) const
{
    const QString normalizedBody = normalize(networkId, body);
    const auto words = m_highlightWords.find(networkId);
    if (words == m_highlightWords.end())
        return false;
    for (const QString& word : words->second) {
        if (containsWord(normalizedBody, normalize(networkId, word)))
            return true;
    }
    return false;
}

bool IrcEventReducer::isMention(const QString& networkId,
                                const QString& body) const
{
    return isNickMention(networkId, body) || isHighlightHit(networkId, body);
}

void IrcEventReducer::appendChat(const IrcConversationKey& key,
                                 const QString& displayTarget,
                                 const QString& author,
                                 const QString& body,
                                 const QDateTime& timestamp,
                                 IrcMessageKind kind,
                                 const IrcMsgId& msgid)
{
    const bool self = isSelf(key.networkId, author);
    IrcConversationState *conversation = ensureConversation(
        key,
        displayTarget,
        self ? IrcConversationCause::InboundSelf
             : IrcConversationCause::InboundOther);
    if (!conversation)
        return;
    if (!msgid.isEmpty() && conversation->messageIds.count(msgid))
        return;
    if (!msgid.isEmpty())
        conversation->messageIds.insert(msgid);
    admitMessage(*conversation, {author, body, timestamp, kind, false,
                                IrcOrigin::Live, msgid});
    persistMessage(*conversation, conversation->messages.back());
    capMessages(*conversation);
    clearTyping(*conversation, normalize(key.networkId, author));
    noteChatArrival(*conversation, key, author, body, kind, msgid,
                    conversation->messages.back().sequence, IrcOrigin::Live);
}

void IrcEventReducer::noteChatArrival(IrcConversationState& conversation,
                                      const IrcConversationKey& key,
                                      const QString& author,
                                      const QString& body,
                                      IrcMessageKind kind,
                                      const IrcMsgId& msgid,
                                      qint64 sequence,
                                      IrcOrigin origin)
{
    const bool self = isSelf(key.networkId, author);
    const bool nickHit = isNickMention(key.networkId, body);
    const bool highlightHit = !nickHit && isHighlightHit(key.networkId, body);
    const std::optional<ChatLineReason> reason = classifyChatLine(
        conversation, kind, self, nickHit, highlightHit);
    if (reason && !conversation.muted) {
        m_mentionArrival = IrcMentionArrival{
            author, body, key.networkId, conversation.target, msgid};
    }
    const bool selected = m_selected && *m_selected == key;
    // Inbox is the waiting list for conversations the user is not looking
    // at. Selection still skips it while the window is unfocused: that open
    // buffer already plants unread, mentionArrived still desktop-notifies,
    // and focus-regain consumes the badge in place. An inbox row would
    // linger, because consume-on-select does not run on focus return.
    if (origin == IrcOrigin::Live && reason && !conversation.muted
        && !selected) {
        m_inboxArrival = IrcInboxArrival{
            inboxKindFor(*reason),
            author,
            body,
            key.networkId,
            conversation.target,
            msgid,
        };
    }
    // Selected live chat is unread only while unfocused. Replay while
    // unfocused is still backlog (CHATHISTORY / bouncer playback), not
    // "new since you left", so it must not plant the mark mid-splice.
    // Focused replay (for example ZNC playback on the first auto-selected
    // channel) still plants the mark without bumping unread.
    if (self)
        return;
    const bool skipLiveFocused =
        selected && origin == IrcOrigin::Live && m_windowActive;
    const bool skipReplayUnfocused =
        selected && origin != IrcOrigin::Live && !m_windowActive;
    if (skipLiveFocused || skipReplayUnfocused)
        return;
    const bool holdFocusedReplayMark = selected && origin != IrcOrigin::Live
        && conversation.unreadMark.has_value();
    if (conversation.unread == 0 && !holdFocusedReplayMark)
        conversation.unreadMark = sequence;
    if (selected && origin != IrcOrigin::Live)
        return;
    ++conversation.unread;
    if ((reason == ChatLineReason::NickMention
         || reason == ChatLineReason::Highlight)
        && !conversation.muted) {
        ++conversation.mentions;
    }
}

void IrcEventReducer::appendEvent(IrcConversationState& conversation,
                                  const QString& body,
                                  bool collapsible)
{
    if (collapsible && !conversation.messages.empty()) {
        IrcReducedMessage& last = conversation.messages.back();
        if (last.kind == IrcMessageKind::Event && last.collapsible) {
            last.body = collapseEventBody(last.body, body);
            return;
        }
    }
    admitMessage(conversation,
                 {QString(), body, QDateTime(), IrcMessageKind::Event, collapsible,
                  IrcOrigin::Live, IrcMsgId{}});
    persistMessage(conversation, conversation.messages.back());
    capMessages(conversation);
}

void IrcEventReducer::appendWhois(IrcConversationState& conversation,
                                  const QString& body)
{
    admitMessage(conversation,
                 {QString(), body, QDateTime(), IrcMessageKind::Whois, false,
                  IrcOrigin::Live, IrcMsgId{}});
    capMessages(conversation);
}

void IrcEventReducer::appendError(IrcConversationState& conversation,
                                  const QString& body)
{
    admitMessage(conversation,
                 {QString(), body, QDateTime(), IrcMessageKind::Error, false,
                  IrcOrigin::Live, IrcMsgId{}});
    capMessages(conversation);
}

void IrcEventReducer::capMessages(IrcConversationState& conversation)
{
    auto& messages = conversation.messages;
    const int extra = int(messages.size()) - kMaxMessages;
    if (extra <= 0)
        return;
    for (int index = 0; index < extra; ++index) {
        if (!messages[std::size_t(index)].msgid.isEmpty())
            conversation.messageIds.erase(messages[std::size_t(index)].msgid);
    }
    messages.erase(messages.begin(), messages.begin() + extra);
    conversation.trimmed += extra;
    if (IrcChannelState *channel = conversation.channel()) {
        if (channel->historyAnchor
            && channel->historyAnchor->sequence < conversation.trimmed) {
            channel->historyAnchor.reset();
        }
    }
}

// A direct message has no join line to splice above, so its replay lands at
// the tail. The anchor is a position in the logical transcript: trimmed
// messages plus the index of the join line. A channel that is already joined
// but whose anchor was consumed, cleared, or trimmed also lands at the tail.
// Holding that batch for another self-join lets the next welcome drop it.
std::optional<std::size_t> IrcEventReducer::peekSpliceIndex(
    const IrcConversationState& conversation) const
{
    const IrcChannelState *channel = conversation.channel();
    if (!channel)
        return conversation.messages.size();
    if (!channel->historyAnchor) {
        if (channel->joined)
            return conversation.messages.size();
        return std::nullopt;
    }
    const qint64 index = channel->historyAnchor->sequence - conversation.trimmed;
    if (index < 0 || index > qint64(conversation.messages.size()))
        return std::nullopt;
    return std::size_t(index);
}

std::optional<std::size_t> IrcEventReducer::takeSpliceIndex(
    IrcConversationState& conversation)
{
    IrcChannelState *channel = conversation.channel();
    if (!channel)
        return conversation.messages.size();
    if (!channel->historyAnchor)
        return std::nullopt;
    const qint64 index = channel->historyAnchor->sequence - conversation.trimmed;
    // A usable anchor stays until a replay line actually lands. An empty or
    // fully deduped batch must not burn it. An index that no longer names a
    // row cannot splice, so drop it here.
    if (index < 0 || index > qint64(conversation.messages.size())) {
        channel->historyAnchor.reset();
        return std::nullopt;
    }
    return std::size_t(index);
}

std::vector<IrcKeptReplay> IrcEventReducer::takeKeptReplay()
{
    std::vector<IrcKeptReplay> kept;
    kept.swap(m_keptReplay);
    return kept;
}

std::vector<IrcRememberedQuery> IrcEventReducer::takeRememberedQueries()
{
    std::vector<IrcRememberedQuery> remembered;
    remembered.swap(m_rememberedQueries);
    return remembered;
}

void IrcEventReducer::reduce(const IrcWelcomeEvent& event)
{
    // Welcome is a new connection. Batches still held here belong to the
    // previous one. Playback on this connection cannot have arrived yet, and
    // open-direct restore is pending again until the next 376 or 422.
    dropPendingPlayback(event.networkId);
    m_queryRestorePending.insert(event.networkId);
    m_currentNicks[event.networkId] = event.currentNick;
    m_presence[event.networkId].clear();
    m_selfAway.erase(event.networkId);
    for (auto& entry : m_conversations) {
        IrcConversationState& conversation = entry.second;
        if (conversation.key.networkId != event.networkId)
            continue;
        if (IrcChannelState *channel = conversation.channel()) {
            channel->members.clear();
            channel->joined = false;
            channel->historyAnchor.reset();
            stopNamesSync(*channel);
        }
        conversation.typing.clear();
    }
}

void IrcEventReducer::reduce(const IrcMessageEvent& event)
{
    appendChat(event.conversation, displayTarget(event.conversation, event.target),
               event.author, event.body,
               event.timestamp, IrcMessageKind::Message, event.msgid);
}

void IrcEventReducer::reduce(const IrcNoticeEvent& event)
{
    appendChat(event.conversation, displayTarget(event.conversation, event.target),
               event.author, event.body,
               event.timestamp, IrcMessageKind::Notice, event.msgid);
}

void IrcEventReducer::reduce(const IrcActionEvent& event)
{
    appendChat(event.conversation, displayTarget(event.conversation, event.target),
               event.author, event.body,
               event.timestamp, IrcMessageKind::Action, event.msgid);
}

void IrcEventReducer::reduce(const IrcJoinEvent& event)
{
    const QString normalizedNick = normalize(event.networkId, event.nick);
    if (event.account && !normalizedNick.isEmpty())
        m_presence[event.networkId].setAccount(normalizedNick, *event.account);

    const IrcConversationKey key = conversationKey(event.networkId, event.channel);
    IrcConversationState *conversation =
        ensureConversation(key, event.channel, IrcConversationCause::ChannelState);
    if (!conversation)
        return;
    IrcChannelState *channel = conversation->channel();
    if (!channel)
        return;
    IrcPrefixSet ranks;
    const auto existing = channel->members.find(normalizedNick);
    if (existing != channel->members.end())
        ranks = existing->second.ranks;
    channel->members.insert_or_assign(
        normalizedNick, IrcMemberState{event.nick, ranks});
    const bool self = isSelf(event.networkId, event.nick);
    if (self) {
        channel->joined = true;
        channel->historyAnchor = IrcTranscriptAnchor{
            conversation->trimmed + qint64(conversation->messages.size())};
    }
    QString body = event.nick + QStringLiteral(" joined");
    if (event.account) {
        const QString shown = displayAccount(event.networkId, event.nick);
        if (!shown.isEmpty()) {
            body = event.nick + QStringLiteral(" (") + shown
                + QStringLiteral(") joined");
        }
    }
    appendEvent(*conversation, body, !self);
    if (self)
        releasePendingPlayback(*conversation);
}

void IrcEventReducer::reduce(const IrcAccountEvent& event)
{
    const QString normalizedNick = normalize(event.networkId, event.nick);
    if (normalizedNick.isEmpty())
        return;
    m_presence[event.networkId].setAccount(normalizedNick, event.account);
}

void IrcEventReducer::reduce(const IrcPartEvent& event)
{
    const IrcConversationKey key = conversationKey(event.networkId, event.channel);
    IrcConversationState *conversation = findMutable(key);
    if (!conversation || !conversation->channel())
        return;
    IrcChannelState& channel = *conversation->channel();
    QStringList departed{normalize(event.networkId, event.nick)};
    channel.members.erase(departed.front());
    if (isSelf(event.networkId, event.nick)) {
        channel.joined = false;
        channel.historyAnchor.reset();
        for (const auto& member : channel.members)
            departed.append(member.first);
        channel.members.clear();
        stopNamesSync(channel);
    }
    forgetUnseen(event.networkId, departed);
    appendEvent(*conversation, event.nick + QStringLiteral(" left"), true);
    if (isSelf(event.networkId, event.nick))
        conversation->typing.clear();
    else
        clearTyping(*conversation, departed.front());
}

void IrcEventReducer::reduce(const IrcQuitEvent& event)
{
    const QString normalizedNick = normalize(event.networkId, event.nick);
    for (auto& entry : m_conversations) {
        IrcConversationState& conversation = entry.second;
        if (conversation.key.networkId != event.networkId)
            continue;
        IrcChannelState *channel = conversation.channel();
        if (!channel || channel->members.erase(normalizedNick) == 0)
            continue;
        appendEvent(conversation, event.nick + QStringLiteral(" quit"), true);
    }
    forgetUnseen(event.networkId, {normalizedNick});
    clearTypingEverywhere(event.networkId, normalizedNick);
}

void IrcEventReducer::reduce(const IrcNickEvent& event)
{
    const QString oldNormalized = normalize(event.networkId, event.oldNick);
    const QString newNormalized = normalize(event.networkId, event.newNick);
    if (isSelf(event.networkId, event.oldNick))
        m_currentNicks[event.networkId] = event.newNick;
    m_presence[event.networkId].rekey(oldNormalized, newNormalized);
    rekeyTyping(event.networkId, oldNormalized, newNormalized, event.newNick);

    for (auto& entry : m_conversations) {
        IrcConversationState& conversation = entry.second;
        if (conversation.key.networkId != event.networkId)
            continue;
        IrcChannelState *channel = conversation.channel();
        if (!channel)
            continue;
        const auto member = channel->members.find(oldNormalized);
        if (member == channel->members.end())
            continue;
        IrcMemberState updated = member->second;
        updated.displayNick = event.newNick;
        channel->members.erase(member);
        channel->members.insert_or_assign(newNormalized, std::move(updated));
        appendEvent(conversation, event.oldNick + QStringLiteral(" is now ")
                         + event.newNick, true);
    }

    const IrcConversationKey oldKey{event.networkId, oldNormalized};
    const IrcConversationKey newKey{event.networkId, newNormalized};
    auto direct = m_conversations.find(oldKey);
    if (direct == m_conversations.end() || direct->second.isChannel())
        return;

    direct->second.target = event.newNick;
    appendEvent(direct->second,
                event.oldNick + QStringLiteral(" is now ") + event.newNick,
                true);
    if (oldKey == newKey)
        return;

    IrcConversationState moved = std::move(direct->second);
    m_conversations.erase(direct);
    moved.key = newKey;
    moved.target = event.newNick;
    if (m_mutedKeys.erase(oldKey))
        m_mutedKeys.insert(newKey);
    auto existing = m_conversations.find(newKey);
    if (existing == m_conversations.end()) {
        m_conversations.emplace(newKey, std::move(moved));
    } else {
        // Both sides may already hold the same msgid. Admitting a duplicate row
        // would leave one id covering two messages, and trimming either one
        // would then let a third copy through.
        for (IrcReducedMessage& message : moved.messages) {
            if (!message.msgid.isEmpty()) {
                if (!existing->second.messageIds.insert(message.msgid).second)
                    continue;
            }
            const qint64 oldSequence = message.sequence;
            message.sequence = existing->second.nextSequence++;
            if (!existing->second.unreadMark && moved.unreadMark
                && oldSequence == *moved.unreadMark) {
                existing->second.unreadMark = message.sequence;
            }
            existing->second.messages.push_back(std::move(message));
        }
        capMessages(existing->second);
        existing->second.unread += moved.unread;
        existing->second.mentions += moved.mentions;
        existing->second.muted = existing->second.muted || moved.muted;
        for (auto& hint : moved.typing)
            existing->second.typing.insert_or_assign(hint.first,
                                                     std::move(hint.second));
        existing->second.target = event.newNick;
    }
    if (m_selected && *m_selected == oldKey)
        m_selected = newKey;
}

void IrcEventReducer::reduce(const IrcKickEvent& event)
{
    const IrcConversationKey key = conversationKey(event.networkId, event.channel);
    IrcConversationState *conversation = findMutable(key);
    if (!conversation || !conversation->channel())
        return;
    IrcChannelState& channel = *conversation->channel();
    QStringList departed{normalize(event.networkId, event.target)};
    channel.members.erase(departed.front());
    if (isSelf(event.networkId, event.target)) {
        channel.joined = false;
        channel.historyAnchor.reset();
        for (const auto& member : channel.members)
            departed.append(member.first);
        channel.members.clear();
        stopNamesSync(channel);
    }
    forgetUnseen(event.networkId, departed);
    appendEvent(*conversation, event.target + QStringLiteral(" was kicked"));
    if (isSelf(event.networkId, event.target)) {
        conversation->typing.clear();
        const QString preview = event.reason.isEmpty()
            ? QStringLiteral("You were kicked")
            : event.reason;
        m_inboxArrival = IrcInboxArrival{
            IrcInboxKind::Kick,
            event.author,
            preview,
            event.networkId,
            event.channel,
            IrcMsgId{},
        };
    } else {
        clearTyping(*conversation, departed.front());
    }
}

void IrcEventReducer::reduce(const IrcTopicEvent& event)
{
    const IrcConversationKey key = conversationKey(event.networkId, event.channel);
    IrcConversationState *conversation =
        ensureConversation(key, event.channel, IrcConversationCause::ChannelState);
    if (!conversation)
        return;
    if (IrcChannelState *channel = conversation->channel())
        channel->topic = event.topic;
}

void IrcEventReducer::reduce(const IrcNamesEvent& event)
{
    const IrcConversationKey key = conversationKey(event.networkId, event.channel);
    IrcConversationState *conversation =
        ensureConversation(key, event.channel, IrcConversationCause::ChannelState);
    if (!conversation)
        return;
    IrcChannelState *channel = conversation->channel();
    if (!channel)
        return;

    if (!channel->namesSyncing)
        startNamesSync(*channel);
    for (const IrcName& name : event.names) {
        channel->members.insert_or_assign(
            normalize(event.networkId, name.nick),
            IrcMemberState{name.nick, name.ranks});
    }
    if (event.complete)
        stopNamesSync(*channel);
}

void IrcEventReducer::reduce(const IrcModeEvent& event)
{
    const IrcConversationKey key = conversationKey(event.networkId, event.target);
    const IrcServerFeatures& features = serverFeatures(event.networkId);
    if (!features.isChannel(utf8(key.normalizedTarget)))
        return;
    IrcConversationState *conversation =
        ensureConversation(key, event.target, IrcConversationCause::ChannelState);
    if (!conversation)
        return;
    appendEvent(*conversation, event.author + QStringLiteral(" set mode ")
                     + event.mode);
    IrcChannelState *channel = conversation->channel();
    if (!channel)
        return;

    std::vector<std::string> arguments;
    arguments.reserve(static_cast<std::size_t>(event.arguments.size()));
    for (const QString& argument : event.arguments)
        arguments.push_back(utf8(argument));
    for (const IrcPrefixChange& change :
         features.prefixChanges(utf8(event.mode), arguments)) {
        const auto member = channel->members.find(
            normalize(event.networkId, ircWireText(change.nick())));
        if (member == channel->members.end())
            continue;
        member->second.ranks = features.apply(member->second.ranks, change);
    }
}

void IrcEventReducer::reduce(const IrcAwayEvent& event)
{
    m_presence[event.networkId].setAway(
        normalize(event.networkId, event.nick), event.away);
}

void IrcEventReducer::reduce(const IrcSelfAwayEvent& event)
{
    if (event.away)
        m_selfAway.insert(event.networkId);
    else
        m_selfAway.erase(event.networkId);
}

void IrcEventReducer::reduce(const IrcMemberMetadataEvent& event)
{
    m_presence[event.networkId].setMetadata(
        normalize(event.networkId, event.nick), event.key, event.value);
}

void IrcEventReducer::reduce(const IrcTypingEvent& event)
{
    IrcConversationState *conversation = findMutable(event.conversation);
    const QString normalizedNick =
        normalize(event.conversation.networkId, event.nick);
    if (!conversation || isSelf(event.conversation.networkId, event.nick)) {
        if (conversation)
            clearTyping(*conversation, normalizedNick);
        return;
    }
    const std::optional<IrcTypingHint> hint =
        IrcTypingHint::stored(event.phase, event.receivedAt, event.nick);
    if (!hint) {
        clearTyping(*conversation, normalizedNick);
        return;
    }
    pruneExpiredTyping(*conversation, event.receivedAt);
    conversation->typing.insert_or_assign(normalizedNick, *hint);
}

void IrcEventReducer::dropPendingPlayback(const QString& networkId)
{
    if (networkId.isEmpty())
        return;
    for (auto it = m_pendingPlayback.begin(); it != m_pendingPlayback.end(); ) {
        if (it->first.networkId == networkId)
            it = m_pendingPlayback.erase(it);
        else
            ++it;
    }
}

void IrcEventReducer::releasePendingPlayback(IrcConversationState& conversation)
{
    const auto found = m_pendingPlayback.find(conversation.key);
    if (found == m_pendingPlayback.end())
        return;
    const IrcHistoryEvent event = std::move(found->second);
    m_pendingPlayback.erase(found);
    spliceHistory(conversation, event, HistoryAnchorUse::Keep);
}

bool IrcEventReducer::replayFromPeer(const IrcHistoryEvent& event) const
{
    return std::any_of(event.lines.begin(), event.lines.end(),
                       [&](const IrcReplayLine& line) {
        return !line.author.isEmpty()
            && !isSelf(event.conversation.networkId, line.author);
    });
}

bool IrcEventReducer::releasePendingQueryPlayback(const QString& networkId)
{
    if (networkId.isEmpty())
        return false;
    m_queryRestorePending.erase(networkId);
    std::vector<IrcConversationKey> keys;
    for (const auto& entry : m_pendingPlayback) {
        if (entry.first.networkId != networkId)
            continue;
        if (serverFeatures(networkId).isChannel(utf8(entry.first.normalizedTarget)))
            continue;
        keys.push_back(entry.first);
    }
    bool changed = false;
    for (const IrcConversationKey& key : keys) {
        const auto found = m_pendingPlayback.find(key);
        if (found == m_pendingPlayback.end())
            continue;
        const IrcHistoryEvent event = std::move(found->second);
        m_pendingPlayback.erase(found);
        IrcConversationState *conversation = findMutable(event.conversation);
        if (!conversation) {
            // Same rule as a query batch that arrives after restore: open
            // only when a peer wrote. A self-only batch with no conversation
            // is finished and must not move the clock.
            if (!replayFromPeer(event))
                continue;
            conversation = ensureConversation(event.conversation, event.target,
                                              IrcConversationCause::InboundOther);
            if (!conversation)
                continue;
        }
        spliceHistory(*conversation, event, HistoryAnchorUse::Consume);
        changed = true;
    }
    return changed;
}

void IrcEventReducer::spliceHistory(IrcConversationState& conversation,
                                    const IrcHistoryEvent& event,
                                    HistoryAnchorUse anchorUse)
{
    const std::optional<std::size_t> spliceIndex =
        anchorUse == HistoryAnchorUse::Consume
            ? takeSpliceIndex(conversation)
            : peekSpliceIndex(conversation);
    if (!spliceIndex)
        return;
    const std::size_t previousSize = conversation.messages.size();
    const std::size_t at = *spliceIndex;
    const bool bouncerQuery = event.kind == IrcHistoryKind::BouncerPlayback
        && !conversation.channel();

    const IrcCaseMapping mapping =
        serverFeatures(event.conversation.networkId).caseMapping();
    const auto sameReplayLine = [&](const IrcReducedMessage& message,
                                    const IrcReplayLine& line,
                                    IrcMessageKind kind) {
        if (message.kind != kind || message.body != line.body)
            return false;
        if (!message.timestamp.isValid() || !line.timestamp.isValid())
            return false;
        if (message.timestamp.toUTC().toMSecsSinceEpoch()
            != line.timestamp.toUTC().toMSecsSinceEpoch()) {
            return false;
        }
        return mapping.equals(utf8(message.author), utf8(line.author));
    };

    std::vector<IrcReducedMessage> run;
    run.reserve(event.lines.size());
    bool sawSelf = false;
    for (const IrcReplayLine& line : event.lines) {
        if (bouncerQuery && !sawSelf && !line.author.isEmpty()
            && isSelf(event.conversation.networkId, line.author)) {
            sawSelf = true;
        }
        // Inserted or already present. A missing time tag stays off the
        // PLAY clock; the transcript timestamp may be wall clock.
        if (line.serverTime && line.serverTime->isValid()) {
            m_keptReplay.push_back(IrcKeptReplay{
                event.conversation.networkId,
                event.target,
                line.serverTime->toUTC(),
            });
        }
        if (!line.msgid.isEmpty() && conversation.messageIds.count(line.msgid))
            continue;
        const IrcMessageKind kind = line.kind == IrcMessageKindTag::Emote
            ? IrcMessageKind::Action
            : IrcMessageKind::Message;
        const bool alreadyPresent = std::any_of(
            conversation.messages.begin(), conversation.messages.end(),
            [&](const IrcReducedMessage& message) {
                return sameReplayLine(message, line, kind);
            })
            || std::any_of(run.begin(), run.end(),
                           [&](const IrcReducedMessage& message) {
                               return sameReplayLine(message, line, kind);
                           });
        if (alreadyPresent)
            continue;
        if (!line.msgid.isEmpty())
            conversation.messageIds.insert(line.msgid);
        run.push_back({line.author, line.body, line.timestamp, kind, false,
                       IrcOrigin::Replay, line.msgid});
        run.back().sequence = conversation.nextSequence++;
    }
    if (sawSelf) {
        m_rememberedQueries.push_back(IrcRememberedQuery{
            event.conversation.networkId,
            event.target.isEmpty() ? conversation.target : event.target,
        });
    }
    if (run.empty())
        return;
    conversation.messages.insert(
        conversation.messages.begin() + std::ptrdiff_t(at),
        run.begin(),
        run.end());
    for (const IrcReducedMessage& message : run)
        persistMessage(conversation, message);
    // Consume drops the anchor only once a replay line has landed. Keep
    // still names the join line, shifted forward by what actually landed.
    if (IrcChannelState *channel = conversation.channel()) {
        if (anchorUse == HistoryAnchorUse::Consume)
            channel->historyAnchor.reset();
        else if (channel->historyAnchor)
            channel->historyAnchor->sequence += qint64(run.size());
    }
    if (at != previousSize)
        ++conversation.spliceEpoch;
    capMessages(conversation);
    for (const IrcReducedMessage& message : run) {
        noteChatArrival(conversation, event.conversation, message.author,
                        message.body, message.kind, message.msgid,
                        message.sequence, IrcOrigin::Replay);
    }
}

void IrcEventReducer::reduce(const IrcHistoryEvent& event)
{
    const bool channelTarget = serverFeatures(event.conversation.networkId)
        .isChannel(utf8(event.conversation.normalizedTarget));
    IrcConversationState *conversation = findMutable(event.conversation);
    // ZNC answers PLAY before JOIN. Hold one batch until this connection's
    // self-join, and hold a parted channel for the next one. Once joined,
    // splice in this call: above the anchor when it is still there, and at
    // the tail when CHATHISTORY, /clear, or the cap already removed it.
    // Waiting for another join lets the next welcome drop the batch. A
    // never-joined buffer must not open a channel. CHATHISTORY before an
    // anchor still drops.
    if (event.kind == IrcHistoryKind::BouncerPlayback && channelTarget) {
        const IrcChannelState *channel =
            conversation ? conversation->channel() : nullptr;
        if (!channel || !channel->joined) {
            m_pendingPlayback.insert_or_assign(event.conversation, event);
            return;
        }
        spliceHistory(*conversation, event, HistoryAnchorUse::Keep);
        return;
    }

    if (!conversation) {
        // A join creates a channel, so replay must not resurrect one the user
        // closed or parted. A query buffer proves nothing by existing. Our own
        // outbound /msg creates one on the bouncer too, so only a line's
        // author proves the peer spoke. A self-only bouncer batch stays held
        // until open directs are restored; dropping it first lets a later
        // kept line move the exclusive PLAY clock past those lines.
        if (channelTarget)
            return;
        if (!replayFromPeer(event)) {
            if (event.kind == IrcHistoryKind::BouncerPlayback
                && m_queryRestorePending.count(event.conversation.networkId)) {
                m_pendingPlayback.insert_or_assign(event.conversation, event);
            }
            return;
        }
        conversation = ensureConversation(event.conversation, event.target,
                                          IrcConversationCause::InboundOther);
        if (!conversation)
            return;
    }
    spliceHistory(*conversation, event, HistoryAnchorUse::Consume);
}

void IrcEventReducer::reduce(const IrcWhoisTranscriptEvent& event)
{
    IrcConversationState *conversation = findMutable(event.destination);
    if (!conversation)
        return;
    appendWhois(*conversation, event.formattedBody);
}

void IrcEventReducer::reduce(const IrcChannelErrorEvent& event)
{
    IrcConversationState *conversation =
        findMutable(conversationKey(event.networkId, event.channel));
    if (!conversation)
        return;
    appendError(*conversation, event.body);
}

