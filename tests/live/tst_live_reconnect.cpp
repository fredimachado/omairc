#include "liveharness.h"
#include "livepeer.h"
#include "conversationlistmodel.h"
#include "irccapability.h"
#include "memberlistmodel.h"

#include <QAbstractItemModel>
#include <QTest>

class LiveReconnectTest : public QObject
{
    Q_OBJECT

private slots:
    void initTestCase();
    void reconnectAutojoinKeepsChannelState();
};

namespace
{
bool sameFolded(const QString &left, const QString &right)
{
    return left.compare(right, Qt::CaseInsensitive) == 0;
}

QString messageText(const std::string &value)
{
    return QString::fromUtf8(value.data(), qsizetype(value.size()));
}

int conversationMatches(QAbstractItemModel *model, const QString &target)
{
    if (!model)
        return 0;
    int matches = 0;
    for (int row = 0; row < model->rowCount(); ++row) {
        if (sameFolded(model->index(row, 0).data(ConversationListModel::ConversationRole)
                           .toString(),
                       target)) {
            ++matches;
        }
    }
    return matches;
}

int namesEndCount(const QVector<IrcMessage> &incoming, const QString &channel)
{
    int ends = 0;
    for (const IrcMessage &message : incoming) {
        if (message.command != "366" || message.parameters.size() < 2)
            continue;
        if (sameFolded(messageText(message.parameters[1]), channel))
            ++ends;
    }
    return ends;
}
}

void LiveReconnectTest::initTestCase()
{
    QVERIFY2(!qEnvironmentVariableIsEmpty("OMAIRC_LIVE_DAEMONS"),
             "Run bin/test-live. live_tests needs the compose runner.");
    QVERIFY2(!liveDaemons().isEmpty(),
             "OMAIRC_LIVE_DAEMONS was set but no daemon ports were exported.");
}

void LiveReconnectTest::reconnectAutojoinKeepsChannelState()
{
    const LiveDaemonInfo *daemon = liveDaemon(QStringLiteral("ergo"));
    if (!daemon)
        QSKIP("Ergo is not in this live profile");

    const bool tls = daemon->plainPort == 0;
    const quint16 port = tls ? daemon->tlsPort : daemon->plainPort;
    const QString channel = uniqueChannel();
    const QString seed = QStringLiteral("reconnect-seed-%1").arg(channel);

    RawIrcPeer peer(liveHost(), port, tls, liveSslConfiguration(),
                    uniqueNick(daemon->nickLength));
    QVERIFY(peer.waitRegistered());
    QVERIFY(peer.join(channel));
    peer.writeLine(QStringLiteral("PRIVMSG %1 :%2").arg(channel, seed));

    LiveClient client(*daemon,
                      uniqueNick(daemon->nickLength),
                      tls,
                      {},
                      {},
                      {},
                      true,
                      {channel});
    if (!client.canForceDisconnect())
        QSKIP("Live transport disconnect is unavailable");

    QVERIFY2(client.waitRegistered(),
             qPrintable(QStringLiteral("register: ") + client.lastError));
    QVERIFY(waitUntil([&] { return namesEndCount(client.incoming, channel) >= 1; }));
    QVERIFY(client.session->capabilities().contains(IrcCapability::ChatHistory)
            || waitUntil([&] {
                   return client.session->capabilities().contains(
                       IrcCapability::ChatHistory);
               }));

    client.selectChannel(channel);
    QVERIFY(waitUntil([&] {
        return client.memberRole(peer.nick, MemberListModel::NickRole).isValid();
    }));

    auto *conversations = client.controller.conversations();
    QCOMPARE(conversationMatches(conversations, channel), 1);
    QCOMPARE(client.controller.selectedTarget(), channel);

    const int namesBefore = namesEndCount(client.incoming, channel);
    QVERIFY(client.forceDisconnect());
    QVERIFY(client.waitReconnecting());
    QVERIFY2(client.waitRegisteredAgain(),
             qPrintable(QStringLiteral("re-register: ") + client.lastError));
    QVERIFY(waitUntil([&] {
        return namesEndCount(client.incoming, channel) > namesBefore;
    }));

    QCOMPARE(conversationMatches(conversations, channel), 1);
    if (!sameFolded(client.controller.selectedTarget(), channel))
        client.selectChannel(channel);
    QCOMPARE(client.controller.selectedTarget(), channel);
    QVERIFY(waitUntil([&] {
        return client.memberRole(peer.nick, MemberListModel::NickRole).isValid();
    }));
    QVERIFY(client.controller.members()->rowCount() > 0);
}

int runLiveReconnectTests(int argc, char **argv)
{
    LiveReconnectTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_live_reconnect.moc"
