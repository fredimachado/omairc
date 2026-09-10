#include <QTest>

#include "irceventreducer.h"
#include "irceventtranslator.h"
#include "ircparser.h"
#include "ircsession.h"

#include <string_view>
#include <vector>

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

IrcMessage mustParse(std::string_view line)
{
    const IrcParseResult parsed = IrcParser::parse(line);
    if (!parsed)
        qFatal("failed to parse IRC line");
    return *parsed.value;
}

IrcReplayLine replayLine(const QString& author,
                         const QString& body,
                         const QString& msgid = {})
{
    return {author, body, timestamp, IrcMessageKindTag::Chat, IrcMsgId{msgid}};
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
    void nickAppendsEventToDirectMessage();
    void nickCaseOnlyUpdatesDirectDisplayNick();
    void identicalChannelsStayIsolated();
    void advertisedChannelTypesCreateChannels();
    void unreadMentionsRespectSelection();
    void mentionArrivalSurvivesSelection();
    void welcomeResetsMembership();
    void awayIsOneFactVisibleInEveryChannel();
    void metadataStatusIsSeparateFromPrefixModes();
    void presenceIsDroppedWithTheLastChannelAndOnWelcome();
    void losingACapabilityClearsTheFactsItFed();
    void modeEditsExistingRowsOnly();
    void joinOfListedNickKeepsRanks();
    void dropDirectMessageErasesOnlyDirectRows();
    void clearMessagesWipesTranscriptKeepsRow();
    void messagesCapAtTwoThousandFifo();
    void clearMessagesEmptiesAfterCap();
    void selfAwayIsNetworkMembershipNotMemberPresence();
    void staleNamesSyncReleasesAfterThirtySeconds();
    void consecutiveJoinsCollapseIntoOneEvent();
    void privmsgBreaksJoinCollapse();
    void kickAndModeStaySeparateFromJoinLine();
    void mixedJoinPartQuitNickCollapse();
    void forgetNetworkLeavesTheOtherNetwork();
    void historySplicesAboveSelfJoin();
    void historyDoesNotMarkUnreadOrMention();
    void msgidDedupSkipsLiveThenReplay();
    void msgidDedupSkipsReplayThenLive();
    void partThenJoinSplicesAboveThisJoin();
    void historicJoinInBatchDoesNotChangePeopleCount();
    void historyAfterPartDoesNotSplice();
    void clearMessagesDropsPendingHistory();
};

