#include "liveharness.h"
#include "testsettings.h"
#include "livepeer.h"
#include "conversationlistmodel.h"
#include "irccapability.h"
#include "ircjointarget.h"
#include "memberlistmodel.h"
#include "messagelistmodel.h"
#include "networklogmodel.h"

#include <QAbstractItemModel>
#include <QCoreApplication>
#include <QSettings>
#include <QTemporaryDir>
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
    void chatHistoryOnJoin();
    void saslPlain();
    void conversationInventionMatrix();
    void foldedNickCollision_data();
    void foldedNickCollision();
    void asciiDirectRestore_data();
    void asciiDirectRestore();
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

bool logContains(QAbstractItemModel *lines, const QString &needle)
{
    if (!lines)
        return false;
    for (int row = 0; row < lines->rowCount(); ++row) {
        const QString text =
            lines->data(lines->index(row, 0), NetworkLogModel::TextRole).toString();
        if (text.contains(needle, Qt::CaseInsensitive))
            return true;
    }
    return false;
}

bool statusTextContains(const QVector<IrcStatusEntry> &entries, const QString &needle)
{
    for (const IrcStatusEntry &entry : entries) {
        if (entry.text().contains(needle, Qt::CaseInsensitive))
            return true;
    }
    return false;
}

bool noStatusTextContains(const QVector<IrcStatusEntry> &entries, const QString &needle)
{
    for (const IrcStatusEntry &entry : entries) {
        if (entry.text().contains(needle, Qt::CaseInsensitive))
            return false;
    }
    return true;
}

bool statusHasLabel(const QVector<IrcStatusEntry> &entries, const QString &label)
{
    for (const IrcStatusEntry &entry : entries) {
        if (entry.label() == label)
            return true;
    }
    return false;
}

bool statusAnyFieldContains(const QVector<IrcStatusEntry> &entries, const QString &needle)
{
    for (const IrcStatusEntry &entry : entries) {
        if (entry.text().contains(needle, Qt::CaseInsensitive)
            || entry.label().contains(needle, Qt::CaseInsensitive)
            || entry.networkId().contains(needle, Qt::CaseInsensitive)) {
            return true;
        }
    }
    return false;
}

QString saslPlainPayloadBase64(const QString &account, const QString &secret)
{
    QByteArray plain;
    const QByteArray accountBytes = account.toUtf8();
    const QByteArray secretBytes = secret.toUtf8();
    plain.append(accountBytes);
    plain.append('\0');
    plain.append(accountBytes);
    plain.append('\0');
    plain.append(secretBytes);
    return QString::fromLatin1(plain.toBase64());
}

bool sawSelfPrivmsgTo(const QVector<IrcMessage> &incoming,
                      const QString &selfNick,
                      const QString &target,
                      const QString &body)
{
    for (const IrcMessage &message : incoming) {
        if (message.command != "PRIVMSG" || message.parameters.size() < 2)
            continue;
        if (!message.prefix
            || !sameFolded(messageText(message.prefix->nick), selfNick)) {
            continue;
        }
        if (!sameFolded(messageText(message.parameters[0]), target))
            continue;
        if (messageText(message.parameters.back()) == body)
            return true;
    }
    return false;
}

bool sawPrivmsgFrom(const QVector<IrcMessage> &incoming,
                    const QString &sender,
                    const QString &target)
{
    for (const IrcMessage &message : incoming) {
        if (message.command != "PRIVMSG" || message.parameters.empty())
            continue;
        if (!message.prefix
            || !sameFolded(messageText(message.prefix->nick), sender)) {
            continue;
        }
        if (!sameFolded(messageText(message.parameters[0]), target))
            continue;
        return true;
    }
    return false;
}

bool sawNoticeFrom(const QVector<IrcMessage> &incoming,
                   const QString &sender,
                   const QString &target)
{
    for (const IrcMessage &message : incoming) {
        if (message.command != "NOTICE" || message.parameters.empty())
            continue;
        if (!message.prefix
            || !sameFolded(messageText(message.prefix->nick), sender)) {
            continue;
        }
        if (!sameFolded(messageText(message.parameters[0]), target))
            continue;
        return true;
    }
    return false;
}

