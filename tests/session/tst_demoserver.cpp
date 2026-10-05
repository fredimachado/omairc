#include <QSignalSpy>
#include <QTest>

#include "channellistmodel.h"
#include "conversationlistmodel.h"
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
    void rejoinEchoesSelfJoin();
    void closedChannelsRejoinFromDemoEcho();
    void seedsServiceAccounts();
    void seedsQueryHeaderFacts();
    void seedsSelfAvatars();
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

void DemoServerTest::seedsQueryHeaderFacts()
{
    IrcController controller;
    IrcDemoServer demo;
    QVERIFY(demo.attach(controller, false));
    const QString network = IrcDemoServer::omarchyNetworkId();

    const QVariantMap anna = controller.peerHeader(network, QStringLiteral("anna"));
    QCOMPARE(anna.value(QStringLiteral("presence")).toString(), QStringLiteral("online"));
    QCOMPARE(anna.value(QStringLiteral("realname")).toString(), QStringLiteral("Anna Vale"));
    QCOMPARE(anna.value(QStringLiteral("labels")).toStringList(), QStringList());

    const QVariantMap ivy = controller.peerHeader(network, QStringLiteral("ivy"));
    QCOMPARE(ivy.value(QStringLiteral("presence")).toString(), QStringLiteral("away"));
    QStringList ivyLabels;
    ivyLabels << QStringLiteral("unauthenticated");
    QCOMPARE(ivy.value(QStringLiteral("labels")).toStringList(), ivyLabels);

    const QVariantMap fred = controller.peerHeader(network, QStringLiteral("fred"));
    QCOMPARE(fred.value(QStringLiteral("presence")).toString(), QStringLiteral("online"));
    QCOMPARE(fred.value(QStringLiteral("realname")).toString(), QStringLiteral("Fred Machado"));
    QStringList fredLabels;
    fredLabels << QStringLiteral("fredm") << QStringLiteral("server operator");
    QCOMPARE(fred.value(QStringLiteral("labels")).toStringList(), fredLabels);

    const QVariantMap dax = controller.peerHeader(network, QStringLiteral("dax"));
    QCOMPARE(dax.value(QStringLiteral("realname")).toString(), QStringLiteral("Packet Bot"));
    QStringList daxLabels;
    daxLabels << QStringLiteral("bot");
    QCOMPARE(dax.value(QStringLiteral("labels")).toStringList(), daxLabels);

    const QVariantMap lena = controller.peerHeader(network, QStringLiteral("lena"));
    QCOMPARE(lena.value(QStringLiteral("presence")).toString(), QStringLiteral("away"));
    QCOMPARE(lena.value(QStringLiteral("realname")).toString(), QStringLiteral("Lena Pink"));
    QStringList lenaLabels;
    lenaLabels << QStringLiteral("pinkieval");
    QCOMPARE(lena.value(QStringLiteral("labels")).toStringList(), lenaLabels);

    const QVariantMap ghost = controller.peerHeader(network, QStringLiteral("ghost"));
    QCOMPARE(ghost.value(QStringLiteral("presence")).toString(), QStringLiteral("offline"));

    const QVariantMap oak = controller.peerHeader(IrcDemoServer::oftcNetworkId(),
                                                  QStringLiteral("oak"));
    QCOMPARE(oak.value(QStringLiteral("labels")).toStringList(), QStringList());
    QCOMPARE(oak.value(QStringLiteral("realname")).toString(), QString());
}

void DemoServerTest::seedsSelfAvatars()
{
    IrcController controller;
    IrcDemoServer demo;
    QVERIFY(demo.attach(controller, false));
    // Both operators advertise the Omairc mark. The value is a real HTTPS URL so
    // the demo exercises the avatar store's network path, unlike the bundled
    // peer art.
    const QString avatar = QStringLiteral("https://omairc.app/omairc128.png");
    QCOMPARE(controller.peerMetadata(IrcDemoServer::omarchyNetworkId(),
                                    QStringLiteral("fred"))
                 .value(QStringLiteral("avatar"))
                 .toString(),
             avatar);
    QCOMPARE(controller.peerMetadata(IrcDemoServer::oftcNetworkId(),
                                    QStringLiteral("oak"))
                 .value(QStringLiteral("avatar"))
                 .toString(),
             avatar);
    // A peer keeps the bundled resource, so the demo shows both paths at once.
    QCOMPARE(controller.peerMetadata(IrcDemoServer::omarchyNetworkId(),
                                    QStringLiteral("mira"))
                 .value(QStringLiteral("avatar"))
                 .toString(),
             QStringLiteral("qrc:/demo/mira-avatar.png"));
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

void DemoServerTest::rejoinEchoesSelfJoin()
{
    IrcController controller;
    IrcDemoServer demo;
    QVERIFY(demo.attach(controller, true));
    controller.selectConversation(IrcDemoServer::omarchyNetworkId(),
                                  QStringLiteral("#omarchy"));
    QVERIFY(controller.channelJoined());

    QVERIFY(controller.sendMessage(QStringLiteral("/part")));
    QVERIFY(!controller.channelJoined());
    QVERIFY(controller.sendMessage(QStringLiteral("/join #omarchy")));
    QVERIFY(controller.channelJoined());
}

void DemoServerTest::closedChannelsRejoinFromDemoEcho()
{
    IrcController controller;
    IrcDemoServer demo;
    QVERIFY(demo.attach(controller, true));
    const QString network = IrcDemoServer::omarchyNetworkId();

    controller.selectConversation(network, QStringLiteral("#desktop"));
    QVERIFY(controller.sendMessage(QStringLiteral("/part")));
    controller.closeConversationRow(network, QStringLiteral("#desktop"));
    controller.selectConversation(network, QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("/part")));
    controller.closeConversationRow(network, QStringLiteral("#omarchy"));

    controller.selectConversation(network, QStringLiteral("#help"));
    QVERIFY(controller.sendMessage(QStringLiteral("/join #desktop,#omarchy")));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QVERIFY(controller.channelJoined());

    auto *model = qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(model);
    bool sawDesktop = false;
    bool sawOmarchy = false;
    for (int row = 0; row < model->rowCount(); ++row) {
        const QString name = model->data(
            model->index(row, 0), ConversationListModel::ConversationRole).toString();
        if (name == QLatin1String("#desktop"))
            sawDesktop = true;
        if (name == QLatin1String("#omarchy"))
            sawOmarchy = true;
    }
    QVERIFY(sawDesktop);
    QVERIFY(sawOmarchy);

    controller.selectConversation(network, QStringLiteral("#desktop"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#desktop"));
    QVERIFY(controller.channelJoined());
}

int runDemoServerTests(int argc, char **argv)
{
    DemoServerTest tests;
    return QTest::qExec(&tests, argc, argv);
}

#include "tst_demoserver.moc"
