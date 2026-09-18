#include <QCoreApplication>
#include <QDateTime>
#include <QDir>
#include <QElapsedTimer>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLocalServer>
#include <QLocalSocket>
#include <QProcess>
#include <QSettings>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QTest>

#include <memory>
#include <variant>

#include "fakeirctransport.h"
#include "testsettings.h"
#include "irccontroller.h"
#include "ircmessage.h"
#include "omairccli.h"
#include "omaircclipcursor.h"
#include "omaircipc.h"
#include "omaircipchandler.h"
#include "singleinstance.h"

namespace {

IrcSessionConfig testConfig(const QString &networkId)
{
    IrcSessionConfig value;
    value.networkId = networkId;
    value.host = QStringLiteral("irc.example");
    value.port = 6697;
    value.tlsEnabled = true;
    value.nick = QStringLiteral("omairc");
    value.username = QStringLiteral("omairc");
    value.realname = QStringLiteral("Omairc User");
    value.reconnectEnabled = false;
    return value;
}

void registerSession(IrcSession *session, FakeIrcTransport *transport)
{
    session->start();
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    QCOMPARE(session->state(), IrcSession::State::Registered);
}

void seedChannelAndDirect(FakeIrcTransport *transport)
{
    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"
                          ":irc.example 332 omairc #omarchy :A cozy corner\r\n"
                          ":alice!u@h JOIN :#omarchy\r\n"
                          "@time=2011-10-19T16:40:51.620Z;msgid=old :alice!u@h PRIVMSG #omarchy :older\r\n"
                          "@time=2011-10-19T16:41:00.000Z;msgid=mid :alice!u@h PRIVMSG #omarchy :middle\r\n"
                          "@time=2011-10-19T16:42:00.000Z;msgid=new :alice!u@h PRIVMSG #omarchy :newest\r\n"
                          "@time=2011-10-19T16:43:00.000Z;msgid=act :alice!u@h PRIVMSG #omarchy :\x01""ACTION waves\x01\r\n"
                          "@time=2011-10-19T16:44:00.000Z;msgid=dm-1 :alice!u@h PRIVMSG omairc :reply\r\n"));
}

QByteArray framesJoin(const QByteArrayList &frames)
{
    QByteArray joined;
    for (const QByteArray &frame : frames)
        joined += frame;
    return joined;
}

QString omaircBinary()
{
    const QDir dir(QCoreApplication::applicationDirPath());
    const QStringList candidates{
        dir.filePath(QStringLiteral("../build/omairc")),
        dir.filePath(QStringLiteral("../build/omairc.exe")),
        dir.filePath(QStringLiteral("../build/release/omairc.exe")),
        dir.filePath(QStringLiteral("../build/omairc.app/Contents/MacOS/omairc")),
    };
    for (const QString &path : candidates) {
        const QFileInfo info(path);
        if (info.exists() && info.isExecutable())
            return info.canonicalFilePath();
    }
    return {};
}

QByteArray lastJsonLine(const QByteArray &bytes)
{
    for (const QByteArray &line : bytes.split('\n')) {
        const QByteArray trimmed = line.trimmed();
        if (trimmed.startsWith('{'))
            return trimmed;
    }
    return {};
}

struct CliProcessResult {
    int exitCode = -1;
    QByteArray stdoutBytes;
    QByteArray stderrBytes;
};

bool listenMuteServer(QLocalServer *server)
{
    const QString path = SingleInstance::socketPath();
    QLocalServer::removeServer(path);
    server->setSocketOptions(QLocalServer::UserAccessOption);
    return server->listen(path);
}

void dropAfterRequest(QLocalSocket *socket)
{
    QElapsedTimer timer;
    timer.start();
    QByteArray buffer;
    while (!buffer.contains('\n') && timer.elapsed() < 3000) {
        if (socket->bytesAvailable() == 0)
            socket->waitForReadyRead(int(3000 - timer.elapsed()));
        buffer += socket->readAll();
    }
    socket->disconnectFromServer();
    if (socket->state() != QLocalSocket::UnconnectedState)
        socket->waitForDisconnected(1000);
}

CliProcessResult runOmaircCli(const QStringList &args)
{
    CliProcessResult result;
    QProcess proc;
    proc.setProgram(omaircBinary());
    proc.setArguments(args);
    proc.start();
    if (!proc.waitForStarted(5000))
        return result;
    if (!proc.waitForFinished(5000)) {
        proc.kill();
        proc.waitForFinished(1000);
        return result;
    }
    result.exitCode = proc.exitCode();
    result.stdoutBytes = proc.readAllStandardOutput();
    result.stderrBytes = proc.readAllStandardError();
    return result;
}

CliProcessResult runOmaircCliAgainstMute(const QStringList &args)
{
    QLocalServer server;
    if (!listenMuteServer(&server))
        return {};

    QProcess proc;
    proc.setProgram(omaircBinary());
    proc.setArguments(args);
    proc.start();
    if (!proc.waitForStarted(5000))
        return {};
    if (!server.waitForNewConnection(3000)) {
        proc.kill();
        proc.waitForFinished(1000);
        return {};
    }
    QLocalSocket *socket = server.nextPendingConnection();
    if (!socket) {
        proc.kill();
        proc.waitForFinished(1000);
        return {};
    }
    dropAfterRequest(socket);
    if (!proc.waitForFinished(5000)) {
        proc.kill();
        proc.waitForFinished(1000);
        return {};
    }

    CliProcessResult result;
    result.exitCode = proc.exitCode();
    result.stdoutBytes = proc.readAllStandardOutput();
    result.stderrBytes = proc.readAllStandardError();
    return result;
}

class ScopedCursorRoot
{
public:
    explicit ScopedCursorRoot(const QString &root)
        : m_had(qEnvironmentVariableIsSet("OMAIRC_CURSOR_ROOT"))
        , m_previous(qgetenv("OMAIRC_CURSOR_ROOT"))
    {
        qputenv("OMAIRC_CURSOR_ROOT", root.toUtf8());
    }

    ~ScopedCursorRoot()
    {
        if (m_had)
            qputenv("OMAIRC_CURSOR_ROOT", m_previous);
        else
            qunsetenv("OMAIRC_CURSOR_ROOT");
    }

private:
    bool m_had;
    QByteArray m_previous;
};

}

class OmaircIpcTest : public QObject
{
    Q_OBJECT

private slots:
    void init();
    void parseRaisePing();
    void parseRaiseJson();
    void parseConnectionsAndList();
    void parseStatusWithNetwork();
    void parseSend();
    void parseUnknownCommand();
    void parseSendMissingFields();
    void resolveNetworkRules();
    void resolveErrorsAreInterfaceNeutral();
    void connectionsResponseShape();
    void handlerRaiseAndUnknown();
    void handlerStatusResolve();
    void handlerConnectionsAndSend();
    void handlerConnectionsEmitsResolvedName();
    void handlerSendSplitsLongLine();
    void socketRaiseStillWorks();
    void socketCommandRoundTrip();
    void socketRejectsOversizedLine();
    void socketRejectsOversizedResponse();
    void socketClosesAfterOneRequest();
    void connectionsSortedById();
    void handlerUsesNetworkScopedErrors();
    void sendAllowsDashPrefixedText();
    void socketRaisePingWithHandlerRaisesOnce();
    void socketAcceptsSplitRaisePing();
    void socketAcceptsDisconnectedRaisePing();
    void socketExpiresIdleClient();
    void socketLimitsConcurrentClients();
    void parseReadWindows();
    void handlerReadNamesConversations();
    void handlerUnreadDoesNotChmodCursorRootParent();
    void handlerReadReportsTruncated();
    void handlerUnreadKeepsSameMillisecondLines();
    void uncertainResponseShape();
    void cliSendUncertainWhenReplyDropped();
    void cliSendNotUncertainWhenClientMissing();

private:
    std::unique_ptr<QTemporaryDir> m_settingsDir;
};

