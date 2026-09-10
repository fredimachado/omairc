#include "liveharness.h"
#include "liveuiworld.h"

#include <QDebug>
#include <QSet>
#include <QTest>

class LiveUiTest : public QObject
{
    Q_OBJECT

private slots:
    void initTestCase();
    void cleanup();
    void cleanupTestCase();

    void ready();
    void duplicateChannelTopicsAndMembers();
    void statusIsolation();
    void inboundSameNickDirectsStayIsolated();
    void outboundSameNickDirectsStayIsolated();

private:
    bool check(bool ok) const;
    bool sameMembers(const QStringList &visible, const QSet<QString> &want) const;

    LiveUiWorld m_world;
};

void LiveUiTest::initTestCase()
{
    if (qEnvironmentVariableIsEmpty("OMAIRC_LIVE_DAEMONS"))
        QSKIP("Run bin/test-live. live_ui_tests needs the compose runner.");

    int plain = 0;
    for (const LiveDaemonInfo &daemon : liveDaemons()) {
        if (daemon.plainPort)
            ++plain;
    }
    if (plain < 2)
        QSKIP("Need two plain-port live daemons.");

    QVERIFY2(m_world.open(), qPrintable(m_world.dump()));
}

void LiveUiTest::cleanup()
{
    if (!QTest::currentTestFailed())
        return;
    m_world.saveShot(QStringLiteral("fail-%1").arg(
        QString::fromLatin1(QTest::currentTestFunction())));
}

void LiveUiTest::cleanupTestCase()
{
    m_world.close();
}

bool LiveUiTest::check(bool ok) const
{
    if (!ok)
        qWarning().noquote() << m_world.dump();
    return ok;
}

bool LiveUiTest::sameMembers(const QStringList &visible, const QSet<QString> &want) const
{
    return QSet<QString>(visible.begin(), visible.end()) == want;
}

void LiveUiTest::ready()
{
    const LiveUiSeat &left = m_world.left();
    const LiveUiSeat &right = m_world.right();
    QVERIFY2(check(m_world.click(left.channelId())), qPrintable(m_world.dump()));
    QVERIFY2(check(m_world.click(right.channelId())), qPrintable(m_world.dump()));
    QVERIFY2(check(m_world.saveShot(QStringLiteral("live-ui-ready"))),
             qPrintable(m_world.dump()));
}

void LiveUiTest::duplicateChannelTopicsAndMembers()
{
    const LiveUiSeat &left = m_world.left();
    const LiveUiSeat &right = m_world.right();

    QVERIFY2(check(m_world.click(left.channelId())), qPrintable(m_world.dump()));
    QVERIFY2(m_world.visibleTopic() == left.topic, qPrintable(m_world.dump()));
    QVERIFY2(m_world.visibleFooterNick() == left.clientNick, qPrintable(m_world.dump()));
    QVERIFY2(sameMembers(m_world.visibleMemberNicks(), left.channelMembers()),
             qPrintable(m_world.dump()));
    QVERIFY2(m_world.visiblePeopleHeading()
                 == QStringLiteral("ONLINE - %1").arg(left.channelMembers().size()),
             qPrintable(m_world.dump()));
    QVERIFY2(!m_world.visibleMemberNicks().contains(right.extraNick),
             qPrintable(m_world.dump()));
    QVERIFY2(check(m_world.saveShot(QStringLiteral("live-ui-channel-left"))),
             qPrintable(m_world.dump()));

    QVERIFY2(check(m_world.click(right.channelId())), qPrintable(m_world.dump()));
    QVERIFY2(m_world.visibleTopic() == right.topic, qPrintable(m_world.dump()));
    QVERIFY2(m_world.visibleFooterNick() == right.clientNick, qPrintable(m_world.dump()));
    QVERIFY2(sameMembers(m_world.visibleMemberNicks(), right.channelMembers()),
             qPrintable(m_world.dump()));
    QVERIFY2(m_world.visiblePeopleHeading()
                 == QStringLiteral("ONLINE - %1").arg(right.channelMembers().size()),
             qPrintable(m_world.dump()));
    QVERIFY2(!m_world.visibleMemberNicks().contains(left.extraNick),
             qPrintable(m_world.dump()));
    QVERIFY2(check(m_world.saveShot(QStringLiteral("live-ui-channel-right"))),
             qPrintable(m_world.dump()));
}

