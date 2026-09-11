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
        LabelRole,
        StatusRole,
        AwayRole,
        NetworkIdRole,
    };

    explicit MemberListModel(IrcEventReducer& reducer, QObject *parent = nullptr);

    int rowCount(const QModelIndex& parent = QModelIndex()) const override;
    QVariant data(const QModelIndex& index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;

    void reload();
    void setSelected(const IrcConversationKey& key);
    void select(const IrcConversationKey& key);
    void clearSelection();
    void touch(const QString& normalizedNick);

private:
    void rebuildRowIndex();

    void resetNicks(QVector<QString> nicks);
    void syncNicks(QVector<QString> nicks);

    IrcEventReducer& m_reducer;
    std::optional<IrcConversationKey> m_selected;
    std::optional<IrcConversationKey> m_loaded;
    QVector<QString> m_nicks;
    QHash<QString, int> m_rowByNick;
};
