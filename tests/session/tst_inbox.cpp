#include <QAbstractItemModel>
#include <QCoreApplication>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QTest>

#include "conversationlistmodel.h"
#include "fakeirctransport.h"
#include "irccasemapping.h"
#include "irccontroller.h"
#include "ircinboxmodel.h"
#include "irceventreducer.h"
#include "ircignore.h"
#include "ircnetworkprofile.h"
#include "ircprofilestore.h"
#include "networklogmodel.h"
#include "testsettings.h"

#include <memory>

namespace
{
const QString networkA = QStringLiteral("libera");
const QDateTime timestamp =
    QDateTime::fromString(QStringLiteral("2026-09-04T00:00:00Z"), Qt::ISODate);

IrcSessionConfig sessionConfig(const QString& networkId = networkA,
                               const QString& nick = QStringLiteral("fred"))
{
    IrcSessionConfig value;
    value.networkId = networkId;
    value.host = QStringLiteral("irc.example");
    value.tlsEnabled = true;
    value.nick = nick;
    value.username = nick;
    value.realname = QStringLiteral("Omairc User");
    value.reconnectEnabled = false;
    return value;
}

void welcome(FakeIrcTransport *transport, const QByteArray& extra = {})
{
    transport->completeConnect();
    QByteArray bytes =
        QByteArrayLiteral(":server CAP fred LS :multi-prefix\r\n"
                          ":server 001 fred :Welcome\r\n");
    bytes += extra;
    transport->injectBytes(bytes);
}

void welcomeMonitor(FakeIrcTransport *transport,
                    const QByteArray& token = QByteArrayLiteral("MONITOR=100"))
{
    welcome(transport,
            QByteArrayLiteral(":server 005 fred ") + token
                + QByteArrayLiteral(" :are supported by this server\r\n"));
}

void welcomeReducer(IrcEventReducer& reducer,
                    const QString& network = networkA,
                    const QString& nick = QStringLiteral("fred"))
{
    reducer.apply(IrcWelcomeEvent{network, nick});
}

IrcReplayLine replayLine(const QString& author,
                         const QString& body,
                         const QString& msgid = {})
{
    return {author, body, timestamp, IrcMessageKindTag::Chat, IrcMsgId{msgid}, {}};
}

bool logContains(QAbstractItemModel *lines, const QString& needle)
{
    for (int row = 0; row < lines->rowCount(); ++row) {
        const QString text =
            lines->data(lines->index(row, 0), NetworkLogModel::TextRole).toString();
        if (text.contains(needle))
            return true;
    }
    return false;
}

bool framesContain(const QByteArrayList& frames, const QByteArray& needle)
{
    for (const QByteArray& frame : frames) {
        if (frame.contains(needle))
            return true;
    }
    return false;
}

IrcInboxItem sampleItem(IrcInboxKind kind,
                        const QString& networkId = networkA,
                        const QString& actor = QStringLiteral("alice"),
                        const QString& target = QStringLiteral("#room"),
                        const QString& preview = QStringLiteral("ping"),
                        const IrcMsgId& msgid = {})
{
    return {
        kind,
        timestamp,
        networkId,
        actor,
        target,
        preview,
        msgid,
    };
}
}

class InboxTest : public QObject
{
    Q_OBJECT

private slots:
    void init();
    void appendConsumeAndCap();
    void inviteAndMonitorReplaceSameTarget();
    void consumeConversationInviteAndMonitor();
    void modelSyncTracksStore();
    void reducerClassifiesMentionHighlightAndDirect();
    void reducerNickMentionPrefersHighlightWord();
    void reducerMutedSelfSelectedAndReplaySkipInbox();
    void reducerKickOfSelfAppendsInbox();
    void controllerInviteIgnoredReplaceAndJoinConsumes();
    void controllerKickSelectingConsumes();
    void controllerMonitorEdgeAppendsHydrationSkips();
    void controllerDismissInboxItem();
    void controllerInboxCountTracksModel();
    void inviteCoalescesWithCaseMapping();
    void monitorCoalescesWithCaseMapping();
    void selectConversationConsumesMonitorOnline();
    void selfJoinConsumesInviteWithoutPendingInvite();
    void forgetNetworkStatePurgesInbox();

private:
    std::unique_ptr<QTemporaryDir> m_dir;
};

