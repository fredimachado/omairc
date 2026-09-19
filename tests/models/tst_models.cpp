#include <QAbstractItemModel>
#include <QDate>
#include <QDateTime>
#include <QList>
#include <QLocale>
#include <QSignalSpy>
#include <QTest>
#include <QTime>
#include <QTimeZone>
#include <QVariantMap>

#include "conversationlistmodel.h"
#include "irceventreducer.h"
#include "ircnetworklog.h"
#include "ircstatusentry.h"
#include "irctyping.h"
#include "memberlistmodel.h"
#include "messagelistmodel.h"
#include "networklogmodel.h"

namespace
{
const QString networkA = QStringLiteral("network-a");
const QString networkB = QStringLiteral("network-b");
const QDateTime timestamp = QDateTime::currentDateTimeUtc();

QDateTime atLocal(const QDate& date, const QTime& time = QTime(12, 0))
{
    return QDateTime(date, time, QTimeZone::systemTimeZone());
}

QString expectedDateLabel(const QDate& date)
{
    const QDate today = QDate::currentDate();
    if (date == today)
        return QStringLiteral("Today");
    if (date == today.addDays(-1))
        return QStringLiteral("Yesterday");
    return QLocale().toString(date, QLocale::ShortFormat);
}

void welcome(IrcEventReducer& reducer,
             const QString& network,
             const QString& nick = QStringLiteral("omairc"))
{
    reducer.apply(IrcWelcomeEvent{network, nick});
}

IrcName parsedName(const IrcServerFeatures& features, std::string_view token)
{
    const auto parsed = features.parseNamesToken(token);
    return {QString::fromStdString(parsed->nick), parsed->ranks};
}

IrcName parsedName(std::string_view token)
{
    return parsedName(IrcServerFeatures(), token);
}

QVariant roleAt(const QAbstractItemModel& model, int row, int role)
{
    return model.data(model.index(row, 0), role);
}

int rowFor(const ConversationListModel& model, const QString& conversationId)
{
    for (int row = 0; row < model.rowCount(); ++row) {
        if (roleAt(model, row, ConversationListModel::ConversationIdRole)
            == conversationId) {
            return row;
        }
    }
    return -1;
}
}

class ModelTest : public QObject
{
    Q_OBJECT

private slots:
    void roleNamesMatchQml();
    void joinNamesPrivmsgPopulateModels();
    void memberStatusIsMetadataNotPrefixModes();
    void memberAndDirectRowsExposeAvatarAndBot();
    void membersOrderByRankThenNick();
    void membersFollowServerPrefixOrder();
    void identicalChannelsStayDistinct();
    void selectingZerosUnread();
    void membersEmptyForDirectMessage();
    void messageKinds();
    void conversationsOrderChannelsThenDirect();
    void conversationGetFieldAndHasDirects();
    void twoNetworksFollowRosterThenChannelRank();
    void parseConversationIdRejectsBareAndDoubleSeparators();
    void neighborAfterDropNextPreviousGhostAndOnly();
    void neighborAfterDropPrefersSameNetwork();
    void reloadUnchangedKeysEmitsDataChangedNotReset();
    void typingRoleDerivesFromExistingDirectAndInvalidates();
    void conversationPresenceFollowsDirectPeerAway();
    void selectedChatAppendInsertsInsteadOfReset();
    void sameSizeReloadEmitsDataChangedCoveringFirstRow();
    void selectedMemberJoinInsertsInsteadOfReset();
    void reloadTrimEmitsRemovesWhenCountUnchanged();
    void reloadClearAfterCapEmitsRemoves();
    void collapsedJoinRewritesLastRow();
    void originRoleNameAndValues();
    void networkLogFieldLooksUpRolesByName();
    void spliceResetsSelectedConversation();
    void dateSeparatorBetweenLocalDays();
    void dateSeparatorAfterHistorySplice();
    void dateSeparatorBetweenSameNickMidnight();
    void clearMessagesDropsDerivedDateSeparators();
    void dateSeparatorAppendsAfterMidnightWithoutReset();
    void reloadTrimAcrossDayRemovesLeadingSeparator();
};

void ModelTest::roleNamesMatchQml()
{
    IrcEventReducer reducer;
    ConversationListModel conversations(reducer);
    MessageListModel messages(reducer);
    MemberListModel members(reducer);

    QCOMPARE(conversations.roleNames()[ConversationListModel::ConversationRole],
             QByteArray("conversation"));
    QCOMPARE(conversations.roleNames()[ConversationListModel::UnreadRole],
             QByteArray("unread"));
    QCOMPARE(conversations.roleNames()[ConversationListModel::MentionRole],
             QByteArray("mention"));
    QCOMPARE(conversations.roleNames()[ConversationListModel::DirectRole],
             QByteArray("direct"));
    QCOMPARE(conversations.roleNames()[ConversationListModel::NetworkIdRole],
             QByteArray("networkId"));
    QCOMPARE(conversations.roleNames()[ConversationListModel::ConversationIdRole],
             QByteArray("conversationId"));
    QCOMPARE(conversations.roleNames()[ConversationListModel::ConversationNameRole],
             QByteArray("conversationName"));
    QCOMPARE(conversations.roleNames()[ConversationListModel::TypingRole],
             QByteArray("typing"));
    QCOMPARE(conversations.roleNames()[ConversationListModel::MutedRole],
             QByteArray("muted"));
    QCOMPARE(conversations.roleNames()[ConversationListModel::PresenceRole],
             QByteArray("presence"));
    QCOMPARE(conversations.roleNames()[ConversationListModel::AvatarRole],
             QByteArray("avatar"));
    QCOMPARE(conversations.roleNames()[ConversationListModel::BotRole],
             QByteArray("bot"));

    QCOMPARE(messages.roleNames()[MessageListModel::AuthorRole], QByteArray("author"));
    QCOMPARE(messages.roleNames()[MessageListModel::TimeRole], QByteArray("time"));
    QCOMPARE(messages.roleNames()[MessageListModel::BodyRole], QByteArray("body"));
    QCOMPARE(messages.roleNames()[MessageListModel::KindRole], QByteArray("kind"));
    QCOMPARE(messages.roleNames()[MessageListModel::NetworkIdRole],
             QByteArray("networkId"));
    QCOMPARE(messages.roleNames()[MessageListModel::OriginRole], QByteArray("origin"));
    QCOMPARE(messages.roleNames()[MessageListModel::MsgidRole], QByteArray("msgid"));
    QCOMPARE(messages.roleNames()[MessageListModel::AuthorAvatarRole],
             QByteArray("authorAvatar"));
    QCOMPARE(messages.roleNames()[MessageListModel::AuthorBotRole],
             QByteArray("authorBot"));

    QCOMPARE(members.roleNames()[MemberListModel::NickRole], QByteArray("nick"));
    QCOMPARE(members.roleNames()[MemberListModel::LabelRole], QByteArray("label"));
    QCOMPARE(members.roleNames()[MemberListModel::StatusRole], QByteArray("status"));
    QCOMPARE(members.roleNames()[MemberListModel::AwayRole], QByteArray("away"));
    QCOMPARE(members.roleNames()[MemberListModel::NetworkIdRole],
             QByteArray("networkId"));
    QCOMPARE(members.roleNames()[MemberListModel::AvatarRole], QByteArray("avatar"));
    QCOMPARE(members.roleNames()[MemberListModel::BotRole], QByteArray("bot"));
}

