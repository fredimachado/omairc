#include <QAbstractItemModel>
#include <QCoreApplication>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QTest>
#include <QVariant>

#include <memory>

#include "fakeirctransport.h"
#include "irccapability.h"
#include "irccontroller.h"
#include "ircsession.h"
#include "messagelistmodel.h"
#include "networklogmodel.h"
#include "testsettings.h"

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
IrcSessionConfig sessionConfig(const QString& networkId = QStringLiteral("network-a"))
{
    IrcSessionConfig value;
    value.networkId = networkId;
    value.host = QStringLiteral("irc.example");
    value.port = 6697;
    value.tlsEnabled = true;
    value.nick = QStringLiteral("omairc");
    value.username = QStringLiteral("omairc");
    value.realname = QStringLiteral("Omairc User");
    value.reconnectEnabled = false;
    return value;
}

struct SessionFixture
{
    SessionFixture()
        : transport(new FakeIrcTransport)
        , timer(new FakeReconnectTimer)
        , capabilityTimer(new FakeReconnectTimer)
        , pingTimer(new FakeReconnectTimer)
        , labelTimer(new FakeReconnectTimer)
        , session(new IrcSession(sessionConfig(), transport, timer, capabilityTimer,
                                 nullptr, pingTimer, nullptr, labelTimer))
    {
    }

    ~SessionFixture()
    {
        delete session;
    }

    void registerLabeled()
    {
        session->start();
        transport->completeConnect();
        transport->injectBytes(
            QByteArrayLiteral(":server CAP omairc LS :message-tags labeled-response\r\n"
                              ":server CAP omairc ACK :message-tags labeled-response\r\n"
                              ":server 001 omairc :Welcome\r\n"));
    }

    void registerUnlabeled()
    {
        session->start();
        transport->completeConnect();
        transport->injectBytes(
            QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                              ":server 001 omairc :Welcome\r\n"));
    }

    FakeIrcTransport *transport;
    FakeReconnectTimer *timer;
    FakeReconnectTimer *capabilityTimer;
    FakeReconnectTimer *pingTimer;
    FakeReconnectTimer *labelTimer;
    IrcSession *session;
};

QString requestLabelOf(const QByteArray& frame)
{
    if (!frame.startsWith("@label="))
        return {};
    const int space = frame.indexOf(' ');
    if (space < 0)
        return {};
    return QString::fromUtf8(frame.mid(7, space - 7));
}

QByteArray commandOf(const QByteArray& frame)
{
    if (!frame.startsWith('@'))
        return frame;
    const int space = frame.indexOf(' ');
    if (space < 0)
        return frame;
    return frame.mid(space + 1);
}

QVariant roleAt(const QAbstractItemModel *model, int row, int role)
{
    return model->data(model->index(row, 0), role);
}

QStringList selectedBodies(const QAbstractItemModel *messages)
{
    QStringList bodies;
    if (!messages)
        return bodies;
    for (int row = 0; row < messages->rowCount(); ++row)
        bodies.append(roleAt(messages, row, MessageListModel::BodyRole).toString());
    return bodies;
}

bool selectedBodiesContain(const QAbstractItemModel *messages, const QString& needle)
{
    for (const QString& body : selectedBodies(messages)) {
        if (body.contains(needle))
            return true;
    }
    return false;
}

bool hasWhoisBody(const QAbstractItemModel *messages, const QString& body)
{
    if (!messages)
        return false;
    for (int row = 0; row < messages->rowCount(); ++row) {
        if (roleAt(messages, row, MessageListModel::BodyRole).toString() == body
            && roleAt(messages, row, MessageListModel::KindRole).toString()
                == QStringLiteral("whois")) {
            return true;
        }
    }
    return false;
}

bool logContains(QAbstractItemModel *lines, const QString& needle)
{
    if (!lines)
        return false;
    for (int row = 0; row < lines->rowCount(); ++row) {
        const QString text =
            lines->data(lines->index(row, 0), NetworkLogModel::TextRole).toString();
        if (text.contains(needle))
            return true;
    }
    return false;
}

