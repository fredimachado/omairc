#include "messagelistmodel.h"

#include "irceventreducer.h"

#include <QLocale>

namespace
{
QString kindString(IrcMessageKind kind)
{
    switch (kind) {
    case IrcMessageKind::Action:
        return QStringLiteral("action");
    case IrcMessageKind::Event:
    case IrcMessageKind::Error:
        return QStringLiteral("event");
    case IrcMessageKind::Whois:
        return QStringLiteral("whois");
    case IrcMessageKind::Message:
        return QStringLiteral("message");
    case IrcMessageKind::Notice:
        return QStringLiteral("notice");
    }
    return QStringLiteral("message");
}

QString displayTime(const IrcReducedMessage& message)
{
    if (message.kind == IrcMessageKind::Event
        || message.kind == IrcMessageKind::Whois
        || !message.timestamp.isValid())
        return {};
    return message.timestamp.toLocalTime().toString(QStringLiteral("HH:mm"));
}

std::optional<QDate> assignedDate(const IrcReducedMessage& message,
                                  const std::optional<QDate>& previous,
                                  const QDate& today)
{
    if (message.timestamp.isValid())
        return message.timestamp.toLocalTime().date();
    if (message.origin == IrcOrigin::Live)
        return today;
    return previous;
}

QString dateLabel(const QDate& date, const QDate& today)
{
    if (date == today)
        return QStringLiteral("Today");
    if (date == today.addDays(-1))
        return QStringLiteral("Yesterday");
    return QLocale().toString(date, QLocale::ShortFormat);
}
}

std::vector<MessageListModel::VisualRow> MessageListModel::buildView(
    const IrcConversationState *conversation,
    const QDate& today)
{
    std::vector<VisualRow> view;
    if (!conversation)
        return view;
    view.reserve(conversation->messages.size() + 8);
    std::optional<QDate> previous;
    const std::optional<qint64> unreadMark = conversation->unreadMark;
    // The first day is labeled only when it is not today, so a backlog that
    // never crosses midnight still says which day it is. A later day is
    // labeled whenever it differs, including today after an older day.
    // Rebuilding the view on a history splice moves or drops that top rule.
    for (int i = 0; i < int(conversation->messages.size()); ++i) {
        const std::optional<QDate> date =
            assignedDate(conversation->messages[size_t(i)], previous, today);
        const bool dayChange = date && previous && *date != *previous;
        const bool leadingBacklog = date && !previous && *date != today;
        if (dayChange || leadingBacklog)
            view.push_back({VisualRow::Type::DateSeparator, 0, *date});
        if (unreadMark && conversation->messages[size_t(i)].sequence == *unreadMark
            && !view.empty())
            view.push_back({VisualRow::Type::UnreadMark, 0, {}});
        view.push_back({VisualRow::Type::Store, i, {}});
        if (date)
            previous = date;
    }
    return view;
}

int MessageListModel::visualPrefixToRemove(const std::vector<VisualRow>& view,
                                           int storeRemoved,
                                           const QDate& today)
{
    if (storeRemoved <= 0)
        return 0;
    int removed = 0;
    for (int i = 0; i < int(view.size()); ++i) {
        const VisualRow& row = view[size_t(i)];
        if (row.type == VisualRow::Type::Store) {
            if (row.storeIndex < storeRemoved)
                ++removed;
            else
                break;
            continue;
        }
        int following = -1;
        for (int j = i + 1; j < int(view.size()); ++j) {
            if (view[size_t(j)].type == VisualRow::Type::Store) {
                following = view[size_t(j)].storeIndex;
                break;
            }
        }
        const bool trimmedWithPrefix = following >= 0 && following < storeRemoved;
        if (trimmedWithPrefix) {
            ++removed;
            continue;
        }
        // The next store row survives. A non-today separator above it is the
        // leading backlog rule and stays. A Today separator would no longer
        // be a rule, and an unread mark above that row would then sit at
        // row 0, which buildView does not draw.
        const bool wouldLeadRemaining = following == storeRemoved;
        const bool keptLeadingDate =
            row.type == VisualRow::Type::DateSeparator && row.date.isValid()
            && row.date != today;
        if (wouldLeadRemaining && keptLeadingDate)
            break;
        if (wouldLeadRemaining)
            ++removed;
        else
            break;
    }
    return removed;
}

