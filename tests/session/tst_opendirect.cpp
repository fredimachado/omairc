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
#include "testsettings.h"

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

void endMotd(FakeIrcTransport *transport)
{
    transport->injectBytes(
        QByteArrayLiteral(":server 376 omairc :End of MOTD\r\n"));
}

void noMotd(FakeIrcTransport *transport)
{
    transport->injectBytes(
        QByteArrayLiteral(":server 422 omairc :MOTD File is missing\r\n"));
}

QStringList conversationTargets(const QAbstractItemModel *model)
{
    QStringList names;
    for (int row = 0; row < model->rowCount(); ++row) {
        names.append(model->data(model->index(row, 0),
                                 ConversationListModel::ConversationRole)
                         .toString());
    }
    return names;
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

bool storeHas(const QString& networkId, const QString& target)
{
    const IrcCaseMapping mapping;
    return IrcOpenDirectStore().listed(networkId, mapping).contains(target);
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
    void defaultOpenConversationsAtUnreadOff();
    void openConversationsAtUnreadPersists();
    void openPersistsInboundDoesNot();
    void closeDropsPersist();
    void reopenRestoresHistoryWithoutUnread();
    void closedDirectStaysClosed();
    void settingOffRestoresNothing();
    void nickChangeRekeysStoredTarget();
    void servicesAreSkipped();
    void staleTargetStillOpens();
    void forgetNetworkDropsStoredDirects();
    void turningOnRestoresLiveSession();
    void asciiCaseMappingKeepsOneConversation();
    void inboundOnlyQueryAbsentAfterRestart();
    void selfAuthoredInboundPersists();

private:
    QString settingsFile() const;

    std::unique_ptr<QTemporaryDir> m_dir;
};

void OpenDirectTest::init()
{
    m_dir = std::make_unique<QTemporaryDir>();
    QVERIFY(m_dir->isValid());
    TestSettings::isolate(m_dir->path());
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
    QCOMPARE(store.listed(QStringLiteral("net-a"), mapping),
             QStringList{QStringLiteral("lena")});
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
    QVERIFY(store.listed(QStringLiteral("drop"), mapping).isEmpty());
    QCOMPARE(store.listed(QStringLiteral("keep"), mapping),
             QStringList{QStringLiteral("bob")});

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

void OpenDirectTest::defaultOpenConversationsAtUnreadOff()
{
    QCOMPARE(IrcController().openConversationsAtUnread(), false);
}

void OpenDirectTest::openConversationsAtUnreadPersists()
{
    QCOMPARE(IrcController().openConversationsAtUnread(), false);

    {
        IrcController controller;
        QVERIFY(!controller.openConversationsAtUnread());
        controller.setOpenConversationsAtUnread(true);
        QVERIFY(controller.openConversationsAtUnread());
    }

    {
        IrcController reloaded;
        QVERIFY(reloaded.openConversationsAtUnread());
        reloaded.setOpenConversationsAtUnread(false);
        QVERIFY(!reloaded.openConversationsAtUnread());
    }

    QVERIFY(!IrcController().openConversationsAtUnread());
}

void OpenDirectTest::openPersistsInboundDoesNot()
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

    QVERIFY(storeHas(QStringLiteral("libera"), QStringLiteral("anna")));
    QVERIFY(!storeHas(QStringLiteral("libera"), QStringLiteral("zed")));
    QVERIFY(!storeHas(QStringLiteral("libera"), QStringLiteral("#omarchy")));
}

void OpenDirectTest::closeDropsPersist()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    controller.openDirectMessage(QStringLiteral("lena"));
    controller.openDirectMessage(QStringLiteral("zed"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    controller.closeDirectMessage();

    QVERIFY(!storeHas(QStringLiteral("libera"), QStringLiteral("lena")));
    QVERIFY(storeHas(QStringLiteral("libera"), QStringLiteral("zed")));
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
        controller.selectConversation(QStringLiteral("libera"), QStringLiteral("alice"));
        QVERIFY(controller.sendMessage(QStringLiteral("hello alice")));
        controller.selectConversation(QStringLiteral("libera"), QStringLiteral("bob"));
        QVERIFY(controller.sendMessage(QStringLiteral("hello bob")));
        controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    }

    IrcController reloaded;
    reloaded.setTranscriptRoot(root);
    auto *transport = new FakeIrcTransport;
    QSignalSpy mentions(&reloaded, &IrcController::mentionArrived);
    QVERIFY(reloaded.addSession(sessionConfig(), transport));
    QVERIFY(reloaded.start(QStringLiteral("libera")));
    welcome(transport);
    endMotd(transport);

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
        transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
        controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
        controller.openDirectMessage(QStringLiteral("alice"));
        controller.openDirectMessage(QStringLiteral("bob"));
        controller.selectConversation(QStringLiteral("libera"), QStringLiteral("alice"));
        controller.closeDirectMessage();
    }

    IrcController reloaded;
    auto *transport = new FakeIrcTransport;
    QVERIFY(reloaded.addSession(sessionConfig(), transport));
    QVERIFY(reloaded.start(QStringLiteral("libera")));
    welcome(transport);
    endMotd(transport);
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
        transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
        controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
        controller.openDirectMessage(QStringLiteral("alice"));
        QVERIFY(storeHas(QStringLiteral("libera"), QStringLiteral("alice")));
    }

    IrcController reloaded;
    QVERIFY(!reloaded.reopenDirectMessages());
    auto *transport = new FakeIrcTransport;
    QVERIFY(reloaded.addSession(sessionConfig(), transport));
    QVERIFY(reloaded.start(QStringLiteral("libera")));
    welcome(transport);
    endMotd(transport);
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
        transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
        controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
        controller.openDirectMessage(QStringLiteral("alice"));
        transport->injectBytes(
            QByteArrayLiteral(":alice!u@h NICK :alicia\r\n"));
        QCOMPARE(controller.selectedTarget(), QStringLiteral("alicia"));
        QVERIFY(storeHas(QStringLiteral("libera"), QStringLiteral("alicia")));
        QVERIFY(!storeHas(QStringLiteral("libera"), QStringLiteral("alice")));
    }

    IrcController reloaded;
    auto *transport = new FakeIrcTransport;
    QVERIFY(reloaded.addSession(sessionConfig(), transport));
    QVERIFY(reloaded.start(QStringLiteral("libera")));
    welcome(transport);
    endMotd(transport);
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
    controller.openDirectMessage(QStringLiteral("alice"));
    transport->injectBytes(
        QByteArrayLiteral(":NickServ!NickServ@services. NOTICE omairc "
                          ":This nickname is registered.\r\n"));

    const IrcCaseMapping mapping;
    QVERIFY(!storeHas(QStringLiteral("libera"), QStringLiteral("NickServ")));
    QVERIFY(storeHas(QStringLiteral("libera"), QStringLiteral("alice")));

    IrcOpenDirectStore().add(QStringLiteral("libera"), QStringLiteral("ChanServ"),
                             mapping);

    IrcController reloaded;
    auto *next = new FakeIrcTransport;
    QVERIFY(reloaded.addSession(sessionConfig(), next));
    QVERIFY(reloaded.start(QStringLiteral("libera")));
    welcome(next);
    endMotd(next);
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(reloaded.conversations());
    QVERIFY(rowForTarget(conversations, QStringLiteral("NickServ")) < 0);
    QVERIFY(rowForTarget(conversations, QStringLiteral("ChanServ")) < 0);
    QVERIFY(rowForTarget(conversations, QStringLiteral("alice")) >= 0);
    QVERIFY(!storeHas(QStringLiteral("libera"), QStringLiteral("ChanServ")));
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
    QVERIFY(rowForTarget(conversations, QStringLiteral("ghost")) < 0);
    noMotd(transport);
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
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    controller.openDirectMessage(QStringLiteral("alice"));
    QVERIFY(storeHas(QStringLiteral("libera"), QStringLiteral("alice")));
    controller.forgetNetworkState(QStringLiteral("libera"));
    QVERIFY(!storeHas(QStringLiteral("libera"), QStringLiteral("alice")));
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