void InboxTest::init()
{
    m_dir = std::make_unique<QTemporaryDir>();
    QVERIFY(m_dir->isValid());
    TestSettings::isolate(m_dir->path());
    QCoreApplication::setOrganizationName(QStringLiteral("omairc"));
    QCoreApplication::setApplicationName(QStringLiteral("omairc"));
}

void InboxTest::appendConsumeAndCap()
{
    const IrcCaseMapping mapping;
    IrcInbox inbox;
    QCOMPARE(inbox.count(), 0);

    for (int index = 0; index < IrcInbox::kMaxItems + 5; ++index) {
        inbox.append(sampleItem(IrcInboxKind::Mention,
                                networkA,
                                QStringLiteral("alice"),
                                QStringLiteral("#room"),
                                QStringLiteral("line %1").arg(index)),
                     mapping);
    }
    QCOMPARE(inbox.count(), IrcInbox::kMaxItems);
    QCOMPARE(inbox.at(0).preview, QStringLiteral("line 54"));
    QCOMPARE(inbox.at(IrcInbox::kMaxItems - 1).preview, QStringLiteral("line 5"));

    inbox.consumeAt(2);
    QCOMPARE(inbox.count(), IrcInbox::kMaxItems - 1);
    QCOMPARE(inbox.at(2).preview, QStringLiteral("line 51"));
}

void InboxTest::inviteAndMonitorReplaceSameTarget()
{
    const IrcCaseMapping mapping;
    IrcInbox inbox;
    inbox.append(sampleItem(IrcInboxKind::Invite,
                            networkA,
                            QStringLiteral("alice"),
                            QStringLiteral("#lab"),
                            QStringLiteral("first")),
                mapping);
    inbox.append(sampleItem(IrcInboxKind::Invite,
                            networkA,
                            QStringLiteral("bob"),
                            QStringLiteral("#desk"),
                            QStringLiteral("other")),
                mapping);
    inbox.append(sampleItem(IrcInboxKind::Invite,
                            networkA,
                            QStringLiteral("carol"),
                            QStringLiteral("#lab"),
                            QStringLiteral("replacement")),
                mapping);
    QCOMPARE(inbox.count(), 2);
    QCOMPARE(inbox.at(0).actor, QStringLiteral("carol"));
    QCOMPARE(inbox.at(0).target, QStringLiteral("#lab"));
    QCOMPARE(inbox.at(0).preview, QStringLiteral("replacement"));
    QCOMPARE(inbox.at(1).actor, QStringLiteral("bob"));

    inbox.append(sampleItem(IrcInboxKind::MonitorOnline,
                            networkA,
                            QStringLiteral("alice"),
                            QStringLiteral("alice"),
                            QStringLiteral("is online")),
                mapping);
    inbox.append(sampleItem(IrcInboxKind::MonitorOnline,
                            networkA,
                            QStringLiteral("bob"),
                            QStringLiteral("bob"),
                            QStringLiteral("is online")),
                mapping);
    inbox.append(sampleItem(IrcInboxKind::MonitorOnline,
                            networkA,
                            QStringLiteral("alice"),
                            QStringLiteral("alice"),
                            QStringLiteral("again")),
                mapping);
    QCOMPARE(inbox.count(), 4);
    QCOMPARE(inbox.at(0).actor, QStringLiteral("alice"));
    QCOMPARE(inbox.at(0).preview, QStringLiteral("again"));
    QCOMPARE(inbox.at(1).actor, QStringLiteral("bob"));
    QCOMPARE(inbox.at(1).preview, QStringLiteral("is online"));
}

