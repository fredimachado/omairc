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
                 "  omairc [--mock]                 Open the client window\n"
                 "  omairc --help                   List commands and GUI flags\n"
                 "  omairc --version                Print version and exit\n"
                 "\n"
                 "Control commands (require a running client):\n"
                 "  omairc connections               List connections as JSON (alias: list)\n"
                 "  omairc status [--network ID]     Show one connection as JSON\n"
                 "  omairc send [--network ID] TARGET TEXT...\n"
                 "                                   Send a message without changing UI selection\n"
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
                 "\n"
                 "  --network ID   Connection to use. See connections.\n"
                 "  --             End options. Later args are the target and text.\n"
                 "\n"
                 "Examples:\n"
                 "  omairc send '#channel' hello\n"
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

int runOmaircCliTests(int argc, char **argv)
{
    OmaircCliTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_omairccli.moc"
