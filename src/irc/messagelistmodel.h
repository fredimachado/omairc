#pragma once

#include "ircevent.h"

#include <QAbstractListModel>
#include <QByteArray>
#include <QHash>
#include <QString>
#include <QVariant>

#include <optional>

class IrcEventReducer;

class MessageListModel : public QAbstractListModel
{
    Q_OBJECT

public:
    enum Role {
        AuthorRole = Qt::UserRole + 1,
        TimeRole,
        BodyRole,
        KindRole,
        NetworkIdRole,
        OriginRole,
    };

    explicit MessageListModel(IrcEventReducer& reducer, QObject *parent = nullptr);

    int rowCount(const QModelIndex& parent = QModelIndex()) const override;
    QVariant data(const QModelIndex& index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;

    void reload();
    void setSelected(const IrcConversationKey& key);
    void select(const IrcConversationKey& key);
    void clearSelection();

private:
    IrcEventReducer& m_reducer;
    std::optional<IrcConversationKey> m_selected;
    std::optional<IrcConversationKey> m_loaded;
    int m_count = 0;
    int m_trimmed = 0;
    int m_spliceEpoch = 0;
};
