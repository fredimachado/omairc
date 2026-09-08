#include <QJsonArray>
#include <QJsonObject>
#include <QLocalSocket>
#include <QSignalSpy>
#include <QTest>

#include "fakeirctransport.h"
#include "irccontroller.h"
#include "omairccli.h"
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

QByteArray framesJoin(const QByteArrayList &frames)
{
    QByteArray joined;
    for (const QByteArray &frame : frames)
        joined += frame;
    return joined;
}

}

class OmaircIpcTest : public QObject
{
    Q_OBJECT

private slots:
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
};

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
    info.host = QStringLiteral("irc.example");
    info.port = 6697;
    info.tls = true;
    info.nick = QStringLiteral("omairc");
    info.state = QStringLiteral("Connected");
    info.selected = true;
    infos.append(info);

    const QByteArray line = OmaircIpc::okConnections(infos);
    QVERIFY(OmaircIpc::responseOk(line));
    const QJsonArray connections = OmaircIpc::responseConnections(line);
    QCOMPARE(connections.size(), 1);
    const QJsonObject row = connections.at(0).toObject();
    QCOMPARE(row.value(QStringLiteral("id")).toString(), QStringLiteral("nid"));
    QCOMPARE(row.value(QStringLiteral("host")).toString(),
             QStringLiteral("irc.example"));
    QCOMPARE(row.value(QStringLiteral("port")).toInt(), 6697);
    QCOMPARE(row.value(QStringLiteral("tls")).toBool(), true);
    QCOMPARE(row.value(QStringLiteral("nick")).toString(),
             QStringLiteral("omairc"));
    QCOMPARE(row.value(QStringLiteral("state")).toString(),
             QStringLiteral("Connected"));
    QCOMPARE(row.value(QStringLiteral("selected")).toBool(), true);
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

    const int framesBefore = transport->writtenFrames().size();
    const QByteArray sendLine = handler.handleLine(QByteArrayLiteral(
        "{\"cmd\":\"send\",\"target\":\"#omarchy\",\"text\":\"hello agents\"}"));
    QVERIFY(OmaircIpc::responseOk(sendLine));
    QVERIFY(framesJoin(transport->writtenFrames().mid(framesBefore))
                .contains(QByteArrayLiteral("PRIVMSG #omarchy :hello agents")));
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
    QString error;
    const auto request = OmaircCli::parseArgs(
        QStringList{QStringLiteral("send"), QStringLiteral("#chan"),
                    QStringLiteral("-hello")},
        error);
    QVERIFY(request.has_value());
    QCOMPARE(request->target, QStringLiteral("#chan"));
    QCOMPARE(request->text, QStringLiteral("-hello"));

    const auto withDashDash = OmaircCli::parseArgs(
        QStringList{QStringLiteral("send"), QStringLiteral("--"),
                    QStringLiteral("#chan"), QStringLiteral("--network"),
                    QStringLiteral("not-an-option")},
        error);
    QVERIFY(withDashDash.has_value());
    QCOMPARE(withDashDash->target, QStringLiteral("#chan"));
    QCOMPARE(withDashDash->text, QStringLiteral("--network not-an-option"));
    QVERIFY(withDashDash->networkId.isEmpty());
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

int runOmaircIpcTests(int argc, char **argv)
{
    OmaircIpcTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_omaircipc.moc"
