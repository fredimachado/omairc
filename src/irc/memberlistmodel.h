#pragma once

#include "irceventreducer.h"

#include <QAbstractListModel>
#include <QByteArray>
#include <QHash>
#include <QString>
#include <QVariant>
#include <QVector>

#include <optional>

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
        AvatarRole,
        BotRole,
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

    void resetMembers(QVector<IrcOrderedMember> members);
    void syncMembers(QVector<IrcOrderedMember> members);

    int rowForNick(const QString& normalizedNick) const;

    IrcEventReducer& m_reducer;
    std::optional<IrcConversationKey> m_selected;
    std::optional<IrcConversationKey> m_loaded;
    QVector<IrcOrderedMember> m_members;
    QHash<QString, int> m_rowByNick;
};
