#include <QTest>

#include "ircconversationlog.h"
#include "irceventreducer.h"
#include "irceventtranslator.h"
#include "ircparser.h"
#include "ircpresence.h"
#include "ircsession.h"
#include "ircviewnotify.h"

#include <QTemporaryDir>

#include <algorithm>
#include <initializer_list>
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
    return {author, body, timestamp, IrcMessageKindTag::Chat, IrcMsgId{msgid}, {}};
}

void applyWire(IrcEventReducer& reducer, std::string_view line)
{
    const IrcMessage message = mustParse(line);
    for (const IrcEvent& event : IrcEventTranslator::translate(
             networkA, QStringLiteral("omairc"),
             reducer.serverFeatures(networkA), message)) {
        reducer.apply(event);
    }
}

const IrcConversationState *roomOf(
    const IrcEventReducer& reducer,
    const QString& channel = QStringLiteral("#room"))
{
    return reducer.find(reducer.conversationKey(networkA, channel));
}

QString lastBody(const IrcEventReducer& reducer,
                 const QString& channel = QStringLiteral("#room"))
{
    const IrcConversationState *conversation = roomOf(reducer, channel);
    if (!conversation || conversation->messages.empty())
        return {};
    return conversation->messages.back().body;
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
    void windowInactiveMarksSelectedChatUnread();
    void markReadConsumesUnreadButKeepsMark();
    void mentionArrivalSurvivesSelection();
    void mentionArrivalOnDirectMessage();
    void mentionArrivalCarriesNetworkTargetAndMsgid();
    void mutedChatDoesNotMention();
    void highlightWordMentionsLikeNick();
    void welcomeResetsMembership();
    void awayIsOneFactVisibleInEveryChannel();
    void metadataStatusIsSeparateFromPrefixModes();
    void metadataKeysStoreIndependentlyOfAwayAndPrefix();
    void presenceIsDroppedWithTheLastChannelAndOnWelcome();
    void losingACapabilityClearsTheFactsItFed();
    void modeEditsExistingRowsOnly();
    void joinOfListedNickKeepsRanks();
    void dropDirectMessageErasesOnlyDirectRows();
    void dropChannelErasesOnlyChannelRows();
    void clearMessagesWipesTranscriptKeepsRow();
    void messagesCapAtTwoThousandFifo();
    void clearMessagesEmptiesAfterCap();
    void selfAwayIsNetworkMembershipNotMemberPresence();
    void selfAwayShowsOnOurOwnRowInEveryChannel();
    void peerPresenceFollowsSharedChannelAndAwayFacts();
    void awayReloadsConversationsOnlyWhenItCanReachADirectRow();
    void metadataNotifyRefreshesOnlyAffectedSurfaces();
    void falseyBotValuesAreNotBots();
    void staleNamesSyncReleasesAfterThirtySeconds();
    void consecutiveJoinsCollapseIntoOneEvent();
    void privmsgBreaksJoinCollapse();
    void kickAndModeStaySeparateFromJoinLine();
    void mixedJoinPartQuitNickCollapse();
    void forgetNetworkLeavesTheOtherNetwork();
    void historySplicesAboveSelfJoin();
    void historyMarksUnreadLikeLiveWhenUnselected();
    void historyReplayPlantsUnreadMarkLikeLive();
    void replayWhileUnfocusedDoesNotPlantUnreadMarkOnSelected();
    void mutedChatStillPlantsUnreadMark();
    void nickMergeAdoptsOrKeepsUnreadMark();
    void msgidDedupSkipsLiveThenReplay();
    void msgidDedupSkipsReplayThenLive();
    void replayDistinctMsgidsWithIdenticalContentRetained();
    void nickMergeDropsDuplicateMsgids();
    void partThenJoinSplicesAboveThisJoin();
    void historicJoinInBatchDoesNotChangePeopleCount();
    void bouncerQueryPlaybackKeepsPreviousNick();
    void bouncerQueryPreviousNickEchoIsOwnLine();
    void transcriptHydrateIsReplayWithoutNotify();
    void transcriptMsgidSkipsLaterLive();
    void historyAfterPartDoesNotSplice();
    void historyAfterCapDoesNotSplice();
    void clearMessagesDropsPendingHistory();
    void queryReplayFromPeerOpensDirectMessage();
    void selfOnlyQueryReplayDoesNotOpenDirectMessageForMsg();
    void selfOnlyPlaybackThenPeerSplicesInOrder();
    void playbackBatchKeptTracksSpliceAndDedup();
    void cappedPlaybackSpliceNotesOnlySurvivingLines();
    void channelPlaybackPreviousNickStaysMutedBacklog();
    void channelPlaybackCasemappingRememberedSelfNick();
    void queryReplayAppendsAtTailOfExistingDirectMessage();
    void channelReplayWithoutConversationCreatesNothing();
    void nickCollisionMergesMessageIds();
    void conversationCauseInsertTable();
    void ensureConversationHonorsCause();
    void nickShapedJoinDoesNotInventDirect();
    void extendedJoinRecordsAccountOnOneLine();
    void accountCommandAndTagShareOneField();
    void nickChangeKeepsServicesAccount();
    void accountChangeRefreshesMemberRowAndTranscript();
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

void ReducerTest::windowInactiveMarksSelectedChatUnread()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey selected =
        reducer.conversationKey(networkA, QStringLiteral("#selected"));
    const IrcConversationKey background =
        reducer.conversationKey(networkA, QStringLiteral("#background"));
    reducer.markSelected(selected);

    reducer.apply(IrcMessageEvent{
        selected,
        QStringLiteral("Alice"),
        QStringLiteral("while-focused"),
        timestamp,
        QStringLiteral("#selected"),
    });
    const IrcConversationState *selectedState = reducer.find(selected);
    QCOMPARE(selectedState->unread, 0);
    QVERIFY(!selectedState->unreadMark.has_value());

    reducer.setWindowActive(false);
    reducer.apply(IrcMessageEvent{
        selected,
        QStringLiteral("Bob"),
        QStringLiteral("first-unfocused"),
        timestamp,
        QStringLiteral("#selected"),
    });
    reducer.apply(IrcMessageEvent{
        selected,
        QStringLiteral("Bob"),
        QStringLiteral("second-unfocused"),
        timestamp,
        QStringLiteral("#selected"),
    });
    QCOMPARE(selectedState->unread, 2);
    QVERIFY(selectedState->unreadMark.has_value());
    QCOMPARE(*selectedState->unreadMark,
             selectedState->messages[1].sequence);

    reducer.setWindowActive(true);
    reducer.apply(IrcMessageEvent{
        selected,
        QStringLiteral("Bob"),
        QStringLiteral("while-focused-again"),
        timestamp,
        QStringLiteral("#selected"),
    });
    QCOMPARE(selectedState->unread, 2);
    QVERIFY(selectedState->unreadMark.has_value());

    // A background conversation still plants unread regardless of focus.
    reducer.apply(IrcMessageEvent{
        background,
        QStringLiteral("Alice"),
        QStringLiteral("background"),
        timestamp,
        QStringLiteral("#background"),
    });
    QCOMPARE(reducer.find(background)->unread, 1);
    QVERIFY(reducer.find(background)->unreadMark.has_value());
}

void ReducerTest::markReadConsumesUnreadButKeepsMark()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey selected =
        reducer.conversationKey(networkA, QStringLiteral("#selected"));
    reducer.markSelected(selected);
    reducer.setWindowActive(false);

    reducer.apply(IrcMessageEvent{
        selected,
        QStringLiteral("Alice"),
        QStringLiteral("omairc: away ping"),
        timestamp,
        QStringLiteral("#selected"),
    });
    const IrcConversationState *selectedState = reducer.find(selected);
    QCOMPARE(selectedState->unread, 1);
    QCOMPARE(selectedState->mentions, 1);
    QVERIFY(selectedState->unreadMark.has_value());
    QVERIFY(reducer.takeMentionArrival().has_value());
    QVERIFY(!reducer.takeInboxArrival().has_value());

    reducer.markRead(selected);
    QCOMPARE(selectedState->unread, 0);
    QCOMPARE(selectedState->mentions, 0);
    QVERIFY(selectedState->unreadMark.has_value());

    // A later selection (a real switch away and back) clears the mark.
    const IrcConversationKey other =
        reducer.conversationKey(networkA, QStringLiteral("#other"));
    reducer.markSelected(other);
    reducer.markSelected(selected);
    QVERIFY(!selectedState->unreadMark.has_value());
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

void ReducerTest::mentionArrivalOnDirectMessage()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey dm =
        reducer.conversationKey(networkA, QStringLiteral("Alice"));
    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));

    reducer.apply(IrcMessageEvent{
        dm,
        QStringLiteral("Alice"),
        QStringLiteral("hello"),
        timestamp,
        QStringLiteral("Alice"),
    });
    const std::optional<IrcMentionArrival> unselected =
        reducer.takeMentionArrival();
    QVERIFY(unselected.has_value());
    QCOMPARE(unselected->author, QStringLiteral("Alice"));
    QCOMPARE(unselected->body, QStringLiteral("hello"));
    QCOMPARE(reducer.find(dm)->mentions, 0);
    QCOMPARE(reducer.find(dm)->unread, 1);

    reducer.markSelected(dm);
    reducer.apply(IrcMessageEvent{
        dm,
        QStringLiteral("Alice"),
        QStringLiteral("hello"),
        timestamp,
        QStringLiteral("Alice"),
    });
    QVERIFY(reducer.takeMentionArrival().has_value());
    QCOMPARE(reducer.find(dm)->mentions, 0);
    QCOMPARE(reducer.find(dm)->unread, 0);

    reducer.apply(IrcMessageEvent{
        room,
        QStringLiteral("Alice"),
        QStringLiteral("hello"),
        timestamp,
        QStringLiteral("#room"),
    });
    QVERIFY(!reducer.takeMentionArrival().has_value());

    reducer.clearSelection();
    reducer.apply(IrcMessageEvent{
        dm,
        QStringLiteral("Alice"),
        QStringLiteral("hey omairc"),
        timestamp,
        QStringLiteral("Alice"),
    });
    QVERIFY(reducer.takeMentionArrival().has_value());
    QCOMPARE(reducer.find(dm)->mentions, 1);

    reducer.apply(IrcActionEvent{
        dm,
        QStringLiteral("Alice"),
        QStringLiteral("waves"),
        timestamp,
        QStringLiteral("Alice"),
    });
    QVERIFY(reducer.takeMentionArrival().has_value());

    reducer.apply(IrcNoticeEvent{
        dm,
        QStringLiteral("Alice"),
        QStringLiteral("hello"),
        timestamp,
        QStringLiteral("Alice"),
    });
    QVERIFY(!reducer.takeMentionArrival().has_value());

    reducer.apply(IrcMessageEvent{
        dm,
        QStringLiteral("omairc"),
        QStringLiteral("hello"),
        timestamp,
        QStringLiteral("Alice"),
    });
    QVERIFY(!reducer.takeMentionArrival().has_value());
}

void ReducerTest::mentionArrivalCarriesNetworkTargetAndMsgid()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#omarchy"));
    reducer.apply(IrcMessageEvent{
        room,
        QStringLiteral("Alice"),
        QStringLiteral("omairc: ping"),
        timestamp,
        QStringLiteral("#omarchy"),
        IrcMsgId{QStringLiteral("mid-1")},
    });

    const std::optional<IrcMentionArrival> mention = reducer.takeMentionArrival();
    QVERIFY(mention.has_value());
    QCOMPARE(mention->author, QStringLiteral("Alice"));
    QCOMPARE(mention->body, QStringLiteral("omairc: ping"));
    QCOMPARE(mention->networkId, networkA);
    QCOMPARE(mention->target, QStringLiteral("#omarchy"));
    QCOMPARE(mention->msgid.value, QStringLiteral("mid-1"));

    const IrcConversationKey dm =
        reducer.conversationKey(networkA, QStringLiteral("Alice"));
    reducer.apply(IrcMessageEvent{
        dm,
        QStringLiteral("Alice"),
        QStringLiteral("hello"),
        timestamp,
        QStringLiteral("Alice"),
        IrcMsgId{QStringLiteral("dm-7")},
    });
    const std::optional<IrcMentionArrival> direct = reducer.takeMentionArrival();
    QVERIFY(direct.has_value());
    QCOMPARE(direct->author, QStringLiteral("Alice"));
    QCOMPARE(direct->body, QStringLiteral("hello"));
    QCOMPARE(direct->networkId, networkA);
    QCOMPARE(direct->target, QStringLiteral("Alice"));
    QCOMPARE(direct->msgid.value, QStringLiteral("dm-7"));
}

