#include <QAbstractItemModel>
#include <QCoreApplication>
#include <QTemporaryDir>
#include <QTest>

#include "fakeirctransport.h"
#include "irccommand.h"
#include "irccontroller.h"
#include "ircpref.h"
#include "ircsession.h"
#include "ircslashcomplete.h"
#include "ircstatusconsole.h"
#include "messagelistmodel.h"
#include "networklogmodel.h"
#include "testsettings.h"

#include <memory>

namespace
{
IrcSessionConfig config(const QString& networkId = QStringLiteral("libera"))
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

FakeIrcTransport *joinNetwork(IrcController& controller)
{
    auto *transport = new FakeIrcTransport;
    if (!controller.addSession(config(), transport))
        return nullptr;
    if (!controller.start(QStringLiteral("libera")))
        return nullptr;
    welcome(transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    return transport;
}

QString lastWhoisBody(QAbstractItemModel *messages)
{
    QString last;
    if (!messages)
        return last;
    for (int row = 0; row < messages->rowCount(); ++row) {
        const QString kind =
            messages->data(messages->index(row, 0), MessageListModel::KindRole)
                .toString();
        if (kind != QLatin1String("whois"))
            continue;
        last = messages->data(messages->index(row, 0), MessageListModel::BodyRole)
                   .toString();
    }
    return last;
}

int whoisCount(QAbstractItemModel *messages)
{
    int count = 0;
    if (!messages)
        return count;
    for (int row = 0; row < messages->rowCount(); ++row) {
        const QString kind =
            messages->data(messages->index(row, 0), MessageListModel::KindRole)
                .toString();
        if (kind == QLatin1String("whois"))
            ++count;
    }
    return count;
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
}

class PrefTest : public QObject
{
    Q_OBJECT

private slots:
    void init();
    void parseQuerySetAndUsage();
    void completeNamesThenValues();
    void sessionInsertsNameThenValue();
    void queryAndSetFromConversation();
    void statusEchoesWithoutTouchingTheTranscript();
    void writeSucceedsWithNowhereToEcho();
    void settingPersists();

private:
    std::unique_ptr<QTemporaryDir> m_settingsDir;
};

void PrefTest::init()
{
    m_settingsDir = std::make_unique<QTemporaryDir>();
    QVERIFY(m_settingsDir->isValid());
    TestSettings::isolate(m_settingsDir->path());
    QCoreApplication::setOrganizationName(QStringLiteral("omairc"));
    QCoreApplication::setApplicationName(QStringLiteral("omairc"));
}

void PrefTest::parseQuerySetAndUsage()
{
    QCOMPARE(ircParsePrefArgument(QString()).kind, IrcPrefKind::QueryAll);
    QCOMPARE(ircParsePrefArgument(QStringLiteral("  ")).kind, IrcPrefKind::QueryAll);

    const IrcPrefRequest query = ircParsePrefArgument(QStringLiteral("Avatars"));
    QCOMPARE(query.kind, IrcPrefKind::QueryOne);
    QCOMPARE(query.name, IrcPrefName::Avatars);

    const IrcPrefRequest set = ircParsePrefArgument(QStringLiteral("UNREAD off"));
    QCOMPARE(set.kind, IrcPrefKind::Set);
    QCOMPARE(set.name, IrcPrefName::Unread);
    QVERIFY(!set.enabled);

    const IrcPrefRequest on = ircParsePrefArgument(QStringLiteral("directs  on"));
    QCOMPARE(on.kind, IrcPrefKind::Set);
    QVERIFY(on.enabled);

    QCOMPARE(ircParsePrefArgument(QStringLiteral("nope")).kind, IrcPrefKind::Usage);
    QCOMPARE(ircParsePrefArgument(QStringLiteral("directs yes")).kind, IrcPrefKind::Usage);
    QCOMPARE(ircParsePrefArgument(QStringLiteral("directs off extra")).kind,
             IrcPrefKind::Usage);
    QCOMPARE(ircParsePrefArgument(QStringLiteral("on")).kind, IrcPrefKind::Usage);

    QCOMPARE(IrcCommand::parse(QStringLiteral("/pref")).verb, IrcCommand::Verb::Pref);
    QCOMPARE(IrcCommand::parse(QStringLiteral("/PREF directs off")).argument,
             QStringLiteral("directs off"));
    QVERIFY(IrcCommand::parse(QStringLiteral("/pref")).allowedOn(IrcComposerSurface::Status));
    QVERIFY(IrcCommand::parse(QStringLiteral("/pref"))
                .allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(!IrcCommand::parse(QStringLiteral("/pref")).isLiveMessage());
    QCOMPARE(IrcVerbTable::find(IrcCommand::Verb::Pref)->usage, ircPrefUsage());
}

void PrefTest::completeNamesThenValues()
{
    const auto names = IrcSlashComplete::project(
        QStringLiteral("/pref "), IrcComposerSurface::Conversation);
    QVERIFY(names.isOpen());
    QCOMPARE(names.hits().size(), 3);
    QCOMPARE(names.hits().at(0).label, QStringLiteral("/pref directs"));
    QCOMPARE(names.hits().at(0).usage,
             QStringLiteral("Reopen direct messages on startup"));
    QCOMPARE(names.hits().at(1).label, QStringLiteral("/pref avatars"));
    QCOMPARE(names.hits().at(1).usage, QStringLiteral("Show peer avatars"));
    QCOMPARE(names.hits().at(2).label, QStringLiteral("/pref unread"));
    QCOMPARE(names.hits().at(2).usage,
             QStringLiteral("Open conversations at unread"));

    const auto folded = IrcSlashComplete::project(
        QStringLiteral("  /PREF a"), IrcComposerSurface::Status);
    QVERIFY(folded.isOpen());
    QCOMPARE(folded.hits().size(), 1);
    QCOMPARE(folded.hits().first().label, QStringLiteral("/pref avatars"));

    QVERIFY(!IrcSlashComplete::project(QStringLiteral("/pref z"),
                                       IrcComposerSurface::Conversation)
                 .isOpen());
    QVERIFY(!IrcSlashComplete::project(QStringLiteral("/pref nope "),
                                       IrcComposerSurface::Conversation)
                 .isOpen());

    const auto values = IrcSlashComplete::project(
        QStringLiteral("/pref directs "), IrcComposerSurface::Conversation);
    QVERIFY(values.isOpen());
    QCOMPARE(values.hits().size(), 2);
    QCOMPARE(values.hits().at(0).label, QStringLiteral("/pref directs on"));
    QCOMPARE(values.hits().at(1).label, QStringLiteral("/pref directs off"));

    const auto off = IrcSlashComplete::project(
        QStringLiteral("/pref directs of"), IrcComposerSurface::Conversation);
    QVERIFY(off.isOpen());
    QCOMPARE(off.hits().size(), 1);
    QCOMPARE(off.hits().first().label, QStringLiteral("/pref directs off"));

    QVERIFY(!IrcSlashComplete::project(QStringLiteral("/pref directs on "),
                                       IrcComposerSurface::Conversation)
                 .isOpen());
    QVERIFY(!IrcSlashComplete::project(QStringLiteral("/join #omarchy"),
                                       IrcComposerSurface::Conversation)
                 .isOpen());
}

void PrefTest::sessionInsertsNameThenValue()
{
    IrcSlashSession session;
    session.sync(QStringLiteral("/pref d"), false);
    QVERIFY(session.open());
    const auto nameTab = session.routeKey(int(Qt::Key_Tab), int(Qt::NoModifier));
    QVERIFY(nameTab.accepted);
    QCOMPARE(nameTab.insertion, QStringLiteral("/pref directs "));

    session.sync(nameTab.insertion, false);
    QVERIFY(session.open());
    const auto valueTab = session.routeKey(int(Qt::Key_Tab), int(Qt::NoModifier));
    QVERIFY(valueTab.accepted);
    QCOMPARE(valueTab.insertion, QStringLiteral("/pref directs on "));

    session.sync(QStringLiteral("/pref directs"), false);
    QVERIFY(session.open());
    const auto enter = session.routeKey(int(Qt::Key_Return), int(Qt::NoModifier));
    QVERIFY(!enter.accepted);
    QVERIFY(enter.insertion.isEmpty());

    session.sync(QStringLiteral("/pref d"), false);
    QVERIFY(session.open());
    const auto escape = session.routeKey(int(Qt::Key_Escape), int(Qt::NoModifier));
    QVERIFY(escape.accepted);
    session.sync(QStringLiteral("/pref d"), false);
    QVERIFY(!session.open());
    session.sync(QStringLiteral("/pref a"), false);
    QVERIFY(session.open());
}

void PrefTest::queryAndSetFromConversation()
{
    IrcController controller;
    auto *transport = joinNetwork(controller);
    QVERIFY(transport);
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);

    const int framesBefore = transport->writtenFrames().size();
    QVERIFY(controller.sendMessage(QStringLiteral("/pref")));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);
    QCOMPARE(lastWhoisBody(messages),
             QStringLiteral("Reopen direct messages on startup: on\n"
                            "Show peer avatars: on\n"
                            "Open conversations at unread: on"));
    QVERIFY(!lastWhoisBody(messages).contains(ircPrefAvatarNote()));

    QVERIFY(controller.sendMessage(QStringLiteral("/pref avatars")));
    QCOMPARE(lastWhoisBody(messages),
             QStringLiteral("Show peer avatars: on\n") + ircPrefAvatarNote());

    QVERIFY(controller.sendMessage(QStringLiteral("/PREF avatars off")));
    QVERIFY(!controller.loadPeerAvatars());
    QCOMPARE(lastWhoisBody(messages), QStringLiteral("Show peer avatars: off"));
    QVERIFY(!lastWhoisBody(messages).contains(ircPrefAvatarNote()));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);

