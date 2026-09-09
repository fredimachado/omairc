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

int networkRank(const QString& networkId, const QStringList& networkOrder)
{
    const int index = networkOrder.indexOf(networkId);
    return index >= 0 ? index : networkOrder.size();
}
}

QVector<IrcConversationKey> ircSidebarOrder(const IrcEventReducer& reducer)
{
    return ircSidebarOrder(reducer, {});
}

QVector<IrcConversationKey> ircSidebarOrder(const IrcEventReducer& reducer,
                                            const QStringList& networkOrder)
{
    QVector<IrcConversationKey> keys;
    keys.reserve(int(reducer.conversations().size()));
    for (const auto& entry : reducer.conversations())
        keys.append(entry.first);

    QStringList order = networkOrder;
    if (order.isEmpty()) {
        for (const IrcConversationKey& key : keys) {
            if (!order.contains(key.networkId))
                order.append(key.networkId);
        }
        order.sort();
    }

    std::sort(keys.begin(), keys.end(), [&reducer, &order](const IrcConversationKey& left,
                                                           const IrcConversationKey& right) {
        const int leftNetwork = networkRank(left.networkId, order);
        const int rightNetwork = networkRank(right.networkId, order);
        if (leftNetwork != rightNetwork)
            return leftNetwork < rightNetwork;
        if (left.networkId != right.networkId)
            return left.networkId < right.networkId;
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
    if (i >= 0) {
        if (i + 1 < n && ordered.at(i + 1).networkId == dropping.networkId)
            return ordered.at(i + 1);
        if (i > 0 && ordered.at(i - 1).networkId == dropping.networkId)
            return ordered.at(i - 1);
        if (i + 1 < n)
            return ordered.at(i + 1);
        if (i > 0)
            return ordered.at(i - 1);
        return std::nullopt;
    }
    if (n > 0)
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
    QVector<IrcConversationKey> keys = ircSidebarOrder(m_reducer, m_networkOrder);
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

void ConversationListModel::setNetworkOrder(const QStringList& networkOrder)
{
    if (m_networkOrder == networkOrder)
        return;
    m_networkOrder = networkOrder;
    reload();
}