MessageListModel::TrimSpan MessageListModel::visualTrimSpan(
    const std::vector<VisualRow>& view,
    int storeRemoved,
    const QDate& today)
{
    if (storeRemoved <= 0)
        return {};

    // The leading rule stays when this trim removes some, but not all, of
    // that day's messages. The separator's own following store is the line
    // being trimmed, so a prefix walk would drop the rule with it and the
    // reload would reset on every later line.
    const bool leadingRule = !view.empty()
        && view.front().type == VisualRow::Type::DateSeparator
        && view.front().date.isValid()
        && view.front().date != today;
    if (leadingRule) {
        int last = 0;
        int seen = 0;
        bool dayEnded = false;
        for (int i = 1; i < int(view.size()) && seen < storeRemoved; ++i) {
            const VisualRow& row = view[size_t(i)];
            if (row.type == VisualRow::Type::DateSeparator) {
                dayEnded = true;
                break;
            }
            if (row.type == VisualRow::Type::Store) {
                if (row.storeIndex >= storeRemoved)
                    break;
                last = i;
                ++seen;
                continue;
            }
            int following = -1;
            for (int j = i + 1; j < int(view.size()); ++j) {
                if (view[size_t(j)].type == VisualRow::Type::Store) {
                    following = view[size_t(j)].storeIndex;
                    break;
                }
            }
            if (following < 0 || following >= storeRemoved)
                break;
            last = i;
        }
        int after = last + 1;
        while (after < int(view.size())
               && view[size_t(after)].type == VisualRow::Type::UnreadMark)
            ++after;
        const bool dayContinues = !dayEnded && seen == storeRemoved
            && after < int(view.size())
            && view[size_t(after)].type == VisualRow::Type::Store;
        if (dayContinues && last >= 1)
            return {1, last};
    }

    const int removed = visualPrefixToRemove(view, storeRemoved, today);
    if (removed <= 0)
        return {};
    return {0, removed - 1};
}

bool MessageListModel::sameLeadingDate(const std::vector<VisualRow>& current,
                                       int firstKept,
                                       const std::vector<VisualRow>& next)
{
    const bool currentLeads = firstKept >= 0 && firstKept < int(current.size())
        && current[size_t(firstKept)].type == VisualRow::Type::DateSeparator
        && current[size_t(firstKept)].date.isValid();
    const bool nextLeads = !next.empty()
        && next.front().type == VisualRow::Type::DateSeparator
        && next.front().date.isValid();
    if (currentLeads != nextLeads)
        return false;
    if (!currentLeads)
        return true;
    return current[size_t(firstKept)].date == next.front().date;
}

MessageListModel::MessageListModel(IrcEventReducer& reducer, QObject *parent)
    : QAbstractListModel(parent)
    , m_reducer(reducer)
{
}

int MessageListModel::rowCount(const QModelIndex& parent) const
{
    return parent.isValid() ? 0 : int(m_view.size());
}

