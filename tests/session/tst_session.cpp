#include <QCoreApplication>
#include <QEvent>
#include <QPointer>
#include <QSignalSpy>
#include <QTest>

#include <optional>
#include <string>
#include <string_view>

#include "fakeirctransport.h"
#include "irceventtranslator.h"
#include "ircjointarget.h"
#include "ircparser.h"
#include "ircserverfeatures.h"
#include "ircsession.h"
#include "ircsessionmanager.h"
#include "ircstatusentry.h"
#include "irctcp.h"

class FakeReconnectTimer : public IrcReconnectTimer
{
public:
    using IrcReconnectTimer::IrcReconnectTimer;

    void start(int delayMilliseconds) override
    {
        active = true;
        delays.append(delayMilliseconds);
    }

    void cancel() override
    {
        active = false;
        ++cancelCount;
    }

    void fire()
    {
        if (!active)
            return;
        active = false;
        emit fired();
    }

    QList<int> delays;
    bool active = false;
    int cancelCount = 0;
};

class FakeReachabilitySource : public IrcReachabilitySource
{
public:
    using IrcReachabilitySource::IrcReachabilitySource;

    void becomeReachable()
    {
        emit reachable();
    }
};

namespace
{
QByteArray ctcpVersionReply(const QByteArray &nick)
{
    return QByteArrayLiteral("NOTICE ") + nick
        + QByteArrayLiteral(" :\x01VERSION Omairc ")
        + OMAIRC_VERSION
        + QByteArrayLiteral("\x01\r\n");
}

QByteArray decodeAuthenticatePlain(const QByteArray &frame)
{
    const QByteArray encoded = frame.mid(
        qsizetype(sizeof("AUTHENTICATE ") - 1),
        frame.size() - qsizetype(sizeof("AUTHENTICATE ") - 1) - 2);
    return QByteArray::fromBase64(encoded);
}

IrcSessionConfig config(const QString &networkId = QStringLiteral("network-a"))
{
    IrcSessionConfig value;
    value.networkId = networkId;
    value.host = QStringLiteral("irc.example");
    value.port = 6697;
    value.tlsEnabled = true;
    value.nick = QStringLiteral("omairc");
    value.username = QStringLiteral("omairc");
    value.realname = QStringLiteral("Omairc User");
    value.autojoinChannels = {QStringLiteral("#omarchy"), QStringLiteral("&local")};
    value.reconnectBaseDelayMilliseconds = 250;
    value.reconnectMaximumDelayMilliseconds = 1000;
    return value;
}

struct FixtureOptions
{
    std::optional<int> stopAfterScheduledReconnects;
};

struct Fixture
{
    explicit Fixture(IrcSessionConfig sessionConfig = config(),
                     FixtureOptions options = {})
        : transport(new FakeIrcTransport)
        , timer(new FakeReconnectTimer)
        , capabilityTimer(new FakeReconnectTimer)
        , pingTimer(new FakeReconnectTimer)
        , reachability(new FakeReachabilitySource)
        , session(new IrcSession(sessionConfig, transport, timer, capabilityTimer,
                                 nullptr, pingTimer, reachability))
    {
        if (!options.stopAfterScheduledReconnects
            || *options.stopAfterScheduledReconnects <= 0)
            return;
        const int limit = *options.stopAfterScheduledReconnects;
        QObject::connect(session, &IrcSession::reconnectScheduled, session,
                         [this, limit] {
            if (++scheduledReconnects >= limit)
                session->stop();
        });
    }

    ~Fixture()
    {
        delete session;
    }

    void connectTls()
    {
        session->start();
        transport->completeConnect();
    }

    void registerWithWelcome()
    {
        connectTls();
        transport->injectBytes(
            QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                              ":server 001 omairc :Welcome\r\n"));
    }

    bool wrote(const QByteArray& frame) const
    {
        return transport->writtenFrames().contains(frame);
    }

    FakeIrcTransport *transport;
    FakeReconnectTimer *timer;
    FakeReconnectTimer *capabilityTimer;
    FakeReconnectTimer *pingTimer;
    FakeReachabilitySource *reachability;
    IrcSession *session;
    int scheduledReconnects = 0;
};

IrcMessage mustParse(std::string_view line)
{
    const IrcParseResult parsed = IrcParser::parse(line);
    if (!parsed)
        qFatal("failed to parse IRC line");
    return *parsed.value;
}
}

class SessionTest : public QObject
{
    Q_OBJECT

private slots:
    void registersAndAutojoins();
    void negotiatesPresenceCapabilities();
    void negotiatesMessageTagsOnOwnLine();
    void refusedPresenceCapabilitiesStayOffWithoutFailing();
    void refusedMessageTagsStayOffWithoutFailing();
    void unansweredPresenceRequestStillRegisters();
    void withdrawnCapabilityIsPublished();
    void negotiatesSaslPlain();
    void sendsPassWhenSaslIsUnavailable();
    void nickServOnlySaslPlainUsesNickServSecret();
    void nickServOnlyWithoutSaslIdentifiesBeforeJoin();
    void bothSecretsSaslSendsPassAndPlainFromNickServ();
    void bothSecretsWithoutSaslPassThenIdentifyBeforeJoin();
    void saslSuccessDoesNotIdentify();
    void saslFailureDoesNotFallThroughToIdentify();
    void plaintextIdentifyEmitsOneStatusWarning();
    void automaticIdentifyDoesNotAppearInStatusAsSecret();
    void registersWhenCapIsUnsupported();
    void tlsCertificateFailureIsExplicit();
    void answersPingImmediately();
    void silentSocketAfterWelcomeSendsClientPing();
    void unansweredClientPingReconnects();
    void matchingPongKeepsSessionRegistered();
    void answersServerPingAfterWelcome();
    void registrationRefusalFailsVisibly();
    void nickInUseBeforeWelcomeRetriesThenRegisters();
    void nickInUseFallbacksExhaustedFails();
    void nickInUseAfterWelcomeKeepsSession();
    void unavailableResourceAfterWelcomeKeepsSession();
    void unavailableResourceBeforeWelcomeFails();
    void connectionTimeoutSchedulesReconnect();
    void remoteCloseSchedulesReconnect();
    void reachabilityStartsReconnectWithoutWaiting();
    void reachabilityIgnoredUnlessReconnecting();
    void malformedInputSurfacesProtocolError();
    void overlongFrameLogsPreviewWithoutSecrets();
    void reconnectCanBeCancelled();
    void reconnectDelayIsBoundedExponential();
    void reconnectKeepsRetryingUntilStop();
    void quitStopsReconnectWait();
    void retryableNetworkErrorIsEmittedOnceUntilWelcome();
    void retryableErrorsStayDedupedAcrossKindsUntilWelcome();
    void quitRejectsInvalidReasonWhileRegistered();
    void authenticationFailureIsExplicit();
    void destructionWhileConnectingIsSafe();
    void managerStartsTwoLiveNetworks();
    void managerCreateStaysAddOnly();
    void managerDiscardUnregistersImmediately();
    void pingAndWelcomeProduceStatusEntries();
    void configuredPasswordNeverAppearsInStatusEntries();
    void keyedJoinIsRedactedInStatusEntries();
    void serviceIdentifyIsRedactedInStatusEntries();
    void channelTalkAboutServicesStaysReadable();
    void negotiatedChannelTypesClassifyDollarTargets();
    void serviceRepliesStayReadable();
    void selfEchoToServiceIsRedacted();
    void sendPrivmsgValidatesTarget();
    void setTopicIsSetOnly();
    void kickWritesOptionalReason();
    void setAwayEncodesOptionalReason();
    void whoisWritesDoubledNick();
    void whoisStatusLinesFormatKnownNumerics();
    void incomingNoticeStatusLinesWrapSpeaker();
    void incomingNoticeDoesNotTranslateToEvents();
    void incomingNickservPrivmsgDoesNotTranslateToEvents();
    void incomingStandardRepliesShowDescriptionOnStatus();
    void incomingStandardRepliesDoNotTranslateToEvents();
    void inboundFailDoesNotFailTheSession();
    void incomingActionTranslatesToActionEvent();
    void latin1PrivmsgBodyIsEAcuteAndNextLineTranslates();
    void incomingCtcpRequestsAreNotConversationEvents();
    void answersCtcpRequests();
    void rateLimitsCtcpVersionRepliesPerNick();
    void dropsOversizedCtcpPingPayload();
    void doesNotAnswerChannelCtcpRequests();
    void welcomeAssignsNickFrom001();
    void emptyWelcomeKeepsConfigNick();
    void selfNickUpdatesSessionNick();
    void batchOpenAndCloseDoNotEmitBatchLines();
    void nestedBatchesDoNotFailTheSession();
    void unknownBatchTypeAndCloseStayRegistered();
    void selfJoinRequestsChatHistoryOnceUntilPart();
    void selfJoinWithBarePrefixRequestsChatHistory();
    void selfJoinWithoutChatHistorySendsNothing();
    void selfJoinWithoutBatchDoesNotRequestChatHistory();
    void deletingStableChatHistoryRequestsDraftToken();
    void historyBatchSwallowsInnerPrivmsg();
    void draftHistoryBatchSwallowsInnerPrivmsg();
    void leftoverHistoryBatchAfterPartDoesNotEmit();
    void delayedHistoryBatchAfterPartDoesNotEmit();
    void unsolicitedHistoryFloodKeepsLiveTraffic();
    void overflowedUnknownBatchPassesInnerPrivmsg();
    void selfJoinWithoutTargetDoesNotRequestChatHistory();
    void rfc1459EqualPartClearsChatHistoryAsk();
    void rfc1459EqualSelfJoinRequestsChatHistory();
    void nakOfStableChatHistoryRequestsDraftToken();
    void leftoverTaggedHistoryLineAfterPartIsSwallowed();
    void leftoverNestedBatchLineAfterRootCloseIsSwallowed();
    void ignoredBatchLedgerRecoversAfterClose();
    void chatHistoryFailClearsPending();
    void chatHistoryMessageErrorClearsPending();
    void chatHistoryFailForOtherChannelKeepsPending();
    void delayedChatHistoryFailAfterRejoinKeepsPending();
    void chatHistoryFailDescriptionDoesNotClearOtherChannel();
    void deletingRequiredHistoryCapClearsPending();
    void deletingUnusedDraftChatHistoryKeepsPending();
    void unsolicitedHistoryBatchStillEmits();
    void historyBatchWithoutCapabilityIsIgnored();
    void solicitedHistoryBatchSurvivesOpenBatchFlood();
    void isupportChatHistoryLimitCapsTheRequest();
    void chatHistoryRequestFillsBothPlaceholdersAtOnce();
    void lateCapAckAfterTimeoutIsIgnored();
    void lateCapNakAfterTimeoutIsIgnored();
    void capNakBeforeCapListIsIgnored();
    void ctcpFromServerPrefixGetsNoReply();
    void ctcpToFoldedSelfNickIsAnswered();
};

void SessionTest::registersAndAutojoins()
{
    Fixture fixture;
    QSignalSpy registered(fixture.session, &IrcSession::registered);

    fixture.connectTls();
    QCOMPARE(fixture.session->state(), IrcSession::State::CapLs);
    QCOMPARE(fixture.transport->writtenFrames(),
             QByteArrayList{QByteArrayLiteral("CAP LS 302\r\n")});

    fixture.transport->injectBytes(
        QByteArrayLiteral(
            ":server CAP omairc LS :multi-prefix chghost cap-notify echo-message\r\n"));
    QCOMPARE(fixture.session->state(), IrcSession::State::Registering);
    QCOMPARE(fixture.transport->writtenFrames().mid(1),
             QByteArrayList({
                 QByteArrayLiteral(
                     "CAP REQ :multi-prefix chghost cap-notify echo-message\r\n"),
                 QByteArrayLiteral("NICK omairc\r\n"),
                 QByteArrayLiteral("USER omairc 8 * :Omairc User\r\n"),
             }));
    QVERIFY(fixture.session->capabilities().isEmpty());

    fixture.transport->injectBytes(
        QByteArrayLiteral(
            ":server CAP omairc ACK :multi-prefix chghost cap-notify echo-message\r\n"));
    QVERIFY(fixture.wrote(QByteArrayLiteral("CAP END\r\n")));
    QVERIFY(fixture.session->capabilities().contains(IrcCapability::MultiPrefix));
    QVERIFY(fixture.session->capabilities().contains(IrcCapability::Chghost));
    QVERIFY(fixture.session->capabilities().contains(IrcCapability::CapNotify));
    QVERIFY(fixture.session->capabilities().contains(IrcCapability::EchoMessage));

    fixture.transport->injectBytes(
        QByteArrayLiteral(":server 001 omairc :Welcome\r\n"));
    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);
    QCOMPARE(fixture.session->nick(), QStringLiteral("omairc"));
    QCOMPARE(registered.size(), 1);
    QCOMPARE(fixture.transport->writtenFrames().mid(5),
             QByteArrayList({
                 QByteArrayLiteral("JOIN #omarchy\r\n"),
                 QByteArrayLiteral("JOIN &local\r\n"),
             }));
}

void SessionTest::negotiatesPresenceCapabilities()
{
    IrcSessionConfig saslConfig = config();
    saslConfig.password = QStringLiteral("secret");
    Fixture fixture(saslConfig);
    QSignalSpy capabilities(fixture.session, &IrcSession::capabilitiesChanged);

    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS * :sasl=PLAIN away-notify\r\n"
                          ":server CAP omairc LS :batch draft/metadata-2 multi-prefix\r\n"));
    QCOMPARE(fixture.transport->writtenFrames(),
             QByteArrayList({
                 QByteArrayLiteral("CAP LS 302\r\n"),
                 QByteArrayLiteral("CAP REQ :sasl\r\n"),
                 QByteArrayLiteral(
                     "CAP REQ :away-notify batch draft/metadata-2 multi-prefix\r\n"),
                 QByteArrayLiteral("NICK omairc\r\n"),
                 QByteArrayLiteral("USER omairc 8 * :Omairc User\r\n"),
             }));

    fixture.transport->injectBytes(
        QByteArrayLiteral(
            ":server CAP omairc ACK :away-notify batch draft/metadata-2 multi-prefix\r\n"));
    QVERIFY(!fixture.wrote(QByteArrayLiteral("CAP END\r\n")));

    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc ACK :sasl\r\n"
                          "AUTHENTICATE +\r\n"
                          ":server 903 omairc :SASL successful\r\n"));
    QVERIFY(fixture.wrote(QByteArrayLiteral("CAP END\r\n")));

    fixture.transport->injectBytes(
        QByteArrayLiteral(":server 001 omairc :Welcome\r\n"));
    QVERIFY(fixture.wrote(QByteArrayLiteral("METADATA * SUB status\r\n")));

    fixture.transport->injectBytes(
        QByteArrayLiteral(":server 366 omairc #omarchy :End of /NAMES\r\n"));
    QVERIFY(fixture.wrote(QByteArrayLiteral("WHO #omarchy\r\n")));

    const IrcCapabilitySet enabled = fixture.session->capabilities();
    QVERIFY(enabled.contains(IrcCapability::AwayNotify));
    QVERIFY(enabled.contains(IrcCapability::Batch));
    QVERIFY(enabled.contains(IrcCapability::MemberMetadata));
    QVERIFY(!capabilities.isEmpty());
}

void SessionTest::negotiatesMessageTagsOnOwnLine()
{
    IrcSessionConfig saslConfig = config();
    saslConfig.password = QStringLiteral("secret");
    Fixture fixture(saslConfig);

    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :sasl=PLAIN message-tags "
                          "away-notify batch draft/metadata-2\r\n"));
    QCOMPARE(fixture.transport->writtenFrames(),
             QByteArrayList({
                 QByteArrayLiteral("CAP LS 302\r\n"),
                 QByteArrayLiteral("CAP REQ :sasl\r\n"),
                 QByteArrayLiteral("CAP REQ :message-tags\r\n"),
                 QByteArrayLiteral("CAP REQ :away-notify batch draft/metadata-2\r\n"),
                 QByteArrayLiteral("NICK omairc\r\n"),
                 QByteArrayLiteral("USER omairc 8 * :Omairc User\r\n"),
             }));
}

void SessionTest::refusedPresenceCapabilitiesStayOffWithoutFailing()
{
    Fixture fixture;
    QSignalSpy errors(fixture.session, &IrcSession::errorOccurred);

    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :away-notify batch draft/metadata-2\r\n"));
    QCOMPARE(fixture.transport->writtenFrames().mid(1),
             QByteArrayList({
                 QByteArrayLiteral("CAP REQ :away-notify batch draft/metadata-2\r\n"),
                 QByteArrayLiteral("NICK omairc\r\n"),
                 QByteArrayLiteral("USER omairc 8 * :Omairc User\r\n"),
             }));

    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc NAK :away-notify batch draft/metadata-2\r\n"));
    QVERIFY(fixture.wrote(QByteArrayLiteral("CAP END\r\n")));
    QCOMPARE(errors.size(), 0);
    QVERIFY(fixture.session->capabilities().isEmpty());

    fixture.transport->injectBytes(
        QByteArrayLiteral(":server 001 omairc :Welcome\r\n"
                          ":server 366 omairc #omarchy :End of /NAMES\r\n"));
    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);
    QVERIFY(!fixture.wrote(QByteArrayLiteral("METADATA * SUB status\r\n")));
    QVERIFY(!fixture.wrote(QByteArrayLiteral("WHO #omarchy\r\n")));
}