void ReducerTest::namesFillAndCompleteWithoutDuplicates()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);

    reducer.apply(IrcNamesEvent{
        networkA,
        QStringLiteral("#room"),
        {parsedName("@Alice"), parsedName("Bob")},
        false,
    });
    reducer.apply(IrcNamesEvent{
        networkA,
        QStringLiteral("#room"),
        {parsedName("@ALICE"), parsedName("+Carol")},
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
    QCOMPARE(conversation->channel()->members.at(QStringLiteral("alice")).ranks,
             parsedName("@Alice").ranks);
    QCOMPARE(reducer.memberView(conversation->key, QStringLiteral("alice"))->label,
             QStringLiteral("@ALICE"));
    QCOMPARE(reducer.memberView(conversation->key, QStringLiteral("alice"))->status,
             QString());
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

void ReducerTest::nickAppendsEventToDirectMessage()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey alice =
        reducer.conversationKey(networkA, QStringLiteral("Alice"));
    reducer.apply(IrcMessageEvent{
        alice, QStringLiteral("Alice"), QStringLiteral("hello"), timestamp,
        QStringLiteral("Alice")});

    reducer.apply(IrcNickEvent{
        networkA, QStringLiteral("Alice"), QStringLiteral("Alicia")});

    const IrcConversationState *renamed = reducer.find(
        reducer.conversationKey(networkA, QStringLiteral("Alicia")));
    QVERIFY(renamed);
    QCOMPARE(renamed->target, QStringLiteral("Alicia"));
    QCOMPARE(renamed->messages.size(), std::size_t(2));
    QCOMPARE(renamed->messages.back().kind, IrcMessageKind::Event);
    QCOMPARE(renamed->messages.back().body, QStringLiteral("Alice is now Alicia"));
}

void ReducerTest::nickCaseOnlyUpdatesDirectDisplayNick()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey alice =
        reducer.conversationKey(networkA, QStringLiteral("Alice"));
    reducer.apply(IrcMessageEvent{
        alice, QStringLiteral("Alice"), QStringLiteral("hello"), timestamp,
        QStringLiteral("Alice")});

    reducer.apply(IrcNickEvent{
        networkA, QStringLiteral("Alice"), QStringLiteral("ALICE")});

    const IrcConversationState *same = reducer.find(alice);
    QVERIFY(same);
    QCOMPARE(same->target, QStringLiteral("ALICE"));
    QCOMPARE(same->messages.size(), std::size_t(2));
    QCOMPARE(same->messages.back().body, QStringLiteral("Alice is now ALICE"));
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

void ReducerTest::mentionArrivalSurvivesSelection()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey selected =
        reducer.conversationKey(networkA, QStringLiteral("#selected"));
    reducer.markSelected(selected);

    reducer.apply(IrcMessageEvent{
        selected,
        QStringLiteral("Alice"),
        QStringLiteral("omairc: ping"),
        timestamp,
        QStringLiteral("#selected"),
    });

    const std::optional<IrcMentionArrival> mention = reducer.takeMentionArrival();
    QVERIFY(mention.has_value());
    QCOMPARE(mention->author, QStringLiteral("Alice"));
    QCOMPARE(mention->body, QStringLiteral("omairc: ping"));
    QCOMPARE(reducer.find(selected)->mentions, 0);
    QVERIFY(!reducer.takeMentionArrival().has_value());

    reducer.apply(IrcMessageEvent{
        selected,
        QStringLiteral("Alice"),
        QStringLiteral("no nick here"),
        timestamp,
        QStringLiteral("#selected"),
    });
    QVERIFY(!reducer.takeMentionArrival().has_value());

    reducer.apply(IrcMessageEvent{
        selected,
        QStringLiteral("omairc"),
        QStringLiteral("omairc: self"),
        timestamp,
        QStringLiteral("#selected"),
    });
    QVERIFY(!reducer.takeMentionArrival().has_value());
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

void ReducerTest::awayIsOneFactVisibleInEveryChannel()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey omarchy =
        reducer.conversationKey(networkA, QStringLiteral("#omarchy"));
    const IrcConversationKey desktop =
        reducer.conversationKey(networkA, QStringLiteral("#desktop"));
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#omarchy"), QStringLiteral("Alice")});
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#desktop"), QStringLiteral("Alice")});

    reducer.apply(IrcAwayEvent{networkA, QStringLiteral("alice"),
                               IrcAway{QStringLiteral("lunch")}});
    reducer.apply(IrcAwayEvent{networkA, QStringLiteral("alice"),
                               IrcAway{QStringLiteral("lunch")}});
    QVERIFY(reducer.memberView(omarchy, QStringLiteral("alice"))->isAway());
    QVERIFY(reducer.memberView(desktop, QStringLiteral("alice"))->isAway());
    QCOMPARE(reducer.memberView(omarchy, QStringLiteral("alice"))->away->reason,
             QStringLiteral("lunch"));

    reducer.apply(IrcNickEvent{
        networkA, QStringLiteral("Alice"), QStringLiteral("Alicia")});
    QVERIFY(reducer.memberView(omarchy, QStringLiteral("alicia"))->isAway());
    QVERIFY(reducer.memberView(desktop, QStringLiteral("alicia"))->isAway());

    reducer.apply(IrcAwayEvent{networkA, QStringLiteral("Alicia"), std::nullopt});
    QVERIFY(!reducer.memberView(omarchy, QStringLiteral("alicia"))->isAway());
    QVERIFY(!reducer.memberView(desktop, QStringLiteral("alicia"))->isAway());
}

void ReducerTest::metadataStatusIsSeparateFromPrefixModes()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    reducer.apply(IrcNamesEvent{
        networkA,
        QStringLiteral("#room"),
        {parsedName("@Alice")},
        true,
    });

    reducer.apply(IrcMemberStatusEvent{
        networkA, QStringLiteral("Alice"), QStringLiteral("writing docs")});
    const std::optional<IrcMemberView> member =
        reducer.memberView(room, QStringLiteral("alice"));
    QCOMPARE(member->status, QStringLiteral("writing docs"));
    QCOMPARE(member->label, QStringLiteral("@Alice"));
    QCOMPARE(member->ranks, parsedName("@Alice").ranks);
    QVERIFY(!member->isAway());

    reducer.apply(IrcMemberStatusEvent{networkA, QStringLiteral("Alice"), QString()});
    QCOMPARE(reducer.memberView(room, QStringLiteral("alice"))->status, QString());
}

