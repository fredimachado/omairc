#include <QAbstractItemModel>
#include <QObject>
#include <QTest>

#include "conversationlistmodel.h"
#include "fakeirctransport.h"
#include "irccontroller.h"
#include "ircevent.h"
#include "irceventreducer.h"
#include "irceventtranslator.h"
#include "ircparser.h"
#include "ircsession.h"
#include "ircstatusentry.h"
#include "irctyping.h"
#include "irctypingpublisher.h"

#include <variant>

namespace
{
const QString network = QStringLiteral("libera");
const QDateTime t0 =
    QDateTime::fromString(QStringLiteral("2026-09-04T12:00:00Z"), Qt::ISODate);

IrcSessionConfig sessionConfig()
{
    IrcSessionConfig value;
    value.networkId = network;
    value.host = QStringLiteral("irc.example");
    value.tlsEnabled = true;
    value.nick = QStringLiteral("omairc");
    value.username = QStringLiteral("omairc");
    value.realname = QStringLiteral("Omairc User");
    value.reconnectEnabled = false;
    return value;
}

IrcMessage parse(const char *line)
{
    const IrcParseResult result = IrcParser::parse(line);
    Q_ASSERT(result);
    return *result.value;
}

void registerWithTags(IrcSession *session, FakeIrcTransport *transport)
{
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :message-tags\r\n"
                          ":server CAP omairc ACK :message-tags\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":server 005 omairc CHANTYPES=# PREFIX=(ov)@+ "
                          ":are supported by this server\r\n"));
    QCOMPARE(session->state(), IrcSession::State::Registered);
}

int tagmsgCount(const QByteArrayList& frames)
{
    int count = 0;
    for (const QByteArray& frame : frames) {
        if (frame.contains("TAGMSG"))
            ++count;
    }
    return count;
}

QVariant roleAt(const QAbstractItemModel *model, int row, int role)
{
    return model->data(model->index(row, 0), role);
}
}

class TypingTest : public QObject
{
    Q_OBJECT

private slots:
    void publisherThrottlesPerTarget();
    void publisherSuppressesDoneAfterChat();
    void publisherRefusesPausedAndUnknownTargets();
    void translatorEmitsTypingFromTagmsg();
    void translatorDropsUnknownAndUntagged();
    void translatorIgnoresTypingTagOnChat();
    void reducerStoresClocksAndExpiresAtRead();
    void reducerClearsOnChatLeaveQuit();
    void reducerRemapsNickIncludingDirectMessage();
    void reducerMergesTypingWhenDirectMessageNicksCollide();
    void reducerHidesSelfAndSkipsMissingConversation();
    void controllerNotifyComposerTextThrottles();
    void controllerDoesNotOpenConversationFromTypingOnly();
};

void TypingTest::publisherThrottlesPerTarget()
{
    IrcTypingPublisher publisher;
    QVERIFY(publisher.shouldSend(QStringLiteral("#omarchy"),
                                 IrcTypingPhase::Active, t0));
    publisher.recordSent(QStringLiteral("#omarchy"), IrcTypingPhase::Active, t0);
    QVERIFY(publisher.wasPublishing(QStringLiteral("#omarchy")));
    QVERIFY(!publisher.shouldSend(QStringLiteral("#omarchy"),
                                  IrcTypingPhase::Active, t0.addMSecs(2999)));
    QVERIFY(publisher.shouldSend(QStringLiteral("#omarchy"),
                                 IrcTypingPhase::Active, t0.addMSecs(3000)));
    QVERIFY(publisher.shouldSend(QStringLiteral("#desktop"),
                                 IrcTypingPhase::Active, t0.addMSecs(1000)));
    QVERIFY(!publisher.shouldSend(QStringLiteral("#omarchy"),
                                  IrcTypingPhase::Done, t0.addMSecs(1000)));
    QVERIFY(publisher.shouldSend(QStringLiteral("#omarchy"),
                                 IrcTypingPhase::Done, t0.addMSecs(3000)));
}