void SessionTest::refusedMessageTagsStayOffWithoutFailing()
{
    Fixture fixture;
    QSignalSpy errors(fixture.session, &IrcSession::errorOccurred);

    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :message-tags away-notify\r\n"));
    QCOMPARE(fixture.transport->writtenFrames().mid(1),
             QByteArrayList({
                 QByteArrayLiteral("CAP REQ :message-tags\r\n"),
                 QByteArrayLiteral("CAP REQ :away-notify\r\n"),
                 QByteArrayLiteral("NICK omairc\r\n"),
                 QByteArrayLiteral("USER omairc 8 * :Omairc User\r\n"),
             }));

    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc NAK :message-tags\r\n"));
    QVERIFY(!fixture.wrote(QByteArrayLiteral("CAP END\r\n")));
    QCOMPARE(errors.size(), 0);
    QVERIFY(!fixture.session->capabilities().contains(IrcCapability::MessageTags));

    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc ACK :away-notify\r\n"));
    QVERIFY(fixture.wrote(QByteArrayLiteral("CAP END\r\n")));

    fixture.transport->injectBytes(
        QByteArrayLiteral(":server 001 omairc :Welcome\r\n"));
    QVERIFY(fixture.session->capabilities().contains(IrcCapability::AwayNotify));
    QVERIFY(!fixture.session->capabilities().contains(IrcCapability::MessageTags));
}

void SessionTest::unansweredPresenceRequestStillRegisters()
{
    Fixture fixture;
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :away-notify\r\n"));
    QVERIFY(fixture.capabilityTimer->active);
    QVERIFY(!fixture.wrote(QByteArrayLiteral("CAP END\r\n")));

    fixture.capabilityTimer->fire();
    QVERIFY(fixture.wrote(QByteArrayLiteral("CAP END\r\n")));
    QVERIFY(fixture.session->capabilities().isEmpty());
}

void SessionTest::lateCapAckAfterTimeoutIsIgnored()
{
    Fixture fixture;
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :away-notify\r\n"));
    fixture.capabilityTimer->fire();
    QVERIFY(fixture.session->capabilities().isEmpty());

    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc ACK :away-notify\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    QVERIFY(!fixture.session->capabilities().contains(IrcCapability::AwayNotify));
}

void SessionTest::lateCapNakAfterTimeoutIsIgnored()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.password = QStringLiteral("s3cret");
    Fixture fixture(sessionConfig);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :sasl\r\n"));
    QSignalSpy failed(fixture.session, &IrcSession::errorOccurred);
    fixture.capabilityTimer->fire();

    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc NAK :sasl\r\n"));
    QVERIFY(failed.isEmpty());
}

void SessionTest::capNakBeforeCapListIsIgnored()
{
    Fixture fixture;
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc NAK :away-notify\r\n"));
    QVERIFY(!fixture.wrote(QByteArrayLiteral("CAP END\r\n")));

    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :away-notify\r\n"
                          ":server CAP omairc ACK :away-notify\r\n"));
    QVERIFY(fixture.wrote(QByteArrayLiteral("CAP END\r\n")));
    QVERIFY(fixture.session->capabilities().contains(IrcCapability::AwayNotify));
}

void SessionTest::ctcpFromServerPrefixGetsNoReply()
{
    Fixture fixture;
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server 001 omairc :Welcome\r\n"));

    fixture.transport->injectBytes(
        QByteArrayLiteral(":services PRIVMSG omairc :\x01VERSION\x01\r\n"));
    QVERIFY(!fixture.wrote(ctcpVersionReply(QByteArrayLiteral("services"))));
}

void SessionTest::ctcpToFoldedSelfNickIsAnswered()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.nick = QStringLiteral("omairc[m]");
    Fixture fixture(sessionConfig);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server 001 omairc[m] :Welcome\r\n"
                          ":server 005 omairc[m] CASEMAPPING=rfc1459 :are supported\r\n"));

    // RFC 1459 folds the bracket characters onto the brace family, so a server
    // may address us by a spelling that plain case folding does not match.
    fixture.transport->injectBytes(
        QByteArrayLiteral(":alice!u@h PRIVMSG omairc{m} :\x01PING one\x01\r\n"));
    QVERIFY(fixture.wrote(QByteArrayLiteral("NOTICE alice :\x01PING one\x01\r\n")));
}

void SessionTest::withdrawnCapabilityIsPublished()
{
    Fixture fixture;
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :away-notify\r\n"
                          ":server CAP omairc ACK :away-notify\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":server 366 omairc #omarchy :End of /NAMES\r\n"));
    QVERIFY(fixture.wrote(QByteArrayLiteral("WHO #omarchy\r\n")));
    QVERIFY(fixture.session->capabilities().contains(IrcCapability::AwayNotify));

    QSignalSpy capabilities(fixture.session, &IrcSession::capabilitiesChanged);
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc DEL :away-notify\r\n"
                          ":server 366 omairc #desktop :End of /NAMES\r\n"));
    QCOMPARE(capabilities.size(), 1);
    QVERIFY(!fixture.session->capabilities().contains(IrcCapability::AwayNotify));
    QVERIFY(!fixture.wrote(QByteArrayLiteral("WHO #desktop\r\n")));
}

void SessionTest::negotiatesSaslPlain()
{
    IrcSessionConfig saslConfig = config();
    saslConfig.password = QStringLiteral("secret");
    Fixture fixture(saslConfig);

    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :sasl=PLAIN,EXTERNAL\r\n"));
    QCOMPARE(fixture.transport->writtenFrames(),
             QByteArrayList({
                 QByteArrayLiteral("CAP LS 302\r\n"),
                 QByteArrayLiteral("CAP REQ :sasl\r\n"),
                 QByteArrayLiteral("NICK omairc\r\n"),
                 QByteArrayLiteral("USER omairc 8 * :Omairc User\r\n"),
             }));

    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc ACK :sasl\r\n"));
    QCOMPARE(fixture.session->state(), IrcSession::State::Sasl);
    QCOMPARE(fixture.transport->writtenFrames().last(),
             QByteArrayLiteral("AUTHENTICATE PLAIN\r\n"));

    fixture.transport->injectBytes(QByteArrayLiteral("AUTHENTICATE +\r\n"));
    const QByteArray authenticate = fixture.transport->writtenFrames().last();
    QVERIFY(authenticate.startsWith("AUTHENTICATE "));
    const QByteArray encoded = authenticate.mid(
        qsizetype(sizeof("AUTHENTICATE ") - 1),
        authenticate.size() - qsizetype(sizeof("AUTHENTICATE ") - 1) - 2);
    QCOMPARE(QByteArray::fromBase64(encoded),
             QByteArray("omairc\0omairc\0secret", 20));

    fixture.transport->injectBytes(
        QByteArrayLiteral(":server 903 omairc :SASL successful\r\n"));
    QCOMPARE(fixture.transport->writtenFrames().last(),
             QByteArrayLiteral("CAP END\r\n"));
}

void SessionTest::sendsPassWhenSaslIsUnavailable()
{
    IrcSessionConfig passwordConfig = config();
    passwordConfig.password = QStringLiteral("secret");
    Fixture fixture(passwordConfig);

    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :account-notify\r\n"));
    QCOMPARE(fixture.transport->writtenFrames().mid(1),
             QByteArrayList({
                 QByteArrayLiteral("PASS secret\r\n"),
                 QByteArrayLiteral("NICK omairc\r\n"),
                 QByteArrayLiteral("USER omairc 8 * :Omairc User\r\n"),
                 QByteArrayLiteral("CAP END\r\n"),
             }));
}

void SessionTest::nickServOnlySaslPlainUsesNickServSecret()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.nickServPassword = QStringLiteral("nick-secret");
    Fixture fixture(sessionConfig);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :sasl=PLAIN\r\n"));
    QCOMPARE(fixture.transport->writtenFrames(),
             QByteArrayList({
                 QByteArrayLiteral("CAP LS 302\r\n"),
                 QByteArrayLiteral("CAP REQ :sasl\r\n"),
                 QByteArrayLiteral("NICK omairc\r\n"),
                 QByteArrayLiteral("USER omairc 8 * :Omairc User\r\n"),
             }));
    QVERIFY(!fixture.wrote(QByteArrayLiteral("PASS nick-secret\r\n")));

    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc ACK :sasl\r\n"
                          "AUTHENTICATE +\r\n"));
    QCOMPARE(decodeAuthenticatePlain(fixture.transport->writtenFrames().last()),
             QByteArray("omairc\0omairc\0nick-secret", 25));

    fixture.transport->injectBytes(
        QByteArrayLiteral(":server 903 omairc :SASL successful\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    QVERIFY(!fixture.wrote(QByteArrayLiteral(
        "PRIVMSG NickServ :IDENTIFY nick-secret\r\n")));
    QVERIFY(fixture.wrote(QByteArrayLiteral("JOIN #omarchy\r\n")));
}

void SessionTest::nickServOnlyWithoutSaslIdentifiesBeforeJoin()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.nickServPassword = QStringLiteral("nick-secret");
    Fixture fixture(sessionConfig);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :account-notify\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    const QByteArrayList frames = fixture.transport->writtenFrames();
    QVERIFY(!fixture.wrote(QByteArrayLiteral("PASS nick-secret\r\n")));
    const int identify = frames.indexOf(
        QByteArrayLiteral("PRIVMSG NickServ :IDENTIFY nick-secret\r\n"));
    const int join = frames.indexOf(QByteArrayLiteral("JOIN #omarchy\r\n"));
    QVERIFY(identify >= 0);
    QVERIFY(join >= 0);
    QVERIFY(identify < join);
}

void SessionTest::bothSecretsSaslSendsPassAndPlainFromNickServ()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.password = QStringLiteral("server-secret");
    sessionConfig.nickServPassword = QStringLiteral("nick-secret");
    Fixture fixture(sessionConfig);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :sasl=PLAIN\r\n"));
    QCOMPARE(fixture.transport->writtenFrames(),
             QByteArrayList({
                 QByteArrayLiteral("CAP LS 302\r\n"),
                 QByteArrayLiteral("CAP REQ :sasl\r\n"),
                 QByteArrayLiteral("PASS server-secret\r\n"),
                 QByteArrayLiteral("NICK omairc\r\n"),
                 QByteArrayLiteral("USER omairc 8 * :Omairc User\r\n"),
             }));

    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc ACK :sasl\r\n"
                          "AUTHENTICATE +\r\n"));
    const QByteArray plain =
        decodeAuthenticatePlain(fixture.transport->writtenFrames().last());
    QCOMPARE(plain, QByteArray("omairc\0omairc\0nick-secret", 25));
    QVERIFY(!plain.contains("server-secret"));
}

void SessionTest::bothSecretsWithoutSaslPassThenIdentifyBeforeJoin()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.password = QStringLiteral("server-secret");
    sessionConfig.nickServPassword = QStringLiteral("nick-secret");
    Fixture fixture(sessionConfig);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :account-notify\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    const QByteArrayList frames = fixture.transport->writtenFrames();
    QVERIFY(fixture.wrote(QByteArrayLiteral("PASS server-secret\r\n")));
    const int identify = frames.indexOf(
        QByteArrayLiteral("PRIVMSG NickServ :IDENTIFY nick-secret\r\n"));
    const int join = frames.indexOf(QByteArrayLiteral("JOIN #omarchy\r\n"));
    QVERIFY(identify >= 0);
    QVERIFY(join >= 0);
    QVERIFY(identify < join);
}

void SessionTest::saslSuccessDoesNotIdentify()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.password = QStringLiteral("server-secret");
    sessionConfig.nickServPassword = QStringLiteral("nick-secret");
    Fixture fixture(sessionConfig);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :sasl=PLAIN\r\n"
                          ":server CAP omairc ACK :sasl\r\n"
                          "AUTHENTICATE +\r\n"
                          ":server 903 omairc :SASL successful\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    QVERIFY(!fixture.wrote(QByteArrayLiteral(
        "PRIVMSG NickServ :IDENTIFY nick-secret\r\n")));
    QVERIFY(fixture.wrote(QByteArrayLiteral("JOIN #omarchy\r\n")));
}

void SessionTest::saslFailureDoesNotFallThroughToIdentify()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.nickServPassword = QStringLiteral("nick-secret");
    Fixture fixture(sessionConfig);
    QSignalSpy failed(fixture.session, &IrcSession::errorOccurred);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :sasl=PLAIN\r\n"
                          ":server CAP omairc ACK :sasl\r\n"
                          "AUTHENTICATE +\r\n"
                          ":server 904 omairc :SASL failed\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    QCOMPARE(failed.size(), 1);
    QCOMPARE(fixture.session->state(), IrcSession::State::Failed);
    QVERIFY(!fixture.wrote(QByteArrayLiteral(
        "PRIVMSG NickServ :IDENTIFY nick-secret\r\n")));
    QVERIFY(!fixture.wrote(QByteArrayLiteral("JOIN #omarchy\r\n")));
}

void SessionTest::tlsCertificateFailureIsExplicit()
{
    Fixture fixture;
    QSignalSpy errors(fixture.session, &IrcSession::errorOccurred);
    fixture.session->start();
    fixture.transport->failTls(
        QStringLiteral("TLS certificate error: The certificate has expired (CN=irc.example)"));

    QCOMPARE(fixture.session->state(), IrcSession::State::Reconnecting);
    QCOMPARE(qvariant_cast<IrcSession::ErrorKind>(errors.at(0).at(1)),
             IrcSession::ErrorKind::Tls);
}

void SessionTest::registersWhenCapIsUnsupported()
{
    Fixture fixture;
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server 421 omairc CAP :Unknown command\r\n"));
    QCOMPARE(fixture.transport->writtenFrames().mid(1),
             QByteArrayList({
                 QByteArrayLiteral("NICK omairc\r\n"),
                 QByteArrayLiteral("USER omairc 8 * :Omairc User\r\n"),
             }));

    fixture.transport->injectBytes(
        QByteArrayLiteral(":server 001 omairc :Welcome\r\n"));
    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);
}

void SessionTest::answersPingImmediately()
{
    Fixture fixture;
    fixture.connectTls();
    fixture.transport->injectBytes(QByteArrayLiteral("PING :server-token\r\n"));
    QCOMPARE(fixture.transport->writtenFrames().last(),
             QByteArrayLiteral("PONG :server-token\r\n"));
    QVERIFY(!fixture.pingTimer->active);
}

void SessionTest::silentSocketAfterWelcomeSendsClientPing()
{
    Fixture fixture;
    fixture.registerWithWelcome();
    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);
    QVERIFY(fixture.pingTimer->active);
    QCOMPARE(fixture.pingTimer->delays, QList<int>{60000});

    fixture.transport->injectBytes(
        QByteArrayLiteral(":bob!u@h PRIVMSG #omarchy :hi\r\n"));
    QCOMPARE(fixture.pingTimer->delays, QList<int>({60000, 60000}));

    const int writtenBeforeProbe = fixture.transport->writtenFrames().size();
    fixture.pingTimer->fire();

    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);
    QCOMPARE(fixture.transport->writtenFrames().mid(writtenBeforeProbe),
             QByteArrayList({QByteArrayLiteral("PING :omairc-watchdog\r\n")}));
    QVERIFY(fixture.pingTimer->active);
}

void SessionTest::unansweredClientPingReconnects()
{
    Fixture fixture;
    QSignalSpy errors(fixture.session, &IrcSession::errorOccurred);
    QSignalSpy scheduled(fixture.session, &IrcSession::reconnectScheduled);
    fixture.registerWithWelcome();
    fixture.pingTimer->fire();
    QCOMPARE(fixture.transport->writtenFrames().last(),
             QByteArrayLiteral("PING :omairc-watchdog\r\n"));

    fixture.pingTimer->fire();

    QCOMPARE(qvariant_cast<IrcSession::ErrorKind>(errors.last().at(1)),
             IrcSession::ErrorKind::Network);
    QCOMPARE(errors.last().at(2).toString(), QStringLiteral("Ping timeout"));
    QCOMPARE(fixture.session->state(), IrcSession::State::Reconnecting);
    QCOMPARE(scheduled.size(), 1);
    QVERIFY(!fixture.pingTimer->active);
}

void SessionTest::matchingPongKeepsSessionRegistered()
{
    Fixture fixture;
    QSignalSpy errors(fixture.session, &IrcSession::errorOccurred);
    fixture.registerWithWelcome();
    fixture.pingTimer->fire();

    fixture.transport->injectBytes(
        QByteArrayLiteral(":server PONG irc.example :omairc-watchdog\r\n"));
    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);
    QCOMPARE(errors.size(), 0);
    QVERIFY(fixture.pingTimer->active);

    const int writtenBeforeSecondProbe = fixture.transport->writtenFrames().size();
    fixture.pingTimer->fire();
    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);
    QCOMPARE(fixture.transport->writtenFrames().mid(writtenBeforeSecondProbe),
             QByteArrayList({QByteArrayLiteral("PING :omairc-watchdog\r\n")}));
    QCOMPARE(errors.size(), 0);
}

void SessionTest::answersServerPingAfterWelcome()
{
    Fixture fixture;
    fixture.registerWithWelcome();
    fixture.transport->injectBytes(QByteArrayLiteral("PING :server-token\r\n"));
    QCOMPARE(fixture.transport->writtenFrames().last(),
             QByteArrayLiteral("PONG :server-token\r\n"));
    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);
    QVERIFY(!fixture.wrote(QByteArrayLiteral("PING :omairc-watchdog\r\n")));
}

void SessionTest::registrationRefusalFailsVisibly()
{
    Fixture fixture;
    QSignalSpy errors(fixture.session, &IrcSession::errorOccurred);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server 432 * omairc :Erroneous nickname\r\n"));

    QCOMPARE(fixture.session->state(), IrcSession::State::Failed);
    QCOMPARE(errors.size(), 1);
    QCOMPARE(qvariant_cast<IrcSession::ErrorKind>(errors.at(0).at(1)),
             IrcSession::ErrorKind::Registration);
}

