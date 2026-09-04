#include <QTest>

#include <string>
#include <variant>

#include "irccasemapping.h"
#include "ircevent.h"
#include "irceventtranslator.h"
#include "ircparser.h"
#include "ircserverfeatures.h"

class CaseMappingTest : public QObject
{
    Q_OBJECT

private slots:
    void normalizesAdvertisedMappings();
    void parsesServerFeatures();
    void defaultPrefixStripsOwnerBefore005();
    void stackedNamesConvergeAndPaintHighest();
    void emptyLeftoverNickIsNotAParse();
    void prefixChangesWalkLettersAndApplyIdempotently();
    void laterPrefixRemapsGlyphsWithoutRewritingSets();
    void translator353UsesParseNamesToken();
};

void CaseMappingTest::normalizesAdvertisedMappings()
{
    const IrcCaseMapping rfc1459(IrcCaseMapping::Kind::Rfc1459);
    QVERIFY(rfc1459.equals("Nick[]\\", "nick{}|"));
    QVERIFY(rfc1459.equals("nick^", "NICK~"));

    const IrcCaseMapping strict(IrcCaseMapping::Kind::Rfc1459Strict);
    QVERIFY(strict.equals("Nick[]\\", "nick{}|"));
    QVERIFY(!strict.equals("nick^", "nick~"));

    const IrcCaseMapping ascii(IrcCaseMapping::Kind::Ascii);
    QVERIFY(ascii.equals("Nick", "nick"));
    QVERIFY(!ascii.equals("[]\\", "{}|"));

    QVERIFY(IrcCaseMapping::fromName("rfc1459-strict").has_value());
    QVERIFY(IrcCaseMapping::fromName("strict-rfc1459").has_value());
    QVERIFY(!IrcCaseMapping::fromName("unknown").has_value());
}

void CaseMappingTest::parsesServerFeatures()
{
    IrcServerFeatures features;
    features.applyTokens({
        "CASEMAPPING=ascii",
        "CHANTYPES=&",
        "PREFIX=(qaohv)~&@%+",
        "NICKLEN=31",
    });

    QCOMPARE(features.caseMapping().kind(), IrcCaseMapping::Kind::Ascii);
    QVERIFY(features.isChannel("&local"));
    QVERIFY(!features.isChannel("#channel"));
    QCOMPARE(features.nickLength(), std::optional<std::size_t>(31));
    QCOMPARE(QString::fromStdString(std::string(features.prefixModes())),
             QStringLiteral("qaohv"));
    QCOMPARE(QString::fromStdString(std::string(features.prefixSymbols())),
             QStringLiteral("~&@%+"));
    const auto op = features.parseNamesToken("@alice");
    QVERIFY(op);
    QCOMPARE(QString::fromStdString(op->nick), QStringLiteral("alice"));
    QCOMPARE(QString::fromStdString(features.memberLabel(op->ranks, "alice")),
             QStringLiteral("@alice"));
    const auto owner = features.parseNamesToken("~alice");
    QVERIFY(owner);
    QCOMPARE(QString::fromStdString(features.memberLabel(owner->ranks, "alice")),
             QStringLiteral("~alice"));
}

void CaseMappingTest::defaultPrefixStripsOwnerBefore005()
{
    const IrcServerFeatures features;
    QCOMPARE(QString::fromStdString(std::string(features.prefixModes())),
             QStringLiteral("qaohv"));
    QCOMPARE(QString::fromStdString(std::string(features.prefixSymbols())),
             QStringLiteral("~&@%+"));
    const auto parsed = features.parseNamesToken("~alice");
    QVERIFY(parsed);
    QCOMPARE(QString::fromStdString(parsed->nick), QStringLiteral("alice"));
    QCOMPARE(QString::fromStdString(features.memberLabel(parsed->ranks, "alice")),
             QStringLiteral("~alice"));

    IrcServerFeatures kept = features;
    kept.applyToken("PREFIX=bad");
    kept.applyToken("-PREFIX");
    kept.applyToken("PREFIX");
    QCOMPARE(QString::fromStdString(std::string(kept.prefixModes())),
             QStringLiteral("qaohv"));
}

void CaseMappingTest::stackedNamesConvergeAndPaintHighest()
{
    IrcServerFeatures features;
    const auto plusAt = features.parseNamesToken("+@alice");
    const auto atPlus = features.parseNamesToken("@+alice");
    QVERIFY(plusAt);
    QVERIFY(atPlus);
    QCOMPARE(plusAt->nick, atPlus->nick);
    QCOMPARE(plusAt->ranks, atPlus->ranks);
    QCOMPARE(QString::fromStdString(features.memberLabel(plusAt->ranks, "alice")),
             QStringLiteral("@alice"));

    features.applyToken("PREFIX=(ov)@+");
    QCOMPARE(QString::fromStdString(features.memberLabel(plusAt->ranks, "alice")),
             QStringLiteral("@alice"));
}

