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
    void serverTimeKeepsItsOwnLine();
    void saslNeedsCredentialsAndPlain();
    void scramSha256IsPreferredWhenAdvertised();
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
    void nakOfStableChatHistoryRequestsDraftToken();
    void lateAcknowledgeAfterTimeoutIsIgnored();
    void lateRejectAfterTimeoutIsIgnored();
    void acknowledgingEitherChatHistorySpellingSettles();
    void acknowledgedRemovalDisablesTheCapability();
    void stsIsAdvertisedButNeverRequested();
    void labeledResponseKeepsItsOwnLine();
    void labeledResponseNeedsMessageTags();
    void accountCapsRequestWhenAdvertised();
    void accountTagNeedsMessageTags();
    void partialAccountAdvertisementRequestsOnlyPresentCaps();
    void accountCapAckEnablesMatchingBits();
};

void CapabilityTest::unwantedAdvertisementProducesNoRequest()
{
    IrcCapabilityNegotiation negotiation(false);
    negotiation.advertise(tokens(QStringLiteral("invite-notify")));

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
        "multi-prefix chghost cap-notify echo-message invite-notify")));

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

void CapabilityTest::serverTimeKeepsItsOwnLine()
{
    IrcCapabilityNegotiation negotiation(false);
    negotiation.advertise(tokens(QStringLiteral("server-time batch multi-prefix")));

    const IrcCapabilityNegotiation::Request request = negotiation.takeRequest();
    QCOMPARE(request.lines,
             QStringList({QStringLiteral("server-time"),
                          QStringLiteral("batch multi-prefix")}));

    const IrcCapabilitySet refused =
        negotiation.reject(tokens(QStringLiteral("server-time")));
    QVERIFY(refused.contains(IrcCapability::ServerTime));
    QVERIFY(!negotiation.enabled().contains(IrcCapability::ServerTime));
    QVERIFY(!negotiation.settled());

    const IrcCapabilitySet granted =
        negotiation.acknowledge(tokens(QStringLiteral("batch multi-prefix")));
    QVERIFY(granted.contains(IrcCapability::Batch));
    QVERIFY(granted.contains(IrcCapability::MultiPrefix));
    QVERIFY(!negotiation.enabled().contains(IrcCapability::ServerTime));
    QVERIFY(negotiation.settled());
}

void CapabilityTest::saslNeedsCredentialsAndPlain()
{
    IrcCapabilityNegotiation withoutCredentials(false);
    withoutCredentials.advertise(tokens(QStringLiteral("sasl=PLAIN")));
    const IrcCapabilityNegotiation::Request denied = withoutCredentials.takeRequest();
    QVERIFY(denied.lines.isEmpty());
    QVERIFY(!denied.requestsSasl);
    QVERIFY(denied.saslMechanism.isEmpty());

    IrcCapabilityNegotiation externalOnly(true);
    externalOnly.advertise(tokens(QStringLiteral("sasl=EXTERNAL")));
    const IrcCapabilityNegotiation::Request external = externalOnly.takeRequest();
    QVERIFY(external.lines.isEmpty());
    QVERIFY(!external.requestsSasl);
    QVERIFY(external.saslMechanism.isEmpty());

    IrcCapabilityNegotiation plain(true);
    plain.advertise(tokens(QStringLiteral("sasl=PLAIN")));
    const IrcCapabilityNegotiation::Request plainRequest = plain.takeRequest();
    QCOMPARE(plainRequest.lines, QStringList{QStringLiteral("sasl")});
    QVERIFY(plainRequest.requestsSasl);
    QCOMPARE(plainRequest.saslMechanism, QStringLiteral("PLAIN"));

    IrcCapabilityNegotiation bare(true);
    bare.advertise(tokens(QStringLiteral("sasl")));
    const IrcCapabilityNegotiation::Request bareRequest = bare.takeRequest();
    QCOMPARE(bareRequest.lines, QStringList{QStringLiteral("sasl")});
    QVERIFY(bareRequest.requestsSasl);
    QCOMPARE(bareRequest.saslMechanism, QStringLiteral("PLAIN"));
}