void ModelTest::joinNamesPrivmsgPopulateModels()
{
    IrcEventReducer reducer;
    ConversationListModel conversations(reducer);
    MessageListModel messages(reducer);
    MemberListModel members(reducer);
    welcome(reducer, networkA);

    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("omairc")});
    reducer.apply(IrcNamesEvent{
        networkA,
        QStringLiteral("#room"),
        {parsedName("@omairc"), parsedName("Alice"), parsedName("+Bob")},
        true,
    });
    reducer.apply(IrcAwayEvent{networkA, QStringLiteral("Bob"),
                               IrcAway{QStringLiteral("brb")}});
    reducer.apply(IrcMessageEvent{
        room, QStringLiteral("Alice"), QStringLiteral("hello"), timestamp,
        QStringLiteral("#room")});

    conversations.reload();
    messages.select(room);
    members.select(room);

    QCOMPARE(conversations.rowCount(), 1);
    QCOMPARE(roleAt(conversations, 0, ConversationListModel::ConversationRole),
             QStringLiteral("#room"));
    QCOMPARE(roleAt(conversations, 0, ConversationListModel::UnreadRole), 1);
    QCOMPARE(roleAt(conversations, 0, ConversationListModel::MentionRole), false);
    QCOMPARE(roleAt(conversations, 0, ConversationListModel::DirectRole), false);
    QCOMPARE(roleAt(conversations, 0, ConversationListModel::NetworkIdRole), networkA);
    QCOMPARE(roleAt(conversations, 0, ConversationListModel::ConversationIdRole),
             ircConversationId(room));
    QCOMPARE(roleAt(conversations, 0, ConversationListModel::ConversationIdRole),
             QStringLiteral("network-a\n#room"));

    QCOMPARE(messages.rowCount(), 2);
    QCOMPARE(roleAt(messages, 0, MessageListModel::KindRole),
             QStringLiteral("event"));
    QCOMPARE(roleAt(messages, 0, MessageListModel::AuthorRole), QString());
    QCOMPARE(roleAt(messages, 0, MessageListModel::TimeRole), QString());
    QCOMPARE(roleAt(messages, 0, MessageListModel::BodyRole),
             QStringLiteral("omairc joined"));
    QCOMPARE(roleAt(messages, 1, MessageListModel::AuthorRole),
             QStringLiteral("Alice"));
    QCOMPARE(roleAt(messages, 1, MessageListModel::BodyRole),
             QStringLiteral("hello"));
    QCOMPARE(roleAt(messages, 1, MessageListModel::KindRole),
             QStringLiteral("message"));
    QCOMPARE(roleAt(messages, 1, MessageListModel::TimeRole),
             timestamp.toLocalTime().toString(QStringLiteral("HH:mm")));
    QCOMPARE(roleAt(messages, 1, MessageListModel::NetworkIdRole), networkA);
    QCOMPARE(roleAt(messages, 1, MessageListModel::OriginRole),
             QStringLiteral("live"));

    QCOMPARE(members.rowCount(), 3);
    QCOMPARE(roleAt(members, 0, MemberListModel::NickRole), QStringLiteral("omairc"));
    QCOMPARE(roleAt(members, 0, MemberListModel::LabelRole), QStringLiteral("@omairc"));
    QCOMPARE(roleAt(members, 0, MemberListModel::StatusRole), QString());
    QCOMPARE(roleAt(members, 1, MemberListModel::NickRole), QStringLiteral("Bob"));
    QCOMPARE(roleAt(members, 1, MemberListModel::LabelRole), QStringLiteral("+Bob"));
    QCOMPARE(roleAt(members, 1, MemberListModel::StatusRole), QString());
    QCOMPARE(roleAt(members, 1, MemberListModel::AwayRole), true);
    QCOMPARE(roleAt(members, 2, MemberListModel::NickRole), QStringLiteral("Alice"));
    QCOMPARE(roleAt(members, 2, MemberListModel::LabelRole), QStringLiteral("Alice"));
    QCOMPARE(roleAt(members, 2, MemberListModel::StatusRole), QString());
    QCOMPARE(roleAt(members, 2, MemberListModel::AwayRole), false);
    QCOMPARE(roleAt(members, 2, MemberListModel::NetworkIdRole), networkA);
}

void ModelTest::memberStatusIsMetadataNotPrefixModes()
{
    IrcEventReducer reducer;
    MemberListModel members(reducer);
    welcome(reducer, networkA);

    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    reducer.apply(IrcNamesEvent{
        networkA,
        QStringLiteral("#room"),
        {parsedName("@Alice"), parsedName("+Bob")},
        true,
    });
    reducer.apply(IrcMemberMetadataEvent{
        networkA, QStringLiteral("Alice"), QStringLiteral("status"),
        QStringLiteral("writing docs")});
    members.select(room);

    QCOMPARE(roleAt(members, 0, MemberListModel::NickRole), QStringLiteral("Alice"));
    QCOMPARE(roleAt(members, 0, MemberListModel::LabelRole), QStringLiteral("@Alice"));
    QCOMPARE(roleAt(members, 0, MemberListModel::StatusRole),
             QStringLiteral("writing docs"));
    QCOMPARE(roleAt(members, 1, MemberListModel::NickRole), QStringLiteral("Bob"));
    QCOMPARE(roleAt(members, 1, MemberListModel::LabelRole), QStringLiteral("+Bob"));
    QCOMPARE(roleAt(members, 1, MemberListModel::StatusRole), QString());
    for (int row = 0; row < members.rowCount(); ++row) {
        const QString status =
            roleAt(members, row, MemberListModel::StatusRole).toString();
        QVERIFY(status != QStringLiteral("o"));
        QVERIFY(status != QStringLiteral("v"));
        QCOMPARE(roleAt(members, row, MemberListModel::AwayRole), false);
    }
}

void ModelTest::memberAndDirectRowsExposeAvatarAndBot()
{
    IrcEventReducer reducer;
    MemberListModel members(reducer);
    ConversationListModel conversations(reducer);
    MessageListModel messages(reducer);
    welcome(reducer, networkA);

    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    const IrcConversationKey alice =
        reducer.conversationKey(networkA, QStringLiteral("Alice"));
    reducer.apply(IrcNamesEvent{
        networkA,
        QStringLiteral("#room"),
        {parsedName("@Alice"), parsedName("+Bob")},
        true,
    });
    reducer.apply(IrcMessageEvent{
        alice, QStringLiteral("Alice"), QStringLiteral("hi"), timestamp,
        QStringLiteral("Alice")});
    reducer.apply(IrcMemberMetadataEvent{
        networkA, QStringLiteral("Alice"), QStringLiteral("avatar"),
        QStringLiteral("https://example.com/a.png")});
    reducer.apply(IrcMemberMetadataEvent{
        networkA, QStringLiteral("Alice"), QStringLiteral("bot"),
        QStringLiteral("PacketBot")});
    members.select(room);
    conversations.reload();
    messages.select(alice);

    QCOMPARE(roleAt(members, 0, MemberListModel::NickRole), QStringLiteral("Alice"));
    QCOMPARE(roleAt(members, 0, MemberListModel::AvatarRole),
             QStringLiteral("https://example.com/a.png"));
    QCOMPARE(roleAt(members, 0, MemberListModel::BotRole), true);
    QCOMPARE(roleAt(members, 1, MemberListModel::AvatarRole), QString());
    QCOMPARE(roleAt(members, 1, MemberListModel::BotRole), false);

    const int aliceRow = rowFor(conversations, ircConversationId(alice));
    const int roomRow = rowFor(conversations, ircConversationId(room));
    QVERIFY(aliceRow >= 0);
    QVERIFY(roomRow >= 0);
    QCOMPARE(roleAt(conversations, aliceRow, ConversationListModel::AvatarRole),
             QStringLiteral("https://example.com/a.png"));
    QCOMPARE(roleAt(conversations, aliceRow, ConversationListModel::BotRole), true);
    QCOMPARE(roleAt(conversations, roomRow, ConversationListModel::AvatarRole),
             QString());
    QCOMPARE(roleAt(conversations, roomRow, ConversationListModel::BotRole), false);

    QCOMPARE(roleAt(messages, 0, MessageListModel::AuthorRole),
             QStringLiteral("Alice"));
    QCOMPARE(roleAt(messages, 0, MessageListModel::AuthorAvatarRole),
             QStringLiteral("https://example.com/a.png"));
    QCOMPARE(roleAt(messages, 0, MessageListModel::AuthorBotRole), true);
}

void ModelTest::membersOrderByRankThenNick()
{
    IrcEventReducer reducer;
    MemberListModel members(reducer);
    welcome(reducer, networkA);

    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    reducer.apply(IrcNamesEvent{
        networkA,
        QStringLiteral("#room"),
        {parsedName("mira"), parsedName("@anna"), parsedName("~zoe"),
         parsedName("+kai"), parsedName("%dax"), parsedName("&bob"),
         parsedName("@abel"), parsedName("~adam")},
        true,
    });
    members.select(room);

    const auto visibleNicks = [&members] {
        QStringList nicks;
        for (int row = 0; row < members.rowCount(); ++row)
            nicks.append(roleAt(members, row, MemberListModel::NickRole).toString());
        return nicks;
    };
    QCOMPARE(visibleNicks(),
             QStringList({QStringLiteral("adam"), QStringLiteral("zoe"),
                          QStringLiteral("bob"), QStringLiteral("abel"),
                          QStringLiteral("anna"), QStringLiteral("dax"),
                          QStringLiteral("kai"), QStringLiteral("mira")}));

    QSignalSpy resets(&members, &QAbstractItemModel::modelReset);
    QSignalSpy moves(&members, &QAbstractItemModel::rowsMoved);

    reducer.apply(IrcModeEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("op"),
        QStringLiteral("+o"), {QStringLiteral("mira")}});
    members.reload();
    QCOMPARE(resets.size(), 0);
    QCOMPARE(moves.size(), 1);
    QCOMPARE(visibleNicks(),
             QStringList({QStringLiteral("adam"), QStringLiteral("zoe"),
                          QStringLiteral("bob"), QStringLiteral("abel"),
                          QStringLiteral("anna"), QStringLiteral("mira"),
                          QStringLiteral("dax"), QStringLiteral("kai")}));
    QCOMPARE(roleAt(members, 5, MemberListModel::LabelRole), QStringLiteral("@mira"));

    reducer.apply(IrcModeEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("op"),
        QStringLiteral("-o"), {QStringLiteral("mira")}});
    members.reload();
    QCOMPARE(resets.size(), 0);
    QCOMPARE(visibleNicks(),
             QStringList({QStringLiteral("adam"), QStringLiteral("zoe"),
                          QStringLiteral("bob"), QStringLiteral("abel"),
                          QStringLiteral("anna"), QStringLiteral("dax"),
                          QStringLiteral("kai"), QStringLiteral("mira")}));
    QCOMPARE(roleAt(members, 7, MemberListModel::LabelRole), QStringLiteral("mira"));
    QCOMPARE(roleAt(members, 3, MemberListModel::LabelRole), QStringLiteral("@abel"));
}

