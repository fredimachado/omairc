#include "memberlistmodel.h"

#include "irceventreducer.h"

#include <QSet>

namespace
{
QSet<QString> nicksOf(const QVector<IrcOrderedMember>& members)
{
    QSet<QString> nicks;
    nicks.reserve(members.size());
    for (const IrcOrderedMember& member : members)
        nicks.insert(member.nick);
    return nicks;
}
}

MemberListModel::MemberListModel(IrcEventReducer& reducer, QObject *parent)
    : QAbstractListModel(parent)
    , m_reducer(reducer)
{
}

int MemberListModel::rowCount(const QModelIndex& parent) const
{
    return parent.isValid() ? 0 : m_members.size();
}

QVariant MemberListModel::data(const QModelIndex& index, int role) const
{
    if (!index.isValid() || !m_selected || index.row() < 0 || index.row() >= m_members.size())
        return {};

    const std::optional<IrcMemberView> member =
        m_reducer.memberView(*m_selected, m_members.at(index.row()).nick);
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
    case AvatarRole:
        return member->avatar;
    case BotRole:
        return member->bot;
    case AccountRole:
        return member->account;
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
        {AvatarRole, "avatar"},
        {BotRole, "bot"},
        {AccountRole, "account"},
    };
}

void MemberListModel::reload()
{
    QVector<IrcOrderedMember> members;
    if (m_selected)
        members = m_reducer.orderedMembers(*m_selected);

    const bool sameConversation = m_selected.has_value() == m_loaded.has_value()
        && (!m_selected || *m_selected == *m_loaded);
    if (!sameConversation) {
        resetMembers(std::move(members));
        return;
    }
    syncMembers(std::move(members));
}

void MemberListModel::resetMembers(QVector<IrcOrderedMember> members)
{
    beginResetModel();
    m_members = std::move(members);
    rebuildRowIndex();
    m_loaded = m_selected;
    endResetModel();
}

void MemberListModel::syncMembers(QVector<IrcOrderedMember> members)
{
    if (members == m_members) {
        if (!m_members.isEmpty())
            emit dataChanged(index(0, 0), index(m_members.size() - 1, 0),
                             {LabelRole, StatusRole, AwayRole, AvatarRole, BotRole,
                              AccountRole});
        return;
    }

    // Departures. A nick that quit, parted, or was kicked is gone from the
    // new order.
    const QSet<QString> wanted = nicksOf(members);
    QSet<QString> present = nicksOf(m_members);
    for (int row = m_members.size() - 1; row >= 0; --row) {
        const QString& nick = m_members.at(row).nick;
        if (wanted.contains(nick))
            continue;
        present.remove(nick);
        beginRemoveRows(QModelIndex(), row, row);
        m_members.removeAt(row);
        endRemoveRows();
    }

    // Arrivals, seeded beside their sorted position.
    for (const IrcOrderedMember& member : members) {
        if (present.contains(member.nick))
            continue;
        present.insert(member.nick);
        int at = 0;
        while (at < m_members.size() && !(member < m_members.at(at)))
            ++at;
        beginInsertRows(QModelIndex(), at, at);
        m_members.insert(at, member);
        endInsertRows();
    }

    // Rank order. The two lists now hold the same nicks, so walking the new
    // order and moving each row into place sorts the survivors without
    // resetting the model. A row that keeps its place keeps its delegate; only
    // the rows that moved or changed rank refresh their roles.
    for (int row = 0; row < members.size(); ++row) {
        const IrcOrderedMember& target = members.at(row);
        int from = row;
        while (from < m_members.size() && m_members.at(from).nick != target.nick)
            ++from;
        if (from >= m_members.size())
            continue;

        const bool moved = from != row;
        if (moved) {
            beginMoveRows(QModelIndex(), from, from, QModelIndex(), row);
            m_members.move(from, row);
            endMoveRows();
        }
        if (moved || m_members.at(row).priority != target.priority) {
            m_members[row].priority = target.priority;
            emit dataChanged(index(row, 0), index(row, 0),
                             {LabelRole, StatusRole, AwayRole, AvatarRole, BotRole,
                              AccountRole});
        }
    }
    rebuildRowIndex();
}

void MemberListModel::rebuildRowIndex()
{
    m_rowByNick.clear();
    m_rowByNick.reserve(m_members.size());
    for (int row = 0; row < m_members.size(); ++row)
        m_rowByNick.insert(m_members.at(row).nick, row);
}

int MemberListModel::rowForNick(const QString& normalizedNick) const
{
    const auto found = m_rowByNick.constFind(normalizedNick);
    return found == m_rowByNick.cend() ? -1 : *found;
}

void MemberListModel::touch(const QString& normalizedNick)
{
    const int row = rowForNick(normalizedNick);
    if (row < 0)
        return;
    emit dataChanged(index(row, 0), index(row, 0),
                     {AwayRole, StatusRole, LabelRole, AvatarRole, BotRole,
                      AccountRole});
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
