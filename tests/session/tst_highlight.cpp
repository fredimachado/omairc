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
#include "irchighlight.h"
#include "ircnetworkprofile.h"
#include "ircprofilestore.h"
#include "ircslashcomplete.h"
#include "messagelistmodel.h"
#include "networklogmodel.h"

#include <memory>

namespace
{
IrcSessionConfig sessionConfig(const QString& networkId = QStringLiteral("libera"),
                               const QString& nick = QStringLiteral("fred"))
{
    IrcSessionConfig value;
    value.networkId = networkId;
    value.host = QStringLiteral("irc.example");
    value.tlsEnabled = true;
    value.nick = nick;
    value.username = nick;
    value.realname = QStringLiteral("Omairc User");
    value.reconnectEnabled = false;
    return value;
}

void welcome(FakeIrcTransport *transport)
{
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP fred LS :multi-prefix\r\n"
                          ":server 001 fred :Welcome\r\n"));
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

class HighlightTest : public QObject
{
    Q_OBJECT

private slots:
    void init();
    void storeRoundTripsAndIsIdempotent();
    void storeMatchesRfc1459AndSurvivesProfileSave();
    void storeForgetDropsTheGroup();
    void parseAndCatalog();
    void controllerHighlightsInboundAndKeepsChannelText();
    void controllerForgetsWithTheNetwork();
    void discardSessionKeepsTheList();
    void nickChangeKeepsTheList();
    void refusesEmptyAndExtraTokens();
    void offlineHighlightIsNotConnected();

private:
    QString settingsFile() const;

    std::unique_ptr<QTemporaryDir> m_dir;
};

void HighlightTest::init()
{
    m_dir = std::make_unique<QTemporaryDir>();
    QVERIFY(m_dir->isValid());
    qputenv("XDG_CONFIG_HOME", m_dir->path().toUtf8());
    QSettings::setPath(QSettings::NativeFormat, QSettings::UserScope, m_dir->path());
    QCoreApplication::setOrganizationName(QStringLiteral("omairc"));
    QCoreApplication::setApplicationName(QStringLiteral("omairc"));
}

QString HighlightTest::settingsFile() const
{
    QSettings settings;
    return settings.fileName();
}

void HighlightTest::storeRoundTripsAndIsIdempotent()
{
    const IrcCaseMapping mapping;
    IrcHighlightStore store;
    QVERIFY(store.add(QStringLiteral("net-a"), QStringLiteral("omairc"), mapping));
    QVERIFY(!store.add(QStringLiteral("net-a"), QStringLiteral("Omairc"), mapping));
    QCOMPARE(store.words(QStringLiteral("net-a")),
             QStringList{QStringLiteral("omairc")});
    QVERIFY(store.contains(QStringLiteral("net-a"), QStringLiteral("OMAIRC"), mapping));
    QVERIFY(!store.remove(QStringLiteral("net-a"), QStringLiteral("deploy"), mapping));
    QVERIFY(store.remove(QStringLiteral("net-a"), QStringLiteral("omairc"), mapping));
    QVERIFY(store.words(QStringLiteral("net-a")).isEmpty());
}

void HighlightTest::storeMatchesRfc1459AndSurvivesProfileSave()
{
    const IrcCaseMapping mapping;
    IrcHighlightStore store;
    QVERIFY(store.add(QStringLiteral("net-a"), QStringLiteral("Word["), mapping));
    QVERIFY(store.contains(QStringLiteral("net-a"), QStringLiteral("word{"), mapping));
    QCOMPARE(store.listed(QStringLiteral("net-a"), mapping),
             QStringList{QStringLiteral("Word[")});

    IrcNetworkProfile profile = IrcNetworkProfile::create();
    profile.networkId = QStringLiteral("net-a");
    profile.host = QStringLiteral("irc.example.net");
    profile.nick = QStringLiteral("fred");
    IrcProfileStore().save(profile);
    IrcProfileStore().save(profile);

    QVERIFY(IrcHighlightStore().contains(
        QStringLiteral("net-a"), QStringLiteral("word{"), mapping));

    QFile file(settingsFile());
    QVERIFY(file.open(QIODevice::ReadOnly | QIODevice::Text));
    const QString contents = QString::fromUtf8(file.readAll());
    QVERIFY(contents.contains(QLatin1String("highlights")));
    QVERIFY(contents.contains(QLatin1String("Word[")));
    QVERIFY(!contents.contains(QLatin1String("password"), Qt::CaseInsensitive));
}