void OmaircIpcTest::init()
{
    m_settingsDir = std::make_unique<QTemporaryDir>();
    QVERIFY(m_settingsDir->isValid());
    TestSettings::isolate(m_settingsDir->path());
    QCoreApplication::setOrganizationName(QStringLiteral("omairc"));
    QCoreApplication::setApplicationName(QStringLiteral("omairc"));
}

void OmaircIpcTest::parseRaisePing()
{
    QVERIFY(OmaircIpc::isRaisePing(QByteArrayLiteral("!")));
    QVERIFY(OmaircIpc::isRaisePing(QByteArrayLiteral("!\n")));
    QVERIFY(!OmaircIpc::isRaisePing(QByteArrayLiteral("raise")));

    OmaircIpc::ParseError error;
    const auto request = OmaircIpc::parseRequest(QByteArrayLiteral("!"), &error);
    QVERIFY(request.has_value());
    QCOMPARE(request->command, OmaircIpc::Command::Raise);
}

void OmaircIpcTest::parseRaiseJson()
{
    const auto request =
        OmaircIpc::parseRequest(QByteArrayLiteral("{\"cmd\":\"raise\"}"));
    QVERIFY(request.has_value());
    QCOMPARE(request->command, OmaircIpc::Command::Raise);
}

void OmaircIpcTest::parseConnectionsAndList()
{
    const auto connections =
        OmaircIpc::parseRequest(QByteArrayLiteral("{\"cmd\":\"connections\"}"));
    QVERIFY(connections.has_value());
    QCOMPARE(connections->command, OmaircIpc::Command::Connections);

    const auto list =
        OmaircIpc::parseRequest(QByteArrayLiteral("{\"cmd\":\"list\"}"));
    QVERIFY(list.has_value());
    QCOMPARE(list->command, OmaircIpc::Command::Connections);
}

void OmaircIpcTest::parseStatusWithNetwork()
{
    const auto request = OmaircIpc::parseRequest(
        QByteArrayLiteral("{\"cmd\":\"status\",\"network\":\"abc\"}"));
    QVERIFY(request.has_value());
    QCOMPARE(request->command, OmaircIpc::Command::Status);
    QCOMPARE(request->networkId, QStringLiteral("abc"));
}

void OmaircIpcTest::parseSend()
{
    const auto request = OmaircIpc::parseRequest(QByteArrayLiteral(
        "{\"cmd\":\"send\",\"network\":\"n1\",\"target\":\"#chan\",\"text\":\"hi there\"}"));
    QVERIFY(request.has_value());
    QCOMPARE(request->command, OmaircIpc::Command::Send);
    QCOMPARE(request->networkId, QStringLiteral("n1"));
    QCOMPARE(request->target, QStringLiteral("#chan"));
    QCOMPARE(request->text, QStringLiteral("hi there"));
}

void OmaircIpcTest::parseUnknownCommand()
{
    OmaircIpc::ParseError error;
    const auto request =
        OmaircIpc::parseRequest(QByteArrayLiteral("{\"cmd\":\"explode\"}"), &error);
    QVERIFY(!request.has_value());
    QVERIFY(error.message.contains(QStringLiteral("Unknown command")));
}

void OmaircIpcTest::parseSendMissingFields()
{
    OmaircIpc::ParseError error;
    QVERIFY(!OmaircIpc::parseRequest(
                 QByteArrayLiteral("{\"cmd\":\"send\",\"target\":\"#c\"}"), &error)
                 .has_value());
    QVERIFY(error.message.contains(QStringLiteral("text")));

    QVERIFY(!OmaircIpc::parseRequest(
                 QByteArrayLiteral("{\"cmd\":\"send\",\"text\":\"hi\"}"), &error)
                 .has_value());
    QVERIFY(error.message.contains(QStringLiteral("target")));
}

void OmaircIpcTest::resolveNetworkRules()
{
    const QStringList none;
    auto zero = OmaircIpc::resolveNetworkId(QString(), none);
    QVERIFY(!zero.ok);
    QVERIFY(zero.error.contains(QStringLiteral("connections")));

    const QStringList one{QStringLiteral("only")};
    auto implied = OmaircIpc::resolveNetworkId(QString(), one);
    QVERIFY(implied.ok);
    QCOMPARE(implied.networkId, QStringLiteral("only"));

    const QStringList many{QStringLiteral("a"), QStringLiteral("b")};
    auto ambiguous = OmaircIpc::resolveNetworkId(QString(), many);
    QVERIFY(!ambiguous.ok);
    QCOMPARE(ambiguous.error,
             QStringLiteral("Multiple connections are available; specify a network id."));

    auto explicitId = OmaircIpc::resolveNetworkId(QStringLiteral("b"), many);
    QVERIFY(explicitId.ok);
    QCOMPARE(explicitId.networkId, QStringLiteral("b"));

    auto missing = OmaircIpc::resolveNetworkId(QStringLiteral("z"), many);
    QVERIFY(!missing.ok);
    QVERIFY(missing.error.contains(QStringLiteral("Unknown network")));
}

void OmaircIpcTest::resolveErrorsAreInterfaceNeutral()
{
    const auto ambiguous = OmaircIpc::resolveNetworkId(
        QString(), {QStringLiteral("a"), QStringLiteral("b")});
    QVERIFY(!ambiguous.ok);
    QVERIFY(!ambiguous.error.contains(QStringLiteral("--network")));
    QVERIFY(!ambiguous.error.contains(QStringLiteral("omairc")));

    const auto missing = OmaircIpc::resolveNetworkId(
        QStringLiteral("z"), {QStringLiteral("a")});
    QVERIFY(!missing.ok);
    QVERIFY(!missing.error.contains(QStringLiteral("omairc")));
}

