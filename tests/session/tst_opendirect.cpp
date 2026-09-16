#include <QAbstractItemModel>
#include <QCoreApplication>
#include <QFile>
#include <QSettings>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QTest>

#include "conversationlistmodel.h"
#include "fakeirctransport.h"
#include "irccontroller.h"
#include "ircopendirect.h"
#include "messagelistmodel.h"

#include <memory>

namespace
{
IrcSessionConfig sessionConfig(const QString& networkId = QStringLiteral("libera"))
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

void welcome(FakeIrcTransport *transport)
{
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"));
}

int rowForTarget(const QAbstractItemModel *model, const QString& target)
{
    for (int row = 0; row < model->rowCount(); ++row) {
        const QString name = model->data(model->index(row, 0),
                                         ConversationListModel::ConversationRole)
                                 .toString();
        if (name == target)
            return row;
    }
    return -1;
}

QVariant roleAt(QAbstractItemModel *model, const QString& target, int role)
{
    const int row = rowForTarget(model, target);
    if (row < 0)
        return {};
    return model->data(model->index(row, 0), role);
}

bool messagesContain(QAbstractItemModel *messages, const QString& body)
{
    for (int row = 0; row < messages->rowCount(); ++row) {
        if (messages->data(messages->index(row, 0), MessageListModel::BodyRole)
                .toString()
            == body) {
            return true;
        }
    }
    return false;
}
}

class OpenDirectTest : public QObject
{
    Q_OBJECT

private slots:
    void init();
    void storeRoundTripsAndIsIdempotent();
    void storeRekeysDisplayAndDropsDuplicate();
    void storeForgetDropsTheGroup();
    void defaultReopensOn();
    void openAndInboundPersist();
    void closeDropsPersist();
    void reopenRestoresHistoryWithoutUnread();
    void closedDirectStaysClosed();
    void settingOffRestoresNothing();
    void nickChangeRekeysStoredTarget();
    void servicesAreSkipped();
    void staleTargetStillOpens();
    void forgetNetworkDropsStoredDirects();
    void turningOnRestoresLiveSession();

private:
    QString settingsFile() const;

    std::unique_ptr<QTemporaryDir> m_dir;
};

void OpenDirectTest::init()
{
    m_dir = std::make_unique<QTemporaryDir>();
    QVERIFY(m_dir->isValid());
    qputenv("XDG_CONFIG_HOME", m_dir->path().toUtf8());
    QSettings::setPath(QSettings::NativeFormat, QSettings::UserScope, m_dir->path());
    QCoreApplication::setOrganizationName(QStringLiteral("omairc"));
    QCoreApplication::setApplicationName(QStringLiteral("omairc"));
}

QString OpenDirectTest::settingsFile() const
{
    QSettings settings;
    return settings.fileName();
}

void OpenDirectTest::storeRoundTripsAndIsIdempotent()
{
    const IrcCaseMapping mapping;
    IrcOpenDirectStore store;
    QVERIFY(store.add(QStringLiteral("net-a"), QStringLiteral("lena"), mapping));
    QVERIFY(!store.add(QStringLiteral("net-a"), QStringLiteral("Lena"), mapping));
    QCOMPARE(store.targets(QStringLiteral("net-a")),
             QStringList{QStringLiteral("lena")});
    QVERIFY(store.contains(QStringLiteral("net-a"), QStringLiteral("LENA"), mapping));
    QVERIFY(!store.remove(QStringLiteral("net-a"), QStringLiteral("bob"), mapping));
    QVERIFY(store.remove(QStringLiteral("net-a"), QStringLiteral("lena"), mapping));
    QVERIFY(store.targets(QStringLiteral("net-a")).isEmpty());
}