void HighlightTest::storeForgetDropsTheGroup()
{
    const IrcCaseMapping mapping;
    IrcHighlightStore store;
    QVERIFY(store.add(QStringLiteral("drop"), QStringLiteral("omairc"), mapping));
    QVERIFY(store.add(QStringLiteral("keep"), QStringLiteral("deploy"), mapping));
    store.forget(QStringLiteral("drop"));
    QVERIFY(!store.contains(QStringLiteral("drop"), QStringLiteral("omairc"), mapping));
    QVERIFY(store.contains(QStringLiteral("keep"), QStringLiteral("deploy"), mapping));

    QFile file(settingsFile());
    QVERIFY(file.open(QIODevice::ReadOnly | QIODevice::Text));
    const QString contents = QString::fromUtf8(file.readAll());
    QVERIFY(!contents.contains(QLatin1String("omairc")));
    QVERIFY(contents.contains(QLatin1String("deploy")));
}

void HighlightTest::parseAndCatalog()
{
    QCOMPARE(IrcCommand::parse(QStringLiteral("/highlight omairc")).verb,
             IrcCommand::Verb::Highlight);
    QCOMPARE(IrcCommand::parse(QStringLiteral("/UNHIGHLIGHT omairc")).verb,
             IrcCommand::Verb::Unhighlight);
    QCOMPARE(IrcCommand::parse(QStringLiteral("/highlights")).verb,
             IrcCommand::Verb::Highlights);
    QVERIFY(IrcCommand::parse(QStringLiteral("/highlight omairc"))
                .allowedOn(IrcComposerSurface::Status));
    QCOMPARE(IrcVerbTable::all().size(), 39);
    QCOMPARE(IrcVerbTable::visibleOn(IrcComposerSurface::Status).size(), 31);
    QCOMPARE(IrcVerbTable::visibleOn(IrcComposerSurface::Conversation).size(), 39);
    const auto probe = IrcSlashComplete::project(
        QStringLiteral("/high"), IrcComposerSurface::Conversation);
    QVERIFY(probe.isOpen());
    QCOMPARE(probe.hits().first().label, QStringLiteral("/highlight"));
    QVERIFY(probe.containsLabel(QStringLiteral("/highlights")));
}

void HighlightTest::controllerHighlightsInboundAndKeepsChannelText()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(sessionConfig(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    transport->injectBytes(
        QByteArrayLiteral(":fred!u@h JOIN :#omarchy\r\n"
                          ":fred!u@h JOIN :#other\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/highlight omairc")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("Highlighting omairc")));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#other"));
    QSignalSpy spy(&controller, &IrcController::mentionArrived);
    transport->injectBytes(
        QByteArrayLiteral(":Alice!u@h PRIVMSG #omarchy :please review omairc\r\n"));

    QCOMPARE(spy.count(), 1);
    QCOMPARE(spy.at(0).at(0).toString(), QStringLiteral("Alice"));
    QCOMPARE(spy.at(0).at(1).toString(), QStringLiteral("please review omairc"));
    QVERIFY(controller.mentionFor(QStringLiteral("libera")));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    const int omarchy = rowForTarget(conversations, QStringLiteral("#omarchy"));
    QVERIFY(omarchy >= 0);
    QCOMPARE(conversations->data(conversations->index(omarchy, 0),
                                 ConversationListModel::UnreadRole)
                 .toInt(),
             1);
    QVERIFY(conversations->data(conversations->index(omarchy, 0),
                                ConversationListModel::MentionRole)
                .toBool());

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(messagesContain(messages, QStringLiteral("please review omairc")));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#other"));
    transport->injectBytes(
        QByteArrayLiteral(":Alice!u@h PRIVMSG #omarchy :please review omaircd\r\n"));
    QCOMPARE(spy.count(), 1);
    QVERIFY(!controller.mentionFor(QStringLiteral("libera")));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(messagesContain(messages, QStringLiteral("please review omaircd")));

    QVERIFY(IrcHighlightStore().contains(
        QStringLiteral("libera"), QStringLiteral("omairc"), IrcCaseMapping()));

    IrcController restarted;
    auto *again = new FakeIrcTransport;
    QVERIFY(restarted.addSession(sessionConfig(), again));
    QVERIFY(restarted.start(QStringLiteral("libera")));
    welcome(again);
    again->injectBytes(QByteArrayLiteral(":fred!u@h JOIN :#omarchy\r\n"
                                        ":fred!u@h JOIN :#other\r\n"));
    restarted.selectConversation(QStringLiteral("libera"), QStringLiteral("#other"));
    QSignalSpy restartSpy(&restarted, &IrcController::mentionArrived);
    again->injectBytes(
        QByteArrayLiteral(":Alice!u@h PRIVMSG #omarchy :please review omairc\r\n"));
    QCOMPARE(restartSpy.count(), 1);
    QVERIFY(restarted.mentionFor(QStringLiteral("libera")));

    QVERIFY(controller.sendMessage(QStringLiteral("/unhighlight omairc")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("No longer highlighting omairc")));
    QVERIFY(controller.sendMessage(QStringLiteral("/highlights")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("No highlight words")));
    QVERIFY(!IrcHighlightStore().contains(
        QStringLiteral("libera"), QStringLiteral("omairc"), IrcCaseMapping()));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#other"));
    QSignalSpy nickSpy(&controller, &IrcController::mentionArrived);
    transport->injectBytes(
        QByteArrayLiteral(":Alice!u@h PRIVMSG #omarchy :fred: ping\r\n"));
    QCOMPARE(nickSpy.count(), 1);
    QVERIFY(controller.mentionFor(QStringLiteral("libera")));
}

