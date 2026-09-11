#include <QAbstractItemModel>
#include <QDateTime>
#include <QSignalSpy>
#include <QTest>

#include <time.h>

#include "fakeirctransport.h"
#include "irccapability.h"
#include "irccontroller.h"
#include "memberlistmodel.h"
#include "messagelistmodel.h"
#include "networklogmodel.h"

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

    QList<int> delays;
    bool active = false;
    int cancelCount = 0;
};

namespace
{
class ScopedTimeZone
{
public:
    explicit ScopedTimeZone(const char *zone)
        : m_hadTz(qEnvironmentVariableIsSet("TZ"))
        , m_previous(qgetenv("TZ"))
    {
        qputenv("TZ", zone);
        tzset();
    }

    ~ScopedTimeZone()
    {
        if (m_hadTz)
            qputenv("TZ", m_previous);
        else
            qunsetenv("TZ");
        tzset();
    }

private:
    bool m_hadTz;
    QByteArray m_previous;
};

IrcSessionConfig config(const QString& networkId)
{
    IrcSessionConfig value;
    value.networkId = networkId;
    value.host = QStringLiteral("irc.example");
    value.tlsEnabled = true;
    value.nick = QStringLiteral("omairc");
    value.username = QStringLiteral("omairc");
    value.realname = QStringLiteral("Omairc User");
    value.reconnectEnabled = false;
    return value;
}

void registerSession(IrcSession *session, FakeIrcTransport *transport)
{
    session->start();
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    QCOMPARE(session->state(), IrcSession::State::Registered);
}

QVariant roleAt(const QAbstractItemModel *model, int row, int role)
{
    return model->data(model->index(row, 0), role);
}

int rowForTarget(const QAbstractItemModel *model, const QString& target)
{
    for (int row = 0; row < model->rowCount(); ++row) {
        if (roleAt(model, row, ConversationListModel::ConversationRole) == target)
            return row;
    }
    return -1;
}

bool framesContain(const QByteArrayList& frames, const QByteArray& needle)
{
    for (const QByteArray& frame : frames) {
        if (frame.contains(needle))
            return true;
    }
    return false;
}

bool logContains(QAbstractItemModel *lines, const QString& needle)
{
    for (int row = 0; row < lines->rowCount(); ++row) {
        const QString text =
            lines->data(lines->index(row, 0), NetworkLogModel::TextRole).toString();
        if (text.contains(needle))
            return true;
    }
    return false;
}

bool selectedBodiesContain(QAbstractItemModel *messages, const QString& needle)
{
    if (!messages)
        return false;
    for (int row = 0; row < messages->rowCount(); ++row) {
        if (roleAt(messages, row, MessageListModel::BodyRole).toString().contains(needle))
            return true;
    }
    return false;
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

int bodyRow(const QAbstractItemModel *messages, const QString& body)
{
    if (!messages)
        return -1;
    for (int row = 0; row < messages->rowCount(); ++row) {
        if (roleAt(messages, row, MessageListModel::BodyRole).toString() == body)
            return row;
    }
    return -1;
}

bool hasEventBody(const QAbstractItemModel *messages, const QString& body)
{
    const int row = bodyRow(messages, body);
    return row >= 0
        && roleAt(messages, row, MessageListModel::KindRole).toString()
            == QStringLiteral("event");
}

bool hasWhoisBody(const QAbstractItemModel *messages, const QString& body)
{
    const int row = bodyRow(messages, body);
    return row >= 0
        && roleAt(messages, row, MessageListModel::KindRole).toString()
            == QStringLiteral("whois");
}

QByteArray namesBurst(int nickCount, int perLine)
{
    QByteArray bytes = QByteArrayLiteral(":omairc!u@h JOIN :#big\r\n");
    QStringList batch;
    batch.reserve(perLine);
    for (int i = 0; i < nickCount; ++i) {
        batch.append(QStringLiteral("n%1").arg(i, 4, 10, QLatin1Char('0')));
        if (batch.size() == perLine) {
            bytes += QByteArrayLiteral(":server 353 omairc = #big :");
            bytes += batch.join(QLatin1Char(' ')).toUtf8();
            bytes += QByteArrayLiteral("\r\n");
            batch.clear();
        }
    }
    if (!batch.isEmpty()) {
        bytes += QByteArrayLiteral(":server 353 omairc = #big :");
        bytes += batch.join(QLatin1Char(' ')).toUtf8();
        bytes += QByteArrayLiteral("\r\n");
    }
    bytes += QByteArrayLiteral(":server 366 omairc #big :End of NAMES\r\n");
    return bytes;
}

QByteArray whoBurst(int nickCount)
{
    QByteArray bytes;
    for (int i = 0; i < nickCount; ++i) {
        bytes += QByteArrayLiteral(":server 352 omairc #big u h s ");
        bytes += QStringLiteral("n%1").arg(i, 4, 10, QLatin1Char('0')).toUtf8();
        bytes += QByteArrayLiteral(" G :0 r\r\n");
    }
    bytes += QByteArrayLiteral(":server 315 omairc #big :End of WHO\r\n");
    return bytes;
}
}

class ControllerTest : public QObject
{
    Q_OBJECT

private slots:
    void reducesTrafficAndRoutesOutboundByNetwork();
    void liberaConnectCreatesChannelNotAuthDirect();
    void incomingNoticeStaysOnStatus();
    void incomingActionUsesActionKindAndStripsCtcp();
    void emptyNetworkIdDoesNotSwitch();
    void presenceCapabilitiesGateAwayAndStatus();
    void defaultPrefixPaintsLabelNotNick();
    void channelCloseSlashIsWrongScope();
    void partDefaultsToSelectedChannel();
    void partFromDirectIsWrongScope();
    void kickDefaultsToSelectedChannel();
    void kickFromDirectIsWrongScope();
    void statusPartDefaultsToSelectedChannel();
    void partImplicitUsesSelectedSession();
    void topicUsesSelectedSession();
    void topicFromDirectIsWrongScope();
    void emptyTopicDoesNotWrite();
    void closeDirectMessageDropsAndSelectsNeighbor();
    void closeDirectMessageInvokableOnChannelIsSilent();
    void closeDirectMessageWhileDisconnected();
    void selfAwayFollowsNumericsAndUnawaysAfterChat();
    void closeLastDirectKeepsSelfAway();
    void queryOpensDirectWithoutPrivmsg();
    void queryAliceCreatesDirectRowWithoutPrivmsg();
    void queryWithTextSendsPrivmsg();
    void queryChannelAndEmptyAreRefused();
    void noticeSendsWithoutSelecting();
    void noticeEchoesExistingDirect();
    void noticeEchoesSelectedChannel();
    void noticeMissingTokensAreRefused();
    void statusNoticeStaysOpen();
    void statusNoticeMissingBodyStaysOpen();
    void disconnectedNoticeIsNotConnected();
    void noticeDoesNotUnaway();
    void msgSendsWithoutSelecting();
    void msgEchoesExistingDirect();
    void msgDoesNotFocusExistingDirect();
    void msgEchoesSelectedGhostQuery();
    void msgMissingTokensAreRefused();
    void msgChannelIsRefused();
    void statusMsgStaysOpen();
    void disconnectedMsgIsNotConnected();
    void msgClearsAway();
    void explicitChatClearsAwayOnOriginNetwork();
    void lastErrorIsolatedByNetwork();
    void closeDirectMessageClearsOriginNetworkError();
    void awayWaitsForNumericThenChatUnaways();
    void statusQueryClosesStatus();
    void statusQueryChannelStaysOpen();
    void disconnectedQuerySelectsBareNotText();
    void statusQueryWithNoNetworkDoesNotCrash();
    void conversationClearWipesMessages();
    void ghostClearIsSent();
    void statusClearLeavesConversationMessages();
    void selectedPrivmsgInsertsMessageRow();
    void largeChannelJoinDoesNotResetModelsPerNick();
    void otherChannelNamesDoesNotSnapshotJoiningMembers();
    void namesBurstFlushesTypingClearedByChat();
    void chatDuringNamesUpdatesMessagesWithoutMemberReset();
    void incomingNickRetargetsSelectedDirect();
    void incomingNickCaseOnlyRetargetsDirect();
    void welcomeAssignedNickRoutesDirectMessages();
    void echoIfPresentUsesAssignedNick();
    void echoMessageAckSkipsLocalPrivmsg();
    void echoMessageAbsentStillEchoesLocally();
    void echoMessageAckSkipsMsgEcho();
    void msgEchoDoesNotOpenMissingDirect();
    void incomingNickservPrivmsgDoesNotOpenDirect();
    void bouncerAttachOpensOnlyPeerAuthoredDirects();
    void bouncerQueryReplayDirectAppearsInConversationModel();
    void pseudoClientPrivmsgOpensNoConversation();
    void pseudoClientReplayBatchOpensNoConversation();
    void routableNicksOpenDirectsAndPseudoClientsDoNot();
    void statusMsgNickservIdentifyDoesNotOpenDirect();
    void automaticIdentifyDoesNotOpenNickServDirect();
    void mentionArrivedOnSelectedBuffer();
    void mentionArrivedOnDirectMessageWithoutNick();
    void chghostLeavesMemberNickAndRanks();
    void twoSessionsStartTogether();
    void startingBackgroundNetworkDoesNotStealStatus();
    void quitWhileReconnecting();
    void statusJoinUsesConsoleNetwork();
    void implicitStatusPartStaysOnFocusedNetwork();
    void implicitStatusKickStaysOnFocusedNetwork();
    void selectConversationByIdUsesCompositeKey();
    void forgetNetworkDropsGhostRowsAndLog();
    void backgroundChatBumpsConversationEpoch();
    void chatHistoryBatchShowsBodyAndTime();
    void chatHistoryAndLiveTimeUseLocalWallClock();
    void whoisFromChannelCopiesStatusLinesAsEvents();
    void whoisInterleavesByAskingBuffer();
    void statusWhoisSupersedesConversationWatch();
    void unsolicitedWhoisStaysOnStatus();
    void emptyChannelWhoisNamesANick();
    void emptyDirectWhoisDefaultsAndRoutes();
    void terminalWhoisClearsWatch();
    void failedPrivmsgDoesNotStealWhoisWatch();
    void whoisFailureBeforeDeliveryDoesNotStealWatch();
    void closedDirectWhoisDoesNotResurrect();
    void whoisEventDoesNotCollapseWithJoin();
};

void ControllerTest::reducesTrafficAndRoutesOutboundByNetwork()
{
    IrcController controller;
    auto *transportA = new FakeIrcTransport;
    auto *transportB = new FakeIrcTransport;
    IrcSession *sessionA = controller.addSession(config(QStringLiteral("network-a")),
                                                 transportA);
    IrcSession *sessionB = controller.addSession(config(QStringLiteral("network-b")),
                                                 transportB);
    QVERIFY(sessionA);
    QVERIFY(sessionB);

    QVERIFY(controller.start(QStringLiteral("network-a")));
    transportA->completeConnect();
    transportA->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":server 005 omairc CHANTYPES=#& PREFIX=(ov)@+ "
                          ":are supported by this server\r\n"
                          ":omairc!u@h JOIN :#chan\r\n"
                          ":server 353 omairc = #chan :@omairc +Alice Bob\r\n"
                          ":server 366 omairc #chan :End of NAMES\r\n"
                          ":Alice!u@h PRIVMSG #chan :hello\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    auto *members = qobject_cast<QAbstractItemModel *>(controller.members());
    QCOMPARE(conversations->rowCount(), 1);
    QCOMPARE(roleAt(conversations, 0, ConversationListModel::ConversationRole),
             QStringLiteral("#chan"));
    QCOMPARE(messages->rowCount(), 2);
    QCOMPARE(roleAt(messages, 1, MessageListModel::BodyRole),
             QStringLiteral("hello"));
    QCOMPARE(members->rowCount(), 3);
    QCOMPARE(roleAt(members, 0, MemberListModel::NickRole), QStringLiteral("Alice"));
    QCOMPARE(roleAt(members, 0, MemberListModel::LabelRole), QStringLiteral("+Alice"));
    QCOMPARE(roleAt(members, 1, MemberListModel::NickRole), QStringLiteral("Bob"));
    QCOMPARE(roleAt(members, 1, MemberListModel::LabelRole), QStringLiteral("Bob"));
    QCOMPARE(roleAt(members, 2, MemberListModel::NickRole), QStringLiteral("omairc"));
    QCOMPARE(roleAt(members, 2, MemberListModel::LabelRole), QStringLiteral("@omairc"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#chan"));
    QVERIFY(controller.isChannel());
    QCOMPARE(controller.peopleCount(), 3);

    QVERIFY(controller.sendMessage(QStringLiteral("hello")));
    QVERIFY(controller.sendMessage(QStringLiteral("/me waves")));
    QCOMPARE(transportA->writtenFrames().at(transportA->writtenFrames().size() - 2),
             QByteArrayLiteral("PRIVMSG #chan :hello\r\n"));
    QCOMPARE(transportA->writtenFrames().last(),
             QByteArray("PRIVMSG #chan :\x01" "ACTION waves\x01\r\n"));
    QCOMPARE(messages->rowCount(), 4);
    QCOMPARE(roleAt(messages, 2, MessageListModel::BodyRole),
             QStringLiteral("hello"));
    QCOMPARE(roleAt(messages, 2, MessageListModel::AuthorRole),
             QStringLiteral("omairc"));
    QCOMPARE(roleAt(messages, 3, MessageListModel::KindRole),
             QStringLiteral("action"));
    QCOMPARE(controller.currentNick(), QStringLiteral("omairc"));

    transportA->injectBytes(
        QByteArrayLiteral(":op!u@h MODE #chan +o Alice\r\n"
                          ":op!u@h MODE #chan -o Alice\r\n"));
    QCOMPARE(roleAt(members, 0, MemberListModel::NickRole), QStringLiteral("Alice"));
    QCOMPARE(roleAt(members, 0, MemberListModel::LabelRole), QStringLiteral("+Alice"));

    registerSession(sessionB, transportB);
    transportB->injectBytes(
        QByteArrayLiteral(":server 005 omairc CHANTYPES=# "
                          ":are supported by this server\r\n"
                          ":omairc!u@h JOIN :#chan\r\n"));
    controller.selectConversation(QStringLiteral("network-b"),
                                  QStringLiteral("#chan"));
    QVERIFY(controller.sendMessage(QStringLiteral("second")));
    QCOMPARE(transportB->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG #chan :second\r\n"));
    QVERIFY(transportA->writtenFrames().last()
            != QByteArrayLiteral("PRIVMSG #chan :second\r\n"));
}

void ControllerTest::liberaConnectCreatesChannelNotAuthDirect()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSessionConfig sessionConfig = config(QStringLiteral("libera"));
    sessionConfig.autojoinChannels = {QStringLiteral("#omarchy")};
    IrcSession *session = controller.addSession(sessionConfig, transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral("NOTICE AUTH :*** Looking up your hostname...\r\n"
                          "NOTICE AUTH :*** Checking Ident\r\n"
                          "NOTICE AUTH :*** No Ident response\r\n"
                          "NOTICE AUTH :*** Found your hostname\r\n"
                          ":copper.libera.chat CAP omairc LS :multi-prefix\r\n"
                          ":copper.libera.chat 001 omairc :Welcome\r\n"
                          ":copper.libera.chat 005 omairc CHANTYPES=# PREFIX=(ov)@+ "
                          "NETWORK=Libera.Chat :are supported by this server\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":copper.libera.chat 353 omairc = #omarchy :@omairc\r\n"
                          ":copper.libera.chat 366 omairc #omarchy :End of NAMES\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(conversations->rowCount(), 1);
    QCOMPARE(roleAt(conversations, 0, ConversationListModel::ConversationRole),
             QStringLiteral("#omarchy"));
    QCOMPARE(roleAt(conversations, 0, ConversationListModel::DirectRole), false);
    QCOMPARE(roleAt(conversations, 0, ConversationListModel::NetworkIdRole),
             QStringLiteral("libera"));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QVERIFY(controller.isChannel());

    auto *lines = controller.console()->lines();
    QVERIFY(lines);
    bool authNotice = false;
    for (int row = 0; row < lines->rowCount(); ++row) {
        const QString text =
            lines->data(lines->index(row, 0), NetworkLogModel::TextRole).toString();
        if (text.contains(QStringLiteral("Looking up your hostname")))
            authNotice = true;
    }
    QVERIFY(authNotice);
}

void ControllerTest::incomingNoticeStaysOnStatus()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSessionConfig sessionConfig = config(QStringLiteral("libera"));
    sessionConfig.autojoinChannels = {QStringLiteral("#omarchy")};
    IrcSession *session = controller.addSession(sessionConfig, transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral("NOTICE AUTH :*** Looking up your hostname...\r\n"
                          ":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":server 005 omairc CHANTYPES=# PREFIX=(ov)@+ "
                          ":are supported by this server\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :@omairc\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"
                          ":NickServ!NickServ@services NOTICE omairc :Please identify\r\n"
                          ":bot!u@h NOTICE #omarchy :heads up\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("NickServ")), -1);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("AUTH")), -1);

    auto *lines = controller.console()->lines();
    QVERIFY(lines);
    QVERIFY(logContains(lines, QStringLiteral("-NickServ-")));
    QVERIFY(logContains(lines, QStringLiteral("-AUTH-")));
    QVERIFY(logContains(lines, QStringLiteral("-bot- heads up")));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    for (int row = 0; row < messages->rowCount(); ++row) {
        const QString body =
            roleAt(messages, row, MessageListModel::BodyRole).toString();
        QVERIFY(!body.contains(QStringLiteral("heads up")));
    }
}

void ControllerTest::incomingActionUsesActionKindAndStripsCtcp()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSessionConfig sessionConfig = config(QStringLiteral("libera"));
    sessionConfig.autojoinChannels = {QStringLiteral("#omarchy")};
    IrcSession *session = controller.addSession(sessionConfig, transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":server 005 omairc CHANTYPES=# PREFIX=(ov)@+ "
                          ":are supported by this server\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :@omairc MetaNova\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));
    transport->injectBytes(
        QByteArray(":MetaNova!u@h PRIVMSG #omarchy :\x01"
                   "ACTION feeds jvaztap\x01\r\n"));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(messages->rowCount() > 0);
    const int last = messages->rowCount() - 1;
    QCOMPARE(roleAt(messages, last, MessageListModel::AuthorRole),
             QStringLiteral("MetaNova"));
    QCOMPARE(roleAt(messages, last, MessageListModel::BodyRole),
             QStringLiteral("feeds jvaztap"));
    QCOMPARE(roleAt(messages, last, MessageListModel::KindRole),
             QStringLiteral("action"));
}

void ControllerTest::emptyNetworkIdDoesNotSwitch()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    controller.selectConversation(QString(), QStringLiteral("AUTH"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
}

void ControllerTest::presenceCapabilitiesGateAwayAndStatus()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :away-notify batch draft/metadata-2\r\n"
                          ":server CAP omairc ACK :away-notify batch draft/metadata-2\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":server 005 omairc CHANTYPES=# PREFIX=(ov)@+ "
                          ":are supported by this server\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :@omairc +Alice Bob\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(controller.hasAwayPresence());
    QVERIFY(controller.hasMemberStatus());

    auto *members = qobject_cast<QAbstractItemModel *>(controller.members());
    QCOMPARE(members->rowCount(), 3);
    QCOMPARE(roleAt(members, 0, MemberListModel::NickRole), QStringLiteral("Alice"));
    QCOMPARE(roleAt(members, 0, MemberListModel::LabelRole), QStringLiteral("+Alice"));
    QCOMPARE(roleAt(members, 0, MemberListModel::StatusRole), QString());
    QCOMPARE(roleAt(members, 0, MemberListModel::AwayRole), false);

    transport->injectBytes(
        QByteArrayLiteral(":server 352 omairc #omarchy u h server Alice G :0 real\r\n"
                          ":server 761 omairc Alice status * :writing docs\r\n"));
    QCOMPARE(roleAt(members, 0, MemberListModel::AwayRole), true);
    QCOMPARE(roleAt(members, 0, MemberListModel::StatusRole),
             QStringLiteral("writing docs"));

    transport->injectBytes(QByteArrayLiteral(":Alice!u@h AWAY\r\n"));
    QCOMPARE(roleAt(members, 0, MemberListModel::AwayRole), false);

    transport->injectBytes(QByteArrayLiteral(":server 766 omairc Alice status :no key\r\n"));
    QCOMPARE(roleAt(members, 0, MemberListModel::StatusRole), QString());

    transport->injectBytes(
        QByteArrayLiteral(":server 761 omairc Alice status * :writing docs\r\n"
                          ":Alice!u@h AWAY :lunch\r\n"
                          ":server CAP omairc DEL :away-notify draft/metadata-2\r\n"));
    QVERIFY(!controller.hasAwayPresence());
    QVERIFY(!controller.hasMemberStatus());
    QCOMPARE(roleAt(members, 0, MemberListModel::AwayRole), false);
    QCOMPARE(roleAt(members, 0, MemberListModel::StatusRole), QString());
}

void ControllerTest::defaultPrefixPaintsLabelNotNick()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :~owner @+Alice +Bob @\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    auto *members = qobject_cast<QAbstractItemModel *>(controller.members());
    QCOMPARE(members->rowCount(), 3);
    QCOMPARE(roleAt(members, 0, MemberListModel::NickRole), QStringLiteral("Alice"));
    QCOMPARE(roleAt(members, 0, MemberListModel::LabelRole), QStringLiteral("@Alice"));
    QCOMPARE(roleAt(members, 1, MemberListModel::NickRole), QStringLiteral("Bob"));
    QCOMPARE(roleAt(members, 1, MemberListModel::LabelRole), QStringLiteral("+Bob"));
    QCOMPARE(roleAt(members, 2, MemberListModel::NickRole), QStringLiteral("owner"));
    QCOMPARE(roleAt(members, 2, MemberListModel::LabelRole), QStringLiteral("~owner"));

    transport->injectBytes(
        QByteArrayLiteral(":op!u@h MODE #omarchy -o Alice\r\n"
                          ":op!u@h MODE #omarchy +o Ghost\r\n"
                          ":Alice!u@h JOIN :#omarchy\r\n"));
    QCOMPARE(members->rowCount(), 3);
    QCOMPARE(roleAt(members, 0, MemberListModel::NickRole), QStringLiteral("Alice"));
    QCOMPARE(roleAt(members, 0, MemberListModel::LabelRole), QStringLiteral("+Alice"));
}

void ControllerTest::channelCloseSlashIsWrongScope()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("hello")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG #omarchy :hello\r\n"));
    const int framesBefore = transport->writtenFrames().size();

    QVERIFY(!controller.sendMessage(QStringLiteral("/close")));
    QCOMPARE(controller.lastError(),
             QStringLiteral("Close applies to direct messages"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("PART")));
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("QUIT")));
}

void ControllerTest::partDefaultsToSelectedChannel()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    QVERIFY(!controller.sendMessage(QStringLiteral("/part")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));

    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/part")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PART #omarchy\r\n"));

    QVERIFY(controller.sendMessage(QStringLiteral("/leave")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PART #omarchy\r\n"));

    QVERIFY(controller.sendMessage(QStringLiteral("/part #desktop leftover")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PART #desktop\r\n"));
}

void ControllerTest::partFromDirectIsWrongScope()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"
                          ":lena!u@h PRIVMSG omairc :hi\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    const int framesBefore = transport->writtenFrames().size();

    QVERIFY(!controller.sendMessage(QStringLiteral("/part")));
    QCOMPARE(controller.lastError(), QStringLiteral("Part applies to channels"));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("PART")));

    QVERIFY(controller.sendMessage(QStringLiteral("/part #omarchy")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PART #omarchy\r\n"));
}

void ControllerTest::kickDefaultsToSelectedChannel()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    QVERIFY(!controller.sendMessage(QStringLiteral("/kick")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QVERIFY(!controller.sendMessage(QStringLiteral("/kick bob")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));

    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(!controller.sendMessage(QStringLiteral("/kick")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QVERIFY(!controller.sendMessage(QStringLiteral("/kick #omarchy")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));

    QVERIFY(controller.sendMessage(QStringLiteral("/kick bob")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("KICK #omarchy bob\r\n"));

    QVERIFY(controller.sendMessage(QStringLiteral("/kick bob spam")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("KICK #omarchy bob :spam\r\n"));

    QVERIFY(controller.sendMessage(QStringLiteral("/kick #desktop alice leftover")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("KICK #desktop alice :leftover\r\n"));
}

void ControllerTest::kickFromDirectIsWrongScope()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"
                          ":lena!u@h PRIVMSG omairc :hi\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    const int framesBefore = transport->writtenFrames().size();

    QVERIFY(!controller.sendMessage(QStringLiteral("/kick")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QVERIFY(!controller.sendMessage(QStringLiteral("/kick bob")));
    QCOMPARE(controller.lastError(), QStringLiteral("Kick applies to channels"));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("KICK")));

    QVERIFY(controller.sendMessage(QStringLiteral("/kick #omarchy bob")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("KICK #omarchy bob\r\n"));

    QVERIFY(controller.sendMessage(QStringLiteral("/kick #omarchy bob spam")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("KICK #omarchy bob :spam\r\n"));
}

void ControllerTest::statusPartDefaultsToSelectedChannel()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.console()->submit(QStringLiteral("/part")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PART #omarchy\r\n"));
}

void ControllerTest::partImplicitUsesSelectedSession()
{
    IrcController controller;
    auto *transportA = new FakeIrcTransport;
    auto *transportB = new FakeIrcTransport;
    IrcSession *sessionA = controller.addSession(config(QStringLiteral("network-a")),
                                                 transportA);
    IrcSession *sessionB = controller.addSession(config(QStringLiteral("network-b")),
                                                 transportB);
    QVERIFY(sessionA);
    QVERIFY(sessionB);

    QVERIFY(controller.start(QStringLiteral("network-a")));
    registerSession(sessionA, transportA);
    transportA->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));

    registerSession(sessionB, transportB);
    transportB->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("network-b"),
                                  QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/part")));
    QCOMPARE(transportB->writtenFrames().last(),
             QByteArrayLiteral("PART #omarchy\r\n"));
    QVERIFY(!framesContain(transportA->writtenFrames(), QByteArrayLiteral("PART")));
}

void ControllerTest::topicUsesSelectedSession()
{
    IrcController controller;
    auto *transportA = new FakeIrcTransport;
    auto *transportB = new FakeIrcTransport;
    IrcSession *sessionA = controller.addSession(config(QStringLiteral("network-a")),
                                                 transportA);
    IrcSession *sessionB = controller.addSession(config(QStringLiteral("network-b")),
                                                 transportB);
    QVERIFY(sessionA);
    QVERIFY(sessionB);

    QVERIFY(controller.start(QStringLiteral("network-a")));
    registerSession(sessionA, transportA);
    transportA->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));

    registerSession(sessionB, transportB);
    transportB->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("network-b"),
                                  QStringLiteral("#omarchy"));

    const QString topicBefore = controller.topic();
    QVERIFY(controller.sendMessage(QStringLiteral("/topic from b")));
    QCOMPARE(transportB->writtenFrames().last(),
             QByteArrayLiteral("TOPIC #omarchy :from b\r\n"));
    QVERIFY(!framesContain(transportA->writtenFrames(), QByteArrayLiteral("TOPIC")));
    QCOMPARE(controller.topic(), topicBefore);

    transportB->injectBytes(
        QByteArrayLiteral(":omairc!u@h TOPIC #omarchy :from b\r\n"));
    QCOMPARE(controller.topic(), QStringLiteral("from b"));
}

void ControllerTest::topicFromDirectIsWrongScope()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"
                          ":lena!u@h PRIVMSG omairc :hi\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    const int framesBefore = transport->writtenFrames().size();

    QVERIFY(!controller.sendMessage(QStringLiteral("/topic hello")));
    QCOMPARE(controller.lastError(), QStringLiteral("Topic applies to channels"));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("TOPIC")));
}

void ControllerTest::emptyTopicDoesNotWrite()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    const int framesBefore = transport->writtenFrames().size();

    QVERIFY(controller.sendMessage(QStringLiteral("/topic")));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("TOPIC")));
}