void ReducerTest::presenceIsDroppedWithTheLastChannelAndOnWelcome()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey omarchy =
        reducer.conversationKey(networkA, QStringLiteral("#omarchy"));
    const IrcConversationKey desktop =
        reducer.conversationKey(networkA, QStringLiteral("#desktop"));
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#omarchy"), QStringLiteral("Alice")});
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#desktop"), QStringLiteral("Alice")});
    reducer.apply(IrcAwayEvent{networkA, QStringLiteral("Alice"),
                               IrcAway{QStringLiteral("lunch")}});

    reducer.apply(IrcPartEvent{
        networkA, QStringLiteral("#desktop"), QStringLiteral("Alice"), QString()});
    QVERIFY(!reducer.memberView(desktop, QStringLiteral("alice")));
    QVERIFY(reducer.memberView(omarchy, QStringLiteral("alice"))->isAway());

    reducer.apply(IrcQuitEvent{networkA, QStringLiteral("Alice"), QString()});
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#omarchy"), QStringLiteral("Alice")});
    QVERIFY(!reducer.memberView(omarchy, QStringLiteral("alice"))->isAway());

    reducer.apply(IrcAwayEvent{networkA, QStringLiteral("Alice"),
                               IrcAway{QStringLiteral("lunch")}});
    welcome(reducer, networkA);
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#omarchy"), QStringLiteral("Alice")});
    QVERIFY(!reducer.memberView(omarchy, QStringLiteral("alice"))->isAway());
}

void ReducerTest::losingACapabilityClearsTheFactsItFed()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("Alice")});
    reducer.apply(IrcAwayEvent{networkA, QStringLiteral("Alice"),
                               IrcAway{QStringLiteral("lunch")}});
    reducer.apply(IrcMemberStatusEvent{
        networkA, QStringLiteral("Alice"), QStringLiteral("writing docs")});

    reducer.clearPresenceFacts(networkA, true, false);
    QVERIFY(!reducer.memberView(room, QStringLiteral("alice"))->isAway());
    QCOMPARE(reducer.memberView(room, QStringLiteral("alice"))->status,
             QStringLiteral("writing docs"));

    reducer.clearPresenceFacts(networkA, false, true);
    QCOMPARE(reducer.memberView(room, QStringLiteral("alice"))->status, QString());
}

void ReducerTest::modeEditsExistingRowsOnly()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    reducer.apply(IrcNamesEvent{
        networkA,
        QStringLiteral("#room"),
        {parsedName("Alice")},
        true,
    });

    reducer.apply(IrcModeEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("op"),
        QStringLiteral("+o"), {QStringLiteral("alice")}});
    QCOMPARE(reducer.memberView(room, QStringLiteral("alice"))->label,
             QStringLiteral("@Alice"));

    reducer.apply(IrcModeEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("op"),
        QStringLiteral("+o"), {QStringLiteral("alice")}});
    QCOMPARE(reducer.memberView(room, QStringLiteral("alice"))->label,
             QStringLiteral("@Alice"));

    reducer.apply(IrcModeEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("op"),
        QStringLiteral("+v"), {QStringLiteral("alice")}});
    QCOMPARE(reducer.memberView(room, QStringLiteral("alice"))->label,
             QStringLiteral("@Alice"));

    reducer.apply(IrcModeEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("op"),
        QStringLiteral("-o"), {QStringLiteral("alice")}});
    QCOMPARE(reducer.memberView(room, QStringLiteral("alice"))->label,
             QStringLiteral("+Alice"));

    reducer.apply(IrcModeEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("op"),
        QStringLiteral("-v"), {QStringLiteral("alice")}});
    QCOMPARE(reducer.memberView(room, QStringLiteral("alice"))->label,
             QStringLiteral("Alice"));

    reducer.apply(IrcModeEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("op"),
        QStringLiteral("+o"), {QStringLiteral("ghost")}});
    QVERIFY(!reducer.memberView(room, QStringLiteral("ghost")));
    QCOMPARE(reducer.find(room)->peopleCount(), 1);
}

