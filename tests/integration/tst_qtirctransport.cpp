#include <QFile>
#include <QHostAddress>
#include <QSignalSpy>
#include <QSslCertificate>
#include <QSslConfiguration>
#include <QSslKey>
#include <QSslServer>
#include <QTcpServer>
#include <QTcpSocket>
#include <QTest>

#include "fakeirctransport.h"
#include "ircsession.h"
#include "qtirctransport.h"

namespace
{
constexpr int SignalTimeout = 3000;
QStringList *capturedMessages = nullptr;

void captureMessage(QtMsgType, const QMessageLogContext &, const QString &message)
{
    if (capturedMessages)
        capturedMessages->append(message);
}

QSslCertificate testCertificate()
{
    QFile file(QStringLiteral(TEST_CERT_DIR "/localhost-cert.pem"));
    if (!file.open(QIODevice::ReadOnly))
        return QSslCertificate();
    return QSslCertificate(file.readAll(), QSsl::Pem);
}

QSslKey testPrivateKey()
{
    QFile file(QStringLiteral(TEST_CERT_DIR "/localhost-key.pem"));
    if (!file.open(QIODevice::ReadOnly))
        return {};
    return QSslKey(file.readAll(), QSsl::Rsa, QSsl::Pem);
}

IrcSessionConfig sessionConfig(const QString &password)
{
    IrcSessionConfig config;
    config.networkId = QStringLiteral("log-test");
    config.host = QStringLiteral("irc.invalid");
    config.port = 6697;
    config.tlsEnabled = true;
    config.nick = QStringLiteral("omairc");
    config.username = QStringLiteral("omairc");
    config.realname = QStringLiteral("Omairc User");
    config.password = password;
    config.reconnectEnabled = false;
    return config;
}
}

class QtIrcTransportIntegrationTest : public QObject
{
    Q_OBJECT

private slots:
    void plainTcpExchangesBytesAndDisconnects();
    void tlsExchangesBytesAndDisconnects();
    void untrustedCertificateFailsWithUsefulError();
    void shutdownDuringConnectIsSafe();
    void sessionLogsDoNotExposeSecrets();
};

void QtIrcTransportIntegrationTest::plainTcpExchangesBytesAndDisconnects()
{
    QTcpServer server;
    QVERIFY(server.listen(QHostAddress::LocalHost, 0));
    QSignalSpy serverConnected(&server, &QTcpServer::newConnection);

    QtIrcTransport transport;
    QSignalSpy connected(&transport, &IrcTransport::connected);
    QSignalSpy received(&transport, &IrcTransport::bytesReceived);
    QSignalSpy disconnected(&transport, &IrcTransport::disconnected);
    const QByteArray outbound("PING :plain\r\n");
    transport.connectToHost(QStringLiteral("127.0.0.1"), server.serverPort(), false);
    transport.write(outbound);

    QVERIFY(!serverConnected.isEmpty() || serverConnected.wait(SignalTimeout));
    QTcpSocket *peer = server.nextPendingConnection();
    QVERIFY(peer);
    QVERIFY(!connected.isEmpty() || connected.wait(SignalTimeout));
    QSignalSpy peerReady(peer, &QTcpSocket::readyRead);
    QVERIFY(peer->bytesAvailable() > 0 || peerReady.wait(SignalTimeout));
    QCOMPARE(peer->readAll(), outbound);

    const QByteArray greeting(":loopback 001 omairc :Welcome\r\n");
    QCOMPARE(peer->write(greeting), qint64(greeting.size()));
    QVERIFY(!received.isEmpty() || received.wait(SignalTimeout));
    QCOMPARE(received.last().at(0).toByteArray(), greeting);

    transport.shutdown();
    QVERIFY(!disconnected.isEmpty() || disconnected.wait(SignalTimeout));
    QCOMPARE(transport.connectionState(), IrcTransport::ConnectionState::Disconnected);
}

void QtIrcTransportIntegrationTest::tlsExchangesBytesAndDisconnects()
{
    const QSslCertificate certificate = testCertificate();
    const QSslKey privateKey = testPrivateKey();
    QVERIFY(!certificate.isNull());
    QVERIFY(!privateKey.isNull());

    QSslConfiguration serverConfiguration = QSslConfiguration::defaultConfiguration();
    serverConfiguration.setLocalCertificate(certificate);
    serverConfiguration.setPrivateKey(privateKey);
    QSslServer server;
    server.setSslConfiguration(serverConfiguration);
    QVERIFY(server.listen(QHostAddress::LocalHost, 0));
    QSignalSpy serverPending(&server, SIGNAL(pendingConnectionAvailable()));

    QSslConfiguration clientConfiguration = QSslConfiguration::defaultConfiguration();
    QList<QSslCertificate> authorities = clientConfiguration.caCertificates();
    authorities.append(certificate);
    clientConfiguration.setCaCertificates(authorities);
    QtIrcTransport transport(clientConfiguration);
    QSignalSpy encrypted(&transport, &IrcTransport::encrypted);
    QSignalSpy received(&transport, &IrcTransport::bytesReceived);
    QSignalSpy disconnected(&transport, &IrcTransport::disconnected);
    const QByteArray outbound("PING :tls\r\n");
    transport.connectToHost(QStringLiteral("127.0.0.1"), server.serverPort(), true);
    transport.write(outbound);

    QVERIFY(!encrypted.isEmpty() || encrypted.wait(SignalTimeout));
    QVERIFY(server.hasPendingConnections()
            || !serverPending.isEmpty()
            || serverPending.wait(SignalTimeout));
    QTcpSocket *peer = server.nextPendingConnection();
    QVERIFY(peer);
    QSignalSpy peerReady(peer, &QTcpSocket::readyRead);
    QVERIFY(peer->bytesAvailable() > 0 || peerReady.wait(SignalTimeout));
    QCOMPARE(peer->readAll(), outbound);

    const QByteArray greeting(":loopback 001 omairc :Welcome over TLS\r\n");
    QCOMPARE(peer->write(greeting), qint64(greeting.size()));
    QVERIFY(!received.isEmpty() || received.wait(SignalTimeout));
    QCOMPARE(received.last().at(0).toByteArray(), greeting);

    transport.shutdown();
    QVERIFY(!disconnected.isEmpty() || disconnected.wait(SignalTimeout));
    QCOMPARE(transport.connectionState(), IrcTransport::ConnectionState::Disconnected);
}

