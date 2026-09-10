#include "backend.h"
#include "fakeirctransport.h"
#include "irccontroller.h"
#include "ircsession.h"
#include "ircslashcomplete.h"
#include "liveharness.h"
#include "liveuiworld.h"
#include "messagelistmodel.h"

#include <QColor>
#include <QCoreApplication>
#include <QDebug>
#include <QDir>
#include <QFileInfo>
#include <QImage>
#include <QQmlApplicationEngine>
#include <QQmlComponent>
#include <QQuickItem>
#include <QQuickWindow>
#include <QSet>
#include <QSettings>
#include <QTemporaryDir>
#include <QTest>
#include <QVariantMap>

#include <memory>

namespace
{
IrcSessionConfig fixtureConfig()
{
    IrcSessionConfig value;
    value.networkId = QStringLiteral("libera");
    value.host = QStringLiteral("irc.example");
    value.tlsEnabled = true;
    value.nick = QStringLiteral("omairc");
    value.username = QStringLiteral("omairc");
    value.realname = QStringLiteral("Omairc User");
    value.autojoinChannels = {QStringLiteral("#omarchy")};
    value.reconnectEnabled = false;
    return value;
}

int chromeIndex(const QVector<TranscriptRowChrome> &rows, const QString &body)
{
    for (int index = 0; index < rows.size(); ++index) {
        if (rows.at(index).body == body)
            return index;
    }
    return -1;
}

QString describeChrome(const QVector<TranscriptRowChrome> &rows)
{
    QStringList parts;
    for (const TranscriptRowChrome &row : rows) {
        parts.append(QStringLiteral("%1 a=%2 h=%3 ht=%4")
                         .arg(row.body)
                         .arg(row.avatarVisible)
                         .arg(row.headerVisible)
                         .arg(row.height));
    }
    return parts.join(QLatin1Char('|'));
}

QString fixtureArtifactDir()
{
    const QByteArray override = qgetenv("OMAIRC_LIVE_UI_ARTIFACTS");
    if (!override.isEmpty())
        return QString::fromLocal8Bit(override);
    return QDir(QCoreApplication::applicationDirPath()
                + QLatin1String("/../test-artifacts/live-ui"))
        .absolutePath();
}

bool saveFixtureShot(QQuickWindow *window, const QString &stem)
{
    if (!window)
        return false;
    QCoreApplication::processEvents();
    const QImage image = window->grabWindow();
    if (image.isNull())
        return false;
    const QString dir = fixtureArtifactDir();
    QDir().mkpath(dir);
    const QString path = dir + QLatin1Char('/') + stem + QLatin1String(".png");
    return image.save(path) && QFileInfo(path).size() > 0;
}
}

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
    void consecutiveSameAuthorMinuteGroupsThroughIrcEvent();
    void replayAndLiveSameAuthorMinuteDoNotGroupThroughIrcEvent();

private:
    bool check(bool ok) const;
    bool sameMembers(const QStringList &visible, const QSet<QString> &want) const;

    LiveUiWorld m_world;
    bool m_live = false;
};

void LiveUiTest::initTestCase()
{
    if (qEnvironmentVariableIsEmpty("OMAIRC_LIVE_DAEMONS"))
        return;

    int plain = 0;
    for (const LiveDaemonInfo &daemon : liveDaemons()) {
        if (daemon.plainPort)
            ++plain;
    }
    if (plain < 2)
        return;

    QVERIFY2(m_world.open(), qPrintable(m_world.dump()));
    m_live = true;
}

void LiveUiTest::cleanup()
{
    if (!QTest::currentTestFailed() || !m_live)
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
    if (!m_live)
        QSKIP("Run bin/test-live. live_ui_tests needs the compose runner.");
    const LiveUiSeat &left = m_world.left();
    const LiveUiSeat &right = m_world.right();
    QVERIFY2(check(m_world.click(left.channelId())), qPrintable(m_world.dump()));
    QVERIFY2(check(m_world.click(right.channelId())), qPrintable(m_world.dump()));
    QVERIFY2(check(m_world.saveShot(QStringLiteral("live-ui-ready"))),
             qPrintable(m_world.dump()));
}