void ControllerTest::closeDirectMessageDropsAndSelectsNeighbor()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":omairc!u@h JOIN :#desktop\r\n"
                          ":omairc!u@h PART #desktop\r\n"
                          ":lena!u@h PRIVMSG omairc :hi\r\n"
                          ":zed!u@h PRIVMSG omairc :later\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(conversations->rowCount(), 4);
    QCOMPARE(roleAt(conversations, 0, ConversationListModel::ConversationRole),
             QStringLiteral("#omarchy"));
    QCOMPARE(roleAt(conversations, 1, ConversationListModel::ConversationRole),
             QStringLiteral("#desktop"));
    QCOMPARE(roleAt(conversations, 2, ConversationListModel::ConversationRole),
             QStringLiteral("lena"));
    QCOMPARE(roleAt(conversations, 3, ConversationListModel::ConversationRole),
             QStringLiteral("zed"));
    QVERIFY(rowForTarget(conversations, QStringLiteral("#desktop")) >= 0);

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    const int framesBeforeClose = transport->writtenFrames().size();
    QVERIFY(controller.sendMessage(QStringLiteral("/close")));
    QCOMPARE(controller.lastError(), QString());
    QCOMPARE(controller.selectedTarget(), QStringLiteral("zed"));
    QVERIFY(rowForTarget(conversations, QStringLiteral("lena")) < 0);
    QVERIFY(rowForTarget(conversations, QStringLiteral("zed")) >= 0);
    QVERIFY(rowForTarget(conversations, QStringLiteral("#desktop")) >= 0);
    QCOMPARE(transport->writtenFrames().size(), framesBeforeClose);
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("PART")));
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("QUIT")));

    QVERIFY(controller.sendMessage(QStringLiteral("/close")));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#desktop"));
    QVERIFY(rowForTarget(conversations, QStringLiteral("zed")) < 0);
    QVERIFY(rowForTarget(conversations, QStringLiteral("#omarchy")) >= 0);
    QVERIFY(rowForTarget(conversations, QStringLiteral("#desktop")) >= 0);
}