void QtIrcTransportIntegrationTest::untrustedCertificateFailsWithUsefulError()
{
    const QSslCertificate certificate = testCertificate();
    const QSslKey privateKey = testPrivateKey();
    QVERIFY(!certificate.isNull());
    QVERIFY(!privateKey.isNull());

    QSslConfiguration serverConfiguration = QSslConfiguration::defaultConfiguration();
    serverConfiguration.setLocalCertificate(certificate);
    serverConfiguration.setPrivateKey(privateKey);
    QSslServer server;
    server.setSslConfiguration(serverConfiguration);
    QVERIFY(server.listen(QHostAddress::LocalHost, 0));

    QtIrcTransport transport;
    QSignalSpy errors(&transport, &IrcTransport::errorOccurred);
    QSignalSpy encrypted(&transport, &IrcTransport::encrypted);
    transport.connectToHost(QStringLiteral("127.0.0.1"), server.serverPort(), true);

    QVERIFY(!errors.isEmpty() || errors.wait(SignalTimeout));
    QCOMPARE(encrypted.size(), 0);
    QCOMPARE(transport.connectionState(), IrcTransport::ConnectionState::Failed);
    const QString message = errors.last().at(0).toString();
    QVERIFY2(message.contains(QStringLiteral("TLS certificate error")),
             qPrintable(message));
    QVERIFY2(message.contains(QStringLiteral("self-signed"), Qt::CaseInsensitive)
                 || message.contains(QStringLiteral("trusted"), Qt::CaseInsensitive),
             qPrintable(message));
}

void QtIrcTransportIntegrationTest::shutdownDuringConnectIsSafe()
{
    QTcpServer server;
    QVERIFY(server.listen(QHostAddress::LocalHost, 0));

    QtIrcTransport transport;
    QSignalSpy disconnected(&transport, &IrcTransport::disconnected);
    transport.connectToHost(QStringLiteral("127.0.0.1"), server.serverPort(), false);
    transport.shutdown();

    QVERIFY(!disconnected.isEmpty() || disconnected.wait(SignalTimeout));
    QCOMPARE(disconnected.size(), 1);
    QCOMPARE(transport.connectionState(), IrcTransport::ConnectionState::Disconnected);
}

void QtIrcTransportIntegrationTest::sessionLogsDoNotExposeSecrets()
{
    const QString password = QStringLiteral("pass-secret-7f93");
    const QString privateBody = QStringLiteral("private-body-28c1");
    QStringList messages;
    capturedMessages = &messages;
    const QtMessageHandler previousHandler = qInstallMessageHandler(captureMessage);
    bool sentPass = false;
    bool sentPrivmsg = false;

    {
        auto *transport = new FakeIrcTransport;
        IrcSession session(sessionConfig(password), transport);
        session.start();
        transport->completeConnect();
        transport->injectBytes(
            QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"));
        transport->injectBytes(
            QByteArrayLiteral(":server 001 omairc :Welcome\r\n"));
        sentPrivmsg = session.sendPrivmsg(QStringLiteral("friend"), privateBody);
        sentPass = transport->writtenFrames().contains(
            QByteArrayLiteral("PASS pass-secret-7f93\r\n"));
    }

    QByteArray authenticatePayload;
    {
        auto *transport = new FakeIrcTransport;
        IrcSession session(sessionConfig(password), transport);
        session.start();
        transport->completeConnect();
        transport->injectBytes(
            QByteArrayLiteral(":server CAP omairc LS :sasl=PLAIN\r\n"));
        transport->injectBytes(
            QByteArrayLiteral(":server CAP omairc ACK :sasl\r\n"));
        transport->injectBytes(QByteArrayLiteral("AUTHENTICATE +\r\n"));
        authenticatePayload = transport->writtenFrames().last().trimmed();
    }

    qInstallMessageHandler(previousHandler);
    capturedMessages = nullptr;
    const QString output = messages.join(QLatin1Char('\n'));
    QVERIFY(sentPass);
    QVERIFY(sentPrivmsg);
    QVERIFY2(authenticatePayload.startsWith("AUTHENTICATE "), authenticatePayload.constData());
    QVERIFY2(!output.contains(password), qPrintable(output));
    QVERIFY2(!output.contains(QString::fromLatin1(authenticatePayload)), qPrintable(output));
    QVERIFY2(!output.contains(privateBody), qPrintable(output));
}

int runQtIrcTransportIntegrationTests(int argc, char **argv)
{
    QtIrcTransportIntegrationTest transport;
    return QTest::qExec(&transport, argc, argv);
}

#include "tst_qtirctransport.moc"