void OpenDirectTest::asciiCaseMappingKeepsOneConversation()
{
    const QByteArray isupport = QByteArrayLiteral(
        ":server 005 omairc CHANTYPES=# CASEMAPPING=ascii :are supported\r\n");
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
        transport->injectBytes(isupport);
        endMotd(transport);
        transport->injectBytes(
            QByteArrayLiteral(":Al[ice]!u@h PRIVMSG omairc :hi there\r\n"));
        controller.selectConversation(QStringLiteral("libera"), QStringLiteral("Al[ice]"));
        QVERIFY(controller.sendMessage(QStringLiteral("hello back")));
    }
    IrcController reloaded;
    reloaded.setTranscriptRoot(root);
    auto *transport = new FakeIrcTransport;
    QVERIFY(reloaded.addSession(sessionConfig(), transport));
    QVERIFY(reloaded.start(QStringLiteral("libera")));
    welcome(transport);
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(reloaded.conversations());
    QVERIFY(conversations);
    QCOMPARE(conversationTargets(conversations).count(QStringLiteral("Al[ice]")), 0);
    transport->injectBytes(isupport);
    endMotd(transport);
    QCOMPARE(conversationTargets(conversations).count(QStringLiteral("Al[ice]")), 1);
    transport->injectBytes(
        QByteArrayLiteral(":Al[ice]!u@h PRIVMSG omairc :second\r\n"));
    QCOMPARE(conversationTargets(conversations).count(QStringLiteral("Al[ice]")), 1);

    reloaded.selectConversation(QStringLiteral("libera"), QStringLiteral("Al[ice]"));
    auto *messages = qobject_cast<QAbstractItemModel *>(reloaded.messages());
    QVERIFY(messagesContain(messages, QStringLiteral("hi there")));
    QVERIFY(messagesContain(messages, QStringLiteral("hello back")));
    QVERIFY(messagesContain(messages, QStringLiteral("second")));
}

