#pragma once

#include "irceventreducer.h"

#include <QAbstractListModel>
#include <QByteArray>
#include <QHash>
#include <QString>
#include <QVariant>
#include <QVector>

inline QString ircConversationId(const IrcConversationKey& key)
{
    return key.networkId + QLatin1Char('\n') + key.normalizedTarget;
}

class ConversationListModel : public QAbstractListModel
{
    Q_OBJECT

public:
    enum Role {
        ConversationRole = Qt::UserRole + 1,
        UnreadRole,
        MentionRole,
        DirectRole,
        NetworkIdRole,
        ConversationIdRole,
    };

    explicit ConversationListModel(IrcEventReducer& reducer,
                                   QObject *parent = nullptr);

    int rowCount(const QModelIndex& parent = QModelIndex()) const override;
    QVariant data(const QModelIndex& index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;

    void reload();
    void select(const IrcConversationKey& key);

private:
    IrcEventReducer& m_reducer;
    QVector<IrcConversationKey> m_keys;
};
