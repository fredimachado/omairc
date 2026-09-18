#include "ircdemoserver.h"

#include "irccontroller.h"
#include "ircloopbacktransport.h"
#include "ircnetworkprofile.h"
#include "ircparser.h"
#include "ircpresence.h"
#include "ircprofilestore.h"
#include "ircserverfeatures.h"
#include "ircsession.h"
#include "irctcp.h"
#include "ircwiretext.h"

#include <QDateTime>
#include <QHash>
#include <QPair>
#include <QSet>
#include <QVector>
#include <QtGlobal>

#include <string>
#include <string_view>
#include <vector>

namespace
{
constexpr auto kToday = "2026-09-12";
constexpr auto kYesterday = "2026-09-11";
constexpr auto kCaps =
    "echo-message message-tags away-notify multi-prefix batch "
    "draft/metadata-2 server-time";
constexpr auto kIsupport = "CHANTYPES=# PREFIX=(qaohv)~&@%+";

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
    QVector<QPair<QString, QString>> ranks;
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
    QString iconUrl;
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

/// A NAMES reply carries the PREFIX symbols in front of the nick. The demo
/// network advertises the full ladder, so a member may carry more than one.
QStringList namesTokens(const SeedChannel &channel)
{
    QStringList tokens = initialMembers(channel);
    for (QString &token : tokens) {
        for (const auto &rank : channel.ranks) {
            if (rank.first != token)
                continue;
            token.prepend(rank.second);
            break;
        }
    }
    return tokens;
}

QByteArray registrationBytes(const QString &nick,
                             const QString &welcome,
                             const QString &iconUrl = {})
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
    QString isupport = QLatin1String(kIsupport);
    if (!iconUrl.isEmpty())
        isupport += QStringLiteral(" draft/ICON=%1").arg(iconUrl);
    out += line(QStringLiteral(":server 005 %1 %2 :are supported by this server")
                    .arg(nick, isupport));
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
                         namesTokens(channel).join(QLatin1Char(' '))));
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
    QSet<QString> members;
    for (const SeedChannel &channel : network.channels) {
        for (const QString &nick : channel.members)
            members.insert(nick);
    }
    if (members.contains(QStringLiteral("dax"))) {
        out += line(QStringLiteral(":server 761 %1 dax bot * :PacketBot")
                        .arg(network.nick));
    }
    // Bundled demo art loads through the avatar store's qrc path so
    // --demo-server shows real glyphs without outbound HTTPS or
    // weakening ircAvatarUrlIsSafe for network URLs.
    if (members.contains(QStringLiteral("mira"))) {
        out += line(
            QStringLiteral(
                ":server 761 %1 mira avatar * :qrc:/demo/mira-avatar.png")
                .arg(network.nick));
    }
    if (members.contains(QStringLiteral("anna"))) {
        out += line(
            QStringLiteral(
                ":server 761 %1 anna avatar * :qrc:/demo/anna-avatar.png")
                .arg(network.nick));
        out += line(QStringLiteral(":server 761 %1 anna display-name * :Anna Docs")
                        .arg(network.nick));
        out += line(QStringLiteral(":server 761 %1 anna pronouns * :she/her")
                        .arg(network.nick));
    }
    if (members.contains(QStringLiteral("kai"))) {
        out += line(
            QStringLiteral(
                ":server 761 %1 kai avatar * :qrc:/demo/kai-avatar.png")
                .arg(network.nick));
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
    network.welcome = QStringLiteral("Welcome to the demo network");
    network.iconUrl = QStringLiteral("qrc:/demo/omarchy-icon.png");

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
    // The demo ladder: founder, admin, two ops, halfop, voice, then the rest
    // plain. `mira` holds two ranks at once, which `multi-prefix` allows, and
    // `teo` is away and voiced, because rank and presence are independent.
    //
    // The `lateJoin` members arrive through the transcript, and an arriving
    // member carries no rank until a server says so, so they stay out of this
    // list and out of the initial NAMES.
    omarchy.ranks = {
        {QStringLiteral("fred"), QStringLiteral("~")},
        {QStringLiteral("anna"), QStringLiteral("&")},
        {QStringLiteral("dax"), QStringLiteral("@")},
        {QStringLiteral("mira"), QStringLiteral("@+")},
        {QStringLiteral("kai"), QStringLiteral("%")},
        {QStringLiteral("teo"), QStringLiteral("+")},
    };
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
    network.welcome = QStringLiteral("Welcome to the demo OFTC network");

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

void injectWorld(IrcLoopbackTransport *transport, const SeedNetwork &network)
{
    QByteArray out;
    for (const SeedChannel &channel : network.channels)
        out += channelStateBytes(network, channel);
    out += presenceBytes(network);
    transport->injectBytes(out);
}

void injectTranscript(IrcLoopbackTransport *transport, const SeedNetwork &network)
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

bool writeSeedProfile(const SeedNetwork &network)
{
    IrcNetworkProfile profile;
    profile.networkId = network.networkId;
    profile.host = QStringLiteral("irc.example");
    profile.port = 6697;
    profile.tlsEnabled = true;
    profile.connectOnStartup = false;
    profile.nick = network.nick;
    profile.username = network.nick;
    profile.realname = network.nick;
    profile.autojoinChannels = autojoinNames(network);
    if (network.networkId == QStringLiteral("omarchy"))
        profile.iconColor = 1;
    else if (network.networkId == QStringLiteral("oftc"))
        profile.iconColor = 2;
    if (!profile.isComplete())
        return false;
    IrcProfileStore().save(profile);
    return true;
}

void injectClientEcho(IrcLoopbackTransport *transport, const QString &nick,
                      const QByteArray &frame)
{
    transport->injectBytes(":" + nick.toUtf8() + "!u@h " + frame);
}

// A real server answers AWAY with 306 (now away) or 305 (no longer away). The
// demo does not echo our own away-notify back, which is the case the client has
// to cover on its own.
bool tryAnswerAway(IrcLoopbackTransport *transport, const QString &selfNick,
                   const QByteArray &frame)
{
    if (!transport || selfNick.isEmpty())
        return false;
    QByteArray wire = frame;
    if (wire.endsWith("\r\n"))
        wire.chop(2);
    else if (wire.endsWith('\n'))
        wire.chop(1);
    if (wire != "AWAY" && !wire.startsWith("AWAY :"))
        return false;
    const bool away = wire.size() > int(qstrlen("AWAY"));
    transport->injectBytes(QByteArrayLiteral(":server ")
                           + (away ? QByteArrayLiteral("306 ") : QByteArrayLiteral("305 "))
                           + selfNick.toUtf8()
                           + (away
                                  ? QByteArrayLiteral(" :You have been marked as being away\r\n")
                                  : QByteArrayLiteral(" :You are no longer marked as being away\r\n")));
    return true;
}

// Real servers answer METADATA SET with 761 / 766 (or 764 / 767 / 769 on
// failure) and typically do not send a separate METADATA notification to self.
// The demo mirrors that reply shape so the client applies standing status from
// the numeric response, not from optimistic local state.
bool tryAnswerMetadata(IrcLoopbackTransport *transport, const QString &selfNick,
                       const QByteArray &frame)
{
    if (!transport || selfNick.isEmpty())
        return false;
    QByteArray wire = frame;
    if (wire.endsWith("\r\n"))
        wire.chop(2);
    else if (wire.endsWith('\n'))
        wire.chop(1);
    if (!wire.startsWith("METADATA "))
        return false;

    const IrcParseResult parsed = IrcParser::parse(
        std::string_view(wire.constData(), std::size_t(wire.size())));
    if (!parsed || parsed.value->command != "METADATA"
        || parsed.value->parameters.size() < 3) {
        return false;
    }

    const QString target = ircWireText(parsed.value->parameters[0]);
    if (target != QLatin1String("*")
        && target.compare(selfNick, Qt::CaseInsensitive) != 0) {
        return false;
    }
    const QString subcommand = ircWireText(parsed.value->parameters[1]);
    if (subcommand.compare(QLatin1String("SET"), Qt::CaseInsensitive) != 0)
        return false;

    const QString key = ircWireText(parsed.value->parameters[2]);
    const QString stored = IrcMetadata::canonicalKey(key);
    if (stored.isEmpty())
        return false;

    QString value;
    if (parsed.value->parameters.size() >= 4)
        value = IrcMetadata::clamped(ircWireText(parsed.value->parameters[3]));
    if (value.isEmpty()) {
        transport->injectBytes(line(QStringLiteral(":server 766 %1 %2 %3 :unset")
                                        .arg(selfNick, selfNick, stored)));
        return true;
    }
    transport->injectBytes(line(QStringLiteral(":server 761 %1 %2 %3 * :%4")
                                    .arg(selfNick, selfNick, stored, value)));
    return true;
}

IrcServerFeatures demoServerFeatures()
{
    IrcServerFeatures features;
    std::vector<std::string> tokens;
    const QByteArray raw = QByteArray::fromRawData(kIsupport, int(qstrlen(kIsupport)));
    for (const QByteArray &token : raw.split(' ')) {
        if (!token.isEmpty())
            tokens.emplace_back(token.constData(), std::size_t(token.size()));
    }
    features.applyTokens(tokens);
    return features;
}

bool tryAnswerCtcp(IrcLoopbackTransport *transport, const QString &selfNick,
                   const QByteArray &frame)
{
    if (!transport || selfNick.isEmpty() || !frame.startsWith("PRIVMSG "))
        return false;

    QByteArray wire = frame;
    if (wire.endsWith("\r\n"))
        wire.chop(2);
    else if (wire.endsWith('\n'))
        wire.chop(1);

    const IrcParseResult parsed = IrcParser::parse(
        std::string_view(wire.constData(), std::size_t(wire.size())));
    if (!parsed || parsed.value->command != "PRIVMSG"
        || parsed.value->parameters.size() < 2) {
        return false;
    }

    const auto request = parseCtcpRequest(ircWireText(parsed.value->parameters[1]));
    if (!request || request->command == QLatin1String("ACTION"))
        return false;

    static const IrcServerFeatures features = demoServerFeatures();
    const std::string &target = parsed.value->parameters[0];
    if (features.isChannel(target))
        return false;

    QString argument;
    if (request->command == QLatin1String("PING")) {
        argument = request->argument;
    } else if (request->command == QLatin1String("TIME")) {
        argument = QDateTime::currentDateTime().toString(Qt::RFC2822Date);
    } else if (request->command == QLatin1String("VERSION")) {
        argument = ctcpVersionReplyText();
    } else {
        return false;
    }

    const QString targetNick = ircWireText(target);
    if (targetNick.isEmpty())
        return false;

    transport->injectBytes(":" + targetNick.toUtf8() + "!u@h NOTICE "
                           + selfNick.toUtf8() + " :"
                           + ctcpPayload({request->command, argument}).toUtf8()
                           + "\r\n");
    return true;
}

bool echoLastPrivmsg(IrcLoopbackTransport *transport, const QString &nick)
{
    if (!transport || nick.isEmpty())
        return false;
    const QByteArrayList frames = transport->writtenFrames();
    for (int index = frames.size() - 1; index >= 0; --index) {
        const QByteArray &frame = frames.at(index);
        if (!frame.startsWith("PRIVMSG "))
            continue;
        injectClientEcho(transport, nick, frame);
        return true;
    }
    return false;
}

}