void CapabilityTest::scramSha256IsPreferredWhenAdvertised()
{
    IrcCapabilityNegotiation both(true);
    both.advertise(tokens(QStringLiteral("sasl=SCRAM-SHA-256,PLAIN")));
    const IrcCapabilityNegotiation::Request request = both.takeRequest();
    QCOMPARE(request.lines, QStringList{QStringLiteral("sasl")});
    QVERIFY(request.requestsSasl);
    QCOMPARE(request.saslMechanism, QStringLiteral("SCRAM-SHA-256"));
    const IrcCapabilityNegotiation::Request again = both.takeRequest();
    QVERIFY(again.lines.isEmpty());
    QVERIFY(!again.requestsSasl);
    QVERIFY(again.saslMechanism.isEmpty());

    IrcCapabilityNegotiation reversed(true);
    reversed.advertise(tokens(QStringLiteral("sasl=PLAIN,SCRAM-SHA-256")));
    QCOMPARE(reversed.takeRequest().saslMechanism, QStringLiteral("SCRAM-SHA-256"));

    IrcCapabilityNegotiation scramOnly(true);
    scramOnly.advertise(tokens(QStringLiteral("sasl=SCRAM-SHA-256")));
    const IrcCapabilityNegotiation::Request only = scramOnly.takeRequest();
    QCOMPARE(only.lines, QStringList{QStringLiteral("sasl")});
    QVERIFY(only.requestsSasl);
    QCOMPARE(only.saslMechanism, QStringLiteral("SCRAM-SHA-256"));

    IrcCapabilityNegotiation folded(true);
    folded.advertise(tokens(QStringLiteral("sasl=scram-sha-256")));
    QCOMPARE(folded.takeRequest().saslMechanism, QStringLiteral("SCRAM-SHA-256"));

    IrcCapabilityNegotiation withExternal(true);
    withExternal.advertise(tokens(QStringLiteral("sasl=EXTERNAL,SCRAM-SHA-256")));
    QCOMPARE(withExternal.takeRequest().saslMechanism,
             QStringLiteral("SCRAM-SHA-256"));

    IrcCapabilityNegotiation withoutCredentials(false);
    withoutCredentials.advertise(tokens(QStringLiteral("sasl=SCRAM-SHA-256,PLAIN")));
    const IrcCapabilityNegotiation::Request denied = withoutCredentials.takeRequest();
    QVERIFY(denied.lines.isEmpty());
    QVERIFY(!denied.requestsSasl);
    QVERIFY(denied.saslMechanism.isEmpty());
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

void CapabilityTest::nakOfStableChatHistoryRequestsDraftToken()
{
    IrcCapabilityNegotiation negotiation(false);
    negotiation.advertise(tokens(QStringLiteral(
        "batch chathistory draft/chathistory")));
    QCOMPARE(negotiation.takeRequest().lines,
             QStringList{QStringLiteral("batch chathistory")});
    negotiation.acknowledge(tokens(QStringLiteral("batch")));
    negotiation.reject(tokens(QStringLiteral("chathistory")));
    QCOMPARE(negotiation.takeRequest().lines,
             QStringList{QStringLiteral("draft/chathistory")});
}

void CapabilityTest::lateAcknowledgeAfterTimeoutIsIgnored()
{
    IrcCapabilityNegotiation negotiation(false);
    negotiation.advertise(tokens(QStringLiteral("away-notify")));
    negotiation.takeRequest();
    negotiation.abandonOutstanding();

    QVERIFY(negotiation.acknowledge(tokens(QStringLiteral("away-notify"))).isEmpty());
    QVERIFY(negotiation.enabled().isEmpty());
    QVERIFY(negotiation.settled());
}

void CapabilityTest::lateRejectAfterTimeoutIsIgnored()
{
    IrcCapabilityNegotiation negotiation(true);
    negotiation.advertise(tokens(QStringLiteral("sasl")));
    QVERIFY(negotiation.takeRequest().requestsSasl);
    negotiation.abandonOutstanding();

    QVERIFY(negotiation.reject(tokens(QStringLiteral("sasl"))).isEmpty());
}

void CapabilityTest::acknowledgingEitherChatHistorySpellingSettles()
{
    IrcCapabilityNegotiation negotiation(false);
    negotiation.advertise(tokens(QStringLiteral("batch draft/chathistory")));
    QCOMPARE(negotiation.takeRequest().lines,
             QStringList{QStringLiteral("batch draft/chathistory")});

    negotiation.acknowledge(tokens(QStringLiteral("batch draft/chathistory")));
    QVERIFY(negotiation.settled());
    QVERIFY(negotiation.enabled().contains(IrcCapability::ChatHistory));
    QVERIFY(negotiation.takeRequest().lines.isEmpty());
}

void CapabilityTest::acknowledgedRemovalDisablesTheCapability()
{
    IrcCapabilityNegotiation negotiation(false);
    negotiation.advertise(tokens(QStringLiteral("batch chathistory")));
    negotiation.takeRequest();
    negotiation.acknowledge(tokens(QStringLiteral("batch chathistory")));
    QVERIFY(negotiation.enabled().contains(IrcCapability::ChatHistory));

    negotiation.acknowledge(tokens(QStringLiteral("-chathistory")));
    QVERIFY(!negotiation.enabled().contains(IrcCapability::ChatHistory));
    QVERIFY(negotiation.enabled().contains(IrcCapability::Batch));
}

void CapabilityTest::stsIsAdvertisedButNeverRequested()
{
    IrcCapabilityNegotiation negotiation(false);
    negotiation.advertise(tokens(
        QStringLiteral("sts=port=6697,duration=60 multi-prefix cap-notify")));

    const IrcCapabilityNegotiation::Request request = negotiation.takeRequest();
    QCOMPARE(request.lines,
             QStringList{QStringLiteral("multi-prefix cap-notify")});
    QVERIFY(!negotiation.settled());
}

void CapabilityTest::labeledResponseKeepsItsOwnLine()
{
    IrcCapabilityNegotiation negotiation(false);
    negotiation.advertise(tokens(
        QStringLiteral("message-tags labeled-response away-notify")));

    const IrcCapabilityNegotiation::Request request = negotiation.takeRequest();
    QCOMPARE(request.lines,
             QStringList({QStringLiteral("message-tags"),
                          QStringLiteral("labeled-response"),
                          QStringLiteral("away-notify")}));
    QVERIFY(!negotiation.settled());

    const IrcCapabilitySet refused =
        negotiation.reject(tokens(QStringLiteral("labeled-response")));
    QVERIFY(refused.contains(IrcCapability::LabeledResponse));
    QVERIFY(!negotiation.enabled().contains(IrcCapability::LabeledResponse));
    QVERIFY(!negotiation.settled());

    const IrcCapabilitySet granted =
        negotiation.acknowledge(tokens(QStringLiteral("message-tags away-notify")));
    QVERIFY(granted.contains(IrcCapability::MessageTags));
    QVERIFY(granted.contains(IrcCapability::AwayNotify));
    QVERIFY(!negotiation.enabled().contains(IrcCapability::LabeledResponse));
    QVERIFY(negotiation.settled());
}

void CapabilityTest::accountCapsRequestWhenAdvertised()
{
    IrcCapabilityNegotiation negotiation(false);
    negotiation.advertise(tokens(QStringLiteral(
        "message-tags account-tag account-notify extended-join")));

    QCOMPARE(negotiation.takeRequest().lines,
             QStringList({QStringLiteral("message-tags"),
                          QStringLiteral(
                              "account-tag account-notify extended-join")}));
}

void CapabilityTest::accountTagNeedsMessageTags()
{
    IrcCapabilityNegotiation negotiation(false);
    negotiation.advertise(tokens(QStringLiteral(
        "account-tag account-notify extended-join")));

    QCOMPARE(negotiation.takeRequest().lines,
             QStringList{QStringLiteral("account-notify extended-join")});
}

void CapabilityTest::partialAccountAdvertisementRequestsOnlyPresentCaps()
{
    IrcCapabilityNegotiation onlyNotify(false);
    onlyNotify.advertise(tokens(QStringLiteral("account-notify")));
    QCOMPARE(onlyNotify.takeRequest().lines,
             QStringList{QStringLiteral("account-notify")});

    IrcCapabilityNegotiation tagAndJoin(false);
    tagAndJoin.advertise(tokens(QStringLiteral(
        "message-tags account-tag extended-join")));
    QCOMPARE(tagAndJoin.takeRequest().lines,
             QStringList({QStringLiteral("message-tags"),
                          QStringLiteral("account-tag extended-join")}));

    IrcCapabilityNegotiation none(false);
    none.advertise(tokens(QStringLiteral("invite-notify")));
    QVERIFY(none.takeRequest().lines.isEmpty());
    QVERIFY(none.settled());
}

void CapabilityTest::accountCapAckEnablesMatchingBits()
{
    IrcCapabilityNegotiation negotiation(false);
    negotiation.advertise(tokens(QStringLiteral(
        "message-tags account-tag account-notify extended-join")));
    negotiation.takeRequest();

    const IrcCapabilitySet granted = negotiation.acknowledge(tokens(QStringLiteral(
        "message-tags account-tag account-notify extended-join")));
    QVERIFY(granted.contains(IrcCapability::AccountTag));
    QVERIFY(granted.contains(IrcCapability::AccountNotify));
    QVERIFY(granted.contains(IrcCapability::ExtendedJoin));
    QVERIFY(negotiation.enabled().contains(IrcCapability::AccountTag));
    QVERIFY(negotiation.enabled().contains(IrcCapability::AccountNotify));
    QVERIFY(negotiation.enabled().contains(IrcCapability::ExtendedJoin));
    QVERIFY(negotiation.settled());
}

void CapabilityTest::labeledResponseNeedsMessageTags()
{
    IrcCapabilityNegotiation negotiation(false);
    negotiation.advertise(tokens(QStringLiteral("labeled-response away-notify")));
    QCOMPARE(negotiation.takeRequest().lines,
             QStringList{QStringLiteral("away-notify")});

    negotiation.advertise(tokens(QStringLiteral("message-tags")));
    QCOMPARE(negotiation.takeRequest().lines,
             QStringList({QStringLiteral("message-tags"),
                          QStringLiteral("labeled-response")}));
}

int runCapabilityTests(int argc, char **argv)
{
    CapabilityTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_capability.moc"