void TypingTest::publisherSuppressesDoneAfterChat()
{
    IrcTypingPublisher publisher;
    publisher.recordSent(QStringLiteral("#omarchy"), IrcTypingPhase::Active, t0);
    publisher.noteMessageSent(QStringLiteral("#omarchy"));
    QVERIFY(!publisher.shouldSend(QStringLiteral("#omarchy"),
                                  IrcTypingPhase::Done, t0.addMSecs(4000)));
    QVERIFY(publisher.wasPublishing(QStringLiteral("#omarchy")));
    QVERIFY(publisher.shouldSend(QStringLiteral("#omarchy"),
                                 IrcTypingPhase::Active, t0.addMSecs(4000)));
    publisher.recordSent(QStringLiteral("#omarchy"), IrcTypingPhase::Active,
                         t0.addMSecs(4000));
    QVERIFY(publisher.shouldSend(QStringLiteral("#omarchy"),
                                 IrcTypingPhase::Done, t0.addMSecs(7000)));
    publisher.reset();
    QVERIFY(!publisher.wasPublishing(QStringLiteral("#omarchy")));
    QVERIFY(publisher.shouldSend(QStringLiteral("#omarchy"),
                                 IrcTypingPhase::Active, t0));
}

void TypingTest::publisherRefusesPausedAndUnknownTargets()
{
    IrcTypingPublisher publisher;
    QVERIFY(!publisher.shouldSend(QStringLiteral("#omarchy"),
                                  IrcTypingPhase::Paused, t0));
    QVERIFY(!publisher.shouldSend(QStringLiteral("#omarchy"),
                                  IrcTypingPhase::Done, t0));
    QCOMPARE(ircTypingTagmsg(QStringLiteral("#omarchy"), IrcTypingPhase::Active),
             QByteArrayLiteral("@+typing=active TAGMSG #omarchy\r\n"));
    QVERIFY(ircTypingTagmsg(QStringLiteral("bad target"),
                            IrcTypingPhase::Active).isEmpty());
    QVERIFY(ircTypingTagmsg(QString(), IrcTypingPhase::Done).isEmpty());
}

void TypingTest::translatorEmitsTypingFromTagmsg()
{
    const IrcServerFeatures features;
    const std::vector<IrcEvent> events = IrcEventTranslator::translate(
        network, QStringLiteral("omairc"), features,
        parse("@+typing=active :alice!u@h TAGMSG #omarchy"));
    QCOMPARE(int(events.size()), 1);
    const auto *typing = std::get_if<IrcTypingEvent>(&events.front());
    QVERIFY(typing);
    QCOMPARE(typing->nick, QStringLiteral("alice"));
    QCOMPARE(typing->phase, IrcTypingPhase::Active);
    QCOMPARE(typing->conversation.normalizedTarget, QStringLiteral("#omarchy"));

    const std::vector<IrcEvent> paused = IrcEventTranslator::translate(
        network, QStringLiteral("omairc"), features,
        parse("@+typing=PAUSED :alice!u@h TAGMSG omairc"));
    QCOMPARE(int(paused.size()), 1);
    const auto *pausedEvent = std::get_if<IrcTypingEvent>(&paused.front());
    QVERIFY(pausedEvent);
    QCOMPARE(pausedEvent->phase, IrcTypingPhase::Paused);
    QCOMPARE(pausedEvent->conversation.normalizedTarget, QStringLiteral("alice"));
}

void TypingTest::translatorDropsUnknownAndUntagged()
{
    const IrcServerFeatures features;
    QVERIFY(IrcEventTranslator::translate(
                network, QStringLiteral("omairc"), features,
                parse("@+typing=draft :alice!u@h TAGMSG #omarchy")).empty());
    QVERIFY(IrcEventTranslator::translate(
                network, QStringLiteral("omairc"), features,
                parse(":alice!u@h TAGMSG #omarchy")).empty());
    QVERIFY(IrcEventTranslator::translate(
                network, QStringLiteral("omairc"), features,
                parse("@+typing=active TAGMSG #omarchy")).empty());
}

void TypingTest::translatorIgnoresTypingTagOnChat()
{
    const IrcServerFeatures features;
    const std::vector<IrcEvent> events = IrcEventTranslator::translate(
        network, QStringLiteral("omairc"), features,
        parse("@+typing=active :alice!u@h PRIVMSG #omarchy :here"));
    QCOMPARE(int(events.size()), 1);
    QVERIFY(std::holds_alternative<IrcMessageEvent>(events.front()));
}