IrcSession *registerLabeledController(IrcController& controller,
                                     FakeIrcTransport *transport,
                                     FakeReconnectTimer *labelTimer = nullptr)
{
    IrcSession *session = controller.addSession(sessionConfig(QStringLiteral("libera")),
                                                transport, nullptr, labelTimer);
    if (!session)
        return nullptr;
    if (!controller.start(QStringLiteral("libera")))
        return nullptr;
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :message-tags labeled-response\r\n"
                          ":server CAP omairc ACK :message-tags labeled-response\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":omairc!u@h JOIN :#help\r\n"));
    if (!session->capabilities().contains(IrcCapability::LabeledResponse))
        return nullptr;
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    return session;
}
}

class LabeledResponseTest : public QObject
{
    Q_OBJECT

private slots:
    void init();
    void requestsLabeledResponseOnOwnLine();
    void doesNotRequestLabeledResponseWithoutMessageTags();
    void whoisStaysUntaggedWithoutCap();
    void whoisAndCtcpGetUniqueLabels();
    void singleLabeledNumericRoutesToAskingTranscript();
    void labeledResponseBatchRoutesInnerLines();
    void unlabeledLeftoverDoesNotStealLabeledWaiter();
    void unsolicitedLabelDoesNotCrashOrSteal();
    void ackCompletesWaiterWithoutTranscript();
    void labeledFailCopiesIntoAskingTranscript();
    void labeledCtcpReplyRoutesToAskingTranscript();
    void labeledCtcp401CopiesIntoAskingTranscript();
    void timeoutClearsWaiters();
    void timeoutExpiresOnlyElapsedLabels();
    void timeoutDropsElapsedWatchWithoutStealingNewer();
    void disconnectClearsWaiters();
    void twoLabeledWhoisForSameNickStayIndependent();

private:
    std::unique_ptr<QTemporaryDir> m_settingsDir;
};

void LabeledResponseTest::init()
{
    m_settingsDir = std::make_unique<QTemporaryDir>();
    QVERIFY(m_settingsDir->isValid());
    TestSettings::isolate(m_settingsDir->path());
    QCoreApplication::setOrganizationName(QStringLiteral("omairc"));
    QCoreApplication::setApplicationName(QStringLiteral("omairc"));
}

void LabeledResponseTest::requestsLabeledResponseOnOwnLine()
{
    SessionFixture fixture;
    fixture.session->start();
    fixture.transport->completeConnect();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :message-tags labeled-response "
                          "away-notify\r\n"));
    QVERIFY(fixture.transport->writtenFrames().contains(
        QByteArrayLiteral("CAP REQ :message-tags\r\n")));
    QVERIFY(fixture.transport->writtenFrames().contains(
        QByteArrayLiteral("CAP REQ :labeled-response\r\n")));
    QVERIFY(fixture.transport->writtenFrames().contains(
        QByteArrayLiteral("CAP REQ :away-notify\r\n")));
}

void LabeledResponseTest::doesNotRequestLabeledResponseWithoutMessageTags()
{
    SessionFixture fixture;
    fixture.session->start();
    fixture.transport->completeConnect();
    fixture.transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :labeled-response away-notify\r\n"));
    QVERIFY(!fixture.transport->writtenFrames().contains(
        QByteArrayLiteral("CAP REQ :labeled-response\r\n")));
    QVERIFY(fixture.transport->writtenFrames().contains(
        QByteArrayLiteral("CAP REQ :away-notify\r\n")));
}

void LabeledResponseTest::whoisStaysUntaggedWithoutCap()
{
    SessionFixture fixture;
    fixture.registerUnlabeled();
    QVERIFY(!fixture.session->capabilities().contains(IrcCapability::LabeledResponse));

    QCOMPARE(fixture.session->startLabeledRequest(), QString());
    QVERIFY(fixture.session->whois(QStringLiteral("lena")));
    QCOMPARE(fixture.transport->writtenFrames().last(),
             QByteArrayLiteral("WHOIS lena lena\r\n"));
    QCOMPARE(fixture.session->pendingRequestLabelCount(), 0);
}

