#include <QAbstractItemModel>
#include <QCoreApplication>
#include <QEvent>
#include <QSettings>
#include <QTemporaryDir>
#include <QTest>

#include "fakeirctransport.h"
#include "ircautoaway.h"
#include "irccommand.h"
#include "irccontroller.h"
#include "ircsession.h"
#include "ircslashcomplete.h"
#include "ircstatusconsole.h"
#include "messagelistmodel.h"
#include "networklogmodel.h"
#include "testsettings.h"

#include <limits>
#include <memory>
#include <optional>

namespace
{
IrcSessionConfig config(const QString& networkId = QStringLiteral("libera"))
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

void welcome(FakeIrcTransport *transport)
{
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"));
}

bool selectedBodiesContain(QAbstractItemModel *messages, const QString& needle)
{
    if (!messages)
        return false;
    for (int row = 0; row < messages->rowCount(); ++row) {
        const QString body =
            messages->data(messages->index(row, 0), MessageListModel::BodyRole)
                .toString();
        if (body.contains(needle))
            return true;
    }
    return false;
}

bool selectedWhoisContains(QAbstractItemModel *messages, const QString& needle)
{
    if (!messages)
        return false;
    for (int row = 0; row < messages->rowCount(); ++row) {
        const QString kind =
            messages->data(messages->index(row, 0), MessageListModel::KindRole)
                .toString();
        const QString body =
            messages->data(messages->index(row, 0), MessageListModel::BodyRole)
                .toString();
        if (kind == QLatin1String("whois") && body.contains(needle))
            return true;
    }
    return false;
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

int awayFrameCount(const QByteArrayList& frames, int from = 0)
{
    int hits = 0;
    for (int i = from; i < frames.size(); ++i) {
        if (frames.at(i).startsWith("AWAY"))
            ++hits;
    }
    return hits;
}

void injectNowAway(FakeIrcTransport *transport)
{
    transport->injectBytes(
        QByteArrayLiteral(":server 306 omairc :You have been marked as being away\r\n"));
}

void injectUnaway(FakeIrcTransport *transport)
{
    transport->injectBytes(
        QByteArrayLiteral(":server 305 omairc :You are no longer marked as being away\r\n"));
}

void postAppEvent(QEvent::Type type)
{
    QEvent event(type);
    QCoreApplication::sendEvent(QCoreApplication::instance(), &event);
}

FakeIrcTransport *joinNetwork(IrcController& controller, const QString& networkId,
                              const QString& channel = QStringLiteral("#omarchy"))
{
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(networkId), transport);
    if (!session)
        return nullptr;
    if (!controller.start(networkId))
        return nullptr;
    welcome(transport);
    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :") + channel.toUtf8()
        + QByteArrayLiteral("\r\n"));
    return transport;
}
}

class AutoawayTest : public QObject
{
    Q_OBJECT

private slots:
    void init();
    void parseBareNumbersAsMinutes();
    void parseSuffixesCaseInsensitive();
    void refuseBelowTimeoutFloor();
    void disableOffAndZero();
    void enableOn();
    void defaultReasonCommands();
    void refuseBareText();
    void timeoutKeepsOrSetsOneShot();
    void confirmationAndQueryCopy();
    void parseAndCatalog();
    void enableDisablePersistsAndQueries();
    void oneShotDoesNotOverwriteDefault();
    void bareTextRefusedDoesNotWriteAway();
    void tripAwaysAllRegisteredNetworksOnce();
    void activityDuringGraceCancelsAway();
    void manualAwayWinsOnThatNetwork();
    void disableWhileActiveClearsAutoAway();
    void sendToTargetClearsAutoAway();
    void incomingPrivmsgDoesNotClearAutoAway();
    void restoreOnAfterOff();
    void emptyAwayClearsProvenanceSoAutoTripMarks();
    void loadBelowFloorTimeoutDisables();
    void reasonChangeWhileTrippedRewritesAway();
    void refuseTimeoutAtOrAboveTimerCap();
    void reconnectWhileTrippedSendsAway();
    void reconnectDropsManualAwaySoAutoTripMarks();
    void activityAfterEmptyTripClearsOneShot();
    void wheelClearsAutoAway();
    void composerNotifyDoesNotClearAutoAway();
    void disableBeforeTripDropsOneShot();

private:
    std::unique_ptr<QTemporaryDir> m_settingsDir;
};

void AutoawayTest::init()
{
    m_settingsDir = std::make_unique<QTemporaryDir>();
    QVERIFY(m_settingsDir->isValid());
    TestSettings::isolate(m_settingsDir->path());
    QCoreApplication::setOrganizationName(QStringLiteral("omairc"));
    QCoreApplication::setApplicationName(QStringLiteral("omairc"));
}

