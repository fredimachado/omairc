#pragma once

#include "irceventreducer.h"

#include <QAbstractListModel>
#include <QByteArray>
#include <QHash>
#include <QString>
#include <QStringList>
#include <QVariant>
#include <QVector>

#include <optional>

inline QString ircConversationId(const IrcConversationKey& key)
{
    return key.networkId + QLatin1Char('\n') + key.normalizedTarget;
}

inline std::optional<IrcConversationKey> ircParseConversationId(const QString& id)
{
    const int sep = id.indexOf(QLatin1Char('\n'));
    if (sep <= 0 || sep + 1 >= id.size())
        return std::nullopt;
    if (id.indexOf(QLatin1Char('\n'), sep + 1) >= 0)
        return std::nullopt;
    IrcConversationKey key;
    key.networkId = id.left(sep);
    key.normalizedTarget = id.mid(sep + 1);
    if (key.networkId.isEmpty() || key.normalizedTarget.isEmpty())
        return std::nullopt;
    return key;
}

QVector<IrcConversationKey> ircSidebarOrder(const IrcEventReducer& reducer);
QVector<IrcConversationKey> ircSidebarOrder(const IrcEventReducer& reducer,
                                            const QStringList& networkOrder);
std::optional<IrcConversationKey> ircNeighborAfterDrop(
    const QVector<IrcConversationKey>& ordered,
    const IrcConversationKey& dropping);

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
        ConversationNameRole,
    };

    explicit ConversationListModel(IrcEventReducer& reducer,
                                   QObject *parent = nullptr);

    int rowCount(const QModelIndex& parent = QModelIndex()) const override;
    QVariant data(const QModelIndex& index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;

    void reload();
    void select(const IrcConversationKey& key);
    void setNetworkOrder(const QStringList& networkOrder);

private:
    IrcEventReducer& m_reducer;
    QStringList m_networkOrder;
    QVector<IrcConversationKey> m_keys;
};