void OpenDirectTest::storeRekeysDisplayAndDropsDuplicate()
{
    const IrcCaseMapping mapping;
    IrcOpenDirectStore store;
    QVERIFY(store.add(QStringLiteral("net-a"), QStringLiteral("Alice"), mapping));
    QVERIFY(store.rekey(QStringLiteral("net-a"), QStringLiteral("alice"),
                        QStringLiteral("Alicia"), mapping));
    QCOMPARE(store.targets(QStringLiteral("net-a")),
             QStringList{QStringLiteral("Alicia")});
    QVERIFY(store.add(QStringLiteral("net-a"), QStringLiteral("bob"), mapping));
    QVERIFY(store.rekey(QStringLiteral("net-a"), QStringLiteral("Alicia"),
                        QStringLiteral("bob"), mapping));
    QCOMPARE(store.listed(QStringLiteral("net-a"), mapping),
             QStringList{QStringLiteral("bob")});
}

void OpenDirectTest::storeForgetDropsTheGroup()
{
    const IrcCaseMapping mapping;
    IrcOpenDirectStore store;
    QVERIFY(store.add(QStringLiteral("drop"), QStringLiteral("lena"), mapping));
    QVERIFY(store.add(QStringLiteral("keep"), QStringLiteral("bob"), mapping));
    store.forget(QStringLiteral("drop"));
    QVERIFY(!store.contains(QStringLiteral("drop"), QStringLiteral("lena"), mapping));
    QVERIFY(store.contains(QStringLiteral("keep"), QStringLiteral("bob"), mapping));

    QFile file(settingsFile());
    QVERIFY(file.open(QIODevice::ReadOnly | QIODevice::Text));
    const QString contents = QString::fromUtf8(file.readAll());
    QVERIFY(!contents.contains(QLatin1String("lena")));
    QVERIFY(contents.contains(QLatin1String("bob")));
}

void OpenDirectTest::defaultReopensOn()
{
    QCOMPARE(IrcController().reopenDirectMessages(), true);
}

void OpenDirectTest::openAndInboundPersist()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    controller.openDirectMessage(QStringLiteral("anna"));
    transport->injectBytes(
        QByteArrayLiteral(":zed!u@h PRIVMSG omairc :hello\r\n"));

    const IrcCaseMapping mapping;
    QVERIFY(IrcOpenDirectStore().contains(
        QStringLiteral("libera"), QStringLiteral("anna"), mapping));
    QVERIFY(IrcOpenDirectStore().contains(
        QStringLiteral("libera"), QStringLiteral("zed"), mapping));
    QVERIFY(!IrcOpenDirectStore().contains(
        QStringLiteral("libera"), QStringLiteral("#omarchy"), mapping));
}

void OpenDirectTest::closeDropsPersist()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    transport->injectBytes(
        QByteArrayLiteral(":lena!u@h PRIVMSG omairc :hi\r\n"
                          ":zed!u@h PRIVMSG omairc :later\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    controller.closeDirectMessage();

    const IrcCaseMapping mapping;
    QVERIFY(!IrcOpenDirectStore().contains(
        QStringLiteral("libera"), QStringLiteral("lena"), mapping));
    QVERIFY(IrcOpenDirectStore().contains(
        QStringLiteral("libera"), QStringLiteral("zed"), mapping));
}

void OpenDirectTest::reopenRestoresHistoryWithoutUnread()
{
    QTemporaryDir state;
    QVERIFY(state.isValid());
    const QString root = state.filePath(QStringLiteral("omairc/logs"));

    {
        IrcController controller;
        controller.setTranscriptRoot(root);
        auto *transport = new FakeIrcTransport;
        QVERIFY(controller.addSession(sessionConfig(), transport));
        QVERIFY(controller.start(QStringLiteral("libera")));
        welcome(transport);
        transport->injectBytes(
            QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"
                              ":alice!u@h PRIVMSG omairc :ping\r\n"
                              ":bob!u@h PRIVMSG omairc :later\r\n"));
        controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    }

    IrcController reloaded;
    reloaded.setTranscriptRoot(root);
    auto *transport = new FakeIrcTransport;
    QSignalSpy mentions(&reloaded, &IrcController::mentionArrived);
    QVERIFY(reloaded.addSession(sessionConfig(), transport));
    QVERIFY(reloaded.start(QStringLiteral("libera")));
    welcome(transport);

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(reloaded.conversations());
    QVERIFY(conversations);
    QVERIFY(rowForTarget(conversations, QStringLiteral("alice")) >= 0);
    QVERIFY(rowForTarget(conversations, QStringLiteral("bob")) >= 0);
    QCOMPARE(roleAt(conversations, QStringLiteral("alice"),
                    ConversationListModel::UnreadRole).toInt(),
             0);
    QCOMPARE(roleAt(conversations, QStringLiteral("bob"),
                    ConversationListModel::UnreadRole).toInt(),
             0);
    QVERIFY(!roleAt(conversations, QStringLiteral("alice"),
                    ConversationListModel::MentionRole).toBool());
    QCOMPARE(mentions.count(), 0);

    reloaded.selectConversation(QStringLiteral("libera"), QStringLiteral("alice"));
    auto *messages = qobject_cast<QAbstractItemModel *>(reloaded.messages());
    QVERIFY(messagesContain(messages, QStringLiteral("ping")));
    const int ping = [&] {
        for (int row = 0; row < messages->rowCount(); ++row) {
            if (messages->data(messages->index(row, 0), MessageListModel::BodyRole)
                    .toString()
                == QStringLiteral("ping")) {
                return row;
            }
        }
        return -1;
    }();
    QVERIFY(ping >= 0);
    QCOMPARE(messages->data(messages->index(ping, 0), MessageListModel::OriginRole)
                 .toString(),
             QStringLiteral("replay"));
    QCOMPARE(mentions.count(), 0);
}