void ModelTest::membersFollowServerPrefixOrder()
{
    IrcEventReducer reducer;
    MemberListModel members(reducer);
    welcome(reducer, networkB);

    IrcServerFeatures features;
    features.applyToken("PREFIX=(ohv)@%+");
    reducer.setServerFeatures(networkB, features);

    const IrcConversationKey room =
        reducer.conversationKey(networkB, QStringLiteral("#room"));
    reducer.apply(IrcNamesEvent{
        networkB,
        QStringLiteral("#room"),
        {parsedName(features, "mira"), parsedName(features, "+kai"),
         parsedName(features, "%dax"), parsedName(features, "@anna")},
        true,
    });
    members.select(room);

    QStringList ordered;
    for (int row = 0; row < members.rowCount(); ++row)
        ordered.append(roleAt(members, row, MemberListModel::NickRole).toString());
    QCOMPARE(ordered,
             QStringList({QStringLiteral("anna"), QStringLiteral("dax"),
                          QStringLiteral("kai"), QStringLiteral("mira")}));
    QCOMPARE(roleAt(members, 0, MemberListModel::LabelRole), QStringLiteral("@anna"));
    QCOMPARE(roleAt(members, 1, MemberListModel::LabelRole), QStringLiteral("%dax"));
    QCOMPARE(roleAt(members, 2, MemberListModel::LabelRole), QStringLiteral("+kai"));
}

void ModelTest::identicalChannelsStayDistinct()
{
    IrcEventReducer reducer;
    ConversationListModel conversations(reducer);
    welcome(reducer, networkA);
    welcome(reducer, networkB);

    const IrcConversationKey keyA =
        reducer.conversationKey(networkA, QStringLiteral("#chan"));
    const IrcConversationKey keyB =
        reducer.conversationKey(networkB, QStringLiteral("#chan"));
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#chan"), QStringLiteral("omairc")});
    reducer.apply(IrcJoinEvent{
        networkB, QStringLiteral("#chan"), QStringLiteral("omairc")});
    conversations.reload();

    QCOMPARE(conversations.rowCount(), 2);
    const int rowA = rowFor(conversations, ircConversationId(keyA));
    const int rowB = rowFor(conversations, ircConversationId(keyB));
    QVERIFY(rowA >= 0);
    QVERIFY(rowB >= 0);
    QVERIFY(rowA != rowB);
    QCOMPARE(roleAt(conversations, rowA, ConversationListModel::ConversationRole),
             QStringLiteral("#chan"));
    QCOMPARE(roleAt(conversations, rowB, ConversationListModel::ConversationRole),
             QStringLiteral("#chan"));
    QCOMPARE(roleAt(conversations, rowA, ConversationListModel::NetworkIdRole),
             networkA);
    QCOMPARE(roleAt(conversations, rowB, ConversationListModel::NetworkIdRole),
             networkB);
    QCOMPARE(roleAt(conversations, rowA, ConversationListModel::ConversationIdRole),
             QStringLiteral("network-a\n#chan"));
    QCOMPARE(roleAt(conversations, rowB, ConversationListModel::ConversationIdRole),
             QStringLiteral("network-b\n#chan"));
}

void ModelTest::selectingZerosUnread()
{
    IrcEventReducer reducer;
    ConversationListModel conversations(reducer);
    welcome(reducer, networkA, QStringLiteral("omairc"));

    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    reducer.apply(IrcMessageEvent{
        room,
        QStringLiteral("Alice"),
        QStringLiteral("omairc: ping"),
        timestamp,
        QStringLiteral("#room"),
    });
    conversations.reload();
    QCOMPARE(roleAt(conversations, 0, ConversationListModel::UnreadRole), 1);
    QCOMPARE(roleAt(conversations, 0, ConversationListModel::MentionRole), true);

    conversations.select(room);
    QCOMPARE(conversations.rowCount(), 1);
    QCOMPARE(roleAt(conversations, 0, ConversationListModel::UnreadRole), 0);
    QCOMPARE(roleAt(conversations, 0, ConversationListModel::MentionRole), false);
}

void ModelTest::membersEmptyForDirectMessage()
{
    IrcEventReducer reducer;
    MemberListModel members(reducer);
    welcome(reducer, networkA);

    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    const IrcConversationKey alice =
        reducer.conversationKey(networkA, QStringLiteral("Alice"));
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("Alice")});
    reducer.apply(IrcNamesEvent{
        networkA,
        QStringLiteral("#room"),
        {parsedName("Alice")},
        true,
    });
    reducer.apply(IrcMessageEvent{
        alice, QStringLiteral("Alice"), QStringLiteral("hi"), timestamp,
        QStringLiteral("Alice")});

    members.select(room);
    QCOMPARE(members.rowCount(), 1);
    QCOMPARE(roleAt(members, 0, MemberListModel::NickRole), QStringLiteral("Alice"));

    members.select(alice);
    QCOMPARE(members.rowCount(), 0);
}

void ModelTest::messageKinds()
{
    IrcEventReducer reducer;
    MessageListModel messages(reducer);
    welcome(reducer, networkA);

    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("Alice")});
    reducer.apply(IrcMessageEvent{
        room, QStringLiteral("Alice"), QStringLiteral("plain"), timestamp,
        QStringLiteral("#room")});
    reducer.apply(IrcNoticeEvent{
        room, QStringLiteral("Alice"), QStringLiteral("notice"), timestamp,
        QStringLiteral("#room")});
    reducer.apply(IrcActionEvent{
        room, QStringLiteral("Alice"), QStringLiteral("waves"), timestamp,
        QStringLiteral("#room")});
    reducer.apply(IrcWhoisTranscriptEvent{
        room, QStringLiteral("lena is ~lena@h (Lena)")});

    messages.select(room);
    QCOMPARE(messages.rowCount(), 5);
    QCOMPARE(roleAt(messages, 0, MessageListModel::KindRole),
             QStringLiteral("event"));
    QCOMPARE(roleAt(messages, 1, MessageListModel::KindRole),
             QStringLiteral("message"));
    QCOMPARE(roleAt(messages, 2, MessageListModel::KindRole),
             QStringLiteral("notice"));
    QCOMPARE(roleAt(messages, 3, MessageListModel::KindRole),
             QStringLiteral("action"));
    QCOMPARE(roleAt(messages, 3, MessageListModel::BodyRole),
             QStringLiteral("waves"));
    QCOMPARE(roleAt(messages, 4, MessageListModel::KindRole),
             QStringLiteral("whois"));
    QCOMPARE(roleAt(messages, 4, MessageListModel::AuthorRole), QString());
    QCOMPARE(roleAt(messages, 4, MessageListModel::TimeRole), QString());
}

void ModelTest::conversationsOrderChannelsThenDirect()
{
    IrcEventReducer reducer;
    ConversationListModel conversations(reducer);
    welcome(reducer, networkA);

    const IrcConversationKey zed =
        reducer.conversationKey(networkA, QStringLiteral("zed"));
    const IrcConversationKey alice =
        reducer.conversationKey(networkA, QStringLiteral("alice"));
    reducer.apply(IrcMessageEvent{
        zed, QStringLiteral("zed"), QStringLiteral("later"), timestamp,
        QStringLiteral("zed")});
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#idle"), QStringLiteral("Alice")});
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#active"), QStringLiteral("omairc")});
    reducer.apply(IrcMessageEvent{
        alice, QStringLiteral("alice"), QStringLiteral("first"), timestamp,
        QStringLiteral("alice")});

    conversations.reload();
    QCOMPARE(conversations.rowCount(), 4);
    QCOMPARE(roleAt(conversations, 0, ConversationListModel::ConversationRole),
             QStringLiteral("#active"));
    QCOMPARE(roleAt(conversations, 0, ConversationListModel::DirectRole), false);
    QCOMPARE(roleAt(conversations, 1, ConversationListModel::ConversationRole),
             QStringLiteral("#idle"));
    QCOMPARE(roleAt(conversations, 1, ConversationListModel::DirectRole), false);
    QCOMPARE(roleAt(conversations, 2, ConversationListModel::ConversationRole),
             QStringLiteral("alice"));
    QCOMPARE(roleAt(conversations, 2, ConversationListModel::DirectRole), true);
    QCOMPARE(roleAt(conversations, 3, ConversationListModel::ConversationRole),
             QStringLiteral("zed"));
    QCOMPARE(roleAt(conversations, 3, ConversationListModel::DirectRole), true);
}