void ReducerTest::joinOfListedNickKeepsRanks()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    reducer.apply(IrcNamesEvent{
        networkA,
        QStringLiteral("#room"),
        {parsedName("@+Alice")},
        true,
    });
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("alice")});
    QCOMPARE(reducer.memberView(room, QStringLiteral("alice"))->label,
             QStringLiteral("@alice"));
    QCOMPARE(reducer.memberView(room, QStringLiteral("alice"))->nick,
             QStringLiteral("alice"));
}

void ReducerTest::dropDirectMessageErasesOnlyDirectRows()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey lena =
        reducer.conversationKey(networkA, QStringLiteral("lena"));
    const IrcConversationKey channel =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    reducer.apply(IrcMessageEvent{
        lena, QStringLiteral("lena"), QStringLiteral("hi"), timestamp,
        QStringLiteral("lena")});
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("omairc")});
    reducer.markSelected(lena);

    QCOMPARE(reducer.conversations().size(), std::size_t(2));
    QVERIFY(reducer.dropDirectMessage(lena));
    QVERIFY(!reducer.find(lena));
    QVERIFY(reducer.find(channel));
    QVERIFY(reducer.find(channel)->isChannel());
    QCOMPARE(reducer.conversations().size(), std::size_t(1));
    QVERIFY(!reducer.dropDirectMessage(lena));
    QVERIFY(!reducer.dropDirectMessage(channel));
    QVERIFY(reducer.find(channel));
    QCOMPARE(reducer.conversations().size(), std::size_t(1));

    reducer.apply(IrcMessageEvent{
        lena, QStringLiteral("lena"), QStringLiteral("again"), timestamp,
        QStringLiteral("lena")});
    const IrcConversationState *recreated = reducer.find(lena);
    QVERIFY(recreated);
    QCOMPARE(recreated->unread, 1);
}

void ReducerTest::clearMessagesWipesTranscriptKeepsRow()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey lena =
        reducer.conversationKey(networkA, QStringLiteral("lena"));
    const IrcConversationKey channel =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    const IrcConversationKey missing =
        reducer.conversationKey(networkA, QStringLiteral("ghost"));

    reducer.apply(IrcMessageEvent{
        lena, QStringLiteral("lena"), QStringLiteral("hi"), timestamp,
        QStringLiteral("lena")});
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("omairc")});
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("Alice")});
    reducer.apply(IrcTopicEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("topic"),
        QStringLiteral("op")});
    reducer.apply(IrcMessageEvent{
        channel, QStringLiteral("Alice"), QStringLiteral("hello"), timestamp,
        QStringLiteral("#room")});

    QCOMPARE(reducer.conversations().size(), std::size_t(2));
    QVERIFY(reducer.find(lena));
    QVERIFY(!reducer.find(lena)->isChannel());
    QCOMPARE(reducer.find(lena)->messages.size(), std::size_t(1));

    reducer.clearMessages(lena);
    const IrcConversationState *direct = reducer.find(lena);
    QVERIFY(direct);
    QVERIFY(!direct->isChannel());
    QCOMPARE(direct->messages.size(), std::size_t(0));
    QCOMPARE(reducer.conversations().size(), std::size_t(2));

    reducer.clearMessages(lena);
    QCOMPARE(reducer.find(lena)->messages.size(), std::size_t(0));
    QVERIFY(reducer.find(lena));

    const IrcConversationState *room = reducer.find(channel);
    QVERIFY(room);
    QVERIFY(room->isChannel());
    QCOMPARE(room->peopleCount(), 2);
    QCOMPARE(room->channel()->topic, QStringLiteral("topic"));
    QVERIFY(!room->messages.empty());
    reducer.clearMessages(channel);
    QCOMPARE(room->messages.size(), std::size_t(0));
    QCOMPARE(room->peopleCount(), 2);
    QCOMPARE(room->channel()->topic, QStringLiteral("topic"));
    QVERIFY(room->isChannel());

    QVERIFY(!reducer.find(missing));
    reducer.clearMessages(missing);
    QVERIFY(!reducer.find(missing));
    QCOMPARE(reducer.conversations().size(), std::size_t(2));

    QVERIFY(reducer.dropDirectMessage(lena));
    QVERIFY(!reducer.find(lena));
    QVERIFY(reducer.find(channel));
    QVERIFY(!reducer.dropDirectMessage(channel));
    QVERIFY(reducer.find(channel));
    QCOMPARE(reducer.conversations().size(), std::size_t(1));
}