void SessionTest::nickInUseBeforeWelcomeRetriesThenRegisters()
{
    Fixture fixture;
    QSignalSpy errors(fixture.session, &IrcSession::errorOccurred);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"));
    QCOMPARE(fixture.session->state(), IrcSession::State::Registering);
    QVERIFY(fixture.wrote(QByteArrayLiteral("NICK omairc\r\n")));

    fixture.transport->injectBytes(
        QByteArrayLiteral(":server 433 * omairc :Nickname in use\r\n"));
    QCOMPARE(fixture.session->state(), IrcSession::State::Registering);
    QCOMPARE(errors.size(), 0);
    QCOMPARE(fixture.transport->writtenFrames().last(),
             QByteArrayLiteral("NICK omairc_\r\n"));
    QCOMPARE(fixture.session->nick(), QStringLiteral("omairc_"));

    fixture.transport->injectBytes(
        QByteArrayLiteral(":server 433 * omairc_ :Nickname in use\r\n"));
    QCOMPARE(fixture.session->state(), IrcSession::State::Registering);
    QCOMPARE(errors.size(), 0);
    QCOMPARE(fixture.transport->writtenFrames().last(),
             QByteArrayLiteral("NICK omairc2\r\n"));
    QCOMPARE(fixture.session->nick(), QStringLiteral("omairc2"));

    fixture.transport->injectBytes(
        QByteArrayLiteral(":server 001 omairc2 :Welcome\r\n"));
    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);
    QCOMPARE(fixture.session->nick(), QStringLiteral("omairc2"));
    QCOMPARE(errors.size(), 0);
}

void SessionTest::nickInUseFallbacksExhaustedFails()
{
    Fixture fixture;
    QSignalSpy errors(fixture.session, &IrcSession::errorOccurred);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"));

    fixture.transport->injectBytes(
        QByteArrayLiteral(":server 433 * omairc :Nickname in use\r\n"
                          ":server 433 * omairc_ :Nickname in use\r\n"
                          ":server 433 * omairc2 :Nickname in use\r\n"));

    QCOMPARE(fixture.session->state(), IrcSession::State::Failed);
    QCOMPARE(errors.size(), 1);
    QCOMPARE(qvariant_cast<IrcSession::ErrorKind>(errors.at(0).at(1)),
             IrcSession::ErrorKind::Registration);
    QCOMPARE(fixture.transport->writtenFrames().last(),
             QByteArrayLiteral("NICK omairc2\r\n"));
}

void SessionTest::nickInUseAfterWelcomeKeepsSession()
{
    Fixture fixture;
    QSignalSpy errors(fixture.session, &IrcSession::errorOccurred);
    int received = 0;
    QString lastCommand;
    QObject::connect(fixture.session, &IrcSession::messageReceived, fixture.session,
                     [&](const QString&, const IrcMessage& message) {
        ++received;
        lastCommand = QString::fromStdString(message.command);
    });
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);
    QCOMPARE(errors.size(), 0);

    fixture.transport->injectBytes(
        QByteArrayLiteral(":server 433 omairc othernick :Nickname is already in use\r\n"));

    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);
    QCOMPARE(fixture.transport->connectionState(),
             IrcTransport::ConnectionState::Encrypted);
    QCOMPARE(errors.size(), 0);
    QCOMPARE(received, 1);
    QCOMPARE(lastCommand, QStringLiteral("433"));
}

void SessionTest::unavailableResourceAfterWelcomeKeepsSession()
{
    Fixture fixture;
    QSignalSpy errors(fixture.session, &IrcSession::errorOccurred);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);
    QCOMPARE(errors.size(), 0);

    fixture.transport->injectBytes(
        QByteArrayLiteral(":server 437 omairc othernick :Nick/channel is temporarily unavailable\r\n"));

    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);
    QCOMPARE(fixture.transport->connectionState(),
             IrcTransport::ConnectionState::Encrypted);
    QCOMPARE(errors.size(), 0);
}

void SessionTest::unavailableResourceBeforeWelcomeFails()
{
    Fixture fixture;
    QSignalSpy errors(fixture.session, &IrcSession::errorOccurred);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server 437 * omairc :Nick/channel is temporarily unavailable\r\n"));

    QCOMPARE(fixture.session->state(), IrcSession::State::Failed);
    QCOMPARE(errors.size(), 1);
    QCOMPARE(qvariant_cast<IrcSession::ErrorKind>(errors.at(0).at(1)),
             IrcSession::ErrorKind::Registration);
}

void SessionTest::connectionTimeoutSchedulesReconnect()
{
    Fixture fixture;
    QSignalSpy errors(fixture.session, &IrcSession::errorOccurred);
    fixture.session->start();
    fixture.transport->timeoutConnect();

    QCOMPARE(fixture.session->state(), IrcSession::State::Reconnecting);
    QCOMPARE(fixture.timer->delays, QList<int>{250});
    QCOMPARE(qvariant_cast<IrcSession::ErrorKind>(errors.at(0).at(1)),
             IrcSession::ErrorKind::Network);
}

void SessionTest::remoteCloseSchedulesReconnect()
{
    Fixture fixture;
    QSignalSpy scheduled(fixture.session, &IrcSession::reconnectScheduled);
    fixture.connectTls();
    fixture.transport->remoteClose();

    QCOMPARE(fixture.session->state(), IrcSession::State::Reconnecting);
    QCOMPARE(scheduled.size(), 1);
    QCOMPARE(scheduled.at(0).at(1).toInt(), 250);
}

void SessionTest::reachabilityStartsReconnectWithoutWaiting()
{
    Fixture fixture;
    QSignalSpy scheduled(fixture.session, &IrcSession::reconnectScheduled);
    fixture.connectTls();
    fixture.transport->remoteClose();

    QCOMPARE(fixture.session->state(), IrcSession::State::Reconnecting);
    QCOMPARE(fixture.timer->delays, QList<int>{250});
    QVERIFY(fixture.timer->active);
    QCOMPARE(fixture.session->reconnectAttempt(), 1);

    fixture.reachability->becomeReachable();

    QCOMPARE(fixture.session->state(), IrcSession::State::Connecting);
    QVERIFY(!fixture.timer->active);
    QCOMPARE(fixture.transport->connectionState(),
             IrcTransport::ConnectionState::Connecting);
    QCOMPARE(scheduled.size(), 1);
    QCOMPARE(fixture.session->reconnectAttempt(), 1);
    QCOMPARE(fixture.timer->delays, QList<int>{250});
}

void SessionTest::reachabilityIgnoredUnlessReconnecting()
{
    Fixture fixture;
    fixture.registerWithWelcome();
    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);

    fixture.reachability->becomeReachable();

    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);
    QCOMPARE(fixture.transport->connectionState(),
             IrcTransport::ConnectionState::Encrypted);

    fixture.transport->remoteClose();
    fixture.session->stop();
    fixture.reachability->becomeReachable();

    QCOMPARE(fixture.session->state(), IrcSession::State::Idle);
    QCOMPARE(fixture.transport->connectionState(),
             IrcTransport::ConnectionState::Disconnected);
}

void SessionTest::malformedInputSurfacesProtocolError()
{
    Fixture fixture;
    QSignalSpy errors(fixture.session, &IrcSession::errorOccurred);
    fixture.connectTls();
    fixture.transport->injectBytes(QByteArray("BAD\0FRAME\r\n", 11));

    QCOMPARE(errors.size(), 1);
    QCOMPARE(qvariant_cast<IrcSession::ErrorKind>(errors.at(0).at(1)),
             IrcSession::ErrorKind::Protocol);
    const QString message = errors.at(0).at(2).toString();
    QVERIFY(message.contains(QStringLiteral("invalid character")));
    QVERIFY(message.contains(QStringLiteral("Preview:")));
    QVERIFY(message.contains(QStringLiteral("BAD")));
    QCOMPARE(fixture.session->state(), IrcSession::State::CapLs);

    fixture.transport->injectBytes(QByteArray("\0PASS hunter2\r\n", 15));
    QCOMPARE(errors.size(), 2);
    const QString leadingNul = errors.at(1).at(2).toString();
    QVERIFY(leadingNul.contains(QStringLiteral("invalid character")));
    QVERIFY(leadingNul.contains(QStringLiteral("PASS ***")));
    QVERIFY(!leadingNul.contains(QStringLiteral("hunter2")));

    fixture.transport->injectBytes(QByteArray("PASS\0hunter2\r\n", 14));
    QCOMPARE(errors.size(), 3);
    const QString infixNul = errors.at(2).at(2).toString();
    QVERIFY(infixNul.contains(QStringLiteral("invalid character")));
    QVERIFY(infixNul.contains(QStringLiteral("PASS ***")));
    QVERIFY(!infixNul.contains(QStringLiteral("hunter2")));

    fixture.transport->injectBytes(QByteArray("P\0ASS hunter2\r\n", 15));
    QCOMPARE(errors.size(), 4);
    const QString splitVerb = errors.at(3).at(2).toString();
    QVERIFY(splitVerb.contains(QStringLiteral("invalid character")));
    QVERIFY(splitVerb.contains(QStringLiteral("PASS ***")));
    QVERIFY(!splitVerb.contains(QStringLiteral("hunter2")));

    fixture.transport->injectBytes(QByteArray("\x01PASS hunter2\r\n", 15));
    QCOMPARE(errors.size(), 5);
    const QString sohPass = errors.at(4).at(2).toString();
    QVERIFY(sohPass.contains(QStringLiteral("invalid command")));
    QVERIFY(sohPass.contains(QStringLiteral("PASS ***")));
    QVERIFY(!sohPass.contains(QStringLiteral("hunter2")));

    fixture.transport->injectBytes(QByteArray("PASS\x7F hunter2\r\n", 15));
    QCOMPARE(errors.size(), 6);
    const QString delPass = errors.at(5).at(2).toString();
    QVERIFY(delPass.contains(QStringLiteral("invalid command")));
    QVERIFY(delPass.contains(QStringLiteral("PASS ***")));
    QVERIFY(!delPass.contains(QStringLiteral("hunter2")));

    fixture.transport->injectBytes(QByteArrayLiteral("P@SS hunter2\r\n"));
    QCOMPARE(errors.size(), 7);
    const QString mangledPass = errors.at(6).at(2).toString();
    QVERIFY(mangledPass.contains(QStringLiteral("invalid command")));
    QVERIFY(mangledPass.contains(QStringLiteral("PASS ***")));
    QVERIFY(!mangledPass.contains(QStringLiteral("hunter2")));

    fixture.transport->injectBytes(QByteArrayLiteral("PA@SS hunter2\r\n"));
    QCOMPARE(errors.size(), 8);
    const QString punctPass = errors.at(7).at(2).toString();
    QVERIFY(punctPass.contains(QStringLiteral("invalid command")));
    QVERIFY(punctPass.contains(QStringLiteral("PASS ***")));
    QVERIFY(!punctPass.contains(QStringLiteral("hunter2")));

    fixture.transport->injectBytes(QByteArrayLiteral("32@4 omairc #omarchy +k s3cret\r\n"));
    QCOMPARE(errors.size(), 9);
    const QString mangledModes = errors.at(8).at(2).toString();
    QVERIFY(mangledModes.contains(QStringLiteral("invalid command")));
    QVERIFY(mangledModes.contains(QStringLiteral("+k ***")));
    QVERIFY(!mangledModes.contains(QStringLiteral("s3cret")));

    QByteArray splitIdentifyFrame = QByteArrayLiteral("PRIVMSG nickserv :identif");
    splitIdentifyFrame.append('\0');
    splitIdentifyFrame.append(QByteArrayLiteral(" my_nick s3cret\r\n"));
    fixture.transport->injectBytes(splitIdentifyFrame);
    QCOMPARE(errors.size(), 10);
    const QString splitIdentify = errors.at(9).at(2).toString();
    QVERIFY(splitIdentify.contains(QStringLiteral("invalid character")));
    QVERIFY(splitIdentify.contains(QStringLiteral("IDENTIFY ***")));
    QVERIFY(!splitIdentify.contains(QStringLiteral("s3cret")));

    fixture.transport->injectBytes(QByteArray("P\0ASS\0hunter2\r\n", 15));
    QCOMPARE(errors.size(), 11);
    const QString splitSecret = errors.at(10).at(2).toString();
    QVERIFY(splitSecret.contains(QStringLiteral("invalid character")));
    QVERIFY(splitSecret.contains(QStringLiteral("PASS ***")));
    QVERIFY(!splitSecret.contains(QStringLiteral("hunter2")));

    fixture.transport->injectBytes(
        QByteArrayLiteral("@bad tag=foo PRIVMSG nickserv :identify my_nick s3cret\r\n"));
    QCOMPARE(errors.size(), 12);
    const QString taggedIdentify = errors.at(11).at(2).toString();
    QVERIFY(taggedIdentify.contains(QStringLiteral("IDENTIFY ***")));
    QVERIFY(!taggedIdentify.contains(QStringLiteral("s3cret")));
}

void SessionTest::overlongFrameLogsPreviewWithoutSecrets()
{
    Fixture fixture;
    QSignalSpy errors(fixture.session, &IrcSession::errorOccurred);
    fixture.connectTls();

    QByteArray overlong = QByteArrayLiteral("PING :");
    overlong.append(int(IrcFramer::kMaxInboundClassicFrameBytes) - overlong.size() - 1, 'z');
    overlong.append("\r\n");
    fixture.transport->injectBytes(overlong);

    QCOMPARE(errors.size(), 1);
    const QString message = errors.at(0).at(2).toString();
    QVERIFY(message.contains(QStringLiteral("too many bytes")));
    QVERIFY(message.contains(
        QStringLiteral("%1 bytes").arg(IrcFramer::kMaxInboundClassicFrameBytes - 1)));
    QVERIFY(message.contains(QStringLiteral("Preview: PING :")));

    QByteArray secret = QByteArrayLiteral("PASS hunter2 ");
    secret.append(int(IrcFramer::kMaxInboundClassicFrameBytes) - secret.size() - 1, 'x');
    secret.append("\r\nPING :ok\r\n");
    fixture.transport->injectBytes(secret);

    QCOMPARE(errors.size(), 2);
    const QString redacted = errors.at(1).at(2).toString();
    QVERIFY(redacted.contains(QStringLiteral("too many bytes")));
    QVERIFY(redacted.contains(QStringLiteral("PASS ***")));
    QVERIFY(!redacted.contains(QStringLiteral("hunter2")));

    QByteArray keyedJoin = QByteArrayLiteral("JOIN #secret hunter2 ");
    keyedJoin.append(int(IrcFramer::kMaxInboundClassicFrameBytes) - keyedJoin.size() - 1,
                     'x');
    keyedJoin.append("\r\nPING :ok\r\n");
    fixture.transport->injectBytes(keyedJoin);

    QCOMPARE(errors.size(), 3);
    const QString joinPreview = errors.at(2).at(2).toString();
    QVERIFY(joinPreview.contains(QStringLiteral("too many bytes")));
    QVERIFY(joinPreview.contains(QStringLiteral("JOIN #secret ***")));
    QVERIFY(!joinPreview.contains(QStringLiteral("hunter2")));

    QByteArray identify = QByteArrayLiteral("PRIVMSG nickserv :identify my_nick s3cret ");
    identify.append(int(IrcFramer::kMaxInboundClassicFrameBytes) - identify.size() - 1,
                    'x');
    identify.append("\r\nPING :ok\r\n");
    fixture.transport->injectBytes(identify);

    QCOMPARE(errors.size(), 4);
    const QString identifyPreview = errors.at(3).at(2).toString();
    QVERIFY(identifyPreview.contains(QStringLiteral("too many bytes")));
    QVERIFY(identifyPreview.contains(QStringLiteral("PRIVMSG nickserv :IDENTIFY ***")));
    QVERIFY(!identifyPreview.contains(QStringLiteral("s3cret")));

    QByteArray keyedMode = QByteArrayLiteral("MODE #omarchy +k s3cret ");
    keyedMode.append(int(IrcFramer::kMaxInboundClassicFrameBytes) - keyedMode.size() - 1,
                     'x');
    keyedMode.append("\r\nPING :ok\r\n");
    fixture.transport->injectBytes(keyedMode);

    QCOMPARE(errors.size(), 5);
    const QString modePreview = errors.at(4).at(2).toString();
    QVERIFY(modePreview.contains(QStringLiteral("too many bytes")));
    QVERIFY(modePreview.contains(QStringLiteral("MODE #omarchy +k ***")));
    QVERIFY(!modePreview.contains(QStringLiteral("s3cret")));

    QByteArray tabPass = QByteArrayLiteral("PASS\thunter2 ");
    tabPass.append(int(IrcFramer::kMaxInboundClassicFrameBytes) - tabPass.size() - 1, 'x');
    tabPass.append("\r\nPING :ok\r\n");
    fixture.transport->injectBytes(tabPass);

    QCOMPARE(errors.size(), 6);
    const QString tabPreview = errors.at(5).at(2).toString();
    QVERIFY(tabPreview.contains(QStringLiteral("too many bytes")));
    QVERIFY(tabPreview.contains(QStringLiteral("PASS ***")));
    QVERIFY(!tabPreview.contains(QStringLiteral("hunter2")));

    QByteArray ctcpIdentify = QByteArrayLiteral(
        "PRIVMSG nickserv :\x01identify my_nick s3cret\x01 ");
    ctcpIdentify.append(
        int(IrcFramer::kMaxInboundClassicFrameBytes) - ctcpIdentify.size() - 1, 'x');
    ctcpIdentify.append("\r\nPING :ok\r\n");
    fixture.transport->injectBytes(ctcpIdentify);

    QCOMPARE(errors.size(), 7);
    const QString ctcpPreview = errors.at(6).at(2).toString();
    QVERIFY(ctcpPreview.contains(QStringLiteral("too many bytes")));
    QVERIFY(ctcpPreview.contains(QStringLiteral("PRIVMSG nickserv :IDENTIFY ***")));
    QVERIFY(!ctcpPreview.contains(QStringLiteral("s3cret")));
}

void SessionTest::reconnectCanBeCancelled()
{
    Fixture fixture;
    fixture.connectTls();
    fixture.transport->remoteClose();
    QVERIFY(fixture.timer->active);

    fixture.session->stop();
    QCOMPARE(fixture.session->state(), IrcSession::State::Idle);
    QVERIFY(!fixture.timer->active);
    const int connectCount = fixture.timer->delays.size();
    fixture.timer->fire();
    QCOMPARE(fixture.timer->delays.size(), connectCount);
    QCOMPARE(fixture.transport->connectionState(),
             IrcTransport::ConnectionState::Disconnected);
}

