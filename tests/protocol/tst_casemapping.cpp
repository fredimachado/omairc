#include <QTest>

#include "irccasemapping.h"
#include "ircserverfeatures.h"

class CaseMappingTest : public QObject
{
    Q_OBJECT

private slots:
    void normalizesAdvertisedMappings();
    void parsesServerFeatures();
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
    QCOMPARE(QString::fromStdString(features.statusForPrefix('@')),
             QStringLiteral("o"));
    QCOMPARE(QString::fromStdString(features.statusForPrefix('~')),
             QStringLiteral("q"));
}

int runCaseMappingTests(int argc, char **argv)
{
    CaseMappingTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_casemapping.moc"