void ControllerTest::closeDirectMessageInvokableOnChannelIsSilent()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(!controller.sendMessage(QStringLiteral("/nope")));
    const QString errorBefore = controller.lastError();
    QCOMPARE(errorBefore, QStringLiteral("Unknown command: /nope"));

    controller.closeDirectMessage();
    QCOMPARE(controller.lastError(), errorBefore);
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
}

void ControllerTest::closeDirectMessageWhileDisconnected()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":lena!u@h PRIVMSG omairc :hi\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("lena"));
    transport->remoteClose();
    QVERIFY(session->state() != IrcSession::State::Registered);

    QVERIFY(controller.sendMessage(QStringLiteral("/close")));
    QCOMPARE(controller.selectedTarget(), QString());
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QCOMPARE(conversations->rowCount(), 0);
}

void ControllerTest::selfAwayFollowsNumericsAndUnawaysAfterChat()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    QVERIFY(!controller.selfAway());

    transport->injectBytes(
        QByteArrayLiteral(":server 306 omairc :You have been marked as being away\r\n"));
    QVERIFY(controller.selfAway());

    transport->injectBytes(
        QByteArrayLiteral(":server 305 omairc :You are no longer marked as being away\r\n"));
    QVERIFY(!controller.selfAway());

    transport->injectBytes(
        QByteArrayLiteral(":server 005 omairc CHANTYPES=# PREFIX=(ov)@+ "
                          ":are supported by this server\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :@omairc\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"
                          ":server 352 omairc #omarchy u h server omairc G :0 real\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(!controller.selfAway());

    transport->injectBytes(
        QByteArrayLiteral(":server 306 omairc :You have been marked as being away\r\n"));
    QVERIFY(controller.selfAway());

    QVERIFY(controller.sendMessage(QStringLiteral("hello")));
    QCOMPARE(transport->writtenFrames().at(transport->writtenFrames().size() - 2),
             QByteArrayLiteral("PRIVMSG #omarchy :hello\r\n"));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("AWAY\r\n"));
    QVERIFY(controller.selfAway());

    QVERIFY(controller.sendMessage(QStringLiteral("again")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG #omarchy :again\r\n"));
    QCOMPARE(transport->writtenFrames().count(QByteArrayLiteral("AWAY\r\n")), 1);
    QVERIFY(controller.selfAway());

    transport->injectBytes(
        QByteArrayLiteral(":server 305 omairc :You are no longer marked as being away\r\n"
                          ":server 306 omairc :You have been marked as being away\r\n"));
    QVERIFY(controller.selfAway());
    QVERIFY(controller.sendMessage(QStringLiteral("third")));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("AWAY\r\n"));
    QCOMPARE(transport->writtenFrames().count(QByteArrayLiteral("AWAY\r\n")), 2);

    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc NEW :away-notify\r\n"
                          ":server CAP omairc ACK :away-notify\r\n"
                          ":server CAP omairc DEL :away-notify\r\n"));
    QVERIFY(!controller.hasAwayPresence());
    QVERIFY(controller.selfAway());
}

void ControllerTest::explicitChatClearsAwayOnOriginNetwork()
{
    IrcController controller;
    auto *transportA = new FakeIrcTransport;
    auto *transportB = new FakeIrcTransport;
    IrcSession *sessionA = controller.addSession(config(QStringLiteral("network-a")),
                                                 transportA);
    IrcSession *sessionB = controller.addSession(config(QStringLiteral("network-b")),
                                                 transportB);
    QVERIFY(sessionA);
    QVERIFY(sessionB);
    QVERIFY(controller.start(QStringLiteral("network-a")));
    registerSession(sessionA, transportA);
    registerSession(sessionB, transportB);
    transportA->injectBytes(
        QByteArrayLiteral(":server 306 omairc :You have been marked as being away\r\n"));
    QVERIFY(controller.selfAway());
    controller.selectConversation(QStringLiteral("network-b"), QStringLiteral("bob"));

    QVERIFY(controller.sendToTarget(QStringLiteral("network-a"),
                                    QStringLiteral("bob"),
                                    QStringLiteral("hello")));
    QCOMPARE(transportA->writtenFrames().last(),
             QByteArrayLiteral("AWAY\r\n"));
}

void ControllerTest::lastErrorIsolatedByNetwork()
{
    IrcController controller;
    auto *transportA = new FakeIrcTransport;
    auto *transportB = new FakeIrcTransport;
    IrcSession *sessionA = controller.addSession(config(QStringLiteral("network-a")),
                                                 transportA);
    IrcSession *sessionB = controller.addSession(config(QStringLiteral("network-b")),
                                                 transportB);
    QVERIFY(sessionA);
    QVERIFY(sessionB);
    registerSession(sessionB, transportB);
    controller.selectConversation(QStringLiteral("network-b"), QStringLiteral("bob"));

    QVERIFY(!controller.sendToTarget(QStringLiteral("network-a"),
                                     QStringLiteral("bob"),
                                     QStringLiteral("hello")));
    QCOMPARE(controller.lastErrorForNetwork(QStringLiteral("network-a")),
             QStringLiteral("Not connected"));
    QCOMPARE(controller.lastError(), QString());
}

void ControllerTest::closeDirectMessageClearsOriginNetworkError()
{
    IrcController controller;
    auto *transportA = new FakeIrcTransport;
    auto *transportB = new FakeIrcTransport;
    IrcSession *sessionA = controller.addSession(config(QStringLiteral("network-a")),
                                                 transportA);
    IrcSession *sessionB = controller.addSession(config(QStringLiteral("network-b")),
                                                 transportB);
    QVERIFY(sessionA);
    QVERIFY(sessionB);
    registerSession(sessionA, transportA);
    registerSession(sessionB, transportB);
    transportA->injectBytes(QByteArrayLiteral(":alice!u@h PRIVMSG omairc :hello\r\n"));
    transportB->injectBytes(QByteArrayLiteral(":bob!u@h PRIVMSG omairc :hello\r\n"));

    controller.selectConversation(QStringLiteral("network-b"), QStringLiteral("bob"));
    QVERIFY(!controller.sendMessage(QStringLiteral("/part")));
    QCOMPARE(controller.lastErrorForNetwork(QStringLiteral("network-b")),
             QStringLiteral("Part applies to channels"));

    transportA->remoteClose();
    controller.selectConversation(QStringLiteral("network-a"), QStringLiteral("alice"));
    QVERIFY(!controller.sendMessage(QStringLiteral("/part")));
    QCOMPARE(controller.lastErrorForNetwork(QStringLiteral("network-a")),
             QStringLiteral("Not connected"));

    controller.closeDirectMessage();
    QCOMPARE(controller.selectedNetworkId(), QStringLiteral("network-b"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("bob"));
    QCOMPARE(controller.lastErrorForNetwork(QStringLiteral("network-a")),
             QString());
    QCOMPARE(controller.lastErrorForNetwork(QStringLiteral("network-b")),
             QStringLiteral("Part applies to channels"));
}

void ControllerTest::closeLastDirectKeepsSelfAway()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":server 306 omairc :You have been marked as being away\r\n"
                          ":zed!u@h PRIVMSG omairc :later\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("zed"));
    QVERIFY(controller.selfAway());
    QVERIFY(controller.sendMessage(QStringLiteral("/close")));
    QCOMPARE(controller.selectedTarget(), QString());
    QVERIFY(controller.selfAway());
}

void ControllerTest::queryOpensDirectWithoutPrivmsg()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/query lena")));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("lena"));
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("PRIVMSG lena")));
}

void ControllerTest::queryAliceCreatesDirectRowWithoutPrivmsg()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/query alice")));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("alice"));
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("PRIVMSG alice")));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    const int row = rowForTarget(conversations, QStringLiteral("alice"));
    QVERIFY(row >= 0);
    QCOMPARE(roleAt(conversations, row, ConversationListModel::DirectRole), true);

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QCOMPARE(messages->rowCount(), 0);
}