void ReducerTest::messagesCapAtTwoThousandFifo()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));

    for (int i = 0; i < 2001; ++i) {
        reducer.apply(IrcMessageEvent{
            room, QStringLiteral("Alice"), QString::number(i), timestamp,
            QStringLiteral("#room")});
    }

    const IrcConversationState *conversation = reducer.find(room);
    QVERIFY(conversation);
    QCOMPARE(conversation->messages.size(), std::size_t(2000));
    QCOMPARE(conversation->messages.front().body, QStringLiteral("1"));
    QCOMPARE(conversation->messages.back().body, QStringLiteral("2000"));
    QCOMPARE(conversation->trimmed, 1);

    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("Bob")});
    QCOMPARE(conversation->messages.size(), std::size_t(2000));
    QCOMPARE(conversation->messages.front().body, QStringLiteral("2"));
    QCOMPARE(conversation->messages.back().body, QStringLiteral("Bob joined"));
    QCOMPARE(conversation->messages.back().kind, IrcMessageKind::Event);
    QCOMPARE(conversation->trimmed, 2);
}

void ReducerTest::clearMessagesEmptiesAfterCap()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));

    for (int i = 0; i < 2001; ++i) {
        reducer.apply(IrcMessageEvent{
            room, QStringLiteral("Alice"), QString::number(i), timestamp,
            QStringLiteral("#room")});
    }

    const IrcConversationState *conversation = reducer.find(room);
    QVERIFY(conversation);
    QCOMPARE(conversation->messages.size(), std::size_t(2000));
    reducer.clearMessages(room);
    QCOMPARE(conversation->messages.size(), std::size_t(0));
    QCOMPARE(conversation->trimmed, 2001);
    QVERIFY(reducer.find(room));
}

void ReducerTest::selfAwayIsNetworkMembershipNotMemberPresence()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey omarchy =
        reducer.conversationKey(networkA, QStringLiteral("#omarchy"));
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#omarchy"), QStringLiteral("omairc")});
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#desktop"), QStringLiteral("omairc")});

    reducer.apply(IrcAwayEvent{networkA, QStringLiteral("omairc"), IrcAway{}});
    QVERIFY(!reducer.selfAway(networkA));
    QVERIFY(reducer.memberView(omarchy, QStringLiteral("omairc"))->isAway());

    reducer.apply(IrcSelfAwayEvent{networkA, true});
    reducer.apply(IrcSelfAwayEvent{networkA, true});
    QVERIFY(reducer.selfAway(networkA));

    reducer.apply(IrcPartEvent{
        networkA, QStringLiteral("#omarchy"), QStringLiteral("omairc"), QString()});
    reducer.apply(IrcPartEvent{
        networkA, QStringLiteral("#desktop"), QStringLiteral("omairc"), QString()});
    QVERIFY(reducer.selfAway(networkA));

    reducer.clearPresenceFacts(networkA, true, true);
    QVERIFY(reducer.selfAway(networkA));

    reducer.apply(IrcSelfAwayEvent{networkA, false});
    reducer.apply(IrcSelfAwayEvent{networkA, false});
    QVERIFY(!reducer.selfAway(networkA));

    reducer.apply(IrcSelfAwayEvent{networkA, true});
    welcome(reducer, networkA);
    QVERIFY(!reducer.selfAway(networkA));
    QVERIFY(!reducer.selfAway(networkB));
}

void ReducerTest::staleNamesSyncReleasesAfterThirtySeconds()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    reducer.apply(IrcNamesEvent{
        networkA,
        QStringLiteral("#room"),
        {parsedName("Alice")},
        false,
    });
    const IrcConversationKey key =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    const IrcConversationState *conversation = reducer.find(key);
    QVERIFY(conversation);
    const IrcChannelState *channel = conversation->channel();
    QVERIFY(channel);
    QVERIFY(channel->namesSyncing);
    QVERIFY(channel->namesSyncStarted.isValid());
    const QDateTime started = channel->namesSyncStarted;

    QVERIFY(!reducer.releaseStaleNamesSync(key, started.addSecs(29)));
    QVERIFY(channel->namesSyncing);

    QVERIFY(reducer.releaseStaleNamesSync(key, started.addSecs(31)));
    QVERIFY(!channel->namesSyncing);
}

