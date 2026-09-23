#include "conversationlistmodel.h"

#include <QDateTime>
#include <QVariantMap>

#include <algorithm>

namespace
{
// A direct message row carries the peer's presence so the sidebar can stop
// claiming "online" for someone the member list would mark away. The facts come
// from the reducer, the same source member rows read.
QString conversationPresence(const IrcEventReducer& reducer,
                             const IrcConversationState& conversation)
{
    if (conversation.isChannel())
        return {};
    switch (reducer.peerPresence(conversation.key.networkId,
                                 conversation.key.normalizedTarget)) {
    case IrcPeerPresence::Online:
        return QStringLiteral("online");
    case IrcPeerPresence::Away:
        return QStringLiteral("away");
    case IrcPeerPresence::Unknown:
        break;
    }
    return QStringLiteral("offline");
}

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
    case ConversationNameRole:
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
    case TypingRole:
        return m_reducer.directPeerIsTyping(conversation->key,
                                            QDateTime::currentDateTimeUtc());
    case MutedRole:
        return conversation->muted;
    case PresenceRole:
        return conversationPresence(m_reducer, *conversation);
    case AvatarRole:
        if (conversation->isChannel())
            return QString();
        return m_reducer.nickPresence(conversation->key.networkId,
                                      conversation->key.normalizedTarget)
            .avatar();
    case BotRole:
        if (conversation->isChannel())
            return false;
        return m_reducer.nickPresence(conversation->key.networkId,
                                      conversation->key.normalizedTarget)
            .isBot();
    }
    return {};
}

QHash<int, QByteArray> ConversationListModel::staticRoleNames()
{
    return {
        {ConversationRole, "conversation"},
        {UnreadRole, "unread"},
        {MentionRole, "mention"},
        {DirectRole, "direct"},
        {NetworkIdRole, "networkId"},
        {ConversationIdRole, "conversationId"},
        {ConversationNameRole, "conversationName"},
        {TypingRole, "typing"},
        {MutedRole, "muted"},
        {PresenceRole, "presence"},
        {AvatarRole, "avatar"},
        {BotRole, "bot"},
    };
}

QHash<int, QByteArray> ConversationListModel::roleNames() const
{
    return staticRoleNames();
}

QVariantMap ConversationListModel::get(int row) const
{
    QVariantMap result;
    if (row < 0 || row >= rowCount())
        return result;
    const QModelIndex idx = index(row, 0);
    const auto names = staticRoleNames();
    for (auto it = names.cbegin(); it != names.cend(); ++it)
        result.insert(QString::fromUtf8(it.value()), data(idx, it.key()));
    return result;
}

QVariant ConversationListModel::field(int row, const QString& name) const
{
    static const QHash<QString, int> roles = [] {
        QHash<QString, int> byName;
        const QHash<int, QByteArray> names = ConversationListModel::staticRoleNames();
        for (auto it = names.cbegin(); it != names.cend(); ++it)
            byName.insert(QString::fromUtf8(it.value()), it.key());
        return byName;
    }();
    const auto role = roles.constFind(name);
    if (role == roles.cend())
        return {};
    return data(index(row, 0), role.value());
}

bool ConversationListModel::hasDirects(const QString& networkId) const
{
    if (networkId.isEmpty())
        return false;
    for (const IrcConversationKey& key : m_keys) {
        if (key.networkId != networkId)
            continue;
        const IrcConversationState *conversation = m_reducer.find(key);
        if (conversation && !conversation->isChannel())
            return true;
    }
    return false;
}

void ConversationListModel::reload()
{
    QVector<IrcConversationKey> keys = ircSidebarOrder(m_reducer, m_networkOrder);
    if (keys == m_keys) {
        if (keys.isEmpty())
            return;
        emit dataChanged(index(0, 0),
                         index(keys.size() - 1, 0),
                         {Qt::DisplayRole, ConversationRole, UnreadRole, MentionRole,
                          TypingRole, MutedRole, PresenceRole, AvatarRole, BotRole});
        return;
    }
    beginResetModel();
    m_keys = std::move(keys);
    endResetModel();
}

void ConversationListModel::invalidateTyping()
{
    if (m_keys.isEmpty())
        return;
    emit dataChanged(index(0, 0),
                     index(m_keys.size() - 1, 0),
                     {TypingRole});
}

void ConversationListModel::select(const IrcConversationKey& key)
{
    Q_UNUSED(key)
    reload();
}

void ConversationListModel::setNetworkOrder(const QStringList& networkOrder)
{
    if (m_networkOrder == networkOrder)
        return;
    m_networkOrder = networkOrder;
    reload();
}
