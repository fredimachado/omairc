#include <QTest>

#include <variant>

#include "omairccli.h"

class OmaircCliTest : public QObject
{
    Q_OBJECT

private slots:
    void overviewHelp();
    void commandPages();
    void sendHelpVsText();
    void versionRequest();
    void sendAllowsDashPrefixedText();
    void readParseAndHelp();
    void namesAndConversationsParse();
};

void OmaircCliTest::overviewHelp()
{
    const auto fromFlag = OmaircCli::parseArgs({QStringLiteral("--help")});
    QVERIFY(std::holds_alternative<OmaircCli::HelpTopic>(fromFlag));
    QCOMPARE(std::get<OmaircCli::HelpTopic>(fromFlag).scope,
             OmaircCli::HelpScope::Overview);
    QCOMPARE(OmaircCli::formatHelp(std::get<OmaircCli::HelpTopic>(fromFlag)),
             QStringLiteral(
                 "A dead-simple IRC client for Omarchy.\n"
                 "\n"
                 "Usage:\n"
                 "  omairc [--demo-server]          Open the client window\n"
                 "  omairc --help                   List commands and GUI flags\n"
                 "  omairc --version                Print version and exit\n"
                 "\n"
                 "Control commands (require a running client):\n"
                 "  omairc connections               List connections as JSON (alias: list)\n"
                 "  omairc status [--network ID]     Show one connection as JSON\n"
                 "  omairc send [--network ID] TARGET TEXT...\n"
                 "                                   Send a message without changing UI selection\n"
                 "  omairc read [--network ID] [TARGET] [--last N|--since DURATION|--unread]\n"
                 "                                   Snapshot chat as JSON without changing UI selection\n"
                 "  omairc names [--network ID] TARGET\n"
                 "                                   List members of a joined channel as JSON\n"
                 "  omairc conversations [--network ID]\n"
                 "                                   List channels and DMs as JSON\n"
                 "  omairc raise                     Activate the existing window\n"
                 "\n"
                 "Run 'omairc <command> --help' for command detail.\n"));
}

void OmaircCliTest::commandPages()
{
    const auto helpSend = OmaircCli::parseArgs(
        {QStringLiteral("help"), QStringLiteral("send")});
    QVERIFY(std::holds_alternative<OmaircCli::HelpTopic>(helpSend));
    QCOMPARE(std::get<OmaircCli::HelpTopic>(helpSend).command,
             OmaircCli::CommandId::Send);

    const auto extra = OmaircCli::parseArgs(
        {QStringLiteral("--help"), QStringLiteral("foo")});
    QVERIFY(std::holds_alternative<OmaircCli::CliError>(extra));

    const auto list = OmaircCli::parseArgs(
        {QStringLiteral("list"), QStringLiteral("--help")});
    QVERIFY(std::holds_alternative<OmaircCli::HelpTopic>(list));
    QCOMPARE(std::get<OmaircCli::HelpTopic>(list).command,
             OmaircCli::CommandId::Connections);

    const auto connections = OmaircCli::parseArgs(
        {QStringLiteral("connections"), QStringLiteral("--help")});
    QVERIFY(std::holds_alternative<OmaircCli::HelpTopic>(connections));
    QCOMPARE(std::get<OmaircCli::HelpTopic>(connections).command,
             OmaircCli::CommandId::Connections);
    QCOMPARE(OmaircCli::formatHelp(std::get<OmaircCli::HelpTopic>(connections)),
             QStringLiteral(
                 "Usage: omairc connections\n"
                 "\n"
                 "List connections as JSON. list is an alias.\n"
                 "\n"
                 "Each row has id, host, port, tls, nick, state, and selected.\n"));

    const auto status = OmaircCli::parseArgs(
        {QStringLiteral("status"), QStringLiteral("--help")});
    QVERIFY(std::holds_alternative<OmaircCli::HelpTopic>(status));
    QCOMPARE(OmaircCli::formatHelp(std::get<OmaircCli::HelpTopic>(status)),
             QStringLiteral(
                 "Usage: omairc status [--network ID]\n"
                 "\n"
                 "Show one connection as JSON.\n"
                 "\n"
                 "  --network ID   Connection to use. See connections.\n"
                 "\n"
                 "With one connection, --network may be omitted.\n"));

    const auto raise = OmaircCli::parseArgs(
        {QStringLiteral("raise"), QStringLiteral("--help")});
    QVERIFY(std::holds_alternative<OmaircCli::HelpTopic>(raise));
    QCOMPARE(OmaircCli::formatHelp(std::get<OmaircCli::HelpTopic>(raise)),
             QStringLiteral(
                 "Usage: omairc raise\n"
                 "\n"
                 "Activate the existing Omairc window.\n"));
}