void LabeledResponseTest::whoisAndCtcpGetUniqueLabels()
{
    SessionFixture fixture;
    fixture.registerLabeled();
    QVERIFY(fixture.session->capabilities().contains(IrcCapability::LabeledResponse));
    QVERIFY(fixture.session->capabilities().contains(IrcCapability::MessageTags));

    const QString first = fixture.session->startLabeledRequest();
    QVERIFY(!first.isEmpty());
    QVERIFY(fixture.session->whois(QStringLiteral("lena"), first));
    QCOMPARE(requestLabelOf(fixture.transport->writtenFrames().last()), first);
    QCOMPARE(commandOf(fixture.transport->writtenFrames().last()),
             QByteArrayLiteral("WHOIS lena lena\r\n"));

    const QString second = fixture.session->startLabeledRequest();
    QVERIFY(!second.isEmpty());
    QVERIFY(first != second);
    QVERIFY(fixture.session->whois(QStringLiteral("mira"), second));
    QCOMPARE(requestLabelOf(fixture.transport->writtenFrames().last()), second);
    QCOMPARE(fixture.session->pendingRequestLabelCount(), 2);

    const QString ctcp = fixture.session->startLabeledRequest();
    QVERIFY(!ctcp.isEmpty());
    QVERIFY(ctcp != first);
    QVERIFY(ctcp != second);
    QVERIFY(fixture.session->sendCtcp(QStringLiteral("lena"),
                                      QStringLiteral("VERSION"),
                                      {},
                                      ctcp));
    QCOMPARE(requestLabelOf(fixture.transport->writtenFrames().last()), ctcp);
    QVERIFY(commandOf(fixture.transport->writtenFrames().last())
                .startsWith("PRIVMSG lena :"));
    QCOMPARE(fixture.session->pendingRequestLabelCount(), 3);
}

void LabeledResponseTest::singleLabeledNumericRoutesToAskingTranscript()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(registerLabeledController(controller, transport));

    QVERIFY(controller.sendMessage(QStringLiteral("/whois lena")));
    const QString label = requestLabelOf(transport->writtenFrames().last());
    QVERIFY(!label.isEmpty());

    transport->injectBytes(
        QByteArray("@label=" + label.toUtf8()
                   + " :irc 401 omairc lena :No such nick/channel\r\n"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(hasWhoisBody(messages, QStringLiteral("No such nick: lena")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("No such nick: lena")));
}

void LabeledResponseTest::labeledResponseBatchRoutesInnerLines()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(registerLabeledController(controller, transport));

    QVERIFY(controller.sendMessage(QStringLiteral("/whois lena")));
    const QString label = requestLabelOf(transport->writtenFrames().last());
    QVERIFY(!label.isEmpty());

    transport->injectBytes(
        QByteArray("@label=" + label.toUtf8()
                   + " :irc.example BATCH +NMzYSq45x labeled-response\r\n"
                     "@batch=NMzYSq45x :irc 311 omairc lena ~lena user/host * :Lena\r\n"
                     "@batch=NMzYSq45x :irc 319 omairc lena :#omarchy\r\n"
                     "@batch=NMzYSq45x :irc 318 omairc lena :End of /WHOIS list.\r\n"
                     ":irc.example BATCH -NMzYSq45x\r\n"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(hasWhoisBody(messages, QStringLiteral("lena is ~lena@user/host (Lena)")));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("lena is on #omarchy")));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("End of WHOIS for lena")));
}

void LabeledResponseTest::unlabeledLeftoverDoesNotStealLabeledWaiter()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(registerLabeledController(controller, transport));

    QVERIFY(controller.sendMessage(QStringLiteral("/whois lena")));
    const QString label = requestLabelOf(transport->writtenFrames().last());
    QVERIFY(!label.isEmpty());

    transport->injectBytes(
        QByteArrayLiteral(":irc 311 omairc lena ~other host * :Other\r\n"
                          ":irc 318 omairc lena :End of /WHOIS list.\r\n"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("Other")));
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("End of WHOIS for lena")));
    QVERIFY(logContains(controller.console()->lines(), QStringLiteral("Other")));

    transport->injectBytes(
        QByteArray("@label=" + label.toUtf8()
                   + " :irc.example BATCH +whois1 labeled-response\r\n"
                     "@batch=whois1 :irc 311 omairc lena ~lena user/host * :Lena\r\n"
                     "@batch=whois1 :irc 318 omairc lena :End of /WHOIS list.\r\n"
                     ":irc.example BATCH -whois1\r\n"));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("lena is ~lena@user/host (Lena)")));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("End of WHOIS for lena")));
}