void ControllerTest::queryWithTextSendsPrivmsg()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/query lena hello")));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("lena"));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG lena :hello\r\n"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QCOMPARE(messages->rowCount(), 1);
    QCOMPARE(roleAt(messages, 0, MessageListModel::AuthorRole),
             QStringLiteral("omairc"));
    QCOMPARE(roleAt(messages, 0, MessageListModel::BodyRole),
             QStringLiteral("hello"));
    QCOMPARE(roleAt(messages, 0, MessageListModel::KindRole),
             QStringLiteral("message"));
}

void ControllerTest::queryChannelAndEmptyAreRefused()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    const int framesBefore = transport->writtenFrames().size();

    QVERIFY(!controller.sendMessage(QStringLiteral("/query #omarchy")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);

    QVERIFY(!controller.sendMessage(QStringLiteral("/query")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);
}

void ControllerTest::noticeSendsWithoutSelecting()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/notice lena later")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("NOTICE lena :later\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("lena")), -1);
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("NOTICE lena")));
}

void ControllerTest::noticeEchoesExistingDirect()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":lena!u@h PRIVMSG omairc :hi\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/notice lena later")));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("NOTICE lena :later\r\n"));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(messages->rowCount() > 0);
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::BodyRole),
             QStringLiteral("later"));
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::KindRole),
             QStringLiteral("notice"));
}

void ControllerTest::noticeEchoesSelectedChannel()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/notice #omarchy heads up")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("NOTICE #omarchy :heads up\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(messages->rowCount() > 0);
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::BodyRole),
             QStringLiteral("heads up"));
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::KindRole),
             QStringLiteral("notice"));
}

void ControllerTest::noticeMissingTokensAreRefused()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    const int framesBefore = transport->writtenFrames().size();

    QVERIFY(!controller.sendMessage(QStringLiteral("/notice")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QVERIFY(!framesContain(transport->writtenFrames().mid(framesBefore),
                           QByteArrayLiteral("NOTICE")));

    QVERIFY(!controller.sendMessage(QStringLiteral("/notice lena")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QVERIFY(!framesContain(transport->writtenFrames().mid(framesBefore),
                           QByteArrayLiteral("NOTICE")));
}

void ControllerTest::statusNoticeStaysOpen()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    IrcStatusConsole *console = controller.console();
    console->setOpen(true);
    QVERIFY(console->submit(QStringLiteral("/notice #omarchy ping")));
    QVERIFY(console->isOpen());
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("NOTICE #omarchy :ping\r\n"));
}

void ControllerTest::statusNoticeMissingBodyStaysOpen()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    IrcStatusConsole *console = controller.console();
    console->setOpen(true);
    const int framesBefore = transport->writtenFrames().size();
    QVERIFY(console->submit(QStringLiteral("/notice lena")));
    QVERIFY(console->isOpen());
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QVERIFY(logContains(console->lines(), QStringLiteral("Command was refused")));
    QVERIFY(!framesContain(transport->writtenFrames().mid(framesBefore),
                           QByteArrayLiteral("NOTICE")));
}

void ControllerTest::disconnectedNoticeIsNotConnected()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    transport->remoteClose();
    QVERIFY(session->state() != IrcSession::State::Registered);
    const int framesBefore = transport->writtenFrames().size();

    QVERIFY(!controller.sendMessage(QStringLiteral("/notice lena later")));
    QCOMPARE(controller.lastError(), QStringLiteral("Not connected"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);
    QVERIFY(!framesContain(transport->writtenFrames().mid(framesBefore),
                           QByteArrayLiteral("NOTICE")));
}

void ControllerTest::noticeDoesNotUnaway()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 306 omairc :You have been marked as being away\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(controller.selfAway());
    const int awayBefore = transport->writtenFrames().count(QByteArrayLiteral("AWAY\r\n"));

    QVERIFY(controller.sendMessage(QStringLiteral("/notice lena later")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("NOTICE lena :later\r\n"));
    QCOMPARE(transport->writtenFrames().count(QByteArrayLiteral("AWAY\r\n")),
             awayBefore);
    QVERIFY(controller.selfAway());
}

void ControllerTest::msgSendsWithoutSelecting()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    const int selectedRows = messages->rowCount();

    QVERIFY(controller.sendMessage(QStringLiteral("/msg lena hello")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG lena :hello\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("lena")), -1);
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("PRIVMSG lena")));
    QCOMPARE(messages->rowCount(), selectedRows);
    for (int row = 0; row < messages->rowCount(); ++row) {
        QVERIFY(roleAt(messages, row, MessageListModel::BodyRole)
                != QStringLiteral("hello"));
    }
}

void ControllerTest::msgEchoesExistingDirect()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":lena!u@h PRIVMSG omairc :hi\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/msg lena later")));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG lena :later\r\n"));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(messages->rowCount() > 0);
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::BodyRole),
             QStringLiteral("later"));
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::KindRole),
             QStringLiteral("message"));
}

void ControllerTest::msgDoesNotFocusExistingDirect()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":lena!u@h PRIVMSG omairc :hi\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/msg lena later")));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
}

void ControllerTest::msgEchoesSelectedGhostQuery()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/query lena")));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("lena"));

    QVERIFY(controller.sendMessage(QStringLiteral("/msg lena hello")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG lena :hello\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("lena"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(messages->rowCount() > 0);
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::BodyRole),
             QStringLiteral("hello"));
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::KindRole),
             QStringLiteral("message"));
}

void ControllerTest::msgMissingTokensAreRefused()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    const int framesBefore = transport->writtenFrames().size();

    QVERIFY(!controller.sendMessage(QStringLiteral("/msg")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QVERIFY(!framesContain(transport->writtenFrames().mid(framesBefore),
                           QByteArrayLiteral("PRIVMSG")));

    QVERIFY(!controller.sendMessage(QStringLiteral("/msg lena")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QVERIFY(!framesContain(transport->writtenFrames().mid(framesBefore),
                           QByteArrayLiteral("PRIVMSG")));
}

void ControllerTest::msgChannelIsRefused()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    const int framesBefore = transport->writtenFrames().size();

    QVERIFY(!controller.sendMessage(QStringLiteral("/msg #omarchy hello")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);
    QVERIFY(!framesContain(transport->writtenFrames().mid(framesBefore),
                           QByteArrayLiteral("PRIVMSG")));
}

void ControllerTest::statusMsgStaysOpen()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    IrcStatusConsole *console = controller.console();
    console->setOpen(true);
    QVERIFY(console->submit(QStringLiteral("/msg NickServ help")));
    QVERIFY(console->isOpen());
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QVERIFY(logContains(console->lines(), QStringLiteral("PRIVMSG NickServ")));
}

void ControllerTest::disconnectedMsgIsNotConnected()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    transport->remoteClose();
    QVERIFY(session->state() != IrcSession::State::Registered);
    const int framesBefore = transport->writtenFrames().size();

    QVERIFY(!controller.sendMessage(QStringLiteral("/msg lena hello")));
    QCOMPARE(controller.lastError(), QStringLiteral("Not connected"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);
    QVERIFY(!framesContain(transport->writtenFrames().mid(framesBefore),
                           QByteArrayLiteral("PRIVMSG")));
}

void ControllerTest::msgClearsAway()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 306 omairc :You have been marked as being away\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(controller.selfAway());
    const int awayBefore = transport->writtenFrames().count(QByteArrayLiteral("AWAY\r\n"));

    QVERIFY(controller.sendMessage(QStringLiteral("/msg lena hello")));
    QCOMPARE(transport->writtenFrames().at(transport->writtenFrames().size() - 2),
             QByteArrayLiteral("PRIVMSG lena :hello\r\n"));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("AWAY\r\n"));
    QCOMPARE(transport->writtenFrames().count(QByteArrayLiteral("AWAY\r\n")),
             awayBefore + 1);
    QVERIFY(controller.selfAway());
}

void ControllerTest::awayWaitsForNumericThenChatUnaways()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(!controller.selfAway());

    QVERIFY(controller.sendMessage(QStringLiteral("/away lunch")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("AWAY :lunch\r\n"));
    QVERIFY(!controller.selfAway());

    transport->injectBytes(
        QByteArrayLiteral(":server 306 omairc :You have been marked as being away\r\n"));
    QVERIFY(controller.selfAway());

    QVERIFY(controller.sendMessage(QStringLiteral("hello")));
    QCOMPARE(transport->writtenFrames().at(transport->writtenFrames().size() - 2),
             QByteArrayLiteral("PRIVMSG #omarchy :hello\r\n"));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("AWAY\r\n"));
    QCOMPARE(transport->writtenFrames().count(QByteArrayLiteral("AWAY\r\n")), 1);
    QVERIFY(controller.selfAway());

    QVERIFY(controller.sendMessage(QStringLiteral("again")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG #omarchy :again\r\n"));
    QCOMPARE(transport->writtenFrames().count(QByteArrayLiteral("AWAY\r\n")), 1);
    QVERIFY(controller.selfAway());
}

void ControllerTest::statusQueryClosesStatus()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    IrcStatusConsole *console = controller.console();
    console->setOpen(true);
    QVERIFY(console->isOpen());
    QVERIFY(console->submit(QStringLiteral("/query lena")));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("lena"));
    QVERIFY(!console->isOpen());
}

void ControllerTest::statusQueryChannelStaysOpen()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    IrcStatusConsole *console = controller.console();
    console->setOpen(true);
    QVERIFY(console->submit(QStringLiteral("/query #desktop")));
    QVERIFY(console->isOpen());
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QVERIFY(logContains(console->lines(), QStringLiteral("Command was refused")));
}

void ControllerTest::disconnectedQuerySelectsBareNotText()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    transport->remoteClose();
    QVERIFY(session->state() != IrcSession::State::Registered);

    QVERIFY(controller.sendMessage(QStringLiteral("/query lena")));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("lena"));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QVERIFY(!controller.sendMessage(QStringLiteral("/query lena hello")));
    QCOMPARE(controller.lastError(), QStringLiteral("Not connected"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
}

void ControllerTest::statusQueryWithNoNetworkDoesNotCrash()
{
    IrcController empty;
    QVERIFY(empty.console()->submit(QStringLiteral("/query")));
    QVERIFY(empty.console()->submit(QStringLiteral("/query lena")));
}

void ControllerTest::conversationClearWipesMessages()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("hello")));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(messages->rowCount() > 0);
    QVERIFY(rowForTarget(conversations, QStringLiteral("#omarchy")) >= 0);
    const int framesBefore = transport->writtenFrames().size();

    QVERIFY(controller.sendMessage(QStringLiteral("/clear")));
    QCOMPARE(controller.lastError(), QString());
    QCOMPARE(messages->rowCount(), 0);
    QVERIFY(rowForTarget(conversations, QStringLiteral("#omarchy")) >= 0);
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("PART")));
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("QUIT")));

    QVERIFY(controller.sendMessage(QStringLiteral("/clear")));
    QCOMPARE(controller.lastError(), QString());
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QCOMPARE(messages->rowCount(), 0);
}

void ControllerTest::ghostClearIsSent()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    controller.openDirectMessage(QStringLiteral("ghost"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("ghost"));
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QVERIFY(rowForTarget(conversations, QStringLiteral("ghost")) >= 0);

    QVERIFY(controller.sendMessage(QStringLiteral("/clear")));
    QCOMPARE(controller.lastError(), QString());
    QCOMPARE(controller.selectedTarget(), QStringLiteral("ghost"));
    QVERIFY(rowForTarget(conversations, QStringLiteral("ghost")) >= 0);
}

void ControllerTest::statusClearLeavesConversationMessages()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("hello")));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    const int conversationRows = messages->rowCount();
    QVERIFY(conversationRows > 0);
    IrcStatusConsole *console = controller.console();
    QVERIFY(console->lines()->rowCount() > 0);

    QVERIFY(console->submit(QStringLiteral("/clear")));
    QCOMPARE(console->lines()->rowCount(), 0);
    QCOMPARE(messages->rowCount(), conversationRows);
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
}

