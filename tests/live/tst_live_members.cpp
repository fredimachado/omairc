#include "liveharness.h"
#include "livepeer.h"
#include "ircjointarget.h"
#include "ircserverfeatures.h"
#include "memberlistmodel.h"

#include <QAbstractItemModel>
#include <QCoreApplication>
#include <QHash>
#include <QTest>

class LiveMembersTest : public QObject
{
    Q_OBJECT

private slots:
    void memberPrefixRanks_data();
    void memberPrefixRanks();
};

namespace
{
void fillDaemonRows()
{
    for (const LiveDaemonInfo &daemon : liveDaemons())
        QTest::newRow(qPrintable(daemon.name)) << daemon.name;
}

bool joinChannel(LiveClient &client, const QString &channel)
{
    const std::optional<IrcJoinTarget> target = IrcJoinTarget::make(channel);
    return target && client.session->join(*target);
}

QStringList memberNicks(LiveClient &client)
{
    QStringList nicks;
    QAbstractItemModel *model = client.controller.members();
    if (!model)
        return nicks;
    for (int row = 0; row < model->rowCount(); ++row) {
        nicks.append(
            model->index(row, 0).data(MemberListModel::NickRole).toString());
    }
    return nicks;
}

QStringList memberLabels(LiveClient &client)
{
    QStringList labels;
    QAbstractItemModel *model = client.controller.members();
    if (!model)
        return labels;
    for (int row = 0; row < model->rowCount(); ++row) {
        labels.append(
            model->index(row, 0).data(MemberListModel::LabelRole).toString());
    }
    return labels;
}

IrcPrefixSet ranksFromMode(const IrcServerFeatures &features, char modeLetter)
{
    const std::string_view modes = features.prefixModes();
    const std::string_view symbols = features.prefixSymbols();
    for (std::size_t index = 0; index < modes.size(); ++index) {
        if (modes[index] != modeLetter)
            continue;
        std::string token;
        token.push_back(symbols[index]);
        token.push_back('x');
        const auto parsed = features.parseNamesToken(token);
        return parsed ? parsed->ranks : IrcPrefixSet{};
    }
    return {};
}

QString expectedLabel(const IrcServerFeatures &features,
                      const QString &nick,
                      const IrcPrefixSet &ranks)
{
    return QString::fromStdString(
        features.memberLabel(ranks, nick.toStdString()));
}

QStringList expectedOrder(const IrcServerFeatures &features,
                          const QVector<QPair<QString, IrcPrefixSet>> &members)
{
    QVector<QPair<QString, int>> ranked;
    ranked.reserve(members.size());
    for (const auto &member : members) {
        ranked.append({member.first, features.rankPriority(member.second)});
    }
    std::sort(ranked.begin(), ranked.end(),
              [](const QPair<QString, int> &left, const QPair<QString, int> &right) {
                  if (left.second != right.second)
                      return left.second < right.second;
                  return left.first.compare(right.first, Qt::CaseInsensitive) < 0;
              });
    QStringList ordered;
    for (const auto &entry : ranked)
        ordered.append(entry.first);
    return ordered;
}

QStringList expectedLabels(const IrcServerFeatures &features,
                           const QVector<QPair<QString, IrcPrefixSet>> &members)
{
    QStringList labels;
    const QStringList order = expectedOrder(features, members);
    QHash<QString, IrcPrefixSet> ranksByNick;
    for (const auto &member : members)
        ranksByNick.insert(member.first, member.second);
    for (const QString &nick : order)
        labels.append(expectedLabel(features, nick, ranksByNick.value(nick)));
    return labels;
}

bool memberListMatches(LiveClient &client,
                       const IrcServerFeatures &features,
                       const QVector<QPair<QString, IrcPrefixSet>> &members)
{
    return memberNicks(client) == expectedOrder(features, members)
        && memberLabels(client) == expectedLabels(features, members);
}

char highestPrefixMode(const IrcServerFeatures &features)
{
    const std::string_view modes = features.prefixModes();
    return modes.empty() ? '\0' : modes.front();
}

char voicePrefixMode(const IrcServerFeatures &features)
{
    const std::string_view modes = features.prefixModes();
    if (modes.size() < 2)
        return '\0';
    return modes.back();
}
}