void ReducerTest::mutedChatDoesNotMention()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    reducer.setMuted(room, true);
    reducer.apply(IrcMessageEvent{
        room,
        QStringLiteral("Alice"),
        QStringLiteral("omairc: ping"),
        timestamp,
        QStringLiteral("#room"),
    });

    QVERIFY(!reducer.takeMentionArrival().has_value());
    const IrcConversationState *muted = reducer.find(room);
    QVERIFY(muted);
    QVERIFY(muted->muted);
    QCOMPARE(muted->mentions, 0);
    QCOMPARE(muted->unread, 1);
    QCOMPARE(muted->messages.back().body, QStringLiteral("omairc: ping"));

    reducer.markSelected(room);
    QVERIFY(reducer.find(room)->muted);
    QCOMPARE(reducer.find(room)->unread, 0);

    reducer.clearSelection();
    reducer.setMuted(room, false);
    QVERIFY(!reducer.find(room)->muted);
    reducer.apply(IrcMessageEvent{
        room,
        QStringLiteral("Alice"),
        QStringLiteral("omairc: back"),
        timestamp,
        QStringLiteral("#room"),
    });
    QVERIFY(reducer.takeMentionArrival().has_value());
    QCOMPARE(reducer.find(room)->mentions, 1);
}

void ReducerTest::highlightWordMentionsLikeNick()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA, QStringLiteral("fred"));
    reducer.setHighlightWords(networkA, QStringList{QStringLiteral("omairc")});
    const IrcConversationKey selected =
        reducer.conversationKey(networkA, QStringLiteral("#selected"));
    const IrcConversationKey background =
        reducer.conversationKey(networkA, QStringLiteral("#background"));
    reducer.markSelected(selected);

    reducer.apply(IrcMessageEvent{
        background,
        QStringLiteral("Alice"),
        QStringLiteral("please review omairc"),
        timestamp,
        QStringLiteral("#background"),
    });
    QCOMPARE(reducer.find(background)->unread, 1);
    QCOMPARE(reducer.find(background)->mentions, 1);
    const std::optional<IrcMentionArrival> hit = reducer.takeMentionArrival();
    QVERIFY(hit.has_value());
    QCOMPARE(hit->author, QStringLiteral("Alice"));
    QCOMPARE(hit->body, QStringLiteral("please review omairc"));

    reducer.apply(IrcMessageEvent{
        background,
        QStringLiteral("Alice"),
        QStringLiteral("please review omaircd"),
        timestamp,
        QStringLiteral("#background"),
    });
    QCOMPARE(reducer.find(background)->unread, 2);
    QCOMPARE(reducer.find(background)->mentions, 1);
    QVERIFY(!reducer.takeMentionArrival().has_value());

    reducer.setHighlightWords(networkA, QStringList{QStringLiteral("deploy")});
    reducer.apply(IrcMessageEvent{
        background,
        QStringLiteral("Alice"),
        QStringLiteral("please review deploy"),
        timestamp,
        QStringLiteral("#background"),
    });
    QCOMPARE(reducer.find(background)->mentions, 2);
    QVERIFY(reducer.takeMentionArrival().has_value());

    reducer.apply(IrcMessageEvent{
        background,
        QStringLiteral("Alice"),
        QStringLiteral("please review deployment"),
        timestamp,
        QStringLiteral("#background"),
    });
    QCOMPARE(reducer.find(background)->mentions, 2);
    QVERIFY(!reducer.takeMentionArrival().has_value());

    reducer.setHighlightWords(networkA, {});
    reducer.apply(IrcMessageEvent{
        background,
        QStringLiteral("Alice"),
        QStringLiteral("fred: still a nick"),
        timestamp,
        QStringLiteral("#background"),
    });
    QCOMPARE(reducer.find(background)->mentions, 3);
    QVERIFY(reducer.takeMentionArrival().has_value());
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
    QVERIFY(conversation->channel()->historyAnchor);

    welcome(reducer, networkA);
    QCOMPARE(conversation->peopleCount(), 0);
    QVERIFY(!conversation->channel()->joined);
    QVERIFY(!conversation->channel()->historyAnchor);

    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    reducer.apply(IrcHistoryEvent{
        room,
        QStringLiteral("#room"),
        {replayLine(QStringLiteral("alice"), QStringLiteral("stale"),
                    QStringLiteral("id-welcome"))},
    });
    QCOMPARE(conversation->messages.size(), std::size_t(2));
    QCOMPARE(conversation->messages[0].body, QStringLiteral("omairc joined"));
    QCOMPARE(conversation->messages[1].body, QStringLiteral("Alice joined"));
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

    reducer.apply(IrcMemberMetadataEvent{
        networkA, QStringLiteral("Alice"), QStringLiteral("status"),
        QStringLiteral("writing docs")});
    const std::optional<IrcMemberView> member =
        reducer.memberView(room, QStringLiteral("alice"));
    QCOMPARE(member->status, QStringLiteral("writing docs"));
    QCOMPARE(member->label, QStringLiteral("@Alice"));
    QCOMPARE(member->ranks, parsedName("@Alice").ranks);
    QVERIFY(!member->isAway());

    reducer.apply(IrcMemberMetadataEvent{
        networkA, QStringLiteral("Alice"), QStringLiteral("status"), QString()});
    QCOMPARE(reducer.memberView(room, QStringLiteral("alice"))->status, QString());
}

void ReducerTest::metadataKeysStoreIndependentlyOfAwayAndPrefix()
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
    reducer.apply(IrcAwayEvent{networkA, QStringLiteral("Alice"),
                               IrcAway{QStringLiteral("lunch")}});

    reducer.apply(IrcMemberMetadataEvent{
        networkA, QStringLiteral("Alice"), QStringLiteral("avatar"),
        QStringLiteral("https://example.com/a.png")});
    reducer.apply(IrcMemberMetadataEvent{
        networkA, QStringLiteral("Alice"), QStringLiteral("bot"),
        QStringLiteral("PacketBot")});
    reducer.apply(IrcMemberMetadataEvent{
        networkA, QStringLiteral("Alice"), QStringLiteral("display-name"),
        QStringLiteral("Anna Docs")});
    reducer.apply(IrcMemberMetadataEvent{
        networkA, QStringLiteral("Alice"), QStringLiteral("status"),
        QStringLiteral("writing docs")});

    const std::optional<IrcMemberView> member =
        reducer.memberView(room, QStringLiteral("alice"));
    QVERIFY(member->isAway());
    QCOMPARE(member->label, QStringLiteral("@Alice"));
    QCOMPARE(member->status, QStringLiteral("writing docs"));
    QCOMPARE(member->avatar, QStringLiteral("https://example.com/a.png"));
    QVERIFY(member->bot);
    QCOMPARE(member->displayName, QStringLiteral("Anna Docs"));

    const IrcNickPresence facts =
        reducer.nickPresence(networkA, QStringLiteral("ALICE"));
    QVERIFY(facts.away.has_value());
    QCOMPARE(facts.avatar(), QStringLiteral("https://example.com/a.png"));
    QVERIFY(facts.isBot());
    QCOMPARE(facts.metadata(QStringLiteral("display-name")),
             QStringLiteral("Anna Docs"));

    reducer.apply(IrcMemberMetadataEvent{
        networkA, QStringLiteral("Alice"), QStringLiteral("bot"), QString()});
    const std::optional<IrcMemberView> afterClear =
        reducer.memberView(room, QStringLiteral("alice"));
    QVERIFY(!afterClear->bot);
    QCOMPARE(afterClear->status, QStringLiteral("writing docs"));
    QCOMPARE(afterClear->avatar, QStringLiteral("https://example.com/a.png"));
    QCOMPARE(afterClear->displayName, QStringLiteral("Anna Docs"));
    QVERIFY(afterClear->isAway());
    QCOMPARE(afterClear->label, QStringLiteral("@Alice"));
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
    reducer.apply(IrcMemberMetadataEvent{
        networkA, QStringLiteral("Alice"), QStringLiteral("status"),
        QStringLiteral("writing docs")});

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

void ReducerTest::dropChannelErasesOnlyChannelRows()
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
    reducer.markSelected(channel);

    QCOMPARE(reducer.conversations().size(), std::size_t(2));
    QVERIFY(reducer.dropChannel(channel));
    QVERIFY(!reducer.find(channel));
    QVERIFY(reducer.find(lena));
    QVERIFY(!reducer.find(lena)->isChannel());
    QCOMPARE(reducer.conversations().size(), std::size_t(1));
    QVERIFY(!reducer.selected());
    QVERIFY(!reducer.dropChannel(channel));
    QVERIFY(!reducer.dropChannel(lena));
    QVERIFY(reducer.find(lena));
    QCOMPARE(reducer.conversations().size(), std::size_t(1));

    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("omairc")});
    const IrcConversationState *recreated = reducer.find(channel);
    QVERIFY(recreated);
    QVERIFY(recreated->isChannel());
    QVERIFY(recreated->channel()->joined);
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

void ReducerTest::selfAwayShowsOnOurOwnRowInEveryChannel()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey omarchy =
        reducer.conversationKey(networkA, QStringLiteral("#omarchy"));
    const IrcConversationKey desktop =
        reducer.conversationKey(networkA, QStringLiteral("#desktop"));
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#omarchy"), QStringLiteral("omairc")});
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#desktop"), QStringLiteral("omairc")});
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#omarchy"), QStringLiteral("Alice")});

    QVERIFY(!reducer.memberView(omarchy, QStringLiteral("omairc"))->isAway());
    QVERIFY(!reducer.memberView(desktop, QStringLiteral("omairc"))->isAway());

    reducer.apply(IrcSelfAwayEvent{networkA, true});
    QVERIFY(reducer.selfAway(networkA));
    QVERIFY(reducer.memberView(omarchy, QStringLiteral("omairc"))->isAway());
    QVERIFY(reducer.memberView(desktop, QStringLiteral("omairc"))->isAway());
    QVERIFY(!reducer.memberView(omarchy, QStringLiteral("alice"))->isAway());

    // The overlay is a read of the network-level fact, so losing member
    // presence facts must not clear our own away state.
    reducer.clearPresenceFacts(networkA, true, true);
    QVERIFY(reducer.memberView(omarchy, QStringLiteral("omairc"))->isAway());

    reducer.apply(IrcSelfAwayEvent{networkA, false});
    QVERIFY(!reducer.memberView(omarchy, QStringLiteral("omairc"))->isAway());
    QVERIFY(!reducer.memberView(desktop, QStringLiteral("omairc"))->isAway());
}

void ReducerTest::peerPresenceFollowsSharedChannelAndAwayFacts()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);

    // No shared channel proves the nick is online, so a direct message must not
    // paint it as available.
    QVERIFY(reducer.peerPresence(networkA, QStringLiteral("alice"))
            == IrcPeerPresence::Unknown);

    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#omarchy"), QStringLiteral("Alice")});
    QVERIFY(reducer.peerPresence(networkA, QStringLiteral("alice"))
            == IrcPeerPresence::Online);

    // The away flag is the same fact a channel member row reads.
    reducer.apply(IrcAwayEvent{networkA, QStringLiteral("Alice"),
                               IrcAway{QStringLiteral("lunch")}});
    QVERIFY(reducer.peerPresence(networkA, QStringLiteral("alice"))
            == IrcPeerPresence::Away);

    reducer.apply(IrcAwayEvent{networkA, QStringLiteral("Alice"), std::nullopt});
    QVERIFY(reducer.peerPresence(networkA, QStringLiteral("alice"))
            == IrcPeerPresence::Online);

    // Losing the last shared channel drops the presence with the facts, so the
    // direct message stops claiming online.
    reducer.apply(IrcQuitEvent{networkA, QStringLiteral("Alice"), QString()});
    QVERIFY(reducer.peerPresence(networkA, QStringLiteral("alice"))
            == IrcPeerPresence::Unknown);

    // Presence is per network.
    QVERIFY(reducer.peerPresence(networkB, QStringLiteral("alice"))
            == IrcPeerPresence::Unknown);
}

void ReducerTest::metadataNotifyRefreshesOnlyAffectedSurfaces()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const std::optional<IrcConversationKey> nothing;

    const auto metadataNotify = [&](const QString& nick, const QString& key) {
        return classifyViewNotify(
            IrcEvent{IrcMemberMetadataEvent{networkA, nick, key, QStringLiteral("x")}},
            reducer,
            nothing);
    };

    IrcViewNotify statusOnly =
        metadataNotify(QStringLiteral("Alice"), IrcMetadata::statusKey());
    QCOMPARE(statusOnly.members, IrcMemberSurface::Row);
    QVERIFY(!statusOnly.conversations);
    QVERIFY(!statusOnly.messages);

    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#omarchy"), QStringLiteral("Alice")});
    IrcViewNotify avatarShared =
        metadataNotify(QStringLiteral("Alice"), IrcMetadata::avatarKey());
    QVERIFY(avatarShared.messages);
    QVERIFY(!avatarShared.conversations);

    const IrcConversationKey alice =
        reducer.conversationKey(networkA, QStringLiteral("Alice"));
    reducer.apply(IrcMessageEvent{
        alice, QStringLiteral("Alice"), QStringLiteral("hi"), timestamp,
        QStringLiteral("Alice")});
    IrcViewNotify avatarDirect =
        metadataNotify(QStringLiteral("Alice"), IrcMetadata::avatarKey());
    QVERIFY(avatarDirect.messages);
    QVERIFY(avatarDirect.conversations);
}

