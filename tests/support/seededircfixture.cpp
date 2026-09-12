#include "seededircfixture.h"

#include "backend.h"
#include "fakeirctransport.h"
#include "irccontroller.h"
#include "ircsession.h"
#include "ircslashcomplete.h"

#include <QCoreApplication>
#include <QDir>
#include <QHash>
#include <QQmlApplicationEngine>
#include <QSet>
#include <QQmlComponent>
#include <QQuickWindow>
#include <QSettings>
#include <QTemporaryDir>
#include <QTest>
#include <QVariantMap>
#include <QVector>

namespace
{
constexpr auto kToday = "2026-09-12";
constexpr auto kYesterday = "2026-09-11";
constexpr auto kCaps =
    "echo-message message-tags away-notify multi-prefix batch "
    "draft/metadata-2 server-time";
constexpr auto kIsupport = "CHANTYPES=# PREFIX=(ov)@+";

struct SeedLine
{
    enum Kind { Chat, Join } kind = Chat;
    QString nick;
    QString body;
    QString day;
    QString hhmm;
};

struct SeedChannel
{
    QString name;
    QString topic;
    QStringList members;
    QStringList lateJoin;
    QStringList away;
    QVector<QPair<QString, QString>> statuses;
    QVector<SeedLine> lines;
    bool markRead = false;
};

struct SeedDirect
{
    QString nick;
    QVector<SeedLine> lines;
    bool markRead = false;
    bool typing = false;
};

struct SeedNetwork
{
    QString networkId;
    QString nick;
    QString welcome;
    QVector<SeedChannel> channels;
    QVector<SeedDirect> directs;
};

SeedLine chat(const QString &nick,
              const QString &body,
              const QString &hhmm,
              const char *day = kToday)
{
    return {SeedLine::Chat, nick, body, QString::fromLatin1(day), hhmm};
}

SeedLine join(const QString &nick)
{
    return {SeedLine::Join, nick, {}, {}, {}};
}

QByteArray line(const QString &text)
{
    return text.toUtf8() + QByteArrayLiteral("\r\n");
}

QByteArray privmsg(const SeedLine &row, const QString &target)
{
    QByteArray out;
    if (!row.day.isEmpty() && !row.hhmm.isEmpty()) {
        out += "@time=";
        out += row.day.toLatin1();
        out += "T";
        out += row.hhmm.toLatin1();
        out += ":00.000Z ";
    }
    out += ":";
    out += row.nick.toUtf8();
    out += "!u@h PRIVMSG ";
    out += target.toUtf8();
    out += " :";
    out += row.body.toUtf8();
    out += "\r\n";
    return out;
}

QByteArray joinLine(const QString &nick, const QString &channel)
{
    return line(QStringLiteral(":%1!u@h JOIN :%2").arg(nick, channel));
}

QStringList initialMembers(const SeedChannel &channel)
{
    QStringList names = channel.members;
    for (const QString &late : channel.lateJoin)
        names.removeAll(late);
    return names;
}

QByteArray registrationBytes(const QString &nick, const QString &welcome)
{
    QByteArray out;
    out += line(QStringLiteral(":server CAP %1 LS :%2")
                    .arg(nick, QLatin1String(kCaps)));
    out += line(QStringLiteral(":server CAP %1 ACK :%2")
                    .arg(nick, QLatin1String(kCaps)));
    out += line(QStringLiteral(":AUTH!AUTH@localhost NOTICE %1 "
                              ":*** Looking up your hostname...")
                    .arg(nick));
    out += line(QStringLiteral(":server 001 %1 :%2").arg(nick, welcome));
    out += line(QStringLiteral(":server 005 %1 %2 :are supported by this server")
                    .arg(nick, QLatin1String(kIsupport)));
    return out;
}

QByteArray channelStateBytes(const SeedNetwork &network, const SeedChannel &channel)
{
    QByteArray out;
    out += joinLine(network.nick, channel.name);
    out += line(QStringLiteral(":server 332 %1 %2 :%3")
                    .arg(network.nick, channel.name, channel.topic));
    out += line(QStringLiteral(":server 353 %1 = %2 :%3")
                    .arg(network.nick,
                         channel.name,
                         initialMembers(channel).join(QLatin1Char(' '))));
    out += line(QStringLiteral(":server 366 %1 %2 :End of NAMES")
                    .arg(network.nick, channel.name));
    return out;
}

QByteArray presenceBytes(const SeedNetwork &network)
{
    QByteArray out;
    QSet<QString> away;
    QHash<QString, QString> status;
    for (const SeedChannel &channel : network.channels) {
        for (const QString &nick : channel.away)
            away.insert(nick);
        for (const auto &entry : channel.statuses)
            status.insert(entry.first, entry.second);
    }
    const QStringList awayNicks = away.values();
    for (const QString &nick : awayNicks)
        out += line(QStringLiteral(":%1!u@h AWAY :away").arg(nick));
    const QStringList statusNicks = status.keys();
    for (const QString &nick : statusNicks) {
        out += line(QStringLiteral(":server 761 %1 %2 status * :%3")
                        .arg(network.nick, nick, status.value(nick)));
    }
    return out;
}

QByteArray transcriptBytes(const SeedNetwork &network)
{
    QByteArray out;
    for (const SeedChannel &channel : network.channels) {
        for (const SeedLine &row : channel.lines) {
            if (row.kind == SeedLine::Join)
                out += joinLine(row.nick, channel.name);
            else
                out += privmsg(row, channel.name);
        }
    }
    for (const SeedDirect &direct : network.directs) {
        for (const SeedLine &row : direct.lines)
            out += privmsg(row, network.nick);
    }
    return out;
}

QByteArray typingBytes(const SeedNetwork &network)
{
    QByteArray out;
    for (const SeedDirect &direct : network.directs) {
        if (!direct.typing)
            continue;
        out += line(QStringLiteral("@+typing=active :%1!u@h TAGMSG %2")
                        .arg(direct.nick, QStringLiteral("#omarchy")));
        out += line(QStringLiteral("@+typing=active :%1!u@h TAGMSG %2")
                        .arg(direct.nick, network.nick));
    }
    return out;
}

SeedNetwork omarchyWorld()
{
    SeedNetwork network;
    network.networkId = QStringLiteral("omarchy");
    network.nick = QStringLiteral("fred");
    network.welcome = QStringLiteral("Welcome to the mock network");

    SeedChannel omarchy;
    omarchy.name = QStringLiteral("#omarchy");
    omarchy.topic = QStringLiteral("A cozy corner for Omarchy users and builders.");
    omarchy.members = {QStringLiteral("anna"), QStringLiteral("dax"),
                       QStringLiteral("mira"), QStringLiteral("sol"),
                       QStringLiteral("fred"), QStringLiteral("kai"),
                       QStringLiteral("nora"), QStringLiteral("teo"),
                       QStringLiteral("lena"), QStringLiteral("sam"),
                       QStringLiteral("ivy"), QStringLiteral("max")};
    omarchy.lateJoin = {QStringLiteral("sol"), QStringLiteral("nora")};
    omarchy.away = {QStringLiteral("teo"), QStringLiteral("lena"),
                    QStringLiteral("sam"), QStringLiteral("ivy"),
                    QStringLiteral("max")};
    omarchy.statuses = {
        {QStringLiteral("anna"), QStringLiteral("writing docs")},
        {QStringLiteral("dax"), QStringLiteral("on #desktop")},
        {QStringLiteral("mira"), QStringLiteral("making tea")},
        {QStringLiteral("sol"), QStringLiteral("new here")},
        {QStringLiteral("fred"), QStringLiteral("building Omairc")},
    };
    omarchy.lines = {
        chat(QStringLiteral("anna"),
             QStringLiteral("Morning! Has anyone tried the new minimal install flow yet?"),
             QStringLiteral("09:41")),
        chat(QStringLiteral("dax"),
             QStringLiteral("Yes. Fresh install on my Framework took about twelve minutes. The defaults feel really considered."),
             QStringLiteral("09:43")),
        chat(QStringLiteral("mira"),
             QStringLiteral("The way the theme carries across the terminal and native apps is my favorite detail."),
             QStringLiteral("09:46")),
        join(QStringLiteral("sol")),
        chat(QStringLiteral("sol"),
             QStringLiteral("Hey all. Just landed here from Arch. This feels surprisingly calm."),
             QStringLiteral("09:52")),
        chat(QStringLiteral("anna"),
             QStringLiteral("Welcome, sol. Calm is the whole idea."),
             QStringLiteral("09:53")),
        chat(QStringLiteral("dax"),
             QStringLiteral("If you have not already, try the keyboard-first app launcher. It becomes muscle memory fast."),
             QStringLiteral("09:55")),
        chat(QStringLiteral("sol"),
             QStringLiteral("I found it. The shortcuts sheet is a nice touch too."),
             QStringLiteral("09:56")),
        chat(QStringLiteral("mira"),
             QStringLiteral("Most of the system makes sense once you learn three or four core bindings."),
             QStringLiteral("09:57")),
        chat(QStringLiteral("anna"),
             QStringLiteral("And everything important is still plain text when you want to look underneath."),
             QStringLiteral("09:58")),
        chat(QStringLiteral("kai"),
             QStringLiteral("That balance is hard to get right: friendly defaults without hiding the actual system."),
             QStringLiteral("09:59")),
        chat(QStringLiteral("dax"),
             QStringLiteral("Exactly. Start simple, then make it yours one deliberate change at a time."),
             QStringLiteral("10:00")),
        join(QStringLiteral("nora")),
        chat(QStringLiteral("nora"),
             QStringLiteral("Good timing. I was just looking for a quiet place to ask about native Omarchy apps."),
             QStringLiteral("10:01")),
        chat(QStringLiteral("fred"),
             QStringLiteral("I am sketching a tiny IRC client that belongs here. No browser chrome, no clutter."),
             QStringLiteral("10:02")),
        chat(QStringLiteral("mira"),
             QStringLiteral("Keep the member list optional and I am sold."),
             QStringLiteral("10:04")),
    };

    SeedChannel desktop;
    desktop.name = QStringLiteral("#desktop");
    desktop.topic = QStringLiteral("Desktops should feel personal, fast, and calm.");
    desktop.members = {QStringLiteral("anna"), QStringLiteral("dax"),
                       QStringLiteral("mira"), QStringLiteral("sol"),
                       QStringLiteral("fred"), QStringLiteral("kai"),
                       QStringLiteral("nora"), QStringLiteral("teo")};
    desktop.lines = {
        chat(QStringLiteral("dax"),
             QStringLiteral("I finally moved every workspace rule into a small, readable file."),
             QStringLiteral("08:22")),
        chat(QStringLiteral("mira"),
             QStringLiteral("That is the dream. Configuration you can understand in one sitting."),
             QStringLiteral("08:24")),
        chat(QStringLiteral("sol"),
             QStringLiteral("Does anyone use a vertical monitor alongside the main display?"),
             QStringLiteral("10:08")),
    };

    SeedChannel ricing;
    ricing.name = QStringLiteral("#ricing");
    ricing.topic = QStringLiteral("Themes, type, wallpapers, and the tiny details.");
    ricing.members = {QStringLiteral("anna"), QStringLiteral("dax"),
                      QStringLiteral("mira"), QStringLiteral("sol"),
                      QStringLiteral("fred"), QStringLiteral("kai"),
                      QStringLiteral("nora"), QStringLiteral("teo"),
                      QStringLiteral("lena"), QStringLiteral("sam")};
    ricing.lines = {
        chat(QStringLiteral("anna"),
             QStringLiteral("Muted colors, one strong accent, and enough breathing room."),
             QStringLiteral("18:10"),
             kYesterday),
        chat(QStringLiteral("mira"),
             QStringLiteral("Typography does more work than decoration ever will."),
             QStringLiteral("18:13"),
             kYesterday),
        chat(QStringLiteral("dax"),
             QStringLiteral("Dropped a new warm theme in the usual place. It looks great after sunset."),
             QStringLiteral("18:20"),
             kYesterday),
    };
    const int ricingUnreadTarget = 12;
    const int ricingMentionLine = 1;
    const int ricingPadCount =
        ricingUnreadTarget - ricing.lines.size() - ricingMentionLine;
    for (int index = 1; index <= ricingPadCount; ++index) {
        ricing.lines.append(chat(QStringLiteral("anna"),
                                 QStringLiteral("rice-pad-%1").arg(index),
                                 QStringLiteral("18:2%1").arg(index),
                                 kYesterday));
    }
    ricing.lines.append(chat(QStringLiteral("mira"),
                             QStringLiteral("looks good, fred."),
                             QStringLiteral("18:29"),
                             kYesterday));

    SeedChannel help;
    help.name = QStringLiteral("#help");
    help.topic = QStringLiteral("Ask a clear question. Share what you already tried.");
    help.members = {QStringLiteral("anna"), QStringLiteral("dax"),
                    QStringLiteral("mira"), QStringLiteral("sol"),
                    QStringLiteral("fred")};
    help.markRead = true;
    help.lines = {
        chat(QStringLiteral("mira"),
             QStringLiteral("Tip: include the command output and the exact behavior you expected."),
             QStringLiteral("09:11")),
        chat(QStringLiteral("sol"),
             QStringLiteral("That made my monitor issue much easier to diagnose. Thanks."),
             QStringLiteral("09:15")),
    };

    SeedDirect anna;
    anna.nick = QStringLiteral("anna");
    anna.typing = true;
    anna.lines = {
        chat(QStringLiteral("anna"),
             QStringLiteral("fred: The prototype already feels at home. Nice work."),
             QStringLiteral("10:12")),
    };

    SeedDirect dax;
    dax.nick = QStringLiteral("dax");
    dax.markRead = true;
    dax.lines = {
        chat(QStringLiteral("dax"),
             QStringLiteral("Send me the build when the mock is ready."),
             QStringLiteral("18:40"),
             kYesterday),
    };

    network.channels = {omarchy, desktop, ricing, help};
    network.directs = {anna, dax};
    return network;
}

SeedNetwork oftcWorld()
{
    SeedNetwork network;
    network.networkId = QStringLiteral("oftc");
    network.nick = QStringLiteral("oak");
    network.welcome = QStringLiteral("Welcome to the mock OFTC network");

    SeedChannel omarchy;
    omarchy.name = QStringLiteral("#omarchy");
    omarchy.topic = QStringLiteral("A different #omarchy, hosted on OFTC.");
    omarchy.members = {QStringLiteral("rio"), QStringLiteral("ness"),
                       QStringLiteral("oak"), QStringLiteral("pip")};
    omarchy.away = {QStringLiteral("pip")};
    omarchy.statuses = {
        {QStringLiteral("rio"), QStringLiteral("on #lab")},
        {QStringLiteral("ness"), QStringLiteral("watching CI")},
        {QStringLiteral("oak"), QStringLiteral("building Omairc")},
    };
    omarchy.markRead = true;
    omarchy.lines = {
        chat(QStringLiteral("rio"),
             QStringLiteral("This #omarchy is the OFTC one. Different people, same name."),
             QStringLiteral("11:02")),
        chat(QStringLiteral("oak"),
             QStringLiteral("Good. If the sidebar mixed them, we would already be lost."),
             QStringLiteral("11:04")),
    };

    SeedChannel lab;
    lab.name = QStringLiteral("#lab");
    lab.topic = QStringLiteral("Build lab for packaging and CI.");
    lab.members = {QStringLiteral("rio"), QStringLiteral("ness"),
                   QStringLiteral("oak"), QStringLiteral("pip"),
                   QStringLiteral("jules"), QStringLiteral("remy")};
    lab.away = {QStringLiteral("pip")};
    lab.markRead = true;
    lab.lines = {
        chat(QStringLiteral("ness"),
             QStringLiteral("Package build is green on the new runner."),
             QStringLiteral("10:18")),
        chat(QStringLiteral("rio"),
             QStringLiteral("Leave the log in #build if it fails after sunset."),
             QStringLiteral("10:21")),
    };

    SeedChannel build;
    build.name = QStringLiteral("#build");
    build.topic = QStringLiteral("Nightly builds and failing tests.");
    build.members = {QStringLiteral("ness"), QStringLiteral("oak"),
                     QStringLiteral("rio")};
    build.lines = {
        chat(QStringLiteral("ness"),
             QStringLiteral("Nightly failed on missing qt6keychain. Looking."),
             QStringLiteral("09:05")),
        chat(QStringLiteral("oak"),
             QStringLiteral("Patched. Waiting on the next image."),
             QStringLiteral("09:12")),
        chat(QStringLiteral("remy"),
             QStringLiteral("Image is cooking."),
             QStringLiteral("09:13")),
    };

    SeedDirect rio;
    rio.nick = QStringLiteral("rio");
    rio.markRead = true;
    rio.lines = {
        chat(QStringLiteral("rio"),
             QStringLiteral("Ping me on OFTC, not Libera."),
             QStringLiteral("11:40")),
    };

    network.channels = {omarchy, lab, build};
    network.directs = {rio};
    return network;
}

IrcSessionConfig sessionConfig(const QString &networkId,
                               const QString &nick,
                               const QStringList &autojoin)
{
    IrcSessionConfig value;
    value.networkId = networkId;
    value.host = QStringLiteral("irc.example");
    value.tlsEnabled = true;
    value.nick = nick;
    value.username = nick;
    value.realname = nick;
    value.autojoinChannels = autojoin;
    value.reconnectEnabled = false;
    return value;
}

QStringList autojoinNames(const SeedNetwork &network)
{
    QStringList names;
    for (const SeedChannel &channel : network.channels)
        names.append(channel.name);
    return names;
}

void injectWorld(FakeIrcTransport *transport, const SeedNetwork &network)
{
    QByteArray out;
    for (const SeedChannel &channel : network.channels)
        out += channelStateBytes(network, channel);
    out += presenceBytes(network);
    transport->injectBytes(out);
}

void injectTranscript(FakeIrcTransport *transport, const SeedNetwork &network)
{
    transport->injectBytes(transcriptBytes(network));
}

void markRead(IrcController &controller, const SeedNetwork &network)
{
    for (const SeedChannel &channel : network.channels) {
        if (channel.markRead)
            controller.selectConversation(network.networkId, channel.name);
    }
    for (const SeedDirect &direct : network.directs) {
        if (direct.markRead)
            controller.selectConversation(network.networkId, direct.nick);
    }
}
}