void ModelTest::conversationGetFieldAndHasDirects()
{
    IrcEventReducer reducer;
    ConversationListModel conversations(reducer);
    welcome(reducer, networkA);
    welcome(reducer, networkB);

    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#active"), QStringLiteral("omairc")});
    reducer.apply(IrcMessageEvent{
        reducer.conversationKey(networkA, QStringLiteral("alice")),
        QStringLiteral("alice"), QStringLiteral("first"), timestamp,
        QStringLiteral("alice")});

    conversations.reload();
    QCOMPARE(conversations.rowCount(), 2);
    QCOMPARE(conversations.field(0, QStringLiteral("conversation")).toString(),
             QStringLiteral("#active"));
    QCOMPARE(conversations.field(0, QStringLiteral("direct")).toBool(), false);
    QCOMPARE(conversations.field(0, QStringLiteral("unread")).toInt(), 0);
    QCOMPARE(conversations.field(1, QStringLiteral("conversationName")).toString(),
             QStringLiteral("alice"));
    QCOMPARE(conversations.field(1, QStringLiteral("direct")).toBool(), true);
    QCOMPARE(conversations.field(1, QStringLiteral("unread")).toInt(), 1);
    QCOMPARE(conversations.field(1, QStringLiteral("networkId")).toString(),
             networkA);

    const QVariantMap channel = conversations.get(0);
    QCOMPARE(channel.value(QStringLiteral("conversation")).toString(),
             QStringLiteral("#active"));
    QCOMPARE(channel.value(QStringLiteral("conversationName")).toString(),
             QStringLiteral("#active"));
    QCOMPARE(channel.value(QStringLiteral("conversationId")).toString(),
             QStringLiteral("network-a\n#active"));
    QCOMPARE(channel.value(QStringLiteral("direct")).toBool(), false);

    const QVariantMap direct = conversations.get(1);
    QCOMPARE(direct.value(QStringLiteral("conversation")).toString(),
             QStringLiteral("alice"));
    QCOMPARE(direct.value(QStringLiteral("direct")).toBool(), true);
    QCOMPARE(direct.value(QStringLiteral("unread")).toInt(), 1);

    QVERIFY(conversations.hasDirects(networkA));
    QVERIFY(!conversations.hasDirects(networkB));
    QVERIFY(!conversations.hasDirects(QString()));
    QVERIFY(conversations.get(-1).isEmpty());
    QVERIFY(conversations.get(99).isEmpty());
    QVERIFY(!conversations.field(0, QStringLiteral("no-such-role")).isValid());
    QVERIFY(!conversations.field(-1, QStringLiteral("conversation")).isValid());
}

void ModelTest::twoNetworksFollowRosterThenChannelRank()
{
    IrcEventReducer reducer;
    ConversationListModel conversations(reducer);
    welcome(reducer, networkA);
    welcome(reducer, networkB);

    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#alpha"), QStringLiteral("omairc")});
    reducer.apply(IrcJoinEvent{
        networkB, QStringLiteral("#zed"), QStringLiteral("omairc")});
    reducer.apply(IrcMessageEvent{
        reducer.conversationKey(networkA, QStringLiteral("alice")),
        QStringLiteral("alice"), QStringLiteral("hi"), timestamp,
        QStringLiteral("alice")});
    reducer.apply(IrcMessageEvent{
        reducer.conversationKey(networkB, QStringLiteral("bob")),
        QStringLiteral("bob"), QStringLiteral("yo"), timestamp,
        QStringLiteral("bob")});

    conversations.setNetworkOrder({networkB, networkA});
    conversations.reload();
    QCOMPARE(conversations.rowCount(), 4);
    QCOMPARE(roleAt(conversations, 0, ConversationListModel::ConversationIdRole),
             QStringLiteral("network-b\n#zed"));
    QCOMPARE(roleAt(conversations, 1, ConversationListModel::ConversationIdRole),
             QStringLiteral("network-b\nbob"));
    QCOMPARE(roleAt(conversations, 2, ConversationListModel::ConversationIdRole),
             QStringLiteral("network-a\n#alpha"));
    QCOMPARE(roleAt(conversations, 3, ConversationListModel::ConversationIdRole),
             QStringLiteral("network-a\nalice"));

    const QVector<IrcConversationKey> ordered =
        ircSidebarOrder(reducer, {networkB, networkA});
    QCOMPARE(ordered.size(), 4);
    QCOMPARE(ircConversationId(ordered.at(0)), QStringLiteral("network-b\n#zed"));
    QCOMPARE(ircConversationId(ordered.at(3)), QStringLiteral("network-a\nalice"));
}

void ModelTest::parseConversationIdRejectsBareAndDoubleSeparators()
{
    const std::optional<IrcConversationKey> parsed =
        ircParseConversationId(QStringLiteral("network-a\n#room"));
    QVERIFY(parsed.has_value());
    QCOMPARE(parsed->networkId, QStringLiteral("network-a"));
    QCOMPARE(parsed->normalizedTarget, QStringLiteral("#room"));
    QCOMPARE(ircConversationId(*parsed), QStringLiteral("network-a\n#room"));

    QVERIFY(!ircParseConversationId(QStringLiteral("#room")).has_value());
    QVERIFY(!ircParseConversationId(QStringLiteral("network-a")).has_value());
    QVERIFY(!ircParseConversationId(QStringLiteral("network-a\n#room\nextra")).has_value());
    QVERIFY(!ircParseConversationId(QStringLiteral("\n#room")).has_value());
    QVERIFY(!ircParseConversationId(QStringLiteral("network-a\n")).has_value());
}

void ModelTest::neighborAfterDropNextPreviousGhostAndOnly()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey zed =
        reducer.conversationKey(networkA, QStringLiteral("zed"));
    const IrcConversationKey alice =
        reducer.conversationKey(networkA, QStringLiteral("alice"));
    reducer.apply(IrcMessageEvent{
        zed, QStringLiteral("zed"), QStringLiteral("later"), timestamp,
        QStringLiteral("zed")});
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#idle"), QStringLiteral("Alice")});
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#active"), QStringLiteral("omairc")});
    reducer.apply(IrcMessageEvent{
        alice, QStringLiteral("alice"), QStringLiteral("first"), timestamp,
        QStringLiteral("alice")});

    const QVector<IrcConversationKey> ordered = ircSidebarOrder(reducer);
    QCOMPARE(ordered.size(), 4);
    const IrcConversationKey active = ordered.at(0);
    const IrcConversationKey idle = ordered.at(1);
    QCOMPARE(alice, ordered.at(2));
    QCOMPARE(zed, ordered.at(3));

    QCOMPARE(*ircNeighborAfterDrop(ordered, active), idle);
    QCOMPARE(*ircNeighborAfterDrop(ordered, alice), zed);
    QCOMPARE(*ircNeighborAfterDrop(ordered, zed), alice);
    QVERIFY(ircNeighborAfterDrop(ordered, zed) != ordered.first());

    const IrcConversationKey ghost{networkA, QStringLiteral("missing")};
    QCOMPARE(*ircNeighborAfterDrop(ordered, ghost), zed);

    const QVector<IrcConversationKey> only{alice};
    QVERIFY(!ircNeighborAfterDrop(only, alice).has_value());
    QVERIFY(!ircNeighborAfterDrop({}, ghost).has_value());
}