void AutoawayTest::parseBareNumbersAsMinutes()
{
    QCOMPARE(ircParseAutoawayDuration(QStringLiteral("15")), std::optional<int>(15 * 60));
    QCOMPARE(ircParseAutoawayDuration(QStringLiteral("1")), std::optional<int>(60));

    const IrcAutoawayRequest request =
        ircParseAutoawayArgument(QStringLiteral("15"));
    QCOMPARE(request.kind, IrcAutoawayKind::SetTimeout);
    QCOMPARE(request.timeoutSeconds, 15 * 60);
    QVERIFY(request.text.isEmpty());
}

void AutoawayTest::parseSuffixesCaseInsensitive()
{
    QCOMPARE(ircParseAutoawayDuration(QStringLiteral("90s")), std::optional<int>(90));
    QCOMPARE(ircParseAutoawayDuration(QStringLiteral("15m")), std::optional<int>(15 * 60));
    QCOMPARE(ircParseAutoawayDuration(QStringLiteral("1h")), std::optional<int>(3600));
    QCOMPARE(ircParseAutoawayDuration(QStringLiteral("15M")), std::optional<int>(15 * 60));
    QCOMPARE(ircParseAutoawayDuration(QStringLiteral("90S")), std::optional<int>(90));
    QCOMPARE(ircParseAutoawayDuration(QStringLiteral("1H")), std::optional<int>(3600));
    QVERIFY(!ircParseAutoawayDuration(QString()));
    QVERIFY(!ircParseAutoawayDuration(QStringLiteral("15x")));
    QVERIFY(!ircParseAutoawayDuration(QStringLiteral("lunch")));
}

void AutoawayTest::refuseBelowTimeoutFloor()
{
    QVERIFY(!ircParseAutoawayDuration(QStringLiteral("20s")));
    QVERIFY(!ircParseAutoawayDuration(QStringLiteral("0.4")));
    QVERIFY(!ircParseAutoawayDuration(QStringLiteral("29.5s")));
    QVERIFY(!ircParseAutoawayDuration(QStringLiteral("0.49m")));
    QCOMPARE(ircParseAutoawayDuration(QStringLiteral("30s")), std::optional<int>(30));
    QCOMPARE(ircParseAutoawayDuration(QStringLiteral("0.5m")), std::optional<int>(30));

    QCOMPARE(ircParseAutoawayArgument(QStringLiteral("20s")).kind,
             IrcAutoawayKind::Usage);
    QCOMPARE(ircParseAutoawayArgument(QStringLiteral("0.4")).kind,
             IrcAutoawayKind::Usage);
    QCOMPARE(ircParseAutoawayArgument(QStringLiteral("29.5s")).kind,
             IrcAutoawayKind::Usage);
    QCOMPARE(ircParseAutoawayArgument(QStringLiteral("0.49m")).kind,
             IrcAutoawayKind::Usage);
    const IrcAutoawayRequest accepted =
        ircParseAutoawayArgument(QStringLiteral("30s"));
    QCOMPARE(accepted.kind, IrcAutoawayKind::SetTimeout);
    QCOMPARE(accepted.timeoutSeconds, 30);
    const IrcAutoawayRequest halfMinute =
        ircParseAutoawayArgument(QStringLiteral("0.5m"));
    QCOMPARE(halfMinute.kind, IrcAutoawayKind::SetTimeout);
    QCOMPARE(halfMinute.timeoutSeconds, 30);
}

void AutoawayTest::refuseTimeoutAtOrAboveTimerCap()
{
    QCOMPARE(ircAutoawayMaxTimeoutSeconds,
             std::numeric_limits<int>::max() / 1000);
    const QString justBelow =
        QString::number(ircAutoawayMaxTimeoutSeconds) + QLatin1Char('s');
    const QString atCap =
        QString::number(ircAutoawayMaxTimeoutSeconds + 1) + QLatin1Char('s');
    QCOMPARE(ircParseAutoawayDuration(justBelow),
             std::optional<int>(ircAutoawayMaxTimeoutSeconds));
    QVERIFY(!ircParseAutoawayDuration(atCap));
    QVERIFY(!ircParseAutoawayDuration(QStringLiteral("600h")));

    const IrcAutoawayRequest acceptedCap = ircParseAutoawayArgument(justBelow);
    QCOMPARE(acceptedCap.kind, IrcAutoawayKind::SetTimeout);
    QCOMPARE(acceptedCap.timeoutSeconds, ircAutoawayMaxTimeoutSeconds);
    QCOMPARE(ircParseAutoawayArgument(atCap).kind, IrcAutoawayKind::Usage);
    QCOMPARE(ircParseAutoawayArgument(QStringLiteral("600h")).kind,
             IrcAutoawayKind::Usage);
}

void AutoawayTest::disableOffAndZero()
{
    QCOMPARE(ircParseAutoawayArgument(QStringLiteral("0")).kind,
             IrcAutoawayKind::Disable);
    QCOMPARE(ircParseAutoawayArgument(QStringLiteral("off")).kind,
             IrcAutoawayKind::Disable);
    QCOMPARE(ircParseAutoawayArgument(QStringLiteral("OFF")).kind,
             IrcAutoawayKind::Disable);
    QCOMPARE(ircParseAutoawayArgument(QStringLiteral("0 extra")).kind,
             IrcAutoawayKind::Usage);
    QCOMPARE(ircParseAutoawayArgument(QStringLiteral("off extra")).kind,
             IrcAutoawayKind::Usage);
    QCOMPARE(ircParseAutoawayArgument(QStringLiteral("0s")).kind,
             IrcAutoawayKind::Usage);
}

