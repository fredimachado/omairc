#include "irceventreducer.h"

#include "ircwiretext.h"

#include <QByteArray>

#include <string>
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

IrcConversationKey IrcEventReducer::conversationKey(
    const QString& networkId, const QString& target) const
{
    return {networkId, normalize(networkId, target)};
}

void IrcEventReducer::markSelected(const IrcConversationKey& key)
{
    m_selected = key;
    if (IrcConversationState *conversation = findMutable(key)) {
        conversation->unread = 0;
        conversation->mentions = 0;
    }
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
    std::visit([this](const auto& value) { reduce(value); }, event);
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

void IrcEventReducer::clearMessages(const IrcConversationKey& key)
{
    IrcConversationState *conversation = findMutable(key);
    if (!conversation)
        return;
    conversation->messages.clear();
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
    return IrcMemberView{
        member->second.displayNick,
        ircWireText(features.memberLabel(
            member->second.ranks, utf8(member->second.displayNick))),
        member->second.ranks,
        facts.away,
        facts.status};
}

void IrcEventReducer::clearPresenceFacts(const QString& networkId,
                                         bool away,
                                         bool status)
{
    const auto presence = m_presence.find(networkId);
    if (presence == m_presence.end())
        return;
    if (away)
        presence->second.clearAway();
    if (status)
        presence->second.clearStatus();
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
        if (!ircIsTyping(entry.second, now))
            continue;
        nicks.append(entry.second.displayNick);
    }
    return nicks;
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
        if (!ircIsTyping(it->second, now))
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

IrcConversationState& IrcEventReducer::ensureConversation(
    const IrcConversationKey& key, const QString& displayTarget)
{
    const auto found = m_conversations.find(key);
    if (found != m_conversations.end())
        return found->second;

    IrcConversationState conversation;
    conversation.key = key;
    conversation.target = displayTarget;
    if (serverFeatures(key.networkId).isChannel(utf8(key.normalizedTarget)))
        conversation.detail = IrcChannelState{};
    else
        conversation.detail = IrcDirectMessageState{};
    return m_conversations.emplace(key, std::move(conversation)).first->second;
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

bool IrcEventReducer::isMention(const QString& networkId,
                                const QString& body) const
{
    const auto current = m_currentNicks.find(networkId);
    if (current == m_currentNicks.end() || current->second.isEmpty())
        return false;

    const QString normalizedBody = normalize(networkId, body);
    const QString normalizedNick = normalize(networkId, current->second);
    qsizetype position = normalizedBody.indexOf(normalizedNick);
    while (position >= 0) {
        const qsizetype end = position + normalizedNick.size();
        const bool leftBoundary = position == 0
            || !isIdentifierCharacter(normalizedBody.at(position - 1));
        const bool rightBoundary = end == normalizedBody.size()
            || !isIdentifierCharacter(normalizedBody.at(end));
        if (leftBoundary && rightBoundary)
            return true;
        position = normalizedBody.indexOf(normalizedNick, position + 1);
    }
    return false;
}

void IrcEventReducer::appendChat(const IrcConversationKey& key,
                                 const QString& displayTarget,
                                 const QString& author,
                                 const QString& body,
                                 const QDateTime& timestamp,
                                 IrcMessageKind kind)
{
    IrcConversationState& conversation =
        ensureConversation(key, displayTarget);
    conversation.messages.push_back({author, body, timestamp, kind});
    clearTyping(conversation, normalize(key.networkId, author));

    if (isSelf(key.networkId, author) || (m_selected && *m_selected == key))
        return;

    ++conversation.unread;
    if ((kind == IrcMessageKind::Message || kind == IrcMessageKind::Action)
        && isMention(key.networkId, body)) {
        ++conversation.mentions;
    }
}

void IrcEventReducer::appendEvent(IrcConversationState& conversation,
                                  const QString& body)
{
    conversation.messages.push_back(
        {QString(), body, QDateTime(), IrcMessageKind::Event});
}

void IrcEventReducer::reduce(const IrcWelcomeEvent& event)
{
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
            stopNamesSync(*channel);
        }
        conversation.typing.clear();
    }
}

