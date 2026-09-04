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

int runControllerTests(int argc, char **argv)
{
    ControllerTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_controller.moc"