void InboxTest::consumeConversationInviteAndMonitor()
{
    const IrcCaseMapping mapping;
    IrcInbox inbox;
    inbox.append(sampleItem(IrcInboxKind::Mention,
                            networkA,
                            QStringLiteral("alice"),
                            QStringLiteral("#room")),
                 mapping);
    inbox.append(sampleItem(IrcInboxKind::Highlight,
                            networkA,
                            QStringLiteral("bob"),
                            QStringLiteral("#room")),
                 mapping);
    inbox.append(sampleItem(IrcInboxKind::Direct,
                            networkA,
                            QStringLiteral("carol"),
                            QStringLiteral("carol")),
                 mapping);
    inbox.append(sampleItem(IrcInboxKind::Kick,
                            networkA,
                            QStringLiteral("op"),
                            QStringLiteral("#room")),
                 mapping);
    inbox.append(sampleItem(IrcInboxKind::Invite,
                            networkA,
                            QStringLiteral("dax"),
                            QStringLiteral("#lab")),
                 mapping);
    inbox.append(sampleItem(IrcInboxKind::MonitorOnline,
                            networkA,
                            QStringLiteral("mira"),
                            QStringLiteral("mira")),
                 mapping);
    QCOMPARE(inbox.count(), 6);

    inbox.consumeConversation(networkA, QStringLiteral("#room"), mapping);
    QCOMPARE(inbox.count(), 3);
    QCOMPARE(inbox.at(0).kind, IrcInboxKind::MonitorOnline);
    QCOMPARE(inbox.at(1).kind, IrcInboxKind::Invite);
    QCOMPARE(inbox.at(2).kind, IrcInboxKind::Direct);

    inbox.consumeInvite(networkA, QStringLiteral("#lab"), mapping);
    QCOMPARE(inbox.count(), 2);
    inbox.consumeMonitor(networkA, QStringLiteral("MIRA"), mapping);
    QCOMPARE(inbox.count(), 1);
    inbox.consumeConversation(networkA, QStringLiteral("carol"), mapping);
    QCOMPARE(inbox.count(), 0);
}

void InboxTest::modelSyncTracksStore()
{
    IrcInbox inbox;
    IrcInboxModel model;
    const IrcCaseMapping mapping;
    inbox.append(sampleItem(IrcInboxKind::Direct,
                            networkA,
                            QStringLiteral("anna"),
                            QStringLiteral("anna"),
                            QStringLiteral("secret"),
                            IrcMsgId{QStringLiteral("dm-1")}),
                mapping);
    model.sync(inbox);

    QCOMPARE(model.rowCount(), 1);
    const QVariantMap row = model.get(0);
    QCOMPARE(row.value(QStringLiteral("kind")).toString(), QStringLiteral("direct"));
    QCOMPARE(row.value(QStringLiteral("actor")).toString(), QStringLiteral("anna"));
    QCOMPARE(row.value(QStringLiteral("target")).toString(), QStringLiteral("anna"));
    QCOMPARE(row.value(QStringLiteral("preview")).toString(), QStringLiteral("secret"));
    QCOMPARE(row.value(QStringLiteral("msgid")).toString(), QStringLiteral("dm-1"));
    QCOMPARE(model.field(0, QStringLiteral("label")),
             QStringLiteral("Message from anna"));

    inbox.append(sampleItem(IrcInboxKind::Mention), mapping);
    model.sync(inbox);
    QCOMPARE(model.rowCount(), 2);
    QCOMPARE(model.field(0, QStringLiteral("kind")), QStringLiteral("mention"));
    QCOMPARE(model.field(1, QStringLiteral("kind")), QStringLiteral("direct"));
}

void InboxTest::reducerClassifiesMentionHighlightAndDirect()
{
    IrcEventReducer reducer;
    welcomeReducer(reducer);
    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));
    const IrcConversationKey dm =
        reducer.conversationKey(networkA, QStringLiteral("Alice"));
    reducer.setHighlightWords(networkA, QStringList{QStringLiteral("deploy")});

    reducer.apply(IrcMessageEvent{
        room,
        QStringLiteral("Bob"),
        QStringLiteral("fred: ping"),
        timestamp,
        QStringLiteral("#room"),
        IrcMsgId{QStringLiteral("mention-1")},
    });
    const std::optional<IrcInboxArrival> mention = reducer.takeInboxArrival();
    QVERIFY(mention.has_value());
    QCOMPARE(mention->kind, IrcInboxKind::Mention);
    QCOMPARE(mention->msgid.value, QStringLiteral("mention-1"));

    reducer.apply(IrcMessageEvent{
        room,
        QStringLiteral("Bob"),
        QStringLiteral("please deploy"),
        timestamp,
        QStringLiteral("#room"),
        IrcMsgId{QStringLiteral("highlight-1")},
    });
    const std::optional<IrcInboxArrival> highlight = reducer.takeInboxArrival();
    QVERIFY(highlight.has_value());
    QCOMPARE(highlight->kind, IrcInboxKind::Highlight);
    QCOMPARE(highlight->msgid.value, QStringLiteral("highlight-1"));

    reducer.apply(IrcMessageEvent{
        dm,
        QStringLiteral("Alice"),
        QStringLiteral("hello"),
        timestamp,
        QStringLiteral("Alice"),
        IrcMsgId{QStringLiteral("dm-1")},
    });
    QVERIFY(!reducer.takeInboxArrival().has_value());

    reducer.apply(IrcMessageEvent{
        dm,
        QStringLiteral("Alice"),
        QStringLiteral("fred: ping"),
        timestamp,
        QStringLiteral("Alice"),
        IrcMsgId{QStringLiteral("dm-mention-1")},
    });
    const std::optional<IrcInboxArrival> dmMention = reducer.takeInboxArrival();
    QVERIFY(dmMention.has_value());
    QCOMPARE(dmMention->kind, IrcInboxKind::Mention);
    QCOMPARE(dmMention->target, QStringLiteral("Alice"));
}

