#include "backend.h"
#include "conversationlistmodel.h"
#include "fakeirctransport.h"
#include "irccapability.h"
#include "irccontroller.h"
#include "ircserverfeatures.h"
#include "ircsession.h"
#include "ircstatusconsole.h"
#include "ircslashcomplete.h"
#include "liveharness.h"
#include "liveuiworld.h"
#include "memberlistmodel.h"
#include "messagelistmodel.h"
#include "networklogmodel.h"
#include "seededircfixture.h"

#include <QColor>
#include <QCoreApplication>
#include <QDebug>
#include <QDir>
#include <QFileInfo>
#include <QImage>
#include <QAbstractItemModel>
#include <QQmlApplicationEngine>
#include <QSignalSpy>
#include <QQmlComponent>
#include <QQuickItem>
#include <QQuickWindow>
#include <QSet>
#include <QSettings>
#include <QStringList>
#include <QTemporaryDir>
#include <QTest>
#include <QVariantMap>

#include <functional>
#include <memory>
#include <string_view>

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

void walkItems(QQuickItem *item, const std::function<void(QQuickItem *)> &visit)
{
    if (!item)
        return;
    visit(item);
    for (QQuickItem *child : item->childItems())
        walkItems(child, visit);
}

QQuickItem *findNamedItem(QQuickWindow *window, const QString &name)
{
    if (!window)
        return nullptr;
    if (QQuickItem *named = window->findChild<QQuickItem *>(name))
        return named;
    QQuickItem *found = nullptr;
    walkItems(window->contentItem(), [&](QQuickItem *item) {
        if (!found && item->objectName() == name)
            found = item;
    });
    return found;
}

void walkSidebarItems(QQuickItem *item, const std::function<void(QQuickItem *)> &visit)
{
    if (!item)
        return;
    visit(item);
    for (QQuickItem *child : item->childItems())
        walkSidebarItems(child, visit);
}

