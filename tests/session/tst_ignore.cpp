#include <QAbstractItemModel>
#include <QCoreApplication>
#include <QFile>
#include <QSettings>
#include <QTemporaryDir>
#include <QTest>

#include "conversationlistmodel.h"
#include "fakeirctransport.h"
#include "irccommand.h"
#include "irccontroller.h"
#include "ircignore.h"
#include "ircnetworkprofile.h"
#include "ircparser.h"
#include "ircprofilestore.h"
#include "ircslashcomplete.h"
#include "messagelistmodel.h"
#include "networklogmodel.h"

#include <memory>
#include <string_view>

namespace
{
IrcMessage mustParse(std::string_view line)
{
    const IrcParseResult parsed = IrcParser::parse(line);
    if (!parsed)
        qFatal("parse failed");
    return *parsed.value;
}

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

bool framesContain(const QByteArrayList& frames, const QByteArray& needle)
{
    for (const QByteArray& frame : frames) {
        if (frame.contains(needle))
            return true;
    }
    return false;
}
}

class IgnoreTest : public QObject
{
    Q_OBJECT

private slots:
    void init();
    void storeRoundTripsAndIsIdempotent();
    void storeMatchesRfc1459AndSurvivesProfileSave();
    void storeForgetDropsTheGroup();
    void filterDropsPrivateNoticeInviteAndKeepsChannel();
    void parseAndCatalog();
    void controllerMutesPrivateTrafficAndListsOnStatus();
    void controllerForgetsWithTheNetwork();
    void discardSessionKeepsTheList();
    void refusesBadNicks();
    void offlineIgnoreIsNotConnected();

private:
    QString settingsFile() const;

    std::unique_ptr<QTemporaryDir> m_dir;
};

void IgnoreTest::init()
{
    m_dir = std::make_unique<QTemporaryDir>();
    QVERIFY(m_dir->isValid());
    qputenv("XDG_CONFIG_HOME", m_dir->path().toUtf8());
    QSettings::setPath(QSettings::NativeFormat, QSettings::UserScope, m_dir->path());
    QCoreApplication::setOrganizationName(QStringLiteral("omairc"));
    QCoreApplication::setApplicationName(QStringLiteral("omairc"));
}

QString IgnoreTest::settingsFile() const
{
    QSettings settings;
    return settings.fileName();
}

void IgnoreTest::storeRoundTripsAndIsIdempotent()
{
    const IrcCaseMapping mapping;
    IrcIgnoreStore store;
    QVERIFY(store.add(QStringLiteral("net-a"), QStringLiteral("lena"), mapping));
    QVERIFY(!store.add(QStringLiteral("net-a"), QStringLiteral("Lena"), mapping));
    QCOMPARE(store.nicks(QStringLiteral("net-a")),
             QStringList{QStringLiteral("lena")});
    QVERIFY(store.contains(QStringLiteral("net-a"), QStringLiteral("LENA"), mapping));
    QVERIFY(!store.remove(QStringLiteral("net-a"), QStringLiteral("bob"), mapping));
    QVERIFY(store.remove(QStringLiteral("net-a"), QStringLiteral("lena"), mapping));
    QVERIFY(store.nicks(QStringLiteral("net-a")).isEmpty());
}

void IgnoreTest::storeMatchesRfc1459AndSurvivesProfileSave()
{
    const IrcCaseMapping mapping;
    IrcIgnoreStore store;
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

    QVERIFY(IrcIgnoreStore().contains(
        QStringLiteral("net-a"), QStringLiteral("nick{"), mapping));

    QFile file(settingsFile());
    QVERIFY(file.open(QIODevice::ReadOnly | QIODevice::Text));
    const QString contents = QString::fromUtf8(file.readAll());
    QVERIFY(contents.contains(QLatin1String("ignores")));
    QVERIFY(contents.contains(QLatin1String("Nick[")));
    QVERIFY(!contents.contains(QLatin1String("password"), Qt::CaseInsensitive));
}