void SessionTest::reconnectDelayIsBoundedExponential()
{
    Fixture fixture(config(), FixtureOptions{3});
    fixture.connectTls();
    fixture.transport->remoteClose();
    QCOMPARE(fixture.timer->delays, QList<int>{250});

    fixture.timer->fire();
    QCOMPARE(fixture.transport->connectionState(),
             IrcTransport::ConnectionState::Connecting);
    fixture.transport->completeConnect();
    fixture.transport->remoteClose();
    QCOMPARE(fixture.timer->delays, QList<int>({250, 500}));

    fixture.timer->fire();
    fixture.transport->completeConnect();
    fixture.transport->remoteClose();
    QCOMPARE(fixture.timer->delays, QList<int>({250, 500, 1000}));
    QCOMPARE(fixture.session->state(), IrcSession::State::Idle);
    QVERIFY(!fixture.timer->active);
}

void SessionTest::reconnectKeepsRetryingUntilStop()
{
    Fixture fixture;
    fixture.connectTls();
    fixture.transport->remoteClose();

    fixture.timer->fire();
    fixture.transport->completeConnect();
    fixture.transport->remoteClose();

    fixture.timer->fire();
    fixture.transport->completeConnect();
    fixture.transport->remoteClose();

    fixture.timer->fire();
    fixture.transport->completeConnect();
    fixture.transport->remoteClose();

    QCOMPARE(fixture.session->state(), IrcSession::State::Reconnecting);
    QVERIFY(fixture.timer->delays.size() > 3);
    QCOMPARE(fixture.timer->delays.last(), 1000);

    fixture.session->stop();
    QCOMPARE(fixture.session->state(), IrcSession::State::Idle);
    QVERIFY(!fixture.timer->active);
}

void SessionTest::quitStopsReconnectWait()
{
    Fixture fixture;
    fixture.connectTls();
    fixture.transport->remoteClose();
    QCOMPARE(fixture.session->state(), IrcSession::State::Reconnecting);
    QVERIFY(fixture.timer->active);

    QVERIFY(fixture.session->quit());
    QCOMPARE(fixture.session->state(), IrcSession::State::Idle);
    QVERIFY(!fixture.timer->active);
    const int connectCount = fixture.transport->writtenFrames().size();
    fixture.timer->fire();
    QCOMPARE(fixture.session->state(), IrcSession::State::Idle);
    QCOMPARE(fixture.transport->writtenFrames().size(), connectCount);
    QCOMPARE(fixture.transport->connectionState(),
             IrcTransport::ConnectionState::Disconnected);
}

void SessionTest::retryableNetworkErrorIsEmittedOnceUntilWelcome()
{
    Fixture fixture;
    QSignalSpy errors(fixture.session, &IrcSession::errorOccurred);
    fixture.connectTls();
    fixture.transport->remoteClose();

    fixture.timer->fire();
    fixture.transport->completeConnect();
    fixture.transport->remoteClose();

    fixture.timer->fire();
    fixture.transport->completeConnect();
    fixture.transport->remoteClose();

    QCOMPARE(errors.size(), 1);
    QCOMPARE(qvariant_cast<IrcSession::ErrorKind>(errors.at(0).at(1)),
             IrcSession::ErrorKind::Network);

    fixture.timer->fire();
    fixture.transport->completeConnect();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);

    fixture.transport->remoteClose();
    QCOMPARE(errors.size(), 2);
    QCOMPARE(qvariant_cast<IrcSession::ErrorKind>(errors.at(1).at(1)),
             IrcSession::ErrorKind::Network);
}

void SessionTest::retryableErrorsStayDedupedAcrossKindsUntilWelcome()
{
    Fixture fixture;
    QSignalSpy errors(fixture.session, &IrcSession::errorOccurred);
    fixture.connectTls();
    fixture.transport->remoteClose();

    QCOMPARE(errors.size(), 1);
    QCOMPARE(qvariant_cast<IrcSession::ErrorKind>(errors.at(0).at(1)),
             IrcSession::ErrorKind::Network);

    fixture.timer->fire();
    fixture.transport->failConnect(QStringLiteral("TLS handshake failed"));

    QCOMPARE(errors.size(), 2);
    QCOMPARE(qvariant_cast<IrcSession::ErrorKind>(errors.at(1).at(1)),
             IrcSession::ErrorKind::Tls);
    QCOMPARE(fixture.session->state(), IrcSession::State::Reconnecting);

    fixture.timer->fire();
    fixture.transport->completeConnect();
    fixture.transport->remoteClose();

    QCOMPARE(errors.size(), 2);
    QCOMPARE(qvariant_cast<IrcSession::ErrorKind>(errors.at(0).at(1)),
             IrcSession::ErrorKind::Network);
    QCOMPARE(qvariant_cast<IrcSession::ErrorKind>(errors.at(1).at(1)),
             IrcSession::ErrorKind::Tls);
}

void SessionTest::quitRejectsInvalidReasonWhileRegistered()
{
    Fixture fixture;
    fixture.registerWithWelcome();
    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);

    QVERIFY(!fixture.session->quit(QStringLiteral("bad\nreason")));
    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);
    QVERIFY(!fixture.wrote(QByteArrayLiteral("QUIT :bad\nreason\r\n")));

    fixture.transport->remoteClose();
    QCOMPARE(fixture.session->state(), IrcSession::State::Reconnecting);
    QVERIFY(fixture.session->quit());
    QCOMPARE(fixture.session->state(), IrcSession::State::Idle);
}

void SessionTest::authenticationFailureIsExplicit()
{
    IrcSessionConfig saslConfig = config();
    saslConfig.password = QStringLiteral("secret");
    Fixture fixture(saslConfig);
    QSignalSpy errors(fixture.session, &IrcSession::errorOccurred);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :sasl\r\n"
                          ":server CAP omairc ACK :sasl\r\n"
                          ":server 904 omairc :SASL failed\r\n"));

    QCOMPARE(fixture.session->state(), IrcSession::State::Failed);
    QCOMPARE(qvariant_cast<IrcSession::ErrorKind>(errors.last().at(1)),
             IrcSession::ErrorKind::Authentication);
}

void SessionTest::destructionWhileConnectingIsSafe()
{
    auto *transport = new FakeIrcTransport;
    QPointer<FakeIrcTransport> transportGuard(transport);
    auto *session = new IrcSession(config(), transport, new FakeReconnectTimer);
    session->start();
    QCOMPARE(transport->connectionState(),
             IrcTransport::ConnectionState::Connecting);

    delete session;
    QVERIFY(transportGuard.isNull());
}

void SessionTest::managerStartsTwoLiveNetworks()
{
    IrcSessionManager manager;
    auto *firstTransport = new FakeIrcTransport;
    auto *secondTransport = new FakeIrcTransport;
    IrcSession *first = manager.createSession(
        config(QStringLiteral("network-a")), firstTransport, new FakeReconnectTimer);
    IrcSession *second = manager.createSession(
        config(QStringLiteral("network-b")), secondTransport, new FakeReconnectTimer);

    QVERIFY(first);
    QVERIFY(second);
    QCOMPARE(manager.findSession(QStringLiteral("network-a")), first);
    QVERIFY(manager.activateSession(QStringLiteral("network-a")));
    QVERIFY(manager.activateSession(QStringLiteral("network-b")));
    QCOMPARE(first->state(), IrcSession::State::Connecting);
    QCOMPARE(second->state(), IrcSession::State::Connecting);
    QCOMPARE(firstTransport->connectionState(),
             IrcTransport::ConnectionState::Connecting);
    QCOMPARE(secondTransport->connectionState(),
             IrcTransport::ConnectionState::Connecting);

    QVERIFY(manager.stopSession(QStringLiteral("network-a")));
    QCOMPARE(second->state(), IrcSession::State::Connecting);
}

void SessionTest::managerCreateStaysAddOnly()
{
    IrcSessionManager manager;
    auto *firstTransport = new FakeIrcTransport;
    auto *secondTransport = new FakeIrcTransport;
    IrcSession *first = manager.createSession(
        config(QStringLiteral("network-a")), firstTransport, new FakeReconnectTimer);
    QVERIFY(first);
    QVERIFY(!manager.createSession(
        config(QStringLiteral("network-a")), secondTransport, new FakeReconnectTimer));
    QCOMPARE(manager.findSession(QStringLiteral("network-a")), first);
    delete secondTransport;
}

void SessionTest::managerDiscardUnregistersImmediately()
{
    IrcSessionManager manager;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = manager.createSession(
        config(QStringLiteral("network-a")), transport, new FakeReconnectTimer);
    QVERIFY(session);
    QPointer<IrcSession> guard(session);

    QVERIFY(manager.activateSession(QStringLiteral("network-a")));
    QVERIFY(manager.discardSession(QStringLiteral("network-a")));
    QCOMPARE(manager.findSession(QStringLiteral("network-a")), nullptr);
    QVERIFY(!manager.discardSession(QStringLiteral("network-a")));

    auto *replacementTransport = new FakeIrcTransport;
    IrcSession *replacement = manager.createSession(
        config(QStringLiteral("network-a")), replacementTransport, new FakeReconnectTimer);
    QVERIFY(replacement);
    QVERIFY(replacement != session);

    QVERIFY(!guard.isNull());
    QCoreApplication::sendPostedEvents(session, QEvent::DeferredDelete);
    QVERIFY(guard.isNull());
    QCOMPARE(manager.findSession(QStringLiteral("network-a")), replacement);
}

namespace
{
struct StatusCollector
{
    explicit StatusCollector(IrcSession *session)
    {
        QObject::connect(session, &IrcSession::statusEntry, session,
                         [this](const IrcStatusEntry& entry) {
            entries.append(entry);
        });
    }

    bool hasLabel(const QString& label) const
    {
        for (const IrcStatusEntry& entry : entries) {
            if (entry.label() == label)
                return true;
        }
        return false;
    }

    bool anyFieldContains(const QString& needle) const
    {
        for (const IrcStatusEntry& entry : entries) {
            if (entry.text().contains(needle)
                || entry.label().contains(needle)
                || entry.networkId().contains(needle)) {
                return true;
            }
        }
        return false;
    }

    QList<IrcStatusEntry> entries;
};
}

void SessionTest::pingAndWelcomeProduceStatusEntries()
{
    Fixture fixture;
    StatusCollector status(fixture.session);

    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          "PING :abc\r\n"
                          ":server 001 omairc :Welcome\r\n"));

    QVERIFY(status.hasLabel(QStringLiteral("PING")));
    QVERIFY(status.hasLabel(QStringLiteral("001")));
}

void SessionTest::configuredPasswordNeverAppearsInStatusEntries()
{
    IrcSessionConfig passwordConfig = config();
    passwordConfig.password = QStringLiteral("hunter2");
    Fixture passFixture(passwordConfig);
    StatusCollector passStatus(passFixture.session);

    passFixture.connectTls();
    passFixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          "PING :abc\r\n"
                          ":server 001 omairc :Welcome\r\n"));

    QVERIFY(passStatus.hasLabel(QStringLiteral("PING")));
    QVERIFY(passStatus.hasLabel(QStringLiteral("001")));
    QVERIFY(passStatus.hasLabel(QStringLiteral("PASS")));
    QVERIFY(!passStatus.anyFieldContains(QStringLiteral("hunter2")));

    IrcSessionConfig saslConfig = config(QStringLiteral("network-sasl"));
    saslConfig.password = QStringLiteral("hunter2");
    Fixture saslFixture(saslConfig);
    StatusCollector saslStatus(saslFixture.session);

    saslFixture.connectTls();
    saslFixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :sasl=PLAIN\r\n"
                          ":server CAP omairc ACK :sasl\r\n"
                          "AUTHENTICATE +\r\n"
                          ":server 903 omairc :SASL successful\r\n"
                          ":server 001 omairc :Welcome\r\n"));

    QVERIFY(saslStatus.hasLabel(QStringLiteral("AUTHENTICATE")));
    QVERIFY(saslStatus.hasLabel(QStringLiteral("001")));
    QVERIFY(!saslStatus.anyFieldContains(QStringLiteral("hunter2")));
    for (const IrcStatusEntry& entry : saslStatus.entries) {
        if (entry.label() == QStringLiteral("AUTHENTICATE")
            || entry.label() == QStringLiteral("PASS")) {
            QCOMPARE(entry.text(), entry.label() + QStringLiteral(" ***"));
        }
    }
}

void SessionTest::plaintextIdentifyEmitsOneStatusWarning()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.tlsEnabled = false;
    sessionConfig.nickServPassword = QStringLiteral("nick-secret");
    Fixture fixture(sessionConfig);
    StatusCollector status(fixture.session);
    fixture.session->start();
    fixture.transport->completeConnect();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :account-notify\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    int warnings = 0;
    for (const IrcStatusEntry &entry : status.entries) {
        if (entry.label() == QStringLiteral("identify")) {
            ++warnings;
            QCOMPARE(entry.text(),
                     QStringLiteral("NickServ identify will be sent in clear text"));
            QCOMPARE(entry.severity(), IrcLogSeverity::Alert);
        }
        QVERIFY(!entry.text().contains(QStringLiteral("nick-secret")));
    }
    QCOMPARE(warnings, 1);
    QVERIFY(status.anyFieldContains(QStringLiteral("PRIVMSG NickServ :IDENTIFY ***")));
}

void SessionTest::automaticIdentifyDoesNotAppearInStatusAsSecret()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.nickServPassword = QStringLiteral("nick-secret");
    Fixture fixture(sessionConfig);
    StatusCollector status(fixture.session);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :account-notify\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    QVERIFY(status.anyFieldContains(QStringLiteral("PRIVMSG NickServ :IDENTIFY ***")));
    QVERIFY(!status.anyFieldContains(QStringLiteral("nick-secret")));
}

void SessionTest::keyedJoinIsRedactedInStatusEntries()
{
    Fixture fixture;
    StatusCollector status(fixture.session);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"));

    const auto target = IrcJoinTarget::make(QStringLiteral("#secret"),
                                            QStringLiteral("hunter2"));
    QVERIFY(target);
    QVERIFY(fixture.session->join(*target));
    QCOMPARE(fixture.transport->writtenFrames().last(),
             QByteArrayLiteral("JOIN #secret hunter2\r\n"));
    QVERIFY(status.anyFieldContains(QStringLiteral("JOIN #secret ***")));
    QVERIFY(!status.anyFieldContains(QStringLiteral("hunter2")));

    const IrcStatusEntry unkeyed = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"), QByteArrayLiteral("JOIN #omarchy\r\n"));
    QCOMPARE(unkeyed.text(), QStringLiteral("JOIN #omarchy"));

    const IrcStatusEntry keyed = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"), QByteArrayLiteral("JOIN #secret hunter2\r\n"));
    QCOMPARE(keyed.text(), QStringLiteral("JOIN #secret ***"));
    QVERIFY(!keyed.text().contains(QStringLiteral("hunter2")));
}