SeededIrcFixture::SeededIrcFixture(QObject *parent)
    : QObject(parent)
{
}

SeededIrcFixture::~SeededIrcFixture()
{
    m_window.clear();
    m_root.reset();
    m_engine.reset();
    m_controller.reset();
    m_omarchyTransport = nullptr;
    m_oftcTransport = nullptr;
    m_slash.reset();
    m_backend.reset();
    m_xdg.reset();
}

QString SeededIrcFixture::omarchyNetworkId()
{
    return QStringLiteral("omarchy");
}

QString SeededIrcFixture::oftcNetworkId()
{
    return QStringLiteral("oftc");
}

Backend &SeededIrcFixture::backend()
{
    return *m_backend;
}

IrcSlashSession &SeededIrcFixture::slash()
{
    return *m_slash;
}

IrcController &SeededIrcFixture::controller()
{
    return *m_controller;
}

FakeIrcTransport *SeededIrcFixture::omarchyTransport() const
{
    return m_omarchyTransport;
}

FakeIrcTransport *SeededIrcFixture::oftcTransport() const
{
    return m_oftcTransport;
}

QQuickWindow *SeededIrcFixture::window() const
{
    return m_window;
}

QString SeededIrcFixture::lastError() const
{
    return m_error;
}

QObject *SeededIrcFixture::backendObject() const
{
    return m_backend.get();
}

