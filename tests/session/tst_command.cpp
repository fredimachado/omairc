#include <QAbstractItemModel>
#include <QTest>

#include "fakeirctransport.h"
#include "irccommand.h"
#include "irccontroller.h"
#include "ircsession.h"
#include "networklogmodel.h"

namespace
{
IrcSessionConfig config(const QString& networkId = QStringLiteral("libera"))
{
    IrcSessionConfig value;
    value.networkId = networkId;
    value.host = QStringLiteral("irc.example");
    value.tlsEnabled = true;
    value.nick = QStringLiteral("omairc");
    value.username = QStringLiteral("omairc");
    value.realname = QStringLiteral("Omairc User");
    value.reconnectEnabled = false;
    return value;
}

void welcome(FakeIrcTransport *transport)
{
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"));
}

QVariant roleAt(const QAbstractItemModel *model, int row, int role)
{
    return model->data(model->index(row, 0), role);
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
}

class CommandTest : public QObject
{
    Q_OBJECT

private slots:
    void parseEmptyAndSay();
    void parseSlashEscape();
    void parseVerbsAndAliases();
    void parseUnknown();
    void parseClose();
    void parseQuery();
    void parseTopic();
    void parseNotice();
    void catalogLookupAndScope();
    void closeWrongScopeUsesCatalogSentence();
    void conversationSendAndUnknown();
    void statusSubmitDoesNotSendAction();
};

void CommandTest::parseEmptyAndSay()
{
    QCOMPARE(IrcCommand::parse(QString()).verb, IrcCommand::Verb::Empty);
    QCOMPARE(IrcCommand::parse(QStringLiteral("   ")).verb, IrcCommand::Verb::Empty);
    const IrcCommand say = IrcCommand::parse(QStringLiteral("hello"));
    QCOMPARE(say.verb, IrcCommand::Verb::Say);
    QCOMPARE(say.argument, QStringLiteral("hello"));
    QVERIFY(say.isLiveMessage());
}

void CommandTest::parseSlashEscape()
{
    const IrcCommand escaped = IrcCommand::parse(QStringLiteral("//hi"));
    QCOMPARE(escaped.verb, IrcCommand::Verb::Say);
    QCOMPARE(escaped.argument, QStringLiteral("/hi"));
    QVERIFY(escaped.isLiveMessage());

    const IrcCommand triple = IrcCommand::parse(QStringLiteral("///x"));
    QCOMPARE(triple.verb, IrcCommand::Verb::Say);
    QCOMPARE(triple.argument, QStringLiteral("//x"));
}

void CommandTest::parseVerbsAndAliases()
{
    const IrcCommand action = IrcCommand::parse(QStringLiteral("/ME waves"));
    QCOMPARE(action.verb, IrcCommand::Verb::Action);
    QCOMPARE(action.argument, QStringLiteral("waves"));
    QCOMPARE(action.name, QStringLiteral("/ME"));
    QVERIFY(action.isLiveMessage());

    const IrcCommand join = IrcCommand::parse(QStringLiteral("/j #omarchy"));
    QCOMPARE(join.verb, IrcCommand::Verb::Join);
    QCOMPARE(join.argument, QStringLiteral("#omarchy"));
    QVERIFY(!join.isLiveMessage());

    const IrcCommand part = IrcCommand::parse(QStringLiteral("/LEAVE #omarchy"));
    QCOMPARE(part.verb, IrcCommand::Verb::Part);
    QCOMPARE(part.argument, QStringLiteral("#omarchy"));

    const IrcCommand barePart = IrcCommand::parse(QStringLiteral("/part"));
    QCOMPARE(barePart.verb, IrcCommand::Verb::Part);
    QVERIFY(barePart.argument.isEmpty());

    const IrcCommand bareLeave = IrcCommand::parse(QStringLiteral("/leave"));
    QCOMPARE(bareLeave.verb, IrcCommand::Verb::Part);
    QVERIFY(bareLeave.argument.isEmpty());

    const IrcCommand bareJoin = IrcCommand::parse(QStringLiteral("/join"));
    QCOMPARE(bareJoin.verb, IrcCommand::Verb::Join);
    QVERIFY(bareJoin.argument.isEmpty());
}

void CommandTest::parseUnknown()
{
    const IrcCommand unknown = IrcCommand::parse(QStringLiteral("/nope"));
    QCOMPARE(unknown.verb, IrcCommand::Verb::Unknown);
    QCOMPARE(unknown.name, QStringLiteral("/nope"));

    const IrcCommand withArg = IrcCommand::parse(QStringLiteral("/nope extra"));
    QCOMPARE(withArg.verb, IrcCommand::Verb::Unknown);
    QCOMPARE(withArg.argument, QStringLiteral("extra"));

    const IrcCommand notQuit = IrcCommand::parse(QStringLiteral("/q"));
    QCOMPARE(notQuit.verb, IrcCommand::Verb::Unknown);

    const IrcCommand notTopic = IrcCommand::parse(QStringLiteral("/t"));
    QCOMPARE(notTopic.verb, IrcCommand::Verb::Unknown);
}

void CommandTest::parseClose()
{
    const IrcCommand close = IrcCommand::parse(QStringLiteral("/close"));
    QCOMPARE(close.verb, IrcCommand::Verb::Close);
    QCOMPARE(close.name, QStringLiteral("/close"));
    QVERIFY(close.argument.isEmpty());
    QVERIFY(!close.isLiveMessage());
    QVERIFY(close.allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(!close.allowedOn(IrcComposerSurface::Status));

    const IrcCommand escaped = IrcCommand::parse(QStringLiteral("//close"));
    QCOMPARE(escaped.verb, IrcCommand::Verb::Say);
    QCOMPARE(escaped.argument, QStringLiteral("/close"));
    QVERIFY(escaped.isLiveMessage());
}

void CommandTest::parseQuery()
{
    const IrcCommand query = IrcCommand::parse(QStringLiteral("/query lena hi"));
    QCOMPARE(query.verb, IrcCommand::Verb::Query);
    QCOMPARE(query.argument, QStringLiteral("lena hi"));
    QCOMPARE(query.name, QStringLiteral("/query"));
    QVERIFY(!query.isLiveMessage());
    QVERIFY(query.allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(query.allowedOn(IrcComposerSurface::Status));

    const IrcCommand msg = IrcCommand::parse(QStringLiteral("/MSG lena"));
    QCOMPARE(msg.verb, IrcCommand::Verb::Query);
    QCOMPARE(msg.argument, QStringLiteral("lena"));
    QCOMPARE(msg.name, QStringLiteral("/MSG"));
    QVERIFY(!msg.isLiveMessage());

    const IrcCommand escapedQuery = IrcCommand::parse(QStringLiteral("//query"));
    QCOMPARE(escapedQuery.verb, IrcCommand::Verb::Say);
    QCOMPARE(escapedQuery.argument, QStringLiteral("/query"));
    QVERIFY(escapedQuery.isLiveMessage());

    const IrcCommand escapedMsg = IrcCommand::parse(QStringLiteral("//msg hi"));
    QCOMPARE(escapedMsg.verb, IrcCommand::Verb::Say);
    QCOMPARE(escapedMsg.argument, QStringLiteral("/msg hi"));
    QVERIFY(escapedMsg.isLiveMessage());
}

void CommandTest::parseTopic()
{
    const IrcCommand bare = IrcCommand::parse(QStringLiteral("/topic"));
    QCOMPARE(bare.verb, IrcCommand::Verb::Topic);
    QCOMPARE(bare.name, QStringLiteral("/topic"));
    QVERIFY(bare.argument.isEmpty());
    QVERIFY(!bare.isLiveMessage());
    QVERIFY(bare.allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(!bare.allowedOn(IrcComposerSurface::Status));

    const IrcCommand text = IrcCommand::parse(QStringLiteral("/TOPIC Hello world"));
    QCOMPARE(text.verb, IrcCommand::Verb::Topic);
    QCOMPARE(text.argument, QStringLiteral("Hello world"));
    QCOMPARE(text.name, QStringLiteral("/TOPIC"));
    QVERIFY(!text.isLiveMessage());

    const IrcCommand hash = IrcCommand::parse(
        QStringLiteral("/topic #omarchy is the place"));
    QCOMPARE(hash.verb, IrcCommand::Verb::Topic);
    QCOMPARE(hash.argument, QStringLiteral("#omarchy is the place"));
    QVERIFY(!hash.isLiveMessage());
}

void CommandTest::parseNotice()
{
    const IrcCommand notice = IrcCommand::parse(QStringLiteral("/notice lena later"));
    QCOMPARE(notice.verb, IrcCommand::Verb::Notice);
    QCOMPARE(notice.argument, QStringLiteral("lena later"));
    QCOMPARE(notice.name, QStringLiteral("/notice"));
    QVERIFY(!notice.isLiveMessage());
    QVERIFY(notice.allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(notice.allowedOn(IrcComposerSurface::Status));

    const IrcCommand channel = IrcCommand::parse(QStringLiteral("/NOTICE #omarchy hi"));
    QCOMPARE(channel.verb, IrcCommand::Verb::Notice);
    QCOMPARE(channel.argument, QStringLiteral("#omarchy hi"));
    QCOMPARE(channel.name, QStringLiteral("/NOTICE"));
    QVERIFY(!channel.isLiveMessage());

    const IrcCommand escaped = IrcCommand::parse(QStringLiteral("//notice hi"));
    QCOMPARE(escaped.verb, IrcCommand::Verb::Say);
    QCOMPARE(escaped.argument, QStringLiteral("/notice hi"));
    QVERIFY(escaped.isLiveMessage());
}

void CommandTest::catalogLookupAndScope()
{
    const IrcVerbSpec *join = IrcVerbTable::lookup(QStringLiteral("J"));
    QVERIFY(join);
    QCOMPARE(join->verb, IrcCommand::Verb::Join);
    QCOMPARE(join->usage, QStringLiteral("/join <channel>"));

    const IrcVerbSpec *leave = IrcVerbTable::lookup(QStringLiteral("leave"));
    QVERIFY(leave);
    QCOMPARE(leave->verb, IrcCommand::Verb::Part);
    QCOMPARE(leave->usage, QStringLiteral("/part [channel]"));
    QCOMPARE(leave->scope, IrcVerbScope::Either);
    QCOMPARE(leave->wrongScopeText, QStringLiteral("Part applies to channels"));

    QVERIFY(!IrcVerbTable::find(IrcCommand::Verb::Say));
    QVERIFY(!IrcVerbTable::find(IrcCommand::Verb::Empty));
    QVERIFY(!IrcVerbTable::find(IrcCommand::Verb::Unknown));

    QCOMPARE(IrcVerbTable::all().size(), 10);
    for (const IrcVerbSpec& row : IrcVerbTable::all())
        QVERIFY(row.name != QLatin1String("say"));

    const IrcVerbSpec *query = IrcVerbTable::lookup(QStringLiteral("msg"));
    QVERIFY(query);
    QCOMPARE(query->verb, IrcCommand::Verb::Query);
    QCOMPARE(query->name, QStringLiteral("query"));
    QCOMPARE(query->usage, QStringLiteral("/query <nick> [text]"));
    QCOMPARE(query->scope, IrcVerbScope::Either);
    QVERIFY(query->wrongScopeText.isEmpty());
    QVERIFY(query->aliases.contains(QStringLiteral("msg")));

    const IrcVerbSpec *close = IrcVerbTable::lookup(QStringLiteral("close"));
    QVERIFY(close);
    QCOMPARE(close->verb, IrcCommand::Verb::Close);
    QCOMPARE(close->usage, QStringLiteral("/close"));
    QVERIFY(close->aliases.isEmpty());
    QCOMPARE(close->scope, IrcVerbScope::Conversation);
    QCOMPARE(close->wrongScopeText, QStringLiteral("Close applies to direct messages"));

    const IrcVerbSpec *topic = IrcVerbTable::lookup(QStringLiteral("topic"));
    QVERIFY(topic);
    QCOMPARE(topic->verb, IrcCommand::Verb::Topic);
    QCOMPARE(topic->usage, QStringLiteral("/topic [text]"));
    QVERIFY(topic->aliases.isEmpty());
    QCOMPARE(topic->scope, IrcVerbScope::Conversation);
    QCOMPARE(topic->wrongScopeText, QStringLiteral("Topic applies to channels"));

    const IrcVerbSpec *notice = IrcVerbTable::lookup(QStringLiteral("notice"));
    QVERIFY(notice);
    QCOMPARE(notice->verb, IrcCommand::Verb::Notice);
    QCOMPARE(notice->name, QStringLiteral("notice"));
    QCOMPARE(notice->usage, QStringLiteral("/notice <target> <text>"));
    QCOMPARE(notice->scope, IrcVerbScope::Either);
    QVERIFY(notice->wrongScopeText.isEmpty());
    QVERIFY(notice->aliases.isEmpty());

    const QVector<IrcVerbSpec> status = IrcVerbTable::visibleOn(IrcComposerSurface::Status);
    QCOMPARE(status.size(), 7);
    for (const IrcVerbSpec& row : status) {
        QVERIFY(row.allowedOn(IrcComposerSurface::Status));
        QVERIFY(row.verb != IrcCommand::Verb::Action);
        QVERIFY(row.verb != IrcCommand::Verb::Close);
        QVERIFY(row.verb != IrcCommand::Verb::Topic);
    }

    const QVector<IrcVerbSpec> conversation =
        IrcVerbTable::visibleOn(IrcComposerSurface::Conversation);
    QCOMPARE(conversation.size(), 10);
    bool sawMe = false;
    bool sawClose = false;
    bool sawQuery = false;
    bool sawTopic = false;
    bool sawNotice = false;
    for (const IrcVerbSpec& row : conversation) {
        if (row.name == QLatin1String("me"))
            sawMe = true;
        if (row.name == QLatin1String("close"))
            sawClose = true;
        if (row.name == QLatin1String("query"))
            sawQuery = true;
        if (row.name == QLatin1String("topic"))
            sawTopic = true;
        if (row.name == QLatin1String("notice"))
            sawNotice = true;
    }
    QVERIFY(sawMe);
    QVERIFY(sawClose);
    QVERIFY(sawQuery);
    QVERIFY(sawTopic);
    QVERIFY(sawNotice);

    const IrcCommand say = IrcCommand::parse(QStringLiteral("hello"));
    QVERIFY(say.allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(!say.allowedOn(IrcComposerSurface::Status));
    const IrcCommand action = IrcCommand::parse(QStringLiteral("/me waves"));
    QVERIFY(action.allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(!action.allowedOn(IrcComposerSurface::Status));
    QVERIFY(IrcCommand::parse(QStringLiteral("/join #x"))
                .allowedOn(IrcComposerSurface::Status));
}

void CommandTest::closeWrongScopeUsesCatalogSentence()
{
    const IrcCommand close = IrcCommand::parse(QStringLiteral("/close"));
    QCOMPARE(ircCommandOutcomeText(IrcCommandOutcome::WrongScope, close),
             QStringLiteral("Close applies to direct messages"));

    const IrcCommand part = IrcCommand::parse(QStringLiteral("/part"));
    QCOMPARE(ircCommandOutcomeText(IrcCommandOutcome::WrongScope, part),
             QStringLiteral("Part applies to channels"));

    const IrcCommand action = IrcCommand::parse(QStringLiteral("/me waves"));
    QCOMPARE(ircCommandOutcomeText(IrcCommandOutcome::WrongScope, action),
             QStringLiteral("Select a connected conversation first"));
    const IrcCommand say = IrcCommand::parse(QStringLiteral("hello"));
    QCOMPARE(ircCommandOutcomeText(IrcCommandOutcome::WrongScope, say),
             QStringLiteral("Select a connected conversation first"));
    const IrcCommand query = IrcCommand::parse(QStringLiteral("/query lena"));
    QCOMPARE(ircCommandOutcomeText(IrcCommandOutcome::WrongScope, query),
             QStringLiteral("Select a connected conversation first"));
    const IrcCommand topic = IrcCommand::parse(QStringLiteral("/topic hello"));
    QCOMPARE(ircCommandOutcomeText(IrcCommandOutcome::WrongScope, topic),
             QStringLiteral("Topic applies to channels"));
}

void CommandTest::conversationSendAndUnknown()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    QCOMPARE(session->state(), IrcSession::State::Registered);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/me waves")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArray("PRIVMSG #omarchy :\x01" "ACTION waves\x01\r\n"));

    QVERIFY(!controller.sendMessage(QStringLiteral("/nope")));
    QCOMPARE(controller.lastError(), QStringLiteral("Unknown command: /nope"));

    QVERIFY(controller.sendMessage(QStringLiteral("/join #other")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("JOIN #other\r\n"));

    QVERIFY(controller.sendMessage(QStringLiteral("/topic new banner")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("TOPIC #omarchy :new banner\r\n"));

    const int beforeEmptyTopic = transport->writtenFrames().size();
    QVERIFY(controller.sendMessage(QStringLiteral("/topic")));
    QCOMPARE(transport->writtenFrames().size(), beforeEmptyTopic);
    QVERIFY(!framesContain(transport->writtenFrames().mid(beforeEmptyTopic),
                           QByteArrayLiteral("TOPIC")));

    QVERIFY(controller.sendMessage(QStringLiteral("/topic #foo is the topic")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("TOPIC #omarchy :#foo is the topic\r\n"));

    transport->injectBytes(QByteArrayLiteral(":lena!u@h PRIVMSG omairc :hi\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    const int beforeDmTopic = transport->writtenFrames().size();
    QVERIFY(!controller.sendMessage(QStringLiteral("/topic hello")));
    QCOMPARE(controller.lastError(), QStringLiteral("Topic applies to channels"));
    QCOMPARE(transport->writtenFrames().size(), beforeDmTopic);
    QVERIFY(!framesContain(transport->writtenFrames().mid(beforeDmTopic),
                           QByteArrayLiteral("TOPIC")));

    IrcController lonely;
    QVERIFY(!lonely.sendMessage(QStringLiteral("/me waves")));
    QCOMPARE(lonely.lastError(),
             QStringLiteral("Select a connected conversation first"));
    QVERIFY(!lonely.sendMessage(QStringLiteral("/query lena")));
    QCOMPARE(lonely.lastError(),
             QStringLiteral("Select a connected conversation first"));
    QVERIFY(!lonely.sendMessage(QStringLiteral("/clear")));
    QCOMPARE(lonely.lastError(),
             QStringLiteral("Select a connected conversation first"));
    QVERIFY(!lonely.sendMessage(QStringLiteral("/topic hello")));
    QCOMPARE(lonely.lastError(), QStringLiteral("Topic applies to channels"));
    QVERIFY(!lonely.sendMessage(QString()));
}

void CommandTest::statusSubmitDoesNotSendAction()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    QCOMPARE(session->state(), IrcSession::State::Registered);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    IrcStatusConsole *console = controller.console();
    QVERIFY(!console->submit(QString()));
    const int beforeAction = transport->writtenFrames().size();
    QVERIFY(console->submit(QStringLiteral("/me waves")));
    for (int i = beforeAction; i < transport->writtenFrames().size(); ++i)
        QVERIFY(!transport->writtenFrames().at(i).contains("ACTION"));
    QVERIFY(logContains(console->lines(),
                        QStringLiteral("Select a connected conversation first")));

    QVERIFY(console->submit(QStringLiteral("/close")));
    QVERIFY(logContains(console->lines(),
                        QStringLiteral("Close applies to direct messages")));

    const int beforeTopic = transport->writtenFrames().size();
    QVERIFY(console->submit(QStringLiteral("/topic hello")));
    QCOMPARE(transport->writtenFrames().size(), beforeTopic);
    QVERIFY(!framesContain(transport->writtenFrames().mid(beforeTopic),
                           QByteArrayLiteral("TOPIC")));
    QVERIFY(logContains(console->lines(),
                        QStringLiteral("Topic applies to channels")));

    QVERIFY(console->submit(QStringLiteral("/nope")));
    QVERIFY(logContains(console->lines(), QStringLiteral("Unknown command: /nope")));

    IrcController empty;
    QVERIFY(empty.console()->submit(QStringLiteral("/clear")));
}

int runCommandTests(int argc, char **argv)
{
    CommandTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_command.moc"
