#include "conversationlistmodel.h"

#include <algorithm>

namespace
{
int groupRank(const IrcConversationState *conversation)
{
    if (!conversation || !conversation->isChannel())
        return 2;
    const IrcChannelState *channel = conversation->channel();
    if (channel && channel->joined)
        return 0;
    return 1;
}
}

QVector<IrcConversationKey> ircSidebarOrder(const IrcEventReducer& reducer)
{
    QVector<IrcConversationKey> keys;
    keys.reserve(int(reducer.conversations().size()));
    for (const auto& entry : reducer.conversations())
        keys.append(entry.first);

    std::sort(keys.begin(), keys.end(), [&reducer](const IrcConversationKey& left,
                                                   const IrcConversationKey& right) {
        const IrcConversationState *leftState = reducer.find(left);
        const IrcConversationState *rightState = reducer.find(right);
        const int leftGroup = groupRank(leftState);
        const int rightGroup = groupRank(rightState);
        if (leftGroup != rightGroup)
            return leftGroup < rightGroup;
        const QString leftTarget = leftState ? leftState->target : left.normalizedTarget;
        const QString rightTarget = rightState ? rightState->target : right.normalizedTarget;
        const int name = QString::compare(leftTarget, rightTarget, Qt::CaseInsensitive);
        if (name != 0)
            return name < 0;
        return left < right;
    });
    return keys;
}

std::optional<IrcConversationKey> ircNeighborAfterDrop(
    const QVector<IrcConversationKey>& ordered,
    const IrcConversationKey& dropping)
{
    const int n = ordered.size();
    const int i = ordered.indexOf(dropping);
    if (i >= 0 && i + 1 < n)
        return ordered.at(i + 1);
    if (i >= 0 && i > 0)
        return ordered.at(i - 1);
    if (i < 0 && n > 0)
        return ordered.last();
    return std::nullopt;
}

ConversationListModel::ConversationListModel(IrcEventReducer& reducer,
                                             QObject *parent)
    : QAbstractListModel(parent)
    , m_reducer(reducer)
{
}

int ConversationListModel::rowCount(const QModelIndex& parent) const
{
    return parent.isValid() ? 0 : m_keys.size();
}

QVariant ConversationListModel::data(const QModelIndex& index, int role) const
{
    if (!index.isValid() || index.row() < 0 || index.row() >= m_keys.size())
        return {};

    const IrcConversationState *conversation = m_reducer.find(m_keys.at(index.row()));
    if (!conversation)
        return {};

    switch (role) {
    case ConversationRole:
    case Qt::DisplayRole:
        return conversation->target;
    case UnreadRole:
        return conversation->unread;
    case MentionRole:
        return conversation->mentions > 0;
    case DirectRole:
        return !conversation->isChannel();
    case NetworkIdRole:
        return conversation->key.networkId;
    case ConversationIdRole:
        return ircConversationId(conversation->key);
    default:
        return {};
    }
}

QHash<int, QByteArray> ConversationListModel::roleNames() const
{
    return {
        {ConversationRole, "conversation"},
        {UnreadRole, "unread"},
        {MentionRole, "mention"},
        {DirectRole, "direct"},
        {NetworkIdRole, "networkId"},
        {ConversationIdRole, "conversationId"},
    };
}

void ConversationListModel::reload()
{
    QVector<IrcConversationKey> keys = ircSidebarOrder(m_reducer);
    if (keys == m_keys) {
        if (keys.isEmpty())
            return;
        emit dataChanged(index(0, 0),
                         index(keys.size() - 1, 0),
                         {Qt::DisplayRole, ConversationRole, UnreadRole, MentionRole});
        return;
    }
    beginResetModel();
    m_keys = std::move(keys);
    endResetModel();
}

void ConversationListModel::select(const IrcConversationKey& key)
{
    m_reducer.markSelected(key);
    reload();
}