void AutoawayTest::enableOn()
{
    QCOMPARE(ircParseAutoawayArgument(QStringLiteral("on")).kind,
             IrcAutoawayKind::EnableOn);
    QCOMPARE(ircParseAutoawayArgument(QStringLiteral("ON")).kind,
             IrcAutoawayKind::EnableOn);
    QCOMPARE(ircParseAutoawayArgument(QStringLiteral("on extra")).kind,
             IrcAutoawayKind::Usage);
}

void AutoawayTest::defaultReasonCommands()
{
    const IrcAutoawayRequest set =
        ircParseAutoawayArgument(QStringLiteral("reason AFK"));
    QCOMPARE(set.kind, IrcAutoawayKind::SetDefaultReason);
    QCOMPARE(set.text, QStringLiteral("AFK"));

    QCOMPARE(ircParseAutoawayArgument(QStringLiteral("reason")).kind,
             IrcAutoawayKind::ClearDefaultReason);
    QCOMPARE(ircParseAutoawayArgument(QStringLiteral("REASON")).kind,
             IrcAutoawayKind::ClearDefaultReason);

    const IrcAutoawayRequest reserved =
        ircParseAutoawayArgument(QStringLiteral("reason off"));
    QCOMPARE(reserved.kind, IrcAutoawayKind::SetDefaultReason);
    QCOMPARE(reserved.text, QStringLiteral("off"));
}

void AutoawayTest::refuseBareText()
{
    QCOMPARE(ircParseAutoawayArgument(QStringLiteral("Sleeping")).kind,
             IrcAutoawayKind::Usage);
    QCOMPARE(ircParseAutoawayArgument(QString()).kind, IrcAutoawayKind::Query);
}

void AutoawayTest::timeoutKeepsOrSetsOneShot()
{
    const IrcAutoawayRequest oneShot =
        ircParseAutoawayArgument(QStringLiteral("15m Stepped out for lunch"));
    QCOMPARE(oneShot.kind, IrcAutoawayKind::SetTimeout);
    QCOMPARE(oneShot.timeoutSeconds, 15 * 60);
    QCOMPARE(oneShot.text, QStringLiteral("Stepped out for lunch"));

    const IrcAutoawayRequest keep =
        ircParseAutoawayArgument(QStringLiteral("15"));
    QCOMPARE(keep.kind, IrcAutoawayKind::SetTimeout);
    QCOMPARE(keep.timeoutSeconds, 15 * 60);
    QVERIFY(keep.text.isEmpty());
}

void AutoawayTest::confirmationAndQueryCopy()
{
    IrcAutoawayConfig config;
    QCOMPARE(ircFormatAutoawayQuery(config), QStringLiteral("Auto-away off"));
    QCOMPARE(ircFormatAutoawayConfirmation(config), QStringLiteral("Auto-away off"));

    config.timeoutSeconds = 15 * 60;
    config.defaultReason = QStringLiteral("AFK");
    QCOMPARE(ircFormatAutoawayQuery(config),
             QStringLiteral("Auto-away off, 15 minutes, reason: AFK"));
    QCOMPARE(ircFormatAutoawayConfirmation(config),
             QStringLiteral("Auto-away off, reason: AFK"));

    config.enabled = true;
    QCOMPARE(ircFormatAutoawayConfirmation(config),
             QStringLiteral("Auto-away 15 minutes, reason: AFK"));
    QCOMPARE(ircFormatAutoawayQuery(config),
             QStringLiteral("Auto-away 15 minutes, reason: AFK"));

    config.oneShotReason = QStringLiteral("Stepped out for lunch");
    QCOMPARE(ircFormatAutoawayConfirmation(config),
             QStringLiteral("Auto-away in 15 minutes: Stepped out for lunch (this time only)"));
    QCOMPARE(ircFormatAutoawayQuery(config),
             QStringLiteral("Auto-away in 15 minutes: Stepped out for lunch (this time only), reason: AFK"));

    config.defaultReason.clear();
    QCOMPARE(ircFormatAutoawayDuration(90), QStringLiteral("90 seconds"));
    QCOMPARE(ircFormatAutoawayDuration(3600), QStringLiteral("1 hour"));
    QCOMPARE(ircAutoawayGraceSeconds(15 * 60), 10);
    QCOMPARE(ircAutoawayGraceSeconds(30), 5);
}

