#include <QAbstractItemModel>
#include <QSignalSpy>
#include <QTest>

#include "fakeirctransport.h"
#include "irccontroller.h"
#include "memberlistmodel.h"
#include "messagelistmodel.h"
#include "networklogmodel.h"

namespace
{
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
    void emptyNetworkIdDoesNotSwitch();
    void presenceCapabilitiesGateAwayAndStatus();
    void defaultPrefixPaintsLabelNotNick();
    void channelCloseSlashIsWrongScope();
    void partDefaultsToSelectedChannel();
    void partFromDirectIsWrongScope();
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
    void awayWaitsForNumericThenChatUnaways();
    void statusQueryClosesStatus();
    void statusQueryChannelStaysOpen();
    void disconnectedQuerySelectsBareNotText();
    void statusQueryWithNoNetworkDoesNotCrash();
    void conversationClearWipesMessages();
    void ghostClearIsSent();
    void statusClearLeavesConversationMessages();
    void largeChannelJoinDoesNotResetModelsPerNick();
    void otherChannelNamesDoesNotSnapshotJoiningMembers();
    void namesBurstFlushesTypingClearedByChat();
    void chatDuringNamesUpdatesMessagesWithoutMemberReset();
    void incomingNickRetargetsSelectedDirect();
    void incomingNickCaseOnlyRetargetsDirect();
    void welcomeAssignedNickRoutesDirectMessages();
    void echoIfPresentUsesAssignedNick();
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
    QCOMPARE(messageResets.size(), 2);
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

int runControllerTests(int argc, char **argv)
{
    ControllerTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_controller.moc"