void LiveMembersTest::memberPrefixRanks_data()
{
    QTest::addColumn<QString>("daemonName");
    fillDaemonRows();
}

void LiveMembersTest::memberPrefixRanks()
{
    QFETCH(QString, daemonName);
    const LiveDaemonInfo *daemon = liveDaemon(daemonName);
    if (!daemon)
        QSKIP("Daemon is not in this live profile");

    const bool tls = daemon->plainPort == 0;
    const quint16 port = tls ? daemon->tlsPort : daemon->plainPort;
    LiveClient client(*daemon, uniqueNick(daemon->nickLength), tls);
    QVERIFY(client.waitRegistered());
    QVERIFY(waitUntil([&] { return !client.features().prefixModes().empty(); }));

    const IrcServerFeatures &features = client.features();
    const char opMode = highestPrefixMode(features);
    const char voiceMode = voicePrefixMode(features);
    if (opMode == '\0' || voiceMode == '\0')
        QSKIP("Daemon PREFIX is too small for +o/+v rank checks");

    const QString channel = uniqueChannel();
    QVERIFY(joinChannel(client, channel));
    QVERIFY(waitUntil([&] {
        return messageHasCommand(client.incoming, QStringLiteral("366"));
    }));
    client.selectChannel(channel);

    RawIrcPeer peerOp(liveHost(), port, tls, liveSslConfiguration(),
                      uniqueNick(daemon->nickLength));
    RawIrcPeer peerVoice(liveHost(), port, tls, liveSslConfiguration(),
                         uniqueNick(daemon->nickLength));
    QVERIFY(peerOp.waitRegistered());
    QVERIFY(peerVoice.waitRegistered());
    QVERIFY(peerOp.join(channel));
    QVERIFY(peerVoice.join(channel));
    QVERIFY(waitUntil([&] {
        return client.memberRole(peerOp.nick, MemberListModel::NickRole).isValid()
            && client.memberRole(peerVoice.nick, MemberListModel::NickRole).isValid();
    }));

    QVERIFY(client.controller.sendMessage(
        QStringLiteral("/mode %1 +%2 %3").arg(channel, QChar(opMode), peerOp.nick)));
    QVERIFY(waitUntil([&] {
        const IrcPrefixSet ranks = ranksFromMode(features, opMode);
        return client.memberRole(peerOp.nick, MemberListModel::LabelRole).toString()
            == expectedLabel(features, peerOp.nick, ranks);
    }));

    peerOp.mode(channel,
               QStringLiteral("+%1").arg(QChar(voiceMode)),
               peerVoice.nick);
    QVERIFY(waitUntil([&] {
        const IrcPrefixSet ranks = ranksFromMode(features, voiceMode);
        return client.memberRole(peerVoice.nick, MemberListModel::LabelRole).toString()
            == expectedLabel(features, peerVoice.nick, ranks);
    }));

    peerOp.mode(channel, QStringLiteral("+%1").arg(QChar(opMode)), peerVoice.nick);
    QVERIFY(waitUntil([&] {
        const IrcPrefixSet ranks = ranksFromMode(features, opMode);
        return client.memberRole(peerVoice.nick, MemberListModel::LabelRole).toString()
            == expectedLabel(features, peerVoice.nick, ranks);
    }));

    QVERIFY(client.controller.sendMessage(
        QStringLiteral("/mode %1 -%2 %3")
            .arg(channel, QChar(opMode), client.config.nick)));
    QVERIFY(waitUntil([&] {
        const QVector<QPair<QString, IrcPrefixSet>> members{
            {peerOp.nick, ranksFromMode(features, opMode)},
            {peerVoice.nick, ranksFromMode(features, opMode)},
            {client.config.nick, {}},
        };
        return memberListMatches(client, features, members);
    }));
}

int runLiveMembersTests(int argc, char **argv)
{
    LiveMembersTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_live_members.moc"
