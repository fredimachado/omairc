#include <QAbstractItemModel>
#include <QCoreApplication>
#include <QFile>
#include <QSettings>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QTest>

#include "fakeirctransport.h"
#include "irccommand.h"
#include "irccontroller.h"
#include "ircmonitor.h"
#include "ircnetworkprofile.h"
#include "ircprofilestore.h"
#include "ircserverfeatures.h"
#include "ircslashcomplete.h"
#include "networklogmodel.h"
#include "testsettings.h"

#include <memory>
#include <optional>
#include <string>

namespace
{
IrcSessionConfig sessionConfig(const QString& networkId = QStringLiteral("libera"))
{
    IrcSessionConfig value;
    value.networkId = networkId;
    value.host = QStringLiteral("irc.example");
    value.tlsEnabled = true;
    value.nick = QStringLiteral("omairc");
    value.username = QStringLiteral("omairc");
    value.realname = QStringLiteral("Omairc User");
    value.reconnectEnabled = false;
    return value;
}

void welcome(FakeIrcTransport *transport, const QByteArray& extra = {})
{
    transport->completeConnect();
    QByteArray bytes =
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n");
    bytes += extra;
    transport->injectBytes(bytes);
}

void welcomeMonitor(FakeIrcTransport *transport,
                    const QByteArray& token = QByteArrayLiteral("MONITOR=100"))
{
    welcome(transport,
            QByteArrayLiteral(":server 005 omairc ") + token
                + QByteArrayLiteral(" :are supported by this server\r\n"));
}

bool logContains(QAbstractItemModel *lines, const QString& needle)
{
    for (int row = 0; row < lines->rowCount(); ++row) {
        const QString text =
            lines->data(lines->index(row, 0), NetworkLogModel::TextRole).toString();
        if (text.contains(needle))
            return true;
    }
    return false;
}

bool framesContain(const QByteArrayList& frames, const QByteArray& needle)
{
    for (const QByteArray& frame : frames) {
        if (frame.contains(needle))
            return true;
    }
    return false;
}

int frameCountContaining(const QByteArrayList& frames, const QByteArray& needle)
{
    int count = 0;
    for (const QByteArray& frame : frames) {
        if (frame.contains(needle))
            ++count;
    }
    return count;
}
}

class MonitorTest : public QObject
{
    Q_OBJECT

private slots:
    void init();
    void parsesIsupport();
    void storeRoundTripsAndIsIdempotent();
    void storeMatchesRfc1459AndSurvivesProfileSave();
    void storeForgetDropsTheGroup();
    void parseAndCatalog();
    void controllerAddsRemovesAndLists();
    void unsupportedNetworkDoesNotSend();
    void clientRefusesWhenFullAndExplains734();
    void hydrationDoesNotNotifyLaterEdgesDo();
    void mutedNickSkipsDesktopNotify();
    void reconnectResubscribesWithoutDuplicateNotify();
    void controllerForgetsWithTheNetwork();
    void refusesBadNicks();
    void offlineMonitorIsNotConnected();

private:
    QString settingsFile() const;

    std::unique_ptr<QTemporaryDir> m_dir;
};

void MonitorTest::init()
{
    m_dir = std::make_unique<QTemporaryDir>();
    QVERIFY(m_dir->isValid());
    TestSettings::isolate(m_dir->path());
    QCoreApplication::setOrganizationName(QStringLiteral("omairc"));
    QCoreApplication::setApplicationName(QStringLiteral("omairc"));
}

QString MonitorTest::settingsFile() const
{
    QSettings settings;
    return settings.fileName();
}

void MonitorTest::parsesIsupport()
{
    IrcServerFeatures absent;
    QVERIFY(!absent.monitorAdvertised());
    QVERIFY(!absent.monitorLimit().has_value());

    IrcServerFeatures unlimited;
    unlimited.applyToken("MONITOR");
    QVERIFY(unlimited.monitorAdvertised());
    QVERIFY(!unlimited.monitorLimit().has_value());

    IrcServerFeatures emptyValue;
    emptyValue.applyToken("MONITOR=");
    QVERIFY(emptyValue.monitorAdvertised());
    QVERIFY(!emptyValue.monitorLimit().has_value());

    IrcServerFeatures limited;
    limited.applyToken("MONITOR=100");
    QVERIFY(limited.monitorAdvertised());
    QCOMPARE(limited.monitorLimit(), std::optional<std::size_t>(100));

    IrcServerFeatures ignored = limited;
    ignored.applyToken("MONITOR=abc");
    ignored.applyToken("MONITOR=0");
    ignored.applyToken("MONITOR=-1");
    QVERIFY(ignored.monitorAdvertised());
    QCOMPARE(ignored.monitorLimit(), std::optional<std::size_t>(100));

    IrcServerFeatures cleared;
    cleared.applyToken("MONITOR=100");
    cleared.applyToken("-MONITOR");
    QVERIFY(!cleared.monitorAdvertised());
    QVERIFY(!cleared.monitorLimit().has_value());
}