void ModelTest::neighborAfterDropPrefersSameNetwork()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    welcome(reducer, networkB);
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#one"), QStringLiteral("omairc")});
    reducer.apply(IrcMessageEvent{
        reducer.conversationKey(networkA, QStringLiteral("alice")),
        QStringLiteral("alice"), QStringLiteral("hi"), timestamp,
        QStringLiteral("alice")});
    reducer.apply(IrcJoinEvent{
        networkB, QStringLiteral("#one"), QStringLiteral("omairc")});

    const QVector<IrcConversationKey> ordered =
        ircSidebarOrder(reducer, {networkA, networkB});
    QCOMPARE(ordered.size(), 3);
    QCOMPARE(ircConversationId(ordered.at(0)), QStringLiteral("network-a\n#one"));
    QCOMPARE(ircConversationId(ordered.at(1)), QStringLiteral("network-a\nalice"));
    QCOMPARE(ircConversationId(ordered.at(2)), QStringLiteral("network-b\n#one"));

    QCOMPARE(ircConversationId(*ircNeighborAfterDrop(ordered, ordered.at(1))),
             QStringLiteral("network-a\n#one"));
    QCOMPARE(ircConversationId(*ircNeighborAfterDrop(ordered, ordered.at(0))),
             QStringLiteral("network-a\nalice"));
}

void ModelTest::reloadUnchangedKeysEmitsDataChangedNotReset()
{
    IrcEventReducer reducer;
    ConversationListModel conversations(reducer);
    welcome(reducer, networkA);

    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("omairc")});
    conversations.reload();
    QCOMPARE(conversations.rowCount(), 1);

    QSignalSpy resets(&conversations, &QAbstractItemModel::modelReset);
    QSignalSpy changes(&conversations, &QAbstractItemModel::dataChanged);
    reducer.apply(IrcMessageEvent{
        reducer.conversationKey(networkA, QStringLiteral("#room")),
        QStringLiteral("Alice"),
        QStringLiteral("hello"),
        timestamp,
        QStringLiteral("#room"),
    });
    conversations.reload();
    QCOMPARE(resets.size(), 0);
    QCOMPARE(changes.size(), 1);
    QVERIFY(changes.at(0).at(2).value<QList<int>>().contains(
        ConversationListModel::TypingRole));
    QCOMPARE(roleAt(conversations, 0, ConversationListModel::UnreadRole), 1);

    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#other"), QStringLiteral("omairc")});
    conversations.reload();
    QCOMPARE(resets.size(), 1);
    QCOMPARE(conversations.rowCount(), 2);
}

void ModelTest::typingRoleDerivesFromExistingDirectAndInvalidates()
{
    IrcEventReducer reducer;
    ConversationListModel conversations(reducer);
    welcome(reducer, networkA);
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("omairc")});
    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    const IrcConversationKey alice =
        reducer.conversationKey(networkA, QStringLiteral("Alice"));
    const IrcConversationKey ghost =
        reducer.conversationKey(networkA, QStringLiteral("ghost"));
    reducer.apply(IrcMessageEvent{
        alice, QStringLiteral("Alice"), QStringLiteral("hi"), timestamp,
        QStringLiteral("Alice")});
    conversations.select(room);
    const int aliceRow = rowFor(conversations, ircConversationId(alice));
    const int roomRow = rowFor(conversations, ircConversationId(room));
    QVERIFY(aliceRow >= 0);
    QVERIFY(roomRow >= 0);
    QCOMPARE(roleAt(conversations, aliceRow, ConversationListModel::TypingRole),
             false);
    QCOMPARE(roleAt(conversations, roomRow, ConversationListModel::TypingRole),
             false);

    const QDateTime now = QDateTime::currentDateTimeUtc();
    reducer.apply(IrcTypingEvent{alice, QStringLiteral("Alice"),
                                 IrcTypingPhase::Active, now});
    reducer.apply(IrcTypingEvent{ghost, QStringLiteral("ghost"),
                                 IrcTypingPhase::Active, now});
    QVERIFY(!reducer.find(ghost));
    QCOMPARE(conversations.rowCount(), 2);

    QSignalSpy changes(&conversations, &QAbstractItemModel::dataChanged);
    conversations.invalidateTyping();
    QCOMPARE(changes.size(), 1);
    QCOMPARE(changes.at(0).at(2).value<QList<int>>(),
             QList<int>{ConversationListModel::TypingRole});
    QCOMPARE(roleAt(conversations, aliceRow, ConversationListModel::TypingRole),
             true);
    QCOMPARE(roleAt(conversations, roomRow, ConversationListModel::TypingRole),
             false);
    QVERIFY(reducer.directPeerIsTyping(alice, now));
    QVERIFY(!reducer.directPeerIsTyping(room, now));
    QVERIFY(!reducer.directPeerIsTyping(ghost, now));

    reducer.apply(IrcTypingEvent{alice, QStringLiteral("Alice"),
                                 IrcTypingPhase::Done, now});
    conversations.invalidateTyping();
    QCOMPARE(roleAt(conversations, aliceRow, ConversationListModel::TypingRole),
             false);

    reducer.apply(IrcTypingEvent{alice, QStringLiteral("Alice"),
                                 IrcTypingPhase::Active, now});
    conversations.invalidateTyping();
    QCOMPARE(roleAt(conversations, aliceRow, ConversationListModel::TypingRole),
             true);
    reducer.apply(IrcMessageEvent{
        alice, QStringLiteral("Alice"), QStringLiteral("here"), timestamp,
        QStringLiteral("Alice")});
    conversations.invalidateTyping();
    QCOMPARE(roleAt(conversations, aliceRow, ConversationListModel::TypingRole),
             false);
}

void ModelTest::conversationPresenceFollowsDirectPeerAway()
{
    IrcEventReducer reducer;
    ConversationListModel conversations(reducer);
    welcome(reducer, networkA);
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("omairc")});
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("Alice")});
    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    const IrcConversationKey alice =
        reducer.conversationKey(networkA, QStringLiteral("Alice"));
    reducer.apply(IrcMessageEvent{
        alice, QStringLiteral("Alice"), QStringLiteral("hi"), timestamp,
        QStringLiteral("Alice")});
    conversations.reload();

    const int aliceRow = rowFor(conversations, ircConversationId(alice));
    const int roomRow = rowFor(conversations, ircConversationId(room));
    QVERIFY(aliceRow >= 0);
    QVERIFY(roomRow >= 0);

    // A direct message paints the same away fact the member row reads, and a
    // channel carries no peer presence.
    QCOMPARE(roleAt(conversations, aliceRow, ConversationListModel::PresenceRole),
             QStringLiteral("online"));
    QCOMPARE(roleAt(conversations, roomRow, ConversationListModel::PresenceRole),
             QString());

    QSignalSpy changes(&conversations, &QAbstractItemModel::dataChanged);
    reducer.apply(IrcAwayEvent{networkA, QStringLiteral("Alice"),
                               IrcAway{QStringLiteral("lunch")}});
    conversations.reload();
    QVERIFY(!changes.isEmpty());
    QVERIFY(changes.last().at(2).value<QList<int>>().contains(
        ConversationListModel::PresenceRole));
    QCOMPARE(roleAt(conversations, aliceRow, ConversationListModel::PresenceRole),
             QStringLiteral("away"));

    reducer.apply(IrcAwayEvent{networkA, QStringLiteral("Alice"), std::nullopt});
    conversations.reload();
    QCOMPARE(roleAt(conversations, aliceRow, ConversationListModel::PresenceRole),
             QStringLiteral("online"));

    // Losing the last shared channel stops the row claiming online.
    reducer.apply(IrcQuitEvent{networkA, QStringLiteral("Alice"), QString()});
    conversations.reload();
    QCOMPARE(roleAt(conversations, aliceRow, ConversationListModel::PresenceRole),
             QStringLiteral("offline"));
}

void ModelTest::selectedChatAppendInsertsInsteadOfReset()
{
    IrcEventReducer reducer;
    MessageListModel messages(reducer);
    welcome(reducer, networkA);

    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("omairc")});
    messages.select(room);
    QCOMPARE(messages.rowCount(), 1);

    QSignalSpy resets(&messages, &QAbstractItemModel::modelReset);
    QSignalSpy inserts(&messages, &QAbstractItemModel::rowsInserted);
    reducer.apply(IrcMessageEvent{
        room, QStringLiteral("Alice"), QStringLiteral("hello"), timestamp,
        QStringLiteral("#room")});
    messages.reload();

    QCOMPARE(resets.size(), 0);
    QCOMPARE(inserts.size(), 1);
    QCOMPARE(inserts.at(0).at(1).toInt(), 1);
    QCOMPARE(inserts.at(0).at(2).toInt(), 1);
    QCOMPARE(messages.rowCount(), 2);
    QCOMPARE(roleAt(messages, 1, MessageListModel::BodyRole),
             QStringLiteral("hello"));

    reducer.apply(IrcMessageEvent{
        room, QStringLiteral("Bob"), QStringLiteral("second"), timestamp,
        QStringLiteral("#room")});
    messages.reload();
    QCOMPARE(resets.size(), 0);
    QCOMPARE(inserts.size(), 2);
    QCOMPARE(messages.rowCount(), 3);
    QCOMPARE(roleAt(messages, 2, MessageListModel::BodyRole),
             QStringLiteral("second"));

    const IrcConversationKey other =
        reducer.conversationKey(networkA, QStringLiteral("#other"));
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#other"), QStringLiteral("omairc")});
    messages.select(other);
    QCOMPARE(resets.size(), 1);
    QCOMPARE(messages.rowCount(), 1);
}

