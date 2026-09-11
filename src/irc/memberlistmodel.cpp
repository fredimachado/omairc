#include "memberlistmodel.h"

#include "irceventreducer.h"

MemberListModel::MemberListModel(IrcEventReducer& reducer, QObject *parent)
    : QAbstractListModel(parent)
    , m_reducer(reducer)
{
}

int MemberListModel::rowCount(const QModelIndex& parent) const
{
    return parent.isValid() ? 0 : m_nicks.size();
}

QVariant MemberListModel::data(const QModelIndex& index, int role) const
{
    if (!index.isValid() || !m_selected || index.row() < 0 || index.row() >= m_nicks.size())
        return {};

    const std::optional<IrcMemberView> member =
        m_reducer.memberView(*m_selected, m_nicks.at(index.row()));
    if (!member)
        return {};

    switch (role) {
    case NickRole:
        return member->nick;
    case LabelRole:
        return member->label;
    case StatusRole:
        return member->status;
    case AwayRole:
        return member->isAway();
    case NetworkIdRole:
        return m_selected->networkId;
    default:
        return {};
    }
}

QHash<int, QByteArray> MemberListModel::roleNames() const
{
    return {
        {NickRole, "nick"},
        {LabelRole, "label"},
        {StatusRole, "status"},
        {AwayRole, "away"},
        {NetworkIdRole, "networkId"},
    };
}

QVector<QString> MemberListModel::collectNicks() const
{
    QVector<QString> nicks;
    if (!m_selected)
        return nicks;
    const IrcConversationState *conversation = m_reducer.find(*m_selected);
    const IrcChannelState *channel = conversation ? conversation->channel() : nullptr;
    if (!channel)
        return nicks;
    nicks.reserve(int(channel->members.size()));
    for (const auto& entry : channel->members)
        nicks.append(entry.first);
    return nicks;
}

bool MemberListModel::syncRows(const QVector<QString>& nicks)
{
    if (nicks == m_nicks) {
        if (nicks.isEmpty())
            return true;
        emit dataChanged(index(0), index(nicks.size() - 1),
                         {NickRole, LabelRole, StatusRole, AwayRole});
        return true;
    }
    if (qAbs(nicks.size() - m_nicks.size()) > 1)
        return false;

    QVector<QString> removed;
    QVector<QString> added;
    for (const QString& nick : m_nicks) {
        if (!nicks.contains(nick))
            removed.append(nick);
    }
    for (const QString& nick : nicks) {
        if (!m_rowByNick.contains(nick))
            added.append(nick);
    }
    if (removed.size() > 1 || added.size() > 1)
        return false;

    if (removed.size() == 1 && added.isEmpty()) {
        const int row = m_rowByNick.value(removed.front());
        beginRemoveRows(QModelIndex(), row, row);
        m_nicks.removeAt(row);
        rebuildRowIndex();
        endRemoveRows();
        return true;
    }
    if (added.size() == 1 && removed.isEmpty()) {
        const int row = nicks.indexOf(added.front());
        beginInsertRows(QModelIndex(), row, row);
        m_nicks.insert(row, added.front());
        rebuildRowIndex();
        endInsertRows();
        return true;
    }
    if (added.size() == 1 && removed.size() == 1) {
        const int from = m_rowByNick.value(removed.front());
        const int to = nicks.indexOf(added.front());
        m_nicks[from] = added.front();
        rebuildRowIndex();
        const QModelIndex changed = index(from);
        emit dataChanged(changed, changed, {NickRole, LabelRole, StatusRole, AwayRole});
        if (from == to)
            return true;
        const int destination = to > from ? to + 1 : to;
        beginMoveRows(QModelIndex(), from, from, QModelIndex(), destination);
        m_nicks.move(from, to);
        rebuildRowIndex();
        endMoveRows();
        return true;
    }
    return false;
}

void MemberListModel::reload()
{
    QVector<QString> nicks = collectNicks();
    const bool sameConversation = m_selected.has_value() == m_loaded.has_value()
        && (!m_selected || *m_selected == *m_loaded);
    if (sameConversation && syncRows(nicks))
        return;

    beginResetModel();
    m_nicks = std::move(nicks);
    m_loaded = m_selected;
    rebuildRowIndex();
    endResetModel();
}

void MemberListModel::rebuildRowIndex()
{
    m_rowByNick.clear();
    m_rowByNick.reserve(m_nicks.size());
    for (int row = 0; row < m_nicks.size(); ++row)
        m_rowByNick.insert(m_nicks.at(row), row);
}

void MemberListModel::touch(const QString& normalizedNick)
{
    const auto found = m_rowByNick.constFind(normalizedNick);
    if (found == m_rowByNick.cend())
        return;
    const QModelIndex row = index(*found, 0);
    emit dataChanged(row, row, {AwayRole, StatusRole, LabelRole});
}

void MemberListModel::setSelected(const IrcConversationKey& key)
{
    m_selected = key;
}

void MemberListModel::select(const IrcConversationKey& key)
{
    setSelected(key);
    reload();
}

void MemberListModel::clearSelection()
{
    m_selected.reset();
    reload();
}