void AutoawayTest::parseAndCatalog()
{
    QCOMPARE(IrcCommand::parse(QStringLiteral("/autoaway")).verb,
             IrcCommand::Verb::Autoaway);
    QCOMPARE(IrcCommand::parse(QStringLiteral("/AUTOAWAY 15")).verb,
             IrcCommand::Verb::Autoaway);
    QCOMPARE(IrcCommand::parse(QStringLiteral("/autoaway 15")).argument,
             QStringLiteral("15"));
    QVERIFY(IrcCommand::parse(QStringLiteral("/autoaway"))
                .allowedOn(IrcComposerSurface::Status));
    QVERIFY(IrcCommand::parse(QStringLiteral("/autoaway"))
                .allowedOn(IrcComposerSurface::Conversation));
    QCOMPARE(IrcVerbTable::all().size(), 46);
    QCOMPARE(IrcVerbTable::visibleOn(IrcComposerSurface::Status).size(), 38);
    QCOMPARE(IrcVerbTable::visibleOn(IrcComposerSurface::Conversation).size(), 46);
    const auto probe = IrcSlashComplete::project(
        QStringLiteral("/auto"), IrcComposerSurface::Conversation);
    QVERIFY(probe.isOpen());
    QVERIFY(probe.containsLabel(QStringLiteral("/autoaway")));
}

void AutoawayTest::enableDisablePersistsAndQueries()
{
    {
        IrcController controller;
        auto *transport = joinNetwork(controller, QStringLiteral("libera"));
        QVERIFY(transport);
        controller.selectConversation(QStringLiteral("libera"),
                                      QStringLiteral("#omarchy"));
        auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
        QVERIFY(messages);

        QVERIFY(controller.sendMessage(QStringLiteral("/autoaway 15")));
        QVERIFY(selectedWhoisContains(messages, QStringLiteral("Auto-away 15 minutes")));
        QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("AWAY")));

        QVERIFY(controller.sendMessage(QStringLiteral("/autoaway")));
        QVERIFY(selectedWhoisContains(messages, QStringLiteral("Auto-away 15 minutes")));

        QVERIFY(controller.sendMessage(QStringLiteral("/autoaway off")));
        QVERIFY(selectedWhoisContains(messages, QStringLiteral("Auto-away off")));
    }

    {
        IrcController reloaded;
        auto *transport = joinNetwork(reloaded, QStringLiteral("libera"));
        QVERIFY(transport);
        reloaded.selectConversation(QStringLiteral("libera"),
                                    QStringLiteral("#omarchy"));
        auto *messages = qobject_cast<QAbstractItemModel *>(reloaded.messages());
        QVERIFY(messages);
        QVERIFY(reloaded.sendMessage(QStringLiteral("/autoaway")));
        QVERIFY(selectedWhoisContains(
            messages, QStringLiteral("Auto-away off, 15 minutes")));
    }
}

void AutoawayTest::oneShotDoesNotOverwriteDefault()
{
    IrcController controller;
    auto *transport = joinNetwork(controller, QStringLiteral("libera"));
    QVERIFY(transport);
    controller.selectConversation(QStringLiteral("libera"),
                                  QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);

    QVERIFY(controller.sendMessage(QStringLiteral("/autoaway reason AFK")));
    QVERIFY(controller.sendMessage(
        QStringLiteral("/autoaway 15m Stepped out for lunch")));
    QVERIFY(selectedWhoisContains(
        messages,
        QStringLiteral("Auto-away in 15 minutes: Stepped out for lunch (this time only)")));
    QVERIFY(!selectedWhoisContains(
        messages,
        QStringLiteral("Auto-away in 15 minutes: Stepped out for lunch (this time only), reason: AFK")));

    QVERIFY(controller.sendMessage(QStringLiteral("/autoaway")));
    QVERIFY(selectedWhoisContains(
        messages,
        QStringLiteral("Auto-away in 15 minutes: Stepped out for lunch (this time only), reason: AFK")));

    controller.fireAutoawayIdleForTest();
    controller.fireAutoawayGraceForTest();
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("AWAY :Stepped out for lunch\r\n"));
    injectNowAway(transport);
    QVERIFY(controller.selfAway());

    controller.noteLocalActivity();
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("AWAY\r\n"));
    injectUnaway(transport);
    QVERIFY(!controller.selfAway());

    const int afterBack = transport->writtenFrames().size();
    controller.fireAutoawayIdleForTest();
    controller.fireAutoawayGraceForTest();
    QCOMPARE(transport->writtenFrames().size(), afterBack + 1);
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("AWAY :AFK\r\n"));
    injectNowAway(transport);
    QVERIFY(controller.selfAway());
}

void AutoawayTest::bareTextRefusedDoesNotWriteAway()
{
    IrcController controller;
    auto *transport = joinNetwork(controller, QStringLiteral("libera"));
    QVERIFY(transport);
    controller.selectConversation(QStringLiteral("libera"),
                                  QStringLiteral("#omarchy"));
    const int before = transport->writtenFrames().size();
    QVERIFY(controller.sendMessage(QStringLiteral("/autoaway Sleeping")));
    QVERIFY(selectedWhoisContains(
        qobject_cast<QAbstractItemModel *>(controller.messages()),
        QStringLiteral("/autoaway [off|on|duration [reason]|reason [text]]")));
    QCOMPARE(transport->writtenFrames().size(), before);
    QVERIFY(!framesContain(transport->writtenFrames().mid(before),
                           QByteArrayLiteral("AWAY")));
}