void ControllerTest::selectedPrivmsgInsertsMessageRow()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :omairc Alice\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    const int rowsAfterJoin = messages->rowCount();
    QVERIFY(rowsAfterJoin > 0);

    QSignalSpy resets(messages, &QAbstractItemModel::modelReset);
    QSignalSpy inserts(messages, &QAbstractItemModel::rowsInserted);
    transport->injectBytes(
        QByteArrayLiteral(":Alice!u@h PRIVMSG #omarchy :hello\r\n"));

    QCOMPARE(resets.size(), 0);
    QCOMPARE(inserts.size(), 1);
    QCOMPARE(inserts.at(0).at(1).toInt(), rowsAfterJoin);
    QCOMPARE(inserts.at(0).at(2).toInt(), rowsAfterJoin);
    QCOMPARE(messages->rowCount(), rowsAfterJoin + 1);
    QCOMPARE(roleAt(messages, rowsAfterJoin, MessageListModel::BodyRole),
             QStringLiteral("hello"));

    QVERIFY(controller.sendMessage(QStringLiteral("own line")));
    QCOMPARE(resets.size(), 0);
    QCOMPARE(inserts.size(), 2);
    QCOMPARE(messages->rowCount(), rowsAfterJoin + 2);
    QCOMPARE(roleAt(messages, rowsAfterJoin + 1, MessageListModel::BodyRole),
             QStringLiteral("own line"));
}

void ControllerTest::largeChannelJoinDoesNotResetModelsPerNick()
{
    constexpr int nickCount = 400;
    constexpr int perLine = 20;
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :away-notify\r\n"
                          ":server CAP omairc ACK :away-notify\r\n"
                          ":server 001 omairc :Welcome\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    auto *members = qobject_cast<QAbstractItemModel *>(controller.members());
    QSignalSpy conversationResets(conversations, &QAbstractItemModel::modelReset);
    QSignalSpy conversationDataChanges(conversations, &QAbstractItemModel::dataChanged);
    QSignalSpy messageResets(messages, &QAbstractItemModel::modelReset);
    QSignalSpy memberResets(members, &QAbstractItemModel::modelReset);
    QSignalSpy memberDataChanges(members, &QAbstractItemModel::dataChanged);

    transport->injectBytes(namesBurst(nickCount, perLine));
    QCOMPARE(members->rowCount(), nickCount);
    QCOMPARE(controller.peopleCount(), nickCount);
    QCOMPARE(controller.peopleCount(), members->rowCount());
    QVERIFY(controller.hasAwayPresence());
    QVERIFY(transport->writtenFrames().contains(QByteArrayLiteral("WHO #big\r\n")));
    QCOMPARE(conversationResets.size(), 1);
    QCOMPARE(conversationDataChanges.size(), 1);
    QCOMPARE(messageResets.size(), 1);
    QCOMPARE(memberResets.size(), 2);

    const int memberResetsAfterNames = memberResets.size();
    const int conversationResetsAfterNames = conversationResets.size();
    const int messageResetsAfterNames = messageResets.size();
    memberDataChanges.clear();
    transport->injectBytes(whoBurst(nickCount));
    QCOMPARE(roleAt(members, 0, MemberListModel::AwayRole), true);
    QCOMPARE(roleAt(members, nickCount - 1, MemberListModel::AwayRole), true);
    QCOMPARE(memberResets.size(), memberResetsAfterNames);
    QCOMPARE(conversationResets.size(), conversationResetsAfterNames);
    QCOMPARE(messageResets.size(), messageResetsAfterNames);
    QCOMPARE(memberDataChanges.size(), nickCount);
}

void ControllerTest::otherChannelNamesDoesNotSnapshotJoiningMembers()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :away-notify\r\n"
                          ":server CAP omairc ACK :away-notify\r\n"
                          ":server 001 omairc :Welcome\r\n"));

    auto *members = qobject_cast<QAbstractItemModel *>(controller.members());
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#big\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#big"));
    QCOMPARE(members->rowCount(), 1);
    QCOMPARE(controller.peopleCount(), 1);
    QCOMPARE(controller.peopleCount(), members->rowCount());
    QCOMPARE(roleAt(members, 0, MemberListModel::NickRole), QStringLiteral("omairc"));

    QSignalSpy memberResets(members, &QAbstractItemModel::modelReset);
    QSignalSpy selection(&controller, &IrcController::selectionChanged);
    transport->injectBytes(
        QByteArrayLiteral(":server 353 omairc = #big :n0000 n0001\r\n"
                          ":omairc!u@h JOIN :#other\r\n"
                          ":server 353 omairc = #other :omairc\r\n"
                          ":server 366 omairc #other :End of NAMES\r\n"));

    QCOMPARE(members->rowCount(), 1);
    QCOMPARE(controller.peopleCount(), members->rowCount());
    QCOMPARE(controller.peopleCount(), 1);
    QCOMPARE(selection.size(), 0);
    QCOMPARE(memberResets.size(), 0);

    transport->injectBytes(
        QByteArrayLiteral(":n0000!u@h PRIVMSG #big :while-other-ended\r\n"));
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::BodyRole),
             QStringLiteral("while-other-ended"));
    QCOMPARE(members->rowCount(), 1);
    QCOMPARE(controller.peopleCount(), members->rowCount());
    QCOMPARE(memberResets.size(), 0);

    transport->injectBytes(
        QByteArrayLiteral(":server 366 omairc #big :End of NAMES\r\n"));
    QCOMPARE(members->rowCount(), 2);
    QCOMPARE(controller.peopleCount(), 2);
    QCOMPARE(controller.peopleCount(), members->rowCount());
    QCOMPARE(memberResets.size(), 1);
    QVERIFY(selection.size() >= 1);
}

void ControllerTest::namesBurstFlushesTypingClearedByChat()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :message-tags\r\n"
                          ":server CAP omairc ACK :message-tags\r\n"
                          ":server 001 omairc :Welcome\r\n"));

    QSignalSpy typing(&controller, &IrcController::typingChanged);
    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#big\r\n"
                          ":server 353 omairc = #big :alice\r\n"
                          "@+typing=active :alice!u@h TAGMSG #big\r\n"));
    QCOMPARE(controller.typingNicks(), QStringList{QStringLiteral("alice")});
    QVERIFY(typing.size() >= 1);

    transport->injectBytes(
        QByteArrayLiteral(":alice!u@h PRIVMSG #big :hello\r\n"));
    QVERIFY(controller.typingNicks().isEmpty());
    const int typingBeforeNamesEnd = typing.size();

    transport->injectBytes(
        QByteArrayLiteral(":server 366 omairc #big :End of NAMES\r\n"));
    QVERIFY(controller.typingNicks().isEmpty());
    QVERIFY(typing.size() > typingBeforeNamesEnd);
}

void ControllerTest::chatDuringNamesUpdatesMessagesWithoutMemberReset()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :away-notify\r\n"
                          ":server CAP omairc ACK :away-notify\r\n"
                          ":server 001 omairc :Welcome\r\n"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    auto *members = qobject_cast<QAbstractItemModel *>(controller.members());
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#big\r\n"));
    QCOMPARE(controller.peopleCount(), members->rowCount());
    QCOMPARE(members->rowCount(), 1);

    QSignalSpy memberResets(members, &QAbstractItemModel::modelReset);
    transport->injectBytes(
        QByteArrayLiteral(":server 353 omairc = #big :omairc n0000 n0001 alice\r\n"));
    QCOMPARE(controller.peopleCount(), members->rowCount());
    QCOMPARE(members->rowCount(), 1);
    QCOMPARE(memberResets.size(), 0);

    const int rowsAfterJoin = messages->rowCount();
    transport->injectBytes(
        QByteArrayLiteral(":alice!u@h PRIVMSG #big :during-names\r\n"));
    QCOMPARE(messages->rowCount(), rowsAfterJoin + 1);
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::BodyRole),
             QStringLiteral("during-names"));
    QCOMPARE(controller.peopleCount(), members->rowCount());
    QCOMPARE(memberResets.size(), 0);

    QVERIFY(controller.sendMessage(QStringLiteral("local-during-names")));
    QCOMPARE(messages->rowCount(), rowsAfterJoin + 2);
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::BodyRole),
             QStringLiteral("local-during-names"));
    QCOMPARE(controller.peopleCount(), members->rowCount());
    QCOMPARE(memberResets.size(), 0);

    transport->injectBytes(
        QByteArrayLiteral(":server 366 omairc #big :End of NAMES\r\n"));
    QCOMPARE(members->rowCount(), 4);
    QCOMPARE(controller.peopleCount(), 4);
    QCOMPARE(controller.peopleCount(), members->rowCount());
    QCOMPARE(memberResets.size(), 1);
}

void ControllerTest::incomingNickRetargetsSelectedDirect()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :omairc Alice\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"
                          ":Alice!u@h PRIVMSG omairc :hi\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("Alice"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("Alice"));
    QCOMPARE(controller.currentNick(), QStringLiteral("omairc"));

    transport->injectBytes(QByteArrayLiteral(":Alice!u@h NICK :Alicia\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QCOMPARE(controller.selectedTarget(), QStringLiteral("Alicia"));
    QVERIFY(rowForTarget(conversations, QStringLiteral("Alice")) < 0);
    QVERIFY(rowForTarget(conversations, QStringLiteral("Alicia")) >= 0);
    QCOMPARE(messages->rowCount(), 2);
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::KindRole),
             QStringLiteral("event"));
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::BodyRole),
             QStringLiteral("Alice is now Alicia"));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *members = qobject_cast<QAbstractItemModel *>(controller.members());
    QCOMPARE(roleAt(members, 0, MemberListModel::NickRole), QStringLiteral("Alicia"));
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::BodyRole),
             QStringLiteral("Alice is now Alicia"));
    QCOMPARE(messages->rowCount(), 2);
    QCOMPARE(roleAt(messages, 0, MessageListModel::BodyRole),
             QStringLiteral("omairc joined"));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("Alicia"));
    QVERIFY(controller.sendMessage(QStringLiteral("hello")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG Alicia :hello\r\n"));

    transport->injectBytes(QByteArrayLiteral(":omairc!u@h NICK :fred\r\n"));
    QCOMPARE(session->nick(), QStringLiteral("fred"));
    QCOMPARE(controller.currentNick(), QStringLiteral("fred"));
}

void ControllerTest::incomingNickCaseOnlyRetargetsDirect()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":Alice!u@h PRIVMSG omairc :hi\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("Alice"));

    transport->injectBytes(QByteArrayLiteral(":Alice!u@h NICK :ALICE\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QCOMPARE(controller.selectedTarget(), QStringLiteral("ALICE"));
    QVERIFY(rowForTarget(conversations, QStringLiteral("ALICE")) >= 0);
    QVERIFY(controller.sendMessage(QStringLiteral("hello")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG ALICE :hello\r\n"));
}

void ControllerTest::welcomeAssignedNickRoutesDirectMessages()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSessionConfig sessionConfig = config(QStringLiteral("libera"));
    sessionConfig.nick = QStringLiteral("omairc-very-long-name");
    IrcSession *session = controller.addSession(sessionConfig, transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc-very-long-name LS :multi-prefix\r\n"
                          ":server 001 omairc-truncated :Welcome\r\n"
                          ":Alice!u@h PRIVMSG omairc-truncated :hi\r\n"
                          ":Bob!u@h PRIVMSG omairc-very-long-name :nope\r\n"));
    QCOMPARE(session->nick(), QStringLiteral("omairc-truncated"));
    QCOMPARE(controller.currentNick(), QStringLiteral("omairc-truncated"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("Alice"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QVERIFY(rowForTarget(conversations, QStringLiteral("Alice")) >= 0);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("Bob")), -1);
}

void ControllerTest::echoIfPresentUsesAssignedNick()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSessionConfig sessionConfig = config(QStringLiteral("libera"));
    sessionConfig.nick = QStringLiteral("omairc-very-long-name");
    IrcSession *session = controller.addSession(sessionConfig, transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc-very-long-name LS :multi-prefix\r\n"
                          ":server 001 omairc-truncated :Welcome\r\n"
                          ":omairc-truncated!u@h JOIN :#omarchy\r\n"
                          ":lena!u@h PRIVMSG omairc-truncated :hi\r\n"));
    QCOMPARE(session->nick(), QStringLiteral("omairc-truncated"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/msg lena later")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG lena :later\r\n"));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(messages->rowCount() > 0);
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::BodyRole),
             QStringLiteral("later"));
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::AuthorRole),
             QStringLiteral("omairc-truncated"));
}

