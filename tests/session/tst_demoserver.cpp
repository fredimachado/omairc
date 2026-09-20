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

int runDemoServerTests(int argc, char **argv)
{
    DemoServerTest tests;
    return QTest::qExec(&tests, argc, argv);
}

#include "tst_demoserver.moc"