void AutoawayTest::tripAwaysAllRegisteredNetworksOnce()
{
    IrcController controller;
    auto *transportA = joinNetwork(controller, QStringLiteral("network-a"));
    auto *transportB = joinNetwork(controller, QStringLiteral("network-b"));
    QVERIFY(transportA);
    QVERIFY(transportB);
    controller.selectConversation(QStringLiteral("network-a"),
                                  QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("/autoaway 15")));

    const int beforeA = transportA->writtenFrames().size();
    const int beforeB = transportB->writtenFrames().size();
    controller.fireAutoawayIdleForTest();
    controller.fireAutoawayGraceForTest();
    QCOMPARE(awayFrameCount(transportA->writtenFrames(), beforeA), 1);
    QCOMPARE(awayFrameCount(transportB->writtenFrames(), beforeB), 1);
    QCOMPARE(transportA->writtenFrames().last(), QByteArrayLiteral("AWAY :\r\n"));
    QCOMPARE(transportB->writtenFrames().last(), QByteArrayLiteral("AWAY :\r\n"));
    injectNowAway(transportA);
    QVERIFY(controller.selfAway());
    controller.selectConversation(QStringLiteral("network-b"),
                                  QStringLiteral("#omarchy"));
    injectNowAway(transportB);
    QVERIFY(controller.selfAway());

    controller.fireAutoawayIdleForTest();
    controller.fireAutoawayGraceForTest();
    QCOMPARE(awayFrameCount(transportA->writtenFrames(), beforeA), 1);
    QCOMPARE(awayFrameCount(transportB->writtenFrames(), beforeB), 1);
}

void AutoawayTest::activityDuringGraceCancelsAway()
{
    IrcController controller;
    auto *transport = joinNetwork(controller, QStringLiteral("libera"));
    QVERIFY(transport);
    controller.selectConversation(QStringLiteral("libera"),
                                  QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("/autoaway 15")));
    const int before = transport->writtenFrames().size();
    controller.fireAutoawayIdleForTest();
    controller.noteLocalActivity();
    controller.fireAutoawayGraceForTest();
    QCOMPARE(transport->writtenFrames().size(), before);
    QVERIFY(!framesContain(transport->writtenFrames().mid(before),
                           QByteArrayLiteral("AWAY")));
}

void AutoawayTest::manualAwayWinsOnThatNetwork()
{
    IrcController controller;
    auto *transportA = joinNetwork(controller, QStringLiteral("network-a"));
    auto *transportB = joinNetwork(controller, QStringLiteral("network-b"));
    QVERIFY(transportA);
    QVERIFY(transportB);
    controller.selectConversation(QStringLiteral("network-a"),
                                  QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("/autoaway 15")));
    QVERIFY(controller.sendMessage(QStringLiteral("/away lunch")));
    QCOMPARE(transportA->writtenFrames().last(),
             QByteArrayLiteral("AWAY :lunch\r\n"));

    const int beforeA = transportA->writtenFrames().size();
    const int beforeB = transportB->writtenFrames().size();
    controller.fireAutoawayIdleForTest();
    controller.fireAutoawayGraceForTest();
    QCOMPARE(awayFrameCount(transportA->writtenFrames(), beforeA), 0);
    QCOMPARE(transportA->writtenFrames().last(),
             QByteArrayLiteral("AWAY :lunch\r\n"));
    QCOMPARE(awayFrameCount(transportB->writtenFrames(), beforeB), 1);
    QCOMPARE(transportB->writtenFrames().last(), QByteArrayLiteral("AWAY :\r\n"));
    controller.selectConversation(QStringLiteral("network-b"),
                                  QStringLiteral("#omarchy"));
    injectNowAway(transportB);
    QVERIFY(controller.selfAway());
}

void AutoawayTest::disableWhileActiveClearsAutoAway()
{
    IrcController controller;
    auto *transportA = joinNetwork(controller, QStringLiteral("network-a"));
    auto *transportB = joinNetwork(controller, QStringLiteral("network-b"));
    QVERIFY(transportA);
    QVERIFY(transportB);
    controller.selectConversation(QStringLiteral("network-a"),
                                  QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("/autoaway 15")));
    controller.fireAutoawayIdleForTest();
    controller.fireAutoawayGraceForTest();
    QCOMPARE(transportA->writtenFrames().last(), QByteArrayLiteral("AWAY :\r\n"));
    QCOMPARE(transportB->writtenFrames().last(), QByteArrayLiteral("AWAY :\r\n"));
    injectNowAway(transportA);
    QVERIFY(controller.selfAway());
    controller.selectConversation(QStringLiteral("network-b"),
                                  QStringLiteral("#omarchy"));
    injectNowAway(transportB);
    QVERIFY(controller.selfAway());

    const int beforeA = transportA->writtenFrames().size();
    const int beforeB = transportB->writtenFrames().size();
    QVERIFY(controller.sendMessage(QStringLiteral("/autoaway off")));
    QCOMPARE(awayFrameCount(transportA->writtenFrames(), beforeA), 1);
    QCOMPARE(awayFrameCount(transportB->writtenFrames(), beforeB), 1);
    QCOMPARE(transportA->writtenFrames().at(beforeA), QByteArrayLiteral("AWAY\r\n"));
    QCOMPARE(transportB->writtenFrames().at(beforeB), QByteArrayLiteral("AWAY\r\n"));
    QVERIFY(selectedWhoisContains(
        qobject_cast<QAbstractItemModel *>(controller.messages()),
        QStringLiteral("Auto-away off")));
    injectUnaway(transportB);
    QVERIFY(!controller.selfAway());
    controller.selectConversation(QStringLiteral("network-a"),
                                  QStringLiteral("#omarchy"));
    injectUnaway(transportA);
    QVERIFY(!controller.selfAway());
}

