#include <QCoreApplication>
#include <QEvent>
#include <QPointer>
#include <QSignalSpy>
#include <QTest>

#include <string>
#include <string_view>

#include "fakeirctransport.h"
#include "irceventtranslator.h"
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

namespace
{
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
    value.reconnectMaximumAttempts = 3;
    return value;
}

struct Fixture
{
    explicit Fixture(IrcSessionConfig sessionConfig = config())
        : transport(new FakeIrcTransport)
        , timer(new FakeReconnectTimer)
        , capabilityTimer(new FakeReconnectTimer)
        , session(new IrcSession(sessionConfig, transport, timer, capabilityTimer))
    {
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

    bool wrote(const QByteArray& frame) const
    {
        return transport->writtenFrames().contains(frame);
    }

    FakeIrcTransport *transport;
    FakeReconnectTimer *timer;
    FakeReconnectTimer *capabilityTimer;
    IrcSession *session;
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
    void registersWhenCapIsUnsupported();
    void tlsCertificateFailureIsExplicit();
    void answersPingImmediately();
    void registrationRefusalFailsVisibly();
    void nickInUseAfterWelcomeKeepsSession();
    void unavailableResourceAfterWelcomeKeepsSession();
    void unavailableResourceBeforeWelcomeFails();
    void connectionTimeoutSchedulesReconnect();
    void remoteCloseSchedulesReconnect();
    void malformedInputSurfacesProtocolError();
    void reconnectCanBeCancelled();
    void reconnectDelayIsBoundedExponential();
    void authenticationFailureIsExplicit();
    void destructionWhileConnectingIsSafe();
    void managerRefusesSecondLiveNetwork();
    void managerCreateStaysAddOnly();
    void managerDiscardUnregistersImmediately();
    void pingAndWelcomeProduceStatusEntries();
    void configuredPasswordNeverAppearsInStatusEntries();
    void sendPrivmsgValidatesTarget();
    void setTopicIsSetOnly();
    void kickWritesOptionalReason();
    void setAwayEncodesOptionalReason();
    void whoisWritesDoubledNick();
    void whoisStatusLinesFormatKnownNumerics();
    void incomingNoticeStatusLinesWrapSpeaker();
    void incomingNoticeDoesNotTranslateToEvents();
    void incomingStandardRepliesShowDescriptionOnStatus();
    void incomingStandardRepliesDoNotTranslateToEvents();
    void inboundFailDoesNotFailTheSession();
    void incomingActionTranslatesToActionEvent();
    void latin1PrivmsgBodyIsEAcuteAndNextLineTranslates();
    void incomingCtcpRequestsAreNotConversationEvents();
    void answersCtcpRequests();
    void doesNotAnswerChannelCtcpRequests();
    void welcomeAssignsNickFrom001();
    void emptyWelcomeKeepsConfigNick();
    void selfNickUpdatesSessionNick();
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
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix echo-message\r\n"));
    QCOMPARE(fixture.session->state(), IrcSession::State::Registering);
    QCOMPARE(fixture.transport->writtenFrames().mid(1),
             QByteArrayList({
                 QByteArrayLiteral("NICK omairc\r\n"),
                 QByteArrayLiteral("USER omairc 8 * :Omairc User\r\n"),
                 QByteArrayLiteral("CAP END\r\n"),
             }));
    QVERIFY(fixture.session->capabilities().isEmpty());

    fixture.transport->injectBytes(
        QByteArrayLiteral(":server 001 omairc :Welcome\r\n"));
    QCOMPARE(fixture.session->state(), IrcSession::State::Registered);
    QCOMPARE(fixture.session->nick(), QStringLiteral("omairc"));
    QCOMPARE(registered.size(), 1);
    QCOMPARE(fixture.transport->writtenFrames().mid(4),
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
                 QByteArrayLiteral("CAP REQ :away-notify batch draft/metadata-2\r\n"),
                 QByteArrayLiteral("NICK omairc\r\n"),
                 QByteArrayLiteral("USER omairc 8 * :Omairc User\r\n"),
             }));

    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc ACK :away-notify batch draft/metadata-2\r\n"));
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
        QByteArrayLiteral(":server CAP omairc LS :sasl=PLAIN,EXTERNAL multi-prefix\r\n"));
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
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"));
    QCOMPARE(fixture.transport->writtenFrames().mid(1),
             QByteArrayList({
                 QByteArrayLiteral("PASS secret\r\n"),
                 QByteArrayLiteral("NICK omairc\r\n"),
                 QByteArrayLiteral("USER omairc 8 * :Omairc User\r\n"),
                 QByteArrayLiteral("CAP END\r\n"),
             }));
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
}