void IgnoreTest::storeForgetDropsTheGroup()
{
    const IrcCaseMapping mapping;
    IrcIgnoreStore store;
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

void IgnoreTest::filterDropsPrivateNoticeInviteAndKeepsChannel()
{
    const IrcCaseMapping mapping;
    QStringList nicks{QStringLiteral("lena")};
    const IrcServerFeatures features;

    QVERIFY(ircIgnoreDropsInbound(
        mustParse(":lena!u@h PRIVMSG omairc :hi"), QStringLiteral("omairc"), nicks,
        features));
    QVERIFY(ircIgnoreDropsInbound(
        mustParse(":lena!u@h PRIVMSG omairc :\x01VERSION\x01"),
        QStringLiteral("omairc"), nicks, features));
    QVERIFY(ircIgnoreDropsInbound(
        mustParse(":lena!u@h NOTICE omairc :psst"), QStringLiteral("omairc"), nicks,
        features));
    QVERIFY(ircIgnoreDropsInbound(
        mustParse(":lena!u@h NOTICE #omarchy :heads up"), QStringLiteral("omairc"),
        nicks, features));
    QVERIFY(ircIgnoreDropsInbound(
        mustParse(":lena!u@h INVITE omairc :#spam"), QStringLiteral("omairc"), nicks,
        features));
    QVERIFY(!ircIgnoreDropsInbound(
        mustParse(":lena!u@h PRIVMSG #omarchy :hello"), QStringLiteral("omairc"),
        nicks, features));
    QVERIFY(!ircIgnoreDropsInbound(
        mustParse(":lena!u@h PRIVMSG #omarchy :\x01ACTION waves\x01"),
        QStringLiteral("omairc"), nicks, features));
    QVERIFY(!ircIgnoreDropsInbound(
        mustParse(":bob!u@h PRIVMSG omairc :hi"), QStringLiteral("omairc"), nicks,
        features));
    QVERIFY(!ircIgnoreDropsInbound(
        mustParse(":lena!u@h JOIN :#omarchy"), QStringLiteral("omairc"), nicks,
        features));
    QVERIFY(ircIgnoreDropsInbound(
        mustParse(":Nick[!u@h PRIVMSG omairc :hi"), QStringLiteral("omairc"),
        QStringList{QStringLiteral("nick{")}, features));
    QVERIFY(ircIgnoreDropsInbound(
        mustParse(":lena PRIVMSG omairc :hi"), QStringLiteral("omairc"), nicks,
        features));
    QVERIFY(ircIgnoreDropsInbound(
        mustParse(":lena NOTICE omairc :psst"), QStringLiteral("omairc"), nicks,
        features));
    QVERIFY(!ircIgnoreDropsInbound(
        mustParse(":irc.example.net PRIVMSG omairc :hi"), QStringLiteral("omairc"),
        nicks, features));
}

void IgnoreTest::parseAndCatalog()
{
    QCOMPARE(IrcCommand::parse(QStringLiteral("/ignore lena")).verb,
             IrcCommand::Verb::Ignore);
    QCOMPARE(IrcCommand::parse(QStringLiteral("/UNIGNORE lena")).verb,
             IrcCommand::Verb::Unignore);
    QCOMPARE(IrcCommand::parse(QStringLiteral("/ignored")).verb,
             IrcCommand::Verb::Ignored);
    QVERIFY(IrcCommand::parse(QStringLiteral("/ignore lena"))
                .allowedOn(IrcComposerSurface::Status));
    QCOMPARE(IrcVerbTable::all().size(), 20);
    QCOMPARE(IrcVerbTable::visibleOn(IrcComposerSurface::Status).size(), 17);
    QCOMPARE(IrcVerbTable::visibleOn(IrcComposerSurface::Conversation).size(), 20);
    const auto probe = IrcSlashComplete::project(
        QStringLiteral("/ig"), IrcComposerSurface::Conversation);
    QVERIFY(probe.isOpen());
    QCOMPARE(probe.hits().first().label, QStringLiteral("/ignore"));
}

void IgnoreTest::controllerMutesPrivateTrafficAndListsOnStatus()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(sessionConfig(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/ignore lena")));
    QVERIFY(logContains(controller.console()->lines(), QStringLiteral("Ignoring lena")));

    transport->injectBytes(
        QByteArrayLiteral(":lena!u@h PRIVMSG omairc :secret\r\n"
                          ":lena PRIVMSG omairc :bare\r\n"
                          ":lena!u@h NOTICE omairc :psst\r\n"
                          ":lena!u@h NOTICE #omarchy :heads up\r\n"
                          ":lena!u@h INVITE omairc :#spam\r\n"
                          ":lena!u@h PRIVMSG omairc :\x01VERSION\x01\r\n"
                          ":lena!u@h PRIVMSG #omarchy :still here\r\n"
                          ":bob!u@h PRIVMSG omairc :open me\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("lena")), -1);
    QVERIFY(rowForTarget(conversations, QStringLiteral("bob")) >= 0);
    QVERIFY(rowForTarget(conversations, QStringLiteral("#omarchy")) >= 0);
    QVERIFY(!logContains(controller.console()->lines(), QStringLiteral("secret")));
    QVERIFY(!logContains(controller.console()->lines(), QStringLiteral("bare")));
    QVERIFY(!logContains(controller.console()->lines(), QStringLiteral("-lena-")));
    QVERIFY(!logContains(controller.console()->lines(), QStringLiteral("heads up")));
    QVERIFY(!logContains(controller.console()->lines(), QStringLiteral("#spam")));
    QVERIFY(!logContains(controller.console()->lines(), QStringLiteral("invited you")));
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("NOTICE lena :")));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    bool sawChannel = false;
    for (int row = 0; row < messages->rowCount(); ++row) {
        if (messages->data(messages->index(row, 0), MessageListModel::BodyRole)
                .toString()
            == QStringLiteral("still here")) {
            sawChannel = true;
        }
    }
    QVERIFY(sawChannel);

    QVERIFY(controller.sendMessage(QStringLiteral("/ignored")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("Ignoring: lena")));
    QVERIFY(controller.sendMessage(QStringLiteral("/ignore lena")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("Already ignoring lena")));
    QVERIFY(controller.sendMessage(QStringLiteral("/unignore lena")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("No longer ignoring lena")));
    QVERIFY(controller.sendMessage(QStringLiteral("/ignored")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("Not ignoring anyone")));
}

void IgnoreTest::controllerForgetsWithTheNetwork()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    controller.openStatus(QStringLiteral("libera"));
    QVERIFY(controller.console()->submit(QStringLiteral("/ignore lena")));
    QVERIFY(IrcIgnoreStore().contains(
        QStringLiteral("libera"), QStringLiteral("lena"), IrcCaseMapping()));
    controller.forgetNetworkState(QStringLiteral("libera"));
    QVERIFY(!IrcIgnoreStore().contains(
        QStringLiteral("libera"), QStringLiteral("lena"), IrcCaseMapping()));
}

void IgnoreTest::discardSessionKeepsTheList()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    controller.openStatus(QStringLiteral("libera"));
    QVERIFY(controller.console()->submit(QStringLiteral("/ignore lena")));
    QVERIFY(IrcIgnoreStore().contains(
        QStringLiteral("libera"), QStringLiteral("lena"), IrcCaseMapping()));
    QVERIFY(controller.discardSession(QStringLiteral("libera")));
    QVERIFY(IrcIgnoreStore().contains(
        QStringLiteral("libera"), QStringLiteral("lena"), IrcCaseMapping()));

    auto *again = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), again));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(again);
    again->injectBytes(QByteArrayLiteral(":lena!u@h PRIVMSG omairc :secret\r\n"));
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("lena")), -1);
}