void AutoawayTest::sendToTargetClearsAutoAway()
{
    IrcController controller;
    auto *transportA = joinNetwork(controller, QStringLiteral("network-a"));
    auto *transportB = joinNetwork(controller, QStringLiteral("network-b"));
    QVERIFY(transportA);
    QVERIFY(transportB);
    controller.selectConversation(QStringLiteral("network-a"),
                                  QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("/autoaway 15")));
    controller.fireAutoawayIdleForTest();
    controller.fireAutoawayGraceForTest();

    const int beforeA = transportA->writtenFrames().size();
    const int beforeB = transportB->writtenFrames().size();
    QVERIFY(controller.sendToTarget(QStringLiteral("network-a"),
                                    QStringLiteral("#omarchy"),
                                    QStringLiteral("still here")));
    QVERIFY(framesContain(transportA->writtenFrames().mid(beforeA),
                          QByteArrayLiteral("PRIVMSG #omarchy :still here\r\n")));
    QCOMPARE(awayFrameCount(transportA->writtenFrames(), beforeA), 1);
    QCOMPARE(awayFrameCount(transportB->writtenFrames(), beforeB), 1);
    QCOMPARE(transportA->writtenFrames().last(), QByteArrayLiteral("AWAY\r\n"));
    QCOMPARE(transportB->writtenFrames().last(), QByteArrayLiteral("AWAY\r\n"));
}

void AutoawayTest::incomingPrivmsgDoesNotClearAutoAway()
{
    IrcController controller;
    auto *transport = joinNetwork(controller, QStringLiteral("libera"));
    QVERIFY(transport);
    controller.selectConversation(QStringLiteral("libera"),
                                  QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("/autoaway 15")));
    controller.fireAutoawayIdleForTest();
    controller.fireAutoawayGraceForTest();
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("AWAY :\r\n"));
    injectNowAway(transport);
    QVERIFY(controller.selfAway());
    const int afterAway = transport->writtenFrames().size();
    transport->injectBytes(
        QByteArrayLiteral(":alice!u@h PRIVMSG #omarchy :hey\r\n"));
    QCOMPARE(transport->writtenFrames().size(), afterAway);
    QVERIFY(controller.selfAway());
    QVERIFY(selectedBodiesContain(
        qobject_cast<QAbstractItemModel *>(controller.messages()),
        QStringLiteral("hey")));
}

void AutoawayTest::restoreOnAfterOff()
{
    IrcController controller;
    auto *transport = joinNetwork(controller, QStringLiteral("libera"));
    QVERIFY(transport);
    controller.selectConversation(QStringLiteral("libera"),
                                  QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(controller.sendMessage(QStringLiteral("/autoaway on")));
    QVERIFY(selectedWhoisContains(
        messages,
        QStringLiteral("/autoaway [off|on|duration [reason]|reason [text]]")));

    QVERIFY(controller.sendMessage(QStringLiteral("/autoaway 15")));
    QVERIFY(controller.sendMessage(QStringLiteral("/autoaway off")));
    QVERIFY(controller.sendMessage(QStringLiteral("/autoaway on")));
    QVERIFY(selectedWhoisContains(messages, QStringLiteral("Auto-away 15 minutes")));

    IrcStatusConsole *console = controller.console();
    QVERIFY(console);
    QVERIFY(console->submit(QStringLiteral("/autoaway")));
    QVERIFY(logContains(console->lines(), QStringLiteral("Auto-away 15 minutes")));
}

void AutoawayTest::emptyAwayClearsProvenanceSoAutoTripMarks()
{
    IrcController controller;
    auto *transport = joinNetwork(controller, QStringLiteral("libera"));
    QVERIFY(transport);
    controller.selectConversation(QStringLiteral("libera"),
                                  QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("/autoaway 15")));
    QVERIFY(controller.sendMessage(QStringLiteral("/away lunch")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("AWAY :lunch\r\n"));
    QVERIFY(controller.sendMessage(QStringLiteral("/away")));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("AWAY\r\n"));

    const int before = transport->writtenFrames().size();
    controller.fireAutoawayIdleForTest();
    controller.fireAutoawayGraceForTest();
    QCOMPARE(awayFrameCount(transport->writtenFrames(), before), 1);
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("AWAY :\r\n"));
    injectNowAway(transport);
    QVERIFY(controller.selfAway());
}

