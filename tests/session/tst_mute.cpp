#include <QAbstractItemModel>
#include <QCoreApplication>
#include <QFile>
#include <QSettings>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QTest>

#include "conversationlistmodel.h"
#include "fakeirctransport.h"
#include "irccommand.h"
#include "irccontroller.h"
#include "ircmute.h"
#include "ircnetworkprofile.h"
#include "ircprofilestore.h"
#include "ircslashcomplete.h"
#include "messagelistmodel.h"
#include "networklogmodel.h"

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

class MuteTest : public QObject
{
    Q_OBJECT

private slots:
    void init();
    void storeRoundTripsAndIsIdempotent();
    void storeMatchesRfc1459AndSurvivesProfileSave();
    void storeForgetDropsTheGroup();
    void parseAndCatalog();
    void muteCurrentStopsMentionAndKeepsChat();
    void mutedDirectMessageDoesNotNotify();
    void unmuteRestoresMention();
    void openingDoesNotUnmute();
    void persistSurvivesRestart();
    void closeDropsMute();
    void forgetsWithTheNetwork();
    void discardSessionKeepsTheList();
    void statusMuteWithoutTargetIsRefused();
    void listsMutedTargets();
    void nonMutedStillNotifies();
    void ignoreStillHidesPrivateTraffic();
    void refusesBadTargets();
    void offlineMuteIsNotConnected();

private:
    QString settingsFile() const;

    std::unique_ptr<QTemporaryDir> m_dir;
};

void MuteTest::init()
{
    m_dir = std::make_unique<QTemporaryDir>();
    QVERIFY(m_dir->isValid());
    qputenv("XDG_CONFIG_HOME", m_dir->path().toUtf8());
    QSettings::setPath(QSettings::NativeFormat, QSettings::UserScope, m_dir->path());
    QCoreApplication::setOrganizationName(QStringLiteral("omairc"));
    QCoreApplication::setApplicationName(QStringLiteral("omairc"));
}

QString MuteTest::settingsFile() const
{
    QSettings settings;
    return settings.fileName();
}

void MuteTest::storeRoundTripsAndIsIdempotent()
{
    const IrcCaseMapping mapping;
    IrcMuteStore store;
    QVERIFY(store.add(QStringLiteral("net-a"), QStringLiteral("#omarchy"), mapping));
    QVERIFY(!store.add(QStringLiteral("net-a"), QStringLiteral("#Omarchy"), mapping));
    QCOMPARE(store.targets(QStringLiteral("net-a")),
             QStringList{QStringLiteral("#omarchy")});
    QVERIFY(store.contains(QStringLiteral("net-a"), QStringLiteral("#OMARCHY"),
                           mapping));
    QVERIFY(!store.remove(QStringLiteral("net-a"), QStringLiteral("bob"), mapping));
    QVERIFY(store.remove(QStringLiteral("net-a"), QStringLiteral("#omarchy"), mapping));
    QVERIFY(store.targets(QStringLiteral("net-a")).isEmpty());
}

void MuteTest::storeMatchesRfc1459AndSurvivesProfileSave()
{
    const IrcCaseMapping mapping;
    IrcMuteStore store;
    QVERIFY(store.add(QStringLiteral("net-a"), QStringLiteral("Nick["), mapping));
    QVERIFY(store.contains(QStringLiteral("net-a"), QStringLiteral("nick{"), mapping));
    QCOMPARE(store.listed(QStringLiteral("net-a"), mapping),
             QStringList{QStringLiteral("Nick[")});

    IrcNetworkProfile profile = IrcNetworkProfile::create();
    profile.networkId = QStringLiteral("net-a");
    profile.host = QStringLiteral("irc.example.net");
    profile.nick = QStringLiteral("omairc");
    IrcProfileStore().save(profile);
    IrcProfileStore().save(profile);

    QVERIFY(IrcMuteStore().contains(
        QStringLiteral("net-a"), QStringLiteral("nick{"), mapping));

    QFile file(settingsFile());
    QVERIFY(file.open(QIODevice::ReadOnly | QIODevice::Text));
    const QString contents = QString::fromUtf8(file.readAll());
    QVERIFY(contents.contains(QLatin1String("mutes")));
    QVERIFY(contents.contains(QLatin1String("Nick[")));
    QVERIFY(!contents.contains(QLatin1String("password"), Qt::CaseInsensitive));
}