bool channelTranscriptContains(QAbstractItemModel *messages, const QString &body)
{
    if (!messages)
        return false;
    for (int row = 0; row < messages->rowCount(); ++row) {
        if (messages->index(row, 0).data(MessageListModel::BodyRole).toString() == body)
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

void LiveIrcdTest::chatHistoryOnJoin()
{
    const LiveDaemonInfo *daemon = liveDaemon(QStringLiteral("ergo"));
    if (!daemon)
        QSKIP("Ergo is not in this live profile");
    const bool tls = daemon->plainPort == 0;
    const quint16 port = tls ? daemon->tlsPort : daemon->plainPort;
    const QString channel = uniqueChannel();
    const QString seed = QStringLiteral("history-seed-%1").arg(channel);

    RawIrcPeer peer(liveHost(), port, tls, liveSslConfiguration(),
                    uniqueNick(daemon->nickLength));
    QVERIFY(peer.waitRegistered());
    QVERIFY(peer.join(channel));
    peer.writeLine(QStringLiteral("PRIVMSG %1 :%2").arg(channel, seed));
    // The server answers in the order it received, so its PONG proves the seed
    // above is already in the channel's history.
    peer.writeLine(QStringLiteral("PING :history-seed"));
    QVERIFY(peer.waitForCommand(QStringLiteral("PONG")));

    LiveClient client(*daemon, uniqueNick(daemon->nickLength), tls);
    QVERIFY(client.waitRegistered());
    QVERIFY(client.session->capabilities().contains(IrcCapability::ChatHistory)
            || waitUntil([&] {
                   return client.session->capabilities().contains(
                       IrcCapability::ChatHistory);
               }));
    QVERIFY(joinChannel(client, channel));
    client.selectChannel(channel);
    QVERIFY(waitUntil([&] {
        QAbstractItemModel *messages = client.controller.messages();
        for (int row = 0; row < messages->rowCount(); ++row) {
            if (messages->index(row, 0).data(MessageListModel::BodyRole).toString()
                == seed) {
                return true;
            }
        }
        return false;
    }));

    int seedRow = -1;
    QAbstractItemModel *messages = client.controller.messages();
    for (int row = 0; row < messages->rowCount(); ++row) {
        if (messages->index(row, 0).data(MessageListModel::BodyRole).toString()
            == seed) {
            seedRow = row;
            break;
        }
    }
    QVERIFY(seedRow >= 0);
    QCOMPARE(client.transcriptRole(seedRow, MessageListModel::BodyRole).toString(),
             seed);
    QCOMPARE(client.transcriptRole(seedRow, MessageListModel::OriginRole).toString(),
             QStringLiteral("replay"));
    const QString time = client.transcriptRole(seedRow, MessageListModel::TimeRole)
                             .toString();
    QVERIFY(!time.isEmpty());
    QCOMPARE(client.controller.unreadCountFor(client.config.networkId), 0);
    for (const IrcStatusEntry &entry : client.status) {
        QVERIFY(entry.label() != QLatin1String("CHATHISTORY"));
        QVERIFY(entry.label() != QLatin1String("PING"));
        QVERIFY(entry.label() != QLatin1String("PONG"));
        QVERIFY(entry.label() != QLatin1String("JOIN"));
        QVERIFY(!entry.text().contains(seed));
    }
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
        auto *conversations = guest.controller.conversations();
        const int notices = messageCommandCount(guest.incoming, QStringLiteral("NOTICE"));
        QVERIFY(guest.session->sendPrivmsg(
            QStringLiteral("NickServ"),
            QStringLiteral("REGISTER %1 live@omairc.test").arg(password)));
        QVERIFY(waitUntil([&] {
            return messageCommandCount(guest.incoming, QStringLiteral("NOTICE")) > notices;
        }));
        QCOMPARE(conversationRow(conversations, QStringLiteral("NickServ")), -1);
        QVERIFY(noStatusTextContains(guest.status, password));
    }
    LiveClient authed(*daemon, nick, daemon->plainPort == 0, password);
    QVERIFY(authed.waitServerLabel(QStringLiteral("903")));
    QVERIFY2(authed.waitRegistered(), qPrintable(authed.lastError));
    authed.controller.openStatus(authed.config.networkId);
    const QString encodedAuth = saslPlainPayloadBase64(nick, password);
    QCOMPARE(conversationRow(authed.controller.conversations(), QStringLiteral("NickServ")),
             -1);
    QVERIFY(!statusHasLabel(authed.status, QStringLiteral("AUTHENTICATE")));
    QVERIFY(!statusAnyFieldContains(authed.status, QStringLiteral("AUTHENTICATE")));
    QVERIFY(noStatusTextContains(authed.status, password));
    QVERIFY(noStatusTextContains(authed.status, encodedAuth));
    QVERIFY(!logContains(authed.controller.console()->lines(),
                         QStringLiteral("AUTHENTICATE")));
    QVERIFY(!logContains(authed.controller.console()->lines(), password));
    QVERIFY(!logContains(authed.controller.console()->lines(), encodedAuth));
}

void LiveIrcdTest::conversationInventionMatrix()
{
    const LiveDaemonInfo *daemon = liveDaemon(QStringLiteral("ergo"));
    if (!daemon)
        QSKIP("Ergo is not in this live profile");
    const bool tls = daemon->plainPort == 0;
    const quint16 port = tls ? daemon->tlsPort : daemon->plainPort;
    const QString peerNick = uniqueNick(daemon->nickLength);
    const QString targetNick = uniqueNick(daemon->nickLength);
    const QString dmBody = QStringLiteral("hello");
    const QString peerBody = QStringLiteral("hi");

    LiveClient client(*daemon, uniqueNick(daemon->nickLength), tls);
    QVERIFY(client.waitRegistered());
    QVERIFY(client.session->capabilities().contains(IrcCapability::EchoMessage)
            || waitUntil([&] {
                   return client.session->capabilities().contains(
                       IrcCapability::EchoMessage);
               }));
    client.controller.openStatus(client.config.networkId);

    const QString channel = uniqueChannel();
    QVERIFY(joinChannel(client, channel));
    QVERIFY(waitUntil([&] {
        return messageHasCommand(client.incoming, QStringLiteral("366"));
    }));
    client.selectChannel(channel);
    QCOMPARE(client.controller.selectedTarget(), channel);

    auto *conversations = client.controller.conversations();
    RawIrcPeer msgTarget(liveHost(), port, tls, liveSslConfiguration(), targetNick);
    QVERIFY(msgTarget.waitRegistered());
    QVERIFY(client.controller.sendMessage(
        QStringLiteral("/msg %1 %2").arg(targetNick, dmBody)));
    QCOMPARE(conversationRow(conversations, targetNick), -1);
    QCOMPARE(client.controller.selectedTarget(), channel);

    QVERIFY(waitUntil([&] {
        return sawSelfPrivmsgTo(client.incoming, client.session->nick(), targetNick, dmBody);
    }));
    QCOMPARE(conversationRow(conversations, targetNick), -1);
    QCOMPARE(client.controller.selectedTarget(), channel);
    QVERIFY(!channelTranscriptContains(client.controller.messages(), dmBody));

    QVERIFY(client.session->sendPrivmsg(QStringLiteral("NickServ"), QStringLiteral("HELP")));
    QVERIFY(waitUntil([&] {
        return sawNoticeFrom(client.incoming,
                             QStringLiteral("NickServ"),
                             client.session->nick());
    }));
    QCOMPARE(conversationRow(conversations, QStringLiteral("NickServ")), -1);
    QCOMPARE(conversationRow(conversations, QStringLiteral("nickserv")), -1);
    QVERIFY(logContains(client.controller.console()->lines(), QStringLiteral("NickServ"))
            || statusTextContains(client.status, QStringLiteral("NickServ")));

    RawIrcPeer peer(liveHost(), port, tls, liveSslConfiguration(), peerNick);
    QVERIFY(peer.waitRegistered());
    peer.writeLine(QStringLiteral("PRIVMSG %1 :%2")
                       .arg(client.session->nick(), peerBody));
    QVERIFY(waitUntil([&] {
        return conversationRow(conversations, peerNick) >= 0;
    }));
    QCOMPARE(client.controller.selectedTarget(), channel);
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
    const bool folded = a.features().caseMapping().equals(
        a.session->nick().toStdString(), nickB.toStdString());
    if (!folded) {
        QVERIFY(b.waitRegistered());
        return;
    }
    if (b.waitRegistered())
        QSKIP("daemon advertised a folding casemap but accepted both nicks");
    QVERIFY(b.waitFailed());
}

void LiveIrcdTest::asciiDirectRestore_data()
{
    QTest::addColumn<QString>("daemonName");
    for (const LiveDaemonInfo &daemon : liveDaemons()) {
        if (daemon.mapping == IrcCaseMapping::Kind::Ascii
            && daemon.nickLength >= 8) {
            QTest::newRow(qPrintable(daemon.name)) << daemon.name;
        }
    }
}

void LiveIrcdTest::asciiDirectRestore()
{
    QFETCH(QString, daemonName);
    const LiveDaemonInfo *daemon = liveDaemon(daemonName);
    QVERIFY(daemon);
    QCOMPARE(daemon->mapping, IrcCaseMapping::Kind::Ascii);

    QTemporaryDir configHome;
    QVERIFY(configHome.isValid());
    TestSettings::isolate(configHome.path());
    QCoreApplication::setOrganizationName(QStringLiteral("omairc"));
    QCoreApplication::setApplicationName(QStringLiteral("omairc"));

    QTemporaryDir logs;
    QVERIFY(logs.isValid());
    const QString networkId = QStringLiteral("ascii-dm");
    const bool tls = daemon->plainPort == 0;
    const QString selfNick = uniqueNick(daemon->nickLength);
    const QString peerNick = uniqueNick(daemon->nickLength - 3) + QStringLiteral("[m]");

    LiveClient peer(*daemon, peerNick, tls);
    QVERIFY2(peer.waitRegistered(), qPrintable(peer.lastError));

    {
        LiveClient self(*daemon, selfNick, tls, {}, networkId, logs.path());
        QVERIFY2(self.waitRegistered(), qPrintable(self.lastError));
        QVERIFY(self.waitMotd());
        QCOMPARE(self.features().caseMapping().kind(), IrcCaseMapping::Kind::Ascii);
        QVERIFY(peer.session->sendPrivmsg(self.session->nick(),
                                          QStringLiteral("hi there")));
        QVERIFY(waitUntil([&] {
            return conversationRow(self.controller.conversations(), peerNick) >= 0;
        }));
        self.controller.selectConversation(networkId, peerNick);
        QVERIFY(self.controller.sendMessage(QStringLiteral("hello back")));
        QVERIFY(waitUntil([&] {
            auto *messages = self.controller.messages();
            for (int row = 0; row < messages->rowCount(); ++row) {
                if (messages->index(row, 0).data(MessageListModel::BodyRole).toString()
                    == QStringLiteral("hello back")) {
                    return true;
                }
            }
            return false;
        }));
    }

    LiveClient reloaded(*daemon, selfNick, tls, {}, networkId, logs.path());
    QVERIFY2(reloaded.waitRegistered(), qPrintable(reloaded.lastError));
    QVERIFY(reloaded.waitMotd());
    auto *conversations = reloaded.controller.conversations();
    QCOMPARE(conversationRow(conversations, peerNick) >= 0, true);
    int rows = 0;
    for (int row = 0; row < conversations->rowCount(); ++row) {
        if (sameFolded(conversations->index(row, 0)
                           .data(ConversationListModel::ConversationRole)
                           .toString(),
                       peerNick)) {
            ++rows;
        }
    }
    QCOMPARE(rows, 1);
    QVERIFY(peer.session->sendPrivmsg(reloaded.session->nick(),
                                      QStringLiteral("second")));
    QVERIFY(waitUntil([&] {
        auto *messages = reloaded.controller.messages();
        reloaded.controller.selectConversation(networkId, peerNick);
        for (int row = 0; row < messages->rowCount(); ++row) {
            if (messages->index(row, 0).data(MessageListModel::BodyRole).toString()
                == QStringLiteral("second")) {
                return true;
            }
        }
        return false;
    }));
    rows = 0;
    for (int row = 0; row < conversations->rowCount(); ++row) {
        if (sameFolded(conversations->index(row, 0)
                           .data(ConversationListModel::ConversationRole)
                           .toString(),
                       peerNick)) {
            ++rows;
        }
    }
    QCOMPARE(rows, 1);
    reloaded.controller.selectConversation(networkId, peerNick);
    auto *messages = reloaded.controller.messages();
    bool sawHi = false;
    bool sawHello = false;
    for (int row = 0; row < messages->rowCount(); ++row) {
        const QString body =
            messages->index(row, 0).data(MessageListModel::BodyRole).toString();
        if (body == QStringLiteral("hi there"))
            sawHi = true;
        if (body == QStringLiteral("hello back"))
            sawHello = true;
    }
    QVERIFY(sawHi);
    QVERIFY(sawHello);
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
    client.controller.openStatus(client.config.networkId);
    const QString first = uniqueChannel();
    const QString second = uniqueChannel();
    QVERIFY(client.controller.console()->submit(
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