void OmaircIpcTest::connectionsResponseShape()
{
    QVector<OmaircIpc::ConnectionInfo> infos;
    OmaircIpc::ConnectionInfo info;
    info.id = QStringLiteral("nid");
    info.name = QStringLiteral("Example Net");
    info.host = QStringLiteral("irc.example");
    info.port = 6697;
    info.tls = true;
    info.nick = QStringLiteral("omairc");
    info.state = QStringLiteral("Connected");
    info.selected = true;
    infos.append(info);

    OmaircIpc::ConnectionInfo plain;
    plain.id = QStringLiteral("plain");
    plain.host = QStringLiteral("irc.local");
    plain.port = 6667;
    plain.tls = false;
    plain.nick = QStringLiteral("guest");
    plain.state = QStringLiteral("Offline");
    plain.selected = false;
    infos.append(plain);

    const QByteArray line = OmaircIpc::okConnections(infos);
    QVERIFY(OmaircIpc::responseOk(line));
    const QJsonArray connections = OmaircIpc::responseConnections(line);
    QCOMPARE(connections.size(), 2);
    const QJsonObject row = connections.at(0).toObject();
    QCOMPARE(row.value(QStringLiteral("id")).toString(), QStringLiteral("nid"));
    QCOMPARE(row.value(QStringLiteral("name")).toString(),
             QStringLiteral("Example Net"));
    QCOMPARE(row.value(QStringLiteral("host")).toString(),
             QStringLiteral("irc.example"));
    QCOMPARE(row.value(QStringLiteral("port")).toInt(), 6697);
    QCOMPARE(row.value(QStringLiteral("tls")).toBool(), true);
    QCOMPARE(row.value(QStringLiteral("nick")).toString(),
             QStringLiteral("omairc"));
    QCOMPARE(row.value(QStringLiteral("state")).toString(),
             QStringLiteral("Connected"));
    QCOMPARE(row.value(QStringLiteral("selected")).toBool(), true);
    QVERIFY(!row.contains(QStringLiteral("lastError")));

    const QJsonObject sparse = connections.at(1).toObject();
    QCOMPARE(sparse.value(QStringLiteral("id")).toString(), QStringLiteral("plain"));
    QCOMPARE(sparse.value(QStringLiteral("port")).toInt(), 6667);
    QVERIFY(!sparse.contains(QStringLiteral("name")));
    QVERIFY(!sparse.contains(QStringLiteral("tls")));
    QVERIFY(!sparse.contains(QStringLiteral("selected")));
    QVERIFY(!sparse.contains(QStringLiteral("lastError")));
}

void OmaircIpcTest::handlerRaiseAndUnknown()
{
    bool raised = false;
    OmaircIpcHandler handler(nullptr, [&raised]() { raised = true; });
    const QByteArray ok =
        handler.handleLine(QByteArrayLiteral("{\"cmd\":\"raise\"}"));
    QVERIFY(OmaircIpc::responseOk(ok));
    QVERIFY(raised);

    const QByteArray bad =
        handler.handleLine(QByteArrayLiteral("{\"cmd\":\"nope\"}"));
    QVERIFY(!OmaircIpc::responseOk(bad));
    QVERIFY(OmaircIpc::responseError(bad).contains(QStringLiteral("Unknown")));
}

void OmaircIpcTest::handlerStatusResolve()
{
    OmaircIpcHandler handler(nullptr);
    const QByteArray response =
        handler.handleLine(QByteArrayLiteral("{\"cmd\":\"status\"}"));
    QVERIFY(!OmaircIpc::responseOk(response));
    QVERIFY(OmaircIpc::responseError(response).contains(
        QStringLiteral("connections")));
}

void OmaircIpcTest::handlerConnectionsAndSend()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session =
        controller.addSession(testConfig(QStringLiteral("net-1")), transport);
    QVERIFY(session);
    registerSession(session, transport);

    OmaircIpcHandler handler(&controller);
    const QByteArray connectionsLine =
        handler.handleLine(QByteArrayLiteral("{\"cmd\":\"connections\"}"));
    QVERIFY(OmaircIpc::responseOk(connectionsLine));
    const QJsonArray connections = OmaircIpc::responseConnections(connectionsLine);
    QCOMPARE(connections.size(), 1);
    const QJsonObject row = connections.at(0).toObject();
    QCOMPARE(row.value(QStringLiteral("id")).toString(), QStringLiteral("net-1"));
    QCOMPARE(row.value(QStringLiteral("name")).toString(),
             QStringLiteral("irc.example"));
    QCOMPARE(row.value(QStringLiteral("host")).toString(),
             QStringLiteral("irc.example"));
    QCOMPARE(row.value(QStringLiteral("port")).toInt(), 6697);
    QCOMPARE(row.value(QStringLiteral("tls")).toBool(), true);
    QCOMPARE(row.value(QStringLiteral("nick")).toString(),
             QStringLiteral("omairc"));
    QCOMPARE(row.value(QStringLiteral("state")).toString(),
             QStringLiteral("Connected"));

    const QByteArray statusLine =
        handler.handleLine(QByteArrayLiteral("{\"cmd\":\"status\"}"));
    QVERIFY(OmaircIpc::responseOk(statusLine));
    QCOMPARE(OmaircIpc::responseStatus(statusLine)
                 .value(QStringLiteral("id"))
                 .toString(),
             QStringLiteral("net-1"));
    QCOMPARE(OmaircIpc::responseStatus(statusLine)
                 .value(QStringLiteral("name"))
                 .toString(),
             QStringLiteral("irc.example"));

    const int framesBefore = transport->writtenFrames().size();
    const QByteArray sendLine = handler.handleLine(QByteArrayLiteral(
        "{\"cmd\":\"send\",\"target\":\"#omarchy\",\"text\":\"hello agents\"}"));
    QVERIFY(OmaircIpc::responseOk(sendLine));
    QVERIFY(framesJoin(transport->writtenFrames().mid(framesBefore))
                .contains(QByteArrayLiteral("PRIVMSG #omarchy :hello agents")));
}

void OmaircIpcTest::handlerConnectionsEmitsResolvedName()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSessionConfig config = testConfig(QStringLiteral("net-1"));
    config.name = QStringLiteral("Example Net");
    IrcSession *session = controller.addSession(config, transport);
    QVERIFY(session);
    registerSession(session, transport);

    OmaircIpcHandler handler(&controller);
    const QByteArray connectionsLine =
        handler.handleLine(QByteArrayLiteral("{\"cmd\":\"connections\"}"));
    QVERIFY(OmaircIpc::responseOk(connectionsLine));
    const QJsonArray connections = OmaircIpc::responseConnections(connectionsLine);
    QCOMPARE(connections.size(), 1);
    const QJsonObject row = connections.at(0).toObject();
    QCOMPARE(row.value(QStringLiteral("name")).toString(),
             QStringLiteral("Example Net"));
    QCOMPARE(row.value(QStringLiteral("host")).toString(),
             QStringLiteral("irc.example"));

    const QByteArray statusLine =
        handler.handleLine(QByteArrayLiteral("{\"cmd\":\"status\"}"));
    QVERIFY(OmaircIpc::responseOk(statusLine));
    QCOMPARE(OmaircIpc::responseStatus(statusLine)
                 .value(QStringLiteral("name"))
                 .toString(),
             QStringLiteral("Example Net"));
}

void OmaircIpcTest::handlerSendSplitsLongLine()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session =
        controller.addSession(testConfig(QStringLiteral("net-1")), transport);
    QVERIFY(session);
    registerSession(session, transport);

    OmaircIpcHandler handler(&controller);
    const QByteArray first(400, 'a');
    const QByteArray second(200, 'b');
    const QByteArray text = first + ' ' + second;
    const int framesBefore = transport->writtenFrames().size();
    const QByteArray sendLine = handler.handleLine(
        QByteArrayLiteral("{\"cmd\":\"send\",\"target\":\"#omarchy\",\"text\":\"")
        + text + QByteArrayLiteral("\"}"));
    QVERIFY(OmaircIpc::responseOk(sendLine));

    const QByteArrayList frames = transport->writtenFrames().mid(framesBefore);
    QCOMPARE(frames.size(), 2);
    QCOMPARE(frames.at(0),
             QByteArrayLiteral("PRIVMSG #omarchy :") + first
                 + QByteArrayLiteral("\r\n"));
    QCOMPARE(frames.at(1),
             QByteArrayLiteral("PRIVMSG #omarchy :") + second
                 + QByteArrayLiteral("\r\n"));
    for (const QByteArray &frame : frames) {
        QVERIFY(frame.endsWith("\r\n"));
        QVERIFY(frame.size() <= int(IrcProtocol::maxClassicFrameBytes));
    }
}