void SessionTest::serviceIdentifyIsRedactedInStatusEntries()
{
    const IrcStatusEntry identify = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"),
        QByteArrayLiteral("PRIVMSG nickserv :identify my_nick s3cret\r\n"));
    QCOMPARE(identify.text(), QStringLiteral("PRIVMSG nickserv :IDENTIFY ***"));
    QVERIFY(!identify.text().contains(QStringLiteral("s3cret")));

    const IrcStatusEntry alias = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"),
        QByteArrayLiteral("PRIVMSG NS :IDENTIFY account hunter2\r\n"),
        QStringLiteral("#&"));
    QCOMPARE(alias.text(), QStringLiteral("PRIVMSG NS :IDENTIFY ***"));
    QVERIFY(!alias.text().contains(QStringLiteral("hunter2")));

    const IrcStatusEntry identifyTabs = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"),
        QByteArrayLiteral("PRIVMSG nickserv :identify\tmy_nick\ts3cret\r\n"));
    QCOMPARE(identifyTabs.text(), QStringLiteral("PRIVMSG nickserv :IDENTIFY ***"));
    QVERIFY(!identifyTabs.text().contains(QStringLiteral("s3cret")));

    const IrcStatusEntry identifyMiddles = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"),
        QByteArrayLiteral("PRIVMSG nickserv IDENTIFY my_nick s3cret\r\n"));
    QCOMPARE(identifyMiddles.text(), QStringLiteral("PRIVMSG nickserv :IDENTIFY ***"));
    QVERIFY(!identifyMiddles.text().contains(QStringLiteral("s3cret")));

    const IrcStatusEntry identifyCtcp = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"),
        QByteArrayLiteral("PRIVMSG nickserv :\x01identify my_nick s3cret\x01\r\n"));
    QCOMPARE(identifyCtcp.text(), QStringLiteral("PRIVMSG nickserv :IDENTIFY ***"));
    QVERIFY(!identifyCtcp.text().contains(QStringLiteral("s3cret")));

    const IrcStatusEntry identifyCtcpPadded = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"),
        QByteArrayLiteral("PRIVMSG nickserv : \x01identify my_nick s3cret\x01 \r\n"));
    QCOMPARE(identifyCtcpPadded.text(), QStringLiteral("PRIVMSG nickserv :IDENTIFY ***"));
    QVERIFY(!identifyCtcpPadded.text().contains(QStringLiteral("s3cret")));

    const IrcStatusEntry identifTypo = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"),
        QByteArrayLiteral("PRIVMSG nickserv :identif my_nick s3cret\r\n"));
    QCOMPARE(identifTypo.text(), QStringLiteral("PRIVMSG nickserv :IDENTIFY ***"));
    QVERIFY(!identifTypo.text().contains(QStringLiteral("s3cret")));

    const IrcStatusEntry dashServ = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"),
        QByteArrayLiteral("PRIVMSG -serv :identify my_nick s3cret\r\n"));
    QCOMPARE(dashServ.text(), QStringLiteral("PRIVMSG -serv :IDENTIFY ***"));
    QVERIFY(!dashServ.text().contains(QStringLiteral("s3cret")));

    const IrcStatusEntry bracketServ = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"),
        QByteArrayLiteral("PRIVMSG [serv :identify my_nick s3cret\r\n"));
    QCOMPARE(bracketServ.text(), QStringLiteral("PRIVMSG [serv :IDENTIFY ***"));
    QVERIFY(!bracketServ.text().contains(QStringLiteral("s3cret")));

    const IrcStatusEntry setPassword = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"),
        QByteArrayLiteral("PRIVMSG nickserv :set password s3cret\r\n"));
    QCOMPARE(setPassword.text(), QStringLiteral("PRIVMSG nickserv :SET PASSWORD ***"));
    QVERIFY(!setPassword.text().contains(QStringLiteral("s3cret")));

    const IrcStatusEntry setEmail = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"),
        QByteArrayLiteral("PRIVMSG nickserv :set email user@example.net\r\n"));
    QCOMPARE(setEmail.text(),
             QStringLiteral("PRIVMSG nickserv :set email user@example.net"));

    const IrcStatusEntry passTabs = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"), QByteArrayLiteral("PASS\thunter2\r\n"));
    QCOMPARE(passTabs.text(), QStringLiteral("PASS ***"));
    QCOMPARE(passTabs.label(), QStringLiteral("PASS"));
    QVERIFY(!passTabs.text().contains(QStringLiteral("hunter2")));
    QVERIFY(!passTabs.label().contains(QStringLiteral("hunter2")));

    const IrcStatusEntry ghost = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"),
        QByteArrayLiteral("NOTICE NickServ :ghost old_nick s3cret\r\n"));
    QCOMPARE(ghost.text(), QStringLiteral("NOTICE NickServ :GHOST ***"));
    QVERIFY(!ghost.text().contains(QStringLiteral("s3cret")));

    const IrcStatusEntry oper = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"), QByteArrayLiteral("OPER admin s3cret\r\n"));
    QCOMPARE(oper.text(), QStringLiteral("OPER admin ***"));
    QVERIFY(!oper.text().contains(QStringLiteral("s3cret")));

    const IrcStatusEntry keyedMode = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"), QByteArrayLiteral("MODE #omarchy +k s3cret\r\n"));
    QCOMPARE(keyedMode.text(), QStringLiteral("MODE #omarchy +k ***"));
    QVERIFY(!keyedMode.text().contains(QStringLiteral("s3cret")));

    const IrcStatusEntry opThenKey = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"),
        QByteArrayLiteral("MODE #omarchy +ok alice s3cret\r\n"));
    QCOMPARE(opThenKey.text(), QStringLiteral("MODE #omarchy +ok alice ***"));
    QVERIFY(!opThenKey.text().contains(QStringLiteral("s3cret")));

    const IrcStatusEntry keyThenOp = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"),
        QByteArrayLiteral("MODE #omarchy +ko s3cret alice\r\n"));
    QCOMPARE(keyThenOp.text(), QStringLiteral("MODE #omarchy +ko *** alice"));
    QVERIFY(!keyThenOp.text().contains(QStringLiteral("s3cret")));

    const IrcStatusEntry unknownThenKey = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"),
        QByteArrayLiteral("MODE #omarchy +fk 10 s3cret\r\n"));
    QCOMPARE(unknownThenKey.text(), QStringLiteral("MODE #omarchy +fk ***"));
    QVERIFY(!unknownThenKey.text().contains(QStringLiteral("s3cret")));

    const IrcStatusEntry adminThenKey = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"),
        QByteArrayLiteral("MODE #omarchy +ak s3cret\r\n"));
    QCOMPARE(adminThenKey.text(), QStringLiteral("MODE #omarchy +ak ***"));
    QVERIFY(!adminThenKey.text().contains(QStringLiteral("s3cret")));

    const IrcStatusEntry opMode = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"), QByteArrayLiteral("MODE #omarchy +o alice\r\n"));
    QCOMPARE(opMode.text(), QStringLiteral("MODE #omarchy +o alice"));

    const IrcStatusEntry commaTargets = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"),
        QByteArrayLiteral("PRIVMSG nickserv,#discuss :identify my_nick s3cret\r\n"));
    QCOMPARE(commaTargets.text(),
             QStringLiteral("PRIVMSG nickserv,#discuss :IDENTIFY ***"));
    QVERIFY(!commaTargets.text().contains(QStringLiteral("s3cret")));

    const IrcStatusEntry commaSecond = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"),
        QByteArrayLiteral("PRIVMSG #discuss,nickserv :identify my_nick s3cret\r\n"));
    QCOMPARE(commaSecond.text(),
             QStringLiteral("PRIVMSG #discuss,nickserv :IDENTIFY ***"));
    QVERIFY(!commaSecond.text().contains(QStringLiteral("s3cret")));

    const IrcStatusEntry atHost = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"),
        QByteArrayLiteral("PRIVMSG NickServ@services :identify my_nick s3cret\r\n"));
    QCOMPARE(atHost.text(), QStringLiteral("PRIVMSG NickServ@services :IDENTIFY ***"));
    QVERIFY(!atHost.text().contains(QStringLiteral("s3cret")));

    const IrcStatusEntry fullMask = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"),
        QByteArrayLiteral("PRIVMSG NickServ!ns@services :identify my_nick s3cret\r\n"));
    QCOMPARE(fullMask.text(),
             QStringLiteral("PRIVMSG NickServ!ns@services :IDENTIFY ***"));
    QVERIFY(!fullMask.text().contains(QStringLiteral("s3cret")));

    const IrcStatusEntry hostOnly = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"),
        QByteArrayLiteral("PRIVMSG helper!u@services :identify my_nick s3cret\r\n"));
    QCOMPARE(hostOnly.text(),
             QStringLiteral("PRIVMSG helper!u@services :IDENTIFY ***"));
    QVERIFY(!hostOnly.text().contains(QStringLiteral("s3cret")));

    const IrcStatusEntry keyedModes = IrcStatusEntry::incoming(
        QStringLiteral("network-a"),
        mustParse(":irc 324 omairc #omarchy +k s3cret"));
    QCOMPARE(keyedModes.text(), QStringLiteral("#omarchy +k ***"));
    QVERIFY(!keyedModes.text().contains(QStringLiteral("s3cret")));

    const IrcStatusEntry listedKeyLimit = IrcStatusEntry::incoming(
        QStringLiteral("network-a"),
        mustParse(":irc 324 omairc #omarchy +kl s3cret 40"));
    QCOMPARE(listedKeyLimit.text(), QStringLiteral("#omarchy +kl *** 40"));
    QVERIFY(!listedKeyLimit.text().contains(QStringLiteral("s3cret")));

    const IrcStatusEntry listedLimitKey = IrcStatusEntry::incoming(
        QStringLiteral("network-a"),
        mustParse(":irc 324 omairc #omarchy +lk 40 s3cret"));
    QCOMPARE(listedLimitKey.text(), QStringLiteral("#omarchy +lk 40 ***"));
    QVERIFY(!listedLimitKey.text().contains(QStringLiteral("s3cret")));

    const IrcStatusEntry listedModes = IrcStatusEntry::incoming(
        QStringLiteral("network-a"),
        mustParse(":irc 324 omairc #omarchy +nt"));
    QCOMPARE(listedModes.text(), QStringLiteral("#omarchy +nt"));

    const IrcStatusEntry extendedJoin = IrcStatusEntry::incoming(
        QStringLiteral("network-a"),
        mustParse(":alice!u@h JOIN #omarchy alice :Alice"));
    QCOMPARE(extendedJoin.text(), QStringLiteral("#omarchy alice Alice"));

    const IrcStatusEntry literalStar = IrcStatusEntry::incoming(
        QStringLiteral("network-a"),
        mustParse(":irc MODE ***"));
    QCOMPARE(literalStar.text(), QStringLiteral("***"));

    const IrcStatusEntry incomingPass = IrcStatusEntry::incoming(
        QStringLiteral("network-a"),
        mustParse("PASS hunter2"));
    QCOMPARE(incomingPass.text(), QStringLiteral("PASS ***"));
    QVERIFY(!incomingPass.text().contains(QStringLiteral("hunter2")));
}

void SessionTest::channelTalkAboutServicesStaysReadable()
{
    const IrcStatusEntry talk = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"),
        QByteArrayLiteral("PRIVMSG #discuss :I told NickServ to IDENTIFY later\r\n"));
    QCOMPARE(talk.text(),
             QStringLiteral("PRIVMSG #discuss :I told NickServ to IDENTIFY later"));

    const IrcStatusEntry channel = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"),
        QByteArrayLiteral("PRIVMSG #serv :identify my_nick s3cret\r\n"));
    QCOMPARE(channel.text(),
             QStringLiteral("PRIVMSG #serv :identify my_nick s3cret"));

    const IrcStatusEntry channelWithAt = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"),
        QByteArrayLiteral("PRIVMSG #help@services :IDENTIFY examples stay visible\r\n"),
        QStringLiteral("#&"));
    QCOMPARE(channelWithAt.text(),
             QStringLiteral("PRIVMSG #help@services :IDENTIFY examples stay visible"));

    const IrcStatusEntry tildeChannel = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"),
        QByteArrayLiteral("PRIVMSG ~serv :identify my_nick s3cret\r\n"));
    QCOMPARE(tildeChannel.text(),
             QStringLiteral("PRIVMSG ~serv :identify my_nick s3cret"));

    const QByteArray dollarLine =
        QByteArrayLiteral("PRIVMSG $serv :identify my_nick s3cret\r\n");
    const IrcStatusEntry dollarChannel = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"), dollarLine);
    QCOMPARE(dollarChannel.text(),
             QStringLiteral("PRIVMSG $serv :identify my_nick s3cret"));

    const IrcStatusEntry dollarTyped = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"), dollarLine, QStringLiteral("$"));
    QCOMPARE(dollarTyped.text(),
             QStringLiteral("PRIVMSG $serv :identify my_nick s3cret"));

    const IrcStatusEntry dollarAsNick = IrcStatusEntry::outgoing(
        QStringLiteral("network-a"), dollarLine, QStringLiteral("#"));
    QCOMPARE(dollarAsNick.text(), QStringLiteral("PRIVMSG $serv :IDENTIFY ***"));
    QVERIFY(!dollarAsNick.text().contains(QStringLiteral("s3cret")));
}

void SessionTest::negotiatedChannelTypesClassifyDollarTargets()
{
    Fixture fixture;
    StatusCollector status(fixture.session);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":server 005 omairc CHANTYPES=$ PREFIX=(ov)@+ :are supported\r\n"));
    QVERIFY(fixture.session->sendPrivmsg(QStringLiteral("$serv"),
                                          QStringLiteral("identify my_nick s3cret")));
    QCOMPARE(fixture.transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG $serv :identify my_nick s3cret\r\n"));

    QString lastDollar;
    for (const IrcStatusEntry& entry : status.entries) {
        if (entry.text().startsWith(QStringLiteral("PRIVMSG $serv")))
            lastDollar = entry.text();
    }
    QCOMPARE(lastDollar, QStringLiteral("PRIVMSG $serv :identify my_nick s3cret"));

    fixture.transport->injectBytes(
        QByteArrayLiteral(":server 005 omairc CHANTYPES=# PREFIX=(ov)@+ :are supported\r\n"));
    QVERIFY(fixture.session->sendPrivmsg(QStringLiteral("$serv"),
                                          QStringLiteral("identify my_nick s3cret")));
    for (const IrcStatusEntry& entry : status.entries) {
        if (entry.text().startsWith(QStringLiteral("PRIVMSG $serv")))
            lastDollar = entry.text();
    }
    QCOMPARE(lastDollar, QStringLiteral("PRIVMSG $serv :IDENTIFY ***"));
    QVERIFY(!lastDollar.contains(QStringLiteral("s3cret")));

    fixture.transport->injectBytes(
        QByteArrayLiteral(":server 005 omairc AWAYLEN=200 :CHANTYPES=$ are supported\r\n"));
    QVERIFY(fixture.session->sendPrivmsg(QStringLiteral("$serv"),
                                          QStringLiteral("identify my_nick s3cret")));
    for (const IrcStatusEntry& entry : status.entries) {
        if (entry.text().startsWith(QStringLiteral("PRIVMSG $serv")))
            lastDollar = entry.text();
    }
    QCOMPARE(lastDollar, QStringLiteral("PRIVMSG $serv :IDENTIFY ***"));
    QVERIFY(!lastDollar.contains(QStringLiteral("s3cret")));
}

void SessionTest::serviceRepliesStayReadable()
{
    const IrcStatusEntry notice = IrcStatusEntry::incoming(
        QStringLiteral("libera"),
        mustParse(":NickServ!NickServ@services NOTICE me :Please identify"));
    QCOMPARE(notice.text(), QStringLiteral("-NickServ- Please identify"));

    const IrcStatusEntry privmsg = IrcStatusEntry::incoming(
        QStringLiteral("libera"),
        mustParse(":NickServ!NickServ@services PRIVMSG me :This nickname is registered."));
    QCOMPARE(privmsg.text(), QStringLiteral("This nickname is registered."));
}

void SessionTest::selfEchoToServiceIsRedacted()
{
    const IrcStatusEntry echo = IrcStatusEntry::incoming(
        QStringLiteral("libera"),
        mustParse(":me!u@h PRIVMSG nickserv :identify my_nick s3cret"));
    QCOMPARE(echo.text(), QStringLiteral("IDENTIFY ***"));
    QVERIFY(!echo.text().contains(QStringLiteral("s3cret")));

    const IrcStatusEntry servNick = IrcStatusEntry::incoming(
        QStringLiteral("libera"),
        mustParse(":myserv!u@h PRIVMSG nickserv :identify my_nick s3cret"));
    QCOMPARE(servNick.text(), QStringLiteral("IDENTIFY ***"));
    QVERIFY(!servNick.text().contains(QStringLiteral("s3cret")));

    const IrcStatusEntry middles = IrcStatusEntry::incoming(
        QStringLiteral("libera"),
        mustParse(":me!u@h PRIVMSG nickserv IDENTIFY my_nick s3cret"));
    QCOMPARE(middles.text(), QStringLiteral("IDENTIFY ***"));
    QVERIFY(!middles.text().contains(QStringLiteral("s3cret")));

    const IrcStatusEntry ctcp = IrcStatusEntry::incoming(
        QStringLiteral("libera"),
        mustParse(":me!u@h PRIVMSG nickserv :\x01IDENTIFY my_nick s3cret\x01"));
    QCOMPARE(ctcp.text(), QStringLiteral("IDENTIFY ***"));
    QVERIFY(!ctcp.text().contains(QStringLiteral("s3cret")));
    QCOMPARE(ctcp.label(), QStringLiteral("PRIVMSG"));

    const IrcStatusEntry setPassword = IrcStatusEntry::incoming(
        QStringLiteral("libera"),
        mustParse(":me!u@h PRIVMSG nickserv :set password s3cret"));
    QCOMPARE(setPassword.text(), QStringLiteral("SET PASSWORD ***"));
    QVERIFY(!setPassword.text().contains(QStringLiteral("s3cret")));

    const IrcStatusEntry setEmail = IrcStatusEntry::incoming(
        QStringLiteral("libera"),
        mustParse(":me!u@h PRIVMSG nickserv :set email user@example.net"));
    QCOMPARE(setEmail.text(), QStringLiteral("set email user@example.net"));
}

void SessionTest::sendPrivmsgValidatesTarget()
{
    Fixture fixture;
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);

    QVERIFY(fixture.session->sendPrivmsg(QStringLiteral("#omarchy"),
                                          QStringLiteral("hello")));
    QVERIFY(fixture.session->sendPrivmsg(QStringLiteral("lena"),
                                          QStringLiteral("hello")));
    QCOMPARE(fixture.transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG lena :hello\r\n"));
    QVERIFY(fixture.transport->writtenFrames().contains(
        QByteArrayLiteral("PRIVMSG #omarchy :hello\r\n")));

    const int before = fixture.transport->writtenFrames().size();
    const QStringList invalidTargets{
        QString(),
        QStringLiteral(" "),
        QStringLiteral("lena smith"),
        QStringLiteral("lena\t"),
        QStringLiteral("lena\n"),
        QStringLiteral("lena\r"),
        QString(QChar(0x01)) + QStringLiteral("lena"),
        QStringLiteral(":lena"),
    };
    for (const QString& target : invalidTargets)
        QVERIFY(!fixture.session->sendPrivmsg(target, QStringLiteral("hello")));
    QCOMPARE(fixture.transport->writtenFrames().size(), before);
}

void SessionTest::setTopicIsSetOnly()
{
    Fixture fixture;
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);

    const int before = fixture.transport->writtenFrames().size();
    QVERIFY(!fixture.session->setTopic(QStringLiteral("#omarchy"), QString()));
    QVERIFY(!fixture.session->setTopic(QString(), QStringLiteral("hello")));
    QCOMPARE(fixture.transport->writtenFrames().size(), before);
    for (const QByteArray& frame : fixture.transport->writtenFrames())
        QVERIFY(!frame.contains(QByteArrayLiteral("TOPIC")));

    QVERIFY(fixture.session->setTopic(QStringLiteral("#omarchy"),
                                      QStringLiteral("hello")));
    QCOMPARE(fixture.transport->writtenFrames().last(),
             QByteArrayLiteral("TOPIC #omarchy :hello\r\n"));
}

void SessionTest::kickWritesOptionalReason()
{
    Fixture fixture;
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);

    const int before = fixture.transport->writtenFrames().size();
    QVERIFY(!fixture.session->kick(QString(), QStringLiteral("alice"),
                                   QStringLiteral("spam")));
    QVERIFY(!fixture.session->kick(QStringLiteral("#omarchy"), QString(),
                                   QStringLiteral("spam")));
    QCOMPARE(fixture.transport->writtenFrames().size(), before);
    for (const QByteArray& frame : fixture.transport->writtenFrames())
        QVERIFY(!frame.contains(QByteArrayLiteral("KICK")));

    QVERIFY(fixture.session->kick(QStringLiteral("#omarchy"),
                                  QStringLiteral("alice"),
                                  QStringLiteral("spam")));
    QCOMPARE(fixture.transport->writtenFrames().last(),
             QByteArrayLiteral("KICK #omarchy alice :spam\r\n"));

    QVERIFY(fixture.session->kick(QStringLiteral("#omarchy"),
                                  QStringLiteral("alice"),
                                  QString()));
    QCOMPARE(fixture.transport->writtenFrames().last(),
             QByteArrayLiteral("KICK #omarchy alice\r\n"));
}