void IgnoreTest::refusesBadNicks()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig(), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(!controller.sendMessage(QStringLiteral("/ignore")));
    QVERIFY(!controller.sendMessage(QStringLiteral("/ignore #omarchy")));
    QVERIFY(!controller.sendMessage(QStringLiteral("/ignore lena extra")));
    QVERIFY(!controller.sendMessage(QStringLiteral("/ignore lena!u@h")));
    QVERIFY(!controller.sendMessage(QStringLiteral("/ignored extra")));
    QVERIFY(!controller.sendMessage(QStringLiteral("/unignore *")));
}

void IgnoreTest::offlineIgnoreIsNotConnected()
{
    IrcController controller;
    QVERIFY(!controller.sendMessage(QStringLiteral("/ignore lena")));
    QCOMPARE(controller.lastError(),
             QStringLiteral("Select a connected conversation first"));
    QVERIFY(!IrcIgnoreStore().contains(
        QStringLiteral("libera"), QStringLiteral("lena"), IrcCaseMapping()));

    controller.openStatus(QStringLiteral("libera"));
    QVERIFY(controller.console()->submit(QStringLiteral("/ignore lena")));
    QVERIFY(logContains(controller.console()->lines(), QStringLiteral("Not connected")));
    QVERIFY(!IrcIgnoreStore().contains(
        QStringLiteral("libera"), QStringLiteral("lena"), IrcCaseMapping()));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(!controller.sendMessage(QStringLiteral("/ignore lena")));
    QCOMPARE(controller.lastError(), QStringLiteral("Not connected"));
    QVERIFY(!IrcIgnoreStore().contains(
        QStringLiteral("libera"), QStringLiteral("lena"), IrcCaseMapping()));
}

int runIgnoreTests(int argc, char **argv)
{
    IgnoreTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_ignore.moc"