QStringList visibleDirectRowLabels(QQuickWindow *window)
{
    QStringList labels;
    if (!window)
        return labels;
    walkSidebarItems(window->contentItem(), [&](QQuickItem *item) {
        if (item->property("conversationName").toString().isEmpty()
            || !item->property("direct").toBool()
            || !item->isVisible() || item->height() <= 0) {
            return;
        }
        auto *label =
            item->findChild<QQuickItem *>(QStringLiteral("conversationLabel"));
        labels.append(label && label->isVisible()
                          ? label->property("text").toString()
                          : QString());
    });
    return labels;
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

bool findMarkVisible(QQuickItem *list, int row)
{
    if (!list || row < 0)
        return false;
    QQuickItem *content = list->property("contentItem").value<QQuickItem *>();
    if (!content)
        return false;
    const QList<QQuickItem *> children = content->childItems();
    for (QQuickItem *child : children) {
        if (child->property("index").toInt() != row)
            continue;
        if (QQuickItem *mark = child->findChild<QQuickItem *>(QStringLiteral("findMatch")))
            return mark->isVisible();
    }
    return false;
}

void typeIntoComposer(QQuickWindow *window, const QString &text)
{
    QTest::keyClick(window, Qt::Key_A, Qt::ControlModifier);
    QTest::keyClick(window, Qt::Key_Backspace);
    for (const QChar ch : text)
        QTest::keyClick(window, ch.toLatin1(), Qt::NoModifier, 0);
}

QVariant roleAt(const QAbstractItemModel *model, int row, int role)
{
    return model->data(model->index(row, 0), role);
}

int rowFor(const QAbstractItemModel *model,
           const QString &networkId,
           const QString &target)
{
    for (int row = 0; row < model->rowCount(); ++row) {
        if (roleAt(model, row, ConversationListModel::NetworkIdRole) == networkId
            && roleAt(model, row, ConversationListModel::ConversationRole) == target) {
            return row;
        }
    }
    return -1;
}

bool logContains(QAbstractItemModel *lines, const QString &needle)
{
    for (int row = 0; row < lines->rowCount(); ++row) {
        const QString text =
            lines->data(lines->index(row, 0), NetworkLogModel::TextRole).toString();
        if (text.contains(needle))
            return true;
    }
    return false;
}

QStringList selectedBodies(const QAbstractItemModel *messages)
{
    QStringList bodies;
    if (!messages)
        return bodies;
    for (int row = 0; row < messages->rowCount(); ++row)
        bodies.append(roleAt(messages, row, MessageListModel::BodyRole).toString());
    return bodies;
}

QString featureText(std::string_view view)
{
    return QString::fromUtf8(view.data(), int(view.size()));
}

QString memberField(const QAbstractItemModel *members,
                    const QString &nick,
                    int role)
{
    for (int row = 0; row < members->rowCount(); ++row) {
        if (roleAt(members, row, MemberListModel::NickRole) == nick)
            return roleAt(members, row, role).toString();
    }
    return {};
}

bool memberAway(const QAbstractItemModel *members, const QString &nick)
{
    for (int row = 0; row < members->rowCount(); ++row) {
        if (roleAt(members, row, MemberListModel::NickRole) == nick)
            return roleAt(members, row, MemberListModel::AwayRole).toBool();
    }
    return false;
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
    void memberJoinPartModeUpdatesWithoutReset();
    void replayAndLiveSameAuthorMinuteDoNotGroupThroughIrcEvent();
    void bouncerQueryReplayRendersDirectMessageInSidebar();
    void ctrlFFindsLiveTranscriptAndStatus();
    void seededIrcFixtureFurnishesMockWorld();

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
    if (m_live)
        QSKIP("FakeIrcTransport grouping runs under bin/test, not the compose world.");
    std::unique_ptr<QTemporaryDir> xdg = std::make_unique<QTemporaryDir>();
    QVERIFY(xdg->isValid());
    const QString xdgRoot = xdg->path();
    const QString config = xdgRoot + QLatin1String("/config");
    QDir().mkpath(config);
    QDir().mkpath(xdgRoot + QLatin1String("/cache"));
    QDir().mkpath(xdgRoot + QLatin1String("/data"));
    qputenv("XDG_CONFIG_HOME", config.toUtf8());
    qputenv("XDG_CACHE_HOME", (xdgRoot + QLatin1String("/cache")).toUtf8());
    qputenv("XDG_DATA_HOME", (xdgRoot + QLatin1String("/data")).toUtf8());
    QSettings::setPath(QSettings::NativeFormat, QSettings::UserScope, config);

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

void LiveUiTest::memberJoinPartModeUpdatesWithoutReset()
{
    if (m_live)
        QSKIP("FakeIrcTransport member updates run under bin/test, not the compose world.");
    std::unique_ptr<QTemporaryDir> xdg = std::make_unique<QTemporaryDir>();
    QVERIFY(xdg->isValid());
    const QString xdgRoot = xdg->path();
    const QString config = xdgRoot + QLatin1String("/config");
    QDir().mkpath(config);
    QDir().mkpath(xdgRoot + QLatin1String("/cache"));
    QDir().mkpath(xdgRoot + QLatin1String("/data"));
    qputenv("XDG_CONFIG_HOME", config.toUtf8());
    qputenv("XDG_CACHE_HOME", (xdgRoot + QLatin1String("/cache")).toUtf8());
    qputenv("XDG_DATA_HOME", (xdgRoot + QLatin1String("/data")).toUtf8());
    QSettings::setPath(QSettings::NativeFormat, QSettings::UserScope, config);

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
        QQuickItem *messages = window->findChild<QQuickItem *>(QStringLiteral("messageList"));
        return messages && messages->isVisible() && messages->height() > 0
            && !window->property("consoleVisible").toBool();
    }));
    window->setProperty("membersVisible", true);
    QCoreApplication::processEvents();
    QVERIFY2(waitUntil([&] {
                    QQuickItem *panel = findNamedItem(window, QStringLiteral("membersPanel"));
                    QQuickItem *anna = findNamedItem(window, QStringLiteral("member-anna"));
                    return panel && panel->isVisible() && anna
                        && window->property("currentPeopleCount").toInt() == 3;
                }),
             qPrintable(QStringLiteral("people=%1 console=%2 members=%3 channel=%4")
                            .arg(window->property("currentPeopleCount").toInt())
                            .arg(window->property("consoleVisible").toBool())
                            .arg(window->property("membersVisible").toBool())
                            .arg(window->property("currentConversationIsChannel").toBool())));

    auto *members = qobject_cast<QAbstractItemModel *>(controller.members());
    QVERIFY(members);
    QSignalSpy memberResets(members, &QAbstractItemModel::modelReset);
    QSignalSpy memberInserts(members, &QAbstractItemModel::rowsInserted);
    QSignalSpy memberRemoves(members, &QAbstractItemModel::rowsRemoved);
    QQuickItem *anna = findNamedItem(window, QStringLiteral("member-anna"));
    QVERIFY(anna);

    transport->injectBytes(QByteArrayLiteral(":zoe!u@h JOIN :#omarchy\r\n"));
    QVERIFY(waitUntil([&] {
        return window->property("currentPeopleCount").toInt() == 4
            && findNamedItem(window, QStringLiteral("member-zoe"));
    }));
    QCOMPARE(memberResets.size(), 0);
    QCOMPARE(memberInserts.size(), 1);
    QCOMPARE(findNamedItem(window, QStringLiteral("member-anna")), anna);
    QCOMPARE(window->property("currentPeopleCount").toInt(), 4);

    transport->injectBytes(QByteArrayLiteral(":rio!u@h PART #omarchy\r\n"));
    QVERIFY(waitUntil([&] {
        return window->property("currentPeopleCount").toInt() == 3
            && !findNamedItem(window, QStringLiteral("member-rio"));
    }));
    QCOMPARE(memberResets.size(), 0);
    QCOMPARE(memberRemoves.size(), 1);
    QCOMPARE(findNamedItem(window, QStringLiteral("member-anna")), anna);

    transport->injectBytes(QByteArrayLiteral(":op!u@h MODE #omarchy +o anna\r\n"));
    QVERIFY(waitUntil([&] {
        QQuickItem *row = findNamedItem(window, QStringLiteral("member-anna"));
        return row && row->property("label").toString() == QStringLiteral("@anna");
    }));
    QCOMPARE(memberResets.size(), 0);
    QCOMPARE(findNamedItem(window, QStringLiteral("member-anna")), anna);
    QCOMPARE(anna->property("label").toString(), QStringLiteral("@anna"));
}