QObject *SeededIrcFixture::irc() const
{
    return m_controller.get();
}

QObject *SeededIrcFixture::slashObject() const
{
    return m_slash.get();
}

QObject *SeededIrcFixture::omarchyTransportObject() const
{
    return m_omarchyTransport;
}

QObject *SeededIrcFixture::oftcTransportObject() const
{
    return m_oftcTransport;
}

QObject *SeededIrcFixture::windowObject() const
{
    return m_window;
}

bool SeededIrcFixture::fail(const QString &why)
{
    m_error = why;
    emit lastErrorChanged();
    return false;
}

bool SeededIrcFixture::installXdg()
{
    m_xdg = std::make_unique<QTemporaryDir>();
    if (!m_xdg->isValid())
        return fail(QStringLiteral("xdg temp dir"));
    const QString root = m_xdg->path();
    const QString config = root + QLatin1String("/config");
    QDir().mkpath(config);
    QDir().mkpath(root + QLatin1String("/cache"));
    QDir().mkpath(root + QLatin1String("/data"));
    qputenv("XDG_CONFIG_HOME", config.toUtf8());
    qputenv("XDG_CACHE_HOME", (root + QLatin1String("/cache")).toUtf8());
    qputenv("XDG_DATA_HOME", (root + QLatin1String("/data")).toUtf8());
    QSettings::setPath(QSettings::NativeFormat, QSettings::UserScope, config);
    return true;
}

