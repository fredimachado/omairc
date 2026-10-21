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
    void echoesSelfPart();
    void partEchoLeavesChannel();
    void echoesSelfNickChange();
    void echoesTopicChangeOnRejoin();
    void rejoinCarriesSeededTopic();
    void rejoinCarriesTopicOnDirectoryChannel();
    void ignoresTopicQueryAndNonChannelTargets();
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

void DemoServerTest::echoesSelfPart()
{
    IrcController controller;
    IrcDemoServer demo;
    QVERIFY(demo.attach(controller, true));
    IrcLoopbackTransport *transport = demo.omarchyTransport();
    QVERIFY(transport);

    QSignalSpy received(transport, &IrcLoopbackTransport::bytesReceived);
    transport->write(QByteArrayLiteral("PART #desktop\r\n"));
    QCOMPARE(received.size(), 1);
    QCOMPARE(received.last().first().toByteArray(),
             QByteArrayLiteral(":fred!u@h PART #desktop\r\n"));

    transport->write(QByteArrayLiteral("PART #desktop :see you\r\n"));
    QCOMPARE(received.size(), 2);
    QCOMPARE(received.last().first().toByteArray(),
             QByteArrayLiteral(":fred!u@h PART #desktop :see you\r\n"));

    // Any channel name is echoed, as JOIN echoes it. A target that is not a
    // channel gets no echo.
    transport->write(QByteArrayLiteral("PART #nowhere\r\n"));
    QCOMPARE(received.size(), 3);
    QCOMPARE(received.last().first().toByteArray(),
             QByteArrayLiteral(":fred!u@h PART #nowhere\r\n"));
    transport->write(QByteArrayLiteral("PART fred\r\n"));
    QCOMPARE(received.size(), 3);
}

void DemoServerTest::partEchoLeavesChannel()
{
    IrcController controller;
    IrcDemoServer demo;
    QVERIFY(demo.attach(controller, true));
    IrcLoopbackTransport *transport = demo.omarchyTransport();
    QVERIFY(transport);
    controller.selectConversation(IrcDemoServer::omarchyNetworkId(),
                                  QStringLiteral("#desktop"));
    QVERIFY(controller.channelJoined());

    // Written straight to the transport, so no controller-side /part runs and
    // only the demo echo can mark the channel left.
    transport->write(QByteArrayLiteral("PART #desktop\r\n"));
    QVERIFY(!controller.channelJoined());
}

void DemoServerTest::echoesSelfNickChange()
{
    IrcController controller;
    IrcDemoServer demo;
    QVERIFY(demo.attach(controller, true));
    IrcLoopbackTransport *transport = demo.omarchyTransport();
    QVERIFY(transport);

    QSignalSpy received(transport, &IrcLoopbackTransport::bytesReceived);
    transport->write(QByteArrayLiteral("NICK bob\r\n"));
    QCOMPARE(received.size(), 1);
    QCOMPARE(received.last().first().toByteArray(),
             QByteArrayLiteral(":fred!u@h NICK :bob\r\n"));

    // Later replies and echoes use the nick the demo now holds.
    transport->write(QByteArrayLiteral("AWAY :brb\r\n"));
    QCOMPARE(received.size(), 2);
    QCOMPARE(received.last().first().toByteArray(),
             QByteArrayLiteral(":server 306 bob :You have been marked as being away\r\n"));

    transport->write(QByteArrayLiteral("PART #desktop\r\n"));
    QCOMPARE(received.size(), 3);
    QCOMPARE(received.last().first().toByteArray(),
             QByteArrayLiteral(":bob!u@h PART #desktop\r\n"));

    // MONITOR rebuilds its online set from the new nick: bob is online, and the
    // seeded fred no longer is.
    transport->write(QByteArrayLiteral("MONITOR + fred,bob\r\n"));
    QCOMPARE(received.size(), 5);
    QCOMPARE(received.at(3).first().toByteArray(),
             QByteArrayLiteral(":server 730 bob :bob!u@h\r\n"));
    QCOMPARE(received.at(4).first().toByteArray(),
             QByteArrayLiteral(":server 731 bob :fred\r\n"));

    // The next NICK carries the old nick as its prefix.
    transport->write(QByteArrayLiteral("NICK alice\r\n"));
    QCOMPARE(received.size(), 6);
    QCOMPARE(received.last().first().toByteArray(),
             QByteArrayLiteral(":bob!u@h NICK :alice\r\n"));
}