void OmaircIpcTest::handlerUsesNetworkScopedErrors()
{
    IrcController controller;
    auto *transportA = new FakeIrcTransport;
    auto *transportB = new FakeIrcTransport;
    QVERIFY(controller.addSession(testConfig(QStringLiteral("net-a")), transportA));
    auto *sessionB = controller.addSession(testConfig(QStringLiteral("net-b")), transportB);
    QVERIFY(sessionB);
    registerSession(sessionB, transportB);

    OmaircIpcHandler handler(&controller);
    const QByteArray response = handler.handleLine(QByteArrayLiteral(
        "{\"cmd\":\"send\",\"network\":\"net-a\",\"target\":\"#chan\",\"text\":\"hello\"}"));
    QVERIFY(!OmaircIpc::responseOk(response));
    QCOMPARE(OmaircIpc::responseError(response), QStringLiteral("Not connected"));

    const QByteArray status = handler.handleLine(QByteArrayLiteral(
        "{\"cmd\":\"status\",\"network\":\"net-b\"}"));
    QVERIFY(OmaircIpc::responseOk(status));
    QVERIFY(!OmaircIpc::responseStatus(status)
                 .contains(QStringLiteral("lastError")));
}

void OmaircIpcTest::socketRaiseStillWorks()
{
    SingleInstance primary;
    QVERIFY(primary.acquireOrNotify());
    QSignalSpy spy(&primary, &SingleInstance::activationRequested);

    QLocalSocket client;
    client.connectToServer(SingleInstance::socketPath());
    QVERIFY(client.waitForConnected(1000));
    QCOMPARE(client.write(OmaircIpc::raisePing()), qint64(1));
    QVERIFY(client.waitForBytesWritten(1000));
    QTRY_COMPARE(spy.count(), 1);
}

void OmaircIpcTest::socketCommandRoundTrip()
{
    bool raised = false;
    OmaircIpcHandler handler(nullptr, [&raised]() { raised = true; });

    SingleInstance primary;
    QVERIFY(primary.acquireOrNotify());
    primary.setRequestHandler(
        [&handler](const QByteArray &line) { return handler.handleLine(line); });

    QLocalSocket client;
    client.connectToServer(SingleInstance::socketPath());
    QVERIFY(client.waitForConnected(1000));
    OmaircIpc::Request raiseRequest;
    raiseRequest.command = OmaircIpc::Command::Raise;
    const QByteArray request = OmaircIpc::encodeRequest(raiseRequest) + '\n';
    QCOMPARE(client.write(request), qint64(request.size()));
    QVERIFY(client.waitForBytesWritten(1000));
    // Reply may already be buffered from waitForBytesWritten's event processing.
    QTRY_VERIFY(client.canReadLine());
    const QByteArray response = client.readLine().trimmed();
    QVERIFY(OmaircIpc::responseOk(response));
    QVERIFY(raised);

    QTRY_COMPARE(client.state(), QLocalSocket::UnconnectedState);
}

void OmaircIpcTest::socketRejectsOversizedLine()
{
    SingleInstance primary;
    QVERIFY(primary.acquireOrNotify());
    primary.setRequestHandler([](const QByteArray &) {
        return OmaircIpc::okResponse();
    });

    QLocalSocket client;
    client.connectToServer(SingleInstance::socketPath());
    QVERIFY(client.waitForConnected(1000));

    QByteArray blob(65 * 1024, 'x');
    QCOMPARE(client.write(blob), qint64(blob.size()));
    QVERIFY(client.waitForBytesWritten(1000));
    QTRY_COMPARE(client.state(), QLocalSocket::UnconnectedState);
}

void OmaircIpcTest::socketRejectsOversizedResponse()
{
    SingleInstance primary;
    QVERIFY(primary.acquireOrNotify());
    primary.setRequestHandler([](const QByteArray &) {
        return QByteArray(64 * 1024, 'x');
    });

    QLocalSocket client;
    client.connectToServer(SingleInstance::socketPath());
    QVERIFY(client.waitForConnected(1000));

    const QByteArray request = QByteArrayLiteral("{\"cmd\":\"raise\"}\n");
    QCOMPARE(client.write(request), qint64(request.size()));
    QVERIFY(client.waitForBytesWritten(1000));
    QTRY_COMPARE(client.state(), QLocalSocket::UnconnectedState);
    QVERIFY(!client.canReadLine());
}

void OmaircIpcTest::socketClosesAfterOneRequest()
{
    int handled = 0;
    SingleInstance primary;
    QVERIFY(primary.acquireOrNotify());
    primary.setRequestHandler([&handled](const QByteArray &line) {
        ++handled;
        Q_UNUSED(line);
        return OmaircIpc::okResponse();
    });

    QLocalSocket client;
    client.connectToServer(SingleInstance::socketPath());
    QVERIFY(client.waitForConnected(1000));

    const QByteArray line = QByteArrayLiteral("{\"cmd\":\"raise\"}\n");
    const QByteArray payload = line + line;

    QCOMPARE(client.write(payload), qint64(payload.size()));
    QVERIFY(client.waitForBytesWritten(1000));
    QTRY_COMPARE(handled, 1);
    QTRY_VERIFY(client.canReadLine());
    QVERIFY(OmaircIpc::responseOk(client.readLine().trimmed()));
    QTRY_COMPARE(client.state(), QLocalSocket::UnconnectedState);
}

void OmaircIpcTest::connectionsSortedById()
{
    IrcController controller;
    auto *transportB = new FakeIrcTransport;
    auto *transportA = new FakeIrcTransport;
    QVERIFY(controller.addSession(testConfig(QStringLiteral("net-b")), transportB));
    QVERIFY(controller.addSession(testConfig(QStringLiteral("net-a")), transportA));
    registerSession(controller.session(QStringLiteral("net-b")), transportB);
    registerSession(controller.session(QStringLiteral("net-a")), transportA);

    OmaircIpcHandler handler(&controller);
    const QByteArray line =
        handler.handleLine(QByteArrayLiteral("{\"cmd\":\"connections\"}"));
    QVERIFY(OmaircIpc::responseOk(line));
    const QJsonArray connections = OmaircIpc::responseConnections(line);
    QCOMPARE(connections.size(), 2);
    QCOMPARE(connections.at(0).toObject().value(QStringLiteral("id")).toString(),
             QStringLiteral("net-a"));
    QCOMPARE(connections.at(1).toObject().value(QStringLiteral("id")).toString(),
             QStringLiteral("net-b"));
}