void OpenDirectTest::closedDirectStaysClosed()
{
    {
        IrcController controller;
        auto *transport = new FakeIrcTransport;
        QVERIFY(controller.addSession(sessionConfig(), transport));
        QVERIFY(controller.start(QStringLiteral("libera")));
        welcome(transport);
        transport->injectBytes(
            QByteArrayLiteral(":alice!u@h PRIVMSG omairc :a\r\n"
                              ":bob!u@h PRIVMSG omairc :b\r\n"));
        controller.selectConversation(QStringLiteral("libera"), QStringLiteral("alice"));
        controller.closeDirectMessage();
    }

    IrcController reloaded;
    auto *transport = new FakeIrcTransport;
    QVERIFY(reloaded.addSession(sessionConfig(), transport));
    QVERIFY(reloaded.start(QStringLiteral("libera")));
    welcome(transport);
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(reloaded.conversations());
    QVERIFY(rowForTarget(conversations, QStringLiteral("alice")) < 0);
    QVERIFY(rowForTarget(conversations, QStringLiteral("bob")) >= 0);
}

void OpenDirectTest::settingOffRestoresNothing()
{
    {
        IrcController controller;
    QVERIFY(controller.reopenDirectMessages());
    controller.setReopenDirectMessages(false);
        QVERIFY(!controller.reopenDirectMessages());
        auto *transport = new FakeIrcTransport;
        QVERIFY(controller.addSession(sessionConfig(), transport));
        QVERIFY(controller.start(QStringLiteral("libera")));
        welcome(transport);
        transport->injectBytes(
            QByteArrayLiteral(":alice!u@h PRIVMSG omairc :hi\r\n"));
        QVERIFY(IrcOpenDirectStore().contains(
            QStringLiteral("libera"), QStringLiteral("alice"), IrcCaseMapping()));
    }

    IrcController reloaded;
    QVERIFY(!reloaded.reopenDirectMessages());
    auto *transport = new FakeIrcTransport;
    QVERIFY(reloaded.addSession(sessionConfig(), transport));
    QVERIFY(reloaded.start(QStringLiteral("libera")));
    welcome(transport);
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(reloaded.conversations());
    QVERIFY(rowForTarget(conversations, QStringLiteral("alice")) < 0);
}