void AutoawayTest::loadBelowFloorTimeoutDisables()
{
    {
        QSettings settings;
        settings.beginGroup(QStringLiteral("preferences"));
        settings.setValue(QStringLiteral("autoawayEnabled"), true);
        settings.setValue(QStringLiteral("autoawayTimeoutSeconds"), 10);
        settings.endGroup();
        settings.sync();
    }

    IrcController controller;
    auto *transport = joinNetwork(controller, QStringLiteral("libera"));
    QVERIFY(transport);
    controller.selectConversation(QStringLiteral("libera"),
                                  QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(controller.sendMessage(QStringLiteral("/autoaway")));
    QVERIFY(selectedWhoisContains(messages, QStringLiteral("Auto-away off")));
    QVERIFY(!selectedWhoisContains(messages, QStringLiteral("10 seconds")));

    QVERIFY(controller.sendMessage(QStringLiteral("/autoaway on")));
    QVERIFY(selectedWhoisContains(
        messages,
        QStringLiteral("/autoaway [off|on|duration [reason]|reason [text]]")));

    const int before = transport->writtenFrames().size();
    controller.fireAutoawayIdleForTest();
    controller.fireAutoawayGraceForTest();
    QCOMPARE(awayFrameCount(transport->writtenFrames(), before), 0);
}

void AutoawayTest::reasonChangeWhileTrippedRewritesAway()
{
    IrcController controller;
    auto *transport = joinNetwork(controller, QStringLiteral("libera"));
    QVERIFY(transport);
    controller.selectConversation(QStringLiteral("libera"),
                                  QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("/autoaway reason AFK")));
    QVERIFY(controller.sendMessage(QStringLiteral("/autoaway 15")));
    controller.fireAutoawayIdleForTest();
    controller.fireAutoawayGraceForTest();
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("AWAY :AFK\r\n"));
    injectNowAway(transport);
    QVERIFY(controller.selfAway());

    QVERIFY(controller.sendMessage(QStringLiteral("/autoaway reason lunch")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("AWAY :lunch\r\n"));

    QVERIFY(controller.sendMessage(
        QStringLiteral("/autoaway 15m Stepped out")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("AWAY :Stepped out\r\n"));

    const int afterOneShot = transport->writtenFrames().size();
    QVERIFY(controller.sendMessage(QStringLiteral("/autoaway reason AFK")));
    QCOMPARE(transport->writtenFrames().size(), afterOneShot);
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("AWAY :Stepped out\r\n"));
}

void AutoawayTest::reconnectWhileTrippedSendsAway()
{
    IrcController controller;
    auto *transportA = joinNetwork(controller, QStringLiteral("network-a"));
    auto *transportB = joinNetwork(controller, QStringLiteral("network-b"));
    QVERIFY(transportA);
    QVERIFY(transportB);
    controller.selectConversation(QStringLiteral("network-a"),
                                  QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("/autoaway 15")));
    controller.fireAutoawayIdleForTest();
    controller.fireAutoawayGraceForTest();
    QCOMPARE(transportA->writtenFrames().last(), QByteArrayLiteral("AWAY :\r\n"));
    QCOMPARE(transportB->writtenFrames().last(), QByteArrayLiteral("AWAY :\r\n"));

    const int beforeB = transportB->writtenFrames().size();
    transportA->remoteClose();
    QVERIFY(controller.start(QStringLiteral("network-a")));
    const int beforeA = transportA->writtenFrames().size();
    welcome(transportA);
    QCOMPARE(awayFrameCount(transportA->writtenFrames(), beforeA), 1);
    QVERIFY(framesContain(transportA->writtenFrames().mid(beforeA),
                          QByteArrayLiteral("AWAY :\r\n")));
    injectNowAway(transportA);
    QVERIFY(controller.selfAway());
    QCOMPARE(transportB->writtenFrames().size(), beforeB);
}

void AutoawayTest::reconnectDropsManualAwaySoAutoTripMarks()
{
    IrcController controller;
    auto *transportA = joinNetwork(controller, QStringLiteral("network-a"));
    auto *transportB = joinNetwork(controller, QStringLiteral("network-b"));
    QVERIFY(transportA);
    QVERIFY(transportB);
    controller.selectConversation(QStringLiteral("network-a"),
                                  QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("/autoaway 15")));
    QVERIFY(controller.sendMessage(QStringLiteral("/away lunch")));
    QCOMPARE(transportA->writtenFrames().last(),
             QByteArrayLiteral("AWAY :lunch\r\n"));

    transportA->remoteClose();
    QVERIFY(controller.start(QStringLiteral("network-a")));
    const int beforeA = transportA->writtenFrames().size();
    welcome(transportA);
    QCOMPARE(awayFrameCount(transportA->writtenFrames(), beforeA), 0);

    const int afterWelcomeA = transportA->writtenFrames().size();
    const int beforeB = transportB->writtenFrames().size();
    controller.fireAutoawayIdleForTest();
    controller.fireAutoawayGraceForTest();
    QCOMPARE(awayFrameCount(transportA->writtenFrames(), afterWelcomeA), 1);
    QCOMPARE(transportA->writtenFrames().last(), QByteArrayLiteral("AWAY :\r\n"));
    QCOMPARE(awayFrameCount(transportB->writtenFrames(), beforeB), 1);
}

