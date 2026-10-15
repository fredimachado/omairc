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
        AuthorAccountRole,
        // Transcript wash for a nick or /highlight hit. Not the sidebar
        // `mention` badge, which also flags every direct message.
        MentionedRole,
        SequenceRole,
        OnlyVisibleToYouRole,
    };

    static QHash<int, QByteArray> staticRoleNames();

    explicit MessageListModel(IrcEventReducer& reducer, QObject *parent = nullptr);

    int rowCount(const QModelIndex& parent = QModelIndex()) const override;
    QVariant data(const QModelIndex& index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;
    Q_INVOKABLE QString field(int row, const QString& name) const;
    Q_INVOKABLE int unreadMarkRow() const;
    // Visual rows include date separators and the unread mark. A store row
    // maps to one transcript message; the other rows return -1.
    int storeIndexAt(int visualRow) const;
    int visualRowForStoreIndex(int storeIndex) const;

    void reload();
    void notifyMentioned();
    void setSelected(const IrcConversationKey& key);
    void select(const IrcConversationKey& key);
    void clearSelection();
#ifdef OMAIRC_TEST
    // Date rules and their Today / Yesterday labels use this day instead of
    // the wall clock. Production reload still reads QDate::currentDate().
    void setTranscriptTodayForTest(const QDate& day);
#endif

private:
    struct VisualRow {
        enum class Type { Store, DateSeparator, UnreadMark };
        Type type = Type::Store;
        int storeIndex = 0;
        QDate date;
    };

    static std::vector<VisualRow> buildView(const IrcConversationState *conversation,
                                            const QDate& today);
    // Inclusive. last < first means the trim removes no visual rows.
    // A prefix trim starts at 0. A trim that stops inside the leading
    // non-today day starts at 1 so that date rule stays at row 0.
    struct TrimSpan {
        int first = 0;
        int last = -1;
        bool empty() const { return last < first; }
    };
    static int visualPrefixToRemove(const std::vector<VisualRow>& view,
                                    int storeRemoved,
                                    const QDate& today);
    static TrimSpan visualTrimSpan(const std::vector<VisualRow>& view,
                                   int storeRemoved,
                                   const QDate& today);
    // The row at firstKept, and next's first row, are the same leading date
    // rule: both absent, or both separators for one calendar day.
    static bool sameLeadingDate(const std::vector<VisualRow>& current,
                                int firstKept,
                                const std::vector<VisualRow>& next);
    void notifySeparatorRows();
    QDate transcriptToday() const;

    IrcEventReducer& m_reducer;
    std::optional<IrcConversationKey> m_selected;
    std::optional<IrcConversationKey> m_loaded;
    std::vector<VisualRow> m_view;
    int m_trimmed = 0;
    int m_spliceEpoch = 0;
    std::optional<qint64> m_unreadMark;
#ifdef OMAIRC_TEST
    std::optional<QDate> m_transcriptToday;
#endif
};
