#include <QTest>

#include "irccapabilitynegotiation.h"

namespace
{
QStringList tokens(const QString& line)
{
    return line.split(QLatin1Char(' '), Qt::SkipEmptyParts);
}
}

class CapabilityTest : public QObject
{
    Q_OBJECT

private slots:
    void unwantedAdvertisementProducesNoRequest();
    void saslKeepsItsOwnLine();
    void saslNeedsCredentialsAndPlain();
    void memberMetadataNeedsBatch();
    void acknowledgeAndRejectSettleIndependently();
    void deletionWithdrawsAnEnabledCapability();
    void timeoutAbandonsOutstandingRequests();
    void requestIsIdempotentUntilSomethingNewIsAdvertised();
};

void CapabilityTest::unwantedAdvertisementProducesNoRequest()
{
    IrcCapabilityNegotiation negotiation(false);
    negotiation.advertise(tokens(QStringLiteral("multi-prefix echo-message")));

    const IrcCapabilityNegotiation::Request request = negotiation.takeRequest();
    QVERIFY(request.lines.isEmpty());
    QVERIFY(!request.requestsSasl);
    QVERIFY(negotiation.settled());
    QVERIFY(negotiation.enabled().isEmpty());
}

void CapabilityTest::saslKeepsItsOwnLine()
{
    IrcCapabilityNegotiation negotiation(true);
    negotiation.advertise(tokens(
        QStringLiteral("sasl=PLAIN,EXTERNAL away-notify batch draft/metadata-2 multi-prefix")));

    const IrcCapabilityNegotiation::Request request = negotiation.takeRequest();
    QCOMPARE(request.lines,
             QStringList({QStringLiteral("sasl"),
                          QStringLiteral("away-notify batch draft/metadata-2")}));
    QVERIFY(request.requestsSasl);
    QVERIFY(!negotiation.settled());
}

void CapabilityTest::saslNeedsCredentialsAndPlain()
{
    IrcCapabilityNegotiation withoutCredentials(false);
    withoutCredentials.advertise(tokens(QStringLiteral("sasl=PLAIN")));
    QVERIFY(withoutCredentials.takeRequest().lines.isEmpty());

    IrcCapabilityNegotiation externalOnly(true);
    externalOnly.advertise(tokens(QStringLiteral("sasl=EXTERNAL")));
    QVERIFY(externalOnly.takeRequest().lines.isEmpty());

    IrcCapabilityNegotiation bare(true);
    bare.advertise(tokens(QStringLiteral("sasl")));
    QCOMPARE(bare.takeRequest().lines, QStringList{QStringLiteral("sasl")});
}

void CapabilityTest::memberMetadataNeedsBatch()
{
    IrcCapabilityNegotiation negotiation(false);
    negotiation.advertise(tokens(QStringLiteral("draft/metadata-2 away-notify")));
    QCOMPARE(negotiation.takeRequest().lines,
             QStringList{QStringLiteral("away-notify")});

    negotiation.advertise(tokens(QStringLiteral("batch")));
    QCOMPARE(negotiation.takeRequest().lines,
             QStringList{QStringLiteral("batch draft/metadata-2")});
}

void CapabilityTest::acknowledgeAndRejectSettleIndependently()
{
    IrcCapabilityNegotiation negotiation(true);
    negotiation.advertise(tokens(
        QStringLiteral("sasl=PLAIN away-notify batch draft/metadata-2")));
    negotiation.takeRequest();

    const IrcCapabilitySet refused =
        negotiation.reject(tokens(QStringLiteral("away-notify batch draft/metadata-2")));
    QVERIFY(refused.contains(IrcCapability::AwayNotify));
    QVERIFY(refused.contains(IrcCapability::MemberMetadata));
    QVERIFY(!refused.contains(IrcCapability::Sasl));
    QVERIFY(negotiation.enabled().isEmpty());
    QVERIFY(!negotiation.settled());

    const IrcCapabilitySet granted = negotiation.acknowledge(tokens(QStringLiteral("sasl")));
    QVERIFY(granted.contains(IrcCapability::Sasl));
    QVERIFY(negotiation.settled());
    QVERIFY(negotiation.enabled().contains(IrcCapability::Sasl));
    QVERIFY(!negotiation.enabled().contains(IrcCapability::AwayNotify));
}

void CapabilityTest::deletionWithdrawsAnEnabledCapability()
{
    IrcCapabilityNegotiation negotiation(false);
    negotiation.advertise(tokens(QStringLiteral("away-notify batch draft/metadata-2")));
    negotiation.takeRequest();
    negotiation.acknowledge(tokens(QStringLiteral("away-notify batch draft/metadata-2")));
    QVERIFY(negotiation.enabled().contains(IrcCapability::AwayNotify));

    negotiation.withdraw(tokens(QStringLiteral("away-notify")));
    QVERIFY(!negotiation.enabled().contains(IrcCapability::AwayNotify));
    QVERIFY(negotiation.enabled().contains(IrcCapability::MemberMetadata));

    QVERIFY(negotiation.takeRequest().lines.isEmpty());
}

void CapabilityTest::timeoutAbandonsOutstandingRequests()
{
    IrcCapabilityNegotiation negotiation(false);
    negotiation.advertise(tokens(QStringLiteral("away-notify batch")));
    negotiation.takeRequest();
    QVERIFY(!negotiation.settled());

    const IrcCapabilitySet abandoned = negotiation.abandonOutstanding();
    QVERIFY(abandoned.contains(IrcCapability::AwayNotify));
    QVERIFY(abandoned.contains(IrcCapability::Batch));
    QVERIFY(negotiation.settled());
    QVERIFY(negotiation.enabled().isEmpty());
}

void CapabilityTest::requestIsIdempotentUntilSomethingNewIsAdvertised()
{
    IrcCapabilityNegotiation negotiation(false);
    negotiation.advertise(tokens(QStringLiteral("away-notify")));
    QCOMPARE(negotiation.takeRequest().lines,
             QStringList{QStringLiteral("away-notify")});
    QVERIFY(negotiation.takeRequest().lines.isEmpty());

    negotiation.acknowledge(tokens(QStringLiteral("away-notify")));
    QVERIFY(negotiation.takeRequest().lines.isEmpty());

    negotiation.reset(false);
    QVERIFY(negotiation.enabled().isEmpty());
    QVERIFY(negotiation.takeRequest().lines.isEmpty());
}

int runCapabilityTests(int argc, char **argv)
{
    CapabilityTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_capability.moc"