void OmaircCliTest::sendHelpVsText()
{
    const auto help = OmaircCli::parseArgs(
        {QStringLiteral("send"), QStringLiteral("--help")});
    QVERIFY(std::holds_alternative<OmaircCli::HelpTopic>(help));
    QCOMPARE(std::get<OmaircCli::HelpTopic>(help).command,
             OmaircCli::CommandId::Send);
    QCOMPARE(OmaircCli::formatHelp(std::get<OmaircCli::HelpTopic>(help)),
             QStringLiteral(
                 "Usage: omairc send [--network ID] TARGET TEXT...\n"
                 "\n"
                 "Send a message to a channel or nick without changing the UI selection.\n"
                 "Quote channel targets. # starts a shell comment.\n"
                 "Quote TEXT when it contains spaces.\n"
                 "\n"
                 "  --network ID   Connection to use. See connections.\n"
                 "  --             End options. Later args are the target and text.\n"
                 "\n"
                 "Examples:\n"
                 "  omairc send '#channel' hello\n"
                 "  omairc send '#channel' 'hello there'\n"
                 "  omairc send --network abc '#channel' hello\n"
                 "  omairc send '#channel' --help\n"
                 "  omairc send -- '#channel' --version\n"));

    const auto request = OmaircCli::parseArgs(
        {QStringLiteral("send"), QStringLiteral("#chan"),
         QStringLiteral("--help")});
    QVERIFY(std::holds_alternative<OmaircIpc::Request>(request));
    QCOMPARE(std::get<OmaircIpc::Request>(request).text,
             QStringLiteral("--help"));
}

void OmaircCliTest::versionRequest()
{
    const auto outcome = OmaircCli::parseArgs({QStringLiteral("--version")});
    QVERIFY(std::holds_alternative<OmaircCli::VersionRequest>(outcome));
    QCOMPARE(OmaircCli::formatVersion(),
             QStringLiteral("omairc ") + QLatin1String(OMAIRC_VERSION)
                 + QLatin1Char('\n'));
}

void OmaircCliTest::sendAllowsDashPrefixedText()
{
    const auto request = OmaircCli::parseArgs(
        {QStringLiteral("send"), QStringLiteral("#chan"),
         QStringLiteral("-hello")});
    QVERIFY(std::holds_alternative<OmaircIpc::Request>(request));
    QCOMPARE(std::get<OmaircIpc::Request>(request).text,
             QStringLiteral("-hello"));

    const auto withDashDash = OmaircCli::parseArgs(
        {QStringLiteral("send"), QStringLiteral("--"),
         QStringLiteral("#chan"), QStringLiteral("--network"),
         QStringLiteral("not-an-option")});
    QVERIFY(std::holds_alternative<OmaircIpc::Request>(withDashDash));
    QCOMPARE(std::get<OmaircIpc::Request>(withDashDash).text,
             QStringLiteral("--network not-an-option"));
}

