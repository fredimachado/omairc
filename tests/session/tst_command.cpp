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
}

class CommandTest : public QObject
{
    Q_OBJECT

private slots:
    void parseEmptyAndSay();
    void parseSlashEscape();
    void parseVerbsAndAliases();
    void parseUnknown();
    void catalogLookupAndScope();
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

    QVERIFY(!IrcVerbTable::find(IrcCommand::Verb::Say));
    QVERIFY(!IrcVerbTable::find(IrcCommand::Verb::Empty));
    QVERIFY(!IrcVerbTable::find(IrcCommand::Verb::Unknown));

    QCOMPARE(IrcVerbTable::all().size(), 6);
    for (const IrcVerbSpec& row : IrcVerbTable::all())
        QVERIFY(row.name != QLatin1String("say"));

    const QVector<IrcVerbSpec> status = IrcVerbTable::visibleOn(IrcComposerSurface::Status);
    QCOMPARE(status.size(), 5);
    for (const IrcVerbSpec& row : status) {
        QVERIFY(row.allowedOn(IrcComposerSurface::Status));
        QVERIFY(row.verb != IrcCommand::Verb::Action);
    }

    const QVector<IrcVerbSpec> conversation =
        IrcVerbTable::visibleOn(IrcComposerSurface::Conversation);
    QCOMPARE(conversation.size(), 6);
    bool sawMe = false;
    for (const IrcVerbSpec& row : conversation) {
        if (row.name == QLatin1String("me"))
            sawMe = true;
    }
    QVERIFY(sawMe);
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

    IrcController lonely;
    QVERIFY(!lonely.sendMessage(QStringLiteral("/me waves")));
    QCOMPARE(lonely.lastError(),
             QStringLiteral("Select a connected conversation first"));
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