void TypingTest::reducerStoresClocksAndExpiresAtRead()
{
    IrcEventReducer reducer;
    reducer.apply(IrcWelcomeEvent{network, QStringLiteral("omairc")});
    reducer.apply(IrcJoinEvent{network, QStringLiteral("#omarchy"),
                               QStringLiteral("omairc")});
    reducer.apply(IrcJoinEvent{network, QStringLiteral("#omarchy"),
                               QStringLiteral("alice")});
    const IrcConversationKey key =
        reducer.conversationKey(network, QStringLiteral("#omarchy"));

    reducer.apply(IrcTypingEvent{key, QStringLiteral("alice"),
                                 IrcTypingPhase::Active, t0});
    QCOMPARE(reducer.typingNicks(key, t0.addMSecs(5999)),
             QStringList{QStringLiteral("alice")});
    QVERIFY(reducer.typingNicks(key, t0.addMSecs(6000)).isEmpty());
    QVERIFY(reducer.find(key)->typing.count(QStringLiteral("alice")) == 1);

    reducer.apply(IrcTypingEvent{key, QStringLiteral("alice"),
                                 IrcTypingPhase::Active, t0.addMSecs(4000)});
    QCOMPARE(reducer.typingNicks(key, t0.addMSecs(9000)),
             QStringList{QStringLiteral("alice")});

    reducer.apply(IrcTypingEvent{key, QStringLiteral("bob"),
                                 IrcTypingPhase::Paused, t0});
    QCOMPARE(reducer.typingNicks(key, t0.addMSecs(5000)).size(), 2);
    QCOMPARE(reducer.typingNicks(key, t0.addMSecs(10000)),
             QStringList{QStringLiteral("bob")});
    QVERIFY(reducer.typingNicks(key, t0.addMSecs(30000)).isEmpty());

    reducer.apply(IrcTypingEvent{key, QStringLiteral("alice"),
                                 IrcTypingPhase::Done, t0.addMSecs(5000)});
    reducer.apply(IrcTypingEvent{key, QStringLiteral("bob"),
                                 IrcTypingPhase::Done, t0.addMSecs(5000)});
    QVERIFY(reducer.typingNicks(key, t0.addMSecs(5000)).isEmpty());
}

void TypingTest::reducerClearsOnChatLeaveQuit()
{
    IrcEventReducer reducer;
    reducer.apply(IrcWelcomeEvent{network, QStringLiteral("omairc")});
    reducer.apply(IrcJoinEvent{network, QStringLiteral("#omarchy"),
                               QStringLiteral("omairc")});
    reducer.apply(IrcJoinEvent{network, QStringLiteral("#desktop"),
                               QStringLiteral("omairc")});
    const IrcConversationKey omarchy =
        reducer.conversationKey(network, QStringLiteral("#omarchy"));
    const IrcConversationKey desktop =
        reducer.conversationKey(network, QStringLiteral("#desktop"));
    reducer.apply(IrcTypingEvent{omarchy, QStringLiteral("alice"),
                                 IrcTypingPhase::Active, t0});
    reducer.apply(IrcTypingEvent{desktop, QStringLiteral("alice"),
                                 IrcTypingPhase::Active, t0});

    reducer.apply(IrcMessageEvent{omarchy, QStringLiteral("alice"),
                                  QStringLiteral("here"), t0, QStringLiteral("#omarchy")});
    QVERIFY(reducer.typingNicks(omarchy, t0).isEmpty());
    QCOMPARE(reducer.typingNicks(desktop, t0),
             QStringList{QStringLiteral("alice")});

    reducer.apply(IrcTypingEvent{omarchy, QStringLiteral("alice"),
                                 IrcTypingPhase::Active, t0});
    reducer.apply(IrcPartEvent{network, QStringLiteral("#omarchy"),
                               QStringLiteral("alice"), QString()});
    QVERIFY(reducer.typingNicks(omarchy, t0).isEmpty());

    reducer.apply(IrcTypingEvent{desktop, QStringLiteral("bob"),
                                 IrcTypingPhase::Active, t0});
    reducer.apply(IrcQuitEvent{network, QStringLiteral("alice"), QStringLiteral("gone")});
    QVERIFY(reducer.typingNicks(desktop, t0)
                == QStringList{QStringLiteral("bob")});

    reducer.apply(IrcWelcomeEvent{network, QStringLiteral("omairc")});
    QVERIFY(reducer.typingNicks(desktop, t0).isEmpty());
}

