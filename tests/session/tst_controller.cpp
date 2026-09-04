#include <QAbstractItemModel>
#include <QTest>

#include "fakeirctransport.h"
#include "irccontroller.h"
#include "memberlistmodel.h"
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
}

class ControllerTest : public QObject
{
    Q_OBJECT

private slots:
    void reducesTrafficAndRoutesOutboundByNetwork();
    void liberaConnectCreatesChannelNotAuthDirect();
    void emptyNetworkIdDoesNotSwitch();
    void presenceCapabilitiesGateAwayAndStatus();
    void defaultPrefixPaintsLabelNotNick();
    void channelCloseSlashIsWrongScope();
    void partDefaultsToSelectedChannel();
    void partFromDirectIsWrongScope();
    void statusPartDefaultsToSelectedChannel();
    void partImplicitUsesSelectedSession();
    void closeDirectMessageDropsAndSelectsNeighbor();
    void closeDirectMessageInvokableOnChannelIsSilent();
    void closeDirectMessageWhileDisconnected();
    void selfAwayFollowsNumericsAndUnawaysAfterChat();
    void closeLastDirectKeepsSelfAway();
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

int runControllerTests(int argc, char **argv)
{
    ControllerTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_controller.moc"