void MonitorTest::storeRoundTripsAndIsIdempotent()
{
    const IrcCaseMapping mapping;
    IrcMonitorStore store;
    QVERIFY(store.add(QStringLiteral("net-a"), QStringLiteral("alice"), mapping));
    QVERIFY(!store.add(QStringLiteral("net-a"), QStringLiteral("Alice"), mapping));
    QCOMPARE(store.nicks(QStringLiteral("net-a")),
             QStringList{QStringLiteral("alice")});
    QVERIFY(store.contains(QStringLiteral("net-a"), QStringLiteral("ALICE"), mapping));
    QVERIFY(!store.remove(QStringLiteral("net-a"), QStringLiteral("bob"), mapping));
    QVERIFY(store.remove(QStringLiteral("net-a"), QStringLiteral("alice"), mapping));
    QVERIFY(store.nicks(QStringLiteral("net-a")).isEmpty());
}

void MonitorTest::storeMatchesRfc1459AndSurvivesProfileSave()
{
    const IrcCaseMapping mapping;
    IrcMonitorStore store;
    QVERIFY(store.add(QStringLiteral("net-a"), QStringLiteral("Nick["), mapping));
    QVERIFY(store.contains(QStringLiteral("net-a"), QStringLiteral("nick{"), mapping));
    QCOMPARE(store.listed(QStringLiteral("net-a"), mapping),
             QStringList{QStringLiteral("Nick[")});

    IrcNetworkProfile profile = IrcNetworkProfile::create();
    profile.networkId = QStringLiteral("net-a");
    profile.host = QStringLiteral("irc.example.net");
    profile.nick = QStringLiteral("omairc");
    IrcProfileStore().save(profile);
    IrcProfileStore().save(profile);

    QVERIFY(IrcMonitorStore().contains(
        QStringLiteral("net-a"), QStringLiteral("nick{"), mapping));

    QFile file(settingsFile());
    QVERIFY(file.open(QIODevice::ReadOnly | QIODevice::Text));
    const QString contents = QString::fromUtf8(file.readAll());
    QVERIFY(contents.contains(QLatin1String("monitors")));
    QVERIFY(contents.contains(QLatin1String("Nick[")));
    QVERIFY(!contents.contains(QLatin1String("password"), Qt::CaseInsensitive));
}

void MonitorTest::storeForgetDropsTheGroup()
{
    const IrcCaseMapping mapping;
    IrcMonitorStore store;
    QVERIFY(store.add(QStringLiteral("drop"), QStringLiteral("alice"), mapping));
    QVERIFY(store.add(QStringLiteral("keep"), QStringLiteral("bob"), mapping));
    store.forget(QStringLiteral("drop"));
    QVERIFY(!store.contains(QStringLiteral("drop"), QStringLiteral("alice"), mapping));
    QVERIFY(store.contains(QStringLiteral("keep"), QStringLiteral("bob"), mapping));

    QFile file(settingsFile());
    QVERIFY(file.open(QIODevice::ReadOnly | QIODevice::Text));
    const QString contents = QString::fromUtf8(file.readAll());
    QVERIFY(!contents.contains(QLatin1String("alice")));
    QVERIFY(contents.contains(QLatin1String("bob")));
}

void MonitorTest::parseAndCatalog()
{
    QCOMPARE(IrcCommand::parse(QStringLiteral("/monitor alice")).verb,
             IrcCommand::Verb::Monitor);
    QCOMPARE(IrcCommand::parse(QStringLiteral("/UNMONITOR alice")).verb,
             IrcCommand::Verb::Unmonitor);
    QCOMPARE(IrcCommand::parse(QStringLiteral("/monitored")).verb,
             IrcCommand::Verb::Monitored);
    QVERIFY(IrcCommand::parse(QStringLiteral("/monitor alice"))
                .allowedOn(IrcComposerSurface::Status));
    QCOMPARE(IrcVerbTable::all().size(), 44);
    QCOMPARE(IrcVerbTable::visibleOn(IrcComposerSurface::Status).size(), 36);
    QCOMPARE(IrcVerbTable::visibleOn(IrcComposerSurface::Conversation).size(), 44);
    const auto probe = IrcSlashComplete::project(
        QStringLiteral("/mon"), IrcComposerSurface::Conversation);
    QVERIFY(probe.isOpen());
    QCOMPARE(probe.hits().first().label, QStringLiteral("/monitor"));
    QVERIFY(probe.containsLabel(QStringLiteral("/monitored")));
}

