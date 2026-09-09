#include <QTest>
#include <QVector>

#include <variant>

#include "omairccli.h"

namespace {

QVector<QByteArray> argvStorage(const QStringList &tokens)
{
    QVector<QByteArray> bytes;
    bytes.reserve(tokens.size());
    for (const QString &token : tokens)
        bytes.append(token.toLocal8Bit());
    return bytes;
}

QVector<char *> argvPointers(QVector<QByteArray> &bytes)
{
    QVector<char *> pointers;
    pointers.reserve(bytes.size());
    for (QByteArray &item : bytes)
        pointers.append(item.data());
    return pointers;
}

OmaircCli::StartupPlan planArgv(const QStringList &tokens,
                                const char *version = OMAIRC_VERSION)
{
    QVector<QByteArray> bytes = argvStorage(tokens);
    QVector<char *> argv = argvPointers(bytes);
    return OmaircCli::plan(int(argv.size()), argv.data(), version);
}

const QByteArray kRootHelp = QByteArrayLiteral(
    "Usage: omairc [--mock] [--help] [--version] [command]\n"
    "\n"
    "A dead-simple IRC client for Omarchy.\n"
    "\n"
    "Options:\n"
    "  --mock      Open the local prototype UI without connecting.\n"
    "  -h, --help  Show this help.\n"
    "  --version   Show the version.\n"
    "\n"
    "Commands:\n"
    "  connections, list  List IRC connections.\n"
    "  status             Show connection status.\n"
    "  send               Send a message to a channel or nick.\n"
    "  raise              Raise the running Omairc window.\n"
    "\n"
    "Run 'omairc <command> --help' for command help.\n");

const QByteArray kConnectionsHelp = QByteArrayLiteral(
    "Usage: omairc connections\n"
    "\n"
    "List IRC connections.\n"
    "\n"
    "Options:\n"
    "  -h, --help  Show this help.\n");

const QByteArray kStatusHelp = QByteArrayLiteral(
    "Usage: omairc status [--network ID]\n"
    "\n"
    "Show connection status.\n"
    "\n"
    "Options:\n"
    "  --network ID  Select a connection by id.\n"
    "  -h, --help    Show this help.\n");

const QByteArray kSendHelp = QByteArrayLiteral(
    "Usage: omairc send [--network ID] [--] TARGET TEXT...\n"
    "\n"
    "Send a message to a channel or nick.\n"
    "\n"
    "Options:\n"
    "  --network ID  Select a connection by id.\n"
    "  -h, --help    Show this help.\n"
    "  --            End option processing.\n");

const QByteArray kRaiseHelp = QByteArrayLiteral(
    "Usage: omairc raise\n"
    "\n"
    "Raise the running Omairc window.\n"
    "\n"
    "Options:\n"
    "  -h, --help  Show this help.\n");

const QByteArray kNetworkRequiresId =
    QByteArrayLiteral("{\"error\":\"--network requires an id\",\"ok\":false}\n");
const QByteArray kConnectionsExtra =
    QByteArrayLiteral("{\"error\":\"connections takes no arguments\",\"ok\":false}\n");

}

class OmaircCliTest : public QObject
{
    Q_OBJECT

private slots:
    void rootHelpLong();
    void rootHelpShort();
    void sendHelpIsLocal();
    void listHelpUsesConnectionsCopy();
    void statusHelp();
    void raiseHelp();
    void sendHelpWhileNetworkOptionOpen();
    void helpAfterSendTargetIsText();
    void versionAfterSendTargetIsText();
    void doubleDashKeepsHelpText();
    void sendAllowsDashPrefixedText();
    void versionIsLocal();
    void statusMissingNetworkIdFails();
    void connectionsExtraFails();
    void bareLaunchIsGui();
    void mockLaunchIsGui();
    void mockHelpIsProgramHelp();
    void unknownFirstTokenStaysGui();
};

void OmaircCliTest::rootHelpLong()
{
    const auto plan = planArgv({QStringLiteral("omairc"), QStringLiteral("--help")});
    const auto &finished = std::get<OmaircCli::TerminalExit>(plan);
    QCOMPARE(finished.code, 0);
    QCOMPARE(finished.standardOutput, kRootHelp);
    QVERIFY(finished.standardError.isEmpty());
}

