#pragma once

#include "ircevent.h"

#include <QAbstractListModel>
#include <QByteArray>
#include <QDate>
#include <QHash>
#include <QString>
#include <QVariant>

#include <optional>
#include <vector>

class IrcEventReducer;
struct IrcConversationState;

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
        MsgidRole,
        AuthorAvatarRole,
        AuthorBotRole,
        MentionedRole,
    };

    static QHash<int, QByteArray> staticRoleNames();

    explicit MessageListModel(IrcEventReducer& reducer, QObject *parent = nullptr);

    int rowCount(const QModelIndex& parent = QModelIndex()) const override;
    QVariant data(const QModelIndex& index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;
    Q_INVOKABLE QString field(int row, const QString& name) const;
    Q_INVOKABLE int unreadMarkRow() const;

    void reload();
    void notifyMentioned();
    void setSelected(const IrcConversationKey& key);
    void select(const IrcConversationKey& key);
    void clearSelection();

private:
    struct VisualRow {
        enum class Type { Store, DateSeparator, UnreadMark };
        Type type = Type::Store;
        int storeIndex = 0;
        QDate date;
    };

    static std::vector<VisualRow> buildView(const IrcConversationState *conversation);
    static int visualPrefixToRemove(const std::vector<VisualRow>& view, int storeRemoved);
    void notifySeparatorRows();

    IrcEventReducer& m_reducer;
    std::optional<IrcConversationKey> m_selected;
    std::optional<IrcConversationKey> m_loaded;
    std::vector<VisualRow> m_view;
    int m_trimmed = 0;
    int m_spliceEpoch = 0;
    std::optional<qint64> m_unreadMark;
};