void DemoServerTest::echoesTopicChangeOnRejoin()
{
    IrcController controller;
    IrcDemoServer demo;
    QVERIFY(demo.attach(controller, true));
    IrcLoopbackTransport *transport = demo.omarchyTransport();
    QVERIFY(transport);

    QSignalSpy received(transport, &IrcLoopbackTransport::bytesReceived);
    transport->write(QByteArrayLiteral("TOPIC #desktop :new banner\r\n"));
    QCOMPARE(received.size(), 1);
    QCOMPARE(received.last().first().toByteArray(),
             QByteArrayLiteral(":fred!u@h TOPIC #desktop :new banner\r\n"));

    // The rejoin's self JOIN is followed by 332 with the value the TOPIC set.
    transport->write(QByteArrayLiteral("PART #desktop\r\n"));
    transport->write(QByteArrayLiteral("JOIN #desktop\r\n"));
    QCOMPARE(received.size(), 4);
    QCOMPARE(received.at(2).first().toByteArray(),
             QByteArrayLiteral(":fred!u@h JOIN :#desktop\r\n"));
    QCOMPARE(received.at(3).first().toByteArray(),
             QByteArrayLiteral(":server 332 fred #desktop :new banner\r\n"));

    // A cleared topic leaves the rejoin with no 332.
    transport->write(QByteArrayLiteral("TOPIC #desktop :\r\n"));
    QCOMPARE(received.size(), 5);
    QCOMPARE(received.last().first().toByteArray(),
             QByteArrayLiteral(":fred!u@h TOPIC #desktop :\r\n"));
    transport->write(QByteArrayLiteral("PART #desktop\r\n"));
    transport->write(QByteArrayLiteral("JOIN #desktop\r\n"));
    QCOMPARE(received.size(), 7);
    QCOMPARE(received.last().first().toByteArray(),
             QByteArrayLiteral(":fred!u@h JOIN :#desktop\r\n"));
}

void DemoServerTest::rejoinCarriesSeededTopic()
{
    IrcController controller;
    IrcDemoServer demo;
    QVERIFY(demo.attach(controller, true));
    IrcLoopbackTransport *transport = demo.omarchyTransport();
    QVERIFY(transport);

    QSignalSpy received(transport, &IrcLoopbackTransport::bytesReceived);
    transport->write(QByteArrayLiteral("JOIN #omarchy\r\n"));
    QCOMPARE(received.size(), 2);
    QCOMPARE(received.at(0).first().toByteArray(),
             QByteArrayLiteral(":fred!u@h JOIN :#omarchy\r\n"));
    QCOMPARE(received.at(1).first().toByteArray(),
             QByteArrayLiteral(":server 332 fred #omarchy :A cozy corner for Omarchy users and builders.\r\n"));
}

void DemoServerTest::ignoresTopicQueryAndNonChannelTargets()
{
    IrcController controller;
    IrcDemoServer demo;
    QVERIFY(demo.attach(controller, true));
    IrcLoopbackTransport *transport = demo.omarchyTransport();
    QVERIFY(transport);

    QSignalSpy received(transport, &IrcLoopbackTransport::bytesReceived);
    // A bare TOPIC asks for the topic; it is not a change, so nothing is echoed.
    transport->write(QByteArrayLiteral("TOPIC #desktop\r\n"));
    // A target that is not a channel has no topic to set.
    transport->write(QByteArrayLiteral("TOPIC fred :x\r\n"));
    QCOMPARE(received.size(), 0);
}

void DemoServerTest::rejoinCarriesTopicOnDirectoryChannel()
{
    IrcController controller;
    IrcDemoServer demo;
    QVERIFY(demo.attach(controller, true));
    IrcLoopbackTransport *transport = demo.omarchyTransport();
    QVERIFY(transport);

    // #linux is only in the LIST directory, not seeded, but a topic set on it is
    // kept and the rejoin reports it, as a real server does for any channel.
    QSignalSpy received(transport, &IrcLoopbackTransport::bytesReceived);
    transport->write(QByteArrayLiteral("TOPIC #linux :kernel talk\r\n"));
    QCOMPARE(received.size(), 1);
    QCOMPARE(received.last().first().toByteArray(),
             QByteArrayLiteral(":fred!u@h TOPIC #linux :kernel talk\r\n"));

    transport->write(QByteArrayLiteral("JOIN #linux\r\n"));
    QCOMPARE(received.size(), 3);
    QCOMPARE(received.at(1).first().toByteArray(),
             QByteArrayLiteral(":fred!u@h JOIN :#linux\r\n"));
    QCOMPARE(received.at(2).first().toByteArray(),
             QByteArrayLiteral(":server 332 fred #linux :kernel talk\r\n"));
}

int runDemoServerTests(int argc, char **argv)
{
    DemoServerTest tests;
    return QTest::qExec(&tests, argc, argv);
}

#include "tst_demoserver.moc"