void OpenDirectTest::nickChangeRekeysStoredTarget()
{
    {
        IrcController controller;
        auto *transport = new FakeIrcTransport;
        QVERIFY(controller.addSession(sessionConfig(), transport));
        QVERIFY(controller.start(QStringLiteral("libera")));
        welcome(transport);
        transport->injectBytes(
            QByteArrayLiteral(":alice!u@h PRIVMSG omairc :hi\r\n"
                              ":alice!u@h NICK :alicia\r\n"));
        QCOMPARE(controller.selectedTarget(), QStringLiteral("alicia"));
        QVERIFY(IrcOpenDirectStore().contains(
            QStringLiteral("libera"), QStringLiteral("alicia"), IrcCaseMapping()));
        QVERIFY(!IrcOpenDirectStore().contains(
            QStringLiteral("libera"), QStringLiteral("alice"), IrcCaseMapping()));
    }

    IrcController reloaded;
    auto *transport = new FakeIrcTransport;
    QVERIFY(reloaded.addSession(sessionConfig(), transport));
    QVERIFY(reloaded.start(QStringLiteral("libera")));
    welcome(transport);
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(reloaded.conversations());
    QVERIFY(rowForTarget(conversations, QStringLiteral("alicia")) >= 0);
    QVERIFY(rowForTarget(conversations, QStringLiteral("alice")) < 0);
}

void OpenDirectTest::servicesAreSkipped()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    controller.openDirectMessage(QStringLiteral("NickServ"));
    transport->injectBytes(
        QByteArrayLiteral(":NickServ!NickServ@services. NOTICE omairc "
                          ":This nickname is registered.\r\n"
                          ":alice!u@h PRIVMSG omairc :hi\r\n"));

    const IrcCaseMapping mapping;
    QVERIFY(!IrcOpenDirectStore().contains(
        QStringLiteral("libera"), QStringLiteral("NickServ"), mapping));
    QVERIFY(IrcOpenDirectStore().contains(
        QStringLiteral("libera"), QStringLiteral("alice"), mapping));

    IrcOpenDirectStore().add(QStringLiteral("libera"), QStringLiteral("ChanServ"),
                             mapping);

    IrcController reloaded;
    auto *next = new FakeIrcTransport;
    QVERIFY(reloaded.addSession(sessionConfig(), next));
    QVERIFY(reloaded.start(QStringLiteral("libera")));
    welcome(next);
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(reloaded.conversations());
    QVERIFY(rowForTarget(conversations, QStringLiteral("NickServ")) < 0);
    QVERIFY(rowForTarget(conversations, QStringLiteral("ChanServ")) < 0);
    QVERIFY(rowForTarget(conversations, QStringLiteral("alice")) >= 0);
    QVERIFY(!IrcOpenDirectStore().contains(
        QStringLiteral("libera"), QStringLiteral("ChanServ"), mapping));
}

void OpenDirectTest::staleTargetStillOpens()
{
    IrcOpenDirectStore().add(QStringLiteral("libera"), QStringLiteral("ghost"),
                             IrcCaseMapping());
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(rowForTarget(conversations, QStringLiteral("ghost")) >= 0);
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("ghost"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QCOMPARE(messages->rowCount(), 0);
}

void OpenDirectTest::forgetNetworkDropsStoredDirects()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    transport->injectBytes(
        QByteArrayLiteral(":alice!u@h PRIVMSG omairc :hi\r\n"));
    QVERIFY(IrcOpenDirectStore().contains(
        QStringLiteral("libera"), QStringLiteral("alice"), IrcCaseMapping()));
    controller.forgetNetworkState(QStringLiteral("libera"));
    QVERIFY(!IrcOpenDirectStore().contains(
        QStringLiteral("libera"), QStringLiteral("alice"), IrcCaseMapping()));
}

void OpenDirectTest::turningOnRestoresLiveSession()
{
    IrcOpenDirectStore().add(QStringLiteral("libera"), QStringLiteral("ghost"),
                             IrcCaseMapping());
    IrcController controller;
    controller.setReopenDirectMessages(false);
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(rowForTarget(conversations, QStringLiteral("ghost")) < 0);

    controller.setReopenDirectMessages(true);
    QVERIFY(rowForTarget(conversations, QStringLiteral("ghost")) >= 0);
}

int runOpenDirectTests(int argc, char **argv)
{
    OpenDirectTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_opendirect.moc"
