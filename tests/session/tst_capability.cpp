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
    void quietCapsRequestWhenAdvertised();
    void saslKeepsItsOwnLine();
    void messageTagsKeepsItsOwnLine();
    void saslNeedsCredentialsAndPlain();
    void memberMetadataNeedsBatch();
    void acknowledgeAndRejectSettleIndependently();
    void deletionWithdrawsAnEnabledCapability();
    void timeoutAbandonsOutstandingRequests();
    void requestIsIdempotentUntilSomethingNewIsAdvertised();
    void dualChatHistoryOfferRequestsStableToken();
    void draftOnlyChatHistoryRequestsDraftToken();
    void chatHistoryWithoutBatchIsNotRequested();
    void deletingStableChatHistoryKeepsDraftAdvertised();
    void deletingUnusedDraftChatHistoryKeepsStableEnabled();
};

void CapabilityTest::unwantedAdvertisementProducesNoRequest()
{
    IrcCapabilityNegotiation negotiation(false);
    negotiation.advertise(tokens(QStringLiteral("account-notify invite-notify")));

    const IrcCapabilityNegotiation::Request request = negotiation.takeRequest();
    QVERIFY(request.lines.isEmpty());
    QVERIFY(!request.requestsSasl);
    QVERIFY(negotiation.settled());
    QVERIFY(negotiation.enabled().isEmpty());
}

void CapabilityTest::quietCapsRequestWhenAdvertised()
{
    IrcCapabilityNegotiation negotiation(false);
    negotiation.advertise(tokens(QStringLiteral(
        "multi-prefix chghost cap-notify echo-message account-notify")));

    const IrcCapabilityNegotiation::Request request = negotiation.takeRequest();
    QCOMPARE(request.lines,
             QStringList{QStringLiteral(
                 "multi-prefix chghost cap-notify echo-message")});
    QVERIFY(!request.requestsSasl);
    QVERIFY(!negotiation.settled());

    const IrcCapabilitySet granted = negotiation.acknowledge(tokens(
        QStringLiteral("multi-prefix chghost cap-notify echo-message")));
    QVERIFY(granted.contains(IrcCapability::MultiPrefix));
    QVERIFY(granted.contains(IrcCapability::Chghost));
    QVERIFY(granted.contains(IrcCapability::CapNotify));
    QVERIFY(granted.contains(IrcCapability::EchoMessage));
    QVERIFY(negotiation.settled());
}

void CapabilityTest::saslKeepsItsOwnLine()
{
    IrcCapabilityNegotiation negotiation(true);
    negotiation.advertise(tokens(
        QStringLiteral("sasl=PLAIN,EXTERNAL away-notify batch draft/metadata-2 multi-prefix")));

    const IrcCapabilityNegotiation::Request request = negotiation.takeRequest();
    QCOMPARE(request.lines,
             QStringList({QStringLiteral("sasl"),
                          QStringLiteral(
                              "away-notify batch draft/metadata-2 multi-prefix")}));
    QVERIFY(request.requestsSasl);
    QVERIFY(!negotiation.settled());
}

void CapabilityTest::messageTagsKeepsItsOwnLine()
{
    IrcCapabilityNegotiation negotiation(true);
    negotiation.advertise(tokens(
        QStringLiteral("sasl=PLAIN message-tags away-notify batch draft/metadata-2")));

    const IrcCapabilityNegotiation::Request request = negotiation.takeRequest();
    QCOMPARE(request.lines,
             QStringList({QStringLiteral("sasl"),
                          QStringLiteral("message-tags"),
                          QStringLiteral("away-notify batch draft/metadata-2")}));
    QVERIFY(request.requestsSasl);

    const IrcCapabilitySet refused =
        negotiation.reject(tokens(QStringLiteral("message-tags")));
    QVERIFY(refused.contains(IrcCapability::MessageTags));
    QVERIFY(!negotiation.enabled().contains(IrcCapability::MessageTags));
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

void CapabilityTest::dualChatHistoryOfferRequestsStableToken()
{
    IrcCapabilityNegotiation negotiation(false);
    negotiation.advertise(tokens(QStringLiteral(
        "batch chathistory draft/chathistory")));

    const IrcCapabilityNegotiation::Request request = negotiation.takeRequest();
    QCOMPARE(request.lines, QStringList{QStringLiteral("batch chathistory")});
    QVERIFY(!request.lines.join(QLatin1Char(' ')).contains(
        QLatin1String("draft/chathistory")));
}

void CapabilityTest::draftOnlyChatHistoryRequestsDraftToken()
{
    IrcCapabilityNegotiation negotiation(false);
    negotiation.advertise(tokens(QStringLiteral("batch draft/chathistory")));

    const IrcCapabilityNegotiation::Request request = negotiation.takeRequest();
    QCOMPARE(request.lines, QStringList{QStringLiteral("batch draft/chathistory")});
}

void CapabilityTest::chatHistoryWithoutBatchIsNotRequested()
{
    IrcCapabilityNegotiation negotiation(false);
    negotiation.advertise(tokens(QStringLiteral("chathistory draft/chathistory")));

    const IrcCapabilityNegotiation::Request request = negotiation.takeRequest();
    QVERIFY(request.lines.isEmpty());
}

void CapabilityTest::deletingStableChatHistoryKeepsDraftAdvertised()
{
    IrcCapabilityNegotiation negotiation(false);
    negotiation.advertise(tokens(QStringLiteral(
        "batch chathistory draft/chathistory")));
    negotiation.takeRequest();
    negotiation.acknowledge(tokens(QStringLiteral("batch chathistory")));
    QVERIFY(negotiation.enabled().contains(IrcCapability::ChatHistory));

    negotiation.withdraw(tokens(QStringLiteral("chathistory")));
    QCOMPARE(negotiation.takeRequest().lines,
             QStringList{QStringLiteral("draft/chathistory")});
}

void CapabilityTest::deletingUnusedDraftChatHistoryKeepsStableEnabled()
{
    IrcCapabilityNegotiation negotiation(false);
    negotiation.advertise(tokens(QStringLiteral(
        "batch chathistory draft/chathistory")));
    negotiation.takeRequest();
    negotiation.acknowledge(tokens(QStringLiteral("batch chathistory")));
    QVERIFY(negotiation.enabled().contains(IrcCapability::ChatHistory));

    negotiation.withdraw(tokens(QStringLiteral("draft/chathistory")));
    QVERIFY(negotiation.enabled().contains(IrcCapability::ChatHistory));
    QVERIFY(negotiation.takeRequest().lines.isEmpty());
}

int runCapabilityTests(int argc, char **argv)
{
    CapabilityTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_capability.moc"