void ControllerTest::echoMessageAckSkipsLocalPrivmsg()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :echo-message\r\n"
                          ":server CAP omairc ACK :echo-message\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :omairc Alice\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));
    QVERIFY(session->capabilities().contains(IrcCapability::EchoMessage));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    const int rowsAfterJoin = messages->rowCount();
    QVERIFY(rowsAfterJoin > 0);

    QVERIFY(controller.sendMessage(QStringLiteral("hello")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG #omarchy :hello\r\n"));
    QCOMPARE(messages->rowCount(), rowsAfterJoin);

    QVERIFY(controller.sendMessage(QStringLiteral("/me waves")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArray("PRIVMSG #omarchy :\x01" "ACTION waves\x01\r\n"));
    QCOMPARE(messages->rowCount(), rowsAfterJoin);

    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h PRIVMSG #omarchy :hello\r\n"));
    QCOMPARE(messages->rowCount(), rowsAfterJoin + 1);
    QCOMPARE(roleAt(messages, rowsAfterJoin, MessageListModel::BodyRole),
             QStringLiteral("hello"));
    QCOMPARE(roleAt(messages, rowsAfterJoin, MessageListModel::AuthorRole),
             QStringLiteral("omairc"));
}

void ControllerTest::echoMessageAbsentStillEchoesLocally()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :echo-message\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :omairc Alice\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));
    QVERIFY(!session->capabilities().contains(IrcCapability::EchoMessage));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    const int rowsAfterJoin = messages->rowCount();

    QVERIFY(controller.sendMessage(QStringLiteral("hello")));
    QCOMPARE(messages->rowCount(), rowsAfterJoin + 1);
    QCOMPARE(roleAt(messages, rowsAfterJoin, MessageListModel::BodyRole),
             QStringLiteral("hello"));
    QCOMPARE(roleAt(messages, rowsAfterJoin, MessageListModel::AuthorRole),
             QStringLiteral("omairc"));
}

void ControllerTest::echoMessageAckSkipsMsgEcho()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :echo-message\r\n"
                          ":server CAP omairc ACK :echo-message\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":lena!u@h PRIVMSG omairc :hi\r\n"));
    QVERIFY(session->capabilities().contains(IrcCapability::EchoMessage));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    const int rowsBeforeMsg = messages->rowCount();

    QVERIFY(controller.sendMessage(QStringLiteral("/msg lena later")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG lena :later\r\n"));
    QCOMPARE(messages->rowCount(), rowsBeforeMsg);

    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h PRIVMSG lena :later\r\n"));
    QCOMPARE(messages->rowCount(), rowsBeforeMsg + 1);
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::BodyRole),
             QStringLiteral("later"));
}

void ControllerTest::msgEchoDoesNotOpenMissingDirect()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :echo-message\r\n"
                          ":server CAP omairc ACK :echo-message\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    QVERIFY(session->capabilities().contains(IrcCapability::EchoMessage));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/msg lena hello")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG lena :hello\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h PRIVMSG lena :hello\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("lena")), -1);

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    for (int row = 0; row < messages->rowCount(); ++row) {
        QVERIFY(roleAt(messages, row, MessageListModel::BodyRole)
                != QStringLiteral("hello"));
    }
}

void ControllerTest::incomingNickservPrivmsgDoesNotOpenDirect()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":NickServ!NickServ@services PRIVMSG omairc "
                          ":This nickname is registered.\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("NickServ")), -1);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("nickserv")), -1);
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("This nickname is registered.")));
}

void ControllerTest::bouncerAttachOpensOnlyPeerAuthoredDirects()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch echo-message\r\n"
                          ":server CAP omairc ACK :batch echo-message\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    QVERIFY(session->capabilities().contains(IrcCapability::Batch));
    QVERIFY(session->capabilities().contains(IrcCapability::EchoMessage));
    QVERIFY(!session->capabilities().contains(IrcCapability::ChatHistory));

    IrcStatusConsole *console = controller.console();
    console->setOpen(true);
    QVERIFY(console->submit(QStringLiteral("/msg lena hi")));
    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h PRIVMSG lena :hi\r\n"
                          ":NickServ!NickServ@services PRIVMSG omairc "
                          ":This nickname is registered.\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("lena")), -1);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("NickServ")), -1);

    transport->injectBytes(
        QByteArrayLiteral(":znc.in BATCH +q1 znc.in/playback lena\r\n"
                          "@batch=q1 :omairc!u@h PRIVMSG lena :hi\r\n"
                          ":znc.in BATCH -q1\r\n"));
    QCOMPARE(rowForTarget(conversations, QStringLiteral("lena")), -1);

    transport->injectBytes(
        QByteArrayLiteral(":znc.in BATCH +q2 znc.in/playback dana\r\n"
                          "@batch=q2 :dana!u@h PRIVMSG omairc :morning\r\n"
                          "@batch=q2 :omairc!u@h PRIVMSG dana :morning back\r\n"
                          ":znc.in BATCH -q2\r\n"));
    QVERIFY(rowForTarget(conversations, QStringLiteral("dana")) >= 0);

    transport->injectBytes(
        QByteArrayLiteral(":rio!u@h PRIVMSG omairc :live hello\r\n"));
    QVERIFY(rowForTarget(conversations, QStringLiteral("rio")) >= 0);

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("dana"));
    auto *danaMessages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QCOMPARE(selectedBodies(danaMessages),
             QStringList({QStringLiteral("morning"),
                          QStringLiteral("morning back")}));
    QCOMPARE(roleAt(danaMessages, 0, MessageListModel::OriginRole).toString(),
             QStringLiteral("replay"));
    QCOMPARE(roleAt(danaMessages, 1, MessageListModel::OriginRole).toString(),
             QStringLiteral("replay"));

    transport->injectBytes(
        QByteArrayLiteral(
            ":znc.in BATCH +c znc.in/playback #omarchy\r\n"
            "@batch=c :***!znc@znc.in PRIVMSG #omarchy :Buffer Playback...\r\n"
            "@batch=c :lena!u@h PRIVMSG #omarchy :yesterday\r\n"
            "@batch=c :***!znc@znc.in PRIVMSG #omarchy :Playback Complete.\r\n"
            ":znc.in BATCH -c\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *roomMessages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QCOMPARE(selectedBodies(roomMessages),
             QStringList({QStringLiteral("yesterday"),
                          QStringLiteral("omairc joined")}));
}

void ControllerTest::bouncerQueryReplayDirectAppearsInConversationModel()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch\r\n"
                          ":server CAP omairc ACK :batch\r\n"
                          ":server 001 omairc :Welcome\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(conversations->rowCount(), 0);

    transport->injectBytes(
        QByteArrayLiteral(":znc.in BATCH +q znc.in/playback dana\r\n"
                          "@batch=q :dana!u@h PRIVMSG omairc :morning\r\n"
                          ":znc.in BATCH -q\r\n"));

    const int row = rowForTarget(conversations, QStringLiteral("dana"));
    QVERIFY(row >= 0);
    QCOMPARE(roleAt(conversations, row, ConversationListModel::ConversationRole)
                 .toString(),
             QStringLiteral("dana"));
    QCOMPARE(roleAt(conversations, row, ConversationListModel::DirectRole).toBool(),
             true);
    QCOMPARE(roleAt(conversations, row, ConversationListModel::UnreadRole).toInt(), 0);
}

void ControllerTest::pseudoClientPrivmsgOpensNoConversation()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":*status!znc@znc.in PRIVMSG omairc "
                          ":You have 1 network attached\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(conversations->rowCount(), 0);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("*status")), -1);
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("You have 1 network attached")));
}

void ControllerTest::pseudoClientReplayBatchOpensNoConversation()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch echo-message\r\n"
                          ":server CAP omairc ACK :batch echo-message\r\n"
                          ":server 001 omairc :Welcome\r\n"));

    // A bouncer keeps a query buffer for its own module, and our own commands
    // to it are echoed back inside that buffer. The sender guard cannot drop
    // those lines, because we are a routable sender, so creation has to fail
    // on the missing peer author instead.
    transport->injectBytes(
        QByteArrayLiteral(":znc.in BATCH +s znc.in/playback *status\r\n"
                          "@batch=s :omairc!u@h PRIVMSG *status :listnetworks\r\n"
                          "@batch=s :*status!znc@znc.in PRIVMSG omairc :Network: libera\r\n"
                          ":znc.in BATCH -s\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("*status")), -1);
    QCOMPARE(conversations->rowCount(), 0);
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("Network: libera")));
}

void ControllerTest::routableNicksOpenDirectsAndPseudoClientsDoNot()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":server 005 omairc CHANTYPES=# "
                          ":are supported by this server\r\n"
                          ":0day!u@h PRIVMSG omairc :one\r\n"
                          ":[bob]!u@h PRIVMSG omairc :two\r\n"
                          ":nick_!u@h PRIVMSG omairc :three\r\n"
                          ":{x}!u@h PRIVMSG omairc :four\r\n"
                          ":|away!u@h PRIVMSG omairc :five\r\n"
                          ":^bob^!u@h PRIVMSG omairc :six\r\n"
                          ":*status!znc@znc.in PRIVMSG omairc :seven\r\n"
                          ":*playback!znc@znc.in PRIVMSG omairc :eight\r\n"
                          ":***!znc@znc.in PRIVMSG omairc :nine\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QStringList opened;
    for (int row = 0; row < conversations->rowCount(); ++row) {
        opened.append(
            roleAt(conversations, row, ConversationListModel::ConversationRole)
                .toString());
    }
    opened.sort();
    QCOMPARE(opened,
             QStringList({QStringLiteral("0day"), QStringLiteral("[bob]"),
                          QStringLiteral("^bob^"), QStringLiteral("nick_"),
                          QStringLiteral("{x}"), QStringLiteral("|away")}));
}

void ControllerTest::statusMsgNickservIdentifyDoesNotOpenDirect()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :echo-message\r\n"
                          ":server CAP omairc ACK :echo-message\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    QVERIFY(session->capabilities().contains(IrcCapability::EchoMessage));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    IrcStatusConsole *console = controller.console();
    console->setOpen(true);
    QVERIFY(console->submit(
        QStringLiteral("/msg nickserv identify my_nick s3cret")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG nickserv :identify my_nick s3cret\r\n"));
    QVERIFY(console->isOpen());
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    transport->injectBytes(
        QByteArrayLiteral(":NickServ!NickServ@services PRIVMSG omairc "
                          ":You are now identified for my_nick.\r\n"
                          ":omairc!u@h PRIVMSG nickserv :identify my_nick s3cret\r\n"));

    QVERIFY(console->isOpen());
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("NickServ")), -1);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("nickserv")), -1);

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("s3cret")));
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("identify my_nick")));
    QVERIFY(logContains(console->lines(),
                        QStringLiteral("PRIVMSG nickserv :IDENTIFY ***")));
    QVERIFY(logContains(console->lines(), QStringLiteral("IDENTIFY ***")));
    QVERIFY(!logContains(console->lines(), QStringLiteral("s3cret")));
}

void ControllerTest::automaticIdentifyDoesNotOpenNickServDirect()
{
    IrcSessionConfig sessionConfig = config(QStringLiteral("libera"));
    sessionConfig.nickServPassword = QStringLiteral("nick-secret");
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(sessionConfig, transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :echo-message\r\n"
                          ":server CAP omairc ACK :echo-message\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":omairc!u@h PRIVMSG NickServ :IDENTIFY nick-secret\r\n"
                          ":NickServ!NickServ@services NOTICE omairc "
                          ":You are now identified.\r\n"));
    QVERIFY(transport->writtenFrames().contains(
        QByteArrayLiteral("PRIVMSG NickServ :IDENTIFY nick-secret\r\n")));
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("NickServ")), -1);
    QVERIFY(!logContains(controller.console()->lines(),
                         QStringLiteral("nick-secret")));
}

void ControllerTest::mentionArrivedOnSelectedBuffer()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    QSignalSpy spy(&controller, &IrcController::mentionArrived);
    transport->injectBytes(
        QByteArrayLiteral(":Alice!u@h PRIVMSG #omarchy :omairc: ping\r\n"));
    QCOMPARE(spy.count(), 1);
    QCOMPARE(spy.at(0).at(0).toString(), QStringLiteral("Alice"));
    QCOMPARE(spy.at(0).at(1).toString(), QStringLiteral("omairc: ping"));

    transport->injectBytes(
        QByteArrayLiteral(":Alice!u@h PRIVMSG #omarchy :hello\r\n"));
    QCOMPARE(spy.count(), 1);

    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h PRIVMSG #omarchy :omairc: self\r\n"));
    QCOMPARE(spy.count(), 1);

    transport->injectBytes(
        QByteArray(":Alice!u@h PRIVMSG #omarchy :\x01"
                   "ACTION pokes omairc\x01\r\n"));
    QCOMPARE(spy.count(), 2);
    QCOMPARE(spy.at(1).at(0).toString(), QStringLiteral("Alice"));
    QCOMPARE(spy.at(1).at(1).toString(), QStringLiteral("pokes omairc"));
}

void ControllerTest::mentionArrivedOnDirectMessageWithoutNick()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    QSignalSpy spy(&controller, &IrcController::mentionArrived);
    transport->injectBytes(
        QByteArrayLiteral(":Alice!u@h PRIVMSG omairc :hello\r\n"));
    QCOMPARE(spy.count(), 1);
    QCOMPARE(spy.at(0).at(0).toString(), QStringLiteral("Alice"));
    QCOMPARE(spy.at(0).at(1).toString(), QStringLiteral("hello"));

    transport->injectBytes(
        QByteArrayLiteral(":Alice!u@h PRIVMSG #omarchy :hello\r\n"));
    QCOMPARE(spy.count(), 1);

    transport->injectBytes(
        QByteArray(":Alice!u@h PRIVMSG omairc :\x01"
                   "ACTION waves\x01\r\n"));
    QCOMPARE(spy.count(), 2);
    QCOMPARE(spy.at(1).at(0).toString(), QStringLiteral("Alice"));
    QCOMPARE(spy.at(1).at(1).toString(), QStringLiteral("waves"));
}

