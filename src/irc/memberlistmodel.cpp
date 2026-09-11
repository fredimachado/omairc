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

void MemberListModel::reload()
{
    QVector<QString> nicks;
    if (m_selected) {
        if (const IrcConversationState *conversation = m_reducer.find(*m_selected)) {
            if (const IrcChannelState *channel = conversation->channel()) {
                nicks.reserve(int(channel->members.size()));
                for (const auto& entry : channel->members)
                    nicks.append(entry.first);
            }
        }
    }

    const bool sameConversation = m_selected.has_value() == m_loaded.has_value()
        && (!m_selected || *m_selected == *m_loaded);
    if (!sameConversation) {
        resetNicks(std::move(nicks));
        return;
    }
    syncNicks(std::move(nicks));
}

void MemberListModel::resetNicks(QVector<QString> nicks)
{
    beginResetModel();
    m_nicks = std::move(nicks);
    rebuildRowIndex();
    m_loaded = m_selected;
    endResetModel();
}

void MemberListModel::syncNicks(QVector<QString> nicks)
{
    if (nicks == m_nicks) {
        if (!m_nicks.isEmpty())
            emit dataChanged(index(0, 0), index(m_nicks.size() - 1, 0),
                             {LabelRole, StatusRole, AwayRole});
        return;
    }

    int oldIndex = 0;
    int newIndex = 0;
    while (oldIndex < m_nicks.size() || newIndex < nicks.size()) {
        if (oldIndex == m_nicks.size()) {
            beginInsertRows(QModelIndex(), oldIndex, oldIndex);
            m_nicks.insert(oldIndex, nicks.at(newIndex));
            endInsertRows();
            ++oldIndex;
            ++newIndex;
            continue;
        }
        if (newIndex == nicks.size()) {
            beginRemoveRows(QModelIndex(), oldIndex, oldIndex);
            m_nicks.removeAt(oldIndex);
            endRemoveRows();
            continue;
        }
        const QString& oldNick = m_nicks.at(oldIndex);
        const QString& newNick = nicks.at(newIndex);
        if (oldNick == newNick) {
            ++oldIndex;
            ++newIndex;
            continue;
        }
        if (oldNick < newNick) {
            beginRemoveRows(QModelIndex(), oldIndex, oldIndex);
            m_nicks.removeAt(oldIndex);
            endRemoveRows();
            continue;
        }
        beginInsertRows(QModelIndex(), oldIndex, oldIndex);
        m_nicks.insert(oldIndex, newNick);
        endInsertRows();
        ++oldIndex;
        ++newIndex;
    }
    rebuildRowIndex();
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