void ReducerTest::falseyBotValuesAreNotBots()
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

    for (const QString& falsey :
         {QStringLiteral("0"), QStringLiteral("false"), QStringLiteral("no")}) {
        reducer.apply(IrcMemberMetadataEvent{
            networkA, QStringLiteral("Alice"), IrcMetadata::botKey(), falsey});
        const std::optional<IrcMemberView> member =
            reducer.memberView(room, QStringLiteral("alice"));
        QVERIFY(member.has_value());
        QVERIFY(!member->bot);
        QVERIFY(!reducer.nickPresence(networkA, QStringLiteral("Alice")).isBot());
        reducer.apply(IrcMemberMetadataEvent{
            networkA, QStringLiteral("Alice"), IrcMetadata::botKey(), QString()});
    }

    reducer.apply(IrcMemberMetadataEvent{
        networkA, QStringLiteral("Alice"), IrcMetadata::botKey(),
        QStringLiteral("PacketBot")});
    QVERIFY(reducer.memberView(room, QStringLiteral("alice"))->bot);
}

void ReducerTest::awayReloadsConversationsOnlyWhenItCanReachADirectRow()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const std::optional<IrcConversationKey> nothing;

    const auto awayNotify = [&](const QString& nick, std::optional<IrcAway> away) {
        return classifyViewNotify(
            IrcEvent{IrcAwayEvent{networkA, nick, away}}, reducer, nothing);
    };

    // No shared channel and no direct row: the presence role cannot change, so
    // the sidebar must not reload.
    IrcViewNotify orphan =
        awayNotify(QStringLiteral("Alice"), IrcAway{QStringLiteral("lunch")});
    QVERIFY(!orphan.conversations);
    QCOMPARE(orphan.members, IrcMemberSurface::Row);
    QCOMPARE(orphan.nick, QStringLiteral("alice"));

    // A shared channel lets peerPresence answer, so a direct row's presence can
    // change and the sidebar still reloads.
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#omarchy"), QStringLiteral("Alice")});
    IrcViewNotify shared =
        awayNotify(QStringLiteral("Alice"), IrcAway{QStringLiteral("lunch")});
    QVERIFY(shared.conversations);

    // A direct row with no shared channel also still reloads, keeping the
    // pre-existing behaviour for nicks that have a DM.
    reducer.apply(IrcQuitEvent{networkA, QStringLiteral("Alice"), QString()});
    const IrcConversationKey alice =
        reducer.conversationKey(networkA, QStringLiteral("Alice"));
    reducer.apply(IrcMessageEvent{
        alice, QStringLiteral("Alice"), QStringLiteral("hi"), timestamp,
        QStringLiteral("Alice")});
    QVERIFY(reducer.find(alice));
    IrcViewNotify direct =
        awayNotify(QStringLiteral("Alice"), IrcAway{QStringLiteral("lunch")});
    QVERIFY(direct.conversations);

    // Clearing away is the same decision path.
    reducer.apply(IrcAwayEvent{networkA, QStringLiteral("Alice"),
                               IrcAway{QStringLiteral("lunch")}});
    IrcViewNotify cleared = awayNotify(QStringLiteral("Alice"), std::nullopt);
    QVERIFY(cleared.conversations);
    QCOMPARE(cleared.members, IrcMemberSurface::Row);
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

void ReducerTest::historyMarksUnreadLikeLiveWhenUnselected()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#selected"), QStringLiteral("omairc")});
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#background"), QStringLiteral("omairc")});
    const IrcConversationKey selected =
        reducer.conversationKey(networkA, QStringLiteral("#selected"));
    const IrcConversationKey background =
        reducer.conversationKey(networkA, QStringLiteral("#background"));
    reducer.markSelected(selected);

    reducer.apply(IrcHistoryEvent{
        background,
        QStringLiteral("#background"),
        {replayLine(QStringLiteral("alice"), QStringLiteral("plain"),
                    QStringLiteral("id-plain")),
         replayLine(QStringLiteral("alice"), QStringLiteral("omairc: ping"),
                    QStringLiteral("id-mention")),
         replayLine(QStringLiteral("omairc"), QStringLiteral("my reply"),
                    QStringLiteral("id-self"))},
    });

    const IrcConversationState *backgroundState = reducer.find(background);
    QVERIFY(backgroundState);
    QCOMPARE(backgroundState->unread, 2);
    QCOMPARE(backgroundState->mentions, 1);
    QCOMPARE(backgroundState->messages[0].origin, IrcOrigin::Replay);
    QCOMPARE(backgroundState->messages[0].body, QStringLiteral("plain"));
    const std::optional<IrcMentionArrival> mention = reducer.takeMentionArrival();
    QVERIFY(mention.has_value());
    QCOMPARE(mention->author, QStringLiteral("alice"));
    QCOMPARE(mention->body, QStringLiteral("omairc: ping"));

    reducer.apply(IrcHistoryEvent{
        selected,
        QStringLiteral("#selected"),
        {replayLine(QStringLiteral("alice"), QStringLiteral("omairc: here"),
                    QStringLiteral("id-focused"))},
    });
    QCOMPARE(reducer.find(selected)->unread, 0);
    QCOMPARE(reducer.find(selected)->mentions, 0);
    QCOMPARE(reducer.find(selected)->messages[0].origin, IrcOrigin::Replay);

    reducer.markSelected(background);
    QCOMPARE(backgroundState->unread, 0);
    QCOMPARE(backgroundState->mentions, 0);
}

void ReducerTest::historyReplayPlantsUnreadMarkLikeLive()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#selected"), QStringLiteral("omairc")});
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#background"), QStringLiteral("omairc")});
    const IrcConversationKey selected =
        reducer.conversationKey(networkA, QStringLiteral("#selected"));
    const IrcConversationKey background =
        reducer.conversationKey(networkA, QStringLiteral("#background"));
    reducer.markSelected(selected);

    reducer.apply(IrcHistoryEvent{
        background,
        QStringLiteral("#background"),
        {replayLine(QStringLiteral("alice"), QStringLiteral("plain"),
                    QStringLiteral("id-plain")),
         replayLine(QStringLiteral("alice"), QStringLiteral("omairc: ping"),
                    QStringLiteral("id-mention")),
         replayLine(QStringLiteral("omairc"), QStringLiteral("my reply"),
                    QStringLiteral("id-self"))},
    });

    const IrcConversationState *backgroundState = reducer.find(background);
    QVERIFY(backgroundState);
    QCOMPARE(backgroundState->unread, 2);
    QCOMPARE(backgroundState->mentions, 1);
    QVERIFY(backgroundState->unreadMark.has_value());
    QCOMPARE(*backgroundState->unreadMark, backgroundState->messages[0].sequence);
    QCOMPARE(backgroundState->messages[0].body, QStringLiteral("plain"));
    QCOMPARE(backgroundState->messages[1].body, QStringLiteral("omairc: ping"));
    QCOMPARE(backgroundState->messages[2].body, QStringLiteral("my reply"));

    reducer.apply(IrcHistoryEvent{
        selected,
        QStringLiteral("#selected"),
        {replayLine(QStringLiteral("alice"), QStringLiteral("omairc: since-away"),
                    QStringLiteral("id-focused")),
         replayLine(QStringLiteral("bob"), QStringLiteral("later"),
                    QStringLiteral("id-later"))},
    });
    const IrcConversationState *selectedState = reducer.find(selected);
    QVERIFY(selectedState);
    QCOMPARE(selectedState->unread, 0);
    QCOMPARE(selectedState->mentions, 0);
    QVERIFY(selectedState->unreadMark.has_value());
    QCOMPARE(selectedState->messages[0].body, QStringLiteral("omairc: since-away"));
    QCOMPARE(selectedState->messages[1].body, QStringLiteral("later"));
    QCOMPARE(selectedState->messages[2].body, QStringLiteral("omairc joined"));
    QCOMPARE(*selectedState->unreadMark, selectedState->messages[0].sequence);
}

void ReducerTest::replayWhileUnfocusedDoesNotPlantUnreadMarkOnSelected()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#selected"), QStringLiteral("omairc")});
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#background"), QStringLiteral("omairc")});
    const IrcConversationKey selected =
        reducer.conversationKey(networkA, QStringLiteral("#selected"));
    const IrcConversationKey background =
        reducer.conversationKey(networkA, QStringLiteral("#background"));
    reducer.markSelected(selected);
    reducer.setWindowActive(false);

    reducer.apply(IrcHistoryEvent{
        selected,
        QStringLiteral("#selected"),
        {replayLine(QStringLiteral("alice"), QStringLiteral("omairc: backlog"),
                    QStringLiteral("id-replay")),
         replayLine(QStringLiteral("alice"), QStringLiteral("plain backlog"),
                    QStringLiteral("id-plain"))},
    });
    const IrcConversationState *selectedState = reducer.find(selected);
    QVERIFY(selectedState);
    QCOMPARE(selectedState->unread, 0);
    QCOMPARE(selectedState->mentions, 0);
    QVERIFY(!selectedState->unreadMark.has_value());
    QCOMPARE(selectedState->messages[0].origin, IrcOrigin::Replay);
    QCOMPARE(selectedState->messages[0].body, QStringLiteral("omairc: backlog"));
    QVERIFY(!reducer.takeInboxArrival().has_value());

    reducer.apply(IrcMessageEvent{
        selected,
        QStringLiteral("alice"),
        QStringLiteral("live-after-replay"),
        timestamp,
        QStringLiteral("#selected"),
    });
    QCOMPARE(selectedState->unread, 1);
    QVERIFY(selectedState->unreadMark.has_value());
    QCOMPARE(*selectedState->unreadMark, selectedState->messages.back().sequence);
    QCOMPARE(selectedState->messages.back().origin, IrcOrigin::Live);
    QCOMPARE(selectedState->messages.back().body, QStringLiteral("live-after-replay"));

    reducer.apply(IrcHistoryEvent{
        background,
        QStringLiteral("#background"),
        {replayLine(QStringLiteral("alice"), QStringLiteral("bg-backlog"),
                    QStringLiteral("id-bg"))},
    });
    const IrcConversationState *backgroundState = reducer.find(background);
    QVERIFY(backgroundState);
    QCOMPARE(backgroundState->unread, 1);
    QVERIFY(backgroundState->unreadMark.has_value());
    QCOMPARE(backgroundState->messages[0].origin, IrcOrigin::Replay);
}

void ReducerTest::mutedChatStillPlantsUnreadMark()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    reducer.setMuted(room, true);
    reducer.apply(IrcMessageEvent{
        room,
        QStringLiteral("Alice"),
        QStringLiteral("omairc: ping"),
        timestamp,
        QStringLiteral("#room"),
    });

    const IrcConversationState *muted = reducer.find(room);
    QVERIFY(muted);
    QVERIFY(muted->muted);
    QCOMPARE(muted->mentions, 0);
    QCOMPARE(muted->unread, 1);
    QVERIFY(muted->unreadMark.has_value());
    QCOMPARE(*muted->unreadMark, muted->messages.back().sequence);
}