void ControllerTest::chghostLeavesMemberNickAndRanks()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :chghost\r\n"
                          ":server CAP omairc ACK :chghost\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":server 005 omairc CHANTYPES=# PREFIX=(ov)@+ "
                          ":are supported by this server\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :@omairc +Alice\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));

    auto *members = qobject_cast<QAbstractItemModel *>(controller.members());
    QVERIFY(members);
    QCOMPARE(members->rowCount(), 2);
    QCOMPARE(roleAt(members, 0, MemberListModel::NickRole), QStringLiteral("Alice"));
    QCOMPARE(roleAt(members, 0, MemberListModel::LabelRole), QStringLiteral("+Alice"));

    transport->injectBytes(
        QByteArrayLiteral(":Alice!olduser@oldhost CHGHOST newuser newhost\r\n"
                          ":Alice!newuser@newhost CHGHOST\r\n"));
    QCOMPARE(members->rowCount(), 2);
    QCOMPARE(roleAt(members, 0, MemberListModel::NickRole), QStringLiteral("Alice"));
    QCOMPARE(roleAt(members, 0, MemberListModel::LabelRole), QStringLiteral("+Alice"));
}

void ControllerTest::twoSessionsStartTogether()
{
    IrcController controller;
    auto *transportA = new FakeIrcTransport;
    auto *transportB = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("network-a")), transportA));
    QVERIFY(controller.addSession(config(QStringLiteral("network-b")), transportB));
    QVERIFY(controller.start(QStringLiteral("network-a")));
    QVERIFY(controller.start(QStringLiteral("network-b")));
    QCOMPARE(transportA->connectionState(), IrcTransport::ConnectionState::Connecting);
    QCOMPARE(transportB->connectionState(), IrcTransport::ConnectionState::Connecting);

    registerSession(controller.session(QStringLiteral("network-a")), transportA);
    registerSession(controller.session(QStringLiteral("network-b")), transportB);
    transportA->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#alpha\r\n"));
    transportB->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#lab\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(conversations->rowCount(), 2);
    QCOMPARE(controller.session(QStringLiteral("network-a"))->state(),
             IrcSession::State::Registered);
    QCOMPARE(controller.session(QStringLiteral("network-b"))->state(),
             IrcSession::State::Registered);
}

void ControllerTest::startingBackgroundNetworkDoesNotStealStatus()
{
    IrcController controller;
    auto *transportA = new FakeIrcTransport;
    auto *transportB = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("network-a")), transportA));
    QVERIFY(controller.addSession(config(QStringLiteral("network-b")), transportB));
    QVERIFY(controller.start(QStringLiteral("network-a")));
    registerSession(controller.session(QStringLiteral("network-a")), transportA);
    transportA->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#chan\r\n"));
    controller.selectConversation(QStringLiteral("network-a"), QStringLiteral("#chan"));
    controller.openStatus(QStringLiteral("network-a"));
    QCOMPARE(controller.focusedNetworkId(), QStringLiteral("network-a"));
    QCOMPARE(controller.console()->networkId(), QStringLiteral("network-a"));
    QVERIFY(controller.console()->isOpen());

    QVERIFY(controller.start(QStringLiteral("network-b")));
    QCOMPARE(controller.focusedNetworkId(), QStringLiteral("network-a"));
    QCOMPARE(controller.console()->networkId(), QStringLiteral("network-a"));
    QVERIFY(controller.console()->isOpen());
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#chan"));
    QCOMPARE(controller.selectedNetworkId(), QStringLiteral("network-a"));
}

void ControllerTest::quitWhileReconnecting()
{
    IrcController controller;
    IrcSessionConfig sessionConfig = config(QStringLiteral("libera"));
    sessionConfig.reconnectEnabled = true;
    auto *transport = new FakeIrcTransport;
    auto *timer = new FakeReconnectTimer;
    IrcSession *session = controller.addSession(sessionConfig, transport, timer);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    transport->remoteClose();
    QCOMPARE(session->state(), IrcSession::State::Reconnecting);
    QVERIFY(timer->active);

    QVERIFY(controller.sendMessage(QStringLiteral("/quit")));
    QCOMPARE(session->state(), IrcSession::State::Idle);
    QVERIFY(!timer->active);
}

void ControllerTest::statusJoinUsesConsoleNetwork()
{
    IrcController controller;
    auto *transportA = new FakeIrcTransport;
    auto *transportB = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("network-a")), transportA));
    QVERIFY(controller.addSession(config(QStringLiteral("network-b")), transportB));
    registerSession(controller.session(QStringLiteral("network-a")), transportA);
    registerSession(controller.session(QStringLiteral("network-b")), transportB);
    transportA->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#chan\r\n"));
    controller.selectConversation(QStringLiteral("network-a"), QStringLiteral("#chan"));
    controller.openStatus(QStringLiteral("network-b"));
    QCOMPARE(controller.focusedNetworkId(), QStringLiteral("network-b"));
    QCOMPARE(controller.selectedNetworkId(), QStringLiteral("network-a"));

    QVERIFY(controller.console()->submit(QStringLiteral("/join #lab")));
    QCOMPARE(transportB->writtenFrames().last(),
             QByteArrayLiteral("JOIN #lab\r\n"));
    QVERIFY(!framesContain(transportA->writtenFrames(),
                           QByteArrayLiteral("JOIN #lab")));
}

void ControllerTest::implicitStatusPartStaysOnFocusedNetwork()
{
    IrcController controller;
    auto *transportA = new FakeIrcTransport;
    auto *transportB = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("network-a")), transportA));
    QVERIFY(controller.addSession(config(QStringLiteral("network-b")), transportB));
    registerSession(controller.session(QStringLiteral("network-a")), transportA);
    registerSession(controller.session(QStringLiteral("network-b")), transportB);
    transportA->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#chan\r\n"));
    transportB->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#lab\r\n"));
    controller.selectConversation(QStringLiteral("network-a"), QStringLiteral("#chan"));
    controller.openStatus(QStringLiteral("network-b"));

    QVERIFY(controller.console()->submit(QStringLiteral("/part")));
    QVERIFY(!framesContain(transportA->writtenFrames(), QByteArrayLiteral("PART")));
    QVERIFY(!framesContain(transportB->writtenFrames(), QByteArrayLiteral("PART")));

    QVERIFY(controller.console()->submit(QStringLiteral("/part #lab")));
    QCOMPARE(transportB->writtenFrames().last(),
             QByteArrayLiteral("PART #lab\r\n"));
    QVERIFY(!framesContain(transportA->writtenFrames(), QByteArrayLiteral("PART")));
}

void ControllerTest::implicitStatusKickStaysOnFocusedNetwork()
{
    IrcController controller;
    auto *transportA = new FakeIrcTransport;
    auto *transportB = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("network-a")), transportA));
    QVERIFY(controller.addSession(config(QStringLiteral("network-b")), transportB));
    registerSession(controller.session(QStringLiteral("network-a")), transportA);
    registerSession(controller.session(QStringLiteral("network-b")), transportB);
    transportA->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#chan\r\n"));
    transportB->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#lab\r\n"));
    controller.selectConversation(QStringLiteral("network-a"), QStringLiteral("#chan"));
    controller.openStatus(QStringLiteral("network-b"));

    QVERIFY(controller.console()->submit(QStringLiteral("/kick bob")));
    QVERIFY(!framesContain(transportA->writtenFrames(), QByteArrayLiteral("KICK")));
    QVERIFY(!framesContain(transportB->writtenFrames(), QByteArrayLiteral("KICK")));

    QVERIFY(controller.console()->submit(QStringLiteral("/kick #lab bob")));
    QCOMPARE(transportB->writtenFrames().last(),
             QByteArrayLiteral("KICK #lab bob\r\n"));
    QVERIFY(!framesContain(transportA->writtenFrames(), QByteArrayLiteral("KICK")));
}

void ControllerTest::selectConversationByIdUsesCompositeKey()
{
    IrcController controller;
    auto *transportA = new FakeIrcTransport;
    auto *transportB = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("network-a")), transportA));
    QVERIFY(controller.addSession(config(QStringLiteral("network-b")), transportB));
    registerSession(controller.session(QStringLiteral("network-a")), transportA);
    registerSession(controller.session(QStringLiteral("network-b")), transportB);
    transportA->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#chan\r\n"));
    transportB->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#chan\r\n"));

    controller.selectConversationById(QStringLiteral("network-b\n#chan"));
    QCOMPARE(controller.selectedNetworkId(), QStringLiteral("network-b"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#chan"));
    QCOMPARE(controller.selectedConversationId(), QStringLiteral("network-b\n#chan"));
    QVERIFY(!controller.console()->isOpen());

    controller.selectConversationById(QStringLiteral("#chan"));
    QCOMPARE(controller.selectedNetworkId(), QStringLiteral("network-b"));
    QCOMPARE(controller.selectedConversationId(), QStringLiteral("network-b\n#chan"));
}

void ControllerTest::forgetNetworkDropsGhostRowsAndLog()
{
    IrcController controller;
    auto *transportA = new FakeIrcTransport;
    auto *transportB = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("network-a")), transportA));
    QVERIFY(controller.addSession(config(QStringLiteral("network-b")), transportB));
    registerSession(controller.session(QStringLiteral("network-a")), transportA);
    registerSession(controller.session(QStringLiteral("network-b")), transportB);
    transportA->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#alpha\r\n"));
    transportB->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#lab\r\n"
                          "NOTICE AUTH :*** Looking up your hostname...\r\n"));
    controller.selectConversation(QStringLiteral("network-b"), QStringLiteral("#lab"));
    controller.openStatus(QStringLiteral("network-b"));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("Looking up your hostname")));

    QVERIFY(controller.discardSession(QStringLiteral("network-b")));
    controller.forgetNetworkState(QStringLiteral("network-b"));
    QCOMPARE(controller.session(QStringLiteral("network-b")), nullptr);

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(conversations->rowCount(), 1);
    QCOMPARE(roleAt(conversations, 0, ConversationListModel::ConversationIdRole),
             QStringLiteral("network-a\n#alpha"));
    QCOMPARE(controller.selectedTarget(), QString());
    QVERIFY(!logContains(controller.console()->lines(),
                         QStringLiteral("Looking up your hostname")));
}

void ControllerTest::backgroundChatBumpsConversationEpoch()
{
    IrcController controller;
    auto *transportA = new FakeIrcTransport;
    auto *transportB = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("network-a")), transportA));
    QVERIFY(controller.addSession(config(QStringLiteral("network-b")), transportB));
    QVERIFY(controller.start(QStringLiteral("network-a")));
    registerSession(controller.session(QStringLiteral("network-a")), transportA);
    transportA->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#alpha\r\n"));
    controller.selectConversation(QStringLiteral("network-a"), QStringLiteral("#alpha"));

    QVERIFY(controller.start(QStringLiteral("network-b")));
    registerSession(controller.session(QStringLiteral("network-b")), transportB);
    transportB->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#lab\r\n"
                          ":server 353 omairc = #lab :@omairc\r\n"
                          ":server 366 omairc #lab :End of NAMES\r\n"));

    const int epoch = controller.conversationEpoch();
    QCOMPARE(controller.unreadCountFor(QStringLiteral("network-b")), 0);
    QSignalSpy spy(&controller, &IrcController::conversationStateChanged);

    transportB->injectBytes(
        QByteArrayLiteral(":zed!u@h PRIVMSG #lab :ping\r\n"));

    QVERIFY(controller.conversationEpoch() > epoch);
    QVERIFY(spy.count() >= 1);
    QCOMPARE(controller.unreadCountFor(QStringLiteral("network-b")), 1);
    QVERIFY(!controller.mentionFor(QStringLiteral("network-b")));
}