void MonitorTest::controllerAddsRemovesAndLists()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcomeMonitor(transport);
    controller.openStatus(QStringLiteral("libera"));

    QSignalSpy spy(&controller, &IrcController::monitorArrived);
    QVERIFY(controller.console()->submit(QStringLiteral("/monitor alice")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("Watching alice")));
    QVERIFY(framesContain(transport->writtenFrames(),
                          QByteArrayLiteral("MONITOR + alice")));
    QVERIFY(IrcMonitorStore().contains(
        QStringLiteral("libera"), QStringLiteral("alice"), IrcCaseMapping()));

    transport->injectBytes(
        QByteArrayLiteral(":server 730 * :alice!u@h\r\n"));
    QCOMPARE(spy.count(), 0);
    QVERIFY(!logContains(controller.console()->lines(),
                         QStringLiteral("alice is online")));

    QVERIFY(controller.console()->submit(QStringLiteral("/monitored")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("alice (online)")));

    QVERIFY(controller.console()->submit(QStringLiteral("/monitor alice")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("Already watching alice")));
    QCOMPARE(frameCountContaining(transport->writtenFrames(),
                                  QByteArrayLiteral("MONITOR + alice")),
             1);

    QVERIFY(controller.console()->submit(QStringLiteral("/unmonitor alice")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("No longer watching alice")));
    QVERIFY(framesContain(transport->writtenFrames(),
                          QByteArrayLiteral("MONITOR - alice")));
    QVERIFY(controller.console()->submit(QStringLiteral("/monitored")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("Not watching anyone")));
}

void MonitorTest::unsupportedNetworkDoesNotSend()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    controller.openStatus(QStringLiteral("libera"));

    const int before = transport->writtenFrames().size();
    QVERIFY(controller.console()->submit(QStringLiteral("/monitor alice")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("This network does not support MONITOR.")));
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("MONITOR")));
    QCOMPARE(transport->writtenFrames().size(), before);
    QVERIFY(!IrcMonitorStore().contains(
        QStringLiteral("libera"), QStringLiteral("alice"), IrcCaseMapping()));
}

void MonitorTest::clientRefusesWhenFullAndExplains734()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcomeMonitor(transport, QByteArrayLiteral("MONITOR=1"));
    controller.openStatus(QStringLiteral("libera"));

    QVERIFY(controller.console()->submit(QStringLiteral("/monitor alice")));
    QVERIFY(framesContain(transport->writtenFrames(),
                          QByteArrayLiteral("MONITOR + alice")));

    const int afterAlice = transport->writtenFrames().size();
    QVERIFY(controller.console()->submit(QStringLiteral("/monitor bob")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("Monitor list is full (1)")));
    QCOMPARE(transport->writtenFrames().size(), afterAlice);
    QVERIFY(!IrcMonitorStore().contains(
        QStringLiteral("libera"), QStringLiteral("bob"), IrcCaseMapping()));

    transport->injectBytes(
        QByteArrayLiteral(":server 734 omairc 1 alice :Monitor list is full.\r\n"));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("Monitor list is full (1): alice")));
    QVERIFY(!IrcMonitorStore().contains(
        QStringLiteral("libera"), QStringLiteral("alice"), IrcCaseMapping()));
}

void MonitorTest::hydrationDoesNotNotifyLaterEdgesDo()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcomeMonitor(transport);
    controller.openStatus(QStringLiteral("libera"));

    QSignalSpy spy(&controller, &IrcController::monitorArrived);
    QVERIFY(controller.console()->submit(QStringLiteral("/monitor alice")));
    transport->injectBytes(
        QByteArrayLiteral(":server 730 omairc :alice!user@host\r\n"));
    QCOMPARE(spy.count(), 0);
    QVERIFY(!logContains(controller.console()->lines(),
                         QStringLiteral("alice is online")));

    transport->injectBytes(QByteArrayLiteral(":server 731 * :alice\r\n"));
    QCOMPARE(spy.count(), 1);
    QCOMPARE(spy.at(0).at(0).toString(), QStringLiteral("alice"));
    QCOMPARE(spy.at(0).at(1).toString(), QStringLiteral("is offline"));
    QCOMPARE(spy.at(0).at(2).toString(), QStringLiteral("libera"));
    QCOMPARE(spy.at(0).at(3).toString(), QStringLiteral("alice"));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("alice is offline")));

    transport->injectBytes(
        QByteArrayLiteral(":server 730 * :alice!u@h,bob!u@h\r\n"));
    QCOMPARE(spy.count(), 2);
    QCOMPARE(spy.at(1).at(1).toString(), QStringLiteral("is online"));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("alice is online")));
}

