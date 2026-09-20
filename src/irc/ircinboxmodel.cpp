#include "ircinboxmodel.h"

IrcInboxModel::IrcInboxModel(QObject *parent)
    : QAbstractListModel(parent)
{
}

int IrcInboxModel::rowCount(const QModelIndex& parent) const
{
    return parent.isValid() ? 0 : m_items.size();
}

QVariant IrcInboxModel::data(const QModelIndex& index, int role) const
{
    if (!index.isValid() || index.row() < 0 || index.row() >= m_items.size())
        return {};

    const IrcInboxItem& item = m_items.at(index.row());
    switch (role) {
    case KindRole:
        return kindName(item.kind);
    case NetworkIdRole:
        return item.networkId;
    case ActorRole:
        return item.actor;
    case TargetRole:
        return item.target;
    case PreviewRole:
        return item.preview;
    case MsgidRole:
        return item.msgid.value;
    case LabelRole:
        return labelFor(item);
    default:
        return {};
    }
}

QHash<int, QByteArray> IrcInboxModel::staticRoleNames()
{
    return {
        {KindRole, "kind"},
        {NetworkIdRole, "networkId"},
        {ActorRole, "actor"},
        {TargetRole, "target"},
        {PreviewRole, "preview"},
        {MsgidRole, "msgid"},
        {LabelRole, "label"},
    };
}

QHash<int, QByteArray> IrcInboxModel::roleNames() const
{
    return staticRoleNames();
}

QVariantMap IrcInboxModel::get(int row) const
{
    if (row < 0 || row >= m_items.size())
        return {};
    const IrcInboxItem& item = m_items.at(row);
    return {
        {QStringLiteral("kind"), kindName(item.kind)},
        {QStringLiteral("networkId"), item.networkId},
        {QStringLiteral("actor"), item.actor},
        {QStringLiteral("target"), item.target},
        {QStringLiteral("preview"), item.preview},
        {QStringLiteral("msgid"), item.msgid.value},
        {QStringLiteral("label"), labelFor(item)},
    };
}

QString IrcInboxModel::field(int row, const QString& name) const
{
    static const QHash<QString, int> roles = [] {
        QHash<QString, int> byName;
        const QHash<int, QByteArray> names = IrcInboxModel::staticRoleNames();
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

void IrcInboxModel::sync(const IrcInbox& inbox)
{
    beginResetModel();
    m_items.clear();
    for (int index = 0; index < inbox.count(); ++index)
        m_items.append(inbox.at(index));
    endResetModel();
}

QString IrcInboxModel::kindName(IrcInboxKind kind)
{
    switch (kind) {
    case IrcInboxKind::Mention:
        return QStringLiteral("mention");
    case IrcInboxKind::Highlight:
        return QStringLiteral("highlight");
    case IrcInboxKind::Direct:
        return QStringLiteral("direct");
    case IrcInboxKind::Invite:
        return QStringLiteral("invite");
    case IrcInboxKind::MonitorOnline:
        return QStringLiteral("monitorOnline");
    case IrcInboxKind::Kick:
        return QStringLiteral("kick");
    }
    return {};
}

QString IrcInboxModel::labelFor(const IrcInboxItem& item)
{
    switch (item.kind) {
    case IrcInboxKind::Mention:
        return QStringLiteral("%1 mentioned you in %2")
            .arg(item.actor, item.target);
    case IrcInboxKind::Highlight:
        return QStringLiteral("Highlight in %1").arg(item.target);
    case IrcInboxKind::Direct:
        return QStringLiteral("Message from %1").arg(item.actor);
    case IrcInboxKind::Invite:
        return QStringLiteral("%1 invited you to %2")
            .arg(item.actor, item.target);
    case IrcInboxKind::MonitorOnline:
        return QStringLiteral("%1 is online").arg(item.actor);
    case IrcInboxKind::Kick:
        return QStringLiteral("You were kicked from %1").arg(item.target);
    }
    return {};
}
