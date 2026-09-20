#include <QSignalSpy>
#include <QTest>

#include "irccontroller.h"
#include "ircdemoserver.h"
#include "ircloopbacktransport.h"

class DemoServerTest : public QObject
{
    Q_OBJECT

private slots:
    void answersClientPing();
    void answersMonitorAdd();
};

void DemoServerTest::answersClientPing()
{
    IrcController controller;
    IrcDemoServer demo;
    QVERIFY(demo.attach(controller, true));
    IrcLoopbackTransport *transport = demo.omarchyTransport();
    QVERIFY(transport);

    QSignalSpy received(transport, &IrcLoopbackTransport::bytesReceived);
    transport->write(QByteArrayLiteral("PING :omairc-watchdog\r\n"));
    QCOMPARE(received.size(), 1);
    QCOMPARE(received.first().first().toByteArray(),
             QByteArrayLiteral(":server PONG irc.example :omairc-watchdog\r\n"));
}

void DemoServerTest::answersMonitorAdd()
{
    IrcController controller;
    IrcDemoServer demo;
    QVERIFY(demo.attach(controller, true));
    IrcLoopbackTransport *transport = demo.omarchyTransport();
    QVERIFY(transport);

    QSignalSpy received(transport, &IrcLoopbackTransport::bytesReceived);
    transport->write(QByteArrayLiteral("MONITOR + anna,ghost\r\n"));
    QCOMPARE(received.size(), 2);
    QCOMPARE(received.at(0).first().toByteArray(),
             QByteArrayLiteral(":server 730 fred :anna!u@h\r\n"));
    QCOMPARE(received.at(1).first().toByteArray(),
             QByteArrayLiteral(":server 731 fred :ghost\r\n"));
}

int runDemoServerTests(int argc, char **argv)
{
    DemoServerTest tests;
    return QTest::qExec(&tests, argc, argv);
}

#include "tst_demoserver.moc"