void ReducerTest::consecutiveJoinsCollapseIntoOneEvent()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("Alice")});
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("Bob")});

    const IrcConversationState *conversation = reducer.find(
        reducer.conversationKey(networkA, QStringLiteral("#room")));
    QVERIFY(conversation);
    QCOMPARE(conversation->messages.size(), std::size_t(1));
    QCOMPARE(conversation->messages.back().kind, IrcMessageKind::Event);
    QCOMPARE(conversation->messages.back().body, QStringLiteral("Alice, Bob joined"));
    QCOMPARE(conversation->trimmed, 0);
}

void ReducerTest::privmsgBreaksJoinCollapse()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("Alice")});
    reducer.apply(IrcMessageEvent{
        room, QStringLiteral("Alice"), QStringLiteral("hello"), timestamp,
        QStringLiteral("#room")});
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("Bob")});

    const IrcConversationState *conversation = reducer.find(room);
    QVERIFY(conversation);
    QCOMPARE(conversation->messages.size(), std::size_t(3));
    QCOMPARE(conversation->messages[0].body, QStringLiteral("Alice joined"));
    QCOMPARE(conversation->messages[1].kind, IrcMessageKind::Message);
    QCOMPARE(conversation->messages[1].body, QStringLiteral("hello"));
    QCOMPARE(conversation->messages[2].body, QStringLiteral("Bob joined"));
}

void ReducerTest::kickAndModeStaySeparateFromJoinLine()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("Alice")});
    reducer.apply(IrcKickEvent{
        networkA,
        QStringLiteral("#room"),
        QStringLiteral("Alice"),
        QStringLiteral("op"),
        QStringLiteral("bye"),
    });
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("Bob")});
    reducer.apply(IrcModeEvent{
        networkA,
        QStringLiteral("#room"),
        QStringLiteral("op"),
        QStringLiteral("+v"),
        {QStringLiteral("Bob")},
    });
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("Carol")});

    const IrcConversationState *conversation = reducer.find(
        reducer.conversationKey(networkA, QStringLiteral("#room")));
    QVERIFY(conversation);
    QCOMPARE(conversation->messages.size(), std::size_t(5));
    QCOMPARE(conversation->messages[0].body, QStringLiteral("Alice joined"));
    QCOMPARE(conversation->messages[1].body, QStringLiteral("Alice was kicked"));
    QCOMPARE(conversation->messages[2].body, QStringLiteral("Bob joined"));
    QCOMPARE(conversation->messages[3].body, QStringLiteral("op set mode +v"));
    QCOMPARE(conversation->messages[4].body, QStringLiteral("Carol joined"));
}

void ReducerTest::mixedJoinPartQuitNickCollapse()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("Alice")});
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("Bob")});
    reducer.apply(IrcPartEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("Alice"), QString()});
    reducer.apply(IrcQuitEvent{
        networkA, QStringLiteral("Bob"), QStringLiteral("gone")});
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("Carol")});
    reducer.apply(IrcNickEvent{
        networkA, QStringLiteral("Carol"), QStringLiteral("Caroline")});

    const IrcConversationState *conversation = reducer.find(
        reducer.conversationKey(networkA, QStringLiteral("#room")));
    QVERIFY(conversation);
    QCOMPARE(conversation->messages.size(), std::size_t(1));
    QCOMPARE(conversation->messages.back().body,
             QStringLiteral("Alice, Bob joined, Alice left, Bob quit, Carol joined, Carol is now Caroline"));
}