bool SeededIrcFixture::startNetwork(const QString &networkId,
                                    const QString &nick,
                                    const QStringList &autojoin,
                                    FakeIrcTransport **transport)
{
    auto *owned = new FakeIrcTransport;
    if (!m_controller->addSession(sessionConfig(networkId, nick, autojoin), owned)) {
        delete owned;
        return fail(QStringLiteral("addSession %1").arg(networkId));
    }
    *transport = owned;
    if (!m_controller->start(networkId))
        return fail(QStringLiteral("start %1").arg(networkId));
    owned->completeConnect();
    return true;
}

bool SeededIrcFixture::open()
{
    if (m_controller)
        return fail(QStringLiteral("already open"));
    if (!installXdg())
        return false;

    m_backend = std::make_unique<Backend>();
    m_slash = std::make_unique<IrcSlashSession>();
    m_controller = std::make_unique<IrcController>();

    const SeedNetwork omarchy = omarchyWorld();
    const SeedNetwork oftc = oftcWorld();
    if (!startNetwork(omarchy.networkId, omarchy.nick, autojoinNames(omarchy),
                      &m_omarchyTransport)) {
        return false;
    }
    if (!startNetwork(oftc.networkId, oftc.nick, autojoinNames(oftc),
                      &m_oftcTransport)) {
        return false;
    }
    m_controller->setNetworkOrder({omarchy.networkId, oftc.networkId});

    m_omarchyTransport->injectBytes(registrationBytes(omarchy.nick, omarchy.welcome));
    m_oftcTransport->injectBytes(registrationBytes(oftc.nick, oftc.welcome));
    injectWorld(m_omarchyTransport, omarchy);
    injectWorld(m_oftcTransport, oftc);

    m_controller->selectConversation(omarchy.networkId, QStringLiteral("#omarchy"));
    injectTranscript(m_omarchyTransport, omarchy);
    injectTranscript(m_oftcTransport, oftc);
    markRead(*m_controller, omarchy);
    markRead(*m_controller, oftc);
    m_controller->selectConversation(omarchy.networkId, QStringLiteral("#omarchy"));
    m_omarchyTransport->injectBytes(typingBytes(omarchy));
    emit openedChanged();
    return true;
}

