#include <QCoreApplication>
#include <QEvent>
#include <QPointer>
#include <QSignalSpy>
#include <QTest>

#include "fakeirctransport.h"
#include "ircsession.h"
#include "ircsessionmanager.h"

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

int runSessionTests(int argc, char **argv)
{
    SessionTest session;
    return QTest::qExec(&session, argc, argv);
}

#include "tst_session.moc"
