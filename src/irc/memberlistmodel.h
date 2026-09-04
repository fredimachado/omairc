#pragma once

#include "ircevent.h"

#include <QAbstractListModel>
#include <QByteArray>
#include <QHash>
#include <QString>
#include <QVariant>
#include <QVector>

#include <optional>

class IrcEventReducer;

class MemberListModel : public QAbstractListModel
{
    Q_OBJECT

public:
    enum Role {
        NickRole = Qt::UserRole + 1,
        StatusRole,
        AwayRole,
        NetworkIdRole,
    };

    explicit MemberListModel(IrcEventReducer& reducer, QObject *parent = nullptr);

    int rowCount(const QModelIndex& parent = QModelIndex()) const override;
    QVariant data(const QModelIndex& index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;

    void reload();
    void select(const IrcConversationKey& key);
    void clearSelection();

private:
    IrcEventReducer& m_reducer;
    std::optional<IrcConversationKey> m_selected;
    QVector<QString> m_nicks;
};
