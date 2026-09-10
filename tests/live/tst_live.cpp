#include "liveharness.h"
#include "livepeer.h"
#include "conversationlistmodel.h"
#include "irccapability.h"
#include "ircjointarget.h"
#include "memberlistmodel.h"

#include <QAbstractItemModel>
#include <QTest>

class LiveIrcdTest : public QObject
{
    Q_OBJECT

private slots:
    void initTestCase();

    void registerPlain_data();
    void registerPlain();
    void registerTls_data();
    void registerTls();
    void capabilityFlags_data();
    void capabilityFlags();
    void isupport_data();
    void isupport();
    void classicTraffic_data();
    void classicTraffic();
    void nickInUse_data();
    void nickInUse();
    void nickLength_data();
    void nickLength();
    void whoSnapshot_data();
    void whoSnapshot();
    void peerAway_data();
    void peerAway();
    void peerTyping_data();
    void peerTyping();
    void peerMetadata();
    void saslPlain();
    void foldedNickCollision_data();
    void foldedNickCollision();
    void joinMultipleChannels_data();
    void joinMultipleChannels();
};

namespace
{
void fillDaemonRows()
{
    for (const LiveDaemonInfo &daemon : liveDaemons())
        QTest::newRow(qPrintable(daemon.name)) << daemon.name;
}

QString messageText(const std::string &value)
{
    return QString::fromUtf8(value.data(), qsizetype(value.size()));
}

bool sameFolded(const QString &left, const QString &right)
{
    return left.compare(right, Qt::CaseInsensitive) == 0;
}

int conversationRow(QAbstractItemModel *model, const QString &target)
{
    if (!model)
        return -1;
    for (int row = 0; row < model->rowCount(); ++row) {
        if (sameFolded(model->index(row, 0).data(ConversationListModel::ConversationRole)
                           .toString(),
                       target)) {
            return row;
        }
    }
    return -1;
}

bool joinChannel(LiveClient &client, const QString &channel)
{
    const std::optional<IrcJoinTarget> target = IrcJoinTarget::make(channel);
    return target && client.session->join(*target);
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

bool sawChannelModeKey(const QVector<IrcMessage> &incoming, const QString &channel)
{
    for (const IrcMessage &message : incoming) {
        if (message.command != "MODE" || message.parameters.size() < 2)
            continue;
        if (!sameFolded(messageText(message.parameters[0]), channel))
            continue;
        for (std::size_t index = 1; index < message.parameters.size(); ++index) {
            if (messageText(message.parameters[index]).contains(QLatin1Char('k')))
                return true;
        }
    }
    return false;
}

bool sawPart(const QVector<IrcMessage> &incoming, const QString &channel)
{
    for (const IrcMessage &message : incoming) {
        if (message.command != "PART" || message.parameters.empty())
            continue;
        if (sameFolded(messageText(message.parameters[0]), channel))
            return true;
    }
    return false;
}
}

void LiveIrcdTest::initTestCase()
{
    QVERIFY2(!qEnvironmentVariableIsEmpty("OMAIRC_LIVE_DAEMONS"),
             "Run bin/test-live. live_tests needs the compose runner.");
    QVERIFY2(!liveDaemons().isEmpty(),
             "OMAIRC_LIVE_DAEMONS was set but no daemon ports were exported.");
}

void LiveIrcdTest::registerPlain_data()
{
    QTest::addColumn<QString>("daemonName");
    for (const LiveDaemonInfo &daemon : liveDaemons()) {
        if (daemon.plainPort)
            QTest::newRow(qPrintable(daemon.name)) << daemon.name;
    }
}

void LiveIrcdTest::registerPlain()
{
    QFETCH(QString, daemonName);
    const LiveDaemonInfo *daemon = liveDaemon(daemonName);
    QVERIFY(daemon);
    LiveClient client(*daemon, uniqueNick(daemon->nickLength), false);
    QVERIFY2(client.waitRegistered(),
             qPrintable(daemonName + QLatin1Char(' ') + client.lastError));
}

void LiveIrcdTest::registerTls_data()
{
    QTest::addColumn<QString>("daemonName");
    for (const LiveDaemonInfo &daemon : liveDaemons()) {
        if (daemon.tlsPort)
            QTest::newRow(qPrintable(daemon.name)) << daemon.name;
    }
}

void LiveIrcdTest::registerTls()
{
    QFETCH(QString, daemonName);
    const LiveDaemonInfo *daemon = liveDaemon(daemonName);
    QVERIFY(daemon);
    LiveClient client(*daemon, uniqueNick(daemon->nickLength), true);
    QVERIFY2(client.waitRegistered(),
             qPrintable(daemonName + QLatin1Char(' ') + client.lastError));
}

void LiveIrcdTest::capabilityFlags_data()
{
    QTest::addColumn<QString>("daemonName");
    fillDaemonRows();
}

void LiveIrcdTest::capabilityFlags()
{
    QFETCH(QString, daemonName);
    const LiveDaemonInfo *daemon = liveDaemon(daemonName);
    QVERIFY(daemon);
    LiveClient client(*daemon, uniqueNick(daemon->nickLength), daemon->plainPort == 0);
    QVERIFY(client.waitRegistered());
    const QString channel = uniqueChannel();
    QVERIFY(joinChannel(client, channel));
    QVERIFY(waitUntil([&] {
        return messageHasCommand(client.incoming, QStringLiteral("366"));
    }));
    client.selectChannel(channel);
    QCOMPARE(client.controller.hasAwayPresence(), daemon->awayNotify);
    QCOMPARE(client.controller.hasTyping(), daemon->messageTags);
    QCOMPARE(client.controller.hasMemberStatus(), daemon->metadata);
    if (!daemon->wantedCapsAdvertised) {
        QVERIFY(!client.session->capabilities().contains(IrcCapability::AwayNotify));
        QVERIFY(!client.session->capabilities().contains(IrcCapability::MessageTags));
        QVERIFY(!client.session->capabilities().contains(IrcCapability::MemberMetadata));
    }
}

void LiveIrcdTest::isupport_data()
{
    QTest::addColumn<QString>("daemonName");
    fillDaemonRows();
}

void LiveIrcdTest::isupport()
{
    QFETCH(QString, daemonName);
    const LiveDaemonInfo *daemon = liveDaemon(daemonName);
    QVERIFY(daemon);
    LiveClient client(*daemon, uniqueNick(daemon->nickLength), daemon->plainPort == 0);
    QVERIFY(client.waitRegistered());
    QVERIFY(waitUntil([&] { return client.features().nickLength().has_value(); }));
    const IrcServerFeatures &features = client.features();
    QCOMPARE(features.caseMapping().kind(), daemon->mapping);
    QVERIFY(!features.prefixModes().empty());
    QVERIFY(!features.prefixSymbols().empty());
    QVERIFY(features.isChannel("#live"));
    QVERIFY(*features.nickLength() > 0);
}

void LiveIrcdTest::classicTraffic_data()
{
    QTest::addColumn<QString>("daemonName");
    fillDaemonRows();
}

void LiveIrcdTest::classicTraffic()
{
    QFETCH(QString, daemonName);
    const LiveDaemonInfo *daemon = liveDaemon(daemonName);
    QVERIFY(daemon);
    const bool tls = daemon->plainPort == 0;
    const quint16 port = tls ? daemon->tlsPort : daemon->plainPort;
    LiveClient client(*daemon, uniqueNick(daemon->nickLength), tls);
    QVERIFY(client.waitRegistered());
    RawIrcPeer peer(liveHost(), port, tls, liveSslConfiguration(),
                    uniqueNick(daemon->nickLength));
    QVERIFY(peer.waitRegistered());
    const QString channel = uniqueChannel();
    QVERIFY(joinChannel(client, channel));
    QVERIFY(waitUntil([&] {
        return messageHasCommand(client.incoming, QStringLiteral("366"));
    }));
    QVERIFY(peer.join(channel));
    LiveClassicSighting want;
    want.channel = channel;
    want.otherNick = client.session->nick();
    want.privmsg = QStringLiteral("hello live");
    want.notice = QStringLiteral("notice live");
    want.action = QStringLiteral("waves");
    want.topic = QStringLiteral("live topic");
    QVERIFY(client.session->setTopic(channel, want.topic));
    QVERIFY(client.session->sendPrivmsg(channel, want.privmsg));
    QVERIFY(client.session->sendNotice(channel, want.notice));
    QVERIFY(client.session->sendAction(channel, want.action));
    QVERIFY(client.session->part(channel));
    QVERIFY2(waitUntil([&] { return liveClassicComplete(peer.incoming, want); }),
             qPrintable(liveClassicGap(peer.incoming, want)));
}

void LiveIrcdTest::nickInUse_data()
{
    QTest::addColumn<QString>("daemonName");
    fillDaemonRows();
}

void LiveIrcdTest::nickInUse()
{
    QFETCH(QString, daemonName);
    const LiveDaemonInfo *daemon = liveDaemon(daemonName);
    QVERIFY(daemon);
    const QString nick = uniqueNick(qMax(1, daemon->nickLength - 1));
    LiveClient first(*daemon, nick, daemon->plainPort == 0);
    QVERIFY(first.waitRegistered());
    LiveClient second(*daemon, nick, daemon->plainPort == 0);
    QVERIFY2(second.waitRegistered(),
             qPrintable(daemonName + QLatin1Char(' ') + second.lastError));
    const QString assigned = second.session->nick();
    QVERIFY(assigned.compare(nick + QLatin1Char('_'), Qt::CaseInsensitive) == 0
            || assigned.compare(nick + QLatin1Char('2'), Qt::CaseInsensitive) == 0);
    QVERIFY(second.hasServerLabel(QStringLiteral("433")));
}

void LiveIrcdTest::nickLength_data()
{
    QTest::addColumn<QString>("daemonName");
    fillDaemonRows();
}

void LiveIrcdTest::nickLength()
{
    QFETCH(QString, daemonName);
    const LiveDaemonInfo *daemon = liveDaemon(daemonName);
    QVERIFY(daemon);
    const QString tooLong = QStringLiteral("abcdefghij");
    LiveClient client(*daemon, tooLong, daemon->plainPort == 0);
    if (daemon->nickLength < tooLong.size()) {
        QVERIFY2(client.waitFailed(),
                 qPrintable(daemonName + QLatin1Char(' ') + client.lastError));
        return;
    }
    QVERIFY(client.waitRegistered());
}

void LiveIrcdTest::whoSnapshot_data()
{
    QTest::addColumn<QString>("daemonName");
    for (const LiveDaemonInfo &daemon : liveDaemons()) {
        if (daemon.awayNotify)
            QTest::newRow(qPrintable(daemon.name)) << daemon.name;
    }
}

void LiveIrcdTest::whoSnapshot()
{
    QFETCH(QString, daemonName);
    const LiveDaemonInfo *daemon = liveDaemon(daemonName);
    QVERIFY(daemon);
    LiveClient client(*daemon, uniqueNick(daemon->nickLength), daemon->plainPort == 0);
    QVERIFY(client.waitRegistered());
    const QString channel = uniqueChannel();
    QVERIFY(joinChannel(client, channel));
    QVERIFY(waitUntil([&] {
        return messageHasCommand(client.incoming, QStringLiteral("352"));
    }));
    QVERIFY2(!messageHasCommand(client.incoming, QStringLiteral("354")),
             "WHO snapshot must be classic 352, not WHOX 354");
}

void LiveIrcdTest::peerAway_data()
{
    QTest::addColumn<QString>("daemonName");
    for (const LiveDaemonInfo &daemon : liveDaemons()) {
        if (daemon.awayNotify)
            QTest::newRow(qPrintable(daemon.name)) << daemon.name;
    }
}

void LiveIrcdTest::peerAway()
{
    QFETCH(QString, daemonName);
    const LiveDaemonInfo *daemon = liveDaemon(daemonName);
    QVERIFY(daemon);
    const bool tls = daemon->plainPort == 0;
    const quint16 port = tls ? daemon->tlsPort : daemon->plainPort;
    LiveClient client(*daemon, uniqueNick(daemon->nickLength), tls);
    QVERIFY(client.waitRegistered());
    const QString channel = uniqueChannel();
    QVERIFY(joinChannel(client, channel));
    QVERIFY(waitUntil([&] {
        return messageHasCommand(client.incoming, QStringLiteral("366"));
    }));

    RawIrcPeer peer(liveHost(), port, tls, liveSslConfiguration(),
                    uniqueNick(daemon->nickLength));
    QVERIFY(peer.waitRegistered());
    QVERIFY(peer.join(channel));
    client.selectChannel(channel);
    QVERIFY(waitUntil([&] {
        return client.memberRole(peer.nick, MemberListModel::NickRole).isValid();
    }));
    peer.writeLine(QStringLiteral("AWAY :gone"));
    QVERIFY(waitUntil([&] {
        return client.memberRole(peer.nick, MemberListModel::AwayRole).toBool();
    }));
}

void LiveIrcdTest::peerTyping_data()
{
    QTest::addColumn<QString>("daemonName");
    for (const LiveDaemonInfo &daemon : liveDaemons()) {
        if (daemon.messageTags)
            QTest::newRow(qPrintable(daemon.name)) << daemon.name;
    }
}

void LiveIrcdTest::peerTyping()
{
    QFETCH(QString, daemonName);
    const LiveDaemonInfo *daemon = liveDaemon(daemonName);
    QVERIFY(daemon);
    const bool tls = daemon->plainPort == 0;
    const quint16 port = tls ? daemon->tlsPort : daemon->plainPort;
    LiveClient client(*daemon, uniqueNick(daemon->nickLength), tls);
    QVERIFY(client.waitRegistered());
    const QString channel = uniqueChannel();
    QVERIFY(joinChannel(client, channel));
    QVERIFY(waitUntil([&] {
        return messageHasCommand(client.incoming, QStringLiteral("366"));
    }));
    client.selectChannel(channel);

    RawIrcPeer peer(liveHost(), port, tls, liveSslConfiguration(),
                    uniqueNick(daemon->nickLength));
    QVERIFY(peer.waitRegistered());
    QVERIFY(peer.join(channel));
    QVERIFY(waitUntil([&] {
        return client.memberRole(peer.nick, MemberListModel::NickRole).isValid();
    }));
    peer.writeLine(QStringLiteral("@+typing=active TAGMSG %1").arg(channel));
    QVERIFY(waitUntil([&] {
        return client.controller.typingNicks().contains(peer.nick, Qt::CaseInsensitive);
    }));
}

void LiveIrcdTest::peerMetadata()
{
    const LiveDaemonInfo *daemon = liveDaemon(QStringLiteral("ergo"));
    if (!daemon)
        QSKIP("Ergo is not in this live profile");
    const bool tls = daemon->plainPort == 0;
    const quint16 port = tls ? daemon->tlsPort : daemon->plainPort;
    LiveClient client(*daemon, uniqueNick(daemon->nickLength), tls);
    QVERIFY(client.waitRegistered());
    QVERIFY(client.controller.hasMemberStatus() || waitUntil([&] {
        return client.session->capabilities().contains(IrcCapability::MemberMetadata);
    }));
    const QString channel = uniqueChannel();
    QVERIFY(joinChannel(client, channel));
    QVERIFY(waitUntil([&] {
        return messageHasCommand(client.incoming, QStringLiteral("366"));
    }));
    client.selectChannel(channel);

    RawIrcPeer peer(liveHost(), port, tls, liveSslConfiguration(),
                    uniqueNick(daemon->nickLength));
    QVERIFY(peer.waitRegistered());
    QVERIFY(peer.join(channel));
    QVERIFY(waitUntil([&] {
        return client.memberRole(peer.nick, MemberListModel::NickRole).isValid();
    }));
    peer.writeLine(QStringLiteral("METADATA %1 SET status :wave").arg(peer.nick));
    QVERIFY(waitUntil([&] {
        return client.memberRole(peer.nick, MemberListModel::StatusRole).toString()
            == QStringLiteral("wave");
    }));
}

void LiveIrcdTest::saslPlain()
{
    const LiveDaemonInfo *daemon = liveDaemon(QStringLiteral("ergo"));
    if (!daemon)
        QSKIP("Ergo is not in this live profile");
    const QString nick = uniqueNick(daemon->nickLength);
    const QString password = QStringLiteral("live-secret");
    {
        LiveClient guest(*daemon, nick, daemon->plainPort == 0);
        QVERIFY(guest.waitRegistered());
        const int notices = messageCommandCount(guest.incoming, QStringLiteral("NOTICE"));
        QVERIFY(guest.session->sendPrivmsg(
            QStringLiteral("NickServ"),
            QStringLiteral("REGISTER %1 live@omairc.test").arg(password)));
        QVERIFY(waitUntil([&] {
            return messageCommandCount(guest.incoming, QStringLiteral("NOTICE")) > notices;
        }));
    }
    LiveClient authed(*daemon, nick, daemon->plainPort == 0, password);
    QVERIFY(authed.waitServerLabel(QStringLiteral("903")));
    QVERIFY2(authed.waitRegistered(), qPrintable(authed.lastError));
}

void LiveIrcdTest::foldedNickCollision_data()
{
    QTest::addColumn<QString>("daemonName");
    fillDaemonRows();
}

void LiveIrcdTest::foldedNickCollision()
{
    QFETCH(QString, daemonName);
    const LiveDaemonInfo *daemon = liveDaemon(daemonName);
    QVERIFY(daemon);
    if (daemon->nickLength < 5)
        QSKIP("nick budget too small for casemapping pair");
    const bool tls = daemon->plainPort == 0;
    const QString base = uniqueNick(daemon->nickLength - 1);
    const QString nickA = base + QLatin1Char('[');
    const QString nickB = base + QLatin1Char('{');
    LiveClient a(*daemon, nickA, tls);
    QVERIFY(a.waitRegistered());
    LiveClient b(*daemon, nickB, tls);
    if (daemon->mapping == IrcCaseMapping::Kind::Rfc1459) {
        QVERIFY2(b.waitFailed(), qPrintable(daemonName + QLatin1Char(' ') + b.lastError));
    } else {
        QVERIFY(b.waitRegistered());
    }
}

void LiveIrcdTest::joinMultipleChannels_data()
{
    QTest::addColumn<QString>("daemonName");
    fillDaemonRows();
}

void LiveIrcdTest::joinMultipleChannels()
{
    QFETCH(QString, daemonName);
    const LiveDaemonInfo *daemon = liveDaemon(daemonName);
    QVERIFY(daemon);
    LiveClient client(*daemon, uniqueNick(daemon->nickLength), daemon->plainPort == 0);
    QVERIFY(client.waitRegistered());
    const QString first = uniqueChannel();
    const QString second = uniqueChannel();
    QVERIFY(client.controller.sendMessage(
        QStringLiteral("/join %1, %2").arg(first, second.mid(1))));

    QVERIFY(waitUntil([&] {
        int ends = 0;
        for (const IrcMessage &message : client.incoming) {
            if (message.command != "366" || message.parameters.size() < 2)
                continue;
            const QString channel = messageText(message.parameters[1]);
            if (sameFolded(channel, first) || sameFolded(channel, second))
                ++ends;
        }
        return ends >= 2;
    }));
    auto *conversations = client.controller.conversations();
    QVERIFY(conversationRow(conversations, first) >= 0);
    QVERIFY(conversationRow(conversations, second) >= 0);

    const QString keyed = uniqueChannel();
    QVERIFY(joinChannel(client, keyed));
    QVERIFY(waitUntil([&] { return namesEndCount(client.incoming, keyed) >= 1; }));
    client.selectChannel(keyed);
    QVERIFY(client.controller.sendMessage(
        QStringLiteral("/mode %1 +k s3cret").arg(keyed)));
    QVERIFY(waitUntil([&] { return sawChannelModeKey(client.incoming, keyed); }));
    QVERIFY(client.controller.sendMessage(QStringLiteral("/part %1").arg(keyed)));
    QVERIFY(waitUntil([&] { return sawPart(client.incoming, keyed); }));
    QVERIFY(client.controller.sendMessage(
        QStringLiteral("/join %1 s3cret").arg(keyed)));
    QVERIFY(waitUntil([&] { return namesEndCount(client.incoming, keyed) >= 2; }));
}

int runLiveIrcdTests(int argc, char **argv)
{
    LiveIrcdTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_live.moc"
