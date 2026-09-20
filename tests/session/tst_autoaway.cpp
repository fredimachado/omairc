#include <QTest>

#include "ircautoaway.h"
#include "irccommand.h"
#include "ircslashcomplete.h"

#include <optional>

class AutoawayTest : public QObject
{
    Q_OBJECT

private slots:
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
};

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
    QCOMPARE(ircParseAutoawayDuration(QStringLiteral("20s")), std::optional<int>(20));
    QCOMPARE(ircParseAutoawayDuration(QStringLiteral("0.4")), std::optional<int>(24));
    QCOMPARE(ircParseAutoawayDuration(QStringLiteral("30s")), std::optional<int>(30));

    QCOMPARE(ircParseAutoawayArgument(QStringLiteral("20s")).kind,
             IrcAutoawayKind::Usage);
    QCOMPARE(ircParseAutoawayArgument(QStringLiteral("0.4")).kind,
             IrcAutoawayKind::Usage);
    const IrcAutoawayRequest accepted =
        ircParseAutoawayArgument(QStringLiteral("30s"));
    QCOMPARE(accepted.kind, IrcAutoawayKind::SetTimeout);
    QCOMPARE(accepted.timeoutSeconds, 30);
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

int runAutoawayTests(int argc, char **argv)
{
    AutoawayTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_autoaway.moc"