void MuteTest::storeForgetDropsTheGroup()
{
    const IrcCaseMapping mapping;
    IrcMuteStore store;
    QVERIFY(store.add(QStringLiteral("drop"), QStringLiteral("#room"), mapping));
    QVERIFY(store.add(QStringLiteral("keep"), QStringLiteral("bob"), mapping));
    store.forget(QStringLiteral("drop"));
    QVERIFY(!store.contains(QStringLiteral("drop"), QStringLiteral("#room"), mapping));
    QVERIFY(store.contains(QStringLiteral("keep"), QStringLiteral("bob"), mapping));

    QFile file(settingsFile());
    QVERIFY(file.open(QIODevice::ReadOnly | QIODevice::Text));
    const QString contents = QString::fromUtf8(file.readAll());
    QVERIFY(!contents.contains(QLatin1String("#room")));
    QVERIFY(contents.contains(QLatin1String("bob")));
}

void MuteTest::parseAndCatalog()
{
    QCOMPARE(IrcCommand::parse(QStringLiteral("/mute")).verb,
             IrcCommand::Verb::Mute);
    QCOMPARE(IrcCommand::parse(QStringLiteral("/MUTE #omarchy")).verb,
             IrcCommand::Verb::Mute);
    QCOMPARE(IrcCommand::parse(QStringLiteral("/unmute lena")).verb,
             IrcCommand::Verb::Unmute);
    QCOMPARE(IrcCommand::parse(QStringLiteral("/muted")).verb,
             IrcCommand::Verb::Muted);
    QVERIFY(IrcCommand::parse(QStringLiteral("/mute #omarchy"))
                .allowedOn(IrcComposerSurface::Status));
    QCOMPARE(IrcVerbTable::all().size(), 35);
    QCOMPARE(IrcVerbTable::visibleOn(IrcComposerSurface::Status).size(), 27);
    QCOMPARE(IrcVerbTable::visibleOn(IrcComposerSurface::Conversation).size(), 35);
    const auto probe = IrcSlashComplete::project(
        QStringLiteral("/mu"), IrcComposerSurface::Conversation);
    QVERIFY(probe.isOpen());
    QCOMPARE(probe.hits().first().label, QStringLiteral("/mute"));
    QVERIFY(probe.containsLabel(QStringLiteral("/muted")));
}

void MuteTest::muteCurrentStopsMentionAndKeepsChat()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/mute")));
    QVERIFY(logContains(controller.console()->lines(), QStringLiteral("Muted #omarchy")));

    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#desktop\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#desktop"));

    QSignalSpy spy(&controller, &IrcController::mentionArrived);
    transport->injectBytes(
        QByteArrayLiteral(":Alice!u@h PRIVMSG #omarchy :omairc: ping\r\n"));
    QCOMPARE(spy.count(), 0);

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(roleAt(conversations, QStringLiteral("#omarchy"),
                    ConversationListModel::MutedRole),
             true);
    QCOMPARE(roleAt(conversations, QStringLiteral("#omarchy"),
                    ConversationListModel::MentionRole),
             false);
    QCOMPARE(roleAt(conversations, QStringLiteral("#omarchy"),
                    ConversationListModel::UnreadRole),
             1);

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(messagesContain(messages, QStringLiteral("omairc: ping")));
    QCOMPARE(roleAt(conversations, QStringLiteral("#omarchy"),
                    ConversationListModel::MutedRole),
             true);
}

void MuteTest::mutedDirectMessageDoesNotNotify()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/mute lena")));
    QVERIFY(logContains(controller.console()->lines(), QStringLiteral("Muted lena")));

    QSignalSpy spy(&controller, &IrcController::mentionArrived);
    transport->injectBytes(
        QByteArrayLiteral(":lena!u@h PRIVMSG omairc :hello\r\n"));
    QCOMPARE(spy.count(), 0);

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(roleAt(conversations, QStringLiteral("lena"),
                    ConversationListModel::MutedRole),
             true);
    QCOMPARE(roleAt(conversations, QStringLiteral("lena"),
                    ConversationListModel::MentionRole),
             false);
    QCOMPARE(roleAt(conversations, QStringLiteral("lena"),
                    ConversationListModel::UnreadRole),
             1);

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(messagesContain(messages, QStringLiteral("hello")));
}

