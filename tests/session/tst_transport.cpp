#include <QSignalSpy>
#include <QTest>

#include "fakeirctransport.h"

class TransportTest : public QObject
{
    Q_OBJECT

private slots:
    void succeedsConnect();
    void recordsWrites();
    void injectsInboundBytes();
    void remoteCloseEndsConnection();
    void shutdownWhileConnecting();
    void tlsCertificateFailureSurfacesError();
    void secondShutdownIsSafe();
    void timeoutFailsWithoutSleep();
};

void TransportTest::succeedsConnect()
{
    FakeIrcTransport transport;
    QSignalSpy connected(&transport, &IrcTransport::connected);
    QSignalSpy encrypted(&transport, &IrcTransport::encrypted);

    QCOMPARE(transport.connectionState(), IrcTransport::ConnectionState::Idle);
    transport.connectToHost(QStringLiteral("irc.example"), 6667, false);
    QCOMPARE(transport.connectionState(), IrcTransport::ConnectionState::Connecting);
    QCOMPARE(transport.connectedHost(), QStringLiteral("irc.example"));
    QCOMPARE(transport.connectedPort(), quint16(6667));
    QVERIFY(!transport.tlsRequested());

    transport.completeConnect();
    QCOMPARE(transport.connectionState(), IrcTransport::ConnectionState::Connected);
    QCOMPARE(connected.size(), 1);
    QCOMPARE(encrypted.size(), 0);
}

void TransportTest::recordsWrites()
{
    FakeIrcTransport transport;
    transport.connectToHost(QStringLiteral("irc.example"), 6667, false);
    transport.completeConnect();

    const QByteArray ping("PING :x\r\n");
    const QByteArray nick("NICK omairc\r\n");
    transport.write(ping);
    transport.write(nick);

    QCOMPARE(transport.writtenFrames().size(), 2);
    QCOMPARE(transport.writtenFrames().at(0), ping);
    QCOMPARE(transport.writtenFrames().at(1), nick);
}

void TransportTest::injectsInboundBytes()
{
    FakeIrcTransport transport;
    QSignalSpy received(&transport, &IrcTransport::bytesReceived);
    transport.connectToHost(QStringLiteral("irc.example"), 6667, false);
    transport.completeConnect();

    const QByteArray pong("PONG :x\r\n");
    transport.injectBytes(pong);

    QCOMPARE(received.size(), 1);
    QCOMPARE(received.at(0).at(0).toByteArray(), pong);
}

void TransportTest::remoteCloseEndsConnection()
{
    FakeIrcTransport transport;
    QSignalSpy disconnected(&transport, &IrcTransport::disconnected);
    QSignalSpy error(&transport, &IrcTransport::errorOccurred);
    transport.connectToHost(QStringLiteral("irc.example"), 6667, false);
    transport.completeConnect();

    transport.remoteClose();

    QCOMPARE(transport.connectionState(), IrcTransport::ConnectionState::Disconnected);
    QCOMPARE(disconnected.size(), 1);
    QCOMPARE(error.size(), 0);
}

void TransportTest::shutdownWhileConnecting()
{
    FakeIrcTransport transport;
    QSignalSpy disconnected(&transport, &IrcTransport::disconnected);
    transport.connectToHost(QStringLiteral("irc.example"), 6697, true);
    QCOMPARE(transport.connectionState(), IrcTransport::ConnectionState::Connecting);

    transport.shutdown();

    QCOMPARE(transport.connectionState(), IrcTransport::ConnectionState::Disconnected);
    QCOMPARE(disconnected.size(), 1);
}

void TransportTest::tlsCertificateFailureSurfacesError()
{
    FakeIrcTransport transport;
    QSignalSpy error(&transport, &IrcTransport::errorOccurred);
    QSignalSpy encrypted(&transport, &IrcTransport::encrypted);
    transport.connectToHost(QStringLiteral("irc.example"), 6697, true);
    QVERIFY(transport.tlsRequested());

    const QString details = QStringLiteral("TLS certificate error: The certificate has expired (CN=irc.example)");
    transport.failTls(details);

    QCOMPARE(transport.connectionState(), IrcTransport::ConnectionState::Failed);
    QCOMPARE(error.size(), 1);
    QCOMPARE(error.at(0).at(0).toString(), details);
    QCOMPARE(transport.lastError(), details);
    QCOMPARE(encrypted.size(), 0);
}

void TransportTest::secondShutdownIsSafe()
{
    FakeIrcTransport transport;
    QSignalSpy disconnected(&transport, &IrcTransport::disconnected);
    transport.connectToHost(QStringLiteral("irc.example"), 6667, false);
    transport.shutdown();
    transport.shutdown();
    transport.shutdown();

    QCOMPARE(transport.connectionState(), IrcTransport::ConnectionState::Disconnected);
    QCOMPARE(disconnected.size(), 1);
}

void TransportTest::timeoutFailsWithoutSleep()
{
    FakeIrcTransport transport;
    QSignalSpy error(&transport, &IrcTransport::errorOccurred);
    transport.connectToHost(QStringLiteral("irc.example"), 6667, false);
    transport.timeoutConnect();

    QCOMPARE(transport.connectionState(), IrcTransport::ConnectionState::Failed);
    QCOMPARE(error.size(), 1);
    QVERIFY(error.at(0).at(0).toString().contains(QStringLiteral("timed out")));
}

int runTransportTests(int argc, char **argv)
{
    TransportTest transport;
    return QTest::qExec(&transport, argc, argv);
}

#include "tst_transport.moc"