void LabeledResponseTest::unsolicitedLabelDoesNotCrashOrSteal()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(registerLabeledController(controller, transport));

    QVERIFY(controller.sendMessage(QStringLiteral("/whois lena")));
    const QString label = requestLabelOf(transport->writtenFrames().last());
    QVERIFY(!label.isEmpty());

    transport->injectBytes(
        QByteArrayLiteral("@label=nope :irc 311 omairc lena ~x h * :Nope\r\n"
                          "@label=nope :irc 318 omairc lena :End of /WHOIS list.\r\n"
                          "@label=nope :irc.example ACK\r\n"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("Nope")));

    transport->injectBytes(
        QByteArray("@label=" + label.toUtf8()
                   + " :irc 401 omairc lena :No such nick/channel\r\n"));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("No such nick: lena")));
}

void LabeledResponseTest::ackCompletesWaiterWithoutTranscript()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(registerLabeledController(controller, transport));

    QVERIFY(controller.sendMessage(QStringLiteral("/whois lena")));
    const QString label = requestLabelOf(transport->writtenFrames().last());
    QVERIFY(!label.isEmpty());
    QCOMPARE(controller.session(QStringLiteral("libera"))->pendingRequestLabelCount(), 1);

    transport->injectBytes(
        QByteArray("@label=" + label.toUtf8() + " :irc.example ACK\r\n"));
    QCOMPARE(controller.session(QStringLiteral("libera"))->pendingRequestLabelCount(), 0);

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    const QStringList before = selectedBodies(messages);
    transport->injectBytes(
        QByteArrayLiteral(":irc 311 omairc lena ~lena user/host * :Lena\r\n"
                          ":irc 318 omairc lena :End of /WHOIS list.\r\n"));
    QCOMPARE(selectedBodies(messages), before);
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("lena is ~lena@user/host (Lena)")));
}

void LabeledResponseTest::labeledFailCopiesIntoAskingTranscript()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(registerLabeledController(controller, transport));

    QVERIFY(controller.sendMessage(QStringLiteral("/whois lena")));
    const QString label = requestLabelOf(transport->writtenFrames().last());
    QVERIFY(!label.isEmpty());

    transport->injectBytes(
        QByteArray("@label=" + label.toUtf8()
                   + " :irc FAIL WHOIS TEMPORARILY_UNAVAILABLE lena :try later\r\n"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("try later")));
}