void MuteTest::unmuteRestoresMention()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("/mute")));
    QVERIFY(controller.sendMessage(QStringLiteral("/unmute")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("No longer muted #omarchy")));

    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#desktop\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#desktop"));

    QSignalSpy spy(&controller, &IrcController::mentionArrived);
    transport->injectBytes(
        QByteArrayLiteral(":Alice!u@h PRIVMSG #omarchy :omairc: back\r\n"));
    QCOMPARE(spy.count(), 1);
    QCOMPARE(spy.at(0).at(0).toString(), QStringLiteral("Alice"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(roleAt(conversations, QStringLiteral("#omarchy"),
                    ConversationListModel::MutedRole),
             false);
    QCOMPARE(roleAt(conversations, QStringLiteral("#omarchy"),
                    ConversationListModel::MentionRole),
             true);
}

void MuteTest::openingDoesNotUnmute()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("/mute")));
    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#desktop\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#desktop"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(roleAt(conversations, QStringLiteral("#omarchy"),
                    ConversationListModel::MutedRole),
             true);

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#desktop"));
    QSignalSpy spy(&controller, &IrcController::mentionArrived);
    transport->injectBytes(
        QByteArrayLiteral(":Alice!u@h PRIVMSG #omarchy :omairc: still\r\n"));
    QCOMPARE(spy.count(), 0);
    QCOMPARE(roleAt(conversations, QStringLiteral("#omarchy"),
                    ConversationListModel::MentionRole),
             false);
}

void MuteTest::persistSurvivesRestart()
{
    {
        IrcController controller;
        auto *transport = new FakeIrcTransport;
        QVERIFY(controller.addSession(sessionConfig(), transport));
        QVERIFY(controller.start(QStringLiteral("libera")));
        welcome(transport);
        transport->injectBytes(
            QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
        controller.selectConversation(QStringLiteral("libera"),
                                      QStringLiteral("#omarchy"));
        QVERIFY(controller.sendMessage(QStringLiteral("/mute")));
    }

    QVERIFY(IrcMuteStore().contains(
        QStringLiteral("libera"), QStringLiteral("#omarchy"), IrcCaseMapping()));

    IrcController again;
    auto *transport = new FakeIrcTransport;
    QVERIFY(again.addSession(sessionConfig(), transport));
    QVERIFY(again.start(QStringLiteral("libera")));
    welcome(transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"
                                             ":omairc!u@h JOIN :#desktop\r\n"));
    again.selectConversation(QStringLiteral("libera"), QStringLiteral("#desktop"));

    QSignalSpy spy(&again, &IrcController::mentionArrived);
    transport->injectBytes(
        QByteArrayLiteral(":Alice!u@h PRIVMSG #omarchy :omairc: after\r\n"));
    QCOMPARE(spy.count(), 0);

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(again.conversations());
    QVERIFY(conversations);
    QCOMPARE(roleAt(conversations, QStringLiteral("#omarchy"),
                    ConversationListModel::MutedRole),
             true);
    QCOMPARE(roleAt(conversations, QStringLiteral("#omarchy"),
                    ConversationListModel::MentionRole),
             false);
}

void MuteTest::closeDropsMute()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"
                                             ":lena!u@h PRIVMSG omairc :hi\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    QVERIFY(controller.sendMessage(QStringLiteral("/mute")));
    QVERIFY(IrcMuteStore().contains(
        QStringLiteral("libera"), QStringLiteral("lena"), IrcCaseMapping()));
    QVERIFY(controller.sendMessage(QStringLiteral("/close")));
    QVERIFY(!IrcMuteStore().contains(
        QStringLiteral("libera"), QStringLiteral("lena"), IrcCaseMapping()));

    QSignalSpy spy(&controller, &IrcController::mentionArrived);
    transport->injectBytes(
        QByteArrayLiteral(":lena!u@h PRIVMSG omairc :again\r\n"));
    QCOMPARE(spy.count(), 1);
}

