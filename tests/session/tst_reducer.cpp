#include <QTest>

#include "irceventreducer.h"

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
}

class ReducerTest : public QObject
{
    Q_OBJECT

private slots:
    void namesFillAndCompleteWithoutDuplicates();
    void nickAndQuitStayNetworkScoped();
    void selfMembershipControlsChannelLifecycle();
    void directMessagesUseCompositeKeys();
    void identicalChannelsStayIsolated();
    void advertisedChannelTypesCreateChannels();
    void unreadMentionsRespectSelection();
    void welcomeResetsMembership();
};

void ReducerTest::namesFillAndCompleteWithoutDuplicates()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);

    reducer.apply(IrcNamesEvent{
        networkA,
        QStringLiteral("#room"),
        {{QStringLiteral("Alice"), QStringLiteral("o"), false},
         {QStringLiteral("Bob"), QString(), true}},
        false,
    });
    reducer.apply(IrcNamesEvent{
        networkA,
        QStringLiteral("#room"),
        {{QStringLiteral("ALICE"), QStringLiteral("o"), false},
         {QStringLiteral("Carol"), QStringLiteral("v"), false}},
        true,
    });
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("carol")});

    const IrcConversationState *conversation =
        reducer.find(reducer.conversationKey(networkA, QStringLiteral("#room")));
    QVERIFY(conversation);
    QVERIFY(conversation->isChannel());
    QCOMPARE(conversation->peopleCount(), 3);
    QVERIFY(!conversation->channel()->namesSyncing);
    QCOMPARE(conversation->channel()->members.at(QStringLiteral("alice")).status,
             QStringLiteral("o"));
}

void ReducerTest::nickAndQuitStayNetworkScoped()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    welcome(reducer, networkB);
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("Alice")});
    reducer.apply(IrcJoinEvent{
        networkB, QStringLiteral("#room"), QStringLiteral("Alice")});

    reducer.apply(IrcNickEvent{
        networkA, QStringLiteral("ALICE"), QStringLiteral("Alicia")});

    const IrcConversationState *channelA =
        reducer.find(reducer.conversationKey(networkA, QStringLiteral("#room")));
    const IrcConversationState *channelB =
        reducer.find(reducer.conversationKey(networkB, QStringLiteral("#room")));
    QVERIFY(channelA->channel()->members.count(QStringLiteral("alicia")) == 1);
    QVERIFY(channelA->channel()->members.count(QStringLiteral("alice")) == 0);
    QVERIFY(channelB->channel()->members.count(QStringLiteral("alice")) == 1);

    reducer.apply(IrcQuitEvent{
        networkA, QStringLiteral("ALICIA"), QStringLiteral("gone")});
    QCOMPARE(channelA->peopleCount(), 0);
    QCOMPARE(channelB->peopleCount(), 1);
}

void ReducerTest::selfMembershipControlsChannelLifecycle()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#one"), QStringLiteral("OMAIRC")});

    const auto oneKey =
        reducer.conversationKey(networkA, QStringLiteral("#one"));
    const IrcConversationState *one = reducer.find(oneKey);
    QVERIFY(one);
    QVERIFY(one->channel()->joined);

    reducer.apply(IrcPartEvent{
        networkA, QStringLiteral("#one"), QStringLiteral("omairc"), QString()});
    QVERIFY(!one->channel()->joined);
    QCOMPARE(one->peopleCount(), 0);

    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#two"), QStringLiteral("omairc")});
    const IrcConversationState *two = reducer.find(
        reducer.conversationKey(networkA, QStringLiteral("#two")));
    reducer.apply(IrcKickEvent{
        networkA,
        QStringLiteral("#two"),
        QStringLiteral("OMAIRC"),
        QStringLiteral("operator"),
        QStringLiteral("bye"),
    });
    QVERIFY(!two->channel()->joined);
    QCOMPARE(two->peopleCount(), 0);
}