void LabeledResponseTest::labeledCtcpReplyRoutesToAskingTranscript()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(registerLabeledController(controller, transport));

    QVERIFY(controller.sendMessage(QStringLiteral("/version lena")));
    const QString label = requestLabelOf(transport->writtenFrames().last());
    QVERIFY(!label.isEmpty());
    QVERIFY(commandOf(transport->writtenFrames().last()).startsWith("PRIVMSG lena :"));

    transport->injectBytes(
        QByteArray("@label=" + label.toUtf8()
                   + " :lena!u@h NOTICE omairc :\x01VERSION Omairc 0.4.0\x01\r\n"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(hasWhoisBody(messages,
                         QStringLiteral("VERSION reply from lena: Omairc 0.4.0")));
}

void LabeledResponseTest::labeledCtcp401CopiesIntoAskingTranscript()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(registerLabeledController(controller, transport));

    QVERIFY(controller.sendMessage(QStringLiteral("/version missingnick")));
    const QString label = requestLabelOf(transport->writtenFrames().last());
    QVERIFY(!label.isEmpty());
    QVERIFY(commandOf(transport->writtenFrames().last())
                .startsWith("PRIVMSG missingnick :"));
    QCOMPARE(controller.session(QStringLiteral("libera"))->pendingRequestLabelCount(), 1);

    transport->injectBytes(
        QByteArray("@label=" + label.toUtf8()
                   + " :irc 401 omairc missingnick :No such nick/channel\r\n"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(hasWhoisBody(messages, QStringLiteral("No such nick: missingnick")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("No such nick: missingnick")));
    QCOMPARE(controller.session(QStringLiteral("libera"))->pendingRequestLabelCount(), 0);

    const QStringList after401 = selectedBodies(messages);
    transport->injectBytes(
        QByteArrayLiteral(":irc 401 omairc missingnick :No such nick/channel\r\n"));
    QCOMPARE(selectedBodies(messages), after401);
}

void LabeledResponseTest::timeoutClearsWaiters()
{
    SessionFixture fixture;
    fixture.registerLabeled();
    QSignalSpy finished(fixture.session, &IrcSession::requestLabelFinished);

    const QString label = fixture.session->startLabeledRequest();
    QVERIFY(!label.isEmpty());
    QVERIFY(fixture.session->whois(QStringLiteral("lena"), label));
    QCOMPARE(fixture.session->pendingRequestLabelCount(), 1);
    QVERIFY(fixture.session->hasPendingRequestLabel(label));
    QVERIFY(fixture.labelTimer->active);
    QCOMPARE(fixture.labelTimer->delays.last(), 45000);

    fixture.labelTimer->fire();
    QCOMPARE(fixture.session->pendingRequestLabelCount(), 0);
    QVERIFY(!fixture.session->hasPendingRequestLabel(label));
    QCOMPARE(finished.size(), 1);
    QCOMPARE(finished.first().at(1).toString(), label);
}

void LabeledResponseTest::timeoutExpiresOnlyElapsedLabels()
{
    SessionFixture fixture;
    fixture.registerLabeled();
    qint64 now = 0;
    fixture.session->setMonotonicClock([&now]() { return now; });
    QSignalSpy finished(fixture.session, &IrcSession::requestLabelFinished);

    const QString first = fixture.session->startLabeledRequest();
    QVERIFY(!first.isEmpty());
    QVERIFY(fixture.session->whois(QStringLiteral("lena"), first));
    QCOMPARE(fixture.labelTimer->delays, QList<int>({45000}));
    QVERIFY(fixture.session->hasPendingRequestLabel(first));

    now = 10000;

    const QString second = fixture.session->startLabeledRequest();
    QVERIFY(!second.isEmpty());
    QVERIFY(first != second);
    QVERIFY(fixture.session->whois(QStringLiteral("mira"), second));
    QCOMPARE(fixture.session->pendingRequestLabelCount(), 2);
    QVERIFY(fixture.session->hasPendingRequestLabel(first));
    QVERIFY(fixture.session->hasPendingRequestLabel(second));
    QCOMPARE(fixture.labelTimer->delays, QList<int>({45000}));

    now = 45000;
    fixture.labelTimer->fire();
    QVERIFY(!fixture.session->hasPendingRequestLabel(first));
    QVERIFY(fixture.session->hasPendingRequestLabel(second));
    QCOMPARE(fixture.session->pendingRequestLabelCount(), 1);
    QCOMPARE(finished.size(), 1);
    QCOMPARE(finished.first().at(1).toString(), first);
    QCOMPARE(fixture.labelTimer->delays, QList<int>({45000, 10000}));
    QVERIFY(fixture.labelTimer->active);

    now = 55000;
    fixture.labelTimer->fire();
    QCOMPARE(fixture.session->pendingRequestLabelCount(), 0);
    QVERIFY(!fixture.session->hasPendingRequestLabel(second));
    QCOMPARE(finished.size(), 2);
    QCOMPARE(finished.at(1).at(1).toString(), second);
}

void LabeledResponseTest::timeoutDropsElapsedWatchWithoutStealingNewer()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    auto *labelTimer = new FakeReconnectTimer;
    QVERIFY(registerLabeledController(controller, transport, labelTimer));
    IrcSession *session = controller.session(QStringLiteral("libera"));
    QVERIFY(session);
    qint64 now = 0;
    session->setMonotonicClock([&now]() { return now; });

    QVERIFY(controller.sendMessage(QStringLiteral("/whois lena")));
    const QString first = requestLabelOf(transport->writtenFrames().last());
    QVERIFY(!first.isEmpty());
    QCOMPARE(labelTimer->delays, QList<int>({45000}));

    now = 10000;
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#help"));
    QVERIFY(controller.sendMessage(QStringLiteral("/whois mira")));
    const QString second = requestLabelOf(transport->writtenFrames().last());
    QVERIFY(!second.isEmpty());
    QVERIFY(first != second);
    QCOMPARE(labelTimer->delays, QList<int>({45000}));
    QVERIFY(session->hasPendingRequestLabel(first));
    QVERIFY(session->hasPendingRequestLabel(second));

    now = 45000;
    labelTimer->fire();
    QVERIFY(!session->hasPendingRequestLabel(first));
    QVERIFY(session->hasPendingRequestLabel(second));

    transport->injectBytes(
        QByteArray("@label=" + first.toUtf8()
                   + " :irc 318 omairc lena :End of /WHOIS list.\r\n"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("End of WHOIS for lena")));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#help"));
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("End of WHOIS for lena")));

    transport->injectBytes(
        QByteArrayLiteral(":irc 311 omairc mira ~other host * :Other\r\n"
                          ":irc 318 omairc mira :End of /WHOIS list.\r\n"));
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("Other")));
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("End of WHOIS for mira")));

    transport->injectBytes(
        QByteArray("@label=" + second.toUtf8()
                   + " :irc.example BATCH +b labeled-response\r\n"
                     "@batch=b :irc 311 omairc mira ~m h * :FromHelp\r\n"
                     "@batch=b :irc 318 omairc mira :End of /WHOIS list.\r\n"
                     ":irc.example BATCH -b\r\n"));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("mira is ~m@h (FromHelp)")));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("End of WHOIS for mira")));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("FromHelp")));
}

