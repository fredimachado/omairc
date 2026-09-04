#include <QAbstractItemModel>
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

    QCOMPARE(messages.roleNames()[MessageListModel::AuthorRole], QByteArray("author"));
    QCOMPARE(messages.roleNames()[MessageListModel::TimeRole], QByteArray("time"));
    QCOMPARE(messages.roleNames()[MessageListModel::BodyRole], QByteArray("body"));
    QCOMPARE(messages.roleNames()[MessageListModel::KindRole], QByteArray("kind"));
    QCOMPARE(messages.roleNames()[MessageListModel::NetworkIdRole],
             QByteArray("networkId"));

    QCOMPARE(members.roleNames()[MemberListModel::NickRole], QByteArray("nick"));
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
        {{QStringLiteral("omairc"), QStringLiteral("o")},
         {QStringLiteral("Alice"), QString()},
         {QStringLiteral("Bob"), QStringLiteral("v")}},
        true,
    });
    reducer.apply(IrcAwayEvent{networkA, QStringLiteral("Bob"), QStringLiteral("brb")});
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
    QCOMPARE(roleAt(members, 0, MemberListModel::StatusRole), QString());
    QCOMPARE(roleAt(members, 0, MemberListModel::AwayRole), false);
    QCOMPARE(roleAt(members, 1, MemberListModel::NickRole), QStringLiteral("Bob"));
    QCOMPARE(roleAt(members, 1, MemberListModel::StatusRole), QString());
    QCOMPARE(roleAt(members, 1, MemberListModel::AwayRole), true);
    QCOMPARE(roleAt(members, 2, MemberListModel::NickRole), QStringLiteral("omairc"));
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
        {{QStringLiteral("Alice"), QStringLiteral("o")},
         {QStringLiteral("Bob"), QStringLiteral("v")}},
        true,
    });
    reducer.apply(IrcMemberStatusEvent{
        networkA, QStringLiteral("Alice"), QStringLiteral("writing docs")});
    members.select(room);

    QCOMPARE(roleAt(members, 0, MemberListModel::StatusRole),
             QStringLiteral("writing docs"));
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
        {{QStringLiteral("Alice"), QString()}},
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
             QStringLiteral("message"));
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

int runModelTests(int argc, char **argv)
{
    ModelTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_models.moc"