void LiveUiTest::duplicateChannelTopicsAndMembers()
{
    if (!m_live)
        QSKIP("Run bin/test-live. live_ui_tests needs the compose runner.");
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
    if (!m_live)
        QSKIP("Run bin/test-live. live_ui_tests needs the compose runner.");
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
    if (!m_live)
        QSKIP("Run bin/test-live. live_ui_tests needs the compose runner.");
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
    if (!m_live)
        QSKIP("Run bin/test-live. live_ui_tests needs the compose runner.");
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

void LiveUiTest::consecutiveSameAuthorMinuteGroupsThroughIrcEvent()
{
    std::unique_ptr<QTemporaryDir> xdg;
    if (!m_live) {
        xdg = std::make_unique<QTemporaryDir>();
        QVERIFY(xdg->isValid());
        const QString root = xdg->path();
        const QString config = root + QLatin1String("/config");
        QDir().mkpath(config);
        QDir().mkpath(root + QLatin1String("/cache"));
        QDir().mkpath(root + QLatin1String("/data"));
        qputenv("XDG_CONFIG_HOME", config.toUtf8());
        qputenv("XDG_CACHE_HOME", (root + QLatin1String("/cache")).toUtf8());
        qputenv("XDG_DATA_HOME", (root + QLatin1String("/data")).toUtf8());
        QSettings::setPath(QSettings::NativeFormat, QSettings::UserScope, config);
    }

    Backend backend;
    IrcSlashSession slash;
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(fixtureConfig(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":server 005 omairc CHANTYPES=# PREFIX=(ov)@+ "
                          ":are supported by this server\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :omairc anna rio\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    QQmlApplicationEngine engine;
    QQmlComponent component(&engine, QUrl(QStringLiteral("qrc:/OmaircWindow.qml")));
    QVERIFY2(component.status() != QQmlComponent::Error, qPrintable(component.errorString()));
    QVariantMap properties;
    properties.insert(QStringLiteral("backend"), QVariant::fromValue(&backend));
    properties.insert(QStringLiteral("irc"), QVariant::fromValue(&controller));
    properties.insert(QStringLiteral("slashCommands"), QVariant::fromValue(&slash));
    std::unique_ptr<QObject> root(component.createWithInitialProperties(properties));
    QVERIFY2(root.get(), qPrintable(component.errorString()));
    auto *window = qobject_cast<QQuickWindow *>(root.get());
    QVERIFY(window);
    QVERIFY(QTest::qWaitForWindowExposed(window));
    window->setWidth(1180);
    window->setHeight(760);
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QCoreApplication::processEvents();

    QVERIFY(waitUntil([&] {
        QQuickItem *list = window->findChild<QQuickItem *>(QStringLiteral("messageList"));
        return list && list->isVisible() && list->height() > 0
            && !window->property("consoleVisible").toBool();
    }));
    QQuickItem *list = window->findChild<QQuickItem *>(QStringLiteral("messageList"));
    QVERIFY(list);
    QVERIFY(qobject_cast<MessageListModel *>(list->property("model").value<QObject *>()));

    transport->injectBytes(
        QByteArrayLiteral("@time=2026-09-10T11:11:00.000Z :anna!u@h PRIVMSG #omarchy :group lead\r\n"
                          "@time=2026-09-10T11:11:00.000Z :anna!u@h PRIVMSG #omarchy :group continuation\r\n"
                          ":rio!u@h PART #omarchy\r\n"
                          "@time=2026-09-10T11:11:00.000Z :anna!u@h PRIVMSG #omarchy :after event\r\n"));
    QVERIFY2(waitUntil([&] {
                    const int after = chromeIndex(collectTranscriptChrome(window),
                                                  QStringLiteral("after event"));
                    const int left = chromeIndex(collectTranscriptChrome(window),
                                                 QStringLiteral("rio left"));
                    return after >= 0 && left >= 0;
                }),
             qPrintable(describeChrome(collectTranscriptChrome(window))));

    const QVector<TranscriptRowChrome> rows = collectTranscriptChrome(window);
    const int leadAt = chromeIndex(rows, QStringLiteral("group lead"));
    const int groupedAt = chromeIndex(rows, QStringLiteral("group continuation"));
    const int eventAt = chromeIndex(rows, QStringLiteral("rio left"));
    const int afterAt = chromeIndex(rows, QStringLiteral("after event"));
    QVERIFY2(leadAt >= 0 && groupedAt == leadAt + 1 && eventAt == groupedAt + 1
                 && afterAt == eventAt + 1,
             qPrintable(describeChrome(rows)));

    const TranscriptRowChrome &lead = rows.at(leadAt);
    const TranscriptRowChrome &grouped = rows.at(groupedAt);
    const TranscriptRowChrome &event = rows.at(eventAt);
    const TranscriptRowChrome &after = rows.at(afterAt);
    QVERIFY2(lead.avatarVisible && lead.headerVisible, qPrintable(describeChrome(rows)));
    QVERIFY2(!grouped.avatarVisible && !grouped.headerVisible,
             qPrintable(describeChrome(rows)));
    QVERIFY2(grouped.height < lead.height, qPrintable(describeChrome(rows)));
    QVERIFY2(!event.avatarVisible && !event.headerVisible, qPrintable(describeChrome(rows)));
    QVERIFY2(after.avatarVisible && after.headerVisible, qPrintable(describeChrome(rows)));
    QVERIFY2(saveFixtureShot(window, QStringLiteral("live-ui-grouped-messages")),
             "grouped screenshot");
}

void LiveUiTest::replayAndLiveSameAuthorMinuteDoNotGroupThroughIrcEvent()
{
    std::unique_ptr<QTemporaryDir> xdg;
    if (!m_live) {
        xdg = std::make_unique<QTemporaryDir>();
        QVERIFY(xdg->isValid());
        const QString root = xdg->path();
        const QString config = root + QLatin1String("/config");
        QDir().mkpath(config);
        QDir().mkpath(root + QLatin1String("/cache"));
        QDir().mkpath(root + QLatin1String("/data"));
        qputenv("XDG_CONFIG_HOME", config.toUtf8());
        qputenv("XDG_CACHE_HOME", (root + QLatin1String("/cache")).toUtf8());
        qputenv("XDG_DATA_HOME", (root + QLatin1String("/data")).toUtf8());
        QSettings::setPath(QSettings::NativeFormat, QSettings::UserScope, config);
    }

    Backend backend;
    IrcSlashSession slash;
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(fixtureConfig(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory\r\n"
                          ":server CAP omairc ACK :batch chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":server 005 omairc CHANTYPES=# PREFIX=(ov)@+ "
                          ":are supported by this server\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :omairc anna\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QQmlApplicationEngine engine;
    QQmlComponent component(&engine, QUrl(QStringLiteral("qrc:/OmaircWindow.qml")));
    QVERIFY2(component.status() != QQmlComponent::Error, qPrintable(component.errorString()));
    QVariantMap properties;
    properties.insert(QStringLiteral("backend"), QVariant::fromValue(&backend));
    properties.insert(QStringLiteral("irc"), QVariant::fromValue(&controller));
    properties.insert(QStringLiteral("slashCommands"), QVariant::fromValue(&slash));
    std::unique_ptr<QObject> root(component.createWithInitialProperties(properties));
    QVERIFY2(root.get(), qPrintable(component.errorString()));
    auto *window = qobject_cast<QQuickWindow *>(root.get());
    QVERIFY(window);
    QVERIFY(QTest::qWaitForWindowExposed(window));
    window->setWidth(1180);
    window->setHeight(760);
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QCoreApplication::processEvents();

    QVERIFY(waitUntil([&] {
        QQuickItem *list = window->findChild<QQuickItem *>(QStringLiteral("messageList"));
        return list && list->isVisible() && list->height() > 0
            && !window->property("consoleVisible").toBool();
    }));
    QVERIFY(qobject_cast<MessageListModel *>(
        window->findChild<QQuickItem *>(QStringLiteral("messageList"))
            ->property("model")
            .value<QObject *>()));

    transport->injectBytes(
        QByteArrayLiteral(
            ":irc.host BATCH +hx chathistory #omarchy\r\n"
            "@batch=hx;time=2011-10-19T16:40:51.620Z;msgid=old :anna!u@h PRIVMSG #omarchy :replayed line\r\n"
            ":irc.host BATCH -hx\r\n"
            "@time=2011-10-19T16:40:51.620Z :anna!u@h PRIVMSG #omarchy :live line\r\n"));
    QVERIFY2(waitUntil([&] {
                    const QVector<TranscriptRowChrome> rows =
                        collectTranscriptChrome(window);
                    return chromeIndex(rows, QStringLiteral("replayed line")) >= 0
                        && chromeIndex(rows, QStringLiteral("live line")) >= 0;
                }),
             qPrintable(describeChrome(collectTranscriptChrome(window))));

    const QVector<TranscriptRowChrome> rows = collectTranscriptChrome(window);
    const int replayAt = chromeIndex(rows, QStringLiteral("replayed line"));
    const int joinAt = chromeIndex(rows, QStringLiteral("omairc joined"));
    const int liveAt = chromeIndex(rows, QStringLiteral("live line"));
    QVERIFY2(replayAt >= 0 && joinAt == replayAt + 1 && liveAt == joinAt + 1,
             qPrintable(describeChrome(rows)));
    QVERIFY2(rows.at(replayAt).avatarVisible && rows.at(replayAt).headerVisible,
             qPrintable(describeChrome(rows)));
    QVERIFY2(rows.at(liveAt).avatarVisible && rows.at(liveAt).headerVisible,
             qPrintable(describeChrome(rows)));
    const QColor muted = window->property("mutedColor").value<QColor>();
    QCOMPARE(rows.at(replayAt).bodyColor, muted);
    QVERIFY2(saveFixtureShot(window, QStringLiteral("live-ui-replay-history")),
             "replay screenshot");
}

int runLiveUiTests(int argc, char **argv)
{
    LiveUiTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_live_ui.moc"