void InboxTest::reducerNickMentionPrefersHighlightWord()
{
    IrcEventReducer reducer;
    welcomeReducer(reducer);
    reducer.setHighlightWords(networkA, QStringList{QStringLiteral("fred")});
    const IrcConversationKey room =
        reducer.conversationKey(networkA, QStringLiteral("#room"));

    reducer.apply(IrcMessageEvent{
        room,
        QStringLiteral("Bob"),
        QStringLiteral("fred: deploy"),
        timestamp,
        QStringLiteral("#room"),
    });
    const std::optional<IrcInboxArrival> arrival = reducer.takeInboxArrival();
    QVERIFY(arrival.has_value());
    QCOMPARE(arrival->kind, IrcInboxKind::Mention);
    QCOMPARE(reducer.find(room)->mentions, 1);
}

void InboxTest::reducerMutedSelfSelectedAndReplaySkipInbox()
{
    IrcEventReducer reducer;
    welcomeReducer(reducer);
    const IrcConversationKey selected =
        reducer.conversationKey(networkA, QStringLiteral("#selected"));
    const IrcConversationKey background =
        reducer.conversationKey(networkA, QStringLiteral("#background"));
    reducer.markSelected(selected);
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#background"), QStringLiteral("fred")});

    reducer.apply(IrcMessageEvent{
        selected,
        QStringLiteral("Alice"),
        QStringLiteral("fred: ping"),
        timestamp,
        QStringLiteral("#selected"),
    });
    QVERIFY(!reducer.takeInboxArrival().has_value());
    QVERIFY(reducer.takeMentionArrival().has_value());

    reducer.apply(IrcMessageEvent{
        selected,
        QStringLiteral("fred"),
        QStringLiteral("fred: self"),
        timestamp,
        QStringLiteral("#selected"),
    });
    QVERIFY(!reducer.takeInboxArrival().has_value());

    reducer.setMuted(background, true);
    reducer.apply(IrcMessageEvent{
        background,
        QStringLiteral("Alice"),
        QStringLiteral("fred: muted"),
        timestamp,
        QStringLiteral("#background"),
    });
    QVERIFY(!reducer.takeInboxArrival().has_value());

    reducer.setMuted(background, false);
    reducer.apply(IrcHistoryEvent{
        background,
        QStringLiteral("#background"),
        {replayLine(QStringLiteral("Alice"),
                    QStringLiteral("fred: replay"),
                    QStringLiteral("replay-1"))},
    });
    QVERIFY(!reducer.takeInboxArrival().has_value());
    QCOMPARE(reducer.find(background)->mentions, 1);
}

void InboxTest::reducerKickOfSelfAppendsInbox()
{
    IrcEventReducer reducer;
    welcomeReducer(reducer);
    reducer.apply(IrcJoinEvent{
        networkA, QStringLiteral("#room"), QStringLiteral("fred")});

    reducer.apply(IrcKickEvent{
        networkA,
        QStringLiteral("#room"),
        QStringLiteral("fred"),
        QStringLiteral("op"),
        QStringLiteral("out"),
    });
    const std::optional<IrcInboxArrival> arrival = reducer.takeInboxArrival();
    QVERIFY(arrival.has_value());
    QCOMPARE(arrival->kind, IrcInboxKind::Kick);
    QCOMPARE(arrival->target, QStringLiteral("#room"));
    QCOMPARE(arrival->body, QStringLiteral("out"));
}