IrcDemoServer::IrcDemoServer(QObject *parent)
    : QObject(parent)
{
}

IrcDemoServer::~IrcDemoServer() = default;

QString IrcDemoServer::omarchyNetworkId()
{
    return QStringLiteral("omarchy");
}

QString IrcDemoServer::oftcNetworkId()
{
    return QStringLiteral("oftc");
}

IrcLoopbackTransport *IrcDemoServer::omarchyTransport() const
{
    return m_omarchyTransport;
}

IrcLoopbackTransport *IrcDemoServer::oftcTransport() const
{
    return m_oftcTransport;
}

QString IrcDemoServer::lastError() const
{
    return m_error;
}

bool IrcDemoServer::fail(const QString &why)
{
    m_error = why;
    return false;
}

bool IrcDemoServer::writeProfiles()
{
    return writeSeedProfile(omarchyWorld()) && writeSeedProfile(oftcWorld());
}

bool IrcDemoServer::startNetwork(IrcController &controller,
                                const QString &networkId,
                                const QString &nick,
                                const QStringList &autojoin,
                                IrcLoopbackTransport **transport)
{
    auto *owned = new IrcLoopbackTransport;
    if (!controller.addSession(sessionConfig(networkId, nick, autojoin), owned)) {
        delete owned;
        return fail(QStringLiteral("addSession %1").arg(networkId));
    }
    *transport = owned;
    if (!controller.start(networkId))
        return fail(QStringLiteral("start %1").arg(networkId));
    owned->completeConnect();
    return true;
}