void LabeledResponseTest::disconnectClearsWaiters()
{
    SessionFixture fixture;
    fixture.registerLabeled();
    const QString label = fixture.session->startLabeledRequest();
    QVERIFY(!label.isEmpty());
    QVERIFY(fixture.session->whois(QStringLiteral("lena"), label));
    QCOMPARE(fixture.session->pendingRequestLabelCount(), 1);

    fixture.session->stop();
    QCOMPARE(fixture.session->pendingRequestLabelCount(), 0);
}

void LabeledResponseTest::twoLabeledWhoisForSameNickStayIndependent()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(registerLabeledController(controller, transport));

    QVERIFY(controller.sendMessage(QStringLiteral("/whois mira")));
    const QString first = requestLabelOf(transport->writtenFrames().last());
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#help"));
    QVERIFY(controller.sendMessage(QStringLiteral("/whois mira")));
    const QString second = requestLabelOf(transport->writtenFrames().last());
    QVERIFY(first != second);

    transport->injectBytes(
        QByteArray("@label=" + first.toUtf8()
                   + " :irc.example BATCH +a labeled-response\r\n"
                     "@batch=a :irc 311 omairc mira ~m h * :FromOmarchy\r\n"
                     "@batch=a :irc 318 omairc mira :End of /WHOIS list.\r\n"
                     ":irc.example BATCH -a\r\n"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("mira is ~m@h (FromOmarchy)")));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#help"));
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("FromOmarchy")));

    transport->injectBytes(
        QByteArray("@label=" + second.toUtf8()
                   + " :irc.example BATCH +b labeled-response\r\n"
                     "@batch=b :irc 311 omairc mira ~m h * :FromHelp\r\n"
                     "@batch=b :irc 318 omairc mira :End of /WHOIS list.\r\n"
                     ":irc.example BATCH -b\r\n"));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("mira is ~m@h (FromHelp)")));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("FromHelp")));
}

int runLabeledResponseTests(int argc, char **argv)
{
    LabeledResponseTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_labeledresponse.moc"
