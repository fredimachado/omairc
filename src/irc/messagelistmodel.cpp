#include "messagelistmodel.h"

#include "irceventreducer.h"

namespace
{
QString kindString(IrcMessageKind kind)
{
    switch (kind) {
    case IrcMessageKind::Action:
        return QStringLiteral("action");
    case IrcMessageKind::Event:
    case IrcMessageKind::Error:
        return QStringLiteral("event");
    case IrcMessageKind::Whois:
        return QStringLiteral("whois");
    case IrcMessageKind::Message:
        return QStringLiteral("message");
    case IrcMessageKind::Notice:
        return QStringLiteral("notice");
    }
    return QStringLiteral("message");
}

QString displayTime(const IrcReducedMessage& message)
{
    if (message.kind == IrcMessageKind::Event
        || message.kind == IrcMessageKind::Whois
        || !message.timestamp.isValid())
        return {};
    return message.timestamp.toUTC().toString(QStringLiteral("HH:mm"));
}
}

MessageListModel::MessageListModel(IrcEventReducer& reducer, QObject *parent)
    : QAbstractListModel(parent)
    , m_reducer(reducer)
{
}

int MessageListModel::rowCount(const QModelIndex& parent) const
{
    return parent.isValid() ? 0 : m_count;
}

QVariant MessageListModel::data(const QModelIndex& index, int role) const
{
    if (!index.isValid() || !m_selected || index.row() < 0 || index.row() >= m_count)
        return {};

    const IrcConversationState *conversation = m_reducer.find(*m_selected);
    if (!conversation || index.row() >= int(conversation->messages.size()))
        return {};

    const IrcReducedMessage& message = conversation->messages[size_t(index.row())];
    switch (role) {
    case AuthorRole:
        return message.author;
    case TimeRole:
        return displayTime(message);
    case BodyRole:
        return message.body;
    case KindRole:
        return kindString(message.kind);
    case NetworkIdRole:
        return conversation->key.networkId;
    default:
        return {};
    }
}

QHash<int, QByteArray> MessageListModel::roleNames() const
{
    return {
        {AuthorRole, "author"},
        {TimeRole, "time"},
        {BodyRole, "body"},
        {KindRole, "kind"},
        {NetworkIdRole, "networkId"},
    };
}

void MessageListModel::reload()
{
    int count = 0;
    int trimmed = 0;
    if (m_selected) {
        if (const IrcConversationState *conversation = m_reducer.find(*m_selected)) {
            count = int(conversation->messages.size());
            trimmed = conversation->trimmed;
        }
    }

    const bool sameConversation = m_selected.has_value() == m_loaded.has_value()
        && (!m_selected || *m_selected == *m_loaded);
    if (sameConversation) {
        int removed = trimmed - m_trimmed;
        if (removed < 0)
            removed = 0;
        if (removed > m_count)
            removed = m_count;
        if (removed > 0) {
            beginRemoveRows(QModelIndex(), 0, removed - 1);
            m_count -= removed;
            m_trimmed = trimmed;
            endRemoveRows();
        }
        if (count > m_count) {
            beginInsertRows(QModelIndex(), m_count, count - 1);
            m_count = count;
            endInsertRows();
            return;
        }
        if (count == m_count) {
            if (count > 0) {
                const QModelIndex last = index(count - 1);
                emit dataChanged(last, last);
            }
            return;
        }
    }

    beginResetModel();
    m_count = count;
    m_loaded = m_selected;
    m_trimmed = trimmed;
    endResetModel();
}

void MessageListModel::setSelected(const IrcConversationKey& key)
{
    m_selected = key;
}

void MessageListModel::select(const IrcConversationKey& key)
{
    setSelected(key);
    reload();
}

void MessageListModel::clearSelection()
{
    m_selected.reset();
    reload();
}