void ModelTest::sameSizeReloadEmitsDataChangedCoveringFirstRow()
{
    IrcEventReducer reducer;
    MessageListModel messages(reducer);
    welcome(reducer, networkA);

    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    reducer.apply(IrcMessageEvent{
        room, QStringLiteral("Alice"), QStringLiteral("first"), timestamp,
        QStringLiteral("#room")});
    reducer.apply(IrcMessageEvent{
        room, QStringLiteral("Bob"), QStringLiteral("second"), timestamp,
        QStringLiteral("#room")});
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("Carol")});
    messages.select(room);
    const int rows = messages.rowCount();
    QVERIFY(rows >= 3);
    QCOMPARE(roleAt(messages, 0, MessageListModel::BodyRole),
             QStringLiteral("first"));
    QCOMPARE(roleAt(messages, rows - 1, MessageListModel::BodyRole),
             QStringLiteral("Carol joined"));

    QSignalSpy resets(&messages, &QAbstractItemModel::modelReset);
    QSignalSpy inserts(&messages, &QAbstractItemModel::rowsInserted);
    QSignalSpy changes(&messages, &QAbstractItemModel::dataChanged);
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("Dave")});
    messages.reload();

    QCOMPARE(resets.size(), 0);
    QCOMPARE(inserts.size(), 0);
    QCOMPARE(messages.rowCount(), rows);
    QCOMPARE(roleAt(messages, 0, MessageListModel::BodyRole),
             QStringLiteral("first"));
    QCOMPARE(roleAt(messages, rows - 1, MessageListModel::BodyRole),
             QStringLiteral("Carol, Dave joined"));

    bool coversFirstAndLast = false;
    for (int i = 0; i < changes.size(); ++i) {
        const int top = changes.at(i).at(0).toModelIndex().row();
        const int bottom = changes.at(i).at(1).toModelIndex().row();
        if (top <= 0 && bottom >= rows - 1)
            coversFirstAndLast = true;
    }
    QVERIFY2(coversFirstAndLast,
             "same-size reload must emit dataChanged covering row 0, not only the last row");
}

void ModelTest::selectedMemberJoinInsertsInsteadOfReset()
{
    IrcEventReducer reducer;
    MemberListModel members(reducer);
    welcome(reducer, networkA);

    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("omairc")});
    reducer.apply(IrcNamesEvent{
        networkA,
        QStringLiteral("#room"),
        {parsedName("@omairc"), parsedName("Alice")},
        true,
    });
    members.select(room);
    QCOMPARE(members.rowCount(), 2);
    QCOMPARE(roleAt(members, 0, MemberListModel::NickRole), QStringLiteral("omairc"));
    QCOMPARE(roleAt(members, 1, MemberListModel::NickRole), QStringLiteral("Alice"));

    QSignalSpy resets(&members, &QAbstractItemModel::modelReset);
    QSignalSpy inserts(&members, &QAbstractItemModel::rowsInserted);
    QSignalSpy removes(&members, &QAbstractItemModel::rowsRemoved);
    QSignalSpy changes(&members, &QAbstractItemModel::dataChanged);

    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("Bob")});
    members.reload();
    QCOMPARE(resets.size(), 0);
    QCOMPARE(inserts.size(), 1);
    QCOMPARE(inserts.at(0).at(1).toInt(), 2);
    QCOMPARE(members.rowCount(), 3);
    QCOMPARE(roleAt(members, 2, MemberListModel::NickRole), QStringLiteral("Bob"));

    reducer.apply(IrcPartEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("Bob"), {}});
    members.reload();
    QCOMPARE(resets.size(), 0);
    QCOMPARE(removes.size(), 1);
    QCOMPARE(members.rowCount(), 2);
    QCOMPARE(roleAt(members, 0, MemberListModel::NickRole), QStringLiteral("omairc"));

    reducer.apply(IrcModeEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("op"),
        QStringLiteral("+o"), {QStringLiteral("Alice")}});
    members.reload();
    QCOMPARE(resets.size(), 0);
    QVERIFY(changes.size() >= 1);
    QCOMPARE(roleAt(members, 0, MemberListModel::NickRole), QStringLiteral("Alice"));
    QCOMPARE(roleAt(members, 0, MemberListModel::LabelRole), QStringLiteral("@Alice"));

    reducer.apply(IrcNickEvent{
        networkA, QStringLiteral("Alice"), QStringLiteral("Alicia")});
    members.reload();
    QCOMPARE(resets.size(), 0);
    QCOMPARE(removes.size(), 2);
    QCOMPARE(inserts.size(), 2);
    QCOMPARE(members.rowCount(), 2);
    QCOMPARE(roleAt(members, 0, MemberListModel::NickRole), QStringLiteral("Alicia"));

    const IrcConversationKey other =
        reducer.conversationKey(networkA, QStringLiteral("#other"));
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#other"), QStringLiteral("omairc")});
    members.select(other);
    QCOMPARE(resets.size(), 1);
    QCOMPARE(members.rowCount(), 1);
}

void ModelTest::reloadTrimEmitsRemovesWhenCountUnchanged()
{
    IrcEventReducer reducer;
    MessageListModel messages(reducer);
    welcome(reducer, networkA);

    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    for (int i = 0; i < 2000; ++i) {
        reducer.apply(IrcMessageEvent{
            room, QStringLiteral("Alice"), QString::number(i), timestamp,
            QStringLiteral("#room")});
    }
    messages.select(room);
    QCOMPARE(messages.rowCount(), 2000);
    QCOMPARE(roleAt(messages, 0, MessageListModel::BodyRole), QStringLiteral("0"));
    QCOMPARE(roleAt(messages, 1999, MessageListModel::BodyRole),
             QStringLiteral("1999"));

    QSignalSpy resets(&messages, &QAbstractItemModel::modelReset);
    QSignalSpy removes(&messages, &QAbstractItemModel::rowsRemoved);
    QSignalSpy inserts(&messages, &QAbstractItemModel::rowsInserted);
    reducer.apply(IrcMessageEvent{
        room, QStringLiteral("Alice"), QStringLiteral("2000"), timestamp,
        QStringLiteral("#room")});
    messages.reload();

    QCOMPARE(resets.size(), 0);
    QCOMPARE(removes.size(), 1);
    QCOMPARE(removes.at(0).at(1).toInt(), 0);
    QCOMPARE(removes.at(0).at(2).toInt(), 0);
    QCOMPARE(inserts.size(), 1);
    QCOMPARE(inserts.at(0).at(1).toInt(), 1999);
    QCOMPARE(inserts.at(0).at(2).toInt(), 1999);
    QCOMPARE(messages.rowCount(), 2000);
    QCOMPARE(roleAt(messages, 0, MessageListModel::BodyRole), QStringLiteral("1"));
    QCOMPARE(roleAt(messages, 1999, MessageListModel::BodyRole),
             QStringLiteral("2000"));
}

void ModelTest::reloadClearAfterCapEmitsRemoves()
{
    IrcEventReducer reducer;
    MessageListModel messages(reducer);
    welcome(reducer, networkA);

    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    for (int i = 0; i < 2001; ++i) {
        reducer.apply(IrcMessageEvent{
            room, QStringLiteral("Alice"), QString::number(i), timestamp,
            QStringLiteral("#room")});
    }
    messages.select(room);
    QCOMPARE(messages.rowCount(), 2000);

    QSignalSpy resets(&messages, &QAbstractItemModel::modelReset);
    QSignalSpy removes(&messages, &QAbstractItemModel::rowsRemoved);
    reducer.clearMessages(room);
    messages.reload();

    QCOMPARE(resets.size(), 1);
    QCOMPARE(removes.size(), 0);
    QCOMPARE(messages.rowCount(), 0);
}

void ModelTest::collapsedJoinRewritesLastRow()
{
    IrcEventReducer reducer;
    MessageListModel messages(reducer);
    welcome(reducer, networkA);

    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("Alice")});
    messages.select(room);
    QCOMPARE(messages.rowCount(), 1);
    QCOMPARE(roleAt(messages, 0, MessageListModel::BodyRole),
             QStringLiteral("Alice joined"));

    QSignalSpy resets(&messages, &QAbstractItemModel::modelReset);
    QSignalSpy inserts(&messages, &QAbstractItemModel::rowsInserted);
    QSignalSpy changes(&messages, &QAbstractItemModel::dataChanged);
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("Bob")});
    messages.reload();

    QCOMPARE(resets.size(), 0);
    QCOMPARE(inserts.size(), 0);
    QCOMPARE(changes.size(), 1);
    QCOMPARE(changes.at(0).at(0).toModelIndex().row(), 0);
    QCOMPARE(messages.rowCount(), 1);
    QCOMPARE(roleAt(messages, 0, MessageListModel::BodyRole),
             QStringLiteral("Alice, Bob joined"));
}