void IrcDemoServer::hookAutoEcho(IrcLoopbackTransport *transport, const QString &nick)
{
    QObject::connect(transport, &IrcLoopbackTransport::frameWritten, this,
                     [transport, nick](const QByteArray &frame) {
        if (tryAnswerCtcp(transport, nick, frame))
            return;
        if (tryAnswerAway(transport, nick, frame))
            return;
        if (tryAnswerMetadata(transport, nick, frame))
            return;
        if (frame.startsWith("PRIVMSG ") || frame.startsWith("NOTICE "))
            injectClientEcho(transport, nick, frame);
    });
}

bool IrcDemoServer::attach(IrcController &controller, bool autoEcho)
{
    const SeedNetwork omarchy = omarchyWorld();
    const SeedNetwork oftc = oftcWorld();
    if (!startNetwork(controller, omarchy.networkId, omarchy.nick,
                      autojoinNames(omarchy), &m_omarchyTransport)) {
        return false;
    }
    if (!startNetwork(controller, oftc.networkId, oftc.nick,
                      autojoinNames(oftc), &m_oftcTransport)) {
        return false;
    }
    controller.setNetworkOrder({omarchy.networkId, oftc.networkId});

    m_omarchyTransport->injectBytes(
        registrationBytes(omarchy.nick, omarchy.welcome, omarchy.iconUrl));
    m_oftcTransport->injectBytes(registrationBytes(oftc.nick, oftc.welcome));
    injectWorld(m_omarchyTransport, omarchy);
    injectWorld(m_oftcTransport, oftc);

    controller.selectConversation(omarchy.networkId, QStringLiteral("#omarchy"));
    injectTranscript(m_omarchyTransport, omarchy);
    injectTranscript(m_oftcTransport, oftc);
    markRead(controller, omarchy);
    markRead(controller, oftc);
    controller.selectConversation(omarchy.networkId, QStringLiteral("#omarchy"));
    m_omarchyTransport->injectBytes(typingBytes(omarchy));
    if (autoEcho) {
        hookAutoEcho(m_omarchyTransport, omarchy.nick);
        hookAutoEcho(m_oftcTransport, oftc.nick);
    }
    return true;
}

void IrcDemoServer::injectOmarchy(const QByteArray &bytes)
{
    if (m_omarchyTransport)
        m_omarchyTransport->injectBytes(bytes);
}

void IrcDemoServer::injectOftc(const QByteArray &bytes)
{
    if (m_oftcTransport)
        m_oftcTransport->injectBytes(bytes);
}

bool IrcDemoServer::echoLastOmarchyPrivmsg(const QString &nick)
{
    return echoLastPrivmsg(m_omarchyTransport, nick);
}

bool IrcDemoServer::echoLastOftcPrivmsg(const QString &nick)
{
    return echoLastPrivmsg(m_oftcTransport, nick);
}