void CaseMappingTest::emptyLeftoverNickIsNotAParse()
{
    const IrcServerFeatures features;
    QVERIFY(!features.parseNamesToken("@"));
    QVERIFY(!features.parseNamesToken("@+"));
    QVERIFY(!features.parseNamesToken("+"));
    QVERIFY(!features.parseNamesToken(""));
}

void CaseMappingTest::prefixChangesWalkLettersAndApplyIdempotently()
{
    const IrcServerFeatures features;
    const auto changes = features.prefixChanges("+ov-o", {"alice", "bob", "alice"});
    QCOMPARE(changes.size(), std::size_t(3));
    QCOMPARE(QString::fromStdString(changes[0].nick()), QStringLiteral("alice"));
    QCOMPARE(QString::fromStdString(changes[1].nick()), QStringLiteral("bob"));
    QCOMPARE(QString::fromStdString(changes[2].nick()), QStringLiteral("alice"));

    const auto named = features.parseNamesToken("alice");
    QVERIFY(named);
    IrcPrefixSet ranks = named->ranks;
    ranks = features.apply(ranks, changes[0]);
    ranks = features.apply(ranks, changes[0]);
    QCOMPARE(QString::fromStdString(features.memberLabel(ranks, "alice")),
             QStringLiteral("@alice"));
    ranks = features.apply(ranks, features.prefixChanges("+v", {"alice"}).front());
    QCOMPARE(QString::fromStdString(features.memberLabel(ranks, "alice")),
             QStringLiteral("@alice"));
    ranks = features.apply(ranks, features.prefixChanges("-o", {"alice"}).front());
    QCOMPARE(QString::fromStdString(features.memberLabel(ranks, "alice")),
             QStringLiteral("+alice"));
    ranks = features.apply(ranks, features.prefixChanges("-v", {"alice"}).front());
    QCOMPARE(QString::fromStdString(features.memberLabel(ranks, "alice")),
             QStringLiteral("alice"));
}

void CaseMappingTest::laterPrefixRemapsGlyphsWithoutRewritingSets()
{
    IrcServerFeatures features;
    const auto stacked = features.parseNamesToken("~@alice");
    QVERIFY(stacked);
    QCOMPARE(QString::fromStdString(features.memberLabel(stacked->ranks, "alice")),
             QStringLiteral("~alice"));

    features.applyToken("PREFIX=(ov)@+");
    QCOMPARE(QString::fromStdString(features.memberLabel(stacked->ranks, "alice")),
             QStringLiteral("@alice"));

    features.applyToken("PREFIX=(ov)!+");
    QCOMPARE(QString::fromStdString(features.memberLabel(stacked->ranks, "alice")),
             QStringLiteral("!alice"));

    features.applyToken("PREFIX=broken");
    QCOMPARE(QString::fromStdString(std::string(features.prefixModes())),
             QStringLiteral("ov"));
    QCOMPARE(QString::fromStdString(features.memberLabel(stacked->ranks, "alice")),
             QStringLiteral("!alice"));
}

void CaseMappingTest::translator353UsesParseNamesToken()
{
    const IrcServerFeatures features;
    const auto message = IrcParser::parse(
        ":server 353 omairc = #chan :@+alice +@alice @ +bob");
    QVERIFY(message);
    const std::vector<IrcEvent> events = IrcEventTranslator::translate(
        QStringLiteral("network-a"), QStringLiteral("omairc"), features,
        *message.value);
    QCOMPARE(events.size(), std::size_t(1));
    const auto *names = std::get_if<IrcNamesEvent>(&events.front());
    QVERIFY(names);
    QCOMPARE(names->names.size(), std::size_t(3));
    QCOMPARE(names->names[0].nick, QStringLiteral("alice"));
    QCOMPARE(names->names[1].nick, QStringLiteral("alice"));
    QCOMPARE(names->names[0].ranks, names->names[1].ranks);
    QCOMPARE(names->names[2].nick, QStringLiteral("bob"));
    QVERIFY(names->names[2].ranks == features.parseNamesToken("+bob")->ranks);
    QCOMPARE(QString::fromStdString(features.memberLabel(names->names[0].ranks, "alice")),
             QStringLiteral("@alice"));
}

int runCaseMappingTests(int argc, char **argv)
{
    CaseMappingTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_casemapping.moc"