QVariant MessageListModel::data(const QModelIndex& index, int role) const
{
    if (!index.isValid() || !m_selected || index.row() < 0
        || index.row() >= int(m_view.size()))
        return {};

    const IrcConversationState *conversation = m_reducer.find(*m_selected);
    if (!conversation)
        return {};

    const VisualRow& row = m_view[size_t(index.row())];
    if (row.type == VisualRow::Type::DateSeparator) {
        switch (role) {
        case AuthorRole:
            return QString();
        case TimeRole:
            return QString();
        case BodyRole:
            return dateLabel(row.date, transcriptToday());
        case KindRole:
            return QStringLiteral("event");
        case NetworkIdRole:
            return conversation->key.networkId;
        case OriginRole:
            return QStringLiteral("live");
        case AuthorAvatarRole:
            return QString();
        case AuthorBotRole:
            return false;
        case AuthorAccountRole:
            return QString();
        case MentionedRole:
            return false;
        case OnlyVisibleToYouRole:
            return false;
        default:
            return {};
        }
    }
    if (row.type == VisualRow::Type::UnreadMark) {
        switch (role) {
        case AuthorRole:
            return QString();
        case TimeRole:
            return QString();
        case BodyRole:
            return QString();
        case KindRole:
            return QStringLiteral("unread");
        case NetworkIdRole:
            return conversation->key.networkId;
        case OriginRole:
            return QStringLiteral("live");
        case AuthorAvatarRole:
            return QString();
        case AuthorBotRole:
            return false;
        case AuthorAccountRole:
            return QString();
        case MentionedRole:
            return false;
        case OnlyVisibleToYouRole:
            return false;
        default:
            return {};
        }
    }

    if (row.storeIndex < 0
        || row.storeIndex >= int(conversation->messages.size()))
        return {};

    const IrcReducedMessage& message =
        conversation->messages[size_t(row.storeIndex)];
    switch (role) {
    case AuthorRole:
        return message.author;
    case TimeRole:
        return displayTime(message);
    case BodyRole:
        return message.body;
    case KindRole:
        return kindString(message.kind);
    case NetworkIdRole:
        return conversation->key.networkId;
    case OriginRole:
        return message.origin == IrcOrigin::Replay
            ? QStringLiteral("replay")
            : QStringLiteral("live");
    case MsgidRole:
        return message.msgid.value;
    case SequenceRole:
        return message.sequence;
    case AuthorAvatarRole:
        if (message.author.isEmpty())
            return QString();
        return m_reducer.nickPresence(conversation->key.networkId, message.author)
            .avatar();
    case AuthorBotRole:
        if (message.author.isEmpty())
            return false;
        return m_reducer.nickPresence(conversation->key.networkId, message.author)
            .isBot();
    case AuthorAccountRole:
        if (message.author.isEmpty())
            return QString();
        return m_reducer.displayAccount(conversation->key.networkId, message.author);
    case MentionedRole:
        return m_reducer.isTranscriptHighlight(
            conversation->key.networkId, message.author, message.kind,
            message.body);
    case OnlyVisibleToYouRole:
        return message.onlyVisibleToYou;
    default:
        return {};
    }
}

QHash<int, QByteArray> MessageListModel::staticRoleNames()
{
    return {
        {AuthorRole, "author"},
        {TimeRole, "time"},
        {BodyRole, "body"},
        {KindRole, "kind"},
        {NetworkIdRole, "networkId"},
        {OriginRole, "origin"},
        {MsgidRole, "msgid"},
        {AuthorAvatarRole, "authorAvatar"},
        {AuthorBotRole, "authorBot"},
        {AuthorAccountRole, "authorAccount"},
        {MentionedRole, "mentioned"},
        {SequenceRole, "sequence"},
        {OnlyVisibleToYouRole, "onlyVisibleToYou"},
    };
}

QHash<int, QByteArray> MessageListModel::roleNames() const
{
    return staticRoleNames();
}

int MessageListModel::storeIndexAt(int visualRow) const
{
    if (visualRow < 0 || visualRow >= int(m_view.size()))
        return -1;
    const VisualRow& row = m_view[size_t(visualRow)];
    if (row.type != VisualRow::Type::Store)
        return -1;
    return row.storeIndex;
}

int MessageListModel::visualRowForStoreIndex(int storeIndex) const
{
    for (int index = 0; index < int(m_view.size()); ++index) {
        const VisualRow& row = m_view[size_t(index)];
        if (row.type == VisualRow::Type::Store && row.storeIndex == storeIndex)
            return index;
    }
    return -1;
}