void LiveUiTest::replayAndLiveSameAuthorMinuteDoNotGroupThroughIrcEvent()
{
    if (m_live)
        QSKIP("FakeIrcTransport grouping runs under bin/test, not the compose world.");
    std::unique_ptr<QTemporaryDir> xdg = std::make_unique<QTemporaryDir>();
    QVERIFY(xdg->isValid());
    const QString xdgRoot = xdg->path();
    const QString config = xdgRoot + QLatin1String("/config");
    QDir().mkpath(config);
    QDir().mkpath(xdgRoot + QLatin1String("/cache"));
    QDir().mkpath(xdgRoot + QLatin1String("/data"));
    qputenv("XDG_CONFIG_HOME", config.toUtf8());
    qputenv("XDG_CACHE_HOME", (xdgRoot + QLatin1String("/cache")).toUtf8());
    qputenv("XDG_DATA_HOME", (xdgRoot + QLatin1String("/data")).toUtf8());
    QSettings::setPath(QSettings::NativeFormat, QSettings::UserScope, config);

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

void LiveUiTest::bouncerQueryReplayRendersDirectMessageInSidebar()
{
    if (m_live)
        QSKIP("FakeIrcTransport playback runs under bin/test, not the compose world.");
    std::unique_ptr<QTemporaryDir> xdg = std::make_unique<QTemporaryDir>();
    QVERIFY(xdg->isValid());
    const QString xdgRoot = xdg->path();
    const QString config = xdgRoot + QLatin1String("/config");
    QDir().mkpath(config);
    QDir().mkpath(xdgRoot + QLatin1String("/cache"));
    QDir().mkpath(xdgRoot + QLatin1String("/data"));
    qputenv("XDG_CONFIG_HOME", config.toUtf8());
    qputenv("XDG_CACHE_HOME", (xdgRoot + QLatin1String("/cache")).toUtf8());
    qputenv("XDG_DATA_HOME", (xdgRoot + QLatin1String("/data")).toUtf8());
    QSettings::setPath(QSettings::NativeFormat, QSettings::UserScope, config);

    Backend backend;
    IrcSlashSession slash;
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(fixtureConfig(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch echo-message\r\n"
                          ":server CAP omairc ACK :batch echo-message\r\n"
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
    QCoreApplication::processEvents();

    QVERIFY(waitUntil([&] {
        QQuickItem *list = window->findChild<QQuickItem *>(QStringLiteral("sidebarList"));
        return list && list->isVisible() && list->height() > 0;
    }));
    QCOMPARE(visibleDirectRowLabels(window), QStringList());

    transport->injectBytes(
        QByteArrayLiteral(
            ":znc.in BATCH +q1 znc.in/playback lena\r\n"
            "@batch=q1;time=2026-09-10T11:11:00.000Z :omairc!u@h PRIVMSG lena :hi\r\n"
            ":znc.in BATCH -q1\r\n"
            ":znc.in BATCH +q2 znc.in/playback dana\r\n"
            "@batch=q2;time=2026-09-10T11:12:00.000Z :dana!u@h PRIVMSG omairc :morning\r\n"
            ":znc.in BATCH -q2\r\n"));

    QVERIFY2(waitUntil([&] {
                    return visibleDirectRowLabels(window)
                        == QStringList({QStringLiteral("dana")});
                }),
             qPrintable(visibleDirectRowLabels(window).join(QLatin1Char('|'))));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("dana"));
    QVERIFY2(waitUntil([&] {
                    return chromeIndex(collectTranscriptChrome(window),
                                       QStringLiteral("morning")) >= 0;
                }),
             qPrintable(describeChrome(collectTranscriptChrome(window))));

    const QVector<TranscriptRowChrome> rows = collectTranscriptChrome(window);
    const int replayAt = chromeIndex(rows, QStringLiteral("morning"));
    QCOMPARE(replayAt, 0);
    const QColor muted = window->property("mutedColor").value<QColor>();
    QCOMPARE(rows.at(replayAt).bodyColor, muted);
    QVERIFY2(saveFixtureShot(window, QStringLiteral("live-ui-bouncer-direct")),
             "bouncer direct screenshot");
}

void LiveUiTest::ctrlFFindsLiveTranscriptAndStatus()
{
    if (m_live)
        QSKIP("FakeIrcTransport find runs under bin/test, not the compose world.");
    std::unique_ptr<QTemporaryDir> xdg = std::make_unique<QTemporaryDir>();
    QVERIFY(xdg->isValid());
    const QString xdgRoot = xdg->path();
    const QString config = xdgRoot + QLatin1String("/config");
    QDir().mkpath(config);
    QDir().mkpath(xdgRoot + QLatin1String("/cache"));
    QDir().mkpath(xdgRoot + QLatin1String("/data"));
    qputenv("XDG_CONFIG_HOME", config.toUtf8());
    qputenv("XDG_CACHE_HOME", (xdgRoot + QLatin1String("/cache")).toUtf8());
    qputenv("XDG_DATA_HOME", (xdgRoot + QLatin1String("/data")).toUtf8());
    QSettings::setPath(QSettings::NativeFormat, QSettings::UserScope, config);

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
                          ":server 366 omairc #omarchy :End of NAMES\r\n"
                          ":zed!u@h PRIVMSG #omarchy :older unique body needle-xyz\r\n"
                          ":uniqnick!u@h PRIVMSG #omarchy :no nick in this line\r\n"
                          ":anna!u@h PRIVMSG #omarchy :later filler\r\n"
                          ":server 404 omairc #omarchy :status body without the numeric\r\n"));
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
    QQuickItem *list = window->findChild<QQuickItem *>(QStringLiteral("messageList"));
    QVERIFY(list);
    auto *messages = qobject_cast<MessageListModel *>(list->property("model").value<QObject *>());
    QVERIFY(messages);
    QVERIFY2(waitUntil([&] {
                    return messages->field(0, QStringLiteral("body")).contains(QStringLiteral("joined"))
                        && chromeIndex(collectTranscriptChrome(window),
                                       QStringLiteral("older unique body needle-xyz"))
                        >= 0;
                }),
             qPrintable(describeChrome(collectTranscriptChrome(window))));

    QQuickItem *composer = window->findChild<QQuickItem *>(QStringLiteral("messageComposer"));
    QVERIFY(composer);
    composer->forceActiveFocus();
    QVERIFY(waitUntil([&] { return composer->hasActiveFocus(); }));
    QCOMPARE(composer->property("text").toString(), QString());

    QTest::keyClick(window, Qt::Key_F, Qt::ControlModifier);
    QVERIFY(waitUntil([&] { return window->property("findActive").toBool(); }));
    QCOMPARE(composer->property("placeholderText").toString(), QStringLiteral("Find"));
    QCOMPARE(window->property("findIndex").toInt(), -1);
    QTest::keyClick(window, Qt::Key_Escape);
    QVERIFY(waitUntil([&] { return !window->property("findActive").toBool(); }));
    QCOMPARE(composer->property("text").toString(), QString());

    typeIntoComposer(window, QStringLiteral("keep-draft"));
    QCOMPARE(composer->property("text").toString(), QStringLiteral("keep-draft"));
    QTest::keyClick(window, Qt::Key_F, Qt::ControlModifier);
    QVERIFY(waitUntil([&] { return window->property("findActive").toBool(); }));
    typeIntoComposer(window, QStringLiteral("needle-xyz"));
    QVERIFY(waitUntil([&] {
        const int index = window->property("findIndex").toInt();
        return index >= 0
            && messages->field(index, QStringLiteral("body"))
                == QStringLiteral("older unique body needle-xyz");
    }));
    QVERIFY(findMarkVisible(list, window->property("findIndex").toInt()));

    const int beforeCount = messages->rowCount();
    typeIntoComposer(window, QStringLiteral("uniqnick"));
    QVERIFY(waitUntil([&] {
        const int index = window->property("findIndex").toInt();
        return index >= 0
            && messages->field(index, QStringLiteral("author")) == QStringLiteral("uniqnick");
    }));
    QVERIFY(!messages->field(window->property("findIndex").toInt(), QStringLiteral("body"))
                 .contains(QStringLiteral("uniqnick")));
    QVERIFY(findMarkVisible(list, window->property("findIndex").toInt()));

    QTest::keyClick(window, Qt::Key_Return);
    QCOMPARE(messages->rowCount(), beforeCount);
    QCOMPARE(composer->property("text").toString(), QStringLiteral("uniqnick"));

    QTest::keyClick(window, Qt::Key_Escape);
    QVERIFY(waitUntil([&] { return !window->property("findActive").toBool(); }));
    QCOMPARE(composer->property("text").toString(), QStringLiteral("keep-draft"));

    controller.openStatus(QStringLiteral("libera"));
    QVERIFY(waitUntil([&] {
        QQuickItem *console = window->findChild<QQuickItem *>(QStringLiteral("consoleList"));
        return window->property("consoleVisible").toBool() && console && console->isVisible();
    }));
    QQuickItem *console = window->findChild<QQuickItem *>(QStringLiteral("consoleList"));
    QVERIFY(console);
    auto *log = qobject_cast<NetworkLogModel *>(console->property("model").value<QObject *>());
    QVERIFY(log);
    QVERIFY(waitUntil([&] {
        for (int row = 0; row < log->rowCount(); ++row) {
            if (log->field(row, QStringLiteral("label")) == QStringLiteral("404"))
                return true;
        }
        return false;
    }));

    composer->forceActiveFocus();
    QVERIFY(waitUntil([&] { return composer->hasActiveFocus(); }));
    QTest::keyClick(window, Qt::Key_F, Qt::ControlModifier);
    QVERIFY(waitUntil([&] { return window->property("findActive").toBool(); }));
    QCOMPARE(composer->property("placeholderText").toString(), QStringLiteral("Find"));
    typeIntoComposer(window, QStringLiteral("404"));
    QVERIFY(waitUntil([&] {
        const int index = window->property("findIndex").toInt();
        return index >= 0 && log->field(index, QStringLiteral("label")) == QStringLiteral("404");
    }));
    QVERIFY(!log->field(window->property("findIndex").toInt(), QStringLiteral("text"))
                 .contains(QStringLiteral("404")));
    QVERIFY(findMarkVisible(console, window->property("findIndex").toInt()));
    QVERIFY2(saveFixtureShot(window, QStringLiteral("live-ui-find")), "find screenshot");
}