void ReducerTest::nickMergeAdoptsOrKeepsUnreadMark()
{
    IrcEventReducer adopt;
    welcome(adopt, networkA);
    const IrcConversationKey alice =
        adopt.conversationKey(networkA, QStringLiteral("Alice"));
    const IrcConversationKey alicia =
        adopt.conversationKey(networkA, QStringLiteral("Alicia"));
    adopt.markSelected(alicia);
    adopt.apply(IrcMessageEvent{
        alicia, QStringLiteral("Alicia"), QStringLiteral("dest-only"), timestamp,
        QStringLiteral("Alicia")});
    QVERIFY(!adopt.find(alicia)->unreadMark.has_value());
    adopt.clearSelection();
    adopt.apply(IrcMessageEvent{
        alice, QStringLiteral("Alice"), QStringLiteral("moved-chat"), timestamp,
        QStringLiteral("Alice")});
    QVERIFY(adopt.find(alice)->unreadMark.has_value());
    adopt.apply(IrcNickEvent{
        networkA, QStringLiteral("Alice"), QStringLiteral("Alicia")});
    QVERIFY(adopt.find(alicia)->unreadMark.has_value());
    qint64 destOnlySequence = -1;
    qint64 movedChatSequence = -1;
    for (const IrcReducedMessage& message : adopt.find(alicia)->messages) {
        if (message.body == QStringLiteral("dest-only"))
            destOnlySequence = message.sequence;
        if (message.body == QStringLiteral("moved-chat"))
            movedChatSequence = message.sequence;
    }
    QVERIFY(destOnlySequence >= 0);
    QVERIFY(movedChatSequence >= 0);
    QCOMPARE(*adopt.find(alicia)->unreadMark, movedChatSequence);
    QVERIFY(*adopt.find(alicia)->unreadMark != destOnlySequence);

    IrcEventReducer keep;
    welcome(keep, networkA);
    const IrcConversationKey fromAlice =
        keep.conversationKey(networkA, QStringLiteral("Alice"));
    const IrcConversationKey fromAlicia =
        keep.conversationKey(networkA, QStringLiteral("Alicia"));
    keep.apply(IrcMessageEvent{
        fromAlice, QStringLiteral("Alice"), QStringLiteral("from alice"), timestamp,
        QStringLiteral("Alice")});
    keep.markSelected(fromAlicia);
    keep.apply(IrcMessageEvent{
        fromAlicia, QStringLiteral("Alicia"), QStringLiteral("dest-prefix"), timestamp,
        QStringLiteral("Alicia")});
    keep.clearSelection();
    keep.apply(IrcMessageEvent{
        fromAlicia, QStringLiteral("Alicia"), QStringLiteral("from alicia"), timestamp,
        QStringLiteral("Alicia")});
    QVERIFY(keep.find(fromAlice)->unreadMark.has_value());
    QVERIFY(keep.find(fromAlicia)->unreadMark.has_value());
    const qint64 destMark = *keep.find(fromAlicia)->unreadMark;
    QVERIFY(destMark != *keep.find(fromAlice)->unreadMark);
    keep.apply(IrcNickEvent{
        networkA, QStringLiteral("Alice"), QStringLiteral("Alicia")});
    QVERIFY(keep.find(fromAlicia)->unreadMark.has_value());
    QCOMPARE(*keep.find(fromAlicia)->unreadMark, destMark);
    qint64 destUnreadSequence = -1;
    qint64 movedUnreadSequence = -1;
    for (const IrcReducedMessage& message : keep.find(fromAlicia)->messages) {
        if (message.body == QStringLiteral("from alicia"))
            destUnreadSequence = message.sequence;
        if (message.body == QStringLiteral("from alice"))
            movedUnreadSequence = message.sequence;
    }
    QCOMPARE(*keep.find(fromAlicia)->unreadMark, destUnreadSequence);
    QVERIFY(destUnreadSequence >= 0);
    QVERIFY(destUnreadSequence != movedUnreadSequence);
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

void ReducerTest::replayDistinctMsgidsWithIdenticalContentRetained()
{
    const auto history = [&](const IrcConversationKey& room,
                             std::vector<IrcReplayLine> lines,
                             IrcHistoryKind kind = IrcHistoryKind::BouncerPlayback) {
        return IrcHistoryEvent{
            room,
            QStringLiteral("#omarchy"),
            std::move(lines),
            kind,
        };
    };

    {
        IrcEventReducer reducer;
        welcome(reducer, networkA);
        const IrcConversationKey room =
            reducer.conversationKey(networkA, QStringLiteral("#omarchy"));
        reducer.apply(IrcJoinEvent{
            networkA, QStringLiteral("#omarchy"), QStringLiteral("omairc")});
        reducer.apply(history(room, {
            replayLine(QStringLiteral("alice"), QStringLiteral("same"),
                       QStringLiteral("message-a")),
            replayLine(QStringLiteral("alice"), QStringLiteral("same"),
                       QStringLiteral("message-b")),
        }));
        const IrcConversationState *conversation = reducer.find(room);
        QVERIFY(conversation);
        QCOMPARE(conversation->messages.size(), std::size_t(3));
        QCOMPARE(conversation->messages[1].msgid.value, QStringLiteral("message-a"));
        QCOMPARE(conversation->messages[2].msgid.value, QStringLiteral("message-b"));
    }

    {
        IrcEventReducer reducer;
        welcome(reducer, networkA);
        const IrcConversationKey room =
            reducer.conversationKey(networkA, QStringLiteral("#omarchy"));
        reducer.apply(IrcJoinEvent{
            networkA, QStringLiteral("#omarchy"), QStringLiteral("omairc")});
        reducer.apply(IrcMessageEvent{
            room, QStringLiteral("alice"), QStringLiteral("same"), timestamp,
            QStringLiteral("#omarchy"), IrcMsgId{QStringLiteral("message-a")}});
        reducer.apply(history(room, {
            replayLine(QStringLiteral("alice"), QStringLiteral("same"),
                       QStringLiteral("message-b")),
        }));
        const IrcConversationState *conversation = reducer.find(room);
        QVERIFY(conversation);
        QCOMPARE(conversation->messages.size(), std::size_t(3));
        QCOMPARE(conversation->messages[1].msgid.value, QStringLiteral("message-a"));
        QCOMPARE(conversation->messages[2].msgid.value, QStringLiteral("message-b"));
    }

    {
        IrcEventReducer reducer;
        welcome(reducer, networkA);
        const IrcConversationKey room =
            reducer.conversationKey(networkA, QStringLiteral("#omarchy"));
        reducer.apply(IrcJoinEvent{
            networkA, QStringLiteral("#omarchy"), QStringLiteral("omairc")});
        reducer.apply(history(room, {
            replayLine(QStringLiteral("alice"), QStringLiteral("same"),
                       QStringLiteral("same-id")),
        }));
        reducer.apply(history(room, {
            replayLine(QStringLiteral("alice"), QStringLiteral("same"),
                       QStringLiteral("same-id")),
        }));
        const IrcConversationState *conversation = reducer.find(room);
        QVERIFY(conversation);
        QCOMPARE(conversation->messages.size(), std::size_t(2));
        QCOMPARE(conversation->messages[1].msgid.value, QStringLiteral("same-id"));
    }

    {
        IrcEventReducer reducer;
        welcome(reducer, networkA);
        const IrcConversationKey room =
            reducer.conversationKey(networkA, QStringLiteral("#omarchy"));
        reducer.apply(IrcJoinEvent{
            networkA, QStringLiteral("#omarchy"), QStringLiteral("omairc")});
        reducer.apply(IrcMessageEvent{
            room, QStringLiteral("alice"), QStringLiteral("same"), timestamp,
            QStringLiteral("#omarchy")});
        reducer.apply(history(room, {
            replayLine(QStringLiteral("alice"), QStringLiteral("same")),
        }));
        const IrcConversationState *conversation = reducer.find(room);
        QVERIFY(conversation);
        QCOMPARE(conversation->messages.size(), std::size_t(2));
        QCOMPARE(conversation->messages[1].body, QStringLiteral("same"));
        QCOMPARE(conversation->messages[1].origin, IrcOrigin::Live);
    }

    {
        IrcEventReducer reducer;
        welcome(reducer, networkA);
        const IrcConversationKey room =
            reducer.conversationKey(networkA, QStringLiteral("#omarchy"));
        reducer.apply(IrcJoinEvent{
            networkA, QStringLiteral("#omarchy"), QStringLiteral("omairc")});
        reducer.apply(history(room, {
            replayLine(QStringLiteral("alice"), QStringLiteral("same"),
                       QStringLiteral("history-a")),
            replayLine(QStringLiteral("alice"), QStringLiteral("same"),
                       QStringLiteral("history-b")),
        }, IrcHistoryKind::ChatHistory));
        const IrcConversationState *conversation = reducer.find(room);
        QVERIFY(conversation);
        QCOMPARE(conversation->messages.size(), std::size_t(3));
        QCOMPARE(conversation->messages[1].msgid.value, QStringLiteral("history-a"));
        QCOMPARE(conversation->messages[2].msgid.value, QStringLiteral("history-b"));
    }
}

void ReducerTest::nickMergeDropsDuplicateMsgids()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey alice =
        reducer.conversationKey(networkA, QStringLiteral("Alice"));
    const IrcConversationKey alicia =
        reducer.conversationKey(networkA, QStringLiteral("Alicia"));
    reducer.apply(IrcMessageEvent{
        alice, QStringLiteral("Alice"), QStringLiteral("shared"), timestamp,
        QStringLiteral("Alice"), IrcMsgId{QStringLiteral("same")}});
    reducer.apply(IrcMessageEvent{
        alicia, QStringLiteral("Alicia"), QStringLiteral("shared"), timestamp,
        QStringLiteral("Alicia"), IrcMsgId{QStringLiteral("same")}});
    reducer.apply(IrcMessageEvent{
        alicia, QStringLiteral("Alicia"), QStringLiteral("only here"), timestamp,
        QStringLiteral("Alicia")});

    reducer.apply(IrcNickEvent{
        networkA, QStringLiteral("Alice"), QStringLiteral("Alicia")});

    const IrcConversationState *merged = reducer.find(alicia);
    QVERIFY(merged);
    QStringList bodies;
    for (const IrcReducedMessage& message : merged->messages)
        bodies.append(message.body);
    QCOMPARE(bodies,
             QStringList({QStringLiteral("shared"), QStringLiteral("only here"),
                          QStringLiteral("Alice is now Alicia")}));
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

void ReducerTest::historyAfterCapDoesNotSplice()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#omarchy"));
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#omarchy"), QStringLiteral("omairc")});
    QVERIFY(reducer.find(room)->channel()->historyAnchor);

    for (int index = 0; index < 2000; ++index) {
        reducer.apply(IrcMessageEvent{
            room, QStringLiteral("alice"), QString::number(index), timestamp,
            QStringLiteral("#omarchy")});
    }

    const IrcConversationState *conversation = reducer.find(room);
    QVERIFY(conversation);
    QCOMPARE(conversation->messages.size(), std::size_t(2000));
    QVERIFY(!conversation->channel()->historyAnchor);
    QCOMPARE(conversation->messages.front().body, QStringLiteral("0"));
    QCOMPARE(conversation->messages.front().origin, IrcOrigin::Live);

    reducer.apply(IrcHistoryEvent{
        room,
        QStringLiteral("#omarchy"),
        {replayLine(QStringLiteral("alice"), QStringLiteral("stale"),
                    QStringLiteral("id-cap"))},
    });
    QCOMPARE(conversation->messages.size(), std::size_t(2000));
    QCOMPARE(conversation->messages.front().body, QStringLiteral("0"));
    QCOMPARE(conversation->messages.front().origin, IrcOrigin::Live);
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

void ReducerTest::queryReplayFromPeerOpensDirectMessage()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey lena =
        reducer.conversationKey(networkA, QStringLiteral("lena"));
    QVERIFY(!reducer.find(lena));

    reducer.apply(IrcHistoryEvent{
        lena,
        QStringLiteral("lena"),
        {replayLine(QStringLiteral("lena"), QStringLiteral("are you there"),
                    QStringLiteral("id-peer")),
         replayLine(QStringLiteral("omairc"), QStringLiteral("just got back"),
                    QStringLiteral("id-self"))},
    });

    const IrcConversationState *conversation = reducer.find(lena);
    QVERIFY(conversation);
    QVERIFY(!conversation->isChannel());
    QCOMPARE(conversation->target, QStringLiteral("lena"));
    QCOMPARE(conversation->messages.size(), std::size_t(2));
    QCOMPARE(conversation->messages[0].author, QStringLiteral("lena"));
    QCOMPARE(conversation->messages[0].body, QStringLiteral("are you there"));
    QCOMPARE(conversation->messages[0].origin, IrcOrigin::Replay);
    QCOMPARE(conversation->messages[1].author, QStringLiteral("omairc"));
    QCOMPARE(conversation->messages[1].body, QStringLiteral("just got back"));
    QCOMPARE(conversation->messages[1].origin, IrcOrigin::Replay);
    QCOMPARE(conversation->unread, 1);
    QCOMPARE(conversation->mentions, 0);
    QCOMPARE(conversation->spliceEpoch, 0);
    const std::optional<IrcMentionArrival> mention = reducer.takeMentionArrival();
    QVERIFY(mention.has_value());
    QCOMPARE(mention->author, QStringLiteral("lena"));
    QCOMPARE(mention->body, QStringLiteral("are you there"));
}

void ReducerTest::selfOnlyQueryReplayDoesNotOpenDirectMessageForMsg()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey lena =
        reducer.conversationKey(networkA, QStringLiteral("lena"));

    reducer.apply(IrcHistoryEvent{
        lena,
        QStringLiteral("lena"),
        {replayLine(QStringLiteral("omairc"), QStringLiteral("hi"),
                    QStringLiteral("id-msg"))},
    });

    QVERIFY(!reducer.find(lena));
    QCOMPARE(reducer.conversations().size(), std::size_t(0));
}

void ReducerTest::selfOnlyPlaybackThenPeerSplicesInOrder()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey lena =
        reducer.conversationKey(networkA, QStringLiteral("lena"));

    reducer.apply(IrcHistoryEvent{
        lena,
        QStringLiteral("lena"),
        {replayLine(QStringLiteral("omairc"), QStringLiteral("held"),
                    QStringLiteral("id-self"))},
        IrcHistoryKind::BouncerPlayback,
    });
    QVERIFY(!reducer.find(lena));

    reducer.apply(IrcHistoryEvent{
        lena,
        QStringLiteral("lena"),
        {},
        IrcHistoryKind::BouncerPlayback,
    });
    QVERIFY(!reducer.find(lena));

    reducer.apply(IrcHistoryEvent{
        lena,
        QStringLiteral("lena"),
        {replayLine(QStringLiteral("lena"), QStringLiteral("from peer"),
                    QStringLiteral("id-peer"))},
        IrcHistoryKind::BouncerPlayback,
    });

    const IrcConversationState *conversation = reducer.find(lena);
    QVERIFY(conversation);
    QCOMPARE(conversation->messages.size(), std::size_t(2));
    QCOMPARE(conversation->messages[0].author, QStringLiteral("omairc"));
    QCOMPARE(conversation->messages[0].body, QStringLiteral("held"));
    QCOMPARE(conversation->messages[1].author, QStringLiteral("lena"));
    QCOMPARE(conversation->messages[1].body, QStringLiteral("from peer"));
    QVERIFY(!reducer.releasePendingQueryPlayback(networkA));
    QCOMPARE(reducer.find(lena)->messages.size(), std::size_t(2));
}