void OmaircIpcTest::sendAllowsDashPrefixedText()
{
    const auto request = OmaircCli::parseArgs(
        {QStringLiteral("send"), QStringLiteral("#chan"),
         QStringLiteral("-hello")});
    QVERIFY(std::holds_alternative<OmaircIpc::Request>(request));
    QCOMPARE(std::get<OmaircIpc::Request>(request).target,
             QStringLiteral("#chan"));
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

void OmaircIpcTest::socketRaisePingWithHandlerRaisesOnce()
{
    int raiseFnCount = 0;
    OmaircIpcHandler handler(nullptr, [&raiseFnCount]() { ++raiseFnCount; });

    SingleInstance primary;
    QVERIFY(primary.acquireOrNotify());
    primary.setRequestHandler(
        [&handler](const QByteArray &line) { return handler.handleLine(line); });
    QSignalSpy spy(&primary, &SingleInstance::activationRequested);

    QLocalSocket client;
    client.connectToServer(SingleInstance::socketPath());
    QVERIFY(client.waitForConnected(1000));
    const QByteArray ping = OmaircIpc::raisePing() + '\n';
    QCOMPARE(client.write(ping), qint64(ping.size()));
    QVERIFY(client.waitForBytesWritten(1000));
    QTRY_COMPARE(spy.count(), 1);
    QCOMPARE(raiseFnCount, 0);
    QTRY_VERIFY(client.canReadLine());
    QVERIFY(OmaircIpc::responseOk(client.readLine().trimmed()));
}

void OmaircIpcTest::socketAcceptsSplitRaisePing()
{
    SingleInstance primary;
    QVERIFY(primary.acquireOrNotify());
    QSignalSpy spy(&primary, &SingleInstance::activationRequested);

    QLocalSocket client;
    client.connectToServer(SingleInstance::socketPath());
    QVERIFY(client.waitForConnected(1000));
    QCOMPARE(client.write(QByteArrayLiteral("!")), qint64(1));
    QVERIFY(client.waitForBytesWritten(1000));
    QCOMPARE(client.write(QByteArrayLiteral("\n")), qint64(1));
    QVERIFY(client.waitForBytesWritten(1000));
    QTRY_COMPARE(spy.count(), 1);
    QTRY_COMPARE(client.state(), QLocalSocket::UnconnectedState);
}

void OmaircIpcTest::socketAcceptsDisconnectedRaisePing()
{
    SingleInstance primary;
    QVERIFY(primary.acquireOrNotify());
    QSignalSpy spy(&primary, &SingleInstance::activationRequested);

    QLocalSocket client;
    client.connectToServer(SingleInstance::socketPath());
    QVERIFY(client.waitForConnected(1000));
    QCOMPARE(client.write(OmaircIpc::raisePing()), qint64(1));
    QVERIFY(client.waitForBytesWritten(1000));
    client.disconnectFromServer();
    QTRY_COMPARE(spy.count(), 1);
}

void OmaircIpcTest::socketExpiresIdleClient()
{
    SingleInstance primary;
    QVERIFY(primary.acquireOrNotify());

    QLocalSocket client;
    client.connectToServer(SingleInstance::socketPath());
    QVERIFY(client.waitForConnected(1000));
    QTRY_COMPARE(client.state(), QLocalSocket::UnconnectedState);
}

void OmaircIpcTest::socketLimitsConcurrentClients()
{
    SingleInstance primary;
    QVERIFY(primary.acquireOrNotify());

    QVector<QLocalSocket *> clients;
    for (int i = 0; i < 32; ++i) {
        auto *client = new QLocalSocket;
        client->connectToServer(SingleInstance::socketPath());
        QVERIFY(client->waitForConnected(1000));
        clients.append(client);
    }

    for (QLocalSocket *client : clients)
        QCOMPARE(client->state(), QLocalSocket::ConnectedState);

    auto *rejected = new QLocalSocket;
    rejected->connectToServer(SingleInstance::socketPath());
    const bool connected = rejected->waitForConnected(1000);
    Q_UNUSED(connected);
    QTRY_COMPARE(rejected->state(), QLocalSocket::UnconnectedState);
    for (QLocalSocket *client : clients)
        QCOMPARE(client->state(), QLocalSocket::ConnectedState);
    clients.append(rejected);

for (QLocalSocket *client : clients) {
        client->disconnectFromServer();
        delete client;
    }
}

void OmaircIpcTest::parseReadWindows()
{
    const auto def =
        OmaircIpc::parseRequest(QByteArrayLiteral("{\"cmd\":\"read\"}"));
    QVERIFY(def.has_value());
    QCOMPARE(def->command, OmaircIpc::Command::Read);
    QVERIFY(std::holds_alternative<OmaircIpc::LastWindow>(def->window));
    QCOMPARE(std::get<OmaircIpc::LastWindow>(def->window).count, 50);

    const auto last = OmaircIpc::parseRequest(
        QByteArrayLiteral("{\"cmd\":\"read\",\"last\":20,\"target\":\"#c\"}"));
    QVERIFY(last.has_value());
    QCOMPARE(std::get<OmaircIpc::LastWindow>(last->window).count, 20);
    QCOMPARE(last->target, QStringLiteral("#c"));

    OmaircIpc::ParseError error;
    QVERIFY(!OmaircIpc::parseRequest(
                 QByteArrayLiteral("{\"cmd\":\"read\",\"last\":101}"), &error)
                 .has_value());
    QVERIFY(error.message.contains(QStringLiteral("100")));

    QVERIFY(!OmaircIpc::parseRequest(
                 QByteArrayLiteral("{\"cmd\":\"read\",\"last\":20,\"unread\":true}"),
                 &error)
                 .has_value());

    const auto unread = OmaircIpc::parseRequest(
        QByteArrayLiteral("{\"cmd\":\"read\",\"unread\":true}"));
    QVERIFY(unread.has_value());
    QVERIFY(std::holds_alternative<OmaircIpc::UnreadWindow>(unread->window));

    QCOMPARE(OmaircIpc::durationMs(QStringLiteral("1s")), qint64(1000));
    QVERIFY(!OmaircIpc::durationMs(QStringLiteral("9999999999999999s")));
    QVERIFY(!OmaircIpc::durationMs(QStringLiteral("9223372036854775807d")));
}

void OmaircIpcTest::handlerReadNamesConversations()
{
    QTemporaryDir cursorDir;
    QVERIFY(cursorDir.isValid());
    const ScopedCursorRoot cursorRoot(cursorDir.path());

    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session =
        controller.addSession(testConfig(QStringLiteral("net-1")), transport);
    QVERIFY(session);
    registerSession(session, transport);
    seedChannelAndDirect(transport);

    const QString selectedBefore = controller.selectedTarget();
    const int unreadBefore = controller.unreadCountFor(QStringLiteral("net-1"));

    OmaircIpcHandler handler(&controller);
    const QByteArray lastTwo = handler.handleLine(
        QByteArrayLiteral("{\"cmd\":\"read\",\"target\":\"#omarchy\",\"last\":2}"));
    QVERIFY(OmaircIpc::responseOk(lastTwo));
    const QJsonArray lastMessages = OmaircIpc::responseMessages(lastTwo);
    QCOMPARE(lastMessages.size(), 2);
    QCOMPARE(lastMessages.at(0).toObject().value(QStringLiteral("message")).toString(),
             QStringLiteral("newest"));
    QCOMPARE(lastMessages.at(1).toObject().value(QStringLiteral("kind")).toString(),
             QStringLiteral("action"));
    QCOMPARE(lastMessages.at(1).toObject().value(QStringLiteral("message")).toString(),
             QStringLiteral("waves"));
    for (const QJsonValue &value : lastMessages) {
        const QString kind = value.toObject().value(QStringLiteral("kind")).toString();
        QVERIFY(kind == QLatin1String("message")
                || kind == QLatin1String("notice")
                || kind == QLatin1String("action"));
        QVERIFY(value.toObject().value(QStringLiteral("message")).toString()
                != QStringLiteral("omairc joined"));
    }

    const QByteArray untargeted = handler.handleLine(
        QByteArrayLiteral("{\"cmd\":\"read\",\"last\":50}"));
    QVERIFY(OmaircIpc::responseOk(untargeted));
    QStringList targets;
    for (const QJsonValue &value : OmaircIpc::responseMessages(untargeted))
        targets.append(value.toObject().value(QStringLiteral("target")).toString());
    QVERIFY(targets.contains(QStringLiteral("#omarchy")));
    QVERIFY(targets.contains(QStringLiteral("alice")));

    const QByteArray since = handler.handleLine(QByteArrayLiteral(
        "{\"cmd\":\"read\",\"target\":\"#omarchy\",\"since\":\"2011-10-19T16:40:51.620Z\"}"));
    QVERIFY(OmaircIpc::responseOk(since));
    QVERIFY(OmaircIpc::responseMessages(since).size() >= 1);
    QCOMPARE(OmaircIpc::responseMessages(since).at(0).toObject()
                 .value(QStringLiteral("msgid")).toString(),
             QStringLiteral("old"));

    const QByteArray missing = handler.handleLine(
        QByteArrayLiteral("{\"cmd\":\"read\",\"target\":\"#nope\"}"));
    QVERIFY(!OmaircIpc::responseOk(missing));
    QVERIFY(OmaircIpc::responseError(missing).contains(QStringLiteral("#nope")));

    const QByteArray names = handler.handleLine(
        QByteArrayLiteral("{\"cmd\":\"names\",\"target\":\"#omarchy\"}"));
    QVERIFY(OmaircIpc::responseOk(names));
    QStringList nicks;
    for (const QJsonValue &value : OmaircIpc::responseMembers(names)) {
        const QJsonObject row = value.toObject();
        nicks.append(row.value(QStringLiteral("nick")).toString());
        QVERIFY(row.contains(QStringLiteral("label")));
        QVERIFY(!row.contains(QStringLiteral("away")));
        QVERIFY(!row.contains(QStringLiteral("status")));
    }
    QVERIFY(nicks.contains(QStringLiteral("omairc")));
    QVERIFY(nicks.contains(QStringLiteral("alice")));

    // Ranks order the snapshot, and the label carries the highest one. This
    // session never saw a 005, so the ladder is the default `~&@%+`.
    transport->injectBytes(
        QByteArrayLiteral(":server 353 omairc = #omarchy :+omairc @alice\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));
    const QByteArray ranked = handler.handleLine(
        QByteArrayLiteral("{\"cmd\":\"names\",\"target\":\"#omarchy\"}"));
    QVERIFY(OmaircIpc::responseOk(ranked));
    QStringList rankedNicks;
    QStringList rankedLabels;
    for (const QJsonValue &value : OmaircIpc::responseMembers(ranked)) {
        const QJsonObject row = value.toObject();
        rankedNicks.append(row.value(QStringLiteral("nick")).toString());
        rankedLabels.append(row.value(QStringLiteral("label")).toString());
    }
    QCOMPARE(rankedNicks,
             QStringList({QStringLiteral("alice"), QStringLiteral("omairc")}));
    QCOMPARE(rankedLabels,
             QStringList({QStringLiteral("@alice"), QStringLiteral("+omairc")}));

    const QByteArray namesDm = handler.handleLine(
        QByteArrayLiteral("{\"cmd\":\"names\",\"target\":\"alice\"}"));
    QVERIFY(!OmaircIpc::responseOk(namesDm));
    const QByteArray namesMissing = handler.handleLine(
        QByteArrayLiteral("{\"cmd\":\"names\",\"target\":\"#nope\"}"));
    QVERIFY(!OmaircIpc::responseOk(namesMissing));
    const QByteArray namesStatus = handler.handleLine(
        QByteArrayLiteral("{\"cmd\":\"names\",\"target\":\"Status\"}"));
    QVERIFY(!OmaircIpc::responseOk(namesStatus));

    const QByteArray conversations = handler.handleLine(
        QByteArrayLiteral("{\"cmd\":\"conversations\"}"));
    QVERIFY(OmaircIpc::responseOk(conversations));
    QJsonObject channelRow;
    QJsonObject dmRow;
    for (const QJsonValue &value : OmaircIpc::responseConversations(conversations)) {
        const QJsonObject row = value.toObject();
        if (row.value(QStringLiteral("target")).toString() == QLatin1String("#omarchy"))
            channelRow = row;
        if (row.value(QStringLiteral("target")).toString() == QLatin1String("alice"))
            dmRow = row;
    }
    QCOMPARE(channelRow.value(QStringLiteral("channel")).toBool(), true);
    QCOMPARE(channelRow.value(QStringLiteral("topic")).toString(),
             QStringLiteral("A cozy corner"));
    QVERIFY(!channelRow.contains(QStringLiteral("mention")));
    QVERIFY(!dmRow.contains(QStringLiteral("channel")));
    QVERIFY(!dmRow.contains(QStringLiteral("topic")));
    QVERIFY(dmRow.value(QStringLiteral("unread")).toInt() >= 1);

    QStringList conversationTargets;
    for (const QJsonValue &value : OmaircIpc::responseConversations(conversations))
        conversationTargets.append(
            value.toObject().value(QStringLiteral("target")).toString());
    QVERIFY(!conversationTargets.contains(QStringLiteral("#nope")));

    QCOMPARE(controller.selectedTarget(), selectedBefore);
    QCOMPARE(controller.unreadCountFor(QStringLiteral("net-1")), unreadBefore);

    const QByteArray firstUnread = handler.handleLine(
        QByteArrayLiteral("{\"cmd\":\"read\",\"target\":\"#omarchy\",\"unread\":true}"));
    QVERIFY(OmaircIpc::responseOk(firstUnread));
    QVERIFY(OmaircIpc::responseMessages(firstUnread).size() >= 1);
    const QString cursorPath =
        QDir(cursorDir.path()).filePath(QStringLiteral("net-1/#omarchy.json"));
    QVERIFY(QFileInfo::exists(cursorPath));
    const QFileDevice::Permissions bits = QFileInfo(cursorPath).permissions();
    QVERIFY(bits & QFileDevice::ReadOwner);
    QVERIFY(bits & QFileDevice::WriteOwner);
    QVERIFY(!(bits & QFileDevice::ReadGroup));
    QVERIFY(!(bits & QFileDevice::ReadOther));
    QFile cursorFile(cursorPath);
    QVERIFY(cursorFile.open(QIODevice::ReadOnly));
    const QByteArray firstCursor = cursorFile.readAll();
    cursorFile.close();

    const QByteArray secondUnread = handler.handleLine(
        QByteArrayLiteral("{\"cmd\":\"read\",\"target\":\"#omarchy\",\"unread\":true}"));
    QVERIFY(OmaircIpc::responseOk(secondUnread));
    QCOMPARE(OmaircIpc::responseMessages(secondUnread).size(), 0);
    QVERIFY(cursorFile.open(QIODevice::ReadOnly));
    QCOMPARE(cursorFile.readAll(), firstCursor);
    cursorFile.close();

    transport->injectBytes(
        QByteArrayLiteral("@time=2011-10-19T16:45:00.000Z;msgid=later :alice!u@h PRIVMSG #omarchy :after cursor\r\n"));
    const QByteArray thirdUnread = handler.handleLine(
        QByteArrayLiteral("{\"cmd\":\"read\",\"target\":\"#omarchy\",\"unread\":true}"));
    QVERIFY(OmaircIpc::responseOk(thirdUnread));
    QCOMPARE(OmaircIpc::responseMessages(thirdUnread).size(), 1);
    QCOMPARE(OmaircIpc::responseMessages(thirdUnread).at(0).toObject()
                 .value(QStringLiteral("msgid")).toString(),
             QStringLiteral("later"));
    QCOMPARE(OmaircIpc::responseMessages(thirdUnread).at(0).toObject()
                 .value(QStringLiteral("message")).toString(),
             QStringLiteral("after cursor"));

    QCOMPARE(controller.selectedTarget(), selectedBefore);
}

void OmaircIpcTest::handlerUnreadDoesNotChmodCursorRootParent()
{
    QTemporaryDir parentDir;
    QVERIFY(parentDir.isValid());
    const QFileDevice::Permissions openParent =
        QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ExeOwner
        | QFileDevice::ReadGroup | QFileDevice::ExeGroup
        | QFileDevice::ReadOther | QFileDevice::ExeOther;
    QVERIFY(QFile::setPermissions(parentDir.path(), openParent));
    const QString cursorRoot =
        QDir(parentDir.path()).filePath(QStringLiteral("cursors"));
    QVERIFY(QDir().mkpath(cursorRoot));
    const ScopedCursorRoot scoped(cursorRoot);

    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session =
        controller.addSession(testConfig(QStringLiteral("net-1")), transport);
    QVERIFY(session);
    registerSession(session, transport);
    seedChannelAndDirect(transport);

    OmaircIpcHandler handler(&controller);
    const QByteArray unread =
        handler.handleLine(QByteArrayLiteral("{\"cmd\":\"read\",\"unread\":true}"));
    QVERIFY(OmaircIpc::responseOk(unread));
    QVERIFY(OmaircIpc::responseMessages(unread).size() >= 1);
    QVERIFY(QFileInfo::exists(
        QDir(cursorRoot).filePath(QStringLiteral("net-1.json"))));

    const QFileDevice::Permissions parentBits =
        QFileInfo(parentDir.path()).permissions();
    QVERIFY(parentBits & QFileDevice::ReadGroup);
    QVERIFY(parentBits & QFileDevice::ReadOther);
}

void OmaircIpcTest::handlerReadReportsTruncated()
{
    QTemporaryDir cursorDir;
    QVERIFY(cursorDir.isValid());
    const ScopedCursorRoot cursorRoot(cursorDir.path());

    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session =
        controller.addSession(testConfig(QStringLiteral("net-1")), transport);
    QVERIFY(session);
    registerSession(session, transport);
    transport->injectBytes(QByteArrayLiteral(
        ":omairc!u@h JOIN :#omarchy\r\n"
        "@time=2011-10-19T16:59:59.000Z :alice!u@h PRIVMSG #omarchy :seed\r\n"));

    OmaircIpcHandler handler(&controller);
    const QByteArray lastTwo = handler.handleLine(
        QByteArrayLiteral("{\"cmd\":\"read\",\"target\":\"#omarchy\",\"last\":2}"));
    QVERIFY(OmaircIpc::responseOk(lastTwo));
    QVERIFY(!OmaircIpc::responseTruncated(lastTwo));

    const QByteArray firstUnread = handler.handleLine(
        QByteArrayLiteral("{\"cmd\":\"read\",\"target\":\"#omarchy\",\"unread\":true}"));
    QVERIFY(OmaircIpc::responseOk(firstUnread));
    QVERIFY(!OmaircIpc::responseTruncated(firstUnread));

    QByteArray flood;
    const QDateTime start = QDateTime::fromString(
        QStringLiteral("2011-10-19T17:00:00.000Z"), Qt::ISODateWithMs);
    for (int i = 0; i < 101; ++i) {
        const QString when = start.addSecs(i).toUTC().toString(Qt::ISODateWithMs);
        flood += QStringLiteral(
                     "@time=%1 :alice!u@h PRIVMSG #omarchy :n%2\r\n")
                     .arg(when)
                     .arg(i)
                     .toUtf8();
    }
    transport->injectBytes(flood);

    const QByteArray since = handler.handleLine(QByteArrayLiteral(
        "{\"cmd\":\"read\",\"target\":\"#omarchy\","
        "\"since\":\"2011-10-19T17:00:00.000Z\"}"));
    QVERIFY(OmaircIpc::responseOk(since));
    QCOMPARE(OmaircIpc::responseMessages(since).size(), 100);
    QVERIFY(OmaircIpc::responseTruncated(since));
    QCOMPARE(OmaircIpc::responseMessages(since).at(0).toObject()
                 .value(QStringLiteral("message")).toString(),
             QStringLiteral("n1"));
    QCOMPARE(OmaircIpc::responseMessages(since).last().toObject()
                 .value(QStringLiteral("message")).toString(),
             QStringLiteral("n100"));

    const QByteArray unread = handler.handleLine(
        QByteArrayLiteral("{\"cmd\":\"read\",\"target\":\"#omarchy\",\"unread\":true}"));
    QVERIFY(OmaircIpc::responseOk(unread));
    QCOMPARE(OmaircIpc::responseMessages(unread).size(), 100);
    QVERIFY(OmaircIpc::responseTruncated(unread));
    QCOMPARE(OmaircIpc::responseMessages(unread).at(0).toObject()
                 .value(QStringLiteral("message")).toString(),
             QStringLiteral("n1"));
    QCOMPARE(OmaircIpc::responseMessages(unread).last().toObject()
                 .value(QStringLiteral("message")).toString(),
             QStringLiteral("n100"));
}

void OmaircIpcTest::uncertainResponseShape()
{
    const QString message = QStringLiteral(
        "No response from Omairc after send; the message may already have been delivered");
    const QByteArray line = OmaircIpc::uncertainResponse(message);
    QVERIFY(!OmaircIpc::responseOk(line));
    QVERIFY(OmaircIpc::responseUncertain(line));
    QCOMPARE(OmaircIpc::responseError(line), message);

    const QJsonObject object =
        QJsonDocument::fromJson(line).object();
    QCOMPARE(object.value(QStringLiteral("ok")).toBool(), false);
    QCOMPARE(object.value(QStringLiteral("uncertain")).toBool(), true);
    QCOMPARE(object.value(QStringLiteral("error")).toString(), message);

    const QByteArray plainError =
        OmaircIpc::errorResponse(QStringLiteral("Not connected"));
    QVERIFY(!OmaircIpc::responseOk(plainError));
    QVERIFY(!OmaircIpc::responseUncertain(plainError));
    QVERIFY(OmaircIpc::responseHasOk(line));
    QVERIFY(OmaircIpc::responseHasOk(plainError));
    QVERIFY(OmaircIpc::responseHasOk(QByteArrayLiteral("{\"ok\":true}")));
    QVERIFY(OmaircIpc::responseHasOk(QByteArrayLiteral("{\"ok\":false}")));
    // sendReplyUncertain treats empty/missing replies and !responseHasOk as
    // uncertain, so a non-boolean "ok" must not look like a readable envelope.
    QVERIFY(!OmaircIpc::responseHasOk(QByteArrayLiteral("not json")));
    QVERIFY(!OmaircIpc::responseHasOk(QByteArrayLiteral("{\"error\":\"x\"}")));
    QVERIFY(!OmaircIpc::responseHasOk(QByteArrayLiteral("{\"ok\":\"true\"}")));
    QVERIFY(!OmaircIpc::responseHasOk(QByteArrayLiteral("{\"ok\":1}")));
    QVERIFY(!OmaircIpc::responseHasOk(QByteArrayLiteral("{\"ok\":null}")));
}

void OmaircIpcTest::cliSendUncertainWhenReplyDropped()
{
    if (omaircBinary().isEmpty())
        QSKIP("omairc binary missing");

    const auto send = runOmaircCliAgainstMute(
        {QStringLiteral("send"), QStringLiteral("#chan"),
         QStringLiteral("hello")});
    QCOMPARE(send.exitCode, 2);
    const QByteArray sendJson = lastJsonLine(send.stdoutBytes);
    QVERIFY(OmaircIpc::responseUncertain(sendJson));
    QVERIFY(!OmaircIpc::responseOk(sendJson));
    QCOMPARE(OmaircIpc::responseError(sendJson),
             QStringLiteral(
                 "No response from Omairc after send; the message may already have been delivered"));
    QVERIFY(QString::fromUtf8(send.stderrBytes)
                .contains(OmaircIpc::responseError(sendJson)));

    const auto raise = runOmaircCliAgainstMute({QStringLiteral("raise")});
    QCOMPARE(raise.exitCode, 1);
    const QByteArray raiseJson = lastJsonLine(raise.stdoutBytes);
    QVERIFY(!OmaircIpc::responseUncertain(raiseJson));
    QCOMPARE(OmaircIpc::responseError(raiseJson),
             QStringLiteral("Invalid or missing response from Omairc"));
}

void OmaircIpcTest::cliSendNotUncertainWhenClientMissing()
{
    if (omaircBinary().isEmpty())
        QSKIP("omairc binary missing");

    QLocalServer::removeServer(SingleInstance::socketPath());
    const auto result = runOmaircCli(
        {QStringLiteral("send"), QStringLiteral("#chan"),
         QStringLiteral("hello")});
    QCOMPARE(result.exitCode, 1);
    const QByteArray json = lastJsonLine(result.stdoutBytes);
    QVERIFY(!OmaircIpc::responseUncertain(json));
    QVERIFY(OmaircIpc::responseError(json).contains(
        QStringLiteral("Omairc is not running")));
}

void OmaircIpcTest::handlerUnreadKeepsSameMillisecondLines()
{
    QTemporaryDir cursorDir;
    QVERIFY(cursorDir.isValid());
    const ScopedCursorRoot cursorRoot(cursorDir.path());

    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session =
        controller.addSession(testConfig(QStringLiteral("net-1")), transport);
    QVERIFY(session);
    registerSession(session, transport);
    transport->injectBytes(QByteArrayLiteral(
        ":omairc!u@h JOIN :#omarchy\r\n"
        "@time=2011-10-19T17:00:00.000Z :alice!u@h PRIVMSG #omarchy :first\r\n"
        "@time=2011-10-19T17:00:00.000Z :alice!u@h PRIVMSG #omarchy :second\r\n"));

    OmaircIpcHandler handler(&controller);
    const QByteArray firstUnread = handler.handleLine(
        QByteArrayLiteral("{\"cmd\":\"read\",\"target\":\"#omarchy\",\"unread\":true}"));
    QVERIFY(OmaircIpc::responseOk(firstUnread));
    const QJsonArray firstMessages = OmaircIpc::responseMessages(firstUnread);
    QCOMPARE(firstMessages.size(), 2);
    QCOMPARE(firstMessages.at(0).toObject().value(QStringLiteral("message")).toString(),
             QStringLiteral("first"));
    QCOMPARE(firstMessages.at(1).toObject().value(QStringLiteral("message")).toString(),
             QStringLiteral("second"));
    QVERIFY(!firstMessages.at(0).toObject().contains(QStringLiteral("msgid")));

    const QString cursorPath =
        QDir(cursorDir.path()).filePath(QStringLiteral("net-1/#omarchy.json"));
    QFile cursorFile(cursorPath);
    QVERIFY(cursorFile.open(QIODevice::ReadOnly));
    const QJsonObject cursor =
        QJsonDocument::fromJson(cursorFile.readAll()).object();
    cursorFile.close();
    QVERIFY(cursor.contains(QStringLiteral("sequence")));
    QVERIFY(!cursor.contains(QStringLiteral("msgid")));

    const QByteArray secondUnread = handler.handleLine(
        QByteArrayLiteral("{\"cmd\":\"read\",\"target\":\"#omarchy\",\"unread\":true}"));
    QVERIFY(OmaircIpc::responseOk(secondUnread));
    QCOMPARE(OmaircIpc::responseMessages(secondUnread).size(), 0);

    transport->injectBytes(QByteArrayLiteral(
        "@time=2011-10-19T17:00:00.000Z :alice!u@h PRIVMSG #omarchy :third\r\n"));
    const QByteArray thirdUnread = handler.handleLine(
        QByteArrayLiteral("{\"cmd\":\"read\",\"target\":\"#omarchy\",\"unread\":true}"));
    QVERIFY(OmaircIpc::responseOk(thirdUnread));
    QCOMPARE(OmaircIpc::responseMessages(thirdUnread).size(), 1);
    QCOMPARE(OmaircIpc::responseMessages(thirdUnread).at(0).toObject()
                 .value(QStringLiteral("message")).toString(),
             QStringLiteral("third"));

    QFile oldCursor(cursorPath);
    QVERIFY(oldCursor.open(QIODevice::WriteOnly | QIODevice::Truncate));
    oldCursor.write(
        QByteArrayLiteral("{\"timestamp\":\"2011-10-19T17:00:00.000Z\"}\n"));
    oldCursor.close();
    transport->injectBytes(QByteArrayLiteral(
        "@time=2011-10-19T17:00:00.000Z :alice!u@h PRIVMSG #omarchy :legacy\r\n"));
    const QByteArray legacyUnread = handler.handleLine(
        QByteArrayLiteral("{\"cmd\":\"read\",\"target\":\"#omarchy\",\"unread\":true}"));
    QVERIFY(OmaircIpc::responseOk(legacyUnread));
    QVERIFY(OmaircIpc::responseMessages(legacyUnread).size() >= 1);
    QCOMPARE(OmaircIpc::responseMessages(legacyUnread).last().toObject()
                 .value(QStringLiteral("message")).toString(),
             QStringLiteral("legacy"));
}

int runOmaircIpcTests(int argc, char **argv)
{
    OmaircIpcTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_omaircipc.moc"
