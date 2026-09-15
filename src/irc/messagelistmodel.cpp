#include "messagelistmodel.h"

#include "irceventreducer.h"

#include <QLocale>

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
    return message.timestamp.toLocalTime().toString(QStringLiteral("HH:mm"));
}

std::optional<QDate> assignedDate(const IrcReducedMessage& message,
                                  const std::optional<QDate>& previous)
{
    if (message.timestamp.isValid())
        return message.timestamp.toLocalTime().date();
    if (message.origin == IrcOrigin::Live)
        return QDate::currentDate();
    return previous;
}

QString dateLabel(const QDate& date)
{
    const QDate today = QDate::currentDate();
    if (date == today)
        return QStringLiteral("Today");
    if (date == today.addDays(-1))
        return QStringLiteral("Yesterday");
    return QLocale().toString(date, QLocale::ShortFormat);
}
}

std::vector<MessageListModel::VisualRow> MessageListModel::buildView(
    const IrcConversationState *conversation)
{
    std::vector<VisualRow> view;
    if (!conversation)
        return view;
    view.reserve(conversation->messages.size() + 8);
    std::optional<QDate> previous;
    for (int i = 0; i < int(conversation->messages.size()); ++i) {
        const std::optional<QDate> date =
            assignedDate(conversation->messages[size_t(i)], previous);
        if (date && previous && *date != *previous)
            view.push_back({VisualRow::Type::Separator, 0, *date});
        view.push_back({VisualRow::Type::Store, i, {}});
        if (date)
            previous = date;
    }
    return view;
}

int MessageListModel::visualPrefixToRemove(const std::vector<VisualRow>& view,
                                           int storeRemoved)
{
    if (storeRemoved <= 0)
        return 0;
    int removed = 0;
    for (int i = 0; i < int(view.size()); ++i) {
        const VisualRow& row = view[size_t(i)];
        if (row.type == VisualRow::Type::Store) {
            if (row.storeIndex < storeRemoved)
                ++removed;
            else
                break;
            continue;
        }
        int following = -1;
        for (int j = i + 1; j < int(view.size()); ++j) {
            if (view[size_t(j)].type == VisualRow::Type::Store) {
                following = view[size_t(j)].storeIndex;
                break;
            }
        }
        const bool trimmedWithPrefix = following >= 0 && following < storeRemoved;
        const bool wouldLeadRemaining = following == storeRemoved;
        if (trimmedWithPrefix || wouldLeadRemaining)
            ++removed;
        else
            break;
    }
    return removed;
}

MessageListModel::MessageListModel(IrcEventReducer& reducer, QObject *parent)
    : QAbstractListModel(parent)
    , m_reducer(reducer)
{
}

int MessageListModel::rowCount(const QModelIndex& parent) const
{
    return parent.isValid() ? 0 : int(m_view.size());
}

QVariant MessageListModel::data(const QModelIndex& index, int role) const
{
    if (!index.isValid() || !m_selected || index.row() < 0
        || index.row() >= int(m_view.size()))
        return {};

    const IrcConversationState *conversation = m_reducer.find(*m_selected);
    if (!conversation)
        return {};

    const VisualRow& row = m_view[size_t(index.row())];
    if (row.type == VisualRow::Type::Separator) {
        switch (role) {
        case AuthorRole:
            return QString();
        case TimeRole:
            return QString();
        case BodyRole:
            return dateLabel(row.date);
        case KindRole:
            return QStringLiteral("event");
        case NetworkIdRole:
            return conversation->key.networkId;
        case OriginRole:
            return QStringLiteral("live");
        default:
            return {};
        }
    }

    if (row.storeIndex < 0
        || row.storeIndex >= int(conversation->messages.size()))
        return {};

    const IrcReducedMessage& message =
        conversation->messages[size_t(row.storeIndex)];
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
    case OriginRole:
        return message.origin == IrcOrigin::Replay
            ? QStringLiteral("replay")
            : QStringLiteral("live");
    case MsgidRole:
        return message.msgid.value;
    default:
        return {};
    }
}

QHash<int, QByteArray> MessageListModel::staticRoleNames()
{
    return {
        {AuthorRole, "author"},
        {TimeRole, "time"},
        {BodyRole, "body"},
        {KindRole, "kind"},
        {NetworkIdRole, "networkId"},
        {OriginRole, "origin"},
        {MsgidRole, "msgid"},
    };
}

QHash<int, QByteArray> MessageListModel::roleNames() const
{
    return staticRoleNames();
}

QString MessageListModel::field(int row, const QString& name) const
{
    static const QHash<QString, int> roles = [] {
        QHash<QString, int> byName;
        const QHash<int, QByteArray> names = MessageListModel::staticRoleNames();
        for (auto it = names.cbegin(); it != names.cend(); ++it)
            byName.insert(QString::fromUtf8(it.value()), it.key());
        return byName;
    }();
    const auto role = roles.constFind(name);
    if (role == roles.cend())
        return {};
    const QVariant value = data(index(row, 0), role.value());
    return value.isValid() ? value.toString() : QString{};
}

void MessageListModel::notifySeparatorRows()
{
    for (int row = 0; row < int(m_view.size()); ++row) {
        if (m_view[size_t(row)].type != VisualRow::Type::Separator)
            continue;
        const QModelIndex idx = index(row, 0);
        emit dataChanged(idx, idx);
    }
}

void MessageListModel::reload()
{
    const IrcConversationState *conversation =
        m_selected ? m_reducer.find(*m_selected) : nullptr;
    std::vector<VisualRow> next = buildView(conversation);
    const int trimmed = conversation ? conversation->trimmed : 0;
    const int spliceEpoch = conversation ? conversation->spliceEpoch : 0;

    const bool sameConversation = m_selected.has_value() == m_loaded.has_value()
        && (!m_selected || *m_selected == *m_loaded)
        && spliceEpoch == m_spliceEpoch;
    if (sameConversation) {
        int storeRemoved = trimmed - m_trimmed;
        if (storeRemoved < 0)
            storeRemoved = 0;
        const int visualRemoved = visualPrefixToRemove(m_view, storeRemoved);
        if (visualRemoved > 0) {
            beginRemoveRows(QModelIndex(), 0, visualRemoved - 1);
            m_view.erase(m_view.begin(),
                         m_view.begin() + visualRemoved);
            const int keep = int(m_view.size());
            for (int i = 0; i < keep && i < int(next.size()); ++i)
                m_view[size_t(i)] = next[size_t(i)];
            m_trimmed = trimmed;
            endRemoveRows();
        }
        if (int(next.size()) > int(m_view.size())) {
            beginInsertRows(QModelIndex(), int(m_view.size()),
                            int(next.size()) - 1);
            m_view = std::move(next);
            m_trimmed = trimmed;
            endInsertRows();
            notifySeparatorRows();
            return;
        }
        if (int(next.size()) == int(m_view.size())) {
            m_view = std::move(next);
            m_trimmed = trimmed;
            if (!m_view.empty()) {
                emit dataChanged(index(0, 0),
                                 index(int(m_view.size()) - 1, 0));
            }
            return;
        }
    }

    beginResetModel();
    m_view = std::move(next);
    m_loaded = m_selected;
    m_trimmed = trimmed;
    m_spliceEpoch = spliceEpoch;
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
