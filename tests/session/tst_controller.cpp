#include <QAbstractItemModel>
#include <QTest>

#include "fakeirctransport.h"
#include "irccontroller.h"

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
}

class ControllerTest : public QObject
{
    Q_OBJECT

private slots:
    void reducesTrafficAndRoutesOutboundByNetwork();
    void liberaConnectCreatesChannelNotAuthDirect();
    void emptyNetworkIdDoesNotSwitch();
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
    QCOMPARE(conversations->rowCount(), 2);

    const QString first =
        roleAt(conversations, 0, ConversationListModel::ConversationRole).toString();
    const QString second =
        roleAt(conversations, 1, ConversationListModel::ConversationRole).toString();
    const int omarchyRow = first == QStringLiteral("#omarchy") ? 0 : 1;
    const int authRow = omarchyRow == 0 ? 1 : 0;
    QCOMPARE(roleAt(conversations, omarchyRow, ConversationListModel::ConversationRole),
             QStringLiteral("#omarchy"));
    QCOMPARE(roleAt(conversations, omarchyRow, ConversationListModel::DirectRole), false);
    QCOMPARE(roleAt(conversations, omarchyRow, ConversationListModel::NetworkIdRole),
             QStringLiteral("libera"));
    QCOMPARE(roleAt(conversations, authRow, ConversationListModel::ConversationRole),
             QStringLiteral("AUTH"));
    QCOMPARE(roleAt(conversations, authRow, ConversationListModel::DirectRole), true);

    controller.selectConversation(
        roleAt(conversations, omarchyRow, ConversationListModel::NetworkIdRole).toString(),
        QStringLiteral("#omarchy"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QVERIFY(controller.isChannel());
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

int runControllerTests(int argc, char **argv)
{
    ControllerTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_controller.moc"