void SessionTest::setAwayEncodesOptionalReason()
{
    Fixture fixture;
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);

    QVERIFY(fixture.session->setAway(QStringLiteral("lunch")));
    QCOMPARE(fixture.transport->writtenFrames().last(),
             QByteArrayLiteral("AWAY :lunch\r\n"));

    QVERIFY(fixture.session->setAway({}));
    QCOMPARE(fixture.transport->writtenFrames().last(),
             QByteArrayLiteral("AWAY\r\n"));

    QVERIFY(fixture.session->setAway(QStringLiteral("   ")));
    QCOMPARE(fixture.transport->writtenFrames().last(),
             QByteArrayLiteral("AWAY\r\n"));

    QVERIFY(fixture.session->clearAway());
    QCOMPARE(fixture.transport->writtenFrames().last(),
             QByteArrayLiteral("AWAY\r\n"));
}

void SessionTest::whoisWritesDoubledNick()
{
    Fixture fixture;
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);

    QVERIFY(fixture.session->whois(QStringLiteral("lena")));
    QCOMPARE(fixture.transport->writtenFrames().last(),
             QByteArrayLiteral("WHOIS lena lena\r\n"));

    const int before = fixture.transport->writtenFrames().size();
    QVERIFY(!fixture.session->whois(QString()));
    QVERIFY(!fixture.session->whois(QStringLiteral("   ")));
    QCOMPARE(fixture.transport->writtenFrames().size(), before);
}

void SessionTest::whoisStatusLinesFormatKnownNumerics()
{
    const IrcStatusEntry user = IrcStatusEntry::incoming(
        QStringLiteral("libera"),
        mustParse(":irc 311 omairc lena ~lena user/host * :Lena"));
    QCOMPARE(user.label(), QStringLiteral("whois"));
    QCOMPARE(user.text(), QStringLiteral("lena is ~lena@user/host (Lena)"));
    QCOMPARE(user.severity(), IrcLogSeverity::Info);

    const IrcStatusEntry channels = IrcStatusEntry::incoming(
        QStringLiteral("libera"),
        mustParse(":irc 319 omairc lena :#omarchy #desktop"));
    QCOMPARE(channels.label(), QStringLiteral("whois"));
    QCOMPARE(channels.text(), QStringLiteral("lena is on #omarchy #desktop"));

    const IrcStatusEntry server = IrcStatusEntry::incoming(
        QStringLiteral("libera"),
        mustParse(":irc 312 omairc lena copper.libera.chat :London, UK"));
    QCOMPARE(server.label(), QStringLiteral("whois"));
    QCOMPARE(server.text(),
             QStringLiteral("lena using copper.libera.chat (London, UK)"));

    const IrcStatusEntry away = IrcStatusEntry::incoming(
        QStringLiteral("libera"),
        mustParse(":irc 301 omairc lena :gone fishing"));
    QCOMPARE(away.label(), QStringLiteral("whois"));
    QCOMPARE(away.text(), QStringLiteral("lena is away: gone fishing"));

    const IrcStatusEntry idle = IrcStatusEntry::incoming(
        QStringLiteral("libera"),
        mustParse(":irc 317 omairc lena 84 :seconds idle"));
    QCOMPARE(idle.label(), QStringLiteral("whois"));
    QCOMPARE(idle.text(), QStringLiteral("lena idle 84s"));

    const IrcStatusEntry idleSignon = IrcStatusEntry::incoming(
        QStringLiteral("libera"),
        mustParse(":irc 317 omairc lena 84 1700000000 :seconds idle"));
    QCOMPARE(idleSignon.label(), QStringLiteral("whois"));
    QCOMPARE(idleSignon.text(), QStringLiteral("lena idle 84s, signon 1700000000"));

    const IrcStatusEntry end = IrcStatusEntry::incoming(
        QStringLiteral("libera"),
        mustParse(":irc 318 omairc lena :End of /WHOIS list."));
    QCOMPARE(end.label(), QStringLiteral("whois"));
    QCOMPARE(end.text(), QStringLiteral("End of WHOIS for lena"));
    QVERIFY(end.whoisLine());
    QCOMPARE(end.whoisLine()->progress(), IrcWhoisLine::Progress::Terminal);

    const IrcStatusEntry account = IrcStatusEntry::incoming(
        QStringLiteral("libera"),
        mustParse(":irc 330 omairc lena pinkieval :is logged in as"));
    QCOMPARE(account.label(), QStringLiteral("whois"));
    QCOMPARE(account.text(), QStringLiteral("lena is logged in as pinkieval"));

    const IrcStatusEntry secure = IrcStatusEntry::incoming(
        QStringLiteral("libera"),
        mustParse(":irc 671 omairc lena :is using a secure connection"));
    QCOMPARE(secure.label(), QStringLiteral("whois"));
    QCOMPARE(secure.text(),
             QStringLiteral("lena is using a secure connection"));

    const IrcStatusEntry unknown = IrcStatusEntry::incoming(
        QStringLiteral("libera"),
        mustParse(":irc 335 omairc lena :bot"));
    QCOMPARE(unknown.label(), QStringLiteral("335"));
    QCOMPARE(unknown.text(), QStringLiteral("lena bot"));

    const IrcStatusEntry shortUser = IrcStatusEntry::incoming(
        QStringLiteral("libera"),
        mustParse(":irc 311 omairc lena"));
    QCOMPARE(shortUser.label(), QStringLiteral("311"));
    QCOMPARE(shortUser.text(), QStringLiteral("lena"));

    const IrcStatusEntry missing = IrcStatusEntry::incoming(
        QStringLiteral("libera"),
        mustParse(":irc 401 omairc lena :No such nick/channel"));
    QCOMPARE(missing.label(), QStringLiteral("401"));
    QCOMPARE(missing.text(), QStringLiteral("No such nick: lena"));
    QCOMPARE(missing.severity(), IrcLogSeverity::Alert);
    QVERIFY(missing.whoisLine());
    QCOMPARE(missing.whoisLine()->progress(), IrcWhoisLine::Progress::Failed);

    const IrcStatusEntry welcome = IrcStatusEntry::incoming(
        QStringLiteral("libera"),
        mustParse(":server 001 omairc :Welcome"));
    QCOMPARE(welcome.label(), QStringLiteral("001"));
    QCOMPARE(welcome.text(), QStringLiteral("Welcome"));
}

void SessionTest::incomingNoticeStatusLinesWrapSpeaker()
{
    const IrcStatusEntry nickserv = IrcStatusEntry::incoming(
        QStringLiteral("libera"),
        mustParse(":NickServ!NickServ@services NOTICE omairc :Please identify"));
    QCOMPARE(nickserv.label(), QStringLiteral("NOTICE"));
    QCOMPARE(nickserv.text(), QStringLiteral("-NickServ- Please identify"));

    const IrcStatusEntry auth = IrcStatusEntry::incoming(
        QStringLiteral("libera"),
        mustParse("NOTICE AUTH :*** Looking up your hostname..."));
    QCOMPARE(auth.label(), QStringLiteral("NOTICE"));
    QCOMPARE(auth.text(), QStringLiteral("-AUTH- *** Looking up your hostname..."));

    const IrcStatusEntry server = IrcStatusEntry::incoming(
        QStringLiteral("libera"),
        mustParse(":copper.libera.chat NOTICE * :*** Found your hostname"));
    QCOMPARE(server.label(), QStringLiteral("NOTICE"));
    QCOMPARE(server.text(),
             QStringLiteral("-copper.libera.chat- *** Found your hostname"));

    const IrcStatusEntry channel = IrcStatusEntry::incoming(
        QStringLiteral("libera"),
        mustParse(":alice!u@h NOTICE #omarchy :heads up"));
    QCOMPARE(channel.label(), QStringLiteral("NOTICE"));
    QCOMPARE(channel.text(), QStringLiteral("-alice- heads up"));

    const IrcStatusEntry bare = IrcStatusEntry::incoming(
        QStringLiteral("libera"),
        mustParse("NOTICE * :hello"));
    QCOMPARE(bare.label(), QStringLiteral("NOTICE"));
    QCOMPARE(bare.text(), QStringLiteral("hello"));
    QVERIFY(!bare.text().startsWith(QStringLiteral("-- ")));

    const IrcStatusEntry oneParam = IrcStatusEntry::incoming(
        QStringLiteral("libera"),
        mustParse("NOTICE AUTH"));
    QCOMPARE(oneParam.label(), QStringLiteral("NOTICE"));
    QCOMPARE(oneParam.text(), QStringLiteral("-AUTH- "));
    QVERIFY(oneParam.text() != QStringLiteral("-AUTH- AUTH"));
}

void SessionTest::incomingNoticeDoesNotTranslateToEvents()
{
    const IrcServerFeatures features;
    QVERIFY(IrcEventTranslator::translate(
                QStringLiteral("libera"),
                QStringLiteral("omairc"),
                features,
                mustParse(":NickServ!NickServ@services NOTICE omairc :Please identify"))
                .empty());
    QVERIFY(IrcEventTranslator::translate(
                QStringLiteral("libera"),
                QStringLiteral("omairc"),
                features,
                mustParse(":alice!u@h NOTICE #omarchy :heads up"))
                .empty());
}

void SessionTest::incomingNickservPrivmsgDoesNotTranslateToEvents()
{
    const IrcServerFeatures features;
    QVERIFY(IrcEventTranslator::translate(
                QStringLiteral("libera"),
                QStringLiteral("omairc"),
                features,
                mustParse(":NickServ!NickServ@services PRIVMSG omairc "
                          ":This nickname is registered."))
                .empty());
    QVERIFY(IrcEventTranslator::translate(
                QStringLiteral("libera"),
                QStringLiteral("omairc"),
                features,
                mustParse(":ChanServ!ChanServ@services.libera.chat PRIVMSG omairc "
                          ":[#omarchy] You are not on that channel."))
                .empty());
}

void SessionTest::incomingStandardRepliesShowDescriptionOnStatus()
{
    const IrcStatusEntry fail = IrcStatusEntry::incoming(
        QStringLiteral("libera"),
        mustParse("FAIL * NEED_REGISTRATION :You need to be registered to continue"));
    QCOMPARE(fail.label(), QStringLiteral("FAIL"));
    QCOMPARE(fail.text(), QStringLiteral("You need to be registered to continue"));
    QCOMPARE(fail.severity(), IrcLogSeverity::Alert);
    QCOMPARE(fail.source(), IrcLogSource::Server);

    const IrcStatusEntry failWithContext = IrcStatusEntry::incoming(
        QStringLiteral("libera"),
        mustParse("FAIL ACC REG_INVALID_CALLBACK REGISTER :Email address is not valid"));
    QCOMPARE(failWithContext.label(), QStringLiteral("FAIL"));
    QCOMPARE(failWithContext.text(), QStringLiteral("Email address is not valid"));
    QCOMPARE(failWithContext.severity(), IrcLogSeverity::Alert);

    const IrcStatusEntry warn = IrcStatusEntry::incoming(
        QStringLiteral("libera"),
        mustParse("WARN REHASH CERTS_EXPIRED :Certificate has expired"));
    QCOMPARE(warn.label(), QStringLiteral("WARN"));
    QCOMPARE(warn.text(), QStringLiteral("Certificate has expired"));
    QCOMPARE(warn.severity(), IrcLogSeverity::Info);

    const IrcStatusEntry note = IrcStatusEntry::incoming(
        QStringLiteral("libera"),
        mustParse("NOTE * OPER_MESSAGE :Registering new accounts has been disabled"));
    QCOMPARE(note.label(), QStringLiteral("NOTE"));
    QCOMPARE(note.text(), QStringLiteral("Registering new accounts has been disabled"));
    QCOMPARE(note.severity(), IrcLogSeverity::Info);
}

void SessionTest::incomingStandardRepliesDoNotTranslateToEvents()
{
    const IrcServerFeatures features;
    QVERIFY(IrcEventTranslator::translate(
                QStringLiteral("libera"),
                QStringLiteral("omairc"),
                features,
                mustParse("FAIL * NEED_REGISTRATION :You need to be registered to continue"))
                .empty());
    QVERIFY(IrcEventTranslator::translate(
                QStringLiteral("libera"),
                QStringLiteral("omairc"),
                features,
                mustParse("WARN REHASH CERTS_EXPIRED :Certificate has expired"))
                .empty());
    QVERIFY(IrcEventTranslator::translate(
                QStringLiteral("libera"),
                QStringLiteral("omairc"),
                features,
                mustParse("NOTE * OPER_MESSAGE :Registering new accounts has been disabled"))
                .empty());
}

void SessionTest::inboundFailDoesNotFailTheSession()
{
    Fixture fixture;
    StatusCollector status(fixture.session);
    QSignalSpy errors(fixture.session, &IrcSession::errorOccurred);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);
    QCOMPARE(errors.size(), 0);

    fixture.transport->injectBytes(
        QByteArrayLiteral(
            "FAIL JOIN ACCOUNT_REQUIRED #omarchy :You must be logged in\r\n"));

    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);
    QCOMPARE(fixture.transport->connectionState(),
             IrcTransport::ConnectionState::Encrypted);
    QCOMPARE(errors.size(), 0);

    bool sawFail = false;
    for (const IrcStatusEntry& entry : status.entries) {
        if (entry.label() != QStringLiteral("FAIL"))
            continue;
        sawFail = true;
        QCOMPARE(entry.text(), QStringLiteral("You must be logged in"));
        QCOMPARE(entry.severity(), IrcLogSeverity::Alert);
    }
    QVERIFY(sawFail);
}

void SessionTest::incomingActionTranslatesToActionEvent()
{
    const IrcServerFeatures features;
    const std::vector<IrcEvent> events = IrcEventTranslator::translate(
        QStringLiteral("libera"),
        QStringLiteral("omairc"),
        features,
        mustParse(":MetaNova!u@h PRIVMSG #omarchy :\x01"
                  "ACTION feeds jvaztap\x01"));
    QCOMPARE(events.size(), std::size_t(1));
    const auto *action = std::get_if<IrcActionEvent>(&events.front());
    QVERIFY(action);
    QCOMPARE(action->author, QStringLiteral("MetaNova"));
    QCOMPARE(action->body, QStringLiteral("feeds jvaztap"));
    QVERIFY(!action->body.contains(QChar(1)));
    QVERIFY(!action->body.contains(QStringLiteral("ACTION")));
}

void SessionTest::latin1PrivmsgBodyIsEAcuteAndNextLineTranslates()
{
    const IrcServerFeatures features;
    std::string latin1 = ":alice!u@h PRIVMSG #omarchy :";
    latin1.push_back('\xe9');
    const std::vector<IrcEvent> first = IrcEventTranslator::translate(
        QStringLiteral("libera"),
        QStringLiteral("omairc"),
        features,
        mustParse(latin1));
    QCOMPARE(first.size(), std::size_t(1));
    const auto *latin1Message = std::get_if<IrcMessageEvent>(&first.front());
    QVERIFY(latin1Message);
    QCOMPARE(latin1Message->body, QString(QChar(0x00E9)));
    QVERIFY(!latin1Message->body.contains(QChar(0xFFFD)));

    const std::vector<IrcEvent> utf8 = IrcEventTranslator::translate(
        QStringLiteral("libera"),
        QStringLiteral("omairc"),
        features,
        mustParse(":alice!u@h PRIVMSG #omarchy :\xc3\xa9"));
    QCOMPARE(utf8.size(), std::size_t(1));
    const auto *utf8Message = std::get_if<IrcMessageEvent>(&utf8.front());
    QVERIFY(utf8Message);
    QCOMPARE(utf8Message->body, QString(QChar(0x00E9)));

    const std::vector<IrcEvent> following = IrcEventTranslator::translate(
        QStringLiteral("libera"),
        QStringLiteral("omairc"),
        features,
        mustParse(":bob!u@h PRIVMSG #omarchy :ok"));
    QCOMPARE(following.size(), std::size_t(1));
    const auto *followingMessage = std::get_if<IrcMessageEvent>(&following.front());
    QVERIFY(followingMessage);
    QCOMPARE(followingMessage->body, QStringLiteral("ok"));
}

void SessionTest::incomingCtcpRequestsAreNotConversationEvents()
{
    const IrcServerFeatures features;
    const std::vector<IrcEvent> events = IrcEventTranslator::translate(
        QStringLiteral("network-a"),
        QStringLiteral("omairc"),
        features,
        mustParse(":MetaNova!u@h PRIVMSG omairc :\x01VERSION\x01"));
    QVERIFY(events.empty());
}

void SessionTest::answersCtcpRequests()
{
    Fixture fixture;
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server 001 omairc :Welcome\r\n"));

    fixture.transport->injectBytes(
        QByteArrayLiteral(":MetaNova!u@h PRIVMSG omairc :\x01PING token\x01\r\n"
                          ":alice!u@h PRIVMSG omairc :\x01TIME\x01\r\n"
                          ":bob!u@h PRIVMSG omairc :\x01VERSION\x01\r\n"));

    QVERIFY(fixture.wrote(QByteArrayLiteral(
        "NOTICE MetaNova :\x01PING token\x01\r\n")));
    QVERIFY(fixture.wrote(ctcpVersionReply(QByteArrayLiteral("bob"))));
    bool wroteTime = false;
    for (const QByteArray &frame : fixture.transport->writtenFrames()) {
        if (frame.startsWith(QByteArrayLiteral("NOTICE alice :\x01TIME ")))
            wroteTime = true;
    }
    QVERIFY(wroteTime);
}