void MuteTest::forgetsWithTheNetwork()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    controller.openStatus(QStringLiteral("libera"));
    QVERIFY(controller.console()->submit(QStringLiteral("/mute #omarchy")));
    QVERIFY(IrcMuteStore().contains(
        QStringLiteral("libera"), QStringLiteral("#omarchy"), IrcCaseMapping()));
    controller.forgetNetworkState(QStringLiteral("libera"));
    QVERIFY(!IrcMuteStore().contains(
        QStringLiteral("libera"), QStringLiteral("#omarchy"), IrcCaseMapping()));
}

void MuteTest::discardSessionKeepsTheList()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    controller.openStatus(QStringLiteral("libera"));
    QVERIFY(controller.console()->submit(QStringLiteral("/mute #omarchy")));
    QVERIFY(IrcMuteStore().contains(
        QStringLiteral("libera"), QStringLiteral("#omarchy"), IrcCaseMapping()));
    QVERIFY(controller.discardSession(QStringLiteral("libera")));
    QVERIFY(IrcMuteStore().contains(
        QStringLiteral("libera"), QStringLiteral("#omarchy"), IrcCaseMapping()));
}

void MuteTest::statusMuteWithoutTargetIsRefused()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    controller.openStatus(QStringLiteral("libera"));
    QVERIFY(controller.console()->submit(QStringLiteral("/mute")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("Mute applies to conversations")));
    QVERIFY(!IrcMuteStore().contains(
        QStringLiteral("libera"), QStringLiteral("#omarchy"), IrcCaseMapping()));
}

void MuteTest::listsMutedTargets()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("/muted")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("Not muting anything")));
    QVERIFY(controller.sendMessage(QStringLiteral("/mute")));
    QVERIFY(controller.sendMessage(QStringLiteral("/mute lena")));
    QVERIFY(controller.sendMessage(QStringLiteral("/muted")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("Muted: #omarchy, lena")));
    QVERIFY(controller.sendMessage(QStringLiteral("/mute")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("Already muted #omarchy")));
    QVERIFY(controller.sendMessage(QStringLiteral("/unmute lena")));
    QVERIFY(controller.sendMessage(QStringLiteral("/muted")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("Muted: #omarchy")));
}

void MuteTest::nonMutedStillNotifies()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"
                                             ":omairc!u@h JOIN :#desktop\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("/mute")));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QSignalSpy spy(&controller, &IrcController::mentionArrived);
    transport->injectBytes(
        QByteArrayLiteral(":Alice!u@h PRIVMSG #desktop :omairc: other\r\n"));
    QCOMPARE(spy.count(), 1);
}

void MuteTest::ignoreStillHidesPrivateTraffic()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("/ignore lena")));
    QVERIFY(controller.sendMessage(QStringLiteral("/mute #omarchy")));
    transport->injectBytes(
        QByteArrayLiteral(":lena!u@h PRIVMSG omairc :secret\r\n"
                          ":lena!u@h PRIVMSG #omarchy :still here\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("lena")), -1);
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(messagesContain(messages, QStringLiteral("still here")));
}

void MuteTest::refusesBadTargets()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(!controller.sendMessage(QStringLiteral("/mute lena extra")));
    QVERIFY(!controller.sendMessage(QStringLiteral("/mute lena!u@h")));
    QVERIFY(!controller.sendMessage(QStringLiteral("/muted extra")));
    QVERIFY(!controller.sendMessage(QStringLiteral("/unmute *")));
}

void MuteTest::offlineMuteIsNotConnected()
{
    IrcController controller;
    QVERIFY(!controller.sendMessage(QStringLiteral("/mute #omarchy")));
    QCOMPARE(controller.lastError(),
             QStringLiteral("Mute applies to conversations"));
    QVERIFY(!IrcMuteStore().contains(
        QStringLiteral("libera"), QStringLiteral("#omarchy"), IrcCaseMapping()));

    controller.openStatus(QStringLiteral("libera"));
    QVERIFY(controller.console()->submit(QStringLiteral("/mute #omarchy")));
    QVERIFY(logContains(controller.console()->lines(), QStringLiteral("Not connected")));
    QVERIFY(!IrcMuteStore().contains(
        QStringLiteral("libera"), QStringLiteral("#omarchy"), IrcCaseMapping()));
}

int runMuteTests(int argc, char **argv)
{
    MuteTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_mute.moc"