void TypingTest::reducerRemapsNickIncludingDirectMessage()
{
    IrcEventReducer reducer;
    reducer.apply(IrcWelcomeEvent{network, QStringLiteral("omairc")});
    reducer.apply(IrcJoinEvent{network, QStringLiteral("#omarchy"),
                               QStringLiteral("omairc")});
    const IrcConversationKey channel =
        reducer.conversationKey(network, QStringLiteral("#omarchy"));
    const IrcConversationKey aliceDm =
        reducer.conversationKey(network, QStringLiteral("Alice"));
    reducer.apply(IrcMessageEvent{aliceDm, QStringLiteral("Alice"),
                                  QStringLiteral("hi"), t0, QStringLiteral("Alice")});
    reducer.apply(IrcTypingEvent{channel, QStringLiteral("Alice"),
                                 IrcTypingPhase::Active, t0});
    reducer.apply(IrcTypingEvent{aliceDm, QStringLiteral("Alice"),
                                 IrcTypingPhase::Active, t0});

    reducer.apply(IrcNickEvent{network, QStringLiteral("Alice"),
                               QStringLiteral("Alicia")});

    QCOMPARE(reducer.typingNicks(channel, t0),
             QStringList{QStringLiteral("Alicia")});
    QVERIFY(!reducer.find(aliceDm));
    const IrcConversationKey aliciaDm =
        reducer.conversationKey(network, QStringLiteral("Alicia"));
    QCOMPARE(reducer.typingNicks(aliciaDm, t0),
             QStringList{QStringLiteral("Alicia")});
}

void TypingTest::reducerMergesTypingWhenDirectMessageNicksCollide()
{
    IrcEventReducer reducer;
    reducer.apply(IrcWelcomeEvent{network, QStringLiteral("omairc")});
    const IrcConversationKey aliceDm =
        reducer.conversationKey(network, QStringLiteral("Alice"));
    const IrcConversationKey aliciaDm =
        reducer.conversationKey(network, QStringLiteral("Alicia"));
    reducer.apply(IrcMessageEvent{aliceDm, QStringLiteral("Alice"),
                                  QStringLiteral("hi"), t0, QStringLiteral("Alice")});
    reducer.apply(IrcMessageEvent{aliciaDm, QStringLiteral("Alicia"),
                                  QStringLiteral("yo"), t0, QStringLiteral("Alicia")});
    reducer.apply(IrcTypingEvent{aliceDm, QStringLiteral("Alice"),
                                 IrcTypingPhase::Active, t0});

    reducer.apply(IrcNickEvent{network, QStringLiteral("Alice"),
                               QStringLiteral("Alicia")});

    QVERIFY(!reducer.find(aliceDm));
    QCOMPARE(reducer.typingNicks(aliciaDm, t0),
             QStringList{QStringLiteral("Alicia")});
}

void TypingTest::reducerHidesSelfAndSkipsMissingConversation()
{
    IrcEventReducer reducer;
    reducer.apply(IrcWelcomeEvent{network, QStringLiteral("omairc")});
    const IrcConversationKey missing =
        reducer.conversationKey(network, QStringLiteral("#omarchy"));
    reducer.apply(IrcTypingEvent{missing, QStringLiteral("alice"),
                                 IrcTypingPhase::Active, t0});
    QVERIFY(!reducer.find(missing));
    QVERIFY(reducer.typingNicks(missing, t0).isEmpty());

    reducer.apply(IrcJoinEvent{network, QStringLiteral("#omarchy"),
                               QStringLiteral("omairc")});
    reducer.apply(IrcTypingEvent{missing, QStringLiteral("omairc"),
                                 IrcTypingPhase::Active, t0});
    QVERIFY(reducer.typingNicks(missing, t0).isEmpty());
}