void ModelTest::originRoleNameAndValues()
{
    IrcEventReducer reducer;
    MessageListModel messages(reducer);
    welcome(reducer, networkA);
    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("omairc")});
    reducer.apply(IrcHistoryEvent{
        room,
        QStringLiteral("#room"),
        {{QStringLiteral("alice"), QStringLiteral("older"), timestamp,
          IrcMessageKindTag::Chat, IrcMsgId{QStringLiteral("id-1")}}},
    });
    messages.select(room);

    QCOMPARE(messages.roleNames()[MessageListModel::OriginRole], QByteArray("origin"));
    QCOMPARE(messages.field(0, QStringLiteral("origin")), QStringLiteral("replay"));
    QCOMPARE(messages.field(0, QStringLiteral("body")), QStringLiteral("older"));
    QCOMPARE(messages.field(0, QStringLiteral("author")), QStringLiteral("alice"));
    QCOMPARE(messages.field(0, QStringLiteral("msgid")), QStringLiteral("id-1"));
    QCOMPARE(messages.field(1, QStringLiteral("origin")), QStringLiteral("live"));
    QCOMPARE(messages.field(1, QStringLiteral("body")), QStringLiteral("omairc joined"));
    QCOMPARE(messages.field(1, QStringLiteral("msgid")), QString());
    QCOMPARE(messages.field(0, QStringLiteral("no-such-role")), QString());
    QCOMPARE(roleAt(messages, 0, MessageListModel::OriginRole),
             QStringLiteral("replay"));
    QCOMPARE(roleAt(messages, 0, MessageListModel::BodyRole),
             QStringLiteral("older"));
    QCOMPARE(roleAt(messages, 1, MessageListModel::OriginRole),
             QStringLiteral("live"));
    QCOMPARE(roleAt(messages, 1, MessageListModel::BodyRole),
             QStringLiteral("omairc joined"));
}

void ModelTest::networkLogFieldLooksUpRolesByName()
{
    IrcNetworkLog log;
    NetworkLogModel lines(log);
    log.append(IrcStatusEntry::lifecycle(
        networkA, IrcLogSeverity::Info, QStringLiteral("NOTICE"),
        QStringLiteral("Looking up your hostname")));
    lines.show(networkA);

    QCOMPARE(lines.roleNames()[NetworkLogModel::LabelRole], QByteArray("label"));
    QCOMPARE(lines.roleNames()[NetworkLogModel::TextRole], QByteArray("text"));
    QCOMPARE(lines.field(0, QStringLiteral("label")), QStringLiteral("NOTICE"));
    QCOMPARE(lines.field(0, QStringLiteral("text")),
             QStringLiteral("Looking up your hostname"));
    QCOMPARE(lines.field(0, QStringLiteral("source")), QStringLiteral("local"));
    QCOMPARE(lines.field(0, QStringLiteral("no-such-role")), QString());
    QCOMPARE(roleAt(lines, 0, NetworkLogModel::LabelRole), QStringLiteral("NOTICE"));
}

void ModelTest::spliceResetsSelectedConversation()
{
    IrcEventReducer reducer;
    MessageListModel messages(reducer);
    welcome(reducer, networkA);
    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("omairc")});
    messages.select(room);
    QCOMPARE(messages.rowCount(), 1);

    QSignalSpy resets(&messages, &QAbstractItemModel::modelReset);
    QSignalSpy inserts(&messages, &QAbstractItemModel::rowsInserted);
    reducer.apply(IrcHistoryEvent{
        room,
        QStringLiteral("#room"),
        {{QStringLiteral("alice"), QStringLiteral("older"), timestamp,
          IrcMessageKindTag::Chat, IrcMsgId{QStringLiteral("id-1")}}},
    });
    messages.reload();

    QCOMPARE(resets.size(), 1);
    QCOMPARE(inserts.size(), 0);
    QCOMPARE(messages.rowCount(), 2);
    QCOMPARE(roleAt(messages, 0, MessageListModel::BodyRole),
             QStringLiteral("older"));
    QCOMPARE(roleAt(messages, 0, MessageListModel::OriginRole),
             QStringLiteral("replay"));
    QCOMPARE(roleAt(messages, 1, MessageListModel::BodyRole),
             QStringLiteral("omairc joined"));
}

void ModelTest::dateSeparatorBetweenLocalDays()
{
    IrcEventReducer reducer;
    ConversationListModel conversations(reducer);
    MessageListModel messages(reducer);
    welcome(reducer, networkA);

    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    const QDate today = QDate::currentDate();
    reducer.apply(IrcMessageEvent{
        room, QStringLiteral("Alice"), QStringLiteral("yesterday-line"),
        atLocal(today.addDays(-1)), QStringLiteral("#room")});
    reducer.apply(IrcMessageEvent{
        room, QStringLiteral("Alice"), QStringLiteral("today-line"),
        atLocal(today), QStringLiteral("#room")});

    conversations.reload();
    messages.select(room);

    QCOMPARE(messages.rowCount(), 3);
    QCOMPARE(roleAt(messages, 0, MessageListModel::BodyRole),
             QStringLiteral("yesterday-line"));
    QCOMPARE(roleAt(messages, 0, MessageListModel::KindRole),
             QStringLiteral("message"));
    QCOMPARE(roleAt(messages, 1, MessageListModel::KindRole),
             QStringLiteral("event"));
    QCOMPARE(roleAt(messages, 1, MessageListModel::BodyRole),
             QStringLiteral("Today"));
    QCOMPARE(roleAt(messages, 1, MessageListModel::TimeRole), QString());
    QCOMPARE(roleAt(messages, 1, MessageListModel::AuthorRole), QString());
    QCOMPARE(roleAt(messages, 2, MessageListModel::BodyRole),
             QStringLiteral("today-line"));
    QCOMPARE(roleAt(conversations, 0, ConversationListModel::UnreadRole), 2);

    IrcEventReducer olderReducer;
    MessageListModel olderMessages(olderReducer);
    welcome(olderReducer, networkA);
    const IrcConversationKey olderRoom =
        olderReducer.conversationKey(networkA, QStringLiteral("#old"));
    const QDate older = today.addDays(-5);
    const QDate newer = today.addDays(-3);
    olderReducer.apply(IrcMessageEvent{
        olderRoom, QStringLiteral("Alice"), QStringLiteral("older-line"),
        atLocal(older), QStringLiteral("#old")});
    olderReducer.apply(IrcMessageEvent{
        olderRoom, QStringLiteral("Alice"), QStringLiteral("newer-line"),
        atLocal(newer), QStringLiteral("#old")});
    olderMessages.select(olderRoom);
    QCOMPARE(olderMessages.rowCount(), 3);
    QCOMPARE(roleAt(olderMessages, 1, MessageListModel::BodyRole),
             expectedDateLabel(newer));
    QCOMPARE(roleAt(olderMessages, 1, MessageListModel::KindRole),
             QStringLiteral("event"));
    QCOMPARE(roleAt(olderMessages, 1, MessageListModel::TimeRole), QString());

    IrcEventReducer yesterdayReducer;
    MessageListModel yesterdayMessages(yesterdayReducer);
    welcome(yesterdayReducer, networkA);
    const IrcConversationKey yesterdayRoom =
        yesterdayReducer.conversationKey(networkA, QStringLiteral("#yday"));
    yesterdayReducer.apply(IrcMessageEvent{
        yesterdayRoom, QStringLiteral("Alice"), QStringLiteral("two-days-ago"),
        atLocal(today.addDays(-2)), QStringLiteral("#yday")});
    yesterdayReducer.apply(IrcMessageEvent{
        yesterdayRoom, QStringLiteral("Alice"), QStringLiteral("yesterday-only"),
        atLocal(today.addDays(-1)), QStringLiteral("#yday")});
    yesterdayMessages.select(yesterdayRoom);
    QCOMPARE(yesterdayMessages.rowCount(), 3);
    QCOMPARE(roleAt(yesterdayMessages, 1, MessageListModel::BodyRole),
             QStringLiteral("Yesterday"));
    QCOMPARE(roleAt(yesterdayMessages, 1, MessageListModel::KindRole),
             QStringLiteral("event"));
    QCOMPARE(roleAt(yesterdayMessages, 1, MessageListModel::TimeRole), QString());
}