void ReducerTest::playbackBatchKeptTracksSpliceAndDedup()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey held =
        reducer.conversationKey(networkA, QStringLiteral("#held"));
    reducer.apply(IrcHistoryEvent{
        held,
        QStringLiteral("#held"),
        {replayLine(QStringLiteral("lena"), QStringLiteral("before"),
                    QStringLiteral("id-before"))},
        IrcHistoryKind::BouncerPlayback,
    });
    QVERIFY(!reducer.playbackBatchKept(networkA, QStringLiteral("#held")));
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#held"), QStringLiteral("omairc")});
    QVERIFY(reducer.playbackBatchKept(networkA, QStringLiteral("#held")));

    const IrcConversationKey dup =
        reducer.conversationKey(networkA, QStringLiteral("#dup"));
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#dup"), QStringLiteral("omairc")});
    reducer.apply(IrcMessageEvent{
        dup, QStringLiteral("lena"), QStringLiteral("already"), timestamp,
        QStringLiteral("#dup")});
    QVERIFY(!reducer.playbackBatchKept(networkA, QStringLiteral("#dup")));
    reducer.apply(IrcHistoryEvent{
        dup,
        QStringLiteral("#dup"),
        {replayLine(QStringLiteral("Lena"), QStringLiteral("already"),
                    QStringLiteral("id-dup"))},
        IrcHistoryKind::BouncerPlayback,
    });
    QVERIFY(reducer.playbackBatchKept(networkA, QStringLiteral("#dup")));
    QCOMPARE(reducer.find(dup)->messages.size(), std::size_t(2));

    const IrcConversationKey empty =
        reducer.conversationKey(networkA, QStringLiteral("#empty"));
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#empty"), QStringLiteral("omairc")});
    reducer.apply(IrcHistoryEvent{
        empty,
        QStringLiteral("#empty"),
        {},
        IrcHistoryKind::BouncerPlayback,
    });
    QVERIFY(!reducer.playbackBatchKept(networkA, QStringLiteral("#empty")));

    welcome(reducer, networkA);
    QVERIFY(!reducer.playbackBatchKept(networkA, QStringLiteral("#held")));
    QVERIFY(!reducer.playbackBatchKept(networkA, QStringLiteral("#dup")));
}

void ReducerTest::cappedPlaybackSpliceNotesOnlySurvivingLines()
{
    const QDateTime trimmedAt = QDateTime::fromString(
        QStringLiteral("2024-03-09T16:00:00.620Z"), Qt::ISODateWithMs);
    const QDateTime keptAt = QDateTime::fromString(
        QStringLiteral("2024-03-09T16:00:01.500Z"), Qt::ISODateWithMs);
    const QDateTime dupAt = QDateTime::fromString(
        QStringLiteral("2024-03-09T16:00:04.000Z"), Qt::ISODateWithMs);
    QVERIFY(trimmedAt.isValid());
    QVERIFY(keptAt.isValid());
    QVERIFY(dupAt.isValid());

    const auto fillTo = [](IrcEventReducer& reducer,
                           const IrcConversationKey& room,
                           int count) {
        for (int index = 0; index < count; ++index) {
            reducer.apply(IrcMessageEvent{
                room, QStringLiteral("alice"), QString::number(index), timestamp,
                QStringLiteral("#omarchy")});
        }
    };
    const auto timedLine = [](const QString& body, const QDateTime& when) {
        IrcReplayLine line = replayLine(QStringLiteral("lena"), body);
        line.timestamp = when;
        line.serverTime = when;
        return line;
    };

    {
        IrcEventReducer reducer;
        welcome(reducer, networkA);
        const IrcConversationKey room =
            reducer.conversationKey(networkA, QStringLiteral("#omarchy"));
        reducer.apply(IrcJoinEvent{
            networkA, QStringLiteral("#omarchy"), QStringLiteral("omairc")});
        fillTo(reducer, room, IrcEventReducer::kMaxMessages - 1);
        const IrcConversationState *before = reducer.find(room);
        QVERIFY(before);
        QCOMPARE(before->messages.size(),
                 std::size_t(IrcEventReducer::kMaxMessages));
        QCOMPARE(before->messages.front().body, QStringLiteral("omairc joined"));
        QVERIFY(before->channel()->historyAnchor);

        reducer.apply(IrcHistoryEvent{
            room,
            QStringLiteral("#omarchy"),
            {timedLine(QStringLiteral("trimmed"), trimmedAt)},
            IrcHistoryKind::BouncerPlayback,
        });
        const IrcConversationState *conversation = reducer.find(room);
        QVERIFY(conversation);
        QCOMPARE(conversation->messages.size(),
                 std::size_t(IrcEventReducer::kMaxMessages));
        QCOMPARE(conversation->messages.front().body, QStringLiteral("omairc joined"));
        QCOMPARE(conversation->messages.at(1).body, QStringLiteral("0"));
        QVERIFY(reducer.takeKeptReplay().empty());
        QVERIFY(!reducer.playbackBatchKept(networkA, QStringLiteral("#omarchy")));

        IrcReplayLine dup = replayLine(QStringLiteral("alice"), QStringLiteral("0"));
        dup.timestamp = timestamp;
        dup.serverTime = dupAt;
        reducer.apply(IrcHistoryEvent{
            room,
            QStringLiteral("#omarchy"),
            {dup},
            IrcHistoryKind::BouncerPlayback,
        });
        QCOMPARE(reducer.find(room)->messages.size(),
                 std::size_t(IrcEventReducer::kMaxMessages));
        QCOMPARE(reducer.find(room)->messages.at(1).body, QStringLiteral("0"));
        QCOMPARE(reducer.find(room)->messages.at(1).origin, IrcOrigin::Live);
        QVERIFY(reducer.playbackBatchKept(networkA, QStringLiteral("#omarchy")));
        const std::vector<IrcKeptReplay> kept = reducer.takeKeptReplay();
        QCOMPARE(kept.size(), std::size_t(1));
        QCOMPARE(kept.front().target, QStringLiteral("#omarchy"));
        QCOMPARE(kept.front().serverTime.toMSecsSinceEpoch(),
                 dupAt.toMSecsSinceEpoch());
    }

    {
        IrcEventReducer reducer;
        welcome(reducer, networkA);
        const IrcConversationKey room =
            reducer.conversationKey(networkA, QStringLiteral("#omarchy"));
        reducer.apply(IrcJoinEvent{
            networkA, QStringLiteral("#omarchy"), QStringLiteral("omairc")});
        fillTo(reducer, room, IrcEventReducer::kMaxMessages - 2);
        QCOMPARE(reducer.find(room)->messages.size(),
                 std::size_t(IrcEventReducer::kMaxMessages - 1));

        reducer.apply(IrcHistoryEvent{
            room,
            QStringLiteral("#omarchy"),
            {timedLine(QStringLiteral("dropped"), trimmedAt),
             timedLine(QStringLiteral("kept"), keptAt)},
            IrcHistoryKind::BouncerPlayback,
        });
        const IrcConversationState *conversation = reducer.find(room);
        QVERIFY(conversation);
        QCOMPARE(conversation->messages.size(),
                 std::size_t(IrcEventReducer::kMaxMessages));
        QCOMPARE(conversation->messages.front().body, QStringLiteral("kept"));
        QCOMPARE(conversation->messages.at(1).body, QStringLiteral("omairc joined"));
        QVERIFY(reducer.playbackBatchKept(networkA, QStringLiteral("#omarchy")));
        const std::vector<IrcKeptReplay> kept = reducer.takeKeptReplay();
        QCOMPARE(kept.size(), std::size_t(1));
        QCOMPARE(kept.front().serverTime.toMSecsSinceEpoch(),
                 keptAt.toMSecsSinceEpoch());
    }
}

void ReducerTest::channelPlaybackPreviousNickStaysMutedBacklog()
{
    const QDateTime when = QDateTime::fromString(
        QStringLiteral("2024-03-09T16:00:00.620Z"), Qt::ISODateWithMs);
    QVERIFY(when.isValid());
    const auto line = [&](const QString& author, const QString& body) {
        IrcReplayLine replay = replayLine(author, body);
        replay.timestamp = when;
        replay.serverTime = when;
        return replay;
    };
    const auto playback = [&](IrcEventReducer& reducer,
                              const IrcConversationKey& room,
                              std::vector<IrcReplayLine> lines) {
        reducer.apply(IrcHistoryEvent{
            room,
            QStringLiteral("#omarchy"),
            std::move(lines),
            IrcHistoryKind::BouncerPlayback,
        });
    };

    {
        IrcEventReducer reducer;
        welcome(reducer, networkA, QStringLiteral("oldnick"));
        reducer.apply(IrcNickEvent{
            networkA, QStringLiteral("oldnick"), QStringLiteral("omairc")});
        reducer.apply(IrcJoinEvent{
            networkA, QStringLiteral("#omarchy"), QStringLiteral("omairc")});
        const IrcConversationKey room =
            reducer.conversationKey(networkA, QStringLiteral("#omarchy"));
        playback(reducer, room, {
            line(QStringLiteral("oldnick"), QStringLiteral("omairc: mine")),
            line(QStringLiteral("lena"), QStringLiteral("omairc: ping")),
        });
        const IrcConversationState *conversation = reducer.find(room);
        QVERIFY(conversation);
        QCOMPARE(conversation->messages.size(), std::size_t(3));
        QCOMPARE(conversation->messages[0].author, QStringLiteral("oldnick"));
        QCOMPARE(conversation->messages[1].author, QStringLiteral("lena"));
        QCOMPARE(conversation->unread, 1);
        QCOMPARE(conversation->mentions, 1);
        QVERIFY(conversation->unreadMark.has_value());
        QCOMPARE(*conversation->unreadMark, conversation->messages[1].sequence);
        const std::optional<IrcMentionArrival> mention = reducer.takeMentionArrival();
        QVERIFY(mention.has_value());
        QCOMPARE(mention->author, QStringLiteral("lena"));

        reducer.apply(IrcMessageEvent{
            room, QStringLiteral("oldnick"), QStringLiteral("omairc: live"),
            timestamp, QStringLiteral("#omarchy")});
        QCOMPARE(conversation->unread, 2);
        QCOMPARE(conversation->mentions, 2);
        const std::optional<IrcMentionArrival> live = reducer.takeMentionArrival();
        QVERIFY(live.has_value());
        QCOMPARE(live->author, QStringLiteral("oldnick"));
    }

    {
        IrcEventReducer reducer;
        welcome(reducer, networkA, QStringLiteral("oldnick"));
        reducer.apply(IrcNickEvent{
            networkA, QStringLiteral("oldnick"), QStringLiteral("omairc")});
        reducer.apply(IrcJoinEvent{
            networkA, QStringLiteral("#omarchy"), QStringLiteral("omairc")});
        const IrcConversationKey room =
            reducer.conversationKey(networkA, QStringLiteral("#omarchy"));
        reducer.markSelected(room);
        playback(reducer, room, {
            line(QStringLiteral("oldnick"), QStringLiteral("omairc: mine")),
            line(QStringLiteral("lena"), QStringLiteral("hello")),
        });
        const IrcConversationState *conversation = reducer.find(room);
        QVERIFY(conversation);
        QCOMPARE(conversation->unread, 0);
        QCOMPARE(conversation->mentions, 0);
        QVERIFY(conversation->unreadMark.has_value());
        QCOMPARE(*conversation->unreadMark, conversation->messages[1].sequence);
        QCOMPARE(conversation->messages[1].author, QStringLiteral("lena"));
        QVERIFY(!reducer.takeMentionArrival().has_value());
    }

    {
        IrcEventReducer reducer;
        welcome(reducer, networkA, QStringLiteral("oldnick"));
        welcome(reducer, networkA, QStringLiteral("omairc"));
        reducer.apply(IrcJoinEvent{
            networkA, QStringLiteral("#omarchy"), QStringLiteral("omairc")});
        const IrcConversationKey room =
            reducer.conversationKey(networkA, QStringLiteral("#omarchy"));
        playback(reducer, room, {
            line(QStringLiteral("oldnick"), QStringLiteral("backlog")),
            line(QStringLiteral("lena"), QStringLiteral("still here")),
        });
        const IrcConversationState *conversation = reducer.find(room);
        QVERIFY(conversation);
        QCOMPARE(conversation->unread, 1);
        QCOMPARE(conversation->mentions, 0);
        QVERIFY(conversation->unreadMark.has_value());
        QCOMPARE(*conversation->unreadMark, conversation->messages[1].sequence);
        QCOMPARE(conversation->messages[0].author, QStringLiteral("oldnick"));
        QCOMPARE(conversation->messages[1].author, QStringLiteral("lena"));
    }
}