QString MessageListModel::field(int row, const QString& name) const
{
    static const QHash<QString, int> roles = [] {
        QHash<QString, int> byName;
        const QHash<int, QByteArray> names = MessageListModel::staticRoleNames();
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

int MessageListModel::unreadMarkRow() const
{
    for (int row = 0; row < int(m_view.size()); ++row) {
        if (m_view[size_t(row)].type == VisualRow::Type::UnreadMark)
            return row;
    }
    return -1;
}

void MessageListModel::notifySeparatorRows()
{
    for (int row = 0; row < int(m_view.size()); ++row) {
        if (m_view[size_t(row)].type != VisualRow::Type::DateSeparator)
            continue;
        const QModelIndex idx = index(row, 0);
        emit dataChanged(idx, idx);
    }
}

QDate MessageListModel::transcriptToday() const
{
#ifdef OMAIRC_TEST
    if (m_transcriptToday)
        return *m_transcriptToday;
#endif
    return QDate::currentDate();
}

#ifdef OMAIRC_TEST
void MessageListModel::setTranscriptTodayForTest(const QDate& day)
{
    m_transcriptToday = day;
}
#endif

void MessageListModel::reload()
{
    const IrcConversationState *conversation =
        m_selected ? m_reducer.find(*m_selected) : nullptr;
    const QDate today = transcriptToday();
    std::vector<VisualRow> next = buildView(conversation, today);
    const int trimmed = conversation ? conversation->trimmed : 0;
    const int spliceEpoch = conversation ? conversation->spliceEpoch : 0;
    const std::optional<qint64> unreadMark =
        conversation ? conversation->unreadMark : std::nullopt;

    const bool sameConversation = m_selected.has_value() == m_loaded.has_value()
        && (!m_selected || *m_selected == *m_loaded)
        && spliceEpoch == m_spliceEpoch
        && unreadMark == m_unreadMark;
    TrimSpan span;
    if (sameConversation) {
        int storeRemoved = trimmed - m_trimmed;
        if (storeRemoved < 0)
            storeRemoved = 0;
        span = visualTrimSpan(m_view, storeRemoved, today);
    }
    // A partial trim of the leading day keeps that rule at row 0. A prefix
    // trim compares the first row that remains. Midnight can add or drop
    // the rule, and that still resets instead of pretending the new rows
    // were appended.
    const int leadingRow = span.first > 0 ? 0 : span.last + 1;
    if (sameConversation && sameLeadingDate(m_view, leadingRow, next)) {
        if (!span.empty()) {
            beginRemoveRows(QModelIndex(), span.first, span.last);
            m_view.erase(m_view.begin() + span.first,
                         m_view.begin() + span.last + 1);
            const int keep = int(m_view.size());
            for (int i = 0; i < keep && i < int(next.size()); ++i)
                m_view[size_t(i)] = next[size_t(i)];
            m_trimmed = trimmed;
            m_unreadMark = unreadMark;
            endRemoveRows();
            // The copy rewrites store indexes the remove did not announce.
            // Skip it when the next branch resets or dataChanged the range.
            if (keep > 0 && int(next.size()) > keep) {
                emit dataChanged(index(0, 0), index(keep - 1, 0));
            }
        }
        if (int(next.size()) > int(m_view.size())) {
            beginInsertRows(QModelIndex(), int(m_view.size()),
                            int(next.size()) - 1);
            m_view = std::move(next);
            m_trimmed = trimmed;
            m_unreadMark = unreadMark;
            endInsertRows();
            notifySeparatorRows();
            // Insert only announces the new indices. MentionedRole and
            // AuthorAccountRole are computed live, so a nick change or an
            // account that arrives with a new row would otherwise leave
            // existing headers stale.
            notifyMentioned();
            return;
        }
        if (int(next.size()) == int(m_view.size())) {
            m_view = std::move(next);
            m_trimmed = trimmed;
            m_unreadMark = unreadMark;
            if (!m_view.empty()) {
                emit dataChanged(index(0, 0),
                                 index(int(m_view.size()) - 1, 0));
            }
            return;
        }
    }

    beginResetModel();
    m_view = std::move(next);
    m_loaded = m_selected;
    m_trimmed = trimmed;
    m_spliceEpoch = spliceEpoch;
    m_unreadMark = unreadMark;
    endResetModel();
}

void MessageListModel::notifyMentioned()
{
    if (m_view.empty())
        return;
    emit dataChanged(index(0, 0),
                     index(int(m_view.size()) - 1, 0),
                     {MentionedRole, AuthorAccountRole});
}

void MessageListModel::setSelected(const IrcConversationKey& key)
{
    m_selected = key;
}

void MessageListModel::select(const IrcConversationKey& key)
{
    setSelected(key);
    reload();
}

void MessageListModel::clearSelection()
{
    m_selected.reset();
    reload();
}
