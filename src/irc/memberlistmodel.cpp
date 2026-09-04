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

    beginResetModel();
    m_nicks = std::move(nicks);
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