void OmaircCliTest::rootHelpShort()
{
    const auto plan = planArgv({QStringLiteral("omairc"), QStringLiteral("-h")});
    const auto &finished = std::get<OmaircCli::TerminalExit>(plan);
    QCOMPARE(finished.standardOutput, kRootHelp);
    QCOMPARE(finished.code, 0);
}

void OmaircCliTest::sendHelpIsLocal()
{
    const auto plan =
        planArgv({QStringLiteral("omairc"), QStringLiteral("send"),
                  QStringLiteral("--help")});
    const auto &finished = std::get<OmaircCli::TerminalExit>(plan);
    QCOMPARE(finished.code, 0);
    QCOMPARE(finished.standardOutput, kSendHelp);
    QVERIFY(finished.standardError.isEmpty());
}

void OmaircCliTest::listHelpUsesConnectionsCopy()
{
    const auto plan =
        planArgv({QStringLiteral("omairc"), QStringLiteral("list"),
                  QStringLiteral("--help")});
    const auto &finished = std::get<OmaircCli::TerminalExit>(plan);
    QCOMPARE(finished.standardOutput, kConnectionsHelp);
}

void OmaircCliTest::statusHelp()
{
    const auto plan =
        planArgv({QStringLiteral("omairc"), QStringLiteral("status"),
                  QStringLiteral("--help")});
    QCOMPARE(std::get<OmaircCli::TerminalExit>(plan).standardOutput, kStatusHelp);
}

void OmaircCliTest::raiseHelp()
{
    const auto plan =
        planArgv({QStringLiteral("omairc"), QStringLiteral("raise"),
                  QStringLiteral("--help")});
    QCOMPARE(std::get<OmaircCli::TerminalExit>(plan).standardOutput, kRaiseHelp);
}

void OmaircCliTest::sendHelpWhileNetworkOptionOpen()
{
    const auto plan = planArgv(
        {QStringLiteral("omairc"), QStringLiteral("send"),
         QStringLiteral("--network"), QStringLiteral("n"),
         QStringLiteral("--help")});
    const auto &finished = std::get<OmaircCli::TerminalExit>(plan);
    QCOMPARE(finished.standardOutput, kSendHelp);
    QCOMPARE(finished.code, 0);
}

void OmaircCliTest::helpAfterSendTargetIsText()
{
    const auto plan =
        planArgv({QStringLiteral("omairc"), QStringLiteral("send"),
                  QStringLiteral("#omarchy"), QStringLiteral("--help")});
    const auto &control = std::get<OmaircCli::ControlRequest>(plan);
    QCOMPARE(control.request.command, OmaircIpc::Command::Send);
    QCOMPARE(control.request.target, QStringLiteral("#omarchy"));
    QCOMPARE(control.request.text, QStringLiteral("--help"));
}

void OmaircCliTest::versionAfterSendTargetIsText()
{
    const auto plan =
        planArgv({QStringLiteral("omairc"), QStringLiteral("send"),
                  QStringLiteral("#c"), QStringLiteral("--version")});
    const auto &control = std::get<OmaircCli::ControlRequest>(plan);
    QCOMPARE(control.request.command, OmaircIpc::Command::Send);
    QCOMPARE(control.request.target, QStringLiteral("#c"));
    QCOMPARE(control.request.text, QStringLiteral("--version"));
}

void OmaircCliTest::doubleDashKeepsHelpText()
{
    const auto plan = planArgv(
        {QStringLiteral("omairc"), QStringLiteral("send"), QStringLiteral("--"),
         QStringLiteral("#c"), QStringLiteral("--help")});
    const auto &control = std::get<OmaircCli::ControlRequest>(plan);
    QCOMPARE(control.request.target, QStringLiteral("#c"));
    QCOMPARE(control.request.text, QStringLiteral("--help"));
}

