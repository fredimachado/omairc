#include <QSignalSpy>
#include <QTest>

#include "ircchannellistrequest.h"

class ChannelListRequestTest : public QObject
{
    Q_OBJECT
private slots:
    void cachePendingAndIsolation();
    void rowsTimeoutRetryAndDrain();
};

void ChannelListRequestTest::cachePendingAndIsolation()
{
    IrcChannelListRequest lists;
    QCOMPARE(lists.request("a", "#a*", false), IrcChannelListRequest::Request::Start);
    QCOMPARE(lists.request("a", "#b*", false), IrcChannelListRequest::Request::Loading);
    QCOMPARE(lists.request("b", {}, false), IrcChannelListRequest::Request::Start);
    const auto firstFinish = lists.finish("a");
    QVERIFY(firstFinish.completed);
    QVERIFY(firstFinish.pendingMask);
    QCOMPARE(*firstFinish.pendingMask, QStringLiteral("#b*"));
    QCOMPARE(lists.request("a", "#b*", true), IrcChannelListRequest::Request::Start);
    lists.finish("a");
    QCOMPARE(lists.request("a", " #B* ", false), IrcChannelListRequest::Request::Cached);
    QVERIFY(lists.state("b")->loading);
    lists.forget("b");
    QVERIFY(!lists.state("b"));
}

void ChannelListRequestTest::rowsTimeoutRetryAndDrain()
{
    IrcChannelListRequest lists;
    QSignalSpy rows(&lists, &IrcChannelListRequest::rowChanged);
    QSignalSpy timeout(&lists, &IrcChannelListRequest::timedOut);
    lists.request("a", {}, false);
    lists.row("a", {"#one", 1, "old"});
    lists.row("a", {"#ONE", 2, "new"});
    QCOMPARE(rows.size(), 2);
    QCOMPARE(lists.state("a")->rows.size(), 1);
    QCOMPARE(lists.state("a")->rows.first().users, 2);
    for (int i = 1; i <= ChannelListModel::kMaxRows; ++i)
        lists.row("a", {QStringLiteral("#%1").arg(i), i, {}});
    QCOMPARE(lists.state("a")->rows.size(), ChannelListModel::kMaxRows);
    lists.fireIdleTimeout("a");
    QCOMPARE(timeout.size(), 1);
    QVERIFY(!lists.state("a")->loading);
    QCOMPARE(lists.request("a", {}, true), IrcChannelListRequest::Request::Start);
    QVERIFY(!lists.finish("a").completed); // drains the timed-out request's late END
    QVERIFY(lists.state("a")->loading);
    lists.row("a", {"#two", 3, {}});
    QVERIFY(lists.finish("a").completed);
    QVERIFY(lists.state("a")->complete);
    QVERIFY(lists.fail("a", "ignored") == false);
}

int runChannelListRequestTests(int argc, char **argv)
{
    ChannelListRequestTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_channellistrequest.moc"
