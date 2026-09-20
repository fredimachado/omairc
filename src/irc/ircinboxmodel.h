#pragma once

#include "ircinbox.h"

#include <QAbstractListModel>
#include <QByteArray>
#include <QHash>
#include <QVariant>
#include <QVector>

class IrcInboxModel : public QAbstractListModel
{
    Q_OBJECT

public:
    enum Role
    {
        KindRole = Qt::UserRole + 1,
        NetworkIdRole,
        ActorRole,
        TargetRole,
        PreviewRole,
        MsgidRole,
        LabelRole,
    };

    explicit IrcInboxModel(QObject *parent = nullptr);

    static QHash<int, QByteArray> staticRoleNames();

    int rowCount(const QModelIndex& parent = QModelIndex()) const override;
    QVariant data(const QModelIndex& index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;
    Q_INVOKABLE QVariantMap get(int row) const;
    Q_INVOKABLE QString field(int row, const QString& name) const;

    void sync(const IrcInbox& inbox);

private:
    static QString kindName(IrcInboxKind kind);
    static QString labelFor(const IrcInboxItem& item);

    QVector<IrcInboxItem> m_items;
};