void OmaircCliTest::readParseAndHelp()
{
    const auto help = OmaircCli::parseArgs({QStringLiteral("read"),
                                            QStringLiteral("--help")});
    QVERIFY(std::holds_alternative<OmaircCli::HelpTopic>(help));
    QCOMPARE(OmaircCli::formatHelp(std::get<OmaircCli::HelpTopic>(help)),
             QStringLiteral(
                 "Usage: omairc read [--network ID] [TARGET] [--last N|--since DURATION|--unread]\n"
                 "\n"
                 "Snapshot recent chat from the running window as JSON.\n"
                 "\n"
                 "No target means every channel and DM on that network.\n"
                 "A named target that is not in the window is an error.\n"
                 "read does not invent conversations.\n"
                 "Quote channel targets. # starts a shell comment.\n"
                 "Output kinds are message, notice, and action. Join and part lines stay out.\n"
                 "\n"
                 "  --network ID     Connection to use. See connections.\n"
                 "  --last N         Newest N lines. Default 50. Maximum 100.\n"
                 "  --since DURATION Lines in the last window, still capped at 100 newest.\n"
                 "                   Examples: 5m, 1h. Units are s, m, h, d.\n"
                 "  --unread         Lines after the CLI cursor for that target, or for the\n"
                 "                   network when untargeted. Still capped at 100 newest.\n"
                 "                   Does not change UI selection or GUI unread badges.\n"
                 "                   Stored under $XDG_STATE_HOME/omairc/.\n"
                 "  --               End options. Later args are the target.\n"
                 "\n"
                 "Give exactly one window flag. They do not combine.\n"
                 "When the 100-line cap drops older lines, the JSON includes \"truncated\": true.\n"
                 "With one connection, --network may be omitted.\n"
                 "\n"
                 "Examples:\n"
                 "  omairc read --last 20\n"
                 "  omairc read '#channel' --last 20\n"
                 "  omairc read --network abc nick --since 5m\n"
                 "  omairc read --unread\n"
                 "  omairc read '#channel' --unread\n"));

    const auto def = OmaircCli::parseArgs({QStringLiteral("read")});
    QVERIFY(std::holds_alternative<OmaircIpc::Request>(def));
    QCOMPARE(std::get<OmaircIpc::Request>(def).command, OmaircIpc::Command::Read);
    QCOMPARE(std::get<OmaircIpc::Request>(def).target, QString());
    QVERIFY(std::holds_alternative<OmaircIpc::LastWindow>(
        std::get<OmaircIpc::Request>(def).window));
    QCOMPARE(std::get<OmaircIpc::LastWindow>(
                 std::get<OmaircIpc::Request>(def).window).count,
             50);

    const auto last = OmaircCli::parseArgs(
        {QStringLiteral("read"), QStringLiteral("--last"), QStringLiteral("20"),
         QStringLiteral("#chan")});
    QVERIFY(std::holds_alternative<OmaircIpc::Request>(last));
    QCOMPARE(std::get<OmaircIpc::Request>(last).target, QStringLiteral("#chan"));
    QCOMPARE(std::get<OmaircIpc::LastWindow>(
                 std::get<OmaircIpc::Request>(last).window).count,
             20);

    const auto afterTarget = OmaircCli::parseArgs(
        {QStringLiteral("read"), QStringLiteral("#chan"),
         QStringLiteral("--unread")});
    QVERIFY(std::holds_alternative<OmaircIpc::Request>(afterTarget));
    QVERIFY(std::holds_alternative<OmaircIpc::UnreadWindow>(
        std::get<OmaircIpc::Request>(afterTarget).window));

    const auto over = OmaircCli::parseArgs(
        {QStringLiteral("read"), QStringLiteral("--last"), QStringLiteral("101")});
    QVERIFY(std::holds_alternative<OmaircCli::CliError>(over));
    QVERIFY(std::get<OmaircCli::CliError>(over).message.contains(
        QStringLiteral("100")));

    const auto mixed = OmaircCli::parseArgs(
        {QStringLiteral("read"), QStringLiteral("--unread"),
         QStringLiteral("--last"), QStringLiteral("1")});
    QVERIFY(std::holds_alternative<OmaircCli::CliError>(mixed));

    const auto since = OmaircCli::parseArgs(
        {QStringLiteral("read"), QStringLiteral("--since"), QStringLiteral("5m")});
    QVERIFY(std::holds_alternative<OmaircIpc::Request>(since));
    QVERIFY(std::holds_alternative<OmaircIpc::SinceWindow>(
        std::get<OmaircIpc::Request>(since).window));
    QCOMPARE(std::get<OmaircIpc::SinceWindow>(
                 std::get<OmaircIpc::Request>(since).window).token,
             QStringLiteral("5m"));

    const auto badSince = OmaircCli::parseArgs(
        {QStringLiteral("read"), QStringLiteral("--since"), QStringLiteral("no")});
    QVERIFY(std::holds_alternative<OmaircCli::CliError>(badSince));

    const auto overflowSince = OmaircCli::parseArgs(
        {QStringLiteral("read"), QStringLiteral("--since"),
         QStringLiteral("9999999999999999s")});
    QVERIFY(std::holds_alternative<OmaircCli::CliError>(overflowSince));
    QCOMPARE(std::get<OmaircCli::CliError>(overflowSince).message,
             QStringLiteral("Invalid --since value"));
}