void ReducerTest::forgetNetworkLeavesTheOtherNetwork()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    welcome(reducer, networkB);
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#alpha"), QStringLiteral("omairc")});
    reducer.apply(IrcJoinEvent{
        networkB, QStringLiteral("#lab"), QStringLiteral("omairc")});
    reducer.apply(IrcMessageEvent{
        reducer.conversationKey(networkB, QStringLiteral("rio")),
        QStringLiteral("rio"), QStringLiteral("ping"), timestamp,
        QStringLiteral("rio")});
    reducer.markSelected(reducer.conversationKey(networkB, QStringLiteral("#lab")));

    reducer.forgetNetwork(networkB);
    QVERIFY(reducer.find(reducer.conversationKey(networkA, QStringLiteral("#alpha"))));
    QVERIFY(!reducer.find(reducer.conversationKey(networkB, QStringLiteral("#lab"))));
    QVERIFY(!reducer.find(reducer.conversationKey(networkB, QStringLiteral("rio"))));
    QVERIFY(!reducer.selected().has_value());
    QCOMPARE(reducer.conversations().size(), std::size_t(1));
}

void ReducerTest::historySplicesAboveSelfJoin()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#omarchy"), QStringLiteral("omairc")});
    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#omarchy"));
    reducer.apply(IrcHistoryEvent{
        room,
        QStringLiteral("#omarchy"),
        {replayLine(QStringLiteral("alice"), QStringLiteral("older"),
                    QStringLiteral("id-old"))},
    });

    const IrcConversationState *conversation = reducer.find(room);
    QVERIFY(conversation);
    QCOMPARE(conversation->messages.size(), std::size_t(2));
    QCOMPARE(conversation->messages[0].body, QStringLiteral("older"));
    QCOMPARE(conversation->messages[0].author, QStringLiteral("alice"));
    QCOMPARE(conversation->messages[0].origin, IrcOrigin::Replay);
    QCOMPARE(conversation->messages[1].body, QStringLiteral("omairc joined"));
    QCOMPARE(conversation->messages[1].origin, IrcOrigin::Live);
    QVERIFY(!conversation->messages[1].collapsible);
}

void ReducerTest::historyDoesNotMarkUnreadOrMention()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#omarchy"), QStringLiteral("omairc")});
    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#omarchy"));
    reducer.apply(IrcHistoryEvent{
        room,
        QStringLiteral("#omarchy"),
        {replayLine(QStringLiteral("alice"), QStringLiteral("omairc: ping"),
                    QStringLiteral("id-mention"))},
    });

    const IrcConversationState *conversation = reducer.find(room);
    QVERIFY(conversation);
    QCOMPARE(conversation->unread, 0);
    QCOMPARE(conversation->mentions, 0);
    QVERIFY(!reducer.takeMentionArrival());
}

void ReducerTest::msgidDedupSkipsLiveThenReplay()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#omarchy"));
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#omarchy"), QStringLiteral("omairc")});
    reducer.apply(IrcMessageEvent{
        room, QStringLiteral("alice"), QStringLiteral("first"), timestamp,
        QStringLiteral("#omarchy"), IrcMsgId{QStringLiteral("same")}});
    reducer.apply(IrcHistoryEvent{
        room,
        QStringLiteral("#omarchy"),
        {replayLine(QStringLiteral("alice"), QStringLiteral("second"),
                    QStringLiteral("same"))},
    });

    const IrcConversationState *conversation = reducer.find(room);
    QVERIFY(conversation);
    QCOMPARE(conversation->messages.size(), std::size_t(2));
    QCOMPARE(conversation->messages[0].body, QStringLiteral("omairc joined"));
    QCOMPARE(conversation->messages[1].body, QStringLiteral("first"));
    QCOMPARE(conversation->messages[1].origin, IrcOrigin::Live);
}

void ReducerTest::msgidDedupSkipsReplayThenLive()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#omarchy"));
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#omarchy"), QStringLiteral("omairc")});
    reducer.apply(IrcHistoryEvent{
        room,
        QStringLiteral("#omarchy"),
        {replayLine(QStringLiteral("alice"), QStringLiteral("first"),
                    QStringLiteral("same"))},
    });
    reducer.apply(IrcMessageEvent{
        room, QStringLiteral("alice"), QStringLiteral("second"), timestamp,
        QStringLiteral("#omarchy"), IrcMsgId{QStringLiteral("same")}});

    const IrcConversationState *conversation = reducer.find(room);
    QVERIFY(conversation);
    QCOMPARE(conversation->messages.size(), std::size_t(2));
    QCOMPARE(conversation->messages[0].body, QStringLiteral("first"));
    QCOMPARE(conversation->messages[0].origin, IrcOrigin::Replay);
    QCOMPARE(conversation->messages[1].body, QStringLiteral("omairc joined"));
}