bool SeededIrcFixture::createWindow()
{
    if (!m_controller)
        return fail(QStringLiteral("open first"));
    if (m_window)
        return fail(QStringLiteral("window already created"));

    m_engine = std::make_unique<QQmlApplicationEngine>();
    QQmlComponent component(m_engine.get(), QUrl(QStringLiteral("qrc:/OmaircWindow.qml")));
    if (component.status() == QQmlComponent::Error)
        return fail(component.errorString());

    QVariantMap properties;
    properties.insert(QStringLiteral("backend"), QVariant::fromValue(m_backend.get()));
    properties.insert(QStringLiteral("irc"), QVariant::fromValue(m_controller.get()));
    properties.insert(QStringLiteral("slashCommands"), QVariant::fromValue(m_slash.get()));
    m_root.reset(component.createWithInitialProperties(properties));
    if (!m_root)
        return fail(component.errorString());
    m_window = qobject_cast<QQuickWindow *>(m_root.get());
    if (!m_window)
        return fail(QStringLiteral("root is not QQuickWindow"));
    if (!QTest::qWaitForWindowExposed(m_window))
        return fail(QStringLiteral("window not exposed"));
    m_window->setWidth(1180);
    m_window->setHeight(760);
    m_controller->selectConversation(omarchyNetworkId(), QStringLiteral("#omarchy"));
    QCoreApplication::processEvents();
    emit windowChanged();
    return true;
}