void TypingTest::controllerNotifyComposerTextThrottles()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(sessionConfig(), transport);
    QVERIFY(session);
    QList<IrcStatusEntry> journal;
    QObject::connect(session, &IrcSession::statusEntry, session,
                     [&journal](const IrcStatusEntry& entry) {
        journal.append(entry);
    });
    QVERIFY(controller.start(network));
    registerWithTags(session, transport);
    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(network, QStringLiteral("#omarchy"));
    QVERIFY(controller.hasTyping());
    QCOMPARE(controller.typingNicks(), QStringList());

    controller.notifyComposerText(QStringLiteral("hello"));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("@+typing=active TAGMSG #omarchy\r\n"));
    const int firstCount = tagmsgCount(transport->writtenFrames());
    controller.notifyComposerText(QStringLiteral("hello there"));
    QCOMPARE(tagmsgCount(transport->writtenFrames()), firstCount);

    bool labeledTyping = false;
    for (const IrcStatusEntry& entry : journal) {
        if (entry.label().contains(QLatin1String("TYPING"), Qt::CaseInsensitive))
            labeledTyping = true;
    }
    QVERIFY(!labeledTyping);

    transport->injectBytes(
        QByteArrayLiteral("@+typing=active :alice!u@h TAGMSG #omarchy\r\n"));
    bool inboundTagmsg = false;
    for (const IrcStatusEntry& entry : journal) {
        if (entry.label() == QStringLiteral("TAGMSG"))
            inboundTagmsg = true;
    }
    QVERIFY(inboundTagmsg);

    controller.notifyComposerText(QStringLiteral("/join #other"));
    QCOMPARE(tagmsgCount(transport->writtenFrames()), firstCount);

    QVERIFY(controller.sendMessage(QStringLiteral("hello")));
    controller.notifyComposerText(QString());
    QCOMPARE(tagmsgCount(transport->writtenFrames()), firstCount);
}

void TypingTest::controllerDoesNotOpenConversationFromTypingOnly()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(sessionConfig(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(network));
    registerWithTags(session, transport);
    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"
                          ":alice!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(network, QStringLiteral("#omarchy"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    const int conversationCount = conversations->rowCount();
    const int messageCount = messages->rowCount();

    transport->injectBytes(
        QByteArrayLiteral("@+typing=active :alice!u@h TAGMSG #omarchy\r\n"
                          "@+typing=active :bob!u@h TAGMSG omairc\r\n"));
    QCOMPARE(controller.typingNicks(), QStringList{QStringLiteral("alice")});
    QVERIFY(controller.nickIsTyping(QStringLiteral("ALICE")));
    QVERIFY(!controller.nickIsTyping(QStringLiteral("omairc")));
    QCOMPARE(conversations->rowCount(), conversationCount);
    QCOMPARE(messages->rowCount(), messageCount);
    QCOMPARE(roleAt(conversations, 0, ConversationListModel::ConversationRole),
             QStringLiteral("#omarchy"));

    transport->injectBytes(
        QByteArrayLiteral(":alice!u@h PRIVMSG #omarchy :here\r\n"));
    QVERIFY(controller.typingNicks().isEmpty());
    QVERIFY(!controller.nickIsTyping(QStringLiteral("Alice")));

    transport->injectBytes(
        QByteArrayLiteral("@+typing=active :Alice!u@h TAGMSG #omarchy\r\n"
                          ":Alice!u@h PART #omarchy\r\n"));
    QVERIFY(controller.typingNicks().isEmpty());

    transport->injectBytes(
        QByteArrayLiteral(":Alice!u@h JOIN :#omarchy\r\n"
                          "@+typing=active :Alice!u@h TAGMSG #omarchy\r\n"
                          ":Alice!u@h QUIT :gone\r\n"));
    QVERIFY(controller.typingNicks().isEmpty());

    transport->injectBytes(
        QByteArrayLiteral("@+typing=active :ghost!u@h TAGMSG omairc\r\n"));
    QCOMPARE(conversations->rowCount(), conversationCount);
    QVERIFY(controller.typingNicks().isEmpty());

    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc DEL :message-tags\r\n"));
    QVERIFY(!controller.hasTyping());
}

int runTypingTests(int argc, char **argv)
{
    TypingTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_typing.moc"