void ReducerTest::channelPlaybackCasemappingRememberedSelfNick()
{
    const QDateTime when = QDateTime::fromString(
        QStringLiteral("2024-03-09T16:00:00.620Z"), Qt::ISODateWithMs);
    QVERIFY(when.isValid());
    const auto line = [&](const QString& author, const QString& body) {
        IrcReplayLine replay = replayLine(author, body);
        replay.timestamp = when;
        replay.serverTime = when;
        return replay;
    };
    const auto playback = [&](IrcEventReducer& reducer,
                              const IrcConversationKey& room,
                              std::vector<IrcReplayLine> lines) {
        reducer.apply(IrcHistoryEvent{
            room,
            QStringLiteral("#omarchy"),
            std::move(lines),
            IrcHistoryKind::BouncerPlayback,
        });
    };

    {
        IrcEventReducer reducer;
        welcome(reducer, networkA, QStringLiteral("nick["));
        IrcServerFeatures features;
        features.applyTokens({std::string("CASEMAPPING=ascii")});
        reducer.setServerFeatures(networkA, features);
        reducer.apply(IrcWelcomeEvent{networkA, QStringLiteral("omairc")});
        reducer.apply(IrcJoinEvent{
            networkA, QStringLiteral("#omarchy"), QStringLiteral("omairc")});
        const IrcConversationKey room =
            reducer.conversationKey(networkA, QStringLiteral("#omarchy"));
        playback(reducer, room, {
            line(QStringLiteral("nick["), QStringLiteral("omairc: mine")),
            line(QStringLiteral("nick{"), QStringLiteral("omairc: ping")),
        });
        const IrcConversationState *conversation = reducer.find(room);
        QVERIFY(conversation);
        QCOMPARE(conversation->unread, 1);
        QCOMPARE(conversation->mentions, 1);
        QVERIFY(conversation->unreadMark.has_value());
        QCOMPARE(conversation->messages[0].author, QStringLiteral("nick["));
        QCOMPARE(conversation->messages[1].author, QStringLiteral("nick{"));

        reducer.apply(IrcMessageEvent{
            room, QStringLiteral("nick["), QStringLiteral("omairc: live"),
            timestamp, QStringLiteral("#omarchy")});
        QCOMPARE(conversation->unread, 2);
        QCOMPARE(conversation->mentions, 2);
        const std::optional<IrcMentionArrival> live = reducer.takeMentionArrival();
        QVERIFY(live.has_value());
        QCOMPARE(live->author, QStringLiteral("nick["));
    }

    {
        IrcEventReducer reducer;
        welcome(reducer, networkA, QStringLiteral("nick^"));
        IrcServerFeatures features;
        features.applyTokens({std::string("CASEMAPPING=strict-rfc1459")});
        reducer.setServerFeatures(networkA, features);
        reducer.apply(IrcWelcomeEvent{networkA, QStringLiteral("omairc")});
        reducer.apply(IrcJoinEvent{
            networkA, QStringLiteral("#omarchy"), QStringLiteral("omairc")});
        const IrcConversationKey room =
            reducer.conversationKey(networkA, QStringLiteral("#omarchy"));
        playback(reducer, room, {
            line(QStringLiteral("nick~"), QStringLiteral("omairc: mine")),
            line(QStringLiteral("nick`"), QStringLiteral("omairc: ping")),
        });
        const IrcConversationState *conversation = reducer.find(room);
        QVERIFY(conversation);
        QCOMPARE(conversation->unread, 1);
        QCOMPARE(conversation->mentions, 1);
        QCOMPARE(conversation->messages[0].author, QStringLiteral("nick~"));
        QCOMPARE(conversation->messages[1].author, QStringLiteral("nick`"));
    }
}

void ReducerTest::queryReplayAppendsAtTailOfExistingDirectMessage()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey lena =
        reducer.conversationKey(networkA, QStringLiteral("lena"));
    reducer.apply(IrcMessageEvent{
        lena, QStringLiteral("lena"), QStringLiteral("live first"), timestamp,
        QStringLiteral("lena"), IrcMsgId{QStringLiteral("id-live")}});

    reducer.apply(IrcHistoryEvent{
        lena,
        QStringLiteral("lena"),
        {replayLine(QStringLiteral("lena"), QStringLiteral("backlog"),
                    QStringLiteral("id-backlog"))},
    });

    const IrcConversationState *conversation = reducer.find(lena);
    QVERIFY(conversation);
    QCOMPARE(conversation->messages.size(), std::size_t(2));
    QCOMPARE(conversation->messages[0].body, QStringLiteral("live first"));
    QCOMPARE(conversation->messages[0].origin, IrcOrigin::Live);
    QCOMPARE(conversation->messages[1].body, QStringLiteral("backlog"));
    QCOMPARE(conversation->messages[1].origin, IrcOrigin::Replay);
    QCOMPARE(conversation->spliceEpoch, 0);
}

void ReducerTest::channelReplayWithoutConversationCreatesNothing()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#omarchy"));

    reducer.apply(IrcHistoryEvent{
        room,
        QStringLiteral("#omarchy"),
        {replayLine(QStringLiteral("alice"), QStringLiteral("older"),
                    QStringLiteral("id-room"))},
    });

    QVERIFY(!reducer.find(room));
    QCOMPARE(reducer.conversations().size(), std::size_t(0));
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

void ReducerTest::bouncerQueryPlaybackKeepsPreviousNick()
{
    const auto at = [](const char *iso) {
        return QDateTime::fromString(QString::fromLatin1(iso), Qt::ISODateWithMs);
    };
    const QDateTime beforeAt = at("2024-03-09T16:00:00.100Z");
    const QDateTime mineAt = at("2024-03-09T16:00:00.620Z");
    const QDateTime afterAt = at("2024-03-09T16:00:01.500Z");
    const QDateTime elsewhereAt = at("2024-03-09T16:00:02.000Z");
    const QDateTime channelAt = at("2024-03-09T16:00:04.000Z");
    QVERIFY(beforeAt.isValid());
    QVERIFY(mineAt.isValid());
    QVERIFY(afterAt.isValid());
    QVERIFY(elsewhereAt.isValid());
    QVERIFY(channelAt.isValid());

    IrcHistoryBatch batch;
    batch.target = QStringLiteral("lena");
    batch.kind = IrcHistoryKind::BouncerPlayback;
    batch.lines.push_back(mustParse(
        "@time=2024-03-09T16:00:00.100Z :lena!u@h PRIVMSG oldnick :before"));
    batch.lines.push_back(mustParse(
        "@time=2024-03-09T16:00:00.620Z :oldnick!u@h PRIVMSG lena :mine"));
    batch.lines.push_back(mustParse(
        "@time=2024-03-09T16:00:01.500Z :lena!u@h PRIVMSG omairc :after"));
    batch.lines.push_back(mustParse(
        "@time=2024-03-09T16:00:02.000Z :stranger!u@h PRIVMSG oldnick :elsewhere"));
    batch.lines.push_back(mustParse(
        "@time=2024-03-09T16:00:04.000Z :lena!u@h PRIVMSG #other :channel"));

    const auto keptEvent = [&](IrcHistoryKind kind) {
        IrcHistoryBatch copy = batch;
        copy.kind = kind;
        return IrcEventTranslator::translateHistory(
            networkA, QStringLiteral("omairc"), IrcServerFeatures(), copy);
    };

    const auto playback = keptEvent(IrcHistoryKind::BouncerPlayback);
    QVERIFY(playback);
    QCOMPARE(playback->lines.size(), std::size_t(3));
    QCOMPARE(playback->lines[0].body, QStringLiteral("before"));
    QCOMPARE(playback->lines[0].author, QStringLiteral("lena"));
    QCOMPARE(playback->lines[1].body, QStringLiteral("mine"));
    QCOMPARE(playback->lines[1].author, QStringLiteral("oldnick"));
    QCOMPARE(playback->lines[2].body, QStringLiteral("after"));
    QCOMPARE(playback->lines[2].author, QStringLiteral("lena"));
    QCOMPARE(playback->lines[0].serverTime->toMSecsSinceEpoch(),
             beforeAt.toMSecsSinceEpoch());
    QCOMPARE(playback->lines[2].serverTime->toMSecsSinceEpoch(),
             afterAt.toMSecsSinceEpoch());

    IrcEventReducer reducer;
    welcome(reducer, networkA);
    reducer.apply(*playback);
    const IrcConversationState *lena =
        reducer.find(reducer.conversationKey(networkA, QStringLiteral("lena")));
    QVERIFY(lena);
    QCOMPARE(lena->messages.size(), std::size_t(3));
    QCOMPARE(lena->messages[0].origin, IrcOrigin::Replay);
    QCOMPARE(lena->messages[1].origin, IrcOrigin::Replay);
    QCOMPARE(lena->messages[2].origin, IrcOrigin::Replay);
    QVERIFY(!reducer.find(reducer.conversationKey(networkA, QStringLiteral("#other"))));
    const std::vector<IrcKeptReplay> kept = reducer.takeKeptReplay();
    QCOMPARE(kept.size(), std::size_t(3));
    QCOMPARE(kept.back().target, QStringLiteral("lena"));
    QCOMPARE(kept.back().serverTime.toMSecsSinceEpoch(), afterAt.toMSecsSinceEpoch());
    for (const IrcKeptReplay& line : kept) {
        QCOMPARE(line.target, QStringLiteral("lena"));
        QVERIFY(line.serverTime.toMSecsSinceEpoch() != channelAt.toMSecsSinceEpoch());
        QVERIFY(line.serverTime.toMSecsSinceEpoch() != elsewhereAt.toMSecsSinceEpoch());
    }

    IrcHistoryBatch withoutAfter = batch;
    withoutAfter.lines.erase(withoutAfter.lines.begin() + 2);
    const auto omitted = IrcEventTranslator::translateHistory(
        networkA, QStringLiteral("omairc"), IrcServerFeatures(), withoutAfter);
    QVERIFY(omitted);
    QCOMPARE(omitted->lines.size(), std::size_t(2));
    QCOMPARE(omitted->lines[0].body, QStringLiteral("before"));
    QCOMPARE(omitted->lines[1].body, QStringLiteral("mine"));
    IrcEventReducer omittedReducer;
    welcome(omittedReducer, networkA);
    omittedReducer.apply(*omitted);
    const std::vector<IrcKeptReplay> omittedKept = omittedReducer.takeKeptReplay();
    QCOMPARE(omittedKept.size(), std::size_t(2));
    QCOMPARE(omittedKept.back().serverTime.toMSecsSinceEpoch(),
             mineAt.toMSecsSinceEpoch());

    IrcEventReducer existing;
    welcome(existing, networkA);
    const IrcConversationKey existingLena =
        existing.conversationKey(networkA, QStringLiteral("lena"));
    QVERIFY(existing.ensureConversation(
        existingLena, QStringLiteral("lena"), IrcConversationCause::UserOpen));
    existing.apply(*playback);
    const IrcConversationState *alreadyOpen = existing.find(existingLena);
    QVERIFY(alreadyOpen);
    QCOMPARE(alreadyOpen->messages.size(), std::size_t(3));
    QCOMPARE(alreadyOpen->messages[0].body, QStringLiteral("before"));
    QCOMPARE(alreadyOpen->messages[0].author, QStringLiteral("lena"));
    QCOMPARE(alreadyOpen->messages[1].body, QStringLiteral("mine"));
    QCOMPARE(alreadyOpen->messages[1].author, QStringLiteral("oldnick"));
    QCOMPARE(alreadyOpen->messages[2].body, QStringLiteral("after"));
    QCOMPARE(alreadyOpen->messages[2].author, QStringLiteral("lena"));
    const std::vector<IrcKeptReplay> existingKept = existing.takeKeptReplay();
    QCOMPARE(existingKept.size(), std::size_t(3));
    QCOMPARE(existingKept.back().serverTime.toMSecsSinceEpoch(),
             afterAt.toMSecsSinceEpoch());

    const auto history = keptEvent(IrcHistoryKind::ChatHistory);
    QVERIFY(history);
    QCOMPARE(history->lines.size(), std::size_t(1));
    QCOMPARE(history->lines.front().body, QStringLiteral("after"));

    IrcHistoryBatch channel;
    channel.target = QStringLiteral("#omarchy");
    channel.kind = IrcHistoryKind::BouncerPlayback;
    channel.lines.push_back(mustParse(
        "@time=2024-03-09T16:00:00.100Z :lena!u@h PRIVMSG oldnick :before"));
    channel.lines.push_back(mustParse(
        "@time=2024-03-09T16:00:01.500Z :lena!u@h PRIVMSG #omarchy :room"));
    const auto channelEvent = IrcEventTranslator::translateHistory(
        networkA, QStringLiteral("omairc"), IrcServerFeatures(), channel);
    QVERIFY(channelEvent);
    QCOMPARE(channelEvent->lines.size(), std::size_t(1));
    QCOMPARE(channelEvent->lines.front().body, QStringLiteral("room"));
}