void IrcEventReducer::reduce(const IrcMessageEvent& event)
{
    appendChat(event.conversation, displayTarget(event.conversation, event.target),
               event.author, event.body,
               event.timestamp, IrcMessageKind::Message);
}

void IrcEventReducer::reduce(const IrcNoticeEvent& event)
{
    appendChat(event.conversation, displayTarget(event.conversation, event.target),
               event.author, event.body,
               event.timestamp, IrcMessageKind::Notice);
}

void IrcEventReducer::reduce(const IrcActionEvent& event)
{
    appendChat(event.conversation, displayTarget(event.conversation, event.target),
               event.author, event.body,
               event.timestamp, IrcMessageKind::Action);
}

void IrcEventReducer::reduce(const IrcJoinEvent& event)
{
    const IrcConversationKey key = conversationKey(event.networkId, event.channel);
    IrcConversationState& conversation = ensureConversation(key, event.channel);
    IrcChannelState *channel = conversation.channel();
    if (!channel)
        return;
    const QString normalizedNick = normalize(event.networkId, event.nick);
    IrcPrefixSet ranks;
    const auto existing = channel->members.find(normalizedNick);
    if (existing != channel->members.end())
        ranks = existing->second.ranks;
    channel->members.insert_or_assign(
        normalizedNick, IrcMemberState{event.nick, ranks});
    if (isSelf(event.networkId, event.nick))
        channel->joined = true;
    appendEvent(conversation, event.nick + QStringLiteral(" joined"));
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
        for (const auto& member : channel.members)
            departed.append(member.first);
        channel.members.clear();
        stopNamesSync(channel);
    }
    forgetUnseen(event.networkId, departed);
    appendEvent(*conversation, event.nick + QStringLiteral(" left"));
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
        appendEvent(conversation, event.nick + QStringLiteral(" quit"));
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
                         + event.newNick);
    }

    const IrcConversationKey oldKey{event.networkId, oldNormalized};
    const IrcConversationKey newKey{event.networkId, newNormalized};
    auto direct = m_conversations.find(oldKey);
    if (direct == m_conversations.end() || direct->second.isChannel())
        return;

    direct->second.target = event.newNick;
    appendEvent(direct->second,
                event.oldNick + QStringLiteral(" is now ") + event.newNick);
    if (oldKey == newKey)
        return;

    IrcConversationState moved = std::move(direct->second);
    m_conversations.erase(direct);
    moved.key = newKey;
    moved.target = event.newNick;
    auto existing = m_conversations.find(newKey);
    if (existing == m_conversations.end()) {
        m_conversations.emplace(newKey, std::move(moved));
    } else {
        existing->second.messages.insert(existing->second.messages.end(),
                                         moved.messages.begin(),
                                         moved.messages.end());
        existing->second.unread += moved.unread;
        existing->second.mentions += moved.mentions;
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
        for (const auto& member : channel.members)
            departed.append(member.first);
        channel.members.clear();
        stopNamesSync(channel);
    }
    forgetUnseen(event.networkId, departed);
    appendEvent(*conversation, event.target + QStringLiteral(" was kicked"));
    if (isSelf(event.networkId, event.target))
        conversation->typing.clear();
    else
        clearTyping(*conversation, departed.front());
}

void IrcEventReducer::reduce(const IrcTopicEvent& event)
{
    const IrcConversationKey key = conversationKey(event.networkId, event.channel);
    IrcConversationState& conversation = ensureConversation(key, event.channel);
    if (IrcChannelState *channel = conversation.channel())
        channel->topic = event.topic;
}

void IrcEventReducer::reduce(const IrcNamesEvent& event)
{
    const IrcConversationKey key = conversationKey(event.networkId, event.channel);
    IrcConversationState& conversation = ensureConversation(key, event.channel);
    IrcChannelState *channel = conversation.channel();
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
    IrcConversationState& conversation = ensureConversation(key, event.target);
    appendEvent(conversation, event.author + QStringLiteral(" set mode ")
                     + event.mode);
    IrcChannelState *channel = conversation.channel();
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

void IrcEventReducer::reduce(const IrcMemberStatusEvent& event)
{
    m_presence[event.networkId].setStatus(
        normalize(event.networkId, event.nick), event.status);
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