void InboxTest::controllerInviteIgnoredReplaceAndJoinConsumes()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(networkA));
    welcome(transport);
    transport->injectBytes(QByteArrayLiteral(":fred!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(networkA, QStringLiteral("#omarchy"));

    transport->injectBytes(QByteArrayLiteral(":alice!u@h INVITE fred :#lab\r\n"));
    QCOMPARE(controller.inboxCount(), 1);
    auto *model = qobject_cast<IrcInboxModel *>(controller.inbox());
    QVERIFY(model);
    QCOMPARE(model->field(0, QStringLiteral("kind")), QStringLiteral("invite"));
    QCOMPARE(model->field(0, QStringLiteral("target")), QStringLiteral("#lab"));

    transport->injectBytes(QByteArrayLiteral(":bob!u@h INVITE fred :#lab\r\n"));
    QCOMPARE(controller.inboxCount(), 1);
    QCOMPARE(model->field(0, QStringLiteral("actor")), QStringLiteral("bob"));

    QVERIFY(controller.sendMessage(QStringLiteral("/ignore lena")));
    const int beforeIgnoredInvite = controller.inboxCount();
    transport->injectBytes(QByteArrayLiteral(":lena!u@h INVITE fred :#spam\r\n"));
    QCOMPARE(controller.inboxCount(), beforeIgnoredInvite);

    controller.activateInboxItem(0);
    QVERIFY(framesContain(transport->writtenFrames(), QByteArrayLiteral("JOIN #lab\r\n")));
    transport->injectBytes(QByteArrayLiteral(":fred!u@h JOIN :#lab\r\n"));
    QCOMPARE(controller.inboxCount(), 0);
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#lab"));
}

void InboxTest::controllerKickSelectingConsumes()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(networkA));
    welcome(transport);
    transport->injectBytes(QByteArrayLiteral(":fred!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(networkA, QStringLiteral("#ricing"));

    transport->injectBytes(
        QByteArrayLiteral(":op!u@h KICK #omarchy fred :removed\r\n"));
    QCOMPARE(controller.inboxCount(), 1);
    auto *model = qobject_cast<IrcInboxModel *>(controller.inbox());
    QVERIFY(model);
    QCOMPARE(model->field(0, QStringLiteral("kind")), QStringLiteral("kick"));
    QCOMPARE(model->field(0, QStringLiteral("target")), QStringLiteral("#omarchy"));

    controller.selectConversation(networkA, QStringLiteral("#omarchy"));
    QCOMPARE(controller.inboxCount(), 0);
}

void InboxTest::controllerMonitorEdgeAppendsHydrationSkips()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(networkA));
    welcomeMonitor(transport);
    controller.openStatus(networkA);

    QVERIFY(controller.console()->submit(QStringLiteral("/monitor alice")));
    transport->injectBytes(
        QByteArrayLiteral(":server 730 omairc :alice!user@host\r\n"));
    QCOMPARE(controller.inboxCount(), 0);

    transport->injectBytes(QByteArrayLiteral(":server 731 * :alice\r\n"));
    QCOMPARE(controller.inboxCount(), 0);

    transport->injectBytes(
        QByteArrayLiteral(":server 730 * :alice!u@h,bob!u@h\r\n"));
    QCOMPARE(controller.inboxCount(), 1);
    auto *model = qobject_cast<IrcInboxModel *>(controller.inbox());
    QVERIFY(model);
    QCOMPARE(model->field(0, QStringLiteral("kind")), QStringLiteral("monitorOnline"));
    QCOMPARE(model->field(0, QStringLiteral("actor")), QStringLiteral("alice"));

    controller.activateInboxItem(0);
    QCOMPARE(controller.inboxCount(), 0);

    QVERIFY(controller.discardSession(networkA));
    auto *again = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), again));
    QVERIFY(controller.start(networkA));
    welcomeMonitor(again);
    again->injectBytes(QByteArrayLiteral(":server 730 * :alice!u@h\r\n"));
    QCOMPARE(controller.inboxCount(), 0);
}