void ReducerTest::bouncerQueryPreviousNickEchoIsOwnLine()
{
    const auto at = [](const char *iso) {
        return QDateTime::fromString(QString::fromLatin1(iso), Qt::ISODateWithMs);
    };
    const QDateTime mineAt = at("2024-03-09T16:00:00.620Z");
    const QDateTime laterAt = at("2024-03-09T16:00:01.500Z");
    QVERIFY(mineAt.isValid());
    QVERIFY(laterAt.isValid());

    const auto playback = [](std::initializer_list<const char *> lines) {
        IrcHistoryBatch batch;
        batch.target = QStringLiteral("lena");
        batch.kind = IrcHistoryKind::BouncerPlayback;
        for (const char *line : lines)
            batch.lines.push_back(mustParse(line));
        return IrcEventTranslator::translateHistory(
            networkA, QStringLiteral("omairc"), IrcServerFeatures(), batch);
    };

    {
        const auto echo = playback({
            "@time=2024-03-09T16:00:00.620Z :oldnick!u@h PRIVMSG lena :mine",
        });
        QVERIFY(echo);
        QCOMPARE(echo->lines.size(), std::size_t(1));
        IrcEventReducer reducer;
        welcome(reducer, networkA);
        reducer.apply(*echo);
        QVERIFY(!reducer.find(reducer.conversationKey(networkA, QStringLiteral("lena"))));
        QVERIFY(reducer.takeKeptReplay().empty());
        QVERIFY(reducer.takeRememberedQueries().empty());
        QVERIFY(!reducer.releasePendingQueryPlayback(networkA));
        QVERIFY(!reducer.find(reducer.conversationKey(networkA, QStringLiteral("lena"))));
        QVERIFY(reducer.takeKeptReplay().empty());
        QVERIFY(reducer.takeRememberedQueries().empty());
    }

    {
        const auto held = playback({
            "@time=2024-03-09T16:00:00.620Z :omairc!u@h PRIVMSG lena :held",
        });
        const auto echo = playback({
            "@time=2024-03-09T16:00:01.500Z :oldnick!u@h PRIVMSG lena :mine",
        });
        QVERIFY(held);
        QVERIFY(echo);
        IrcEventReducer reducer;
        welcome(reducer, networkA);
        reducer.apply(*held);
        reducer.apply(*echo);
        QVERIFY(!reducer.find(reducer.conversationKey(networkA, QStringLiteral("lena"))));
        QVERIFY(reducer.takeKeptReplay().empty());
        QVERIFY(reducer.takeRememberedQueries().empty());
        QVERIFY(!reducer.releasePendingQueryPlayback(networkA));
        QVERIFY(!reducer.find(reducer.conversationKey(networkA, QStringLiteral("lena"))));
        QVERIFY(reducer.takeKeptReplay().empty());
    }

    {
        const auto echo = playback({
            "@time=2024-03-09T16:00:00.620Z :oldnick!u@h PRIVMSG lena :omairc: backlog",
        });
        QVERIFY(echo);
        IrcEventReducer reducer;
        welcome(reducer, networkA);
        const IrcConversationKey lena =
            reducer.conversationKey(networkA, QStringLiteral("lena"));
        QVERIFY(reducer.ensureConversation(
            lena, QStringLiteral("lena"), IrcConversationCause::UserOpen));
        reducer.apply(*echo);
        const IrcConversationState *conversation = reducer.find(lena);
        QVERIFY(conversation);
        QCOMPARE(conversation->messages.size(), std::size_t(1));
        QCOMPARE(conversation->messages.front().author, QStringLiteral("oldnick"));
        QCOMPARE(conversation->messages.front().body, QStringLiteral("omairc: backlog"));
        QCOMPARE(conversation->messages.front().origin, IrcOrigin::Replay);
        QCOMPARE(conversation->unread, 0);
        QCOMPARE(conversation->mentions, 0);
        QVERIFY(!reducer.takeMentionArrival().has_value());
        const std::vector<IrcKeptReplay> kept = reducer.takeKeptReplay();
        QCOMPARE(kept.size(), std::size_t(1));
        QCOMPARE(kept.front().target, QStringLiteral("lena"));
        QCOMPARE(kept.front().serverTime.toMSecsSinceEpoch(),
                 mineAt.toMSecsSinceEpoch());
        const std::vector<IrcRememberedQuery> remembered =
            reducer.takeRememberedQueries();
        QCOMPARE(remembered.size(), std::size_t(1));
        QCOMPARE(remembered.front().target, QStringLiteral("lena"));

        reducer.apply(IrcMessageEvent{
            lena, QStringLiteral("oldnick"), QStringLiteral("omairc: live"),
            timestamp, QStringLiteral("lena")});
        QCOMPARE(conversation->unread, 1);
        QCOMPARE(conversation->mentions, 1);
        const std::optional<IrcMentionArrival> live = reducer.takeMentionArrival();
        QVERIFY(live.has_value());
        QCOMPARE(live->author, QStringLiteral("oldnick"));
        QCOMPARE(live->body, QStringLiteral("omairc: live"));
    }

    {
        const auto both = playback({
            "@time=2024-03-09T16:00:00.620Z :oldnick!u@h PRIVMSG lena :mine",
            "@time=2024-03-09T16:00:01.500Z :lena!u@h PRIVMSG omairc :later",
        });
        QVERIFY(both);
        QCOMPARE(both->lines.size(), std::size_t(2));
        IrcEventReducer reducer;
        welcome(reducer, networkA);
        const IrcConversationKey lena =
            reducer.conversationKey(networkA, QStringLiteral("lena"));
        QVERIFY(reducer.ensureConversation(
            lena, QStringLiteral("lena"), IrcConversationCause::UserOpen));
        reducer.markSelected(lena);
        reducer.apply(*both);
        const IrcConversationState *conversation = reducer.find(lena);
        QVERIFY(conversation);
        QCOMPARE(conversation->messages.size(), std::size_t(2));
        QCOMPARE(conversation->messages[0].author, QStringLiteral("oldnick"));
        QCOMPARE(conversation->messages[0].body, QStringLiteral("mine"));
        QCOMPARE(conversation->messages[0].origin, IrcOrigin::Replay);
        QCOMPARE(conversation->messages[1].author, QStringLiteral("lena"));
        QCOMPARE(conversation->messages[1].body, QStringLiteral("later"));
        QCOMPARE(conversation->messages[1].origin, IrcOrigin::Replay);
        QCOMPARE(conversation->unread, 0);
        QCOMPARE(conversation->mentions, 0);
        const std::vector<IrcKeptReplay> kept = reducer.takeKeptReplay();
        QCOMPARE(kept.size(), std::size_t(2));
        QCOMPARE(kept.back().serverTime.toMSecsSinceEpoch(),
                 laterAt.toMSecsSinceEpoch());
        QCOMPARE(reducer.takeRememberedQueries().size(), std::size_t(1));
    }

    {
        const auto peer = playback({
            "@time=2024-03-09T16:00:00.100Z :Lena!u@h PRIVMSG oldnick :before",
        });
        QVERIFY(peer);
        IrcEventReducer reducer;
        welcome(reducer, networkA);
        reducer.apply(*peer);
        const IrcConversationState *conversation =
            reducer.find(reducer.conversationKey(networkA, QStringLiteral("lena")));
        QVERIFY(conversation);
        QCOMPARE(conversation->messages.size(), std::size_t(1));
        QCOMPARE(conversation->messages.front().author, QStringLiteral("Lena"));
        QCOMPARE(conversation->messages.front().body, QStringLiteral("before"));
        QCOMPARE(conversation->messages.front().origin, IrcOrigin::Replay);
        QVERIFY(reducer.takeRememberedQueries().empty());
        const std::vector<IrcKeptReplay> kept = reducer.takeKeptReplay();
        QCOMPARE(kept.size(), std::size_t(1));
        QCOMPARE(kept.front().target, QStringLiteral("lena"));
    }
}

void ReducerTest::transcriptHydrateIsReplayWithoutNotify()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    IrcConversationLog log(dir.path());
    const IrcConversationKey room =
        IrcEventReducer().conversationKey(networkA, QStringLiteral("#omarchy"));

    {
        IrcEventReducer reducer;
        reducer.setConversationLog(&log);
        welcome(reducer, networkA);
        reducer.apply(IrcJoinEvent{
            networkA, QStringLiteral("#omarchy"), QStringLiteral("omairc")});
        reducer.apply(IrcMessageEvent{
            room, QStringLiteral("alice"), QStringLiteral("omairc: ping"),
            timestamp, QStringLiteral("#omarchy"),
            IrcMsgId{QStringLiteral("id-mention")}});
    }

    IrcEventReducer again;
    again.setConversationLog(&log);
    welcome(again, networkA);
    again.apply(IrcJoinEvent{
        networkA, QStringLiteral("#omarchy"), QStringLiteral("omairc")});
    const IrcConversationState *conversation = again.find(room);
    QVERIFY(conversation);
    QCOMPARE(conversation->unread, 0);
    QCOMPARE(conversation->mentions, 0);
    QVERIFY(!again.takeMentionArrival().has_value());
    QCOMPARE(conversation->messages.front().body, QStringLiteral("omairc joined"));
    QCOMPARE(conversation->messages[1].body, QStringLiteral("omairc: ping"));
    QCOMPARE(conversation->messages[1].origin, IrcOrigin::Replay);
    QCOMPARE(conversation->messages.back().body, QStringLiteral("omairc joined"));
}

void ReducerTest::transcriptMsgidSkipsLaterLive()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    IrcConversationLog log(dir.path());
    const IrcConversationKey room =
        IrcEventReducer().conversationKey(networkA, QStringLiteral("#omarchy"));

    {
        IrcEventReducer reducer;
        reducer.setConversationLog(&log);
        welcome(reducer, networkA);
        reducer.apply(IrcJoinEvent{
            networkA, QStringLiteral("#omarchy"), QStringLiteral("omairc")});
        reducer.apply(IrcMessageEvent{
            room, QStringLiteral("alice"), QStringLiteral("first"), timestamp,
            QStringLiteral("#omarchy"), IrcMsgId{QStringLiteral("same")}});
    }

    IrcEventReducer again;
    again.setConversationLog(&log);
    welcome(again, networkA);
    again.apply(IrcJoinEvent{
        networkA, QStringLiteral("#omarchy"), QStringLiteral("omairc")});
    again.apply(IrcMessageEvent{
        room, QStringLiteral("alice"), QStringLiteral("second"), timestamp,
        QStringLiteral("#omarchy"), IrcMsgId{QStringLiteral("same")}});
    const IrcConversationState *conversation = again.find(room);
    QVERIFY(conversation);
    QCOMPARE(conversation->messages[1].body, QStringLiteral("first"));
    QCOMPARE(conversation->messages[1].origin, IrcOrigin::Replay);
    QVERIFY(std::none_of(conversation->messages.begin(), conversation->messages.end(),
                         [](const IrcReducedMessage& message) {
                             return message.body == QStringLiteral("second");
                         }));
}

void ReducerTest::nickCollisionMergesMessageIds()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey alice =
        reducer.conversationKey(networkA, QStringLiteral("Alice"));
    const IrcConversationKey alicia =
        reducer.conversationKey(networkA, QStringLiteral("Alicia"));
    reducer.apply(IrcMessageEvent{
        alice, QStringLiteral("Alice"), QStringLiteral("from alice"), timestamp,
        QStringLiteral("Alice"), IrcMsgId{QStringLiteral("id-a")}});
    reducer.apply(IrcMessageEvent{
        alicia, QStringLiteral("Alicia"), QStringLiteral("from alicia"), timestamp,
        QStringLiteral("Alicia"), IrcMsgId{QStringLiteral("id-b")}});

    reducer.apply(IrcNickEvent{
        networkA, QStringLiteral("Alice"), QStringLiteral("Alicia")});

    const IrcConversationState *merged = reducer.find(alicia);
    QVERIFY(merged);
    const std::size_t afterMerge = merged->messages.size();
    QVERIFY(afterMerge >= std::size_t(2));

    reducer.apply(IrcMessageEvent{
        alicia, QStringLiteral("Alicia"), QStringLiteral("repeat a"), timestamp,
        QStringLiteral("Alicia"), IrcMsgId{QStringLiteral("id-a")}});
    reducer.apply(IrcMessageEvent{
        alicia, QStringLiteral("Alicia"), QStringLiteral("repeat b"), timestamp,
        QStringLiteral("Alicia"), IrcMsgId{QStringLiteral("id-b")}});
    QCOMPARE(reducer.find(alicia)->messages.size(), afterMerge);

    reducer.apply(IrcMessageEvent{
        alicia, QStringLiteral("Alicia"), QStringLiteral("fresh"), timestamp,
        QStringLiteral("Alicia"), IrcMsgId{QStringLiteral("id-c")}});
    QCOMPARE(reducer.find(alicia)->messages.size(), afterMerge + 1);
    QCOMPARE(reducer.find(alicia)->messages.back().body, QStringLiteral("fresh"));
}