void ReducerTest::partThenJoinSplicesAboveThisJoin()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#omarchy"));
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#omarchy"), QStringLiteral("omairc")});
    reducer.apply(IrcPartEvent{
        networkA, QStringLiteral("#omarchy"), QStringLiteral("omairc"), QString()});
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#omarchy"), QStringLiteral("omairc")});
    reducer.apply(IrcHistoryEvent{
        room,
        QStringLiteral("#omarchy"),
        {replayLine(QStringLiteral("alice"), QStringLiteral("backlog"),
                    QStringLiteral("id-rejoin"))},
    });

    const IrcConversationState *conversation = reducer.find(room);
    QVERIFY(conversation);
    QCOMPARE(conversation->messages.size(), std::size_t(4));
    QCOMPARE(conversation->messages[0].body, QStringLiteral("omairc joined"));
    QCOMPARE(conversation->messages[1].body, QStringLiteral("omairc left"));
    QCOMPARE(conversation->messages[2].body, QStringLiteral("backlog"));
    QCOMPARE(conversation->messages[2].origin, IrcOrigin::Replay);
    QCOMPARE(conversation->messages[3].body, QStringLiteral("omairc joined"));
    QVERIFY(!conversation->messages[3].collapsible);
}

void ReducerTest::historyAfterPartDoesNotSplice()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#omarchy"));
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#omarchy"), QStringLiteral("omairc")});
    reducer.apply(IrcPartEvent{
        networkA, QStringLiteral("#omarchy"), QStringLiteral("omairc"), QString()});
    reducer.apply(IrcHistoryEvent{
        room,
        QStringLiteral("#omarchy"),
        {replayLine(QStringLiteral("alice"), QStringLiteral("stale"),
                    QStringLiteral("id-stale"))},
    });

    const IrcConversationState *conversation = reducer.find(room);
    QVERIFY(conversation);
    QCOMPARE(conversation->messages.size(), std::size_t(2));
    QCOMPARE(conversation->messages[0].body, QStringLiteral("omairc joined"));
    QCOMPARE(conversation->messages[1].body, QStringLiteral("omairc left"));
}

void ReducerTest::clearMessagesDropsPendingHistory()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#omarchy"));
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#omarchy"), QStringLiteral("omairc")});
    reducer.clearMessages(room);
    reducer.apply(IrcHistoryEvent{
        room,
        QStringLiteral("#omarchy"),
        {replayLine(QStringLiteral("alice"), QStringLiteral("older"),
                    QStringLiteral("id-clear"))},
    });

    const IrcConversationState *conversation = reducer.find(room);
    QVERIFY(conversation);
    QCOMPARE(conversation->messages.size(), std::size_t(0));
    QCOMPARE(conversation->peopleCount(), 1);
    QVERIFY(conversation->isChannel());
    QVERIFY(!conversation->channel()->historyAnchor);
}

void ReducerTest::historicJoinInBatchDoesNotChangePeopleCount()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#omarchy"), QStringLiteral("omairc")});
    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#omarchy"));
    QCOMPARE(reducer.find(room)->peopleCount(), 1);

    IrcHistoryBatch batch;
    batch.target = QStringLiteral("#omarchy");
    batch.lines.push_back(mustParse(":alice!u@h JOIN :#omarchy"));
    batch.lines.push_back(mustParse(":alice!u@h PRIVMSG #omarchy :from history"));
    const auto event = IrcEventTranslator::translateHistory(
        networkA, QStringLiteral("omairc"), IrcServerFeatures(), batch);
    QVERIFY(event);
    QCOMPARE(event->lines.size(), std::size_t(1));
    QCOMPARE(event->lines.front().body, QStringLiteral("from history"));
    reducer.apply(*event);

    const IrcConversationState *conversation = reducer.find(room);
    QVERIFY(conversation);
    QCOMPARE(conversation->peopleCount(), 1);
    QCOMPARE(conversation->messages[0].body, QStringLiteral("from history"));
}

int runReducerTests(int argc, char **argv)
{
    ReducerTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_reducer.moc"
