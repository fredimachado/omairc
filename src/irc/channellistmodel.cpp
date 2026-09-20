#include "channellistmodel.h"

#include <algorithm>

namespace
{
bool rowLess(const IrcChannelListRow& left, const IrcChannelListRow& right)
{
    if (left.users != right.users)
        return left.users > right.users;
    return QString::compare(left.channel, right.channel, Qt::CaseInsensitive) < 0;
}
}

ChannelListModel::ChannelListModel(QObject *parent)
    : QAbstractListModel(parent)
{
    setObjectName(QStringLiteral("channelList"));
}

QHash<int, QByteArray> ChannelListModel::staticRoleNames()
{
    return {
        {ChannelRole, "channel"},
        {UsersRole, "users"},
        {TopicRole, "topic"},
        {LabelRole, "label"},
    };
}

int ChannelListModel::rowCount(const QModelIndex& parent) const
{
    return parent.isValid() ? 0 : m_visible.size();
}

QVariant ChannelListModel::data(const QModelIndex& index, int role) const
{
    if (!index.isValid() || index.row() < 0 || index.row() >= m_visible.size())
        return {};
    const IrcChannelListRow& row = m_source.at(m_visible.at(index.row()));
    switch (role) {
    case ChannelRole:
        return row.channel;
    case UsersRole:
        return row.users;
    case TopicRole:
        return row.topic;
    case LabelRole:
        return rowLabel(row);
    default:
        return {};
    }
}

QHash<int, QByteArray> ChannelListModel::roleNames() const
{
    return staticRoleNames();
}

QVariantMap ChannelListModel::get(int row) const
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

QVariant ChannelListModel::field(int row, const QString& name) const
{
    static const QHash<QString, int> roles = [] {
        QHash<QString, int> byName;
        const QHash<int, QByteArray> names = ChannelListModel::staticRoleNames();
        for (auto it = names.cbegin(); it != names.cend(); ++it)
            byName.insert(QString::fromUtf8(it.value()), it.key());
        return byName;
    }();
    const auto role = roles.constFind(name);
    if (role == roles.cend())
        return {};
    return data(index(row, 0), role.value());
}

QString ChannelListModel::filter() const
{
    return m_filter;
}

void ChannelListModel::setFilter(const QString& filter)
{
    if (m_filter == filter)
        return;
    m_filter = filter;
    emit filterChanged();
    rebuildVisible(true);
}

bool ChannelListModel::loading() const
{
    return m_loading;
}

bool ChannelListModel::complete() const
{
    return m_complete;
}

bool ChannelListModel::cached() const
{
    return m_cached;
}

QString ChannelListModel::networkId() const
{
    return m_networkId;
}

QString ChannelListModel::mask() const
{
    return m_mask;
}

int ChannelListModel::sourceCount() const
{
    return m_source.size();
}

const QVector<IrcChannelListRow>& ChannelListModel::sourceRows() const
{
    return m_source;
}

void ChannelListModel::show(const QString& networkId,
                            const QString& mask,
                            QVector<IrcChannelListRow> rows,
                            bool complete,
                            bool loading,
                            bool cached)
{
    std::sort(rows.begin(), rows.end(), rowLess);
    beginResetModel();
    m_networkId = networkId;
    m_mask = mask;
    m_source = std::move(rows);
    m_complete = complete;
    m_loading = loading;
    m_cached = cached;
    m_visible.clear();
    m_visible.reserve(m_source.size());
    for (int i = 0; i < m_source.size(); ++i) {
        if (matches(m_source.at(i)))
            m_visible.append(i);
    }
    endResetModel();
    emit stateChanged();
}

void ChannelListModel::beginLoad(const QString& networkId, const QString& mask)
{
    beginResetModel();
    m_networkId = networkId;
    m_mask = mask;
    m_source.clear();
    m_visible.clear();
    m_complete = false;
    m_loading = true;
    m_cached = false;
    endResetModel();
    emit stateChanged();
}

void ChannelListModel::appendRow(IrcChannelListRow row)
{
    const int sourceIndex = m_source.size();
    m_source.append(std::move(row));
    if (matches(m_source.last())) {
        const int visibleIndex = m_visible.size();
        beginInsertRows(QModelIndex(), visibleIndex, visibleIndex);
        m_visible.append(sourceIndex);
        endInsertRows();
    }
    emit stateChanged();
}

void ChannelListModel::finish(bool complete)
{
    std::sort(m_source.begin(), m_source.end(), rowLess);
    m_complete = complete;
    m_loading = false;
    m_cached = false;
    rebuildVisible(true);
    emit stateChanged();
}

void ChannelListModel::clear()
{
    if (m_source.isEmpty() && m_networkId.isEmpty() && !m_loading && !m_complete)
        return;
    beginResetModel();
    m_source.clear();
    m_visible.clear();
    m_networkId.clear();
    m_mask.clear();
    m_loading = false;
    m_complete = false;
    m_cached = false;
    endResetModel();
    emit stateChanged();
}

bool ChannelListModel::matches(const IrcChannelListRow& row) const
{
    const QString query = m_filter.trimmed().toLower();
    if (query.isEmpty())
        return true;
    return row.channel.toLower().contains(query)
        || row.topic.toLower().contains(query);
}

void ChannelListModel::rebuildVisible(bool resetAlways)
{
    QVector<int> next;
    next.reserve(m_source.size());
    for (int i = 0; i < m_source.size(); ++i) {
        if (matches(m_source.at(i)))
            next.append(i);
    }
    std::sort(next.begin(), next.end(), [this](int left, int right) {
        return rowLess(m_source.at(left), m_source.at(right));
    });
    if (!resetAlways && next == m_visible)
        return;
    beginResetModel();
    m_visible = std::move(next);
    endResetModel();
}

QString ChannelListModel::rowLabel(const IrcChannelListRow& row) const
{
    QString label = row.channel;
    label += QLatin1Char(' ');
    label += QString::number(row.users);
    if (!row.topic.isEmpty()) {
        label += QLatin1Char(' ');
        label += row.topic;
    }
    return label;
}