void ReducerTest::conversationCauseInsertTable()
{
    QVERIFY(ircConversationCauseInserts(
        IrcConversationCause::UserOpen, false, false));
    QVERIFY(ircConversationCauseInserts(
        IrcConversationCause::UserOpen, false, true));
    QVERIFY(!ircConversationCauseInserts(
        IrcConversationCause::UserOpen, true, false));
    QVERIFY(ircConversationCauseInserts(
        IrcConversationCause::ChannelState, true, false));
    QVERIFY(!ircConversationCauseInserts(
        IrcConversationCause::ChannelState, false, false));
    QVERIFY(ircConversationCauseInserts(
        IrcConversationCause::InboundOther, true, false));
    QVERIFY(ircConversationCauseInserts(
        IrcConversationCause::InboundOther, false, false));
    QVERIFY(!ircConversationCauseInserts(
        IrcConversationCause::InboundOther, false, true));
    QVERIFY(!ircConversationCauseInserts(
        IrcConversationCause::InboundSelf, false, false));
    QVERIFY(!ircConversationCauseInserts(
        IrcConversationCause::InboundSelf, true, false));
    QVERIFY(!ircConversationCauseInserts(
        IrcConversationCause::QuietSend, false, false));
    QVERIFY(!ircConversationCauseInserts(
        IrcConversationCause::QuietSend, true, false));
    QVERIFY(ircConversationCauseInserts(
        IrcConversationCause::Restore, false, false));
    QVERIFY(!ircConversationCauseInserts(
        IrcConversationCause::Restore, false, true));
    QVERIFY(!ircConversationCauseInserts(
        IrcConversationCause::Restore, true, false));
}

void ReducerTest::ensureConversationHonorsCause()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    const IrcConversationKey lena =
        reducer.conversationKey(networkA, QStringLiteral("lena"));
    const IrcConversationKey nickserv =
        reducer.conversationKey(networkA, QStringLiteral("NickServ"));
    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    const IrcConversationKey ghost =
        reducer.conversationKey(networkA, QStringLiteral("ghost"));

    QVERIFY(!reducer.ensureConversation(
        lena, QStringLiteral("lena"), IrcConversationCause::QuietSend));
    QVERIFY(!reducer.find(lena));
    QVERIFY(!reducer.ensureConversation(
        lena, QStringLiteral("lena"), IrcConversationCause::InboundSelf));
    QVERIFY(!reducer.find(lena));

    reducer.markSelected(ghost);
    QVERIFY(!reducer.ensureConversation(
        ghost, QStringLiteral("ghost"), IrcConversationCause::InboundSelf));
    QVERIFY(!reducer.find(ghost));

    IrcConversationState *human = reducer.ensureConversation(
        lena, QStringLiteral("lena"), IrcConversationCause::InboundOther);
    QVERIFY(human);
    QVERIFY(!human->isChannel());

    QVERIFY(!reducer.ensureConversation(
        nickserv, QStringLiteral("NickServ"), IrcConversationCause::InboundOther));
    QVERIFY(!reducer.find(nickserv));
    IrcConversationState *queried = reducer.ensureConversation(
        nickserv, QStringLiteral("NickServ"), IrcConversationCause::UserOpen);
    QVERIFY(queried);
    QVERIFY(!queried->isChannel());

    QVERIFY(!reducer.ensureConversation(
        room, QStringLiteral("#room"), IrcConversationCause::UserOpen));
    QVERIFY(!reducer.find(room));
    IrcConversationState *channel = reducer.ensureConversation(
        room, QStringLiteral("#room"), IrcConversationCause::ChannelState);
    QVERIFY(channel);
    QVERIFY(channel->isChannel());

    QVERIFY(reducer.ensureConversation(
        lena, QStringLiteral("lena"), IrcConversationCause::QuietSend));
    QVERIFY(reducer.ensureConversation(
        lena, QStringLiteral("lena"), IrcConversationCause::InboundSelf));

    QVERIFY(!reducer.find(ghost));
    IrcConversationState *restored = reducer.ensureConversation(
        ghost, QStringLiteral("ghost"), IrcConversationCause::Restore);
    QVERIFY(restored);
    QVERIFY(!restored->isChannel());

    IrcEventReducer restoreOnly;
    welcome(restoreOnly, networkA);
    QVERIFY(!restoreOnly.ensureConversation(
        nickserv, QStringLiteral("NickServ"), IrcConversationCause::Restore));
    QVERIFY(!restoreOnly.find(nickserv));
    QVERIFY(!restoreOnly.ensureConversation(
        room, QStringLiteral("#room"), IrcConversationCause::Restore));

    reducer.apply(IrcMessageEvent{
        lena, QStringLiteral("omairc"), QStringLiteral("hello"), timestamp,
        QStringLiteral("lena")});
    QCOMPARE(human->messages.size(), std::size_t(1));
    QCOMPARE(human->messages.back().body, QStringLiteral("hello"));

    IrcEventReducer inbound;
    welcome(inbound, networkA);
    inbound.apply(IrcMessageEvent{
        nickserv, QStringLiteral("NickServ"), QStringLiteral("identify"), timestamp,
        QStringLiteral("NickServ")});
    QVERIFY(!inbound.find(nickserv));
    inbound.apply(IrcMessageEvent{
        lena, QStringLiteral("lena"), QStringLiteral("hi"), timestamp,
        QStringLiteral("lena")});
    const IrcConversationState *opened = inbound.find(lena);
    QVERIFY(opened);
    QVERIFY(!opened->isChannel());
    QCOMPARE(opened->messages.size(), std::size_t(1));
}

void ReducerTest::nickShapedJoinDoesNotInventDirect()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("lena"), QStringLiteral("omairc")});
    QVERIFY(!reducer.find(reducer.conversationKey(networkA, QStringLiteral("lena"))));
}

void ReducerTest::extendedJoinRecordsAccountOnOneLine()
{
    // The wire form is enough. These caps do not have to be enabled.
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    applyWire(reducer, ":Alice!a@h JOIN #room services :Alice Example");

    QCOMPARE(reducer.nickPresence(networkA, QStringLiteral("Alice")).account,
             QStringLiteral("services"));
    QCOMPARE(reducer.displayAccount(networkA, QStringLiteral("Alice")),
             QStringLiteral("services"));
    const IrcConversationState *room = roomOf(reducer);
    QVERIFY(room);
    QCOMPARE(room->messages.size(), std::size_t(1));
    QCOMPARE(room->messages.front().body,
             QStringLiteral("Alice (services) joined"));
    QCOMPARE(reducer.memberView(room->key, QStringLiteral("alice"))->account,
             QStringLiteral("services"));

    applyWire(reducer, ":Alice!a@h PRIVMSG #room :hi");
    applyWire(reducer, ":Alice!a@h JOIN :#room");
    QCOMPARE(reducer.nickPresence(networkA, QStringLiteral("Alice")).account,
             QStringLiteral("services"));
    QCOMPARE(lastBody(reducer), QStringLiteral("Alice joined"));

    applyWire(reducer, ":Alice!a@h PRIVMSG #room :again");
    applyWire(reducer, ":Alice!a@h JOIN #room * :Alice Example");
    QCOMPARE(reducer.nickPresence(networkA, QStringLiteral("Alice")).account,
             QString());
    QCOMPARE(reducer.displayAccount(networkA, QStringLiteral("Alice")), QString());
    QCOMPARE(lastBody(reducer), QStringLiteral("Alice joined"));
}

void ReducerTest::accountCommandAndTagShareOneField()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    applyWire(reducer, ":Alice!a@h ACCOUNT services");
    QCOMPARE(reducer.nickPresence(networkA, QStringLiteral("Alice")).account,
             QStringLiteral("services"));
    QVERIFY(!reducer.find(reducer.conversationKey(networkA, QStringLiteral("#room"))));

    applyWire(reducer, ":Alice!a@h ACCOUNT *");
    QCOMPARE(reducer.nickPresence(networkA, QStringLiteral("Alice")).account,
             QString());

    applyWire(reducer, "@account=services :Alice!a@h PRIVMSG #room :one");
    QCOMPARE(reducer.nickPresence(networkA, QStringLiteral("Alice")).account,
             QStringLiteral("services"));
    applyWire(reducer, ":Alice!a@h PRIVMSG #room :two");
    QCOMPARE(reducer.nickPresence(networkA, QStringLiteral("Alice")).account,
             QStringLiteral("services"));
    applyWire(reducer, "@account=* :Alice!a@h PRIVMSG #room :three");
    QCOMPARE(reducer.nickPresence(networkA, QStringLiteral("Alice")).account,
             QString());

    applyWire(reducer, "@account=services :Alice!a@h NOTICE #room :psst");
    IrcEventReducer viaCommand;
    welcome(viaCommand, networkA);
    applyWire(viaCommand, ":Alice!a@h ACCOUNT services");
    QCOMPARE(reducer.nickPresence(networkA, QStringLiteral("Alice")).account,
             viaCommand.nickPresence(networkA, QStringLiteral("Alice")).account);

    applyWire(reducer, ":server 330 omairc Alice whoisacct :is logged in as");
    QCOMPARE(reducer.nickPresence(networkA, QStringLiteral("Alice")).account,
             QStringLiteral("whoisacct"));
    applyWire(reducer, ":server 311 omairc Alice user host * :Alice Example");
    QCOMPARE(reducer.nickPresence(networkA, QStringLiteral("Alice")).account,
             QStringLiteral("whoisacct"));
}

void ReducerTest::nickChangeKeepsServicesAccount()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    applyWire(reducer, ":Alice!a@h JOIN #room services :Alice");
    applyWire(reducer, ":Alice!a@h NICK Alicia");
    QCOMPARE(reducer.nickPresence(networkA, QStringLiteral("Alicia")).account,
             QStringLiteral("services"));
    QCOMPARE(reducer.nickPresence(networkA, QStringLiteral("Alice")).account,
             QString());
    QCOMPARE(reducer.displayAccount(networkA, QStringLiteral("Alicia")),
             QStringLiteral("services"));

    IrcEventReducer same;
    welcome(same, networkA);
    applyWire(same, ":Bob!b@h JOIN #room bob :Bob");
    QCOMPARE(lastBody(same), QStringLiteral("Bob joined"));
    QCOMPARE(same.nickPresence(networkA, QStringLiteral("Bob")).account,
             QStringLiteral("bob"));
    QCOMPARE(same.displayAccount(networkA, QStringLiteral("Bob")), QString());
    const IrcConversationState *room = roomOf(same);
    QVERIFY(room);
    QCOMPARE(same.memberView(room->key, QStringLiteral("bob"))->account, QString());
    applyWire(same, ":Bob!b@h NICK Robert");
    QCOMPARE(same.nickPresence(networkA, QStringLiteral("Robert")).account,
             QStringLiteral("bob"));
    QCOMPARE(same.displayAccount(networkA, QStringLiteral("Robert")),
             QStringLiteral("bob"));

    IrcEventReducer folded;
    welcome(folded, networkA);
    applyWire(folded, ":a[b!u@h JOIN #room a{b :name");
    QCOMPARE(lastBody(folded), QStringLiteral("a[b joined"));
    QCOMPARE(folded.displayAccount(networkA, QStringLiteral("a[b")), QString());

    IrcEventReducer ascii;
    welcome(ascii, networkA);
    IrcServerFeatures features;
    features.applyTokens({std::string("CASEMAPPING=ascii")});
    ascii.setServerFeatures(networkA, features);
    applyWire(ascii, ":a[b!u@h JOIN #room a{b :name");
    QCOMPARE(lastBody(ascii), QStringLiteral("a[b (a{b) joined"));
    QCOMPARE(ascii.displayAccount(networkA, QStringLiteral("a[b")),
             QStringLiteral("a{b"));
}

void ReducerTest::accountChangeRefreshesMemberRowAndTranscript()
{
    IrcEventReducer reducer;
    welcome(reducer, networkA);
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("Alice")});
    const std::optional<IrcConversationKey> nothing;
    const IrcViewNotify notify = classifyViewNotify(
        IrcEvent{IrcAccountEvent{
            networkA, QStringLiteral("Alice"), QStringLiteral("*")}},
        reducer,
        nothing);
    QCOMPARE(notify.members, IrcMemberSurface::Row);
    QCOMPARE(notify.nick, QStringLiteral("alice"));
    QVERIFY(notify.messages);
    QVERIFY(!notify.conversations);
}

int runReducerTests(int argc, char **argv)
{
    ReducerTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_reducer.moc"