void SessionTest::registrationRefusalFailsVisibly()
{
    Fixture fixture;
    QSignalSpy errors(fixture.session, &IrcSession::errorOccurred);
    fixture.connectTls();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server 433 * omairc :Nickname in use\r\n"));

    QCOMPARE(fixture.session->state(), IrcSession::State::Failed);
    QCOMPARE(errors.size(), 1);
    QCOMPARE(qvariant_cast<IrcSession::ErrorKind>(errors.at(0).at(1)),
             IrcSession::ErrorKind::Registration);
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

void SessionTest::malformedInputSurfacesProtocolError()
{
    Fixture fixture;
    QSignalSpy errors(fixture.session, &IrcSession::errorOccurred);
    fixture.connectTls();
    fixture.transport->injectBytes(QByteArray("BAD\0FRAME\r\n", 11));

    QCOMPARE(errors.size(), 1);
    QCOMPARE(qvariant_cast<IrcSession::ErrorKind>(errors.at(0).at(1)),
             IrcSession::ErrorKind::Protocol);
    QCOMPARE(fixture.session->state(), IrcSession::State::CapLs);
}

void SessionTest::reconnectCanBeCancelled()
{
    Fixture fixture;
    fixture.connectTls();
    fixture.transport->remoteClose();
    QVERIFY(fixture.timer->active);

    fixture.session->cancelReconnect();
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
    Fixture fixture;
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

    fixture.timer->fire();
    fixture.transport->completeConnect();
    fixture.transport->remoteClose();
    QCOMPARE(fixture.session->state(), IrcSession::State::Failed);
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

void SessionTest::managerRefusesSecondLiveNetwork()
{
    IrcSessionManager manager;
    QSignalSpy refused(&manager, &IrcSessionManager::activationRefused);
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
    QVERIFY(!manager.activateSession(QStringLiteral("network-b")));
    QCOMPARE(refused.size(), 1);
    QCOMPARE(manager.activeNetworkId(), QStringLiteral("network-a"));
    QCOMPARE(secondTransport->connectionState(),
             IrcTransport::ConnectionState::Idle);

    QVERIFY(manager.stopSession(QStringLiteral("network-a")));
    QVERIFY(manager.activateSession(QStringLiteral("network-b")));
    QCOMPARE(manager.activeNetworkId(), QStringLiteral("network-b"));
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
    QCOMPARE(manager.activeNetworkId(), QString());
    QVERIFY(!manager.discardSession(QStringLiteral("network-a")));

    auto *replacementTransport = new FakeIrcTransport;
    IrcSession *replacement = manager.createSession(
        config(QStringLiteral("network-a")), replacementTransport, new FakeReconnectTimer);
    QVERIFY(replacement);
    QVERIFY(replacement != session);

    QVERIFY(!guard.isNull());
    QCoreApplication::sendPostedEvents(session, QEvent::DeferredDelete);
    QVERIFY(guard.isNull());
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
                          ":MetaNova!u@h PRIVMSG omairc :\x01TIME\x01\r\n"
                          ":MetaNova!u@h PRIVMSG omairc :\x01VERSION\x01\r\n"));

    QVERIFY(fixture.wrote(QByteArrayLiteral(
        "NOTICE MetaNova :\x01PING token\x01\r\n")));
    QVERIFY(fixture.transport->writtenFrames().last().startsWith(
        QByteArrayLiteral("NOTICE MetaNova :\x01VERSION Omairc 0.1.0\x01\r\n")));
    QVERIFY(fixture.transport->writtenFrames().at(
        fixture.transport->writtenFrames().size() - 2).startsWith(
        QByteArrayLiteral("NOTICE MetaNova :\x01TIME ")));
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

int runSessionTests(int argc, char **argv)
{
    SessionTest session;
    return QTest::qExec(&session, argc, argv);
}

#include "tst_session.moc"