void SessionTest::rateLimitsCtcpVersionRepliesPerNick()
{
    Fixture fixture;
    StatusCollector status(fixture.session);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server 001 omairc :Welcome\r\n"));

    const QByteArray probe =
        QByteArrayLiteral(":MetaNova!u@h PRIVMSG omairc :\x01VERSION\x01\r\n");
    const QByteArray reply = ctcpVersionReply(QByteArrayLiteral("MetaNova"));
    const QByteArray otherProbe =
        QByteArrayLiteral(":alice!u@h PRIVMSG omairc :\x01VERSION\x01\r\n");
    const QByteArray otherReply = ctcpVersionReply(QByteArrayLiteral("alice"));

    fixture.transport->injectBytes(probe + probe);
    QCOMPARE(fixture.transport->writtenFrames().count(reply), 1);

    fixture.transport->injectBytes(otherProbe);
    QCOMPARE(fixture.transport->writtenFrames().count(otherReply), 1);

    int ctcpEntries = 0;
    for (const IrcStatusEntry &entry : status.entries) {
        if (entry.label() == QStringLiteral("CTCP"))
            ++ctcpEntries;
    }
    QCOMPARE(ctcpEntries, 3);

    QTest::qWait(5500);
    fixture.transport->injectBytes(probe);
    QCOMPARE(fixture.transport->writtenFrames().count(reply), 2);
    ctcpEntries = 0;
    for (const IrcStatusEntry &entry : status.entries) {
        if (entry.label() == QStringLiteral("CTCP"))
            ++ctcpEntries;
    }
    QCOMPARE(ctcpEntries, 4);
}

void SessionTest::dropsOversizedCtcpPingPayload()
{
    Fixture fixture;
    StatusCollector status(fixture.session);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server 001 omairc :Welcome\r\n"));

    const QByteArray tooLong(33, 'x');
    const QByteArray exact(32, 'y');
    fixture.transport->injectBytes(
        QByteArrayLiteral(":MetaNova!u@h PRIVMSG omairc :\x01PING ")
        + tooLong
        + QByteArrayLiteral("\x01\r\n"));

    QVERIFY(!fixture.wrote(
        QByteArrayLiteral("NOTICE MetaNova :\x01PING ") + tooLong
        + QByteArrayLiteral("\x01\r\n")));
    QVERIFY(!fixture.wrote(
        QByteArrayLiteral("NOTICE MetaNova :\x01PING ") + QByteArray(32, 'x')
        + QByteArrayLiteral("\x01\r\n")));
    QVERIFY(status.hasLabel(QStringLiteral("CTCP")));
    QVERIFY(status.anyFieldContains(QStringLiteral("PING")));

    fixture.transport->injectBytes(
        QByteArrayLiteral(":MetaNova!u@h PRIVMSG omairc :\x01PING ")
        + exact
        + QByteArrayLiteral("\x01\r\n"));
    QVERIFY(fixture.wrote(
        QByteArrayLiteral("NOTICE MetaNova :\x01PING ") + exact
        + QByteArrayLiteral("\x01\r\n")));
}

void SessionTest::doesNotAnswerChannelCtcpRequests()
{
    Fixture fixture;
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server 001 omairc :Welcome\r\n"));
    const int before = fixture.transport->writtenFrames().size();

    fixture.transport->injectBytes(
        QByteArrayLiteral(":MetaNova!u@h PRIVMSG #omarchy :\x01PING token\x01\r\n"));

    QCOMPARE(fixture.transport->writtenFrames().size(), before);
}

void SessionTest::welcomeAssignsNickFrom001()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.nick = QStringLiteral("omairc-very-long-name");
    Fixture fixture(sessionConfig);
    QCOMPARE(fixture.session->nick(), QStringLiteral("omairc-very-long-name"));

    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc-very-long-name LS :multi-prefix\r\n"
                          ":server 001 omairc-truncated :Welcome\r\n"));
    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);
    QCOMPARE(fixture.session->nick(), QStringLiteral("omairc-truncated"));
}

void SessionTest::emptyWelcomeKeepsConfigNick()
{
    Fixture fixture;
    QCOMPARE(fixture.session->nick(), QStringLiteral("omairc"));

    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001\r\n"));
    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);
    QCOMPARE(fixture.session->nick(), QStringLiteral("omairc"));
}

void SessionTest::selfNickUpdatesSessionNick()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.nick = QStringLiteral("omairc-very-long-name");
    Fixture fixture(sessionConfig);

    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc-very-long-name LS :multi-prefix\r\n"
                          ":server 001 omairc-truncated :Welcome\r\n"
                          ":Alice!u@h NICK :Alicia\r\n"));
    QCOMPARE(fixture.session->nick(), QStringLiteral("omairc-truncated"));

    fixture.transport->injectBytes(
        QByteArrayLiteral(":omairc-truncated!u@h NICK :fred\r\n"));
    QCOMPARE(fixture.session->nick(), QStringLiteral("fred"));
}

void SessionTest::batchOpenAndCloseDoNotEmitBatchLines()
{
    Fixture fixture;
    StatusCollector status(fixture.session);
    QStringList commands;
    QObject::connect(fixture.session, &IrcSession::messageReceived, fixture.session,
                     [&](const QString&, const IrcMessage& message) {
        commands.append(QString::fromStdString(message.command));
    });
    fixture.registerWithWelcome();

    fixture.transport->injectBytes(
        QByteArrayLiteral(
            ":irc.host BATCH +ns netsplit irc.example irc.other\r\n"
            "@batch=ns :alice!u@h PRIVMSG #omarchy :still here\r\n"
            ":irc.host BATCH -ns\r\n"));

    QCOMPARE(commands, QStringList{QStringLiteral("PRIVMSG")});
    QVERIFY(status.hasLabel(QStringLiteral("BATCH")));
    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);
}

void SessionTest::nestedBatchesDoNotFailTheSession()
{
    Fixture fixture;
    QSignalSpy errors(fixture.session, &IrcSession::errorOccurred);
    QStringList commands;
    QObject::connect(fixture.session, &IrcSession::messageReceived, fixture.session,
                     [&](const QString&, const IrcMessage& message) {
        commands.append(QString::fromStdString(message.command));
    });
    fixture.registerWithWelcome();

    fixture.transport->injectBytes(
        QByteArrayLiteral(
            ":irc.host BATCH +outer netsplit irc.a irc.b\r\n"
            "@batch=outer :irc.host BATCH +inner netjoin irc.a irc.b\r\n"
            "@batch=inner :alice!u@h PRIVMSG #omarchy :Hi\r\n"
            "@batch=outer :irc.host BATCH -inner\r\n"
            ":irc.host BATCH -outer\r\n"));

    QCOMPARE(errors.size(), 0);
    QCOMPARE(commands, QStringList{QStringLiteral("PRIVMSG")});
    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);
}

void SessionTest::unknownBatchTypeAndCloseStayRegistered()
{
    Fixture fixture;
    QSignalSpy errors(fixture.session, &IrcSession::errorOccurred);
    QStringList commands;
    QObject::connect(fixture.session, &IrcSession::messageReceived, fixture.session,
                     [&](const QString&, const IrcMessage& message) {
        commands.append(QString::fromStdString(message.command));
    });
    fixture.registerWithWelcome();

    fixture.transport->injectBytes(
        QByteArrayLiteral(
            ":irc.host BATCH +x unknown.example/foo\r\n"
            "@batch=x :alice!u@h PRIVMSG #omarchy :delivered\r\n"
            ":irc.host BATCH -x\r\n"
            ":irc.host BATCH -missing\r\n"
            ":bob!u@h PRIVMSG #omarchy :after\r\n"));

    QCOMPARE(errors.size(), 0);
    QCOMPARE(commands, QStringList({QStringLiteral("PRIVMSG"),
                                    QStringLiteral("PRIVMSG")}));
    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);
}

void SessionTest::selfJoinRequestsChatHistoryOnceUntilPart()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.autojoinChannels = {};
    Fixture fixture(sessionConfig);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory\r\n"
                          ":server CAP omairc ACK :batch chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);

    int writesWhenJoinEmitted = -1;
    QObject::connect(fixture.session, &IrcSession::messageReceived, fixture.session,
                     [&](const QString&, const IrcMessage& message) {
        if (message.command == "JOIN")
            writesWhenJoinEmitted = fixture.transport->writtenFrames().size();
    });
    fixture.transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    QCOMPARE(fixture.transport->writtenFrames().last(),
             QByteArrayLiteral("CHATHISTORY LATEST #omarchy * 100\r\n"));
    QVERIFY(writesWhenJoinEmitted >= 0);
    QVERIFY(!fixture.transport->writtenFrames().mid(0, writesWhenJoinEmitted)
                 .contains(QByteArrayLiteral("CHATHISTORY LATEST #omarchy * 100\r\n")));
    QVERIFY(fixture.session->historyPending());
    fixture.transport->injectBytes(
        QByteArrayLiteral(
            ":irc.host BATCH +hx chathistory #omarchy\r\n"
            "@batch=hx :alice!u@h PRIVMSG #omarchy :older\r\n"
            ":irc.host BATCH -hx\r\n"));
    QVERIFY(!fixture.session->historyPending());
    const int afterFirst = fixture.transport->writtenFrames().size();

    fixture.transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    QCOMPARE(fixture.transport->writtenFrames().size(), afterFirst);

    fixture.transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h PART :#omarchy\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    QCOMPARE(fixture.transport->writtenFrames().last(),
             QByteArrayLiteral("CHATHISTORY LATEST #omarchy * 100\r\n"));
}

void SessionTest::selfJoinWithBarePrefixRequestsChatHistory()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.autojoinChannels = {};
    Fixture fixture(sessionConfig);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory\r\n"
                          ":server CAP omairc ACK :batch chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    fixture.transport->injectBytes(
        QByteArrayLiteral(":omairc JOIN :#omarchy\r\n"));
    QCOMPARE(fixture.transport->writtenFrames().last(),
             QByteArrayLiteral("CHATHISTORY LATEST #omarchy * 100\r\n"));
}

void SessionTest::selfJoinWithoutChatHistorySendsNothing()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.autojoinChannels = {};
    Fixture fixture(sessionConfig);
    fixture.registerWithWelcome();
    const int before = fixture.transport->writtenFrames().size();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    QCOMPARE(fixture.transport->writtenFrames().size(), before);
    QVERIFY(!fixture.wrote(QByteArrayLiteral("CHATHISTORY LATEST #omarchy * 100\r\n")));
}

void SessionTest::selfJoinWithoutBatchDoesNotRequestChatHistory()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.autojoinChannels = {};
    Fixture fixture(sessionConfig);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory\r\n"
                          ":server CAP omairc ACK :batch chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    QVERIFY(fixture.session->capabilities().contains(IrcCapability::ChatHistory));
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc DEL :batch\r\n"));
    const int before = fixture.transport->writtenFrames().size();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    QCOMPARE(fixture.transport->writtenFrames().size(), before);
    QVERIFY(!fixture.wrote(QByteArrayLiteral("CHATHISTORY LATEST #omarchy * 100\r\n")));
}

void SessionTest::deletingStableChatHistoryRequestsDraftToken()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.autojoinChannels = {};
    Fixture fixture(sessionConfig);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory draft/chathistory\r\n"
                          ":server CAP omairc ACK :batch chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    QVERIFY(fixture.session->capabilities().contains(IrcCapability::ChatHistory));
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc DEL :chathistory\r\n"));
    QVERIFY(fixture.wrote(QByteArrayLiteral("CAP REQ :draft/chathistory\r\n")));
}

void SessionTest::historyBatchSwallowsInnerPrivmsg()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.autojoinChannels = {};
    Fixture fixture(sessionConfig);
    StatusCollector status(fixture.session);
    QStringList commands;
    QList<IrcHistoryBatch> batches;
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory\r\n"
                          ":server CAP omairc ACK :batch chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    QObject::connect(fixture.session, &IrcSession::messageReceived, fixture.session,
                     [&](const QString&, const IrcMessage& message) {
        commands.append(QString::fromStdString(message.command));
    });
    QObject::connect(fixture.session, &IrcSession::historyBatchReceived, fixture.session,
                     [&](const QString&, const IrcHistoryBatch& batch) {
        batches.append(batch);
    });

    fixture.transport->injectBytes(
        QByteArrayLiteral(
            ":irc.host BATCH +hx chathistory #omarchy\r\n"
            "@batch=hx :alice!u@h PRIVMSG #omarchy :older\r\n"
            ":irc.host BATCH -hx\r\n"));

    QVERIFY(commands.isEmpty());
    QCOMPARE(batches.size(), 1);
    QCOMPARE(batches.front().target, QStringLiteral("#omarchy"));
    QCOMPARE(int(batches.front().lines.size()), 1);
    QCOMPARE(QString::fromStdString(batches.front().lines.front().command),
             QStringLiteral("PRIVMSG"));
    QVERIFY(status.hasLabel(QStringLiteral("BATCH")));
    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);

    fixture.transport->injectBytes(
        QByteArrayLiteral(
            ":irc.host BATCH +empty chathistory #omarchy\r\n"
            ":irc.host BATCH -empty\r\n"));
    QCOMPARE(batches.size(), 2);
    QVERIFY(batches.back().lines.empty());
    QCOMPARE(batches.back().target, QStringLiteral("#omarchy"));
}

void SessionTest::draftHistoryBatchSwallowsInnerPrivmsg()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.autojoinChannels = {};
    Fixture fixture(sessionConfig);
    QStringList commands;
    QList<IrcHistoryBatch> batches;
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch draft/chathistory\r\n"
                          ":server CAP omairc ACK :batch draft/chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    QObject::connect(fixture.session, &IrcSession::messageReceived, fixture.session,
                     [&](const QString&, const IrcMessage& message) {
        commands.append(QString::fromStdString(message.command));
    });
    QObject::connect(fixture.session, &IrcSession::historyBatchReceived, fixture.session,
                     [&](const QString&, const IrcHistoryBatch& batch) {
        batches.append(batch);
    });

    fixture.transport->injectBytes(
        QByteArrayLiteral(
            ":irc.host BATCH +hx draft/chathistory #omarchy\r\n"
            "@batch=hx :alice!u@h PRIVMSG #omarchy :older\r\n"
            ":irc.host BATCH -hx\r\n"));

    QVERIFY(commands.isEmpty());
    QCOMPARE(batches.size(), 1);
    QCOMPARE(batches.front().target, QStringLiteral("#omarchy"));
}

void SessionTest::leftoverHistoryBatchAfterPartDoesNotEmit()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.autojoinChannels = {};
    Fixture fixture(sessionConfig);
    QList<IrcHistoryBatch> batches;
    QObject::connect(fixture.session, &IrcSession::historyBatchReceived, fixture.session,
                     [&](const QString&, const IrcHistoryBatch& batch) {
        batches.append(batch);
    });
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory\r\n"
                          ":server CAP omairc ACK :batch chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":irc.host BATCH +stale chathistory #omarchy\r\n"
                          "@batch=stale :alice!u@h PRIVMSG #omarchy :old\r\n"
                          ":omairc!u@h PART :#omarchy\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":irc.host BATCH -stale\r\n"));
    QVERIFY(batches.isEmpty());
}

void SessionTest::delayedHistoryBatchAfterPartDoesNotEmit()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.autojoinChannels = {};
    Fixture fixture(sessionConfig);
    QList<IrcHistoryBatch> batches;
    QObject::connect(fixture.session, &IrcSession::historyBatchReceived, fixture.session,
                     [&](const QString&, const IrcHistoryBatch& batch) {
        batches.append(batch);
    });
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory\r\n"
                          ":server CAP omairc ACK :batch chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":omairc!u@h PART :#omarchy\r\n"
                          ":irc.host BATCH +stale chathistory #omarchy\r\n"
                          "@batch=stale :alice!u@h PRIVMSG #omarchy :old\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":irc.host BATCH -stale\r\n"));
    QVERIFY(batches.isEmpty());
    QVERIFY(fixture.session->historyPending());

    fixture.transport->injectBytes(
        QByteArrayLiteral(
            ":irc.host BATCH +hx chathistory #omarchy\r\n"
            "@batch=hx :alice!u@h PRIVMSG #omarchy :fresh\r\n"
            ":irc.host BATCH -hx\r\n"));
    QCOMPARE(batches.size(), 1);
    QCOMPARE(batches.front().target, QStringLiteral("#omarchy"));
    QCOMPARE(int(batches.front().lines.size()), 1);
    QVERIFY(!fixture.session->historyPending());
}

void SessionTest::unsolicitedHistoryFloodKeepsLiveTraffic()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.autojoinChannels = {};
    Fixture fixture(sessionConfig);
    QStringList bodies;
    QList<IrcHistoryBatch> batches;
    QObject::connect(fixture.session, &IrcSession::messageReceived, fixture.session,
                     [&](const QString&, const IrcMessage& message) {
        if (message.command == "PRIVMSG" && !message.parameters.empty())
            bodies.append(QString::fromStdString(message.parameters.back()));
    });
    QObject::connect(fixture.session, &IrcSession::historyBatchReceived, fixture.session,
                     [&](const QString&, const IrcHistoryBatch& batch) {
        batches.append(batch);
    });
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory\r\n"
                          ":server CAP omairc ACK :batch chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"));

    QByteArray fill;
    for (int index = 0; index < 16; ++index) {
        fill += QByteArrayLiteral(":irc.host BATCH +d")
            + QByteArray::number(index)
            + QByteArrayLiteral(" unknown.example/foo\r\n");
    }
    for (int index = 0; index < 40; ++index) {
        fill += QByteArrayLiteral(":irc.host BATCH +x")
            + QByteArray::number(index)
            + QByteArrayLiteral(" chathistory #omarchy\r\n");
    }
    // x0 fits in the ignore ledger, x39 does not, and hx arrives once the
    // ledger is already full. Only the reference we recorded may be swallowed.
    fill += QByteArrayLiteral(
        ":irc.host BATCH +hx chathistory #omarchy\r\n"
        "@batch=x0 :alice!u@h PRIVMSG #omarchy :recorded\r\n"
        "@batch=x39 :alice!u@h PRIVMSG #omarchy :unrecorded\r\n"
        "@batch=hx :alice!u@h PRIVMSG #omarchy :overflow\r\n"
        ":irc.host BATCH -hx\r\n"
        ":bob!u@h PRIVMSG #omarchy :after\r\n");
    fixture.transport->injectBytes(fill);

    QCOMPARE(bodies,
             QStringList({QStringLiteral("unrecorded"), QStringLiteral("overflow"),
                          QStringLiteral("after")}));
    QVERIFY(batches.isEmpty());
}

