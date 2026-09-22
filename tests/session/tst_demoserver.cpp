#include <QSignalSpy>
#include <QTest>

#include "channellistmodel.h"
#include "irccontroller.h"
#include "ircdemoserver.h"
#include "ircloopbacktransport.h"

class DemoServerTest : public QObject
{
    Q_OBJECT

private slots:
    void answersClientPing();
    void answersMonitorAdd();
    void answersList();
    void seedsServiceAccounts();
    void skipsRedundantAccountTag();
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

void DemoServerTest::seedsServiceAccounts()
{
    IrcController controller;
    IrcDemoServer demo;
    QVERIFY(demo.attach(controller, false));
    QCOMPARE(controller.peerAccount(IrcDemoServer::omarchyNetworkId(),
                                  QStringLiteral("lena")),
             QStringLiteral("pinkieval"));
    QCOMPARE(controller.peerAccount(IrcDemoServer::omarchyNetworkId(),
                                  QStringLiteral("fred")),
             QStringLiteral("fredm"));
    QCOMPARE(controller.peerAccount(IrcDemoServer::omarchyNetworkId(),
                                  QStringLiteral("sol")),
             QStringLiteral("solarius"));
    QCOMPARE(controller.peerAccount(IrcDemoServer::omarchyNetworkId(),
                                  QStringLiteral("teo")),
             QStringLiteral("teoval"));
    QCOMPARE(controller.peerAccount(IrcDemoServer::omarchyNetworkId(),
                                  QStringLiteral("kai")),
             QStringLiteral("kaidev"));
}

void DemoServerTest::skipsRedundantAccountTag()
{
    IrcController controller;
    IrcDemoServer demo;
    QVERIFY(demo.attach(controller, false));
    const int epoch = controller.peerAccountEpoch();
    IrcLoopbackTransport *transport = demo.omarchyTransport();
    QVERIFY(transport);

    transport->injectBytes(
        QByteArrayLiteral("@account=kaidev :kai!u@h PRIVMSG #omarchy :still here\r\n"));
    QCOMPARE(controller.peerAccount(IrcDemoServer::omarchyNetworkId(),
                                    QStringLiteral("kai")),
             QStringLiteral("kaidev"));
    QCOMPARE(controller.peerAccountEpoch(), epoch);
}

void DemoServerTest::answersList()
{
    IrcController controller;
    IrcDemoServer demo;
    QVERIFY(demo.attach(controller, true));
    IrcLoopbackTransport *transport = demo.omarchyTransport();
    QVERIFY(transport);

    QVERIFY(controller.sendMessage(QStringLiteral("/list")));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("LIST\r\n"));
    auto *model = qobject_cast<ChannelListModel *>(controller.channelList());
    QVERIFY(model);
    QVERIFY(model->complete());
    QCOMPARE(model->sourceCount(), 6);
    QCOMPARE(model->field(0, QStringLiteral("channel")).toString(),
             QStringLiteral("#linux"));
    QCOMPARE(model->field(0, QStringLiteral("users")).toInt(), 42);
    QCOMPARE(model->field(1, QStringLiteral("channel")).toString(),
             QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/list #l*")));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("LIST #l*\r\n"));
    QVERIFY(model->complete());
    QCOMPARE(model->rowCount(), 1);
    QCOMPARE(model->field(0, QStringLiteral("channel")).toString(),
             QStringLiteral("#linux"));
}

int runDemoServerTests(int argc, char **argv)
{
    DemoServerTest tests;
    return QTest::qExec(&tests, argc, argv);
}

#include "tst_demoserver.moc"
