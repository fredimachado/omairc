#include <QAbstractItemModel>
#include <QSignalSpy>
#include <QTest>

#include "conversationlistmodel.h"
#include "irceventreducer.h"
#include "memberlistmodel.h"
#include "messagelistmodel.h"

namespace
{
const QString networkA = QStringLiteral("network-a");
const QString networkB = QStringLiteral("network-b");
const QDateTime timestamp =
    QDateTime::fromString(QStringLiteral("2026-09-04T00:00:00Z"), Qt::ISODate);

void welcome(IrcEventReducer& reducer,
             const QString& network,
             const QString& nick = QStringLiteral("omairc"))
{
    reducer.apply(IrcWelcomeEvent{network, nick});
}

IrcName parsedName(std::string_view token)
{
    const auto parsed = IrcServerFeatures().parseNamesToken(token);
    return {QString::fromStdString(parsed->nick), parsed->ranks};
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
    void identicalChannelsStayDistinct();
    void selectingZerosUnread();
    void membersEmptyForDirectMessage();
    void messageKinds();
    void conversationsOrderChannelsThenDirect();
    void twoNetworksFollowRosterThenChannelRank();
    void parseConversationIdRejectsBareAndDoubleSeparators();
    void neighborAfterDropNextPreviousGhostAndOnly();
    void neighborAfterDropPrefersSameNetwork();
    void reloadUnchangedKeysEmitsDataChangedNotReset();
    void selectedChatAppendInsertsInsteadOfReset();
    void reloadTrimEmitsRemovesWhenCountUnchanged();
    void reloadClearAfterCapEmitsRemoves();
    void collapsedJoinRewritesLastRow();
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

    QCOMPARE(messages.roleNames()[MessageListModel::AuthorRole], QByteArray("author"));
    QCOMPARE(messages.roleNames()[MessageListModel::TimeRole], QByteArray("time"));
    QCOMPARE(messages.roleNames()[MessageListModel::BodyRole], QByteArray("body"));
    QCOMPARE(messages.roleNames()[MessageListModel::KindRole], QByteArray("kind"));
    QCOMPARE(messages.roleNames()[MessageListModel::NetworkIdRole],
             QByteArray("networkId"));

    QCOMPARE(members.roleNames()[MemberListModel::NickRole], QByteArray("nick"));
    QCOMPARE(members.roleNames()[MemberListModel::LabelRole], QByteArray("label"));
    QCOMPARE(members.roleNames()[MemberListModel::StatusRole], QByteArray("status"));
    QCOMPARE(members.roleNames()[MemberListModel::AwayRole], QByteArray("away"));
    QCOMPARE(members.roleNames()[MemberListModel::NetworkIdRole],
             QByteArray("networkId"));
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
             QStringLiteral("00:00"));
    QCOMPARE(roleAt(messages, 1, MessageListModel::NetworkIdRole), networkA);

    QCOMPARE(members.rowCount(), 3);
    QCOMPARE(roleAt(members, 0, MemberListModel::NickRole), QStringLiteral("Alice"));
    QCOMPARE(roleAt(members, 0, MemberListModel::LabelRole), QStringLiteral("Alice"));
    QCOMPARE(roleAt(members, 0, MemberListModel::StatusRole), QString());
    QCOMPARE(roleAt(members, 0, MemberListModel::AwayRole), false);
    QCOMPARE(roleAt(members, 1, MemberListModel::NickRole), QStringLiteral("Bob"));
    QCOMPARE(roleAt(members, 1, MemberListModel::LabelRole), QStringLiteral("+Bob"));
    QCOMPARE(roleAt(members, 1, MemberListModel::StatusRole), QString());
    QCOMPARE(roleAt(members, 1, MemberListModel::AwayRole), true);
    QCOMPARE(roleAt(members, 2, MemberListModel::NickRole), QStringLiteral("omairc"));
    QCOMPARE(roleAt(members, 2, MemberListModel::LabelRole), QStringLiteral("@omairc"));
    QCOMPARE(roleAt(members, 2, MemberListModel::StatusRole), QString());
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
    reducer.apply(IrcMemberStatusEvent{
        networkA, QStringLiteral("Alice"), QStringLiteral("writing docs")});
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

    messages.select(room);
    QCOMPARE(messages.rowCount(), 4);
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
    QCOMPARE(roleAt(conversations, 0, ConversationListModel::UnreadRole), 1);

    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#other"), QStringLiteral("omairc")});
    conversations.reload();
    QCOMPARE(resets.size(), 1);
    QCOMPARE(conversations.rowCount(), 2);
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

    QCOMPARE(resets.size(), 0);
    QCOMPARE(removes.size(), 1);
    QCOMPARE(removes.at(0).at(1).toInt(), 0);
    QCOMPARE(removes.at(0).at(2).toInt(), 1999);
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

int runModelTests(int argc, char **argv)
{
    ModelTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_models.moc"