void LiveUiTest::seededIrcFixtureFurnishesMockWorld()
{
    if (m_live)
        QSKIP("SeededIrcFixture runs under bin/test, not the compose world.");

    SeededIrcFixture world;
    QVERIFY2(world.open(), qPrintable(world.lastError()));

    IrcController &controller = world.controller();
    QCOMPARE(controller.networkIds(),
             QStringList({QStringLiteral("oftc"), QStringLiteral("omarchy")}));
    QCOMPARE(controller.selectedNetworkId(), SeededIrcFixture::omarchyNetworkId());
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QCOMPARE(controller.currentNick(), QStringLiteral("fred"));
    QCOMPARE(controller.topic(),
             QStringLiteral("A cozy corner for Omarchy users and builders."));
    QCOMPARE(controller.peopleCount(), 12);
    QVERIFY(controller.hasAwayPresence());
    QVERIFY(controller.hasMemberStatus());
    QVERIFY(controller.hasTyping());
    QCOMPARE(controller.typingNicks(), QStringList({QStringLiteral("anna")}));
    QVERIFY(controller.nickIsTyping(QStringLiteral("anna")));

    const IrcServerFeatures &omarchyFeatures =
        controller.serverFeatures(SeededIrcFixture::omarchyNetworkId());
    QCOMPARE(featureText(omarchyFeatures.channelTypes()), QStringLiteral("#"));
    QCOMPARE(featureText(omarchyFeatures.prefixModes()), QStringLiteral("ov"));
    QCOMPARE(featureText(omarchyFeatures.prefixSymbols()), QStringLiteral("@+"));
    const IrcServerFeatures &oftcFeatures =
        controller.serverFeatures(SeededIrcFixture::oftcNetworkId());
    QCOMPARE(featureText(oftcFeatures.channelTypes()), QStringLiteral("#"));
    QCOMPARE(featureText(oftcFeatures.prefixSymbols()), QStringLiteral("@+"));

    IrcSession *omarchySession =
        controller.session(SeededIrcFixture::omarchyNetworkId());
    QVERIFY(omarchySession);
    QVERIFY(omarchySession->capabilities().contains(IrcCapability::EchoMessage));
    QVERIFY(omarchySession->capabilities().contains(IrcCapability::MessageTags));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    const int omarchyRow = rowFor(conversations, SeededIrcFixture::omarchyNetworkId(),
                                  QStringLiteral("#omarchy"));
    const int desktopRow = rowFor(conversations, SeededIrcFixture::omarchyNetworkId(),
                                  QStringLiteral("#desktop"));
    const int ricingRow = rowFor(conversations, SeededIrcFixture::omarchyNetworkId(),
                                 QStringLiteral("#ricing"));
    const int helpRow = rowFor(conversations, SeededIrcFixture::omarchyNetworkId(),
                               QStringLiteral("#help"));
    const int annaRow = rowFor(conversations, SeededIrcFixture::omarchyNetworkId(),
                               QStringLiteral("anna"));
    const int daxRow = rowFor(conversations, SeededIrcFixture::omarchyNetworkId(),
                              QStringLiteral("dax"));
    const int oftcOmarchyRow = rowFor(conversations, SeededIrcFixture::oftcNetworkId(),
                                      QStringLiteral("#omarchy"));
    const int labRow = rowFor(conversations, SeededIrcFixture::oftcNetworkId(),
                              QStringLiteral("#lab"));
    const int buildRow = rowFor(conversations, SeededIrcFixture::oftcNetworkId(),
                                QStringLiteral("#build"));
    const int rioRow = rowFor(conversations, SeededIrcFixture::oftcNetworkId(),
                              QStringLiteral("rio"));
    QVERIFY(omarchyRow >= 0 && desktopRow >= 0 && ricingRow >= 0 && helpRow >= 0);
    QVERIFY(annaRow >= 0 && daxRow >= 0);
    QVERIFY(oftcOmarchyRow >= 0 && labRow >= 0 && buildRow >= 0 && rioRow >= 0);
    QCOMPARE(rowFor(conversations, SeededIrcFixture::omarchyNetworkId(),
                    QStringLiteral("NickServ")),
             -1);

    QCOMPARE(roleAt(conversations, omarchyRow, ConversationListModel::UnreadRole), 0);
    QCOMPARE(roleAt(conversations, desktopRow, ConversationListModel::UnreadRole), 3);
    QCOMPARE(roleAt(conversations, desktopRow, ConversationListModel::MentionRole), false);
    QCOMPARE(roleAt(conversations, ricingRow, ConversationListModel::UnreadRole), 12);
    QCOMPARE(roleAt(conversations, ricingRow, ConversationListModel::MentionRole), true);
    QCOMPARE(roleAt(conversations, helpRow, ConversationListModel::UnreadRole), 0);
    QCOMPARE(roleAt(conversations, annaRow, ConversationListModel::UnreadRole), 1);
    QCOMPARE(roleAt(conversations, annaRow, ConversationListModel::MentionRole), true);
    QCOMPARE(roleAt(conversations, annaRow, ConversationListModel::TypingRole), true);
    QCOMPARE(roleAt(conversations, daxRow, ConversationListModel::UnreadRole), 0);
    QCOMPARE(roleAt(conversations, oftcOmarchyRow, ConversationListModel::UnreadRole), 0);
    QCOMPARE(roleAt(conversations, labRow, ConversationListModel::UnreadRole), 0);
    QCOMPARE(roleAt(conversations, buildRow, ConversationListModel::UnreadRole), 2);
    QCOMPARE(roleAt(conversations, rioRow, ConversationListModel::UnreadRole), 0);
    QCOMPARE(controller.unreadCountFor(SeededIrcFixture::omarchyNetworkId()), 16);
    QVERIFY(controller.mentionFor(SeededIrcFixture::omarchyNetworkId()));
    QCOMPARE(controller.unreadCountFor(SeededIrcFixture::oftcNetworkId()), 2);
    QVERIFY(!controller.mentionFor(SeededIrcFixture::oftcNetworkId()));

    auto *members = qobject_cast<QAbstractItemModel *>(controller.members());
    QVERIFY(members);
    QCOMPARE(members->rowCount(), 12);
    for (const QString &nick : {QStringLiteral("teo"), QStringLiteral("lena"),
                                QStringLiteral("sam"), QStringLiteral("ivy"),
                                QStringLiteral("max")}) {
        QVERIFY2(memberAway(members, nick), qPrintable(nick));
    }
    for (const QString &nick : {QStringLiteral("anna"), QStringLiteral("dax"),
                                QStringLiteral("mira"), QStringLiteral("sol"),
                                QStringLiteral("fred"), QStringLiteral("kai"),
                                QStringLiteral("nora")}) {
        QVERIFY2(!memberAway(members, nick), qPrintable(nick));
    }
    QCOMPARE(memberField(members, QStringLiteral("anna"), MemberListModel::StatusRole),
             QStringLiteral("writing docs"));
    QCOMPARE(memberField(members, QStringLiteral("dax"), MemberListModel::StatusRole),
             QStringLiteral("on #desktop"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    const QStringList omarchyBodies = selectedBodies(messages);
    QVERIFY(omarchyBodies.contains(
        QStringLiteral("Morning! Has anyone tried the new minimal install flow yet?")));
    QVERIFY(omarchyBodies.contains(
        QStringLiteral("Keep the member list optional and I am sold.")));
    QVERIFY(omarchyBodies.contains(QStringLiteral("sol joined")));
    QVERIFY(omarchyBodies.contains(QStringLiteral("nora joined")));

    controller.console()->setNetwork(SeededIrcFixture::omarchyNetworkId());
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("Looking up your hostname")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("Welcome to the mock network")));
    controller.console()->setNetwork(SeededIrcFixture::oftcNetworkId());
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("Welcome to the mock OFTC network")));

    QVERIFY2(world.createWindow(), qPrintable(world.lastError()));
    QQuickWindow *window = world.window();
    QVERIFY(window);
    QVERIFY(waitUntil([&] {
        QQuickItem *list = window->findChild<QQuickItem *>(QStringLiteral("messageList"));
        return list && list->isVisible() && list->height() > 0
            && !window->property("consoleVisible").toBool();
    }));
    QQuickItem *list = window->findChild<QQuickItem *>(QStringLiteral("messageList"));
    QVERIFY(list);
    QVERIFY(qobject_cast<MessageListModel *>(list->property("model").value<QObject *>()));
    QCOMPARE(window->property("currentTopic").toString(),
             QStringLiteral("A cozy corner for Omarchy users and builders."));
    QVERIFY2(waitUntil([&] {
                    return chromeIndex(collectTranscriptChrome(window),
                                       QStringLiteral("Keep the member list optional and I am sold."))
                        >= 0;
                }),
             qPrintable(describeChrome(collectTranscriptChrome(window))));
    QVERIFY2(saveFixtureShot(window, QStringLiteral("live-ui-seeded-mock-world")),
             "seeded screenshot");

    controller.selectConversation(SeededIrcFixture::oftcNetworkId(),
                                  QStringLiteral("#omarchy"));
    QCOMPARE(controller.currentNick(), QStringLiteral("oak"));
    QCOMPARE(controller.topic(),
             QStringLiteral("A different #omarchy, hosted on OFTC."));
    QCOMPARE(controller.peopleCount(), 4);
    QVERIFY(selectedBodies(messages).contains(
        QStringLiteral("This #omarchy is the OFTC one. Different people, same name.")));

    controller.selectConversation(SeededIrcFixture::oftcNetworkId(),
                                  QStringLiteral("#lab"));
    QCOMPARE(controller.topic(),
             QStringLiteral("Build lab for packaging and CI."));
    QCOMPARE(controller.peopleCount(), 6);

    controller.selectConversation(SeededIrcFixture::oftcNetworkId(),
                                  QStringLiteral("#build"));
    QCOMPARE(controller.topic(),
             QStringLiteral("Nightly builds and failing tests."));
    QCOMPARE(controller.peopleCount(), 3);

    controller.selectConversation(SeededIrcFixture::omarchyNetworkId(),
                                  QStringLiteral("#desktop"));
    QCOMPARE(controller.topic(),
             QStringLiteral("Desktops should feel personal, fast, and calm."));
    QCOMPARE(controller.peopleCount(), 8);
    QVERIFY(selectedBodies(messages).contains(
        QStringLiteral("I finally moved every workspace rule into a small, readable file.")));

    controller.selectConversation(SeededIrcFixture::omarchyNetworkId(),
                                  QStringLiteral("#ricing"));
    QVERIFY(selectedBodies(messages).contains(
        QStringLiteral("Muted colors, one strong accent, and enough breathing room.")));
    QVERIFY(selectedBodies(messages).contains(QStringLiteral("looks good, fred.")));

    controller.selectConversation(SeededIrcFixture::omarchyNetworkId(),
                                  QStringLiteral("anna"));
    QVERIFY(selectedBodies(messages).contains(
        QStringLiteral("fred: The prototype already feels at home. Nice work.")));

    controller.selectConversation(SeededIrcFixture::omarchyNetworkId(),
                                  QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/msg glen hello")));
    QCOMPARE(world.omarchyTransport()->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG glen :hello\r\n"));
    QCOMPARE(rowFor(conversations, SeededIrcFixture::omarchyNetworkId(),
                    QStringLiteral("glen")),
             -1);
    world.omarchyTransport()->injectBytes(
        QByteArrayLiteral(":fred!u@h PRIVMSG glen :hello\r\n"));
    QCOMPARE(rowFor(conversations, SeededIrcFixture::omarchyNetworkId(),
                    QStringLiteral("glen")),
             -1);

    world.omarchyTransport()->injectBytes(
        QByteArrayLiteral(":NickServ!NickServ@services PRIVMSG fred "
                          ":This nickname is registered.\r\n"));
    QCOMPARE(rowFor(conversations, SeededIrcFixture::omarchyNetworkId(),
                    QStringLiteral("NickServ")),
             -1);
    controller.console()->setNetwork(SeededIrcFixture::omarchyNetworkId());
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("This nickname is registered.")));

    world.omarchyTransport()->injectBytes(
        QByteArrayLiteral(":mira!u@h PRIVMSG fred :incoming human\r\n"));
    QVERIFY(rowFor(conversations, SeededIrcFixture::omarchyNetworkId(),
                   QStringLiteral("mira"))
            >= 0);
}

int runLiveUiTests(int argc, char **argv)
{
    LiveUiTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_live_ui.moc"