void LiveUiTest::statusIsolation()
{
    const LiveUiSeat &left = m_world.left();
    const LiveUiSeat &right = m_world.right();

    QVERIFY2(check(m_world.click(left.statusId())), qPrintable(m_world.dump()));
    QVERIFY2(m_world.visibleConsoleTexts().contains(left.statusMark),
             qPrintable(m_world.dump()));
    QVERIFY2(!m_world.visibleConsoleTexts().contains(right.statusMark),
             qPrintable(m_world.dump()));
    QVERIFY2(m_world.visibleStatusNetworkId() == left.networkId,
             qPrintable(m_world.dump()));
    QVERIFY2(check(m_world.saveShot(QStringLiteral("live-ui-status-left"))),
             qPrintable(m_world.dump()));

    QVERIFY2(check(m_world.click(right.statusId())), qPrintable(m_world.dump()));
    QVERIFY2(m_world.visibleConsoleTexts().contains(right.statusMark),
             qPrintable(m_world.dump()));
    QVERIFY2(!m_world.visibleConsoleTexts().contains(left.statusMark),
             qPrintable(m_world.dump()));
    QVERIFY2(m_world.visibleStatusNetworkId() == right.networkId,
             qPrintable(m_world.dump()));
    QVERIFY2(check(m_world.saveShot(QStringLiteral("live-ui-status-right"))),
             qPrintable(m_world.dump()));
}

void LiveUiTest::inboundSameNickDirectsStayIsolated()
{
    const LiveUiSeat &left = m_world.left();
    const LiveUiSeat &right = m_world.right();

    QVERIFY2(check(m_world.peerPrivmsg(left, left.clientNick, left.inboundMark)),
             qPrintable(m_world.dump()));
    QVERIFY2(check(m_world.click(left.peerDirectId())), qPrintable(m_world.dump()));
    QVERIFY2(m_world.visibleBodies().contains(left.inboundMark),
             qPrintable(m_world.dump()));
    QVERIFY2(!m_world.visibleBodies().contains(right.inboundMark),
             qPrintable(m_world.dump()));
    QVERIFY2(check(m_world.saveShot(QStringLiteral("live-ui-dm-in-left"))),
             qPrintable(m_world.dump()));

    QVERIFY2(check(m_world.peerPrivmsg(right, right.clientNick, right.inboundMark)),
             qPrintable(m_world.dump()));
    QVERIFY2(check(m_world.click(right.peerDirectId())), qPrintable(m_world.dump()));
    QVERIFY2(m_world.visibleBodies().contains(right.inboundMark),
             qPrintable(m_world.dump()));
    QVERIFY2(!m_world.visibleBodies().contains(left.inboundMark),
             qPrintable(m_world.dump()));
    QVERIFY2(check(m_world.saveShot(QStringLiteral("live-ui-dm-in-right"))),
             qPrintable(m_world.dump()));

    QVERIFY2(check(m_world.click(left.peerDirectId())), qPrintable(m_world.dump()));
    QVERIFY2(m_world.visibleBodies().contains(left.inboundMark),
             qPrintable(m_world.dump()));
    QVERIFY2(!m_world.visibleBodies().contains(right.inboundMark),
             qPrintable(m_world.dump()));
}

void LiveUiTest::outboundSameNickDirectsStayIsolated()
{
    const LiveUiSeat &left = m_world.left();
    const LiveUiSeat &right = m_world.right();

    QVERIFY2(check(m_world.click(left.channelId())), qPrintable(m_world.dump()));
    QVERIFY2(check(m_world.clickMember(left.peerNick)), qPrintable(m_world.dump()));
    QVERIFY2(m_world.selectedConversationId() == left.peerDirectId().wire(),
             qPrintable(m_world.dump()));
    QVERIFY2(check(m_world.typeAndSend(left.outboundMark)), qPrintable(m_world.dump()));
    QVERIFY2(m_world.visibleBodies().contains(left.outboundMark),
             qPrintable(m_world.dump()));
    QVERIFY2(check(m_world.saveShot(QStringLiteral("live-ui-dm-out-left"))),
             qPrintable(m_world.dump()));

    QVERIFY2(check(m_world.click(right.channelId())), qPrintable(m_world.dump()));
    QVERIFY2(check(m_world.clickMember(right.peerNick)), qPrintable(m_world.dump()));
    QVERIFY2(m_world.selectedConversationId() == right.peerDirectId().wire(),
             qPrintable(m_world.dump()));
    QVERIFY2(check(m_world.typeAndSend(right.outboundMark)), qPrintable(m_world.dump()));
    QVERIFY2(m_world.visibleBodies().contains(right.outboundMark),
             qPrintable(m_world.dump()));
    QVERIFY2(!m_world.visibleBodies().contains(left.outboundMark),
             qPrintable(m_world.dump()));
    QVERIFY2(check(m_world.saveShot(QStringLiteral("live-ui-dm-out-right"))),
             qPrintable(m_world.dump()));

    QVERIFY2(check(m_world.click(left.peerDirectId())), qPrintable(m_world.dump()));
    QVERIFY2(m_world.visibleBodies().contains(left.outboundMark),
             qPrintable(m_world.dump()));
    QVERIFY2(!m_world.visibleBodies().contains(right.outboundMark),
             qPrintable(m_world.dump()));
}

int runLiveUiTests(int argc, char **argv)
{
    LiveUiTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_live_ui.moc"