void MonitorTest::mutedNickSkipsDesktopNotify()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcomeMonitor(transport);
    controller.openStatus(QStringLiteral("libera"));

    QVERIFY(controller.console()->submit(QStringLiteral("/monitor alice")));
    QVERIFY(controller.console()->submit(QStringLiteral("/mute alice")));
    transport->injectBytes(
        QByteArrayLiteral(":server 730 omairc :alice!u@h\r\n"));

    QSignalSpy spy(&controller, &IrcController::monitorArrived);
    transport->injectBytes(QByteArrayLiteral(":server 731 omairc :alice\r\n"));
    QCOMPARE(spy.count(), 0);
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("alice is offline")));
}

void MonitorTest::reconnectResubscribesWithoutDuplicateNotify()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcomeMonitor(transport);
    controller.openStatus(QStringLiteral("libera"));

    QSignalSpy spy(&controller, &IrcController::monitorArrived);
    QVERIFY(controller.console()->submit(QStringLiteral("/monitor alice")));
    transport->injectBytes(
        QByteArrayLiteral(":server 730 omairc :alice!u@h\r\n"));
    QCOMPARE(spy.count(), 0);

    QVERIFY(controller.discardSession(QStringLiteral("libera")));
    QVERIFY(IrcMonitorStore().contains(
        QStringLiteral("libera"), QStringLiteral("alice"), IrcCaseMapping()));

    auto *again = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), again));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcomeMonitor(again);
    QVERIFY(framesContain(again->writtenFrames(),
                          QByteArrayLiteral("MONITOR + alice")));
    again->injectBytes(QByteArrayLiteral(":server 730 * :alice!u@h\r\n"));
    QCOMPARE(spy.count(), 0);

    again->injectBytes(QByteArrayLiteral(":server 731 * :alice\r\n"));
    QCOMPARE(spy.count(), 1);
}

void MonitorTest::controllerForgetsWithTheNetwork()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcomeMonitor(transport);
    controller.openStatus(QStringLiteral("libera"));
    QVERIFY(controller.console()->submit(QStringLiteral("/monitor alice")));
    QVERIFY(IrcMonitorStore().contains(
        QStringLiteral("libera"), QStringLiteral("alice"), IrcCaseMapping()));
    controller.forgetNetworkState(QStringLiteral("libera"));
    QVERIFY(!IrcMonitorStore().contains(
        QStringLiteral("libera"), QStringLiteral("alice"), IrcCaseMapping()));
}

void MonitorTest::refusesBadNicks()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcomeMonitor(transport);
    controller.openStatus(QStringLiteral("libera"));

    QVERIFY(controller.console()->submit(QStringLiteral("/monitor")));
    QVERIFY(!controller.console()->lastSubmitAccepted());
    QVERIFY(controller.console()->submit(QStringLiteral("/monitor #omarchy")));
    QVERIFY(!controller.console()->lastSubmitAccepted());
    QVERIFY(controller.console()->submit(QStringLiteral("/monitor alice extra")));
    QVERIFY(!controller.console()->lastSubmitAccepted());
    QVERIFY(controller.console()->submit(QStringLiteral("/monitor alice!u@h")));
    QVERIFY(!controller.console()->lastSubmitAccepted());
    QVERIFY(controller.console()->submit(QStringLiteral("/monitored extra")));
    QVERIFY(!controller.console()->lastSubmitAccepted());
    QVERIFY(controller.console()->submit(QStringLiteral("/unmonitor *")));
    QVERIFY(!controller.console()->lastSubmitAccepted());
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("MONITOR +")));
}

void MonitorTest::offlineMonitorIsNotConnected()
{
    IrcController controller;
    QVERIFY(!controller.sendMessage(QStringLiteral("/monitor alice")));
    QCOMPARE(controller.lastError(),
             QStringLiteral("Select a connected conversation first"));
    QVERIFY(!IrcMonitorStore().contains(
        QStringLiteral("libera"), QStringLiteral("alice"), IrcCaseMapping()));

    controller.openStatus(QStringLiteral("libera"));
    QVERIFY(controller.console()->submit(QStringLiteral("/monitor alice")));
    QVERIFY(logContains(controller.console()->lines(), QStringLiteral("Not connected")));
    QVERIFY(!IrcMonitorStore().contains(
        QStringLiteral("libera"), QStringLiteral("alice"), IrcCaseMapping()));
}

int runMonitorTests(int argc, char **argv)
{
    MonitorTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_monitor.moc"