void ModelTest::dateSeparatorAfterHistorySplice()
{
    IrcEventReducer reducer;
    MessageListModel messages(reducer);
    welcome(reducer, networkA);

    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("omairc")});
    messages.select(room);
    QCOMPARE(messages.rowCount(), 1);

    const QDate yesterday = QDate::currentDate().addDays(-1);
    QSignalSpy resets(&messages, &QAbstractItemModel::modelReset);
    reducer.apply(IrcHistoryEvent{
        room,
        QStringLiteral("#room"),
        {{QStringLiteral("alice"), QStringLiteral("from-yesterday"),
          atLocal(yesterday, QTime(15, 4)), IrcMessageKindTag::Chat,
          IrcMsgId{QStringLiteral("hist-1")}},
         {QStringLiteral("bob"), QStringLiteral("also-yesterday"),
          atLocal(yesterday, QTime(16, 8)), IrcMessageKindTag::Chat,
          IrcMsgId{QStringLiteral("hist-2")}}},
    });
    messages.reload();

    QCOMPARE(resets.size(), 1);
    QCOMPARE(messages.rowCount(), 4);
    QCOMPARE(roleAt(messages, 0, MessageListModel::BodyRole),
             QStringLiteral("from-yesterday"));
    QCOMPARE(roleAt(messages, 0, MessageListModel::OriginRole),
             QStringLiteral("replay"));
    QCOMPARE(roleAt(messages, 1, MessageListModel::BodyRole),
             QStringLiteral("also-yesterday"));
    QCOMPARE(roleAt(messages, 1, MessageListModel::OriginRole),
             QStringLiteral("replay"));
    QCOMPARE(roleAt(messages, 2, MessageListModel::KindRole),
             QStringLiteral("event"));
    QCOMPARE(roleAt(messages, 2, MessageListModel::BodyRole),
             QStringLiteral("Today"));
    QCOMPARE(roleAt(messages, 2, MessageListModel::TimeRole), QString());
    QCOMPARE(roleAt(messages, 2, MessageListModel::AuthorRole), QString());
    QCOMPARE(roleAt(messages, 3, MessageListModel::BodyRole),
             QStringLiteral("omairc joined"));
    QCOMPARE(roleAt(messages, 3, MessageListModel::OriginRole),
             QStringLiteral("live"));
}

void ModelTest::dateSeparatorBetweenSameNickMidnight()
{
    IrcEventReducer reducer;
    MessageListModel messages(reducer);
    welcome(reducer, networkA);

    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    const QDate today = QDate::currentDate();
    reducer.apply(IrcMessageEvent{
        room, QStringLiteral("Alice"), QStringLiteral("before-midnight"),
        atLocal(today.addDays(-1), QTime(23, 59)), QStringLiteral("#room")});
    reducer.apply(IrcMessageEvent{
        room, QStringLiteral("Alice"), QStringLiteral("after-midnight"),
        atLocal(today, QTime(23, 59)), QStringLiteral("#room")});
    messages.select(room);

    QCOMPARE(messages.rowCount(), 3);
    QCOMPARE(roleAt(messages, 0, MessageListModel::AuthorRole),
             QStringLiteral("Alice"));
    QCOMPARE(roleAt(messages, 0, MessageListModel::TimeRole),
             QStringLiteral("23:59"));
    QCOMPARE(roleAt(messages, 0, MessageListModel::BodyRole),
             QStringLiteral("before-midnight"));
    QCOMPARE(roleAt(messages, 1, MessageListModel::KindRole),
             QStringLiteral("event"));
    QCOMPARE(roleAt(messages, 1, MessageListModel::BodyRole),
             QStringLiteral("Today"));
    QCOMPARE(roleAt(messages, 1, MessageListModel::TimeRole), QString());
    QCOMPARE(roleAt(messages, 2, MessageListModel::AuthorRole),
             QStringLiteral("Alice"));
    QCOMPARE(roleAt(messages, 2, MessageListModel::TimeRole),
             QStringLiteral("23:59"));
    QCOMPARE(roleAt(messages, 2, MessageListModel::BodyRole),
             QStringLiteral("after-midnight"));
}

void ModelTest::clearMessagesDropsDerivedDateSeparators()
{
    IrcEventReducer reducer;
    MessageListModel messages(reducer);
    welcome(reducer, networkA);

    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    const QDate today = QDate::currentDate();
    reducer.apply(IrcMessageEvent{
        room, QStringLiteral("Alice"), QStringLiteral("yesterday-line"),
        atLocal(today.addDays(-1)), QStringLiteral("#room")});
    reducer.apply(IrcMessageEvent{
        room, QStringLiteral("Alice"), QStringLiteral("today-line"),
        atLocal(today), QStringLiteral("#room")});
    messages.select(room);
    QCOMPARE(messages.rowCount(), 3);

    reducer.clearMessages(room);
    messages.reload();
    QCOMPARE(messages.rowCount(), 0);
}

void ModelTest::dateSeparatorAppendsAfterMidnightWithoutReset()
{
    IrcEventReducer reducer;
    MessageListModel messages(reducer);
    welcome(reducer, networkA);

    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    const QDate today = QDate::currentDate();
    reducer.apply(IrcMessageEvent{
        room, QStringLiteral("Alice"), QStringLiteral("yesterday-line"),
        atLocal(today.addDays(-1)), QStringLiteral("#room")});
    messages.select(room);
    QCOMPARE(messages.rowCount(), 1);

    QSignalSpy resets(&messages, &QAbstractItemModel::modelReset);
    QSignalSpy inserts(&messages, &QAbstractItemModel::rowsInserted);
    reducer.apply(IrcMessageEvent{
        room, QStringLiteral("Alice"), QStringLiteral("today-line"),
        atLocal(today), QStringLiteral("#room")});
    messages.reload();

    QCOMPARE(resets.size(), 0);
    QCOMPARE(inserts.size(), 1);
    QCOMPARE(inserts.at(0).at(1).toInt(), 1);
    QCOMPARE(inserts.at(0).at(2).toInt(), 2);
    QCOMPARE(messages.rowCount(), 3);
    QCOMPARE(roleAt(messages, 1, MessageListModel::KindRole),
             QStringLiteral("event"));
    QCOMPARE(roleAt(messages, 1, MessageListModel::BodyRole),
             QStringLiteral("Today"));
    QCOMPARE(roleAt(messages, 1, MessageListModel::TimeRole), QString());
    QCOMPARE(roleAt(messages, 2, MessageListModel::BodyRole),
             QStringLiteral("today-line"));
}

void ModelTest::reloadTrimAcrossDayRemovesLeadingSeparator()
{
    IrcEventReducer reducer;
    MessageListModel messages(reducer);
    welcome(reducer, networkA);

    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    const QDate today = QDate::currentDate();
    reducer.apply(IrcMessageEvent{
        room, QStringLiteral("Alice"), QStringLiteral("yesterday-line"),
        atLocal(today.addDays(-1)), QStringLiteral("#room")});
    for (int i = 0; i < 1999; ++i) {
        reducer.apply(IrcMessageEvent{
            room, QStringLiteral("Alice"), QString::number(i), atLocal(today),
            QStringLiteral("#room")});
    }
    messages.select(room);
    QCOMPARE(messages.rowCount(), 2001);
    QCOMPARE(roleAt(messages, 0, MessageListModel::BodyRole),
             QStringLiteral("yesterday-line"));
    QCOMPARE(roleAt(messages, 1, MessageListModel::BodyRole),
             QStringLiteral("Today"));

    QSignalSpy resets(&messages, &QAbstractItemModel::modelReset);
    QSignalSpy removes(&messages, &QAbstractItemModel::rowsRemoved);
    QSignalSpy inserts(&messages, &QAbstractItemModel::rowsInserted);
    reducer.apply(IrcMessageEvent{
        room, QStringLiteral("Alice"), QStringLiteral("capped"), atLocal(today),
        QStringLiteral("#room")});
    messages.reload();

    QCOMPARE(resets.size(), 0);
    QCOMPARE(removes.size(), 1);
    QCOMPARE(removes.at(0).at(1).toInt(), 0);
    QCOMPARE(removes.at(0).at(2).toInt(), 1);
    QCOMPARE(inserts.size(), 1);
    QCOMPARE(inserts.at(0).at(1).toInt(), 1999);
    QCOMPARE(inserts.at(0).at(2).toInt(), 1999);
    QCOMPARE(messages.rowCount(), 2000);
    QCOMPARE(roleAt(messages, 0, MessageListModel::BodyRole), QStringLiteral("0"));
    QCOMPARE(roleAt(messages, 1999, MessageListModel::BodyRole),
             QStringLiteral("capped"));
}

int runModelTests(int argc, char **argv)
{
    ModelTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_models.moc"