void ControllerTest::chatHistoryBatchShowsBodyAndTime()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory\r\n"
                          ":server CAP omairc ACK :batch chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    transport->injectBytes(
        QByteArrayLiteral(
            ":irc.host BATCH +hx chathistory #omarchy\r\n"
            "@batch=hx;time=2011-10-19T16:40:51.620Z;msgid=old :alice!u@h PRIVMSG #omarchy :older\r\n"
            ":irc.host BATCH -hx\r\n"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QCOMPARE(roleAt(messages, 0, MessageListModel::BodyRole),
             QStringLiteral("older"));
    const QDateTime replayed = QDateTime::fromString(
        QStringLiteral("2011-10-19T16:40:51.620Z"), Qt::ISODateWithMs);
    QCOMPARE(roleAt(messages, 0, MessageListModel::TimeRole),
             replayed.toLocalTime().toString(QStringLiteral("HH:mm")));
    QCOMPARE(roleAt(messages, 0, MessageListModel::OriginRole),
             QStringLiteral("replay"));
    QCOMPARE(roleAt(messages, 1, MessageListModel::BodyRole),
             QStringLiteral("omairc joined"));
    QCOMPARE(controller.unreadCountFor(QStringLiteral("libera")), 0);
}

void ControllerTest::chatHistoryAndLiveTimeUseLocalWallClock()
{
    const ScopedTimeZone brisbane("Australia/Brisbane");
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory\r\n"
                          ":server CAP omairc ACK :batch chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    transport->injectBytes(
        QByteArrayLiteral(
            ":irc.host BATCH +hx chathistory #omarchy\r\n"
            "@batch=hx;time=2026-09-11T03:30:15.000Z;msgid=old "
            ":alice!u@h PRIVMSG #omarchy :[13:30:15] hello\r\n"
            ":irc.host BATCH -hx\r\n"
            "@time=2026-09-11T03:30:15.000Z :bob!u@h PRIVMSG #omarchy :live now\r\n"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QCOMPARE(roleAt(messages, 0, MessageListModel::BodyRole),
             QStringLiteral("[13:30:15] hello"));
    QCOMPARE(roleAt(messages, 0, MessageListModel::OriginRole),
             QStringLiteral("replay"));
    QCOMPARE(roleAt(messages, 0, MessageListModel::TimeRole),
             QStringLiteral("13:30"));
    QCOMPARE(roleAt(messages, 1, MessageListModel::BodyRole),
             QStringLiteral("omairc joined"));
    QCOMPARE(roleAt(messages, 2, MessageListModel::BodyRole),
             QStringLiteral("live now"));
    QCOMPARE(roleAt(messages, 2, MessageListModel::OriginRole),
             QStringLiteral("live"));
    QCOMPARE(roleAt(messages, 2, MessageListModel::TimeRole),
             QStringLiteral("13:30"));
}

void ControllerTest::whoisFromChannelCopiesStatusLinesAsEvents()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                 transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/whois lena")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("WHOIS lena lena\r\n"));

    transport->injectBytes(
        QByteArrayLiteral(":irc 311 omairc lena ~lena user/host * :Lena\r\n"
                          ":irc 319 omairc lena :#omarchy\r\n"
                          ":irc 318 omairc lena :End of /WHOIS list.\r\n"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    const QStringList expected = {
        QStringLiteral("lena is ~lena@user/host (Lena)"),
        QStringLiteral("lena is on #omarchy"),
        QStringLiteral("End of WHOIS for lena"),
    };
    QCOMPARE(selectedBodies(messages).mid(selectedBodies(messages).size() - 3),
             expected);
    for (const QString& body : expected) {
        QVERIFY(hasWhoisBody(messages, body));
        QVERIFY(logContains(controller.console()->lines(), body));
    }
}

void ControllerTest::whoisInterleavesByAskingBuffer()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                 transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":omairc!u@h JOIN :#help\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("/whois Lena")));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#help"));
    QVERIFY(controller.sendMessage(QStringLiteral("/whois sam")));

    transport->injectBytes(
        QByteArrayLiteral(":irc 311 omairc sam ~s h * :Sam\r\n"
                          ":irc 311 omairc lena ~l h * :Lena\r\n"
                          ":irc 318 omairc lena :End of WHOIS\r\n"
                          ":irc 318 omairc sam :End of WHOIS\r\n"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QCOMPARE(controller.unreadCountFor(QStringLiteral("libera")), 0);

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("lena is ~l@h (Lena)")));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("End of WHOIS for lena")));
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("sam")));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#help"));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("sam is ~s@h (Sam)")));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("End of WHOIS for sam")));
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("lena")));

    auto *lines = controller.console()->lines();
    QVERIFY(logContains(lines, QStringLiteral("lena is ~l@h (Lena)")));
    QVERIFY(logContains(lines, QStringLiteral("End of WHOIS for lena")));
    QVERIFY(logContains(lines, QStringLiteral("sam is ~s@h (Sam)")));
    QVERIFY(logContains(lines, QStringLiteral("End of WHOIS for sam")));
}

void ControllerTest::statusWhoisSupersedesConversationWatch()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                 transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("/whois lena")));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    const QStringList before = selectedBodies(messages);

    controller.openStatus(QStringLiteral("libera"));
    QVERIFY(controller.console()->submit(QStringLiteral("/whois lena")));

    transport->injectBytes(
        QByteArrayLiteral(":irc 311 omairc lena ~lena user/host * :Lena\r\n"
                          ":irc 318 omairc lena :End of /WHOIS list.\r\n"));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QCOMPARE(selectedBodies(messages), before);
    QVERIFY(!selectedBodiesContain(messages,
                                   QStringLiteral("lena is ~lena@user/host (Lena)")));
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("End of WHOIS for lena")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("lena is ~lena@user/host (Lena)")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("End of WHOIS for lena")));
}

void ControllerTest::unsolicitedWhoisStaysOnStatus()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                 transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :away-notify\r\n"
                          ":server CAP omairc ACK :away-notify\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :@omairc Alice\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    auto *members = qobject_cast<QAbstractItemModel *>(controller.members());
    QVERIFY(messages);
    QVERIFY(members);
    const QStringList before = selectedBodies(messages);
    QCOMPARE(roleAt(members, 0, MemberListModel::NickRole), QStringLiteral("Alice"));
    QCOMPARE(roleAt(members, 0, MemberListModel::AwayRole), false);

    transport->injectBytes(
        QByteArrayLiteral(":irc 311 omairc lena ~lena user/host * :Lena\r\n"
                          ":irc 301 omairc Alice :gone fishing\r\n"
                          ":irc 401 omairc missing :No such nick/channel\r\n"));

    QCOMPARE(selectedBodies(messages), before);
    QVERIFY(!selectedBodiesContain(messages,
                                   QStringLiteral("lena is ~lena@user/host (Lena)")));
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("Alice is away: gone fishing")));
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("No such nick: missing")));
    QCOMPARE(roleAt(members, 0, MemberListModel::AwayRole), false);
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("lena is ~lena@user/host (Lena)")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("Alice is away: gone fishing")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("No such nick: missing")));
}

void ControllerTest::emptyChannelWhoisNamesANick()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                 transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    const int before = transport->writtenFrames().size();
    QVERIFY(!controller.sendMessage(QStringLiteral("/whois")));
    QCOMPARE(controller.lastError(), QStringLiteral("Name a nick"));
    QCOMPARE(transport->writtenFrames().size(), before);
    QVERIFY(!framesContain(transport->writtenFrames().mid(before),
                           QByteArrayLiteral("WHOIS")));
}

void ControllerTest::emptyDirectWhoisDefaultsAndRoutes()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                 transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":lena!u@h PRIVMSG omairc :hi\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));

    QVERIFY(controller.sendMessage(QStringLiteral("/whois")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("WHOIS lena lena\r\n"));

    transport->injectBytes(
        QByteArrayLiteral(":irc 311 omairc lena ~lena user/host * :Lena\r\n"
                          ":irc 318 omairc lena :End of /WHOIS list.\r\n"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(hasWhoisBody(messages, QStringLiteral("lena is ~lena@user/host (Lena)")));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("End of WHOIS for lena")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("lena is ~lena@user/host (Lena)")));
}

void ControllerTest::terminalWhoisClearsWatch()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                 transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);

    QVERIFY(controller.sendMessage(QStringLiteral("/whois lena")));
    transport->injectBytes(
        QByteArrayLiteral(":irc 318 omairc lena :End of /WHOIS list.\r\n"));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("End of WHOIS for lena")));
    const QStringList after318 = selectedBodies(messages);
    transport->injectBytes(
        QByteArrayLiteral(":irc 311 omairc lena ~lena user/host * :Lena\r\n"));
    QCOMPARE(selectedBodies(messages), after318);
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("lena is ~lena@user/host (Lena)")));

    QVERIFY(controller.sendMessage(QStringLiteral("/whois missing")));
    transport->injectBytes(
        QByteArrayLiteral(":irc 401 omairc missing :No such nick/channel\r\n"));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("No such nick: missing")));
    const QStringList after401 = selectedBodies(messages);
    transport->injectBytes(
        QByteArrayLiteral(":irc 311 omairc missing ~m h * :Missing\r\n"));
    QCOMPARE(selectedBodies(messages), after401);
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("missing is ~m@h (Missing)")));

    QVERIFY(controller.sendMessage(QStringLiteral("/whois ghost")));
    transport->injectBytes(
        QByteArrayLiteral(":irc 402 omairc ghost :No such server\r\n"));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("No such server: ghost")));
    const QStringList after402 = selectedBodies(messages);
    transport->injectBytes(
        QByteArrayLiteral(":irc 311 omairc ghost ~g h * :Ghost\r\n"));
    QCOMPARE(selectedBodies(messages), after402);
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("ghost is ~g@h (Ghost)")));
}

void ControllerTest::failedPrivmsgDoesNotStealWhoisWatch()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                 transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);

    QVERIFY(controller.sendMessage(QStringLiteral("/whois lena")));
    QVERIFY(controller.sendMessage(QStringLiteral("/msg lena hello")));
    QVERIFY(controller.sendMessage(QStringLiteral("/msg lena again")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG lena :again\r\n"));

    transport->injectBytes(
        QByteArrayLiteral(":irc 401 omairc lena :No such nick/channel\r\n"
                          ":irc 401 omairc lena :No such nick/channel\r\n"));
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("No such nick: lena")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("No such nick: lena")));

    transport->injectBytes(
        QByteArrayLiteral(":irc 311 omairc lena ~lena user/host * :Lena\r\n"
                          ":irc 318 omairc lena :End of /WHOIS list.\r\n"));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("lena is ~lena@user/host (Lena)")));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("End of WHOIS for lena")));
}

void ControllerTest::whoisFailureBeforeDeliveryDoesNotStealWatch()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                 transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);

    QVERIFY(controller.sendMessage(QStringLiteral("/whois lena")));
    QVERIFY(controller.sendMessage(QStringLiteral("/msg lena hello")));

    transport->injectBytes(
        QByteArrayLiteral(":irc 401 omairc lena :No such nick/channel\r\n"
                          ":irc 401 omairc lena :No such nick/channel\r\n"));
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("No such nick: lena")));

    transport->injectBytes(
        QByteArrayLiteral(":irc 311 omairc lena ~lena user/host * :Lena\r\n"
                          ":irc 318 omairc lena :End of /WHOIS list.\r\n"));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("lena is ~lena@user/host (Lena)")));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("End of WHOIS for lena")));
}

void ControllerTest::closedDirectWhoisDoesNotResurrect()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                 transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":lena!u@h PRIVMSG omairc :hi\r\n"));
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QVERIFY(rowForTarget(conversations, QStringLiteral("lena")) >= 0);

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    QVERIFY(controller.sendMessage(QStringLiteral("/whois")));
    controller.closeDirectMessage();
    QVERIFY(rowForTarget(conversations, QStringLiteral("lena")) < 0);
    const int rows = conversations->rowCount();

    transport->injectBytes(
        QByteArrayLiteral(":irc 311 omairc lena ~lena user/host * :Lena\r\n"
                          ":irc 318 omairc lena :End of /WHOIS list.\r\n"));

    QCOMPARE(conversations->rowCount(), rows);
    QVERIFY(rowForTarget(conversations, QStringLiteral("lena")) < 0);
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(!selectedBodiesContain(messages,
                                   QStringLiteral("lena is ~lena@user/host (Lena)")));
}

void ControllerTest::whoisEventDoesNotCollapseWithJoin()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                 transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("/whois lena")));
    transport->injectBytes(
        QByteArrayLiteral(":irc 311 omairc lena ~lena user/host * :Lena\r\n"
                          ":irc 318 omairc lena :End of /WHOIS list.\r\n"
                          ":alice!u@h JOIN :#omarchy\r\n"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(hasWhoisBody(messages, QStringLiteral("lena is ~lena@user/host (Lena)")));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("End of WHOIS for lena")));
    QVERIFY(hasEventBody(messages, QStringLiteral("alice joined")));
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("End of WHOIS for lena, alice joined")));
    QCOMPARE(bodyRow(messages, QStringLiteral("End of WHOIS for lena")) + 1,
             bodyRow(messages, QStringLiteral("alice joined")));
}

int runControllerTests(int argc, char **argv)
{
    ControllerTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_controller.moc"
