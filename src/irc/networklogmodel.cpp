#include "networklogmodel.h"

#include "ircnetworklog.h"

namespace
{
QString sourceName(IrcLogSource source)
{
    switch (source) {
    case IrcLogSource::Client:
        return QStringLiteral("client");
    case IrcLogSource::Local:
        return QStringLiteral("local");
    case IrcLogSource::Server:
        break;
    }
    return QStringLiteral("server");
}

QString severityName(IrcLogSeverity severity)
{
    switch (severity) {
    case IrcLogSeverity::Trace:
        return QStringLiteral("trace");
    case IrcLogSeverity::Alert:
        return QStringLiteral("alert");
    case IrcLogSeverity::Info:
        break;
    }
    return QStringLiteral("info");
}
}

NetworkLogModel::NetworkLogModel(IrcNetworkLog& log, QObject *parent)
    : QAbstractListModel(parent)
    , m_log(log)
{
    connect(&m_log, &IrcNetworkLog::appended, this, &NetworkLogModel::onAppended);
    connect(&m_log, &IrcNetworkLog::trimmed, this, &NetworkLogModel::onTrimmed);
    connect(&m_log, &IrcNetworkLog::cleared, this, &NetworkLogModel::onCleared);
}

int NetworkLogModel::rowCount(const QModelIndex& parent) const
{
    return parent.isValid() ? 0 : m_log.count(m_networkId);
}

QVariant NetworkLogModel::data(const QModelIndex& index, int role) const
{
    if (!index.isValid() || index.row() < 0 || index.row() >= rowCount())
        return {};

    const IrcStatusEntry& entry = m_log.at(m_networkId, index.row());
    switch (role) {
    case TimeRole:
        return entry.timestamp().toLocalTime().toString(QStringLiteral("hh:mm:ss"));
    case LabelRole:
        return entry.label();
    case TextRole:
        return entry.text();
    case SourceRole:
        return sourceName(entry.source());
    case SeverityRole:
        return severityName(entry.severity());
    default:
        return {};
    }
}

QHash<int, QByteArray> NetworkLogModel::roleNames() const
{
    return {
        {TimeRole, "time"},
        {LabelRole, "label"},
        {TextRole, "text"},
        {SourceRole, "source"},
        {SeverityRole, "severity"},
    };
}

void NetworkLogModel::show(const QString& networkId)
{
    if (m_networkId == networkId)
        return;
    beginResetModel();
    m_networkId = networkId;
    endResetModel();
}

void NetworkLogModel::onAppended(const QString& networkId, int row)
{
    if (networkId != m_networkId)
        return;
    beginInsertRows(QModelIndex(), row, row);
    endInsertRows();
}

void NetworkLogModel::onTrimmed(const QString& networkId, int removedFromFront)
{
    if (networkId != m_networkId || removedFromFront <= 0)
        return;
    beginRemoveRows(QModelIndex(), 0, removedFromFront - 1);
    endRemoveRows();
}

void NetworkLogModel::onCleared(const QString& networkId)
{
    if (networkId != m_networkId)
        return;
    beginResetModel();
    endResetModel();
}