void OmaircCliTest::sendAllowsDashPrefixedText()
{
    const auto plan =
        planArgv({QStringLiteral("omairc"), QStringLiteral("send"),
                  QStringLiteral("#chan"), QStringLiteral("-hello")});
    const auto &control = std::get<OmaircCli::ControlRequest>(plan);
    QCOMPARE(control.request.target, QStringLiteral("#chan"));
    QCOMPARE(control.request.text, QStringLiteral("-hello"));

    const auto withDashDash = planArgv(
        {QStringLiteral("omairc"), QStringLiteral("send"), QStringLiteral("--"),
         QStringLiteral("#chan"), QStringLiteral("--network"),
         QStringLiteral("not-an-option")});
    const auto &request = std::get<OmaircCli::ControlRequest>(withDashDash).request;
    QCOMPARE(request.target, QStringLiteral("#chan"));
    QCOMPARE(request.text, QStringLiteral("--network not-an-option"));
    QVERIFY(request.networkId.isEmpty());
}

void OmaircCliTest::versionIsLocal()
{
    const auto plan =
        planArgv({QStringLiteral("omairc"), QStringLiteral("--version")}, "9.9.9");
    const auto &finished = std::get<OmaircCli::TerminalExit>(plan);
    QCOMPARE(finished.code, 0);
    QCOMPARE(finished.standardOutput, QByteArrayLiteral("omairc 9.9.9\n"));
    QVERIFY(finished.standardError.isEmpty());
}

void OmaircCliTest::statusMissingNetworkIdFails()
{
    const auto plan =
        planArgv({QStringLiteral("omairc"), QStringLiteral("status"),
                  QStringLiteral("--network")});
    const auto &finished = std::get<OmaircCli::TerminalExit>(plan);
    QCOMPARE(finished.code, 1);
    QCOMPARE(finished.standardOutput, kNetworkRequiresId);
    QCOMPARE(finished.standardError, QByteArrayLiteral("--network requires an id\n"));
}

void OmaircCliTest::connectionsExtraFails()
{
    const auto plan =
        planArgv({QStringLiteral("omairc"), QStringLiteral("connections"),
                  QStringLiteral("extra")});
    const auto &finished = std::get<OmaircCli::TerminalExit>(plan);
    QCOMPARE(finished.code, 1);
    QCOMPARE(finished.standardOutput, kConnectionsExtra);
    QCOMPARE(finished.standardError,
             QByteArrayLiteral("connections takes no arguments\n"));
}

void OmaircCliTest::bareLaunchIsGui()
{
    const auto plan = planArgv({QStringLiteral("omairc")});
    const auto &gui = std::get<OmaircCli::GuiLaunch>(plan);
    QCOMPARE(gui.mockMode, false);
}

void OmaircCliTest::mockLaunchIsGui()
{
    const auto plan =
        planArgv({QStringLiteral("omairc"), QStringLiteral("--mock")});
    const auto &gui = std::get<OmaircCli::GuiLaunch>(plan);
    QCOMPARE(gui.mockMode, true);
}

void OmaircCliTest::mockHelpIsProgramHelp()
{
    const auto plan =
        planArgv({QStringLiteral("omairc"), QStringLiteral("--mock"),
                  QStringLiteral("--help")});
    QCOMPARE(std::get<OmaircCli::TerminalExit>(plan).standardOutput, kRootHelp);

    const auto versionHelp =
        planArgv({QStringLiteral("omairc"), QStringLiteral("--version"),
                  QStringLiteral("--help")});
    QCOMPARE(std::get<OmaircCli::TerminalExit>(versionHelp).standardOutput,
             kRootHelp);
}

void OmaircCliTest::unknownFirstTokenStaysGui()
{
    const auto plan =
        planArgv({QStringLiteral("omairc"), QStringLiteral("-platform"),
                  QStringLiteral("offscreen")});
    const auto &gui = std::get<OmaircCli::GuiLaunch>(plan);
    QCOMPARE(gui.mockMode, false);

    const auto mockPlatform = planArgv(
        {QStringLiteral("omairc"), QStringLiteral("--mock"),
         QStringLiteral("-platform"), QStringLiteral("offscreen")});
    QCOMPARE(std::get<OmaircCli::GuiLaunch>(mockPlatform).mockMode, true);
}

int runOmaircCliTests(int argc, char **argv)
{
    OmaircCliTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_omairccli.moc"