void HighlightTest::controllerForgetsWithTheNetwork()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    controller.openStatus(QStringLiteral("libera"));
    QVERIFY(controller.console()->submit(QStringLiteral("/highlight omairc")));
    QVERIFY(IrcHighlightStore().contains(
        QStringLiteral("libera"), QStringLiteral("omairc"), IrcCaseMapping()));
    controller.forgetNetworkState(QStringLiteral("libera"));
    QVERIFY(!IrcHighlightStore().contains(
        QStringLiteral("libera"), QStringLiteral("omairc"), IrcCaseMapping()));
}

void HighlightTest::discardSessionKeepsTheList()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    controller.openStatus(QStringLiteral("libera"));
    QVERIFY(controller.console()->submit(QStringLiteral("/highlight omairc")));
    QVERIFY(IrcHighlightStore().contains(
        QStringLiteral("libera"), QStringLiteral("omairc"), IrcCaseMapping()));
    QVERIFY(controller.discardSession(QStringLiteral("libera")));
    QVERIFY(IrcHighlightStore().contains(
        QStringLiteral("libera"), QStringLiteral("omairc"), IrcCaseMapping()));

    auto *again = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), again));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(again);
    again->injectBytes(QByteArrayLiteral(":fred!u@h JOIN :#omarchy\r\n"
                                        ":fred!u@h JOIN :#other\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#other"));
    QSignalSpy spy(&controller, &IrcController::mentionArrived);
    again->injectBytes(
        QByteArrayLiteral(":Alice!u@h PRIVMSG #omarchy :please review omairc\r\n"));
    QCOMPARE(spy.count(), 1);
    QVERIFY(controller.mentionFor(QStringLiteral("libera")));
}

void HighlightTest::nickChangeKeepsTheList()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    transport->injectBytes(
        QByteArrayLiteral(":fred!u@h JOIN :#omarchy\r\n"
                          ":fred!u@h JOIN :#other\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("/highlight omairc")));
    transport->injectBytes(QByteArrayLiteral(":fred!u@h NICK :fred2\r\n"));
    QVERIFY(IrcHighlightStore().contains(
        QStringLiteral("libera"), QStringLiteral("omairc"), IrcCaseMapping()));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#other"));
    QSignalSpy spy(&controller, &IrcController::mentionArrived);
    transport->injectBytes(
        QByteArrayLiteral(":Alice!u@h PRIVMSG #omarchy :please review omairc\r\n"));
    QCOMPARE(spy.count(), 1);
    QVERIFY(controller.mentionFor(QStringLiteral("libera")));
}

void HighlightTest::refusesEmptyAndExtraTokens()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    transport->injectBytes(QByteArrayLiteral(":fred!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(!controller.sendMessage(QStringLiteral("/highlight")));
    QVERIFY(!controller.sendMessage(QStringLiteral("/highlight   ")));
    QVERIFY(!controller.sendMessage(QStringLiteral("/highlight omairc extra")));
    QVERIFY(!controller.sendMessage(QStringLiteral("/highlights extra")));
    QVERIFY(!IrcHighlightStore().contains(
        QStringLiteral("libera"), QStringLiteral("omairc"), IrcCaseMapping()));

    QVERIFY(controller.sendMessage(QStringLiteral("/highlight omairc.example.net")));
    QVERIFY(controller.sendMessage(QStringLiteral("/highlight #omarchy")));
    QVERIFY(IrcHighlightStore().contains(
        QStringLiteral("libera"), QStringLiteral("omairc.example.net"),
        IrcCaseMapping()));
    QVERIFY(IrcHighlightStore().contains(
        QStringLiteral("libera"), QStringLiteral("#omarchy"), IrcCaseMapping()));
}

void HighlightTest::offlineHighlightIsNotConnected()
{
    IrcController controller;
    QVERIFY(!controller.sendMessage(QStringLiteral("/highlight omairc")));
    QCOMPARE(controller.lastError(),
             QStringLiteral("Select a connected conversation first"));
    QVERIFY(!IrcHighlightStore().contains(
        QStringLiteral("libera"), QStringLiteral("omairc"), IrcCaseMapping()));

    controller.openStatus(QStringLiteral("libera"));
    QVERIFY(controller.console()->submit(QStringLiteral("/highlight omairc")));
    QVERIFY(logContains(controller.console()->lines(), QStringLiteral("Not connected")));
    QVERIFY(!IrcHighlightStore().contains(
        QStringLiteral("libera"), QStringLiteral("omairc"), IrcCaseMapping()));
}

int runHighlightTests(int argc, char **argv)
{
    HighlightTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_highlight.moc"