void OmaircCliTest::namesAndConversationsParse()
{
    const auto namesHelp = OmaircCli::parseArgs(
        {QStringLiteral("names"), QStringLiteral("--help")});
    QVERIFY(std::holds_alternative<OmaircCli::HelpTopic>(namesHelp));
    QCOMPARE(OmaircCli::formatHelp(std::get<OmaircCli::HelpTopic>(namesHelp)),
             QStringLiteral(
                 "Usage: omairc names [--network ID] TARGET\n"
                 "\n"
                 "List the current members of a joined channel as JSON.\n"
                 "This is the member panel snapshot, not a live NAMES round-trip.\n"
                 "A DM, Status, or a channel that is not joined is an error.\n"
                 "Quote channel targets. # starts a shell comment.\n"
                 "\n"
                 "  --network ID   Connection to use. See connections.\n"
                 "  --             End options. Later args are the target.\n"
                 "\n"
                 "With one connection, --network may be omitted.\n"
                 "\n"
                 "Examples:\n"
                 "  omairc names '#channel'\n"
                 "  omairc names --network abc '#channel'\n"
                 "  omairc names -- --dash-nick\n"));

    const auto missing = OmaircCli::parseArgs({QStringLiteral("names")});
    QVERIFY(std::holds_alternative<OmaircCli::CliError>(missing));

    const auto names = OmaircCli::parseArgs(
        {QStringLiteral("names"), QStringLiteral("#chan")});
    QVERIFY(std::holds_alternative<OmaircIpc::Request>(names));
    QCOMPARE(std::get<OmaircIpc::Request>(names).command,
             OmaircIpc::Command::Names);
    QCOMPARE(std::get<OmaircIpc::Request>(names).target,
             QStringLiteral("#chan"));

    const auto dashTarget = OmaircCli::parseArgs(
        {QStringLiteral("names"), QStringLiteral("--"),
         QStringLiteral("--dash-nick")});
    QVERIFY(std::holds_alternative<OmaircIpc::Request>(dashTarget));
    QCOMPARE(std::get<OmaircIpc::Request>(dashTarget).target,
             QStringLiteral("--dash-nick"));

    const auto conversationsHelp = OmaircCli::parseArgs(
        {QStringLiteral("conversations"), QStringLiteral("--help")});
    QVERIFY(std::holds_alternative<OmaircCli::HelpTopic>(conversationsHelp));
    QCOMPARE(OmaircCli::formatHelp(std::get<OmaircCli::HelpTopic>(conversationsHelp)),
             QStringLiteral(
                 "Usage: omairc conversations [--network ID]\n"
                 "\n"
                 "List channels and DMs on that network as JSON.\n"
                 "Each row has target, channel, unread, and mention from the GUI.\n"
                 "Channel rows also include topic.\n"
                 "This snapshot does not clear those badges.\n"
                 "\n"
                 "  --network ID   Connection to use. See connections.\n"
                 "\n"
                 "With one connection, --network may be omitted.\n"));

    const auto conversations = OmaircCli::parseArgs(
        {QStringLiteral("conversations")});
    QVERIFY(std::holds_alternative<OmaircIpc::Request>(conversations));
    QCOMPARE(std::get<OmaircIpc::Request>(conversations).command,
             OmaircIpc::Command::Conversations);
}

int runOmaircCliTests(int argc, char **argv)
{
    OmaircCliTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_omairccli.moc"
