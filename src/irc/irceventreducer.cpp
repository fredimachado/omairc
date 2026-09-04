#include "irceventreducer.h"

#include <QByteArray>

namespace
{
std::string utf8(const QString& value)
{
    const QByteArray encoded = value.toUtf8();
    return std::string(encoded.constData(), static_cast<std::size_t>(encoded.size()));
}

QString fromUtf8(const std::string& value)
{
    return QString::fromUtf8(value.data(), static_cast<qsizetype>(value.size()));
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

void IrcEventReducer::apply(const IrcEvent& event)
{
    std::visit([this](const auto& value) { reduce(value); }, event);
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
    return fromUtf8(serverFeatures(networkId).caseMapping().normalize(
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
    for (auto& entry : m_conversations) {
        IrcConversationState& conversation = entry.second;
        if (conversation.key.networkId != event.networkId)
            continue;
        if (IrcChannelState *channel = conversation.channel()) {
            channel->members.clear();
            channel->joined = false;
            channel->namesSyncing = false;
        }
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
    channel->members.insert_or_assign(
        normalizedNick, IrcMemberState{event.nick, QString(), false});
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
    channel.members.erase(normalize(event.networkId, event.nick));
    if (isSelf(event.networkId, event.nick)) {
        channel.joined = false;
        channel.members.clear();
        channel.namesSyncing = false;
    }
    appendEvent(*conversation, event.nick + QStringLiteral(" left"));
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
}

void IrcEventReducer::reduce(const IrcNickEvent& event)
{
    const QString oldNormalized = normalize(event.networkId, event.oldNick);
    const QString newNormalized = normalize(event.networkId, event.newNick);
    if (isSelf(event.networkId, event.oldNick))
        m_currentNicks[event.networkId] = event.newNick;

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
        updated.nick = event.newNick;
        channel->members.erase(member);
        channel->members.insert_or_assign(newNormalized, std::move(updated));
        appendEvent(conversation, event.oldNick + QStringLiteral(" is now ")
                         + event.newNick);
    }

    const IrcConversationKey oldKey{event.networkId, oldNormalized};
    const IrcConversationKey newKey{event.networkId, newNormalized};
    auto direct = m_conversations.find(oldKey);
    if (direct != m_conversations.end() && !direct->second.isChannel()
        && oldKey != newKey) {
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
        }
        if (m_selected && *m_selected == oldKey)
            m_selected = newKey;
    }
}

void IrcEventReducer::reduce(const IrcKickEvent& event)
{
    const IrcConversationKey key = conversationKey(event.networkId, event.channel);
    IrcConversationState *conversation = findMutable(key);
    if (!conversation || !conversation->channel())
        return;
    IrcChannelState& channel = *conversation->channel();
    channel.members.erase(normalize(event.networkId, event.target));
    if (isSelf(event.networkId, event.target)) {
        channel.joined = false;
        channel.members.clear();
        channel.namesSyncing = false;
    }
    appendEvent(*conversation, event.target + QStringLiteral(" was kicked"));
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

    if (!channel->namesSyncing) {
        channel->members.clear();
        channel->namesSyncing = true;
    }
    for (const IrcName& name : event.names) {
        channel->members.insert_or_assign(
            normalize(event.networkId, name.nick),
            IrcMemberState{name.nick, name.status, name.away});
    }
    if (event.complete)
        channel->namesSyncing = false;
}

void IrcEventReducer::reduce(const IrcModeEvent& event)
{
    const IrcConversationKey key = conversationKey(event.networkId, event.target);
    if (!serverFeatures(event.networkId).isChannel(utf8(key.normalizedTarget)))
        return;
    IrcConversationState& conversation = ensureConversation(key, event.target);
    appendEvent(conversation, event.author + QStringLiteral(" set mode ")
                     + event.mode);
}