void SessionTest::overflowedUnknownBatchPassesInnerPrivmsg()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.autojoinChannels = {};
    Fixture fixture(sessionConfig);
    QStringList bodies;
    QList<IrcHistoryBatch> batches;
    QObject::connect(fixture.session, &IrcSession::messageReceived, fixture.session,
                     [&](const QString&, const IrcMessage& message) {
        if (message.command == "PRIVMSG" && !message.parameters.empty())
            bodies.append(QString::fromStdString(message.parameters.back()));
    });
    QObject::connect(fixture.session, &IrcSession::historyBatchReceived, fixture.session,
                     [&](const QString&, const IrcHistoryBatch& batch) {
        batches.append(batch);
    });
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory\r\n"
                          ":server CAP omairc ACK :batch chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"));

    QByteArray fill;
    for (int index = 0; index < 16; ++index) {
        fill += QByteArrayLiteral(":irc.host BATCH +d")
            + QByteArray::number(index)
            + QByteArrayLiteral(" unknown.example/foo\r\n");
    }
    fill += QByteArrayLiteral(
        ":irc.host BATCH +extra unknown.example/foo\r\n"
        "@batch=extra :alice!u@h PRIVMSG #omarchy :passed\r\n"
        ":irc.host BATCH +hx chathistory #omarchy\r\n"
        "@batch=hx :alice!u@h PRIVMSG #omarchy :overflow\r\n"
        ":bob!u@h PRIVMSG #omarchy :after\r\n");
    fixture.transport->injectBytes(fill);

    QCOMPARE(bodies, QStringList({QStringLiteral("passed"),
                                  QStringLiteral("after")}));
    QVERIFY(batches.isEmpty());
}

void SessionTest::selfJoinWithoutTargetDoesNotRequestChatHistory()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.autojoinChannels = {};
    Fixture fixture(sessionConfig);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory\r\n"
                          ":server CAP omairc ACK :batch chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    const int before = fixture.transport->writtenFrames().size();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN\r\n"));
    QCOMPARE(fixture.transport->writtenFrames().size(), before);
    QVERIFY(!fixture.wrote(QByteArrayLiteral("CHATHISTORY LATEST  * 100\r\n")));
    QVERIFY(!fixture.session->historyPending());
}

void SessionTest::rfc1459EqualPartClearsChatHistoryAsk()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.autojoinChannels = {};
    Fixture fixture(sessionConfig);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory\r\n"
                          ":server CAP omairc ACK :batch chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    fixture.transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#[room]\r\n"));
    QCOMPARE(fixture.transport->writtenFrames().last(),
             QByteArrayLiteral("CHATHISTORY LATEST #[room] * 100\r\n"));
    const int afterJoin = fixture.transport->writtenFrames().size();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h PART :#{room}\r\n"
                          ":omairc!u@h JOIN :#[room]\r\n"));
    QVERIFY(fixture.transport->writtenFrames().size() > afterJoin);
    QCOMPARE(fixture.transport->writtenFrames().last(),
             QByteArrayLiteral("CHATHISTORY LATEST #[room] * 100\r\n"));
}

void SessionTest::nakOfStableChatHistoryRequestsDraftToken()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.autojoinChannels = {};
    Fixture fixture(sessionConfig);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory draft/chathistory\r\n"));
    QVERIFY(fixture.wrote(QByteArrayLiteral("CAP REQ :batch chathistory\r\n")));
    QVERIFY(!fixture.wrote(QByteArrayLiteral("CAP REQ :draft/chathistory\r\n")));
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc ACK :batch\r\n"
                          ":server CAP omairc NAK :chathistory\r\n"));
    QVERIFY(fixture.wrote(QByteArrayLiteral("CAP REQ :draft/chathistory\r\n")));
}

void SessionTest::rfc1459EqualSelfJoinRequestsChatHistory()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.autojoinChannels = {};
    sessionConfig.nick = QStringLiteral("[omairc");
    Fixture fixture(sessionConfig);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP [omairc LS :batch chathistory\r\n"
                          ":server CAP [omairc ACK :batch chathistory\r\n"
                          ":server 001 [omairc :Welcome\r\n"));
    fixture.transport->injectBytes(
        QByteArrayLiteral(":{omairc!u@h JOIN :#omarchy\r\n"));
    QCOMPARE(fixture.transport->writtenFrames().last(),
             QByteArrayLiteral("CHATHISTORY LATEST #omarchy * 100\r\n"));
}

void SessionTest::leftoverTaggedHistoryLineAfterPartIsSwallowed()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.autojoinChannels = {};
    Fixture fixture(sessionConfig);
    QStringList privmsgs;
    QList<IrcHistoryBatch> batches;
    QObject::connect(fixture.session, &IrcSession::messageReceived, fixture.session,
                     [&](const QString&, const IrcMessage& message) {
        if (message.command == "PRIVMSG" && !message.parameters.empty())
            privmsgs.append(QString::fromStdString(message.parameters.back()));
    });
    QObject::connect(fixture.session, &IrcSession::historyBatchReceived, fixture.session,
                     [&](const QString&, const IrcHistoryBatch& batch) {
        batches.append(batch);
    });
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory\r\n"
                          ":server CAP omairc ACK :batch chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":irc.host BATCH +stale chathistory #omarchy\r\n"
                          "@batch=stale :alice!u@h PRIVMSG #omarchy :old\r\n"
                          ":omairc!u@h PART :#omarchy\r\n"
                          "@batch=stale :alice!u@h PRIVMSG #omarchy :late\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":irc.host BATCH -stale\r\n"));
    QVERIFY(privmsgs.isEmpty());
    QVERIFY(batches.isEmpty());
}

void SessionTest::leftoverNestedBatchLineAfterRootCloseIsSwallowed()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.autojoinChannels = {};
    Fixture fixture(sessionConfig);
    QStringList privmsgs;
    QList<IrcHistoryBatch> batches;
    QObject::connect(fixture.session, &IrcSession::messageReceived, fixture.session,
                     [&](const QString&, const IrcMessage& message) {
        if (message.command == "PRIVMSG" && !message.parameters.empty())
            privmsgs.append(QString::fromStdString(message.parameters.back()));
    });
    QObject::connect(fixture.session, &IrcSession::historyBatchReceived, fixture.session,
                     [&](const QString&, const IrcHistoryBatch& batch) {
        batches.append(batch);
    });
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory\r\n"
                          ":server CAP omairc ACK :batch chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":irc.host BATCH +stale chathistory #omarchy\r\n"
                          "@batch=stale :irc.host BATCH +child chathistory #omarchy\r\n"
                          "@batch=child :alice!u@h PRIVMSG #omarchy :old\r\n"
                          ":irc.host BATCH -stale\r\n"
                          "@batch=child :alice!u@h PRIVMSG #omarchy :late\r\n"
                          ":irc.host BATCH -child\r\n"
                          ":bob!u@h PRIVMSG #omarchy :after\r\n"));
    QCOMPARE(privmsgs, QStringList({QStringLiteral("after")}));
    QCOMPARE(batches.size(), 1);
    QCOMPARE(int(batches.front().lines.size()), 1);
}

void SessionTest::ignoredBatchLedgerRecoversAfterClose()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.autojoinChannels = {};
    Fixture fixture(sessionConfig);
    QStringList bodies;
    QList<IrcHistoryBatch> batches;
    QObject::connect(fixture.session, &IrcSession::messageReceived, fixture.session,
                     [&](const QString&, const IrcMessage& message) {
        if (message.command == "PRIVMSG" && !message.parameters.empty())
            bodies.append(QString::fromStdString(message.parameters.back()));
    });
    QObject::connect(fixture.session, &IrcSession::historyBatchReceived, fixture.session,
                     [&](const QString&, const IrcHistoryBatch& batch) {
        batches.append(batch);
    });
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory\r\n"
                          ":server CAP omairc ACK :batch chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"));

    QByteArray fill;
    for (int index = 0; index < 16; ++index) {
        fill += QByteArrayLiteral(":irc.host BATCH +d")
            + QByteArray::number(index)
            + QByteArrayLiteral(" unknown.example/foo\r\n");
    }
    for (int index = 0; index < 32; ++index) {
        fill += QByteArrayLiteral(":irc.host BATCH +x")
            + QByteArray::number(index)
            + QByteArrayLiteral(" chathistory #omarchy\r\n");
    }
    // x0 through x31 fill the ignore ledger. Closing one releases its slot, so
    // hx is recorded and its replay line is still kept out of the transcript.
    fill += QByteArrayLiteral(
        ":irc.host BATCH -x0\r\n"
        ":irc.host BATCH +hx chathistory #omarchy\r\n"
        "@batch=hx :alice!u@h PRIVMSG #omarchy :overflow\r\n"
        ":bob!u@h PRIVMSG #omarchy :after\r\n");
    fixture.transport->injectBytes(fill);

    QCOMPARE(bodies, QStringList({QStringLiteral("after")}));
    QVERIFY(batches.isEmpty());
}

void SessionTest::chatHistoryFailClearsPending()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.autojoinChannels = {};
    Fixture fixture(sessionConfig);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory\r\n"
                          ":server CAP omairc ACK :batch chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    QVERIFY(fixture.session->historyPending());
    fixture.transport->injectBytes(QByteArrayLiteral(
        ":server FAIL CHATHISTORY INVALID_TARGET LATEST #omarchy :no\r\n"));
    QVERIFY(!fixture.session->historyPending());
}

void SessionTest::chatHistoryMessageErrorClearsPending()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.autojoinChannels = {};
    Fixture fixture(sessionConfig);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory\r\n"
                          ":server CAP omairc ACK :batch chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    QVERIFY(fixture.session->historyPending());
    fixture.transport->injectBytes(QByteArrayLiteral(
        ":server FAIL CHATHISTORY MESSAGE_ERROR LATEST #omarchy :bad\r\n"));
    QVERIFY(!fixture.session->historyPending());
}

void SessionTest::chatHistoryFailForOtherChannelKeepsPending()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.autojoinChannels = {};
    Fixture fixture(sessionConfig);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory\r\n"
                          ":server CAP omairc ACK :batch chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    QVERIFY(fixture.session->historyPending());
    fixture.transport->injectBytes(QByteArrayLiteral(
        ":server FAIL CHATHISTORY INVALID_TARGET LATEST #desktop :no\r\n"));
    QVERIFY(fixture.session->historyPending());
}

void SessionTest::delayedChatHistoryFailAfterRejoinKeepsPending()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.autojoinChannels = {};
    Fixture fixture(sessionConfig);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory\r\n"
                          ":server CAP omairc ACK :batch chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":omairc!u@h PART :#omarchy\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":irc.host BATCH +hx chathistory #omarchy\r\n"));
    QVERIFY(fixture.session->historyPending());
    fixture.transport->injectBytes(QByteArrayLiteral(
        ":server FAIL CHATHISTORY INVALID_TARGET LATEST #omarchy :no\r\n"));
    QVERIFY(fixture.session->historyPending());
    fixture.transport->injectBytes(
        QByteArrayLiteral(":irc.host BATCH -hx\r\n"));
    QVERIFY(!fixture.session->historyPending());
}

void SessionTest::chatHistoryFailDescriptionDoesNotClearOtherChannel()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.autojoinChannels = {};
    Fixture fixture(sessionConfig);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory\r\n"
                          ":server CAP omairc ACK :batch chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":omairc!u@h JOIN :#desktop\r\n"));
    QVERIFY(fixture.session->historyPending());
    fixture.transport->injectBytes(QByteArrayLiteral(
        ":server FAIL CHATHISTORY INVALID_TARGET LATEST #omarchy :also #desktop\r\n"));
    QVERIFY(fixture.session->historyPending());
    fixture.transport->injectBytes(
        QByteArrayLiteral(":irc.host BATCH +hx chathistory #desktop\r\n"
                          ":irc.host BATCH -hx\r\n"));
    QVERIFY(!fixture.session->historyPending());
}

void SessionTest::deletingRequiredHistoryCapClearsPending()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.autojoinChannels = {};
    Fixture fixture(sessionConfig);
    QStringList privmsgs;
    QList<IrcHistoryBatch> batches;
    QObject::connect(fixture.session, &IrcSession::messageReceived, fixture.session,
                     [&](const QString&, const IrcMessage& message) {
        if (message.command == "PRIVMSG" && !message.parameters.empty())
            privmsgs.append(QString::fromStdString(message.parameters.back()));
    });
    QObject::connect(fixture.session, &IrcSession::historyBatchReceived, fixture.session,
                     [&](const QString&, const IrcHistoryBatch& batch) {
        batches.append(batch);
    });
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory\r\n"
                          ":server CAP omairc ACK :batch chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":irc.host BATCH +hx chathistory #omarchy\r\n"
                          "@batch=hx :alice!u@h PRIVMSG #omarchy :old\r\n"));
    QVERIFY(fixture.session->historyPending());
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc DEL :batch\r\n"
                          "@batch=hx :alice!u@h PRIVMSG #omarchy :late\r\n"
                          ":irc.host BATCH -hx\r\n"
                          ":bob!u@h PRIVMSG #omarchy :after\r\n"));
    QVERIFY(!fixture.session->historyPending());
    QCOMPARE(privmsgs, QStringList({QStringLiteral("after")}));
    QVERIFY(batches.isEmpty());
}

void SessionTest::deletingUnusedDraftChatHistoryKeepsPending()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.autojoinChannels = {};
    Fixture fixture(sessionConfig);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory draft/chathistory\r\n"
                          ":server CAP omairc ACK :batch chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    QVERIFY(fixture.session->historyPending());
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc DEL :draft/chathistory\r\n"));
    QVERIFY(fixture.session->historyPending());
}

void SessionTest::unsolicitedHistoryBatchStillEmits()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.autojoinChannels = {};
    Fixture fixture(sessionConfig);
    QList<IrcHistoryBatch> batches;
    QObject::connect(fixture.session, &IrcSession::historyBatchReceived, fixture.session,
                     [&](const QString&, const IrcHistoryBatch& batch) {
        batches.append(batch);
    });
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory\r\n"
                          ":server CAP omairc ACK :batch chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":irc.host BATCH +hx chathistory #omarchy\r\n"
                          "@batch=hx :alice!u@h PRIVMSG #omarchy :older\r\n"
                          ":irc.host BATCH -hx\r\n"));
    QCOMPARE(batches.size(), 1);
    QCOMPARE(batches.front().target, QStringLiteral("#omarchy"));
    QCOMPARE(int(batches.front().lines.size()), 1);
    QCOMPARE(QString::fromStdString(batches.front().lines.front().parameters.back()),
             QStringLiteral("older"));
}

void SessionTest::historyBatchWithoutCapabilityIsIgnored()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.autojoinChannels = {};
    Fixture fixture(sessionConfig);
    QStringList bodies;
    QList<IrcHistoryBatch> batches;
    QObject::connect(fixture.session, &IrcSession::messageReceived, fixture.session,
                     [&](const QString&, const IrcMessage& message) {
        if (message.command == "PRIVMSG" && !message.parameters.empty())
            bodies.append(QString::fromStdString(message.parameters.back()));
    });
    QObject::connect(fixture.session, &IrcSession::historyBatchReceived, fixture.session,
                     [&](const QString&, const IrcHistoryBatch& batch) {
        batches.append(batch);
    });
    fixture.registerWithWelcome();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":irc.host BATCH +hx chathistory #omarchy\r\n"
                          "@batch=hx :alice!u@h PRIVMSG #omarchy :older\r\n"
                          ":irc.host BATCH -hx\r\n"
                          ":bob!u@h PRIVMSG #omarchy :after\r\n"));
    QVERIFY(batches.isEmpty());
    QCOMPARE(bodies, QStringList({QStringLiteral("after")}));
}

void SessionTest::solicitedHistoryBatchSurvivesOpenBatchFlood()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.autojoinChannels = {};
    Fixture fixture(sessionConfig);
    QList<IrcHistoryBatch> batches;
    QObject::connect(fixture.session, &IrcSession::historyBatchReceived, fixture.session,
                     [&](const QString&, const IrcHistoryBatch& batch) {
        batches.append(batch);
    });
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory\r\n"
                          ":server CAP omairc ACK :batch chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    QVERIFY(fixture.session->historyPending());

    QByteArray fill;
    for (int index = 0; index < 16; ++index) {
        fill += QByteArrayLiteral(":irc.host BATCH +d")
            + QByteArray::number(index)
            + QByteArrayLiteral(" unknown.example/foo\r\n");
    }
    fill += QByteArrayLiteral(
        ":irc.host BATCH +hx chathistory #omarchy\r\n"
        "@batch=hx :alice!u@h PRIVMSG #omarchy :older\r\n"
        ":irc.host BATCH -hx\r\n");
    fixture.transport->injectBytes(fill);

    QCOMPARE(batches.size(), 1);
    QCOMPARE(batches.front().target, QStringLiteral("#omarchy"));
    QCOMPARE(int(batches.front().lines.size()), 1);
    QVERIFY(!fixture.session->historyPending());
}

void SessionTest::isupportChatHistoryLimitCapsTheRequest()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.autojoinChannels = {};
    Fixture fixture(sessionConfig);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory\r\n"
                          ":server CAP omairc ACK :batch chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":server 005 omairc CHATHISTORY=25 :are supported\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    QVERIFY(fixture.wrote(
        QByteArrayLiteral("CHATHISTORY LATEST #omarchy * 25\r\n")));
}

void SessionTest::chatHistoryRequestFillsBothPlaceholdersAtOnce()
{
    IrcSessionConfig sessionConfig = config();
    sessionConfig.autojoinChannels = {};
    Fixture fixture(sessionConfig);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory\r\n"
                          ":server CAP omairc ACK :batch chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#%2omarchy\r\n"));
    QVERIFY(fixture.wrote(
        QByteArrayLiteral("CHATHISTORY LATEST #%2omarchy * 100\r\n")));
}

int runSessionTests(int argc, char **argv)
{
    SessionTest session;
    return QTest::qExec(&session, argc, argv);
}

#include "tst_session.moc"