void InboxTest::inviteCoalescesWithCaseMapping()
{
    const IrcCaseMapping mapping(IrcCaseMapping::Kind::Ascii);
    IrcInbox inbox;
    inbox.append(sampleItem(IrcInboxKind::Invite,
                            networkA,
                            QStringLiteral("alice"),
                            QStringLiteral("#Lab"),
                            QStringLiteral("first")),
                mapping);
    inbox.append(sampleItem(IrcInboxKind::Invite,
                            networkA,
                            QStringLiteral("bob"),
                            QStringLiteral("#lab"),
                            QStringLiteral("replacement")),
                mapping);
    QCOMPARE(inbox.count(), 1);
    QCOMPARE(inbox.at(0).actor, QStringLiteral("bob"));
    QCOMPARE(inbox.at(0).target, QStringLiteral("#lab"));
    QCOMPARE(inbox.at(0).preview, QStringLiteral("replacement"));
}

void InboxTest::monitorCoalescesWithCaseMapping()
{
    const IrcCaseMapping mapping(IrcCaseMapping::Kind::Ascii);
    IrcInbox inbox;
    inbox.append(sampleItem(IrcInboxKind::MonitorOnline,
                            networkA,
                            QStringLiteral("Alice"),
                            QStringLiteral("Alice"),
                            QStringLiteral("first")),
                mapping);
    inbox.append(sampleItem(IrcInboxKind::MonitorOnline,
                            networkA,
                            QStringLiteral("bob"),
                            QStringLiteral("bob"),
                            QStringLiteral("other")),
                mapping);
    inbox.append(sampleItem(IrcInboxKind::MonitorOnline,
                            networkA,
                            QStringLiteral("alice"),
                            QStringLiteral("alice"),
                            QStringLiteral("replacement")),
                mapping);
    QCOMPARE(inbox.count(), 2);
    QCOMPARE(inbox.at(0).actor, QStringLiteral("alice"));
    QCOMPARE(inbox.at(0).preview, QStringLiteral("replacement"));
    QCOMPARE(inbox.at(1).actor, QStringLiteral("bob"));
}