    QVERIFY(controller.sendMessage(QStringLiteral("/pref avatars off")));
    QCOMPARE(lastWhoisBody(messages), QStringLiteral("Show peer avatars: off"));

    QVERIFY(controller.sendMessage(QStringLiteral("/pref banana")));
    QCOMPARE(lastWhoisBody(messages), ircPrefUsage());
    QVERIFY(!controller.loadPeerAvatars());
    QVERIFY(controller.reopenDirectMessages());
    QVERIFY(controller.openConversationsAtUnread());
}

void PrefTest::statusEchoesWithoutTouchingTheTranscript()
{
    IrcController controller;
    auto *transport = joinNetwork(controller);
    QVERIFY(transport);
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    const int before = whoisCount(messages);

    IrcStatusConsole *console = controller.console();
    QVERIFY(console);
    const int framesBefore = transport->writtenFrames().size();
    QVERIFY(console->submit(QStringLiteral("/pref unread off")));
    QVERIFY(console->lastSubmitAccepted());
    QCOMPARE(whoisCount(messages), before);
    QVERIFY(!controller.openConversationsAtUnread());
    QVERIFY(logContains(console->lines(),
                        QStringLiteral("Open conversations at unread: off")));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);
}

void PrefTest::writeSucceedsWithNowhereToEcho()
{
    IrcController controller;
    QVERIFY(controller.openConversationsAtUnread());
    QVERIFY(controller.sendMessage(QStringLiteral("/pref unread off")));
    QVERIFY(!controller.openConversationsAtUnread());
    QCOMPARE(controller.lastError(), QString());

    IrcStatusConsole *console = controller.console();
    QVERIFY(console);
    const int rows = console->lines()->rowCount();
    QVERIFY(console->submit(QStringLiteral("/pref directs off")));
    QVERIFY(console->lastSubmitAccepted());
    QCOMPARE(console->lines()->rowCount(), rows);
    QVERIFY(!controller.reopenDirectMessages());
}

void PrefTest::settingPersists()
{
    {
        IrcController controller;
        QVERIFY(controller.sendMessage(QStringLiteral("/pref unread off")));
        QVERIFY(controller.sendMessage(QStringLiteral("/pref avatars off")));
        QVERIFY(!controller.openConversationsAtUnread());
        QVERIFY(!controller.loadPeerAvatars());
    }

    IrcController reloaded;
    QVERIFY(!reloaded.openConversationsAtUnread());
    QVERIFY(!reloaded.loadPeerAvatars());
    QVERIFY(reloaded.reopenDirectMessages());
}

int runPrefTests(int argc, char **argv)
{
    PrefTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_pref.moc"