void OpenDirectTest::inboundOnlyQueryAbsentAfterRestart()
{
    {
        IrcController controller;
        auto *transport = new FakeIrcTransport;
        QVERIFY(controller.addSession(sessionConfig(), transport));
        QVERIFY(controller.start(QStringLiteral("libera")));
        welcome(transport);
        transport->injectBytes(
            QByteArrayLiteral(":zed!u@h PRIVMSG omairc :hello\r\n"));
        QVERIFY(rowForTarget(
                    qobject_cast<QAbstractItemModel *>(controller.conversations()),
                    QStringLiteral("zed"))
                >= 0);
        QVERIFY(!storeHas(QStringLiteral("libera"), QStringLiteral("zed")));
    }

    IrcController reloaded;
    auto *transport = new FakeIrcTransport;
    QVERIFY(reloaded.addSession(sessionConfig(), transport));
    QVERIFY(reloaded.start(QStringLiteral("libera")));
    welcome(transport);
    endMotd(transport);
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(reloaded.conversations());
    QVERIFY(rowForTarget(conversations, QStringLiteral("zed")) < 0);
}

void OpenDirectTest::selfAuthoredInboundPersists()
{
    {
        IrcController controller;
        auto *transport = new FakeIrcTransport;
        QVERIFY(controller.addSession(sessionConfig(), transport));
        QVERIFY(controller.start(QStringLiteral("libera")));
        welcome(transport);
        transport->injectBytes(
            QByteArrayLiteral(":alice!u@h PRIVMSG omairc :hi\r\n"
                              ":omairc!u@h PRIVMSG alice :from other client\r\n"));
        QVERIFY(storeHas(QStringLiteral("libera"), QStringLiteral("alice")));
    }

    IrcController reloaded;
    auto *transport = new FakeIrcTransport;
    QVERIFY(reloaded.addSession(sessionConfig(), transport));
    QVERIFY(reloaded.start(QStringLiteral("libera")));
    welcome(transport);
    endMotd(transport);
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(reloaded.conversations());
    QVERIFY(rowForTarget(conversations, QStringLiteral("alice")) >= 0);
}

int runOpenDirectTests(int argc, char **argv)
{
    OpenDirectTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_opendirect.moc"