void AutoawayTest::activityAfterEmptyTripClearsOneShot()
{
    IrcController controller;
    auto *transport = joinNetwork(controller, QStringLiteral("libera"));
    QVERIFY(transport);
    controller.selectConversation(QStringLiteral("libera"),
                                  QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("/autoaway reason AFK")));
    QVERIFY(controller.sendMessage(
        QStringLiteral("/autoaway 15m Stepped out for lunch")));

    transport->remoteClose();
    const int afterClose = transport->writtenFrames().size();
    controller.fireAutoawayIdleForTest();
    controller.fireAutoawayGraceForTest();
    QCOMPARE(awayFrameCount(transport->writtenFrames(), afterClose), 0);

    controller.noteLocalActivity();
    QVERIFY(controller.start(QStringLiteral("libera")));
    const int beforeWelcome = transport->writtenFrames().size();
    welcome(transport);
    QCOMPARE(awayFrameCount(transport->writtenFrames(), beforeWelcome), 0);

    const int afterWelcome = transport->writtenFrames().size();
    controller.fireAutoawayIdleForTest();
    controller.fireAutoawayGraceForTest();
    QCOMPARE(awayFrameCount(transport->writtenFrames(), afterWelcome), 1);
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("AWAY :AFK\r\n"));
    injectNowAway(transport);
    QVERIFY(controller.selfAway());
}

void AutoawayTest::wheelClearsAutoAway()
{
    IrcController controller;
    auto *transport = joinNetwork(controller, QStringLiteral("libera"));
    QVERIFY(transport);
    controller.selectConversation(QStringLiteral("libera"),
                                  QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("/autoaway 15")));
    controller.fireAutoawayIdleForTest();
    controller.fireAutoawayGraceForTest();
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("AWAY :\r\n"));
    injectNowAway(transport);
    QVERIFY(controller.selfAway());

    const int before = transport->writtenFrames().size();
    postAppEvent(QEvent::Wheel);
    QCOMPARE(awayFrameCount(transport->writtenFrames(), before), 1);
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("AWAY\r\n"));
}

void AutoawayTest::composerNotifyDoesNotClearAutoAway()
{
    IrcController controller;
    auto *transport = joinNetwork(controller, QStringLiteral("libera"));
    QVERIFY(transport);
    controller.selectConversation(QStringLiteral("libera"),
                                  QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("/autoaway 15")));
    controller.fireAutoawayIdleForTest();
    controller.fireAutoawayGraceForTest();
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("AWAY :\r\n"));
    injectNowAway(transport);
    QVERIFY(controller.selfAway());

    const int afterAway = transport->writtenFrames().size();
    controller.notifyComposerText(QStringLiteral("hello"));
    QCOMPARE(transport->writtenFrames().size(), afterAway);
    QVERIFY(controller.selfAway());

    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#linux\r\n"));
    controller.selectConversation(QStringLiteral("libera"),
                                  QStringLiteral("#linux"));
    QCOMPARE(transport->writtenFrames().size(), afterAway);
    QVERIFY(controller.selfAway());
}

void AutoawayTest::disableBeforeTripDropsOneShot()
{
    IrcController controller;
    auto *transport = joinNetwork(controller, QStringLiteral("libera"));
    QVERIFY(transport);
    controller.selectConversation(QStringLiteral("libera"),
                                  QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);

    QVERIFY(controller.sendMessage(QStringLiteral("/autoaway reason AFK")));
    QVERIFY(controller.sendMessage(
        QStringLiteral("/autoaway 15m Stepped out for lunch")));
    QVERIFY(controller.sendMessage(QStringLiteral("/autoaway off")));
    QVERIFY(controller.sendMessage(QStringLiteral("/autoaway")));
    QVERIFY(selectedWhoisContains(
        messages, QStringLiteral("Auto-away off, 15 minutes, reason: AFK")));

    QVERIFY(controller.sendMessage(QStringLiteral("/autoaway on")));
    QVERIFY(selectedWhoisContains(
        messages, QStringLiteral("Auto-away 15 minutes, reason: AFK")));

    controller.fireAutoawayIdleForTest();
    controller.fireAutoawayGraceForTest();
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("AWAY :AFK\r\n"));
}

int runAutoawayTests(int argc, char **argv)
{
    AutoawayTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_autoaway.moc"