void InboxTest::selectConversationConsumesMonitorOnline()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(networkA));
    welcomeMonitor(transport);
    controller.openStatus(networkA);

    QVERIFY(controller.console()->submit(QStringLiteral("/monitor alice")));
    transport->injectBytes(
        QByteArrayLiteral(":server 730 omairc :alice!user@host\r\n"));
    QCOMPARE(controller.inboxCount(), 0);

    transport->injectBytes(QByteArrayLiteral(":server 731 * :alice\r\n"));
    transport->injectBytes(
        QByteArrayLiteral(":server 730 * :alice!u@h\r\n"));
    QCOMPARE(controller.inboxCount(), 1);

    transport->injectBytes(QByteArrayLiteral(":fred!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(networkA, QStringLiteral("#omarchy"));
    QCOMPARE(controller.inboxCount(), 1);

    controller.openDirectMessage(QStringLiteral("alice"));
    QCOMPARE(controller.inboxCount(), 0);
    QCOMPARE(controller.selectedTarget(), QStringLiteral("alice"));

    QVERIFY(controller.console()->submit(QStringLiteral("/monitor bob")));
    transport->injectBytes(
        QByteArrayLiteral(":server 730 omairc :bob!user@host\r\n"));
    QCOMPARE(controller.inboxCount(), 0);
    transport->injectBytes(QByteArrayLiteral(":server 731 * :bob\r\n"));
    transport->injectBytes(
        QByteArrayLiteral(":server 730 * :bob!u@h\r\n"));
    QCOMPARE(controller.inboxCount(), 1);
    controller.selectConversation(networkA, QStringLiteral("bob"));
    QCOMPARE(controller.inboxCount(), 0);
}

void InboxTest::selfJoinConsumesInviteWithoutPendingInvite()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(networkA));
    welcome(transport);
    transport->injectBytes(QByteArrayLiteral(":fred!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(networkA, QStringLiteral("#omarchy"));

    transport->injectBytes(QByteArrayLiteral(":alice!u@h INVITE fred :#lab\r\n"));
    QCOMPARE(controller.inboxCount(), 1);

    QVERIFY(controller.sendMessage(QStringLiteral("/join #lab")));
    transport->injectBytes(QByteArrayLiteral(":fred!u@h JOIN :#lab\r\n"));
    QCOMPARE(controller.inboxCount(), 0);

    transport->injectBytes(QByteArrayLiteral(":bob!u@h INVITE fred :#desk\r\n"));
    QCOMPARE(controller.inboxCount(), 1);
    transport->injectBytes(QByteArrayLiteral(":alice!u@h INVITE fred :#lab\r\n"));
    QCOMPARE(controller.inboxCount(), 2);
    QVERIFY(controller.sendMessage(QStringLiteral("/join #lab")));
    transport->injectBytes(QByteArrayLiteral(":fred!u@h JOIN :#lab\r\n"));
    QCOMPARE(controller.inboxCount(), 1);
    auto *model = qobject_cast<IrcInboxModel *>(controller.inbox());
    QVERIFY(model);
    QCOMPARE(model->field(0, QStringLiteral("target")), QStringLiteral("#desk"));
}

void InboxTest::forgetNetworkStatePurgesInbox()
{
    const IrcCaseMapping mapping;
    IrcInbox inbox;
    inbox.append(sampleItem(IrcInboxKind::Mention), mapping);
    inbox.append(sampleItem(IrcInboxKind::Invite,
                            QStringLiteral("other"),
                            QStringLiteral("alice"),
                            QStringLiteral("#room")),
                mapping);
    inbox.purgeNetwork(networkA);
    QCOMPARE(inbox.count(), 1);
    QCOMPARE(inbox.at(0).networkId, QStringLiteral("other"));

    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(networkA));
    welcome(transport);
    transport->injectBytes(QByteArrayLiteral(":fred!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(networkA, QStringLiteral("#other"));
    transport->injectBytes(
        QByteArrayLiteral("@msgid=purge-1 :Alice!u@h PRIVMSG #omarchy :fred: ping\r\n"));
    QCOMPARE(controller.inboxCount(), 1);
    controller.forgetNetworkState(networkA);
    QCOMPARE(controller.inboxCount(), 0);
}

void InboxTest::controllerDismissInboxItem()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(networkA));
    welcome(transport);
    transport->injectBytes(QByteArrayLiteral(":fred!u@h JOIN :#omarchy\r\n"
                                             ":fred!u@h JOIN :#other\r\n"));
    controller.selectConversation(networkA, QStringLiteral("#other"));

    transport->injectBytes(
        QByteArrayLiteral("@msgid=dismiss-1 :Alice!u@h PRIVMSG #omarchy :fred: first\r\n"));
    transport->injectBytes(
        QByteArrayLiteral("@msgid=dismiss-2 :Bob!u@h PRIVMSG #omarchy :fred: second\r\n"));
    QCOMPARE(controller.inboxCount(), 2);
    auto *model = qobject_cast<IrcInboxModel *>(controller.inbox());
    QVERIFY(model);
    QCOMPARE(model->field(0, QStringLiteral("msgid")), QStringLiteral("dismiss-2"));
    QCOMPARE(model->field(1, QStringLiteral("msgid")), QStringLiteral("dismiss-1"));

    controller.dismissInboxItem(0);
    QCOMPARE(controller.inboxCount(), 1);
    QCOMPARE(model->field(0, QStringLiteral("msgid")), QStringLiteral("dismiss-1"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#other"));

    controller.dismissInboxItem(0);
    QCOMPARE(controller.inboxCount(), 0);
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#other"));
}

void InboxTest::controllerInboxCountTracksModel()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(networkA));
    welcome(transport);
    transport->injectBytes(QByteArrayLiteral(":fred!u@h JOIN :#omarchy\r\n"
                                             ":fred!u@h JOIN :#other\r\n"));
    controller.selectConversation(networkA, QStringLiteral("#other"));

    QSignalSpy spy(&controller, &IrcController::inboxChanged);
    transport->injectBytes(
        QByteArrayLiteral("@msgid=bg-1 :Alice!u@h PRIVMSG #omarchy :fred: ping\r\n"));
    QCOMPARE(spy.count(), 1);
    QCOMPARE(controller.inboxCount(), 1);
    auto *model = qobject_cast<IrcInboxModel *>(controller.inbox());
    QVERIFY(model);
    QCOMPARE(model->rowCount(), 1);
    QCOMPARE(model->field(0, QStringLiteral("kind")), QStringLiteral("mention"));
    QCOMPARE(model->field(0, QStringLiteral("msgid")), QStringLiteral("bg-1"));
}

int runInboxTests(int argc, char **argv)
{
    InboxTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_inbox.moc"