void ReducerTest::directMessagesUseCompositeKeys()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    welcome(reducer, networkB);
    const IrcConversationKey aliceA =
        reducer.conversationKey(networkA, QStringLiteral("Alice"));
    const IrcConversationKey aliceB =
        reducer.conversationKey(networkB, QStringLiteral("Alice"));

    reducer.apply(IrcMessageEvent{
        aliceA, QStringLiteral("ALICE"), QStringLiteral("hello"), timestamp,
        QStringLiteral("Alice")});

    const IrcConversationState *direct = reducer.find(aliceA);
    QVERIFY(direct);
    QVERIFY(!direct->isChannel());
    QCOMPARE(direct->target, QStringLiteral("Alice"));
    QCOMPARE(direct->messages.size(), std::size_t(1));
    QVERIFY(!reducer.find(aliceB));

    reducer.apply(IrcNickEvent{
        networkA, QStringLiteral("alice"), QStringLiteral("Alicia")});
    QVERIFY(!reducer.find(aliceA));
    const IrcConversationState *renamed = reducer.find(
        reducer.conversationKey(networkA, QStringLiteral("alicia")));
    QVERIFY(renamed);
    QCOMPARE(renamed->target, QStringLiteral("Alicia"));
}

void ReducerTest::identicalChannelsStayIsolated()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    welcome(reducer, networkB);
    const IrcConversationKey keyA =
        reducer.conversationKey(networkA, QStringLiteral("#same"));
    const IrcConversationKey keyB =
        reducer.conversationKey(networkB, QStringLiteral("#same"));

    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#same"), QStringLiteral("Alice")});
    reducer.apply(IrcJoinEvent{
        networkB, QStringLiteral("#same"), QStringLiteral("Bob")});
    reducer.apply(IrcMessageEvent{
        keyA, QStringLiteral("Alice"), QStringLiteral("only a"), timestamp,
        QStringLiteral("#same")});
    reducer.apply(IrcMessageEvent{
        keyB, QStringLiteral("Bob"), QStringLiteral("only b"), timestamp,
        QStringLiteral("#same")});

    const IrcConversationState *channelA = reducer.find(keyA);
    const IrcConversationState *channelB = reducer.find(keyB);
    QCOMPARE(channelA->peopleCount(), 1);
    QCOMPARE(channelB->peopleCount(), 1);
    QCOMPARE(channelA->messages.back().body, QStringLiteral("only a"));
    QCOMPARE(channelB->messages.back().body, QStringLiteral("only b"));
}

void ReducerTest::advertisedChannelTypesCreateChannels()
{
    IrcEventReducer reducer;
    IrcServerFeatures features;
    features.applyToken("CHANTYPES=&");
    reducer.setServerFeatures(networkA, features);
    welcome(reducer, networkA);

    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("&local"), QStringLiteral("omairc")});
    const IrcConversationState *channel = reducer.find(
        reducer.conversationKey(networkA, QStringLiteral("&local")));
    QVERIFY(channel);
    QVERIFY(channel->isChannel());
}

void ReducerTest::unreadMentionsRespectSelection()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA, QStringLiteral("Potato[]"));
    const IrcConversationKey selected =
        reducer.conversationKey(networkA, QStringLiteral("#selected"));
    const IrcConversationKey background =
        reducer.conversationKey(networkA, QStringLiteral("#background"));
    reducer.markSelected(selected);

    reducer.apply(IrcMessageEvent{
        selected,
        QStringLiteral("Alice"),
        QStringLiteral("Potato{}: selected"),
        timestamp,
        QStringLiteral("#selected"),
    });
    reducer.apply(IrcMessageEvent{
        background,
        QStringLiteral("Alice"),
        QStringLiteral("hello Potato{}"),
        timestamp,
        QStringLiteral("#background"),
    });
    reducer.apply(IrcMessageEvent{
        background,
        QStringLiteral("Potato{}"),
        QStringLiteral("my own text"),
        timestamp,
        QStringLiteral("#background"),
    });

    const IrcConversationState *selectedState = reducer.find(selected);
    const IrcConversationState *backgroundState = reducer.find(background);
    QCOMPARE(selectedState->unread, 0);
    QCOMPARE(selectedState->mentions, 0);
    QCOMPARE(backgroundState->unread, 1);
    QCOMPARE(backgroundState->mentions, 1);

    reducer.markSelected(background);
    QCOMPARE(backgroundState->unread, 0);
    QCOMPARE(backgroundState->mentions, 0);
}

void ReducerTest::welcomeResetsMembership()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("omairc")});
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("Alice")});

    IrcConversationState const *conversation = reducer.find(
        reducer.conversationKey(networkA, QStringLiteral("#room")));
    QCOMPARE(conversation->peopleCount(), 2);
    QVERIFY(conversation->channel()->joined);

    welcome(reducer, networkA);
    QCOMPARE(conversation->peopleCount(), 0);
    QVERIFY(!conversation->channel()->joined);
}

int runReducerTests(int argc, char **argv)
{
    ReducerTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_reducer.moc"
