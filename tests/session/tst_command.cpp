#include <QCryptographicHash>
#include <QAbstractItemModel>
#include <QCoreApplication>
#include <QSettings>
#include <QTemporaryDir>
#include <QSignalSpy>
#include <QTest>
#include <QVariant>

#include "fakeirctransport.h"
#include "testsettings.h"
#include "storage/credentialstore.h"
#include "ircavatarurl.h"
#include "ircconnection.h"
#include "ircchannelmode.h"
#include "irccommand.h"
#include "irccontroller.h"
#include "ircnetworkprofile.h"
#include "ircprofilestore.h"
#include "ircjointarget.h"
#include "ircpresence.h"
#include "ircserverfeatures.h"
#include "ircsession.h"
#include "ircslashcomplete.h"
#include "ircstatusentry.h"
#include "channellistmodel.h"
#include "memberlistmodel.h"
#include "messagelistmodel.h"
#include "networklogmodel.h"

#include <memory>
#include <type_traits>

namespace
{
class CommandCredentialStore final : public CredentialStore
{
public:
    explicit CommandCredentialStore(QObject *parent = nullptr)
        : CredentialStore(parent)
    {
    }

    void read(const CredentialKey &) override
    {
        emit readFinished(State::Missing, {}, {});
    }

    void write(const CredentialKey &, const QString &) override
    {
        emit writeFinished(State::Missing, {});
    }

    void remove(const CredentialKey &) override
    {
        emit writeFinished(State::Missing, {});
    }
};

IrcNetworkProfile liberaStoredProfile()
{
    IrcNetworkProfile profile = IrcNetworkProfile::create();
    profile.networkId = QStringLiteral("libera");
    profile.host = QStringLiteral("irc.example");
    profile.name = profile.host;
    profile.port = 6697;
    profile.tlsEnabled = true;
    profile.nick = QStringLiteral("omairc");
    profile.username = QStringLiteral("omairc");
    profile.realname = QStringLiteral("Omairc User");
    return profile;
}

IrcConnection::TransportFactory nullTransportFactory()
{
    return []() -> IrcTransport * { return nullptr; };
}

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

void welcomeMetadata(FakeIrcTransport *transport)
{
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :away-notify batch draft/metadata-2\r\n"
                          ":server CAP omairc ACK :away-notify batch draft/metadata-2\r\n"
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

bool selectedBodiesContain(QAbstractItemModel *messages, const QString& needle)
{
    if (!messages)
        return false;
    for (int row = 0; row < messages->rowCount(); ++row) {
        const QString body =
            messages->data(messages->index(row, 0), MessageListModel::BodyRole)
                .toString();
        if (body.contains(needle))
            return true;
    }
    return false;
}

int selectedBodyHits(QAbstractItemModel *messages, const QString& needle)
{
    int hits = 0;
    if (!messages)
        return hits;
    for (int row = 0; row < messages->rowCount(); ++row) {
        const QString body =
            messages->data(messages->index(row, 0), MessageListModel::BodyRole)
                .toString();
        if (body.contains(needle))
            ++hits;
    }
    return hits;
}

bool framesContain(const QByteArrayList& frames, const QByteArray& needle)
{
    for (const QByteArray& frame : frames) {
        if (frame.contains(needle))
            return true;
    }
    return false;
}

QVariant memberRole(QAbstractItemModel *members, const QString& nick, int role)
{
    if (!members)
        return {};
    for (int row = 0; row < members->rowCount(); ++row) {
        if (members->data(members->index(row, 0), MemberListModel::NickRole)
                .toString()
            == nick) {
            return members->data(members->index(row, 0), role);
        }
    }
    return {};
}

struct ParsedChannelMode {
    bool ok = false;
    bool query = false;
    QString channel;
    QString modes;
    QStringList parameters;
};

ParsedChannelMode parsedChannelMode(const QString& argument)
{
    ParsedChannelMode result;
    const auto request = IrcChannelModeRequest::parse(argument, IrcServerFeatures());
    if (!request)
        return result;
    result.ok = true;
    result.channel = request->channel();
    request->visit([&](const auto& payload) {
        using T = std::decay_t<decltype(payload)>;
        if constexpr (std::is_same_v<T, IrcChannelModeRequest::Query>) {
            result.query = true;
        } else {
            result.modes = payload.modes;
            result.parameters = payload.parameters;
        }
    });
    return result;
}
}

class CommandTest : public QObject
{
    Q_OBJECT

private slots:
    void init();
    void parseEmptyAndSay();
    void parseSlashEscape();
    void parseVerbsAndAliases();
    void parseUnknown();
    void parseClose();
    void parseQuery();
    void parseTopic();
    void parseNotice();
    void parseAwayAndBack();
    void parseAutoaway();
    void parseStatus();
    void parseAvatar();
    void parseWhois();
    void parseCtcpQueries();
    void parseMode();
    void parseKick();
    void parseInvite();
    void parseWrappersAndHelp();
    void parseChannelModeRequest();
    void parseJoinTargets();
    void catalogLookupAndScope();
    void closeWrongScopeUsesCatalogSentence();
    void conversationSendAndUnknown();
    void joinSendsKeyedFrames();
    void statusSubmitDoesNotSendAction();
    void awayAndBackWriteAwayFrames();
    void statusWritesMetadataFrames();
    void statusRefusesOnMetadataError();
    void statusRefusesOnMetadataFailReplies();
    void ownMetadataFailKeylessPreservesConcurrentWatches();
    void statusRefusesNonEmptyWhenMaxValueBytesZero();
    void avatarRefusesNonEmptyWhenMaxValueBytesZero();
    void avatarClearTreatsKeyNotSetAsSuccess();
    void avatarWritesMetadataFrames();
    void avatarAppliesSavedUrlOnConnect();
    void avatarAppliesSavedUrlAfterLateCaps();
    void avatarRefusesUnsafeInput();
    void avatarRefusesOnMetadataFailReplies();
    void whoisSendsAndDefaults();
    void ctcpSendsAndDefaults();
    void modeSendsAndRefuses();
    void wrappersSendAndHelp();
    void listSendsAndCaches();
    void listFiltersPlainTopic();
    void listSerializesInFlightMaskChange();
    void listTryAgainUnwedgesAndRetries();
    void listTooManyMatchesUnwedges();
    void listIdleTimeoutUnwedges();
    void listIdleTimeoutLateEndDoesNotCompleteRetry();
    void listIdleTimeoutLateEndAfterRetryStartDoesNotComplete();
    void listLoadingPresentsOwnNetwork();
    void slashProjectClosed();
    void slashProjectOpen();
    void slashSessionKeys();

private:
    std::unique_ptr<QTemporaryDir> m_settingsDir;
};

void CommandTest::init()
{
    m_settingsDir = std::make_unique<QTemporaryDir>();
    QVERIFY(m_settingsDir->isValid());
    TestSettings::isolate(m_settingsDir->path());
    QCoreApplication::setOrganizationName(QStringLiteral("omairc"));
    QCoreApplication::setApplicationName(QStringLiteral("omairc"));
}

void CommandTest::parseEmptyAndSay()
{
    QCOMPARE(IrcCommand::parse(QString()).verb, IrcCommand::Verb::Empty);
    QCOMPARE(IrcCommand::parse(QStringLiteral("   ")).verb, IrcCommand::Verb::Empty);
    const IrcCommand say = IrcCommand::parse(QStringLiteral("hello"));
    QCOMPARE(say.verb, IrcCommand::Verb::Say);
    QCOMPARE(say.argument, QStringLiteral("hello"));
    QVERIFY(say.isLiveMessage());
}

void CommandTest::parseSlashEscape()
{
    const IrcCommand escaped = IrcCommand::parse(QStringLiteral("//hi"));
    QCOMPARE(escaped.verb, IrcCommand::Verb::Say);
    QCOMPARE(escaped.argument, QStringLiteral("/hi"));
    QVERIFY(escaped.isLiveMessage());

    const IrcCommand triple = IrcCommand::parse(QStringLiteral("///x"));
    QCOMPARE(triple.verb, IrcCommand::Verb::Say);
    QCOMPARE(triple.argument, QStringLiteral("//x"));
}

void CommandTest::parseVerbsAndAliases()
{
    const IrcCommand action = IrcCommand::parse(QStringLiteral("/ME waves"));
    QCOMPARE(action.verb, IrcCommand::Verb::Action);
    QCOMPARE(action.argument, QStringLiteral("waves"));
    QCOMPARE(action.name, QStringLiteral("/ME"));
    QVERIFY(action.isLiveMessage());

    const IrcCommand join = IrcCommand::parse(QStringLiteral("/j #omarchy"));
    QCOMPARE(join.verb, IrcCommand::Verb::Join);
    QCOMPARE(join.argument, QStringLiteral("#omarchy"));
    QVERIFY(!join.isLiveMessage());

    const IrcCommand part = IrcCommand::parse(QStringLiteral("/LEAVE #omarchy"));
    QCOMPARE(part.verb, IrcCommand::Verb::Part);
    QCOMPARE(part.argument, QStringLiteral("#omarchy"));

    const IrcCommand barePart = IrcCommand::parse(QStringLiteral("/part"));
    QCOMPARE(barePart.verb, IrcCommand::Verb::Part);
    QVERIFY(barePart.argument.isEmpty());

    const IrcCommand bareLeave = IrcCommand::parse(QStringLiteral("/leave"));
    QCOMPARE(bareLeave.verb, IrcCommand::Verb::Part);
    QVERIFY(bareLeave.argument.isEmpty());

    const IrcCommand bareJoin = IrcCommand::parse(QStringLiteral("/join"));
    QCOMPARE(bareJoin.verb, IrcCommand::Verb::Join);
    QVERIFY(bareJoin.argument.isEmpty());

    const IrcCommand disconnect = IrcCommand::parse(QStringLiteral("/disconnect later"));
    QCOMPARE(disconnect.verb, IrcCommand::Verb::Quit);
    QCOMPARE(disconnect.argument, QStringLiteral("later"));

    const IrcCommand quit = IrcCommand::parse(QStringLiteral("/quit later"));
    QCOMPARE(quit.verb, IrcCommand::Verb::Quit);
    QCOMPARE(quit.argument, QStringLiteral("later"));
}

void CommandTest::parseUnknown()
{
    const IrcCommand unknown = IrcCommand::parse(QStringLiteral("/nope"));
    QCOMPARE(unknown.verb, IrcCommand::Verb::Unknown);
    QCOMPARE(unknown.name, QStringLiteral("/nope"));

    const IrcCommand withArg = IrcCommand::parse(QStringLiteral("/nope extra"));
    QCOMPARE(withArg.verb, IrcCommand::Verb::Unknown);
    QCOMPARE(withArg.argument, QStringLiteral("extra"));

    const IrcCommand notQuit = IrcCommand::parse(QStringLiteral("/q"));
    QCOMPARE(notQuit.verb, IrcCommand::Verb::Unknown);

    const IrcCommand notTopic = IrcCommand::parse(QStringLiteral("/t"));
    QCOMPARE(notTopic.verb, IrcCommand::Verb::Unknown);
}

void CommandTest::parseClose()
{
    const IrcCommand close = IrcCommand::parse(QStringLiteral("/close"));
    QCOMPARE(close.verb, IrcCommand::Verb::Close);
    QCOMPARE(close.name, QStringLiteral("/close"));
    QVERIFY(close.argument.isEmpty());
    QVERIFY(!close.isLiveMessage());
    QVERIFY(close.allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(!close.allowedOn(IrcComposerSurface::Status));

    const IrcCommand escaped = IrcCommand::parse(QStringLiteral("//close"));
    QCOMPARE(escaped.verb, IrcCommand::Verb::Say);
    QCOMPARE(escaped.argument, QStringLiteral("/close"));
    QVERIFY(escaped.isLiveMessage());
}

void CommandTest::parseQuery()
{
    const IrcCommand query = IrcCommand::parse(QStringLiteral("/query lena hi"));
    QCOMPARE(query.verb, IrcCommand::Verb::Query);
    QCOMPARE(query.argument, QStringLiteral("lena hi"));
    QCOMPARE(query.name, QStringLiteral("/query"));
    QVERIFY(!query.isLiveMessage());
    QVERIFY(query.allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(query.allowedOn(IrcComposerSurface::Status));

    const IrcCommand foldedMsg = IrcCommand::parse(QStringLiteral("/MSG lena"));
    QCOMPARE(foldedMsg.verb, IrcCommand::Verb::Msg);
    QCOMPARE(foldedMsg.argument, QStringLiteral("lena"));
    QCOMPARE(foldedMsg.name, QStringLiteral("/MSG"));
    QVERIFY(!foldedMsg.isLiveMessage());

    const IrcCommand msg = IrcCommand::parse(QStringLiteral("/msg lena hi"));
    QCOMPARE(msg.verb, IrcCommand::Verb::Msg);
    QCOMPARE(msg.argument, QStringLiteral("lena hi"));
    QCOMPARE(msg.name, QStringLiteral("/msg"));
    QVERIFY(!msg.isLiveMessage());
    QVERIFY(msg.allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(msg.allowedOn(IrcComposerSurface::Status));

    const IrcCommand escapedQuery = IrcCommand::parse(QStringLiteral("//query"));
    QCOMPARE(escapedQuery.verb, IrcCommand::Verb::Say);
    QCOMPARE(escapedQuery.argument, QStringLiteral("/query"));
    QVERIFY(escapedQuery.isLiveMessage());

    const IrcCommand escapedMsg = IrcCommand::parse(QStringLiteral("//msg hi"));
    QCOMPARE(escapedMsg.verb, IrcCommand::Verb::Say);
    QCOMPARE(escapedMsg.argument, QStringLiteral("/msg hi"));
    QVERIFY(escapedMsg.isLiveMessage());
}

void CommandTest::parseTopic()
{
    const IrcCommand bare = IrcCommand::parse(QStringLiteral("/topic"));
    QCOMPARE(bare.verb, IrcCommand::Verb::Topic);
    QCOMPARE(bare.name, QStringLiteral("/topic"));
    QVERIFY(bare.argument.isEmpty());
    QVERIFY(!bare.isLiveMessage());
    QVERIFY(bare.allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(!bare.allowedOn(IrcComposerSurface::Status));

    const IrcCommand text = IrcCommand::parse(QStringLiteral("/TOPIC Hello world"));
    QCOMPARE(text.verb, IrcCommand::Verb::Topic);
    QCOMPARE(text.argument, QStringLiteral("Hello world"));
    QCOMPARE(text.name, QStringLiteral("/TOPIC"));
    QVERIFY(!text.isLiveMessage());

    const IrcCommand hash = IrcCommand::parse(
        QStringLiteral("/topic #omarchy is the place"));
    QCOMPARE(hash.verb, IrcCommand::Verb::Topic);
    QCOMPARE(hash.argument, QStringLiteral("#omarchy is the place"));
    QVERIFY(!hash.isLiveMessage());
}

void CommandTest::parseNotice()
{
    const IrcCommand notice = IrcCommand::parse(QStringLiteral("/notice lena later"));
    QCOMPARE(notice.verb, IrcCommand::Verb::Notice);
    QCOMPARE(notice.argument, QStringLiteral("lena later"));
    QCOMPARE(notice.name, QStringLiteral("/notice"));
    QVERIFY(!notice.isLiveMessage());
    QVERIFY(notice.allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(notice.allowedOn(IrcComposerSurface::Status));

    const IrcCommand channel = IrcCommand::parse(QStringLiteral("/NOTICE #omarchy hi"));
    QCOMPARE(channel.verb, IrcCommand::Verb::Notice);
    QCOMPARE(channel.argument, QStringLiteral("#omarchy hi"));
    QCOMPARE(channel.name, QStringLiteral("/NOTICE"));
    QVERIFY(!channel.isLiveMessage());

    const IrcCommand escaped = IrcCommand::parse(QStringLiteral("//notice hi"));
    QCOMPARE(escaped.verb, IrcCommand::Verb::Say);
    QCOMPARE(escaped.argument, QStringLiteral("/notice hi"));
    QVERIFY(escaped.isLiveMessage());
}

void CommandTest::parseAwayAndBack()
{
    const IrcCommand lunch = IrcCommand::parse(QStringLiteral("/away lunch"));
    QCOMPARE(lunch.verb, IrcCommand::Verb::Away);
    QCOMPARE(lunch.argument, QStringLiteral("lunch"));
    QCOMPARE(lunch.name, QStringLiteral("/away"));
    QVERIFY(!lunch.isLiveMessage());
    QVERIFY(lunch.allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(lunch.allowedOn(IrcComposerSurface::Status));

    const IrcCommand bareAway = IrcCommand::parse(QStringLiteral("/away"));
    QCOMPARE(bareAway.verb, IrcCommand::Verb::Away);
    QVERIFY(bareAway.argument.isEmpty());
    QVERIFY(!bareAway.isLiveMessage());

    const IrcCommand foldedAway = IrcCommand::parse(QStringLiteral("/AWAY lunch"));
    QCOMPARE(foldedAway.verb, IrcCommand::Verb::Away);
    QCOMPARE(foldedAway.argument, QStringLiteral("lunch"));
    QCOMPARE(foldedAway.name, QStringLiteral("/AWAY"));
    QVERIFY(!foldedAway.isLiveMessage());

    const IrcCommand back = IrcCommand::parse(QStringLiteral("/back"));
    QCOMPARE(back.verb, IrcCommand::Verb::Back);
    QVERIFY(back.argument.isEmpty());
    QCOMPARE(back.name, QStringLiteral("/back"));
    QVERIFY(!back.isLiveMessage());
    QVERIFY(back.allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(back.allowedOn(IrcComposerSurface::Status));

    const IrcCommand leftover = IrcCommand::parse(QStringLiteral("/back leftover"));
    QCOMPARE(leftover.verb, IrcCommand::Verb::Back);
    QVERIFY(leftover.argument.isEmpty());
    QVERIFY(!leftover.isLiveMessage());

    const IrcCommand foldedBack = IrcCommand::parse(QStringLiteral("/BACK leftover"));
    QCOMPARE(foldedBack.verb, IrcCommand::Verb::Back);
    QVERIFY(foldedBack.argument.isEmpty());
    QCOMPARE(foldedBack.name, QStringLiteral("/BACK"));
    QVERIFY(!foldedBack.isLiveMessage());

    const IrcCommand escaped = IrcCommand::parse(QStringLiteral("//away lunch"));
    QCOMPARE(escaped.verb, IrcCommand::Verb::Say);
    QCOMPARE(escaped.argument, QStringLiteral("/away lunch"));
    QVERIFY(escaped.isLiveMessage());
}

void CommandTest::parseAutoaway()
{
    const IrcCommand query = IrcCommand::parse(QStringLiteral("/autoaway"));
    QCOMPARE(query.verb, IrcCommand::Verb::Autoaway);
    QVERIFY(query.argument.isEmpty());
    QCOMPARE(query.name, QStringLiteral("/autoaway"));
    QVERIFY(!query.isLiveMessage());
    QVERIFY(query.allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(query.allowedOn(IrcComposerSurface::Status));

    const IrcCommand timeout = IrcCommand::parse(QStringLiteral("/autoaway 15"));
    QCOMPARE(timeout.verb, IrcCommand::Verb::Autoaway);
    QCOMPARE(timeout.argument, QStringLiteral("15"));

    const IrcCommand folded = IrcCommand::parse(QStringLiteral("/AUTOAWAY off"));
    QCOMPARE(folded.verb, IrcCommand::Verb::Autoaway);
    QCOMPARE(folded.argument, QStringLiteral("off"));
    QCOMPARE(folded.name, QStringLiteral("/AUTOAWAY"));

    const IrcCommand escaped = IrcCommand::parse(QStringLiteral("//autoaway 15"));
    QCOMPARE(escaped.verb, IrcCommand::Verb::Say);
    QCOMPARE(escaped.argument, QStringLiteral("/autoaway 15"));
    QVERIFY(escaped.isLiveMessage());
}

void CommandTest::parseStatus()
{
    const IrcCommand empty = IrcCommand::parse(QStringLiteral("/status"));
    QCOMPARE(empty.verb, IrcCommand::Verb::Status);
    QVERIFY(empty.argument.isEmpty());
    QCOMPARE(empty.name, QStringLiteral("/status"));
    QVERIFY(!empty.isLiveMessage());
    QVERIFY(empty.allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(empty.allowedOn(IrcComposerSurface::Status));

    const IrcCommand writing = IrcCommand::parse(QStringLiteral("/status writing docs"));
    QCOMPARE(writing.verb, IrcCommand::Verb::Status);
    QCOMPARE(writing.argument, QStringLiteral("writing docs"));
    QCOMPARE(writing.name, QStringLiteral("/status"));
    QVERIFY(!writing.isLiveMessage());

    const IrcCommand foldedClear = IrcCommand::parse(QStringLiteral("/STATUS clear"));
    QCOMPARE(foldedClear.verb, IrcCommand::Verb::Status);
    QCOMPARE(foldedClear.argument, QStringLiteral("clear"));
    QCOMPARE(foldedClear.name, QStringLiteral("/STATUS"));
    QVERIFY(!foldedClear.isLiveMessage());

    const IrcCommand keepClear = IrcCommand::parse(QStringLiteral("/status clear the table"));
    QCOMPARE(keepClear.verb, IrcCommand::Verb::Status);
    QCOMPARE(keepClear.argument, QStringLiteral("clear the table"));

    const IrcCommand escaped = IrcCommand::parse(QStringLiteral("//status lunch"));
    QCOMPARE(escaped.verb, IrcCommand::Verb::Say);
    QCOMPARE(escaped.argument, QStringLiteral("/status lunch"));
    QVERIFY(escaped.isLiveMessage());
}

void CommandTest::parseAvatar()
{
    const IrcCommand empty = IrcCommand::parse(QStringLiteral("/avatar"));
    QCOMPARE(empty.verb, IrcCommand::Verb::Avatar);
    QVERIFY(empty.argument.isEmpty());
    QCOMPARE(empty.name, QStringLiteral("/avatar"));
    QVERIFY(!empty.isLiveMessage());
    QVERIFY(empty.allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(empty.allowedOn(IrcComposerSurface::Status));

    const IrcCommand url = IrcCommand::parse(
        QStringLiteral("/avatar https://example.com/a.png"));
    QCOMPARE(url.verb, IrcCommand::Verb::Avatar);
    QCOMPARE(url.argument, QStringLiteral("https://example.com/a.png"));

    const IrcCommand email = IrcCommand::parse(QStringLiteral("/avatar me@example.com"));
    QCOMPARE(email.verb, IrcCommand::Verb::Avatar);
    QCOMPARE(email.argument, QStringLiteral("me@example.com"));

    const IrcCommand foldedClear = IrcCommand::parse(QStringLiteral("/AVATAR clear"));
    QCOMPARE(foldedClear.verb, IrcCommand::Verb::Avatar);
    QCOMPARE(foldedClear.argument, QStringLiteral("clear"));
}

void CommandTest::parseWhois()
{
    const IrcCommand whois = IrcCommand::parse(QStringLiteral("/whois lena"));
    QCOMPARE(whois.verb, IrcCommand::Verb::Whois);
    QCOMPARE(whois.argument, QStringLiteral("lena"));
    QCOMPARE(whois.name, QStringLiteral("/whois"));
    QVERIFY(!whois.isLiveMessage());
    QVERIFY(whois.allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(whois.allowedOn(IrcComposerSurface::Status));

    const IrcCommand folded = IrcCommand::parse(QStringLiteral("/WHOIS"));
    QCOMPARE(folded.verb, IrcCommand::Verb::Whois);
    QVERIFY(folded.argument.isEmpty());
    QCOMPARE(folded.name, QStringLiteral("/WHOIS"));
    QVERIFY(!folded.isLiveMessage());

    const IrcCommand empty = IrcCommand::parse(QStringLiteral("/whois"));
    QCOMPARE(empty.verb, IrcCommand::Verb::Whois);
    QVERIFY(empty.argument.isEmpty());
    QVERIFY(!empty.isLiveMessage());

    const IrcCommand escapedWhois = IrcCommand::parse(QStringLiteral("//whois lena"));
    QCOMPARE(escapedWhois.verb, IrcCommand::Verb::Say);
    QCOMPARE(escapedWhois.argument, QStringLiteral("/whois lena"));
    QVERIFY(escapedWhois.isLiveMessage());
}

void CommandTest::parseCtcpQueries()
{
    const IrcCommand ping = IrcCommand::parse(QStringLiteral("/ping lena"));
    QCOMPARE(ping.verb, IrcCommand::Verb::Ping);
    QCOMPARE(ping.argument, QStringLiteral("lena"));
    QCOMPARE(ping.name, QStringLiteral("/ping"));
    QVERIFY(!ping.isLiveMessage());
    QVERIFY(ping.allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(ping.allowedOn(IrcComposerSurface::Status));

    const IrcCommand foldedPing = IrcCommand::parse(QStringLiteral("/PING"));
    QCOMPARE(foldedPing.verb, IrcCommand::Verb::Ping);
    QVERIFY(foldedPing.argument.isEmpty());

    const IrcCommand time = IrcCommand::parse(QStringLiteral("/TIME lena"));
    QCOMPARE(time.verb, IrcCommand::Verb::Time);
    QCOMPARE(time.argument, QStringLiteral("lena"));
    QCOMPARE(time.name, QStringLiteral("/TIME"));
    QVERIFY(time.allowedOn(IrcComposerSurface::Status));

    const IrcCommand version = IrcCommand::parse(QStringLiteral("/version lena"));
    QCOMPARE(version.verb, IrcCommand::Verb::Version);
    QCOMPARE(version.argument, QStringLiteral("lena"));
    QVERIFY(version.allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(version.allowedOn(IrcComposerSurface::Status));

    const IrcCommand empty = IrcCommand::parse(QStringLiteral("/version"));
    QCOMPARE(empty.verb, IrcCommand::Verb::Version);
    QVERIFY(empty.argument.isEmpty());

    const IrcCommand escaped = IrcCommand::parse(QStringLiteral("//ping lena"));
    QCOMPARE(escaped.verb, IrcCommand::Verb::Say);
    QCOMPARE(escaped.argument, QStringLiteral("/ping lena"));
    QVERIFY(escaped.isLiveMessage());
}

void CommandTest::parseMode()
{
    const IrcCommand mode = IrcCommand::parse(QStringLiteral("/mode #omarchy +o lena"));
    QCOMPARE(mode.verb, IrcCommand::Verb::Mode);
    QCOMPARE(mode.argument, QStringLiteral("#omarchy +o lena"));
    QCOMPARE(mode.name, QStringLiteral("/mode"));
    QVERIFY(!mode.isLiveMessage());
    QVERIFY(mode.allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(mode.allowedOn(IrcComposerSurface::Status));

    const IrcCommand folded = IrcCommand::parse(QStringLiteral("/MODE #omarchy"));
    QCOMPARE(folded.verb, IrcCommand::Verb::Mode);
    QCOMPARE(folded.argument, QStringLiteral("#omarchy"));
    QCOMPARE(folded.name, QStringLiteral("/MODE"));
    QVERIFY(!folded.isLiveMessage());

    const IrcCommand empty = IrcCommand::parse(QStringLiteral("/mode"));
    QCOMPARE(empty.verb, IrcCommand::Verb::Mode);
    QVERIFY(empty.argument.isEmpty());
    QVERIFY(!empty.isLiveMessage());

    const IrcCommand escaped = IrcCommand::parse(QStringLiteral("//mode #omarchy"));
    QCOMPARE(escaped.verb, IrcCommand::Verb::Say);
    QCOMPARE(escaped.argument, QStringLiteral("/mode #omarchy"));
    QVERIFY(escaped.isLiveMessage());
}

void CommandTest::parseKick()
{
    const IrcCommand kick = IrcCommand::parse(QStringLiteral("/kick bob spam"));
    QCOMPARE(kick.verb, IrcCommand::Verb::Kick);
    QCOMPARE(kick.argument, QStringLiteral("bob spam"));
    QCOMPARE(kick.name, QStringLiteral("/kick"));
    QVERIFY(!kick.isLiveMessage());
    QVERIFY(kick.allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(kick.allowedOn(IrcComposerSurface::Status));

    const IrcCommand folded = IrcCommand::parse(QStringLiteral("/KICK #omarchy bob"));
    QCOMPARE(folded.verb, IrcCommand::Verb::Kick);
    QCOMPARE(folded.argument, QStringLiteral("#omarchy bob"));
    QCOMPARE(folded.name, QStringLiteral("/KICK"));
    QVERIFY(!folded.isLiveMessage());

    const IrcCommand empty = IrcCommand::parse(QStringLiteral("/kick"));
    QCOMPARE(empty.verb, IrcCommand::Verb::Kick);
    QVERIFY(empty.argument.isEmpty());
    QVERIFY(!empty.isLiveMessage());

    const IrcCommand escaped = IrcCommand::parse(QStringLiteral("//kick bob"));
    QCOMPARE(escaped.verb, IrcCommand::Verb::Say);
    QCOMPARE(escaped.argument, QStringLiteral("/kick bob"));
    QVERIFY(escaped.isLiveMessage());
}

void CommandTest::parseInvite()
{
    const IrcCommand invite = IrcCommand::parse(QStringLiteral("/invite bob #lab"));
    QCOMPARE(invite.verb, IrcCommand::Verb::Invite);
    QCOMPARE(invite.argument, QStringLiteral("bob #lab"));
    QCOMPARE(invite.name, QStringLiteral("/invite"));
    QVERIFY(!invite.isLiveMessage());
    QVERIFY(invite.allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(invite.allowedOn(IrcComposerSurface::Status));

    const IrcCommand folded = IrcCommand::parse(QStringLiteral("/INVITE bob"));
    QCOMPARE(folded.verb, IrcCommand::Verb::Invite);
    QCOMPARE(folded.argument, QStringLiteral("bob"));
    QCOMPARE(folded.name, QStringLiteral("/INVITE"));
    QVERIFY(!folded.isLiveMessage());

    const IrcCommand empty = IrcCommand::parse(QStringLiteral("/invite"));
    QCOMPARE(empty.verb, IrcCommand::Verb::Invite);
    QVERIFY(empty.argument.isEmpty());
    QVERIFY(!empty.isLiveMessage());

    const IrcCommand escaped = IrcCommand::parse(QStringLiteral("//invite bob"));
    QCOMPARE(escaped.verb, IrcCommand::Verb::Say);
    QCOMPARE(escaped.argument, QStringLiteral("/invite bob"));
    QVERIFY(escaped.isLiveMessage());
}

void CommandTest::parseWrappersAndHelp()
{
    const IrcCommand op = IrcCommand::parse(QStringLiteral("/op alice"));
    QCOMPARE(op.verb, IrcCommand::Verb::Op);
    QCOMPARE(op.argument, QStringLiteral("alice"));
    QCOMPARE(op.name, QStringLiteral("/op"));
    QVERIFY(!op.isLiveMessage());
    QVERIFY(op.allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(!op.allowedOn(IrcComposerSurface::Status));

    const IrcCommand deop = IrcCommand::parse(QStringLiteral("/DEOP alice"));
    QCOMPARE(deop.verb, IrcCommand::Verb::Deop);
    QCOMPARE(deop.argument, QStringLiteral("alice"));
    QCOMPARE(deop.name, QStringLiteral("/DEOP"));

    const IrcCommand voice = IrcCommand::parse(QStringLiteral("/voice bob"));
    QCOMPARE(voice.verb, IrcCommand::Verb::Voice);
    QCOMPARE(voice.argument, QStringLiteral("bob"));

    const IrcCommand devoice = IrcCommand::parse(QStringLiteral("/devoice bob"));
    QCOMPARE(devoice.verb, IrcCommand::Verb::Devoice);
    QCOMPARE(devoice.argument, QStringLiteral("bob"));

    const IrcCommand ban = IrcCommand::parse(QStringLiteral("/ban eve"));
    QCOMPARE(ban.verb, IrcCommand::Verb::Ban);
    QCOMPARE(ban.argument, QStringLiteral("eve"));
    QVERIFY(ban.allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(!ban.allowedOn(IrcComposerSurface::Status));

    const IrcCommand ns = IrcCommand::parse(QStringLiteral("/ns identify hunter2"));
    QCOMPARE(ns.verb, IrcCommand::Verb::Ns);
    QCOMPARE(ns.argument, QStringLiteral("identify hunter2"));
    QVERIFY(ns.allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(ns.allowedOn(IrcComposerSurface::Status));

    const IrcCommand cs = IrcCommand::parse(QStringLiteral("/CS help"));
    QCOMPARE(cs.verb, IrcCommand::Verb::Cs);
    QCOMPARE(cs.argument, QStringLiteral("help"));
    QCOMPARE(cs.name, QStringLiteral("/CS"));

    const IrcCommand znc = IrcCommand::parse(QStringLiteral("/znc ListMods"));
    QCOMPARE(znc.verb, IrcCommand::Verb::Znc);
    QCOMPARE(znc.argument, QStringLiteral("ListMods"));
    QVERIFY(znc.allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(znc.allowedOn(IrcComposerSurface::Status));

    const IrcCommand raw = IrcCommand::parse(QStringLiteral("/raw PING :x"));
    QCOMPARE(raw.verb, IrcCommand::Verb::Raw);
    QCOMPARE(raw.argument, QStringLiteral("PING :x"));
    QVERIFY(raw.allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(raw.allowedOn(IrcComposerSurface::Status));

    const IrcCommand quote = IrcCommand::parse(QStringLiteral("/quote PING :x"));
    QCOMPARE(quote.verb, IrcCommand::Verb::Raw);
    QCOMPARE(quote.argument, QStringLiteral("PING :x"));
    QCOMPARE(quote.name, QStringLiteral("/quote"));

    const IrcCommand help = IrcCommand::parse(QStringLiteral("/help"));
    QCOMPARE(help.verb, IrcCommand::Verb::Help);
    QVERIFY(help.argument.isEmpty());
    QVERIFY(help.allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(help.allowedOn(IrcComposerSurface::Status));

    const IrcCommand list = IrcCommand::parse(QStringLiteral("/list"));
    QCOMPARE(list.verb, IrcCommand::Verb::List);
    QVERIFY(list.argument.isEmpty());
    QVERIFY(list.allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(list.allowedOn(IrcComposerSurface::Status));
    const IrcCommand listMask = IrcCommand::parse(QStringLiteral("/LIST #foo*"));
    QCOMPARE(listMask.verb, IrcCommand::Verb::List);
    QCOMPARE(listMask.argument, QStringLiteral("#foo*"));
    const IrcCommand escapedList = IrcCommand::parse(QStringLiteral("//list"));
    QCOMPARE(escapedList.verb, IrcCommand::Verb::Say);
    QCOMPARE(escapedList.argument, QStringLiteral("/list"));

    const IrcCommand escapedOp = IrcCommand::parse(QStringLiteral("//op alice"));
    QCOMPARE(escapedOp.verb, IrcCommand::Verb::Say);
    QCOMPARE(escapedOp.argument, QStringLiteral("/op alice"));
}

void CommandTest::parseChannelModeRequest()
{
    const ParsedChannelMode query = parsedChannelMode(QStringLiteral("#omarchy"));
    QVERIFY(query.ok);
    QVERIFY(query.query);
    QCOMPARE(query.channel, QStringLiteral("#omarchy"));

    const ParsedChannelMode plusO = parsedChannelMode(QStringLiteral("#omarchy +o lena"));
    QVERIFY(plusO.ok);
    QVERIFY(!plusO.query);
    QCOMPARE(plusO.channel, QStringLiteral("#omarchy"));
    QCOMPARE(plusO.modes, QStringLiteral("+o"));
    QCOMPARE(plusO.parameters, QStringList{QStringLiteral("lena")});

    const ParsedChannelMode plusOoo =
        parsedChannelMode(QStringLiteral("#omarchy +ooo n1 n2 n3"));
    QVERIFY(plusOoo.ok);
    QVERIFY(!plusOoo.query);
    QCOMPARE(plusOoo.channel, QStringLiteral("#omarchy"));
    QCOMPARE(plusOoo.modes, QStringLiteral("+ooo"));
    QCOMPARE(plusOoo.parameters,
             (QStringList{QStringLiteral("n1"), QStringLiteral("n2"), QStringLiteral("n3")}));

    const ParsedChannelMode minusO = parsedChannelMode(QStringLiteral("#omarchy -o lena"));
    QVERIFY(minusO.ok);
    QVERIFY(!minusO.query);
    QCOMPARE(minusO.channel, QStringLiteral("#omarchy"));
    QCOMPARE(minusO.modes, QStringLiteral("-o"));
    QCOMPARE(minusO.parameters, QStringList{QStringLiteral("lena")});

    const ParsedChannelMode plusB =
        parsedChannelMode(QStringLiteral("#omarchy +b *!*@*.example"));
    QVERIFY(plusB.ok);
    QVERIFY(!plusB.query);
    QCOMPARE(plusB.channel, QStringLiteral("#omarchy"));
    QCOMPARE(plusB.modes, QStringLiteral("+b"));
    QCOMPARE(plusB.parameters, QStringList{QStringLiteral("*!*@*.example")});

    const ParsedChannelMode plusNt = parsedChannelMode(QStringLiteral("#omarchy +nt"));
    QVERIFY(plusNt.ok);
    QVERIFY(!plusNt.query);
    QCOMPARE(plusNt.channel, QStringLiteral("#omarchy"));
    QCOMPARE(plusNt.modes, QStringLiteral("+nt"));
    QVERIFY(plusNt.parameters.isEmpty());

    const ParsedChannelMode mixed =
        parsedChannelMode(QStringLiteral("#omarchy +o +v a b"));
    QVERIFY(mixed.ok);
    QVERIFY(!mixed.query);
    QCOMPARE(mixed.modes, QStringLiteral("+o"));
    QCOMPARE(mixed.parameters,
             (QStringList{QStringLiteral("+v"), QStringLiteral("a"), QStringLiteral("b")}));

    QVERIFY(!parsedChannelMode(QString()).ok);
    QVERIFY(!parsedChannelMode(QStringLiteral("lena +i")).ok);
    QVERIFY(!parsedChannelMode(QStringLiteral("+o lena")).ok);
    QVERIFY(!parsedChannelMode(QStringLiteral("#omarchy +\r o")).ok);
}

void CommandTest::parseJoinTargets()
{
    const auto keyed = ircParseJoinTargets(QStringLiteral("#a pword, #b, c"));
    QVERIFY(keyed);
    QCOMPARE(keyed->size(), 3);
    QCOMPARE(keyed->at(0).channel(), QStringLiteral("#a"));
    QVERIFY(keyed->at(0).hasKey());
    QCOMPARE(*keyed->at(0).key(), QStringLiteral("pword"));
    QCOMPARE(keyed->at(1).channel(), QStringLiteral("#b"));
    QVERIFY(!keyed->at(1).hasKey());
    QCOMPARE(keyed->at(2).channel(), QStringLiteral("#c"));
    QVERIFY(!keyed->at(2).hasKey());

    const auto lastKeyed = ircParseJoinTargets(QStringLiteral("#a, #b, #c pworddd"));
    QVERIFY(lastKeyed);
    QCOMPARE(lastKeyed->size(), 3);
    QVERIFY(!lastKeyed->at(0).hasKey());
    QVERIFY(!lastKeyed->at(1).hasKey());
    QCOMPARE(lastKeyed->at(2).channel(), QStringLiteral("#c"));
    QCOMPARE(*lastKeyed->at(2).key(), QStringLiteral("pworddd"));

    const auto packed = ircParseJoinTargets(QStringLiteral("#a,b,#c"));
    QVERIFY(packed);
    QCOMPARE(packed->size(), 3);
    QCOMPARE(packed->at(0).channel(), QStringLiteral("#a"));
    QCOMPARE(packed->at(1).channel(), QStringLiteral("#b"));
    QCOMPARE(packed->at(2).channel(), QStringLiteral("#c"));
    QVERIFY(!packed->at(0).hasKey());
    QVERIFY(!packed->at(1).hasKey());
    QVERIFY(!packed->at(2).hasKey());

    const auto local = ircParseJoinTargets(QStringLiteral("&local secret"));
    QVERIFY(local);
    QCOMPARE(local->size(), 1);
    QCOMPARE(local->at(0).channel(), QStringLiteral("&local"));
    QCOMPARE(*local->at(0).key(), QStringLiteral("secret"));

    const auto trailingComma = ircParseJoinTargets(QStringLiteral("#a,#b,"));
    QVERIFY(trailingComma);
    QCOMPARE(trailingComma->size(), 2);
    QCOMPARE(trailingComma->at(0).channel(), QStringLiteral("#a"));
    QCOMPARE(trailingComma->at(1).channel(), QStringLiteral("#b"));

    QVERIFY(!ircParseJoinTargets(QStringLiteral("#a b c")));
    QVERIFY(!ircParseJoinTargets(QString()));
    QVERIFY(!ircParseJoinTargets(QStringLiteral(" , ")));
    QVERIFY(!ircParseJoinTargets(QStringLiteral("#")));

    const auto plus = ircParseJoinTargets(QStringLiteral("+foo"));
    QVERIFY(plus);
    QCOMPARE(plus->size(), 1);
    QCOMPARE(plus->at(0).channel(), QStringLiteral("#+foo"));

    IrcServerFeatures dollar;
    dollar.applyToken("CHANTYPES=$");
    const auto custom = ircParseJoinTargets(QStringLiteral("$serv"), dollar);
    QVERIFY(custom);
    QCOMPARE(custom->size(), 1);
    QCOMPARE(custom->at(0).channel(), QStringLiteral("$serv"));
}

void CommandTest::catalogLookupAndScope()
{
    const IrcVerbSpec *join = IrcVerbTable::lookup(QStringLiteral("J"));
    QVERIFY(join);
    QCOMPARE(join->verb, IrcCommand::Verb::Join);
    QCOMPARE(join->usage, QStringLiteral("/join [channel] [key][, ...]"));

    const IrcVerbSpec *leave = IrcVerbTable::lookup(QStringLiteral("leave"));
    QVERIFY(leave);
    QCOMPARE(leave->verb, IrcCommand::Verb::Part);
    QCOMPARE(leave->usage, QStringLiteral("/part [channel]"));
    QCOMPARE(leave->scope, IrcVerbScope::Either);
    QCOMPARE(leave->wrongScopeText, QStringLiteral("Part applies to channels"));

    QVERIFY(!IrcVerbTable::find(IrcCommand::Verb::Say));
    QVERIFY(!IrcVerbTable::find(IrcCommand::Verb::Empty));
    QVERIFY(!IrcVerbTable::find(IrcCommand::Verb::Unknown));

    QCOMPARE(IrcVerbTable::all().size(), 47);
    for (const IrcVerbSpec& row : IrcVerbTable::all())
        QVERIFY(row.name != QLatin1String("say"));

    const IrcVerbSpec *disconnect = IrcVerbTable::lookup(QStringLiteral("disconnect"));
    QVERIFY(disconnect);
    QCOMPARE(disconnect->verb, IrcCommand::Verb::Quit);
    QCOMPARE(disconnect->name, QStringLiteral("disconnect"));
    QCOMPARE(disconnect->usage, QStringLiteral("/disconnect [reason]"));
    QVERIFY(disconnect->aliases.contains(QStringLiteral("quit")));
    QVERIFY(!disconnect->aliases.contains(QStringLiteral("q")));

    const IrcVerbSpec *quit = IrcVerbTable::lookup(QStringLiteral("quit"));
    QVERIFY(quit);
    QCOMPARE(quit, disconnect);
    QCOMPARE(quit->verb, IrcCommand::Verb::Quit);
    QCOMPARE(quit->name, QStringLiteral("disconnect"));
    QCOMPARE(quit->usage, QStringLiteral("/disconnect [reason]"));
    QVERIFY(!IrcVerbTable::lookup(QStringLiteral("q")));

    const IrcVerbSpec *query = IrcVerbTable::lookup(QStringLiteral("query"));
    QVERIFY(query);
    QCOMPARE(query->verb, IrcCommand::Verb::Query);
    QCOMPARE(query->name, QStringLiteral("query"));
    QCOMPARE(query->usage, QStringLiteral("/query <nick> [text]"));
    QCOMPARE(query->scope, IrcVerbScope::Either);
    QVERIFY(query->wrongScopeText.isEmpty());
    QVERIFY(!query->aliases.contains(QStringLiteral("msg")));

    const IrcVerbSpec *msg = IrcVerbTable::lookup(QStringLiteral("msg"));
    QVERIFY(msg);
    QCOMPARE(msg->verb, IrcCommand::Verb::Msg);
    QCOMPARE(msg->name, QStringLiteral("msg"));
    QCOMPARE(msg->usage, QStringLiteral("/msg <nick> <text>"));
    QCOMPARE(msg->scope, IrcVerbScope::Either);
    QVERIFY(msg->wrongScopeText.isEmpty());
    QVERIFY(msg->aliases.isEmpty());

    const IrcVerbSpec *close = IrcVerbTable::lookup(QStringLiteral("close"));
    QVERIFY(close);
    QCOMPARE(close->verb, IrcCommand::Verb::Close);
    QCOMPARE(close->usage, QStringLiteral("/close"));
    QVERIFY(close->aliases.isEmpty());
    QCOMPARE(close->scope, IrcVerbScope::Conversation);
    QCOMPARE(close->wrongScopeText, QStringLiteral("Close applies to direct messages"));

    const IrcVerbSpec *topic = IrcVerbTable::lookup(QStringLiteral("topic"));
    QVERIFY(topic);
    QCOMPARE(topic->verb, IrcCommand::Verb::Topic);
    QCOMPARE(topic->usage, QStringLiteral("/topic [text]"));
    QVERIFY(topic->aliases.isEmpty());
    QCOMPARE(topic->scope, IrcVerbScope::Conversation);
    QCOMPARE(topic->wrongScopeText, QStringLiteral("Topic applies to channels"));

    const IrcVerbSpec *notice = IrcVerbTable::lookup(QStringLiteral("notice"));
    QVERIFY(notice);
    QCOMPARE(notice->verb, IrcCommand::Verb::Notice);
    QCOMPARE(notice->name, QStringLiteral("notice"));
    QCOMPARE(notice->usage, QStringLiteral("/notice <target> <text>"));
    QCOMPARE(notice->scope, IrcVerbScope::Either);
    QVERIFY(notice->wrongScopeText.isEmpty());
    QVERIFY(notice->aliases.isEmpty());

    const IrcVerbSpec *away = IrcVerbTable::lookup(QStringLiteral("away"));
    QVERIFY(away);
    QCOMPARE(away->verb, IrcCommand::Verb::Away);
    QCOMPARE(away->name, QStringLiteral("away"));
    QCOMPARE(away->usage, QStringLiteral("/away [reason]"));
    QCOMPARE(away->scope, IrcVerbScope::Either);
    QVERIFY(away->wrongScopeText.isEmpty());
    QVERIFY(away->aliases.isEmpty());

    const IrcVerbSpec *back = IrcVerbTable::lookup(QStringLiteral("back"));
    QVERIFY(back);
    QCOMPARE(back->verb, IrcCommand::Verb::Back);
    QCOMPARE(back->name, QStringLiteral("back"));
    QCOMPARE(back->usage, QStringLiteral("/back"));
    QCOMPARE(back->scope, IrcVerbScope::Either);
    QVERIFY(back->wrongScopeText.isEmpty());
    QVERIFY(back->aliases.isEmpty());

    const IrcVerbSpec *autoaway = IrcVerbTable::lookup(QStringLiteral("autoaway"));
    QVERIFY(autoaway);
    QCOMPARE(autoaway->verb, IrcCommand::Verb::Autoaway);
    QCOMPARE(autoaway->name, QStringLiteral("autoaway"));
    QCOMPARE(autoaway->usage,
             QStringLiteral("/autoaway [off|on|duration [reason]|reason [text]]"));
    QCOMPARE(autoaway->scope, IrcVerbScope::Either);
    QVERIFY(autoaway->wrongScopeText.isEmpty());
    QVERIFY(autoaway->aliases.isEmpty());

    const IrcVerbSpec *pref = IrcVerbTable::lookup(QStringLiteral("pref"));
    QVERIFY(pref);
    QCOMPARE(pref->verb, IrcCommand::Verb::Pref);
    QCOMPARE(pref->name, QStringLiteral("pref"));
    QCOMPARE(pref->usage, QStringLiteral("/pref [directs|avatars|unread] [on|off]"));
    QCOMPARE(pref->scope, IrcVerbScope::Either);
    QVERIFY(pref->wrongScopeText.isEmpty());
    QVERIFY(pref->aliases.isEmpty());

    const IrcVerbSpec *status = IrcVerbTable::lookup(QStringLiteral("status"));
    QVERIFY(status);
    QCOMPARE(status->verb, IrcCommand::Verb::Status);
    QCOMPARE(status->name, QStringLiteral("status"));
    QCOMPARE(status->usage, QStringLiteral("/status [text]"));
    QCOMPARE(status->scope, IrcVerbScope::Either);
    QVERIFY(status->wrongScopeText.isEmpty());
    QVERIFY(status->aliases.isEmpty());

    const IrcVerbSpec *avatar = IrcVerbTable::lookup(QStringLiteral("avatar"));
    QVERIFY(avatar);
    QCOMPARE(avatar->verb, IrcCommand::Verb::Avatar);
    QCOMPARE(avatar->name, QStringLiteral("avatar"));
    QCOMPARE(avatar->usage, QStringLiteral("/avatar [url|email]"));
    QCOMPARE(avatar->scope, IrcVerbScope::Either);
    QVERIFY(avatar->wrongScopeText.isEmpty());
    QVERIFY(avatar->aliases.isEmpty());

    const IrcVerbSpec *whois = IrcVerbTable::lookup(QStringLiteral("whois"));
    QVERIFY(whois);
    QCOMPARE(whois->verb, IrcCommand::Verb::Whois);
    QCOMPARE(whois->name, QStringLiteral("whois"));
    QCOMPARE(whois->usage, QStringLiteral("/whois [nick]"));
    QCOMPARE(whois->scope, IrcVerbScope::Either);
    QCOMPARE(whois->wrongScopeText, QStringLiteral("Name a nick"));
    QVERIFY(whois->aliases.isEmpty());

    const IrcVerbSpec *ping = IrcVerbTable::lookup(QStringLiteral("ping"));
    QVERIFY(ping);
    QCOMPARE(ping->verb, IrcCommand::Verb::Ping);
    QCOMPARE(ping->name, QStringLiteral("ping"));
    QCOMPARE(ping->usage, QStringLiteral("/ping [nick]"));
    QCOMPARE(ping->scope, IrcVerbScope::Either);
    QCOMPARE(ping->wrongScopeText, QStringLiteral("Name a nick"));
    QVERIFY(ping->aliases.isEmpty());

    const IrcVerbSpec *time = IrcVerbTable::lookup(QStringLiteral("time"));
    QVERIFY(time);
    QCOMPARE(time->verb, IrcCommand::Verb::Time);
    QCOMPARE(time->usage, QStringLiteral("/time [nick]"));
    QCOMPARE(time->scope, IrcVerbScope::Either);
    QCOMPARE(time->wrongScopeText, QStringLiteral("Name a nick"));

    const IrcVerbSpec *version = IrcVerbTable::lookup(QStringLiteral("version"));
    QVERIFY(version);
    QCOMPARE(version->verb, IrcCommand::Verb::Version);
    QCOMPARE(version->usage, QStringLiteral("/version [nick]"));
    QCOMPARE(version->scope, IrcVerbScope::Either);
    QCOMPARE(version->wrongScopeText, QStringLiteral("Name a nick"));

    const IrcVerbSpec *mode = IrcVerbTable::lookup(QStringLiteral("mode"));
    QVERIFY(mode);
    QCOMPARE(mode->verb, IrcCommand::Verb::Mode);
    QCOMPARE(mode->name, QStringLiteral("mode"));
    QCOMPARE(mode->usage, QStringLiteral("/mode <channel> [[+|-]modechars [parameters]]"));
    QCOMPARE(mode->scope, IrcVerbScope::Either);
    QVERIFY(mode->wrongScopeText.isEmpty());
    QVERIFY(mode->aliases.isEmpty());

    const IrcVerbSpec *kick = IrcVerbTable::lookup(QStringLiteral("kick"));
    QVERIFY(kick);
    QCOMPARE(kick->verb, IrcCommand::Verb::Kick);
    QCOMPARE(kick->name, QStringLiteral("kick"));
    QCOMPARE(kick->usage, QStringLiteral("/kick [channel] <nick> [reason]"));
    QCOMPARE(kick->scope, IrcVerbScope::Either);
    QCOMPARE(kick->wrongScopeText, QStringLiteral("Kick applies to channels"));
    QVERIFY(kick->aliases.isEmpty());

    const IrcVerbSpec *invite = IrcVerbTable::lookup(QStringLiteral("invite"));
    QVERIFY(invite);
    QCOMPARE(invite->verb, IrcCommand::Verb::Invite);
    QCOMPARE(invite->name, QStringLiteral("invite"));
    QCOMPARE(invite->usage, QStringLiteral("/invite <nick> [channel]"));
    QCOMPARE(invite->scope, IrcVerbScope::Either);
    QCOMPARE(invite->wrongScopeText, QStringLiteral("Invite applies to channels"));
    QVERIFY(invite->aliases.isEmpty());

    const IrcVerbSpec *ignore = IrcVerbTable::lookup(QStringLiteral("ignore"));
    QVERIFY(ignore);
    QCOMPARE(ignore->verb, IrcCommand::Verb::Ignore);
    QCOMPARE(ignore->name, QStringLiteral("ignore"));
    QCOMPARE(ignore->usage, QStringLiteral("/ignore <nick>"));
    QCOMPARE(ignore->scope, IrcVerbScope::Either);
    QVERIFY(ignore->wrongScopeText.isEmpty());
    QVERIFY(ignore->aliases.isEmpty());

    const IrcVerbSpec *unignore = IrcVerbTable::lookup(QStringLiteral("unignore"));
    QVERIFY(unignore);
    QCOMPARE(unignore->verb, IrcCommand::Verb::Unignore);
    QCOMPARE(unignore->usage, QStringLiteral("/unignore <nick>"));
    QCOMPARE(unignore->scope, IrcVerbScope::Either);
    QVERIFY(unignore->wrongScopeText.isEmpty());

    const IrcVerbSpec *ignored = IrcVerbTable::lookup(QStringLiteral("ignored"));
    QVERIFY(ignored);
    QCOMPARE(ignored->verb, IrcCommand::Verb::Ignored);
    QCOMPARE(ignored->usage, QStringLiteral("/ignored"));
    QCOMPARE(ignored->scope, IrcVerbScope::Either);
    QVERIFY(ignored->wrongScopeText.isEmpty());

    const IrcVerbSpec *monitor = IrcVerbTable::lookup(QStringLiteral("monitor"));
    QVERIFY(monitor);
    QCOMPARE(monitor->verb, IrcCommand::Verb::Monitor);
    QCOMPARE(monitor->usage, QStringLiteral("/monitor <nick>"));
    QCOMPARE(monitor->scope, IrcVerbScope::Either);
    QVERIFY(monitor->wrongScopeText.isEmpty());

    const IrcVerbSpec *unmonitor = IrcVerbTable::lookup(QStringLiteral("unmonitor"));
    QVERIFY(unmonitor);
    QCOMPARE(unmonitor->verb, IrcCommand::Verb::Unmonitor);
    QCOMPARE(unmonitor->usage, QStringLiteral("/unmonitor <nick>"));
    QCOMPARE(unmonitor->scope, IrcVerbScope::Either);
    QVERIFY(unmonitor->wrongScopeText.isEmpty());

    const IrcVerbSpec *monitored = IrcVerbTable::lookup(QStringLiteral("monitored"));
    QVERIFY(monitored);
    QCOMPARE(monitored->verb, IrcCommand::Verb::Monitored);
    QCOMPARE(monitored->usage, QStringLiteral("/monitored"));
    QCOMPARE(monitored->scope, IrcVerbScope::Either);
    QVERIFY(monitored->wrongScopeText.isEmpty());

    const IrcVerbSpec *mute = IrcVerbTable::lookup(QStringLiteral("mute"));
    QVERIFY(mute);
    QCOMPARE(mute->verb, IrcCommand::Verb::Mute);
    QCOMPARE(mute->name, QStringLiteral("mute"));
    QCOMPARE(mute->usage, QStringLiteral("/mute [target]"));
    QCOMPARE(mute->scope, IrcVerbScope::Either);
    QCOMPARE(mute->wrongScopeText, QStringLiteral("Mute applies to conversations"));

    const IrcVerbSpec *unmute = IrcVerbTable::lookup(QStringLiteral("unmute"));
    QVERIFY(unmute);
    QCOMPARE(unmute->verb, IrcCommand::Verb::Unmute);
    QCOMPARE(unmute->usage, QStringLiteral("/unmute [target]"));
    QCOMPARE(unmute->scope, IrcVerbScope::Either);
    QCOMPARE(unmute->wrongScopeText, QStringLiteral("Mute applies to conversations"));

    const IrcVerbSpec *muted = IrcVerbTable::lookup(QStringLiteral("muted"));
    QVERIFY(muted);
    QCOMPARE(muted->verb, IrcCommand::Verb::Muted);
    QCOMPARE(muted->usage, QStringLiteral("/muted"));
    QCOMPARE(muted->scope, IrcVerbScope::Either);
    QVERIFY(muted->wrongScopeText.isEmpty());

    const IrcVerbSpec *highlight = IrcVerbTable::lookup(QStringLiteral("highlight"));
    QVERIFY(highlight);
    QCOMPARE(highlight->verb, IrcCommand::Verb::Highlight);
    QCOMPARE(highlight->name, QStringLiteral("highlight"));
    QCOMPARE(highlight->usage, QStringLiteral("/highlight <word>"));
    QCOMPARE(highlight->scope, IrcVerbScope::Either);
    QVERIFY(highlight->wrongScopeText.isEmpty());
    QVERIFY(highlight->aliases.isEmpty());

    const IrcVerbSpec *unhighlight = IrcVerbTable::lookup(QStringLiteral("unhighlight"));
    QVERIFY(unhighlight);
    QCOMPARE(unhighlight->verb, IrcCommand::Verb::Unhighlight);
    QCOMPARE(unhighlight->usage, QStringLiteral("/unhighlight <word>"));
    QCOMPARE(unhighlight->scope, IrcVerbScope::Either);
    QVERIFY(unhighlight->wrongScopeText.isEmpty());

    const IrcVerbSpec *highlights = IrcVerbTable::lookup(QStringLiteral("highlights"));
    QVERIFY(highlights);
    QCOMPARE(highlights->verb, IrcCommand::Verb::Highlights);
    QCOMPARE(highlights->usage, QStringLiteral("/highlights"));
    QCOMPARE(highlights->scope, IrcVerbScope::Either);
    QVERIFY(highlights->wrongScopeText.isEmpty());

    const IrcVerbSpec *op = IrcVerbTable::lookup(QStringLiteral("op"));
    QVERIFY(op);
    QCOMPARE(op->verb, IrcCommand::Verb::Op);
    QCOMPARE(op->usage, QStringLiteral("/op <nick>"));
    QCOMPARE(op->scope, IrcVerbScope::Conversation);
    QCOMPARE(op->wrongScopeText, QStringLiteral("Op applies to channels"));

    const IrcVerbSpec *ban = IrcVerbTable::lookup(QStringLiteral("ban"));
    QVERIFY(ban);
    QCOMPARE(ban->verb, IrcCommand::Verb::Ban);
    QCOMPARE(ban->usage, QStringLiteral("/ban <mask>"));
    QCOMPARE(ban->scope, IrcVerbScope::Conversation);
    QCOMPARE(ban->wrongScopeText, QStringLiteral("Ban applies to channels"));

    const IrcVerbSpec *ns = IrcVerbTable::lookup(QStringLiteral("ns"));
    QVERIFY(ns);
    QCOMPARE(ns->verb, IrcCommand::Verb::Ns);
    QCOMPARE(ns->usage, QStringLiteral("/ns <text>"));
    QCOMPARE(ns->scope, IrcVerbScope::Either);

    const IrcVerbSpec *cs = IrcVerbTable::lookup(QStringLiteral("cs"));
    QVERIFY(cs);
    QCOMPARE(cs->verb, IrcCommand::Verb::Cs);
    QCOMPARE(cs->scope, IrcVerbScope::Either);

    const IrcVerbSpec *znc = IrcVerbTable::lookup(QStringLiteral("znc"));
    QVERIFY(znc);
    QCOMPARE(znc->verb, IrcCommand::Verb::Znc);
    QCOMPARE(znc->usage, QStringLiteral("/znc <text>"));
    QCOMPARE(znc->scope, IrcVerbScope::Either);

    const IrcVerbSpec *raw = IrcVerbTable::lookup(QStringLiteral("quote"));
    QVERIFY(raw);
    QCOMPARE(raw->verb, IrcCommand::Verb::Raw);
    QCOMPARE(raw->name, QStringLiteral("raw"));
    QCOMPARE(raw->usage, QStringLiteral("/raw <line>"));
    QCOMPARE(raw->scope, IrcVerbScope::Either);
    QVERIFY(raw->aliases.contains(QStringLiteral("quote")));

    const IrcVerbSpec *help = IrcVerbTable::lookup(QStringLiteral("help"));
    QVERIFY(help);
    QCOMPARE(help->verb, IrcCommand::Verb::Help);
    QCOMPARE(help->usage, QStringLiteral("/help"));
    QCOMPARE(help->scope, IrcVerbScope::Either);

    const IrcVerbSpec *list = IrcVerbTable::lookup(QStringLiteral("list"));
    QVERIFY(list);
    QCOMPARE(list->verb, IrcCommand::Verb::List);
    QCOMPARE(list->usage, QStringLiteral("/list [mask]"));
    QCOMPARE(list->scope, IrcVerbScope::Either);

    const QVector<IrcVerbSpec> statusRows = IrcVerbTable::visibleOn(IrcComposerSurface::Status);
    QCOMPARE(statusRows.size(), 39);
    for (const IrcVerbSpec& row : statusRows) {
        QVERIFY(row.allowedOn(IrcComposerSurface::Status));
        QVERIFY(row.verb != IrcCommand::Verb::Action);
        QVERIFY(row.verb != IrcCommand::Verb::Close);
        QVERIFY(row.verb != IrcCommand::Verb::Topic);
        QVERIFY(row.verb != IrcCommand::Verb::Op);
        QVERIFY(row.verb != IrcCommand::Verb::Deop);
        QVERIFY(row.verb != IrcCommand::Verb::Voice);
        QVERIFY(row.verb != IrcCommand::Verb::Devoice);
        QVERIFY(row.verb != IrcCommand::Verb::Ban);
    }

    const QVector<IrcVerbSpec> conversation =
        IrcVerbTable::visibleOn(IrcComposerSurface::Conversation);
    QCOMPARE(conversation.size(), 47);
    bool sawMe = false;
    bool sawClose = false;
    bool sawQuery = false;
    bool sawMsg = false;
    bool sawTopic = false;
    bool sawNotice = false;
    bool sawAway = false;
    bool sawBack = false;
    bool sawAutoaway = false;
    bool sawPref = false;
    bool sawStatus = false;
    bool sawAvatar = false;
    bool sawWhois = false;
    bool sawPing = false;
    bool sawTime = false;
    bool sawVersion = false;
    bool sawMode = false;
    bool sawKick = false;
    bool sawInvite = false;
    bool sawIgnore = false;
    bool sawUnignore = false;
    bool sawIgnored = false;
    bool sawMonitor = false;
    bool sawUnmonitor = false;
    bool sawMonitored = false;
    bool sawMute = false;
    bool sawUnmute = false;
    bool sawMuted = false;
    bool sawHighlight = false;
    bool sawUnhighlight = false;
    bool sawHighlights = false;
    bool sawOp = false;
    bool sawNs = false;
    bool sawRaw = false;
    bool sawHelp = false;
    bool sawList = false;
    for (const IrcVerbSpec& row : conversation) {
        if (row.name == QLatin1String("me"))
            sawMe = true;
        if (row.name == QLatin1String("close"))
            sawClose = true;
        if (row.name == QLatin1String("query"))
            sawQuery = true;
        if (row.name == QLatin1String("msg"))
            sawMsg = true;
        if (row.name == QLatin1String("topic"))
            sawTopic = true;
        if (row.name == QLatin1String("notice"))
            sawNotice = true;
        if (row.name == QLatin1String("away"))
            sawAway = true;
        if (row.name == QLatin1String("back"))
            sawBack = true;
        if (row.name == QLatin1String("autoaway"))
            sawAutoaway = true;
        if (row.name == QLatin1String("pref"))
            sawPref = true;
        if (row.name == QLatin1String("status"))
            sawStatus = true;
        if (row.name == QLatin1String("avatar"))
            sawAvatar = true;
        if (row.name == QLatin1String("whois"))
            sawWhois = true;
        if (row.name == QLatin1String("ping"))
            sawPing = true;
        if (row.name == QLatin1String("time"))
            sawTime = true;
        if (row.name == QLatin1String("version"))
            sawVersion = true;
        if (row.name == QLatin1String("mode"))
            sawMode = true;
        if (row.name == QLatin1String("kick"))
            sawKick = true;
        if (row.name == QLatin1String("invite"))
            sawInvite = true;
        if (row.name == QLatin1String("ignore"))
            sawIgnore = true;
        if (row.name == QLatin1String("unignore"))
            sawUnignore = true;
        if (row.name == QLatin1String("ignored"))
            sawIgnored = true;
        if (row.name == QLatin1String("monitor"))
            sawMonitor = true;
        if (row.name == QLatin1String("unmonitor"))
            sawUnmonitor = true;
        if (row.name == QLatin1String("monitored"))
            sawMonitored = true;
        if (row.name == QLatin1String("mute"))
            sawMute = true;
        if (row.name == QLatin1String("unmute"))
            sawUnmute = true;
        if (row.name == QLatin1String("muted"))
            sawMuted = true;
        if (row.name == QLatin1String("highlight"))
            sawHighlight = true;
        if (row.name == QLatin1String("unhighlight"))
            sawUnhighlight = true;
        if (row.name == QLatin1String("highlights"))
            sawHighlights = true;
        if (row.name == QLatin1String("op"))
            sawOp = true;
        if (row.name == QLatin1String("ns"))
            sawNs = true;
        if (row.name == QLatin1String("raw"))
            sawRaw = true;
        if (row.name == QLatin1String("help"))
            sawHelp = true;
        if (row.name == QLatin1String("list"))
            sawList = true;
    }
    QVERIFY(sawMe);
    QVERIFY(sawClose);
    QVERIFY(sawQuery);
    QVERIFY(sawMsg);
    QVERIFY(sawTopic);
    QVERIFY(sawNotice);
    QVERIFY(sawAway);
    QVERIFY(sawBack);
    QVERIFY(sawAutoaway);
    QVERIFY(sawPref);
    QVERIFY(sawStatus);
    QVERIFY(sawAvatar);
    QVERIFY(sawWhois);
    QVERIFY(sawPing);
    QVERIFY(sawTime);
    QVERIFY(sawVersion);
    QVERIFY(sawMode);
    QVERIFY(sawKick);
    QVERIFY(sawInvite);
    QVERIFY(sawIgnore);
    QVERIFY(sawUnignore);
    QVERIFY(sawIgnored);
    QVERIFY(sawMonitor);
    QVERIFY(sawUnmonitor);
    QVERIFY(sawMonitored);
    QVERIFY(sawMute);
    QVERIFY(sawUnmute);
    QVERIFY(sawMuted);
    QVERIFY(sawHighlight);
    QVERIFY(sawUnhighlight);
    QVERIFY(sawHighlights);
    QVERIFY(sawOp);
    QVERIFY(sawNs);
    QVERIFY(sawRaw);
    QVERIFY(sawHelp);
    QVERIFY(sawList);

    const IrcCommand say = IrcCommand::parse(QStringLiteral("hello"));
    QVERIFY(say.allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(!say.allowedOn(IrcComposerSurface::Status));
    const IrcCommand action = IrcCommand::parse(QStringLiteral("/me waves"));
    QVERIFY(action.allowedOn(IrcComposerSurface::Conversation));
    QVERIFY(!action.allowedOn(IrcComposerSurface::Status));
    QVERIFY(IrcCommand::parse(QStringLiteral("/join #x"))
                .allowedOn(IrcComposerSurface::Status));
}

void CommandTest::closeWrongScopeUsesCatalogSentence()
{
    const IrcCommand close = IrcCommand::parse(QStringLiteral("/close"));
    QCOMPARE(ircCommandOutcomeText(IrcCommandOutcome::WrongScope, close),
             QStringLiteral("Close applies to direct messages"));

    const IrcCommand part = IrcCommand::parse(QStringLiteral("/part"));
    QCOMPARE(ircCommandOutcomeText(IrcCommandOutcome::WrongScope, part),
             QStringLiteral("Part applies to channels"));

    const IrcCommand action = IrcCommand::parse(QStringLiteral("/me waves"));
    QCOMPARE(ircCommandOutcomeText(IrcCommandOutcome::WrongScope, action),
             QStringLiteral("Select a connected conversation first"));
    const IrcCommand say = IrcCommand::parse(QStringLiteral("hello"));
    QCOMPARE(ircCommandOutcomeText(IrcCommandOutcome::WrongScope, say),
             QStringLiteral("Select a connected conversation first"));
    const IrcCommand query = IrcCommand::parse(QStringLiteral("/query lena"));
    QCOMPARE(ircCommandOutcomeText(IrcCommandOutcome::WrongScope, query),
             QStringLiteral("Select a connected conversation first"));
    const IrcCommand topic = IrcCommand::parse(QStringLiteral("/topic hello"));
    QCOMPARE(ircCommandOutcomeText(IrcCommandOutcome::WrongScope, topic),
             QStringLiteral("Topic applies to channels"));
    const IrcCommand op = IrcCommand::parse(QStringLiteral("/op alice"));
    QCOMPARE(ircCommandOutcomeText(IrcCommandOutcome::WrongScope, op),
             QStringLiteral("Op applies to channels"));
    const IrcCommand whois = IrcCommand::parse(QStringLiteral("/whois"));
    QCOMPARE(ircCommandOutcomeText(IrcCommandOutcome::WrongScope, whois),
             QStringLiteral("Name a nick"));
    const IrcCommand ping = IrcCommand::parse(QStringLiteral("/ping"));
    QCOMPARE(ircCommandOutcomeText(IrcCommandOutcome::WrongScope, ping),
             QStringLiteral("Name a nick"));
    const IrcCommand version = IrcCommand::parse(QStringLiteral("/version"));
    QCOMPARE(ircCommandOutcomeText(IrcCommandOutcome::WrongScope, version),
             QStringLiteral("Name a nick"));
}

void CommandTest::conversationSendAndUnknown()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    QCOMPARE(session->state(), IrcSession::State::Registered);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/me waves")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArray("PRIVMSG #omarchy :\x01" "ACTION waves\x01\r\n"));

    QVERIFY(!controller.sendMessage(QStringLiteral("/nope")));
    QCOMPARE(controller.lastError(), QStringLiteral("Unknown command: /nope"));

    QVERIFY(controller.sendMessage(QStringLiteral("/join #other")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("JOIN #other\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#other"));

    QVERIFY(controller.sendMessage(QStringLiteral("/join #alpha,#beta desktop")));
    QCOMPARE(transport->writtenFrames().at(transport->writtenFrames().size() - 2),
             QByteArrayLiteral("JOIN #alpha\r\n"));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("JOIN #beta desktop\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#beta"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/topic new banner")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("TOPIC #omarchy :new banner\r\n"));

    const int beforeEmptyTopic = transport->writtenFrames().size();
    QVERIFY(controller.sendMessage(QStringLiteral("/topic")));
    QCOMPARE(transport->writtenFrames().size(), beforeEmptyTopic);
    QVERIFY(!framesContain(transport->writtenFrames().mid(beforeEmptyTopic),
                           QByteArrayLiteral("TOPIC")));

    QVERIFY(controller.sendMessage(QStringLiteral("/topic #foo is the topic")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("TOPIC #omarchy :#foo is the topic\r\n"));

    transport->injectBytes(QByteArrayLiteral(":lena!u@h PRIVMSG omairc :hi\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    const int beforeDmTopic = transport->writtenFrames().size();
    QVERIFY(!controller.sendMessage(QStringLiteral("/topic hello")));
    QCOMPARE(controller.lastError(), QStringLiteral("Topic applies to channels"));
    QCOMPARE(transport->writtenFrames().size(), beforeDmTopic);
    QVERIFY(!framesContain(transport->writtenFrames().mid(beforeDmTopic),
                           QByteArrayLiteral("TOPIC")));

    IrcController lonely;
    QVERIFY(!lonely.sendMessage(QStringLiteral("/me waves")));
    QCOMPARE(lonely.lastError(),
             QStringLiteral("Select a connected conversation first"));
    QVERIFY(!lonely.sendMessage(QStringLiteral("/query lena")));
    QCOMPARE(lonely.lastError(),
             QStringLiteral("Select a connected conversation first"));
    QVERIFY(!lonely.sendMessage(QStringLiteral("/clear")));
    QCOMPARE(lonely.lastError(),
             QStringLiteral("Select a connected conversation first"));
    QVERIFY(!lonely.sendMessage(QStringLiteral("/topic hello")));
    QCOMPARE(lonely.lastError(), QStringLiteral("Topic applies to channels"));
    QVERIFY(!lonely.sendMessage(QString()));
}

void CommandTest::joinSendsKeyedFrames()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    QCOMPARE(session->state(), IrcSession::State::Registered);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/join #a pword, #b, c")));
    QCOMPARE(transport->writtenFrames().mid(transport->writtenFrames().size() - 3),
             QByteArrayList({
                 QByteArrayLiteral("JOIN #a pword\r\n"),
                 QByteArrayLiteral("JOIN #b\r\n"),
                 QByteArrayLiteral("JOIN #c\r\n"),
             }));

    QVERIFY(controller.sendMessage(QStringLiteral("/join #a, #b, #c pworddd")));
    QCOMPARE(transport->writtenFrames().mid(transport->writtenFrames().size() - 3),
             QByteArrayList({
                 QByteArrayLiteral("JOIN #a\r\n"),
                 QByteArrayLiteral("JOIN #b\r\n"),
                 QByteArrayLiteral("JOIN #c pworddd\r\n"),
             }));

    QVERIFY(controller.sendMessage(QStringLiteral("/join #a,b,#c")));
    QCOMPARE(transport->writtenFrames().mid(transport->writtenFrames().size() - 3),
             QByteArrayList({
                 QByteArrayLiteral("JOIN #a\r\n"),
                 QByteArrayLiteral("JOIN #b\r\n"),
                 QByteArrayLiteral("JOIN #c\r\n"),
             }));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#c"));

    IrcStatusConsole *console = controller.console();
    QVERIFY(!logContains(console->lines(), QStringLiteral("JOIN #c pworddd")));
    QVERIFY(!logContains(console->lines(), QStringLiteral("JOIN #a pword")));
    QVERIFY(!logContains(console->lines(), QStringLiteral("pworddd")));
    QVERIFY(!logContains(console->lines(), QStringLiteral("pword")));
    QVERIFY(!logContains(console->lines(), QStringLiteral("JOIN")));

    QVERIFY(controller.sendMessage(QStringLiteral("/join #secretchan hunter2")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("JOIN #secretchan hunter2\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#secretchan"));
    QVERIFY(!logContains(console->lines(), QStringLiteral("hunter2")));
    QCOMPARE(IrcStatusEntry::outgoing(
                 QStringLiteral("libera"),
                 QByteArrayLiteral("JOIN #secretchan hunter2\r\n"))
                 .text(),
             QStringLiteral("JOIN #secretchan ***"));

    QVERIFY(console->submit(QStringLiteral("/join #fromstatus deskkey")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("JOIN #fromstatus deskkey\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#fromstatus"));
    QVERIFY(!controller.console()->isOpen());
    QVERIFY(!logContains(console->lines(), QStringLiteral("deskkey")));
    QCOMPARE(IrcStatusEntry::outgoing(
                 QStringLiteral("libera"),
                 QByteArrayLiteral("JOIN #fromstatus deskkey\r\n"))
                 .text(),
             QStringLiteral("JOIN #fromstatus ***"));

    const int beforeBad = transport->writtenFrames().size();
    QVERIFY(!controller.sendMessage(QStringLiteral("/join #a b c")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QVERIFY(!controller.sendMessage(QStringLiteral("/join")));
    QVERIFY(!controller.sendMessage(QStringLiteral("/join #")));
    QVERIFY(!controller.sendMessage(QStringLiteral("/join , ,")));
    QCOMPARE(transport->writtenFrames().size(), beforeBad);
    QVERIFY(!framesContain(transport->writtenFrames().mid(beforeBad),
                           QByteArrayLiteral("JOIN")));
}

void CommandTest::statusSubmitDoesNotSendAction()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    QCOMPARE(session->state(), IrcSession::State::Registered);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    IrcStatusConsole *console = controller.console();
    QVERIFY(!console->submit(QString()));
    const int beforeAction = transport->writtenFrames().size();
    QVERIFY(console->submit(QStringLiteral("/me waves")));
    for (int i = beforeAction; i < transport->writtenFrames().size(); ++i)
        QVERIFY(!transport->writtenFrames().at(i).contains("ACTION"));
    QVERIFY(logContains(console->lines(),
                        QStringLiteral("Select a connected conversation first")));

    QVERIFY(console->submit(QStringLiteral("/close")));
    QVERIFY(logContains(console->lines(),
                        QStringLiteral("Close applies to direct messages")));

    const int beforeTopic = transport->writtenFrames().size();
    QVERIFY(console->submit(QStringLiteral("/topic hello")));
    QCOMPARE(transport->writtenFrames().size(), beforeTopic);
    QVERIFY(!framesContain(transport->writtenFrames().mid(beforeTopic),
                           QByteArrayLiteral("TOPIC")));
    QVERIFY(logContains(console->lines(),
                        QStringLiteral("Topic applies to channels")));

    QVERIFY(console->submit(QStringLiteral("/nope")));
    QVERIFY(logContains(console->lines(), QStringLiteral("Unknown command: /nope")));

    IrcController empty;
    QVERIFY(empty.console()->submit(QStringLiteral("/clear")));
}

void CommandTest::awayAndBackWriteAwayFrames()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    QCOMPARE(session->state(), IrcSession::State::Registered);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/away lunch")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("AWAY :lunch\r\n"));
    QVERIFY(!controller.selfAway());

    QVERIFY(controller.sendMessage(QStringLiteral("/away")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("AWAY\r\n"));

    QVERIFY(controller.sendMessage(QStringLiteral("/back leftover")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("AWAY\r\n"));

    IrcStatusConsole *console = controller.console();
    QVERIFY(console->submit(QStringLiteral("/away lunch")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("AWAY :lunch\r\n"));
    QVERIFY(console->submit(QStringLiteral("/away")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("AWAY\r\n"));
    QVERIFY(console->submit(QStringLiteral("/back leftover")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("AWAY\r\n"));

    transport->remoteClose();
    QVERIFY(session->state() != IrcSession::State::Registered);
    const int framesBefore = transport->writtenFrames().size();
    QVERIFY(!controller.sendMessage(QStringLiteral("/away lunch")));
    QCOMPARE(controller.lastError(), QStringLiteral("Not connected"));
    QVERIFY(console->submit(QStringLiteral("/back")));
    QVERIFY(logContains(console->lines(), QStringLiteral("Not connected")));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);
    QVERIFY(!framesContain(transport->writtenFrames().mid(framesBefore),
                           QByteArrayLiteral("AWAY")));

    IrcController lonely;
    QVERIFY(!lonely.sendMessage(QStringLiteral("/away lunch")));
    QCOMPARE(lonely.lastError(), QStringLiteral("Not connected"));
    QVERIFY(!lonely.sendMessage(QStringLiteral("/back")));
    QCOMPARE(lonely.lastError(), QStringLiteral("Not connected"));
}

void CommandTest::statusWritesMetadataFrames()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcomeMetadata(transport);
    QCOMPARE(session->state(), IrcSession::State::Registered);
    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :omairc Alice\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(controller.hasMemberStatus());
    QVERIFY(!controller.selfAway());

    auto *members = qobject_cast<QAbstractItemModel *>(controller.members());
    QVERIFY(members);
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    IrcStatusConsole *console = controller.console();
    QVERIFY(console);

    const int beforeEmpty = transport->writtenFrames().size();
    const int statusRowsBeforeEmpty = console->lines()->rowCount();
    QVERIFY(controller.sendMessage(QStringLiteral("/status")));
    QCOMPARE(transport->writtenFrames().size(), beforeEmpty);
    QVERIFY(!framesContain(transport->writtenFrames().mid(beforeEmpty),
                           QByteArrayLiteral("METADATA")));
    QCOMPARE(console->lines()->rowCount(), statusRowsBeforeEmpty);
    QVERIFY(selectedBodiesContain(
        messages, QStringLiteral("No standing status. Use /status <text> or /status clear.")));
    QCOMPARE(controller.peerMetadata(QStringLiteral("libera"), QStringLiteral("omairc"))
                 .value(QStringLiteral("status"))
                 .toString(),
             QString());

    QVERIFY(controller.sendMessage(QStringLiteral("/status writing docs")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("METADATA * SET status :writing docs\r\n"));
    QVERIFY(!framesContain(QByteArrayList{transport->writtenFrames().last()},
                           QByteArrayLiteral("AWAY")));
    QVERIFY(!controller.selfAway());
    QCOMPARE(controller.peerMetadata(QStringLiteral("libera"), QStringLiteral("omairc"))
                 .value(QStringLiteral("status"))
                 .toString(),
             QString());
    QVERIFY(!selectedBodiesContain(messages,
                                   QStringLiteral("Standing status set to writing docs.")));
    transport->injectBytes(
        QByteArrayLiteral(":server 761 omairc omairc status * :writing docs\r\n"));
    QCOMPARE(controller.peerMetadata(QStringLiteral("libera"), QStringLiteral("omairc"))
                 .value(QStringLiteral("status"))
                 .toString(),
             QStringLiteral("writing docs"));
    QCOMPARE(memberRole(members, QStringLiteral("omairc"), MemberListModel::StatusRole),
             QStringLiteral("writing docs"));
    QCOMPARE(memberRole(members, QStringLiteral("omairc"), MemberListModel::AwayRole),
             false);
    QVERIFY(selectedBodiesContain(messages,
                                  QStringLiteral("Standing status set to writing docs.")));

    const int beforeQuery = transport->writtenFrames().size();
    QVERIFY(controller.sendMessage(QStringLiteral("/status")));
    QCOMPARE(transport->writtenFrames().size(), beforeQuery);
    QVERIFY(!framesContain(transport->writtenFrames().mid(beforeQuery),
                           QByteArrayLiteral("METADATA")));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("Standing status: writing docs")));
    QCOMPARE(controller.peerMetadata(QStringLiteral("libera"), QStringLiteral("omairc"))
                 .value(QStringLiteral("status"))
                 .toString(),
             QStringLiteral("writing docs"));

    QVERIFY(controller.sendMessage(QStringLiteral("/status clear the table")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("METADATA * SET status :clear the table\r\n"));
    QCOMPARE(controller.peerMetadata(QStringLiteral("libera"), QStringLiteral("omairc"))
                 .value(QStringLiteral("status"))
                 .toString(),
             QStringLiteral("writing docs"));
    QVERIFY(!selectedBodiesContain(
        messages, QStringLiteral("Standing status set to clear the table.")));
    transport->injectBytes(
        QByteArrayLiteral(":server 761 omairc omairc status * :clear the table\r\n"));
    QCOMPARE(controller.peerMetadata(QStringLiteral("libera"), QStringLiteral("omairc"))
                 .value(QStringLiteral("status"))
                 .toString(),
             QStringLiteral("clear the table"));
    QVERIFY(selectedBodiesContain(
        messages, QStringLiteral("Standing status set to clear the table.")));

    QVERIFY(controller.sendMessage(QStringLiteral("/STATUS clear")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("METADATA * SET status\r\n"));
    QCOMPARE(controller.peerMetadata(QStringLiteral("libera"), QStringLiteral("omairc"))
                 .value(QStringLiteral("status"))
                 .toString(),
             QStringLiteral("clear the table"));
    const int clearedHitsBeforeAlias =
        selectedBodyHits(messages, QStringLiteral("Standing status cleared."));
    transport->injectBytes(
        QByteArrayLiteral(":server 766 omairc omairc status :unset\r\n"));
    QCOMPARE(controller.peerMetadata(QStringLiteral("libera"), QStringLiteral("omairc"))
                 .value(QStringLiteral("status"))
                 .toString(),
             QString());
    QCOMPARE(memberRole(members, QStringLiteral("omairc"), MemberListModel::StatusRole),
             QString());
    QCOMPARE(selectedBodyHits(messages, QStringLiteral("Standing status cleared.")),
             clearedHitsBeforeAlias + 1);
    QVERIFY(!controller.selfAway());
    QVERIFY(!framesContain(QByteArrayList{transport->writtenFrames().last()},
                           QByteArrayLiteral("AWAY")));

    const int statusRowsBeforeSubmit = console->lines()->rowCount();
    const int transcriptHitsBeforeSubmit =
        selectedBodyHits(messages, QStringLiteral("Standing status set to lunch."));
    QVERIFY(console->submit(QStringLiteral("/status lunch")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("METADATA * SET status :lunch\r\n"));
    QCOMPARE(controller.peerMetadata(QStringLiteral("libera"), QStringLiteral("omairc"))
                 .value(QStringLiteral("status"))
                 .toString(),
             QString());
    QVERIFY(!logContains(console->lines(), QStringLiteral("Standing status set to lunch.")));
    transport->injectBytes(
        QByteArrayLiteral(":server 761 omairc omairc status * :lunch\r\n"));
    QVERIFY(!logContains(console->lines(), QStringLiteral("Standing status set to lunch.")));
    QCOMPARE(console->lines()->rowCount(), statusRowsBeforeSubmit);
    QCOMPARE(selectedBodyHits(messages, QStringLiteral("Standing status set to lunch.")),
             transcriptHitsBeforeSubmit + 1);
    QCOMPARE(controller.peerMetadata(QStringLiteral("libera"), QStringLiteral("omairc"))
                 .value(QStringLiteral("status"))
                 .toString(),
             QStringLiteral("lunch"));
    QVERIFY(!controller.selfAway());

    IrcController unsupported;
    auto *plain = new FakeIrcTransport;
    IrcSession *plainSession = unsupported.addSession(config(), plain);
    QVERIFY(plainSession);
    QVERIFY(unsupported.start(QStringLiteral("libera")));
    welcome(plain);
    QCOMPARE(plainSession->state(), IrcSession::State::Registered);
    plain->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    unsupported.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(!unsupported.hasMemberStatus());
    const int beforeUnsupported = plain->writtenFrames().size();
    QVERIFY(unsupported.sendMessage(QStringLiteral("/status hi")));
    QCOMPARE(plain->writtenFrames().size(), beforeUnsupported);
    QVERIFY(!framesContain(plain->writtenFrames().mid(beforeUnsupported),
                           QByteArrayLiteral("METADATA")));
    QVERIFY(selectedBodiesContain(
        qobject_cast<QAbstractItemModel *>(unsupported.messages()),
        QStringLiteral("This network does not support standing status.")));

    IrcController lonely;
    QVERIFY(!lonely.sendMessage(QStringLiteral("/status hi")));
    QCOMPARE(lonely.lastError(), QStringLiteral("Not connected"));
}

void CommandTest::statusRefusesOnMetadataError()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcomeMetadata(transport);
    QCOMPARE(session->state(), IrcSession::State::Registered);
    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :omairc Alice\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);

    QVERIFY(controller.sendMessage(QStringLiteral("/status blocked")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("METADATA * SET status :blocked\r\n"));
    QCOMPARE(controller.peerMetadata(QStringLiteral("libera"), QStringLiteral("omairc"))
                 .value(QStringLiteral("status"))
                 .toString(),
             QString());
    transport->injectBytes(
        QByteArrayLiteral(":server 769 omairc status :permission denied\r\n"));
    QCOMPARE(controller.peerMetadata(QStringLiteral("libera"), QStringLiteral("omairc"))
                 .value(QStringLiteral("status"))
                 .toString(),
             QString());
    QVERIFY(selectedBodiesContain(
        messages,
        QStringLiteral("Could not set standing status: status permission denied")));
}

void CommandTest::statusRefusesOnMetadataFailReplies()
{
    auto runFail = [&](const QByteArray& failLine, const QString& needle) {
        IrcController controller;
        auto *transport = new FakeIrcTransport;
        IrcSession *session = controller.addSession(config(), transport);
        QVERIFY(session);
        QVERIFY(controller.start(QStringLiteral("libera")));
        welcomeMetadata(transport);
        transport->injectBytes(
            QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"
                              ":server 353 omairc = #omarchy :omairc Alice\r\n"
                              ":server 366 omairc #omarchy :End of NAMES\r\n"));
        controller.selectConversation(QStringLiteral("libera"),
                                      QStringLiteral("#omarchy"));
        auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
        QVERIFY(messages);

        QVERIFY(controller.sendMessage(QStringLiteral("/status blocked")));
        QCOMPARE(transport->writtenFrames().last(),
                 QByteArrayLiteral("METADATA * SET status :blocked\r\n"));
        transport->injectBytes(failLine);
        QCOMPARE(controller.peerMetadata(QStringLiteral("libera"),
                                         QStringLiteral("omairc"))
                     .value(QStringLiteral("status"))
                     .toString(),
                 QString());
        QVERIFY(selectedBodiesContain(messages, needle));

        // A later success for status must not falsely confirm the failed watch.
        transport->injectBytes(
            QByteArrayLiteral(
                ":server 761 omairc omairc status * :should-not-confirm\r\n"));
        QVERIFY(!selectedBodiesContain(
            messages, QStringLiteral("Standing status set to should-not-confirm")));
        QCOMPARE(controller.peerMetadata(QStringLiteral("libera"),
                                         QStringLiteral("omairc"))
                     .value(QStringLiteral("status"))
                     .toString(),
                 QStringLiteral("should-not-confirm"));
    };

    runFail(QByteArrayLiteral(
                ":server FAIL METADATA KEY_NO_PERMISSION omairc status "
                ":permission denied\r\n"),
            QStringLiteral("Could not set standing status: KEY_NO_PERMISSION"));
    runFail(QByteArrayLiteral(
                ":server FAIL METADATA RATE_LIMITED * status 5 "
                ":too many changes\r\n"),
            QStringLiteral("Could not set standing status: RATE_LIMITED"));

    IrcController keyless;
    auto *keylessTransport = new FakeIrcTransport;
    IrcSession *keylessSession = keyless.addSession(config(), keylessTransport);
    QVERIFY(keylessSession);
    QVERIFY(keyless.start(QStringLiteral("libera")));
    welcomeMetadata(keylessTransport);
    keylessTransport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :omairc Alice\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));
    keyless.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *keylessMessages =
        qobject_cast<QAbstractItemModel *>(keyless.messages());
    QVERIFY(keylessMessages);
    QVERIFY(keyless.sendMessage(QStringLiteral("/status blocked")));
    keylessTransport->injectBytes(
        QByteArrayLiteral(
            ":server FAIL METADATA VALUE_INVALID :value is too long\r\n"));
    QVERIFY(!selectedBodiesContain(
        keylessMessages,
        QStringLiteral("Could not set standing status")));
    keylessTransport->injectBytes(
        QByteArrayLiteral(":server 761 omairc omairc status * :blocked\r\n"));
    QVERIFY(selectedBodiesContain(
        keylessMessages, QStringLiteral("Standing status set to blocked.")));

    // Unrelated FAIL for another key must not consume the watch.
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcomeMetadata(transport);
    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :omairc Alice\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(controller.sendMessage(QStringLiteral("/status waiting")));
    transport->injectBytes(
        QByteArrayLiteral(
            ":server FAIL METADATA KEY_NO_PERMISSION omairc avatar "
            ":no avatar permission\r\n"));
    QVERIFY(!selectedBodiesContain(
        messages, QStringLiteral("Could not set standing status")));
    transport->injectBytes(
        QByteArrayLiteral(":server 761 omairc omairc status * :waiting\r\n"));
    QVERIFY(selectedBodiesContain(
        messages, QStringLiteral("Standing status set to waiting.")));
}

void CommandTest::ownMetadataFailKeylessPreservesConcurrentWatches()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcomeMetadata(transport);
    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :omairc Alice\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);

    QVERIFY(controller.sendMessage(QStringLiteral("/status writing")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("METADATA * SET status :writing\r\n"));
    QVERIFY(controller.sendMessage(
        QStringLiteral("/avatar https://example.com/a.png")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("METADATA * SET avatar :https://example.com/a.png\r\n"));

    transport->injectBytes(
        QByteArrayLiteral(
            ":server FAIL METADATA VALUE_INVALID :value is too long\r\n"));
    QVERIFY(!selectedBodiesContain(
        messages, QStringLiteral("Could not set standing status")));
    QVERIFY(!selectedBodiesContain(
        messages, QStringLiteral("Could not set avatar")));

    transport->injectBytes(
        QByteArrayLiteral(
            ":server FAIL METADATA KEY_NO_PERMISSION omairc avatar "
            ":permission denied\r\n"));
    QVERIFY(!selectedBodiesContain(
        messages, QStringLiteral("Could not set standing status")));
    QVERIFY(selectedBodiesContain(
        messages, QStringLiteral("Could not set avatar: KEY_NO_PERMISSION")));

    transport->injectBytes(
        QByteArrayLiteral(":server 761 omairc omairc status * :writing\r\n"));
    QVERIFY(selectedBodiesContain(
        messages, QStringLiteral("Standing status set to writing.")));
    transport->injectBytes(
        QByteArrayLiteral(
            ":server 761 omairc omairc avatar * :https://example.com/a.png\r\n"));
    QVERIFY(!selectedBodiesContain(
        messages,
        QStringLiteral("Avatar set to https://example.com/a.png.")));
}

void CommandTest::statusRefusesNonEmptyWhenMaxValueBytesZero()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(
            ":server CAP omairc LS :away-notify batch "
            "draft/metadata-2=max-value-bytes=0\r\n"
            ":server CAP omairc ACK :away-notify batch draft/metadata-2\r\n"
            ":server 001 omairc :Welcome\r\n"));
    QCOMPARE(session->state(), IrcSession::State::Registered);
    QCOMPARE(session->metadataCapability().maxValueBytes, 0);
    QCOMPARE(IrcMetadata::effectiveMaxValueBytes(
                 session->metadataCapability().maxValueBytes),
             0);
    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :omairc Alice\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);

    const int beforeSet = transport->writtenFrames().size();
    QVERIFY(controller.sendMessage(QStringLiteral("/status writing")));
    QCOMPARE(transport->writtenFrames().size(), beforeSet);
    QVERIFY(!framesContain(transport->writtenFrames().mid(beforeSet),
                           QByteArrayLiteral("METADATA * SET status :")));
    QVERIFY(selectedBodiesContain(
        messages,
        QStringLiteral("This network does not allow standing status text.")));

    QVERIFY(controller.sendMessage(QStringLiteral("/status clear")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("METADATA * SET status\r\n"));
    transport->injectBytes(
        QByteArrayLiteral(":server 766 omairc omairc status :unset\r\n"));
    QVERIFY(selectedBodiesContain(messages,
                                  QStringLiteral("Standing status cleared.")));
}

void CommandTest::avatarRefusesNonEmptyWhenMaxValueBytesZero()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(
            ":server CAP omairc LS :away-notify batch "
            "draft/metadata-2=max-value-bytes=0\r\n"
            ":server CAP omairc ACK :away-notify batch draft/metadata-2\r\n"
            ":server 001 omairc :Welcome\r\n"));
    QCOMPARE(session->state(), IrcSession::State::Registered);
    QCOMPARE(session->metadataCapability().maxValueBytes, 0);
    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :omairc Alice\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);

    const int beforeSet = transport->writtenFrames().size();
    QVERIFY(controller.sendMessage(
        QStringLiteral("/avatar https://example.com/a.png")));
    QCOMPARE(transport->writtenFrames().size(), beforeSet);
    QVERIFY(!framesContain(transport->writtenFrames().mid(beforeSet),
                           QByteArrayLiteral("METADATA * SET avatar :")));
    QVERIFY(selectedBodiesContain(
        messages,
        QStringLiteral("This network does not allow avatar URLs.")));

    QVERIFY(controller.sendMessage(QStringLiteral("/avatar clear")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("METADATA * SET avatar\r\n"));
    transport->injectBytes(
        QByteArrayLiteral(":server 766 omairc omairc avatar :unset\r\n"));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("Avatar cleared.")));
}

void CommandTest::avatarClearTreatsKeyNotSetAsSuccess()
{
    IrcProfileStore().save(liberaStoredProfile());
    CommandCredentialStore credentials;
    IrcController controller;
    IrcConnection connection(controller, nullTransportFactory(), credentials);
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcomeMetadata(transport);
    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :omairc Alice\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);

    QVERIFY(controller.sendMessage(
        QStringLiteral("/avatar https://example.com/a.png")));
    transport->injectBytes(
        QByteArrayLiteral(
            ":server 761 omairc omairc avatar * :https://example.com/a.png\r\n"));

    QVERIFY(controller.sendMessage(QStringLiteral("/avatar clear")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("METADATA * SET avatar\r\n"));
    transport->injectBytes(
        QByteArrayLiteral(
            ":server FAIL METADATA KEY_NOT_SET omairc avatar "
            ":already unset\r\n"));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("Avatar cleared.")));
    {
        IrcNetworkProfile loaded;
        for (const IrcNetworkProfile& profile : IrcProfileStore().profiles()) {
            if (profile.networkId == QStringLiteral("libera")) {
                loaded = profile;
                break;
            }
        }
        QVERIFY(loaded.avatarUrl.isEmpty());
    }
}

void CommandTest::avatarWritesMetadataFrames()
{
    IrcProfileStore().save(liberaStoredProfile());
    CommandCredentialStore credentials;
    IrcController controller;
    IrcConnection connection(controller, nullTransportFactory(), credentials);
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcomeMetadata(transport);
    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :omairc Alice\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);

    const QByteArray hash =
        QCryptographicHash::hash(QByteArrayLiteral("me@example.com"),
                                 QCryptographicHash::Sha256)
            .toHex();
    const QString gravatarUrl =
        QStringLiteral("https://www.gravatar.com/avatar/%1?s={size}&d=404")
            .arg(QString::fromLatin1(hash));

    QVERIFY(controller.sendMessage(
        QStringLiteral("/avatar https://example.com/a.png")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("METADATA * SET avatar :https://example.com/a.png\r\n"));
    transport->injectBytes(
        QByteArrayLiteral(
            ":server 761 omairc omairc avatar * :https://example.com/a.png\r\n"));
    QVERIFY(selectedBodiesContain(
        messages, QStringLiteral("Avatar set to https://example.com/a.png.")));
    {
        IrcNetworkProfile loaded;
        for (const IrcNetworkProfile& profile : IrcProfileStore().profiles()) {
            if (profile.networkId == QStringLiteral("libera")) {
                loaded = profile;
                break;
            }
        }
        QCOMPARE(loaded.avatarUrl, QStringLiteral("https://example.com/a.png"));
    }

    QVERIFY(controller.sendMessage(QStringLiteral("/avatar me@example.com")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("METADATA * SET avatar :")
                 + gravatarUrl.toUtf8()
                 + QByteArrayLiteral("\r\n"));
    transport->injectBytes(
        QByteArrayLiteral(":server 761 omairc omairc avatar * :")
            + gravatarUrl.toUtf8()
            + QByteArrayLiteral("\r\n"));
    QVERIFY(selectedBodiesContain(
        messages, QStringLiteral("Avatar set to %1.").arg(gravatarUrl)));

    QVERIFY(controller.sendMessage(QStringLiteral("/avatar clear")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("METADATA * SET avatar\r\n"));
    transport->injectBytes(
        QByteArrayLiteral(":server 766 omairc omairc avatar :unset\r\n"));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("Avatar cleared.")));
    {
        IrcNetworkProfile loaded;
        for (const IrcNetworkProfile& profile : IrcProfileStore().profiles()) {
            if (profile.networkId == QStringLiteral("libera")) {
                loaded = profile;
                break;
            }
        }
        QVERIFY(loaded.avatarUrl.isEmpty());
    }

    const int beforeInspect = transport->writtenFrames().size();
    QVERIFY(controller.sendMessage(QStringLiteral("/avatar")));
    QCOMPARE(transport->writtenFrames().size(), beforeInspect);
    QVERIFY(!framesContain(transport->writtenFrames().mid(beforeInspect),
                           QByteArrayLiteral("METADATA")));
    QVERIFY(selectedBodiesContain(
        messages, QStringLiteral("No standing avatar. Use /avatar <url|email> or /avatar clear.")));

    transport->injectBytes(
        QByteArrayLiteral(
            ":server 761 omairc omairc avatar * :https://example.com/a.png\r\n"));
    QVERIFY(controller.sendMessage(QStringLiteral("/avatar")));
    QCOMPARE(transport->writtenFrames().size(), beforeInspect);
    QVERIFY(selectedBodiesContain(
        messages,
        QStringLiteral("Standing avatar: https://example.com/a.png")));
}

void CommandTest::avatarAppliesSavedUrlOnConnect()
{
    IrcNetworkProfile profile = liberaStoredProfile();
    profile.avatarUrl = QStringLiteral("https://example.com/saved.png");
    IrcProfileStore().save(profile);

    CommandCredentialStore credentials;
    IrcController controller;
    IrcConnection connection(controller, nullTransportFactory(), credentials);
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcomeMetadata(transport);
    QCOMPARE(session->state(), IrcSession::State::Registered);
    QVERIFY(framesContain(transport->writtenFrames(),
                          QByteArrayLiteral(
                              "METADATA * SET avatar :https://example.com/saved.png\r\n")));
}

void CommandTest::avatarAppliesSavedUrlAfterLateCaps()
{
    IrcNetworkProfile profile = liberaStoredProfile();
    profile.avatarUrl = QStringLiteral("https://example.com/saved.png");
    IrcProfileStore().save(profile);

    CommandCredentialStore credentials;
    IrcController controller;
    IrcConnection connection(controller, nullTransportFactory(), credentials);
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :away-notify\r\n"
                          ":server CAP omairc ACK :away-notify\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    QCOMPARE(session->state(), IrcSession::State::Registered);
    QVERIFY(!framesContain(transport->writtenFrames(),
                          QByteArrayLiteral("METADATA * SET avatar")));

    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc NEW :batch draft/metadata-2\r\n"));
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc ACK :batch draft/metadata-2\r\n"));
    QVERIFY(framesContain(transport->writtenFrames(),
                          QByteArrayLiteral(
                              "METADATA * SET avatar :https://example.com/saved.png\r\n")));
}

void CommandTest::avatarRefusesUnsafeInput()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcomeMetadata(transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);

    const int beforeUnsafe = transport->writtenFrames().size();
    QVERIFY(controller.sendMessage(QStringLiteral("/avatar http://example.com/a.png")));
    QCOMPARE(transport->writtenFrames().size(), beforeUnsafe);
    QVERIFY(selectedBodiesContain(
        messages,
        QStringLiteral("Avatar must be an HTTPS URL or an email address.")));

    IrcController unsupported;
    auto *plain = new FakeIrcTransport;
    IrcSession *plainSession = unsupported.addSession(config(), plain);
    QVERIFY(plainSession);
    QVERIFY(unsupported.start(QStringLiteral("libera")));
    welcome(plain);
    plain->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    unsupported.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    const int beforeUnsupported = plain->writtenFrames().size();
    QVERIFY(unsupported.sendMessage(
        QStringLiteral("/avatar https://example.com/a.png")));
    QCOMPARE(plain->writtenFrames().size(), beforeUnsupported);
    QVERIFY(selectedBodiesContain(
        qobject_cast<QAbstractItemModel *>(unsupported.messages()),
        QStringLiteral("This network does not support avatars.")));
}

void CommandTest::avatarRefusesOnMetadataFailReplies()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcomeMetadata(transport);
    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :omairc Alice\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);

    QVERIFY(controller.sendMessage(QStringLiteral("/status waiting")));
    transport->injectBytes(
        QByteArrayLiteral(
            ":server FAIL METADATA KEY_NO_PERMISSION omairc avatar "
            ":no avatar permission\r\n"));
    QVERIFY(!selectedBodiesContain(
        messages, QStringLiteral("Could not set standing status")));
    transport->injectBytes(
        QByteArrayLiteral(":server 761 omairc omairc status * :waiting\r\n"));
    QVERIFY(selectedBodiesContain(
        messages, QStringLiteral("Standing status set to waiting.")));

    QVERIFY(controller.sendMessage(
        QStringLiteral("/avatar https://example.com/a.png")));
    transport->injectBytes(
        QByteArrayLiteral(
            ":server FAIL METADATA KEY_NO_PERMISSION omairc avatar "
            ":permission denied\r\n"));
    QVERIFY(selectedBodiesContain(
        messages, QStringLiteral("Could not set avatar: KEY_NO_PERMISSION")));
    transport->injectBytes(
        QByteArrayLiteral(
            ":server 761 omairc omairc avatar * :https://example.com/a.png\r\n"));
    QVERIFY(!selectedBodiesContain(
        messages,
        QStringLiteral("Avatar set to https://example.com/a.png.")));
}

void CommandTest::whoisSendsAndDefaults()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    QCOMPARE(session->state(), IrcSession::State::Registered);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/whois lena")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("WHOIS lena lena\r\n"));

    IrcStatusConsole *console = controller.console();
    QVERIFY(console->submit(QStringLiteral("/whois lena")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("WHOIS lena lena\r\n"));

    QVERIFY(controller.sendMessage(QStringLiteral("/whois #omarchy")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("WHOIS #omarchy #omarchy\r\n"));

    const int beforeChannelEmpty = transport->writtenFrames().size();
    QVERIFY(!controller.sendMessage(QStringLiteral("/whois")));
    QCOMPARE(controller.lastError(),
             QStringLiteral("Name a nick"));
    QCOMPARE(transport->writtenFrames().size(), beforeChannelEmpty);
    QVERIFY(!framesContain(transport->writtenFrames().mid(beforeChannelEmpty),
                           QByteArrayLiteral("WHOIS")));

    transport->injectBytes(QByteArrayLiteral(":lena!u@h PRIVMSG omairc :hi\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    QVERIFY(controller.sendMessage(QStringLiteral("/whois")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("WHOIS lena lena\r\n"));
    QVERIFY(console->submit(QStringLiteral("/whois")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("WHOIS lena lena\r\n"));

    IrcController statusOnly;
    auto *statusTransport = new FakeIrcTransport;
    IrcSession *statusSession = statusOnly.addSession(config(), statusTransport);
    QVERIFY(statusSession);
    QVERIFY(statusOnly.start(QStringLiteral("libera")));
    welcome(statusTransport);
    QCOMPARE(statusSession->state(), IrcSession::State::Registered);
    QVERIFY(statusOnly.selectedTarget().isEmpty());
    IrcStatusConsole *statusConsole = statusOnly.console();
    const int beforeStatusEmpty = statusTransport->writtenFrames().size();
    QVERIFY(statusConsole->submit(QStringLiteral("/whois")));
    QVERIFY(logContains(statusConsole->lines(), QStringLiteral("Command was refused")));
    QCOMPARE(statusTransport->writtenFrames().size(), beforeStatusEmpty);
    QVERIFY(!framesContain(statusTransport->writtenFrames().mid(beforeStatusEmpty),
                           QByteArrayLiteral("WHOIS")));

    transport->remoteClose();
    QVERIFY(session->state() != IrcSession::State::Registered);
    const int framesBefore = transport->writtenFrames().size();
    QVERIFY(!controller.sendMessage(QStringLiteral("/whois lena")));
    QCOMPARE(controller.lastError(), QStringLiteral("Not connected"));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);
    QVERIFY(!framesContain(transport->writtenFrames().mid(framesBefore),
                           QByteArrayLiteral("WHOIS")));
}

void CommandTest::ctcpSendsAndDefaults()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    QCOMPARE(session->state(), IrcSession::State::Registered);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/version lena")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArray("PRIVMSG lena :\x01" "VERSION\x01\r\n"));

    QVERIFY(controller.sendMessage(QStringLiteral("/time lena")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArray("PRIVMSG lena :\x01" "TIME\x01\r\n"));

    QVERIFY(controller.sendMessage(QStringLiteral("/ping lena")));
    const QByteArray pingFrame = transport->writtenFrames().last();
    QVERIFY(pingFrame.startsWith(QByteArray("PRIVMSG lena :\x01" "PING ")));
    QVERIFY(pingFrame.endsWith(QByteArray("\x01\r\n")));

    IrcStatusConsole *console = controller.console();
    QVERIFY(!logContains(console->lines(), QString(QChar(1))));
    QVERIFY(!logContains(console->lines(), QStringLiteral("PRIVMSG lena")));

    QVERIFY(console->submit(QStringLiteral("/version lena")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArray("PRIVMSG lena :\x01" "VERSION\x01\r\n"));

    const int beforeChannel = transport->writtenFrames().size();
    QVERIFY(!controller.sendMessage(QStringLiteral("/ping #omarchy")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QVERIFY(!controller.sendMessage(QStringLiteral("/version lena extra")));
    QCOMPARE(transport->writtenFrames().size(), beforeChannel);

    const int beforeChannelEmpty = transport->writtenFrames().size();
    QVERIFY(!controller.sendMessage(QStringLiteral("/ping")));
    QCOMPARE(controller.lastError(), QStringLiteral("Name a nick"));
    QVERIFY(!controller.sendMessage(QStringLiteral("/time")));
    QCOMPARE(controller.lastError(), QStringLiteral("Name a nick"));
    QVERIFY(!controller.sendMessage(QStringLiteral("/version")));
    QCOMPARE(transport->writtenFrames().size(), beforeChannelEmpty);

    transport->injectBytes(QByteArrayLiteral(":lena!u@h PRIVMSG omairc :hi\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    QVERIFY(controller.sendMessage(QStringLiteral("/version")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArray("PRIVMSG lena :\x01" "VERSION\x01\r\n"));
    QVERIFY(controller.sendMessage(QStringLiteral("/time")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArray("PRIVMSG lena :\x01" "TIME\x01\r\n"));
    QVERIFY(controller.sendMessage(QStringLiteral("/ping")));
    QVERIFY(transport->writtenFrames().last().startsWith(
        QByteArray("PRIVMSG lena :\x01" "PING ")));

    IrcController statusOnly;
    auto *statusTransport = new FakeIrcTransport;
    IrcSession *statusSession = statusOnly.addSession(config(), statusTransport);
    QVERIFY(statusSession);
    QVERIFY(statusOnly.start(QStringLiteral("libera")));
    welcome(statusTransport);
    QCOMPARE(statusSession->state(), IrcSession::State::Registered);
    QVERIFY(statusOnly.selectedTarget().isEmpty());
    IrcStatusConsole *statusConsole = statusOnly.console();
    const int beforeStatusEmpty = statusTransport->writtenFrames().size();
    QVERIFY(statusConsole->submit(QStringLiteral("/ping")));
    QVERIFY(logContains(statusConsole->lines(), QStringLiteral("Command was refused")));
    QCOMPARE(statusTransport->writtenFrames().size(), beforeStatusEmpty);

    transport->remoteClose();
    QVERIFY(session->state() != IrcSession::State::Registered);
    const int framesBefore = transport->writtenFrames().size();
    QVERIFY(!controller.sendMessage(QStringLiteral("/version lena")));
    QCOMPARE(controller.lastError(), QStringLiteral("Not connected"));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);
}

void CommandTest::modeSendsAndRefuses()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    QCOMPARE(session->state(), IrcSession::State::Registered);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/mode #omarchy +o lena")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("MODE #omarchy +o lena\r\n"));

    QVERIFY(controller.sendMessage(QStringLiteral("/mode #omarchy +ooo n1 n2 n3")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("MODE #omarchy +ooo n1 n2 n3\r\n"));

    QVERIFY(controller.sendMessage(QStringLiteral("/mode #omarchy -o lena")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("MODE #omarchy -o lena\r\n"));

    QVERIFY(controller.sendMessage(QStringLiteral("/mode #omarchy +b *!*@*.example")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("MODE #omarchy +b *!*@*.example\r\n"));

    QVERIFY(controller.sendMessage(QStringLiteral("/mode #omarchy")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("MODE #omarchy\r\n"));

    const int beforeNick = transport->writtenFrames().size();
    QVERIFY(!controller.sendMessage(QStringLiteral("/mode lena +i")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QCOMPARE(transport->writtenFrames().size(), beforeNick);
    QVERIFY(!framesContain(transport->writtenFrames().mid(beforeNick),
                           QByteArrayLiteral("MODE")));

    const int beforeEmpty = transport->writtenFrames().size();
    QVERIFY(!controller.sendMessage(QStringLiteral("/mode")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QCOMPARE(transport->writtenFrames().size(), beforeEmpty);
    QVERIFY(!framesContain(transport->writtenFrames().mid(beforeEmpty),
                           QByteArrayLiteral("MODE")));

    IrcStatusConsole *console = controller.console();
    QVERIFY(console->submit(QStringLiteral("/mode #omarchy +o lena")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("MODE #omarchy +o lena\r\n"));

    IrcController lonely;
    QVERIFY(!lonely.sendMessage(QStringLiteral("/mode")));
    QCOMPARE(lonely.lastError(), QStringLiteral("Command was refused"));

    IrcController unselected;
    auto *unselectedTransport = new FakeIrcTransport;
    IrcSession *unselectedSession = unselected.addSession(config(), unselectedTransport);
    QVERIFY(unselectedSession);
    QVERIFY(unselected.start(QStringLiteral("libera")));
    welcome(unselectedTransport);
    QCOMPARE(unselectedSession->state(), IrcSession::State::Registered);
    QVERIFY(unselected.selectedTarget().isEmpty());
    const int beforeUnselected = unselectedTransport->writtenFrames().size();
    QVERIFY(!unselected.sendMessage(QStringLiteral("/mode #omarchy")));
    QCOMPARE(unselected.lastError(),
             QStringLiteral("Select a connected conversation first"));
    QCOMPARE(unselectedTransport->writtenFrames().size(), beforeUnselected);
    QVERIFY(!framesContain(unselectedTransport->writtenFrames().mid(beforeUnselected),
                           QByteArrayLiteral("MODE")));

    transport->remoteClose();
    QVERIFY(session->state() != IrcSession::State::Registered);
    const int beforeOffline = transport->writtenFrames().size();
    QVERIFY(!controller.sendMessage(QStringLiteral("/mode #omarchy +o lena")));
    QCOMPARE(controller.lastError(), QStringLiteral("Not connected"));
    QCOMPARE(transport->writtenFrames().size(), beforeOffline);
    QVERIFY(!framesContain(transport->writtenFrames().mid(beforeOffline),
                           QByteArrayLiteral("MODE")));
}

void CommandTest::wrappersSendAndHelp()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    QCOMPARE(session->state(), IrcSession::State::Registered);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/op alice")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("MODE #omarchy +o alice\r\n"));

    QVERIFY(controller.sendMessage(QStringLiteral("/deop alice")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("MODE #omarchy -o alice\r\n"));

    QVERIFY(controller.sendMessage(QStringLiteral("/voice bob")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("MODE #omarchy +v bob\r\n"));

    QVERIFY(controller.sendMessage(QStringLiteral("/devoice bob")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("MODE #omarchy -v bob\r\n"));

    QVERIFY(controller.sendMessage(QStringLiteral("/ban eve")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("MODE #omarchy +b eve!*@*\r\n"));

    QVERIFY(controller.sendMessage(QStringLiteral("/ban *!*@spam.host")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("MODE #omarchy +b *!*@spam.host\r\n"));

    const int beforeEmptyOp = transport->writtenFrames().size();
    QVERIFY(!controller.sendMessage(QStringLiteral("/op")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QVERIFY(!controller.sendMessage(QStringLiteral("/op alice extra")));
    QCOMPARE(transport->writtenFrames().size(), beforeEmptyOp);
    QVERIFY(!framesContain(transport->writtenFrames().mid(beforeEmptyOp),
                           QByteArrayLiteral("MODE")));

    QVERIFY(controller.sendMessage(QStringLiteral("/ns status")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG NickServ :status\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/cs info #omarchy")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG ChanServ :info #omarchy\r\n"));

    QVERIFY(controller.sendMessage(QStringLiteral("/znc help")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG *status :help\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    const int beforeEmptyZnc = transport->writtenFrames().size();
    QVERIFY(!controller.sendMessage(QStringLiteral("/znc")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QCOMPARE(transport->writtenFrames().size(), beforeEmptyZnc);

    QVERIFY(controller.sendMessage(QStringLiteral("/raw PING :x")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PING :x\r\n"));

    QVERIFY(controller.sendMessage(QStringLiteral("/quote PING :y")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PING :y\r\n"));

    const int beforeBadRaw = transport->writtenFrames().size();
    QVERIFY(!controller.sendMessage(QStringLiteral("/raw")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QVERIFY(!controller.sendMessage(QStringLiteral("/raw PING :x\nQUIT")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QVERIFY(!controller.sendMessage(QStringLiteral("/raw ")
                                    + QString(511, QLatin1Char('A'))));
    QCOMPARE(transport->writtenFrames().size(), beforeBadRaw);

    IrcStatusConsole *console = controller.console();
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    const int statusRowsBeforeChannelHelp = console->lines()->rowCount();
    QVERIFY(controller.sendMessage(QStringLiteral("/help")));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QCOMPARE(console->lines()->rowCount(), statusRowsBeforeChannelHelp);
    QVERIFY(!logContains(console->lines(), QStringLiteral("Commands:")));
    QCOMPARE(selectedBodyHits(messages, QStringLiteral("Commands:")), 1);
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("/op")));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("/invite")));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("/ns")));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("/cs")));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("/znc")));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("/raw")));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("/help")));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("/autoaway")));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("/pref")));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("/list")));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("/ping")));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("/time")));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("/version")));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("/mute")));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("/unmute")));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("/muted")));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("/monitor")));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("/unmonitor")));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("/monitored")));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("/ban")));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("/disconnect")));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("/status")));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("Empty /join joins the latest invite")));

    QVERIFY(console->submit(QStringLiteral("/help")));
    QVERIFY(logContains(console->lines(), QStringLiteral("Commands:")));
    QVERIFY(logContains(console->lines(),
                        QStringLiteral("Empty /join joins the latest invite")));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QCOMPARE(selectedBodyHits(messages, QStringLiteral("Commands:")), 1);

    QVERIFY(controller.sendMessage(QStringLiteral("/raw PASS :x")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PASS :x\r\n"));
    QVERIFY(logContains(console->lines(), QStringLiteral("PASS ***")));
    QVERIFY(!logContains(console->lines(), QStringLiteral("PASS :x")));

    QVERIFY(controller.sendMessage(QStringLiteral("/raw AUTHENTICATE PLAIN")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("AUTHENTICATE PLAIN\r\n"));
    QVERIFY(!logContains(console->lines(), QStringLiteral("AUTHENTICATE")));

    QVERIFY(controller.sendMessage(QStringLiteral("/raw OPER admin raw-oper-secret")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("OPER admin raw-oper-secret\r\n"));
    QVERIFY(!logContains(console->lines(), QStringLiteral("raw-oper-secret")));
    QVERIFY(!logContains(console->lines(), QStringLiteral("OPER admin")));

    QVERIFY(controller.sendMessage(QStringLiteral("/ns identify hunter2")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG NickServ :identify hunter2\r\n"));
    QVERIFY(!logContains(console->lines(), QStringLiteral("hunter2")));

    const int beforeStatusOp = transport->writtenFrames().size();
    QVERIFY(console->submit(QStringLiteral("/op alice")));
    QVERIFY(logContains(console->lines(), QStringLiteral("Op applies to channels")));
    QCOMPARE(transport->writtenFrames().size(), beforeStatusOp);

    transport->injectBytes(QByteArrayLiteral(":lena!u@h PRIVMSG omairc :hi\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    const int statusRowsBeforeDmHelp = console->lines()->rowCount();
    QVERIFY(controller.sendMessage(QStringLiteral("/help")));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("lena"));
    QCOMPARE(console->lines()->rowCount(), statusRowsBeforeDmHelp);
    QCOMPARE(selectedBodyHits(messages, QStringLiteral("Commands:")), 1);
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("/help")));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QCOMPARE(selectedBodyHits(messages, QStringLiteral("Commands:")), 1);
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    const int beforeDmOp = transport->writtenFrames().size();
    QVERIFY(!controller.sendMessage(QStringLiteral("/op alice")));
    QCOMPARE(controller.lastError(), QStringLiteral("Op applies to channels"));
    QVERIFY(!controller.sendMessage(QStringLiteral("/ban eve")));
    QCOMPARE(controller.lastError(), QStringLiteral("Ban applies to channels"));
    QCOMPARE(transport->writtenFrames().size(), beforeDmOp);

    QVERIFY(controller.sendMessage(QStringLiteral("/ns help")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG NickServ :help\r\n"));
    QVERIFY(console->submit(QStringLiteral("/cs help")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG ChanServ :help\r\n"));
    QVERIFY(console->submit(QStringLiteral("/znc ListMods")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG *status :ListMods\r\n"));
}

void CommandTest::listSendsAndCaches()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    QCOMPARE(session->state(), IrcSession::State::Registered);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QSignalSpy requested(&controller, &IrcController::channelListRequested);
    const int statusRowsBefore = controller.console()->lines()->rowCount();
    const int framesBefore = transport->writtenFrames().size();
    QVERIFY(controller.sendMessage(QStringLiteral("/list")));
    QCOMPARE(transport->writtenFrames().size(), framesBefore + 1);
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("LIST\r\n"));
    QCOMPARE(requested.size(), 1);

    auto *model = qobject_cast<ChannelListModel *>(controller.channelList());
    QVERIFY(model);
    QVERIFY(model->loading());
    QVERIFY(!model->complete());

    transport->injectBytes(
        QByteArrayLiteral(":server 321 omairc Channel :Users  Name\r\n"
                          ":server 322 omairc #omarchy 12 :Cozy corner\r\n"
                          ":server 322 omairc #linux 42 :Kernel discussion\r\n"
                          ":server 322 omairc #random 4 :Off-topic\r\n"
                          ":server 323 omairc :End of /LIST\r\n"));
    QVERIFY(model->complete());
    QVERIFY(!model->loading());
    QVERIFY(!model->cached());
    QCOMPARE(model->sourceCount(), 3);
    QCOMPARE(model->rowCount(), 3);
    QCOMPARE(model->field(0, QStringLiteral("channel")).toString(),
             QStringLiteral("#linux"));
    QCOMPARE(model->field(0, QStringLiteral("users")).toInt(), 42);
    QCOMPARE(model->field(1, QStringLiteral("channel")).toString(),
             QStringLiteral("#omarchy"));
    QCOMPARE(model->field(2, QStringLiteral("channel")).toString(),
             QStringLiteral("#random"));
    QCOMPARE(controller.console()->lines()->rowCount(), statusRowsBefore);
    QVERIFY(!logContains(controller.console()->lines(), QStringLiteral("#linux")));
    QVERIFY(!logContains(controller.console()->lines(), QStringLiteral("End of /LIST")));

    model->setFilter(QStringLiteral("lin"));
    QCOMPARE(model->rowCount(), 1);
    QCOMPARE(model->field(0, QStringLiteral("channel")).toString(),
             QStringLiteral("#linux"));
    model->setFilter(QString());
    QCOMPARE(model->rowCount(), 3);

    const int framesAfterFirst = transport->writtenFrames().size();
    QVERIFY(controller.sendMessage(QStringLiteral("/list")));
    QCOMPARE(transport->writtenFrames().size(), framesAfterFirst);
    QVERIFY(model->cached());
    QCOMPARE(model->rowCount(), 3);
    QCOMPARE(requested.size(), 2);

    controller.setChannelListPresented(true);
    QVERIFY(controller.sendMessage(QStringLiteral("/list")));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("LIST\r\n"));
    QVERIFY(model->loading());
    transport->injectBytes(
        QByteArrayLiteral(":server 322 omairc #linux 40 :Kernel discussion\r\n"
                          ":server 323 omairc :End of /LIST\r\n"));
    QVERIFY(model->complete());
    QVERIFY(!model->cached());
    QCOMPARE(model->field(0, QStringLiteral("users")).toInt(), 40);

    QVERIFY(controller.sendMessage(QStringLiteral("/list #om*")));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("LIST #om*\r\n"));
    transport->injectBytes(
        QByteArrayLiteral(":server 322 omairc #omarchy 12 :Cozy corner\r\n"
                          ":server 323 omairc :End of /LIST\r\n"));
    QCOMPARE(model->rowCount(), 1);
    QCOMPARE(model->field(0, QStringLiteral("channel")).toString(),
             QStringLiteral("#omarchy"));
    QCOMPARE(model->mask(), QStringLiteral("#om*"));

    QVERIFY(controller.joinListedChannel(QStringLiteral("#linux")));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("JOIN #linux\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#linux"));

    const int framesBeforeFocus = transport->writtenFrames().size();
    QVERIFY(controller.joinListedChannel(QStringLiteral("#omarchy")));
    QCOMPARE(transport->writtenFrames().size(), framesBeforeFocus);
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    const int framesBeforeStatus = transport->writtenFrames().size();
    QVERIFY(controller.console()->submit(QStringLiteral("/list")));
    QCOMPARE(transport->writtenFrames().size(), framesBeforeStatus + 1);
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("LIST\r\n"));

    IrcController offline;
    QVERIFY(!offline.sendMessage(QStringLiteral("/list")));
}

void CommandTest::listFiltersPlainTopic()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    QCOMPARE(session->state(), IrcSession::State::Registered);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/list")));
    auto *model = qobject_cast<ChannelListModel *>(controller.channelList());
    QVERIFY(model);

    const QString topic = QChar(0x02) + QStringLiteral("Kernel") + QChar(0x02)
        + QStringLiteral(" discussion and ")
        + QChar(0x03) + QStringLiteral("04distro") + QChar(0x03)
        + QStringLiteral(" help.");
    transport->injectBytes(
        QByteArrayLiteral(":server 321 omairc Channel :Users  Name\r\n"
                          ":server 322 omairc #linux 42 :")
        + topic.toUtf8()
        + QByteArrayLiteral("\r\n"
                          ":server 322 omairc #random 4 :Off-topic\r\n"
                          ":server 323 omairc :End of /LIST\r\n"));
    QVERIFY(model->complete());
    QCOMPARE(model->rowCount(), 2);
    QCOMPARE(model->field(0, QStringLiteral("channel")).toString(),
             QStringLiteral("#linux"));
    QCOMPARE(model->field(0, QStringLiteral("topic")).toString(), topic);
    QCOMPARE(model->field(0, QStringLiteral("label")).toString(),
             QStringLiteral("#linux 42 Kernel discussion and distro help."));
    QVERIFY(!model->field(0, QStringLiteral("label")).toString().contains(QChar(0x02)));
    QVERIFY(!model->field(0, QStringLiteral("label")).toString().contains(QChar(0x03)));

    model->setFilter(QStringLiteral("distro"));
    QCOMPARE(model->rowCount(), 1);
    QCOMPARE(model->field(0, QStringLiteral("channel")).toString(),
             QStringLiteral("#linux"));

    model->setFilter(QStringLiteral("04"));
    QCOMPARE(model->rowCount(), 0);

    model->setFilter(QStringLiteral("lin"));
    QCOMPARE(model->rowCount(), 1);
    QCOMPARE(model->field(0, QStringLiteral("channel")).toString(),
             QStringLiteral("#linux"));
}

void CommandTest::listSerializesInFlightMaskChange()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    QCOMPARE(session->state(), IrcSession::State::Registered);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QSignalSpy requested(&controller, &IrcController::channelListRequested);
    const int framesBefore = transport->writtenFrames().size();
    QVERIFY(controller.sendMessage(QStringLiteral("/list")));
    QCOMPARE(transport->writtenFrames().size(), framesBefore + 1);
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("LIST\r\n"));
    QCOMPARE(requested.size(), 1);

    auto *model = qobject_cast<ChannelListModel *>(controller.channelList());
    QVERIFY(model);
    QVERIFY(model->loading());
    QVERIFY(!model->complete());

    QVERIFY(controller.sendMessage(QStringLiteral("/list")));
    QCOMPARE(transport->writtenFrames().size(), framesBefore + 1);
    QCOMPARE(requested.size(), 2);

    QVERIFY(controller.sendMessage(QStringLiteral("/list #om*")));
    QCOMPARE(transport->writtenFrames().size(), framesBefore + 1);
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("LIST\r\n"));
    QCOMPARE(requested.size(), 3);
    QVERIFY(model->loading());

    transport->injectBytes(
        QByteArrayLiteral(":server 321 omairc Channel :Users  Name\r\n"
                          ":server 322 omairc #omarchy 12 :Cozy corner\r\n"
                          ":server 322 omairc #linux 42 :Kernel discussion\r\n"
                          ":server 322 omairc #random 4 :Off-topic\r\n"
                          ":server 323 omairc :End of /LIST\r\n"));
    QCOMPARE(transport->writtenFrames().size(), framesBefore + 2);
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("LIST #om*\r\n"));
    QCOMPARE(requested.size(), 4);
    QVERIFY(model->loading());
    QVERIFY(!model->complete());
    QCOMPARE(model->mask(), QStringLiteral("#om*"));
    QCOMPARE(model->rowCount(), 0);

    transport->injectBytes(
        QByteArrayLiteral(":server 322 omairc #omarchy 12 :Cozy corner\r\n"
                          ":server 323 omairc :End of /LIST\r\n"));
    QVERIFY(model->complete());
    QVERIFY(!model->loading());
    QCOMPARE(model->mask(), QStringLiteral("#om*"));
    QCOMPARE(model->sourceCount(), 1);
    QCOMPARE(model->rowCount(), 1);
    QCOMPARE(model->field(0, QStringLiteral("channel")).toString(),
             QStringLiteral("#omarchy"));

    const int framesAfterMasked = transport->writtenFrames().size();
    QVERIFY(controller.sendMessage(QStringLiteral("/list")));
    QCOMPARE(transport->writtenFrames().size(), framesAfterMasked + 1);
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("LIST\r\n"));
    QVERIFY(controller.sendMessage(QStringLiteral("/list #om*")));
    QCOMPARE(transport->writtenFrames().size(), framesAfterMasked + 1);
    QVERIFY(controller.sendMessage(QStringLiteral("/list #lin*")));
    QCOMPARE(transport->writtenFrames().size(), framesAfterMasked + 1);
    QCOMPARE(requested.size(), 7);

    transport->injectBytes(
        QByteArrayLiteral(":server 322 omairc #omarchy 12 :Cozy corner\r\n"
                          ":server 322 omairc #linux 42 :Kernel discussion\r\n"
                          ":server 322 omairc #random 4 :Off-topic\r\n"
                          ":server 323 omairc :End of /LIST\r\n"));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("LIST #lin*\r\n"));
    QCOMPARE(model->mask(), QStringLiteral("#lin*"));
    QCOMPARE(model->rowCount(), 0);

    transport->injectBytes(
        QByteArrayLiteral(":server 322 omairc #linux 42 :Kernel discussion\r\n"
                          ":server 323 omairc :End of /LIST\r\n"));
    QVERIFY(model->complete());
    QCOMPARE(model->mask(), QStringLiteral("#lin*"));
    QCOMPARE(model->rowCount(), 1);
    QCOMPARE(model->field(0, QStringLiteral("channel")).toString(),
             QStringLiteral("#linux"));
}

void CommandTest::listTryAgainUnwedgesAndRetries()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    const int statusRowsBefore = controller.console()->lines()->rowCount();
    QVERIFY(controller.sendMessage(QStringLiteral("/list")));
    auto *model = qobject_cast<ChannelListModel *>(controller.channelList());
    QVERIFY(model);
    QVERIFY(model->loading());

    transport->injectBytes(
        QByteArrayLiteral(":server 263 omairc WHO :Please wait a while and try again.\r\n"));
    QVERIFY(model->loading());
    QVERIFY(model->error().isEmpty());

    transport->injectBytes(
        QByteArrayLiteral(":server 263 omairc LIST :Server load is temporarily too heavy.\r\n"));
    QVERIFY(!model->loading());
    QVERIFY(!model->complete());
    QCOMPARE(model->error(), QStringLiteral("Server load is temporarily too heavy."));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("Server load is temporarily too heavy.")));
    QVERIFY(controller.console()->lines()->rowCount() > statusRowsBefore);

    const int framesAfterFail = transport->writtenFrames().size();
    QVERIFY(controller.sendMessage(QStringLiteral("/list")));
    QCOMPARE(transport->writtenFrames().size(), framesAfterFail + 1);
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("LIST\r\n"));
    QVERIFY(model->loading());
    QVERIFY(model->error().isEmpty());
}

void CommandTest::listTooManyMatchesUnwedges()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/list")));
    auto *model = qobject_cast<ChannelListModel *>(controller.channelList());
    QVERIFY(model);
    transport->injectBytes(
        QByteArrayLiteral(":server 322 omairc #linux 42 :Kernel discussion\r\n"
                          ":server 416 omairc LIST :Too many matches\r\n"));
    QVERIFY(!model->loading());
    QVERIFY(!model->complete());
    QCOMPARE(model->error(), QStringLiteral("Too many matches"));
    QCOMPARE(model->sourceCount(), 1);
    QVERIFY(logContains(controller.console()->lines(), QStringLiteral("Too many matches")));

    const int framesAfterFail = transport->writtenFrames().size();
    QVERIFY(controller.sendMessage(QStringLiteral("/list")));
    QCOMPARE(transport->writtenFrames().size(), framesAfterFail + 1);
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("LIST\r\n"));
    QVERIFY(model->loading());
}

void CommandTest::listIdleTimeoutUnwedges()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    controller.setChannelListIdleTimeoutMs(30);
    QVERIFY(controller.sendMessage(QStringLiteral("/list")));
    auto *model = qobject_cast<ChannelListModel *>(controller.channelList());
    QVERIFY(model);
    QVERIFY(model->loading());
    QTest::qWait(200);
    QVERIFY(!model->loading());
    QVERIFY(!model->complete());
    QVERIFY(model->error().contains(QStringLiteral("timed out")));
    QVERIFY(logContains(controller.console()->lines(), QStringLiteral("timed out")));

    const int framesAfterFail = transport->writtenFrames().size();
    QVERIFY(controller.sendMessage(QStringLiteral("/list")));
    QCOMPARE(transport->writtenFrames().size(), framesAfterFail + 1);
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("LIST\r\n"));
    QVERIFY(model->loading());
}

void CommandTest::listIdleTimeoutLateEndDoesNotCompleteRetry()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    controller.setChannelListIdleTimeoutMs(30);
    QVERIFY(controller.sendMessage(QStringLiteral("/list")));
    auto *model = qobject_cast<ChannelListModel *>(controller.channelList());
    QVERIFY(model);
    QVERIFY(model->loading());
    QTest::qWait(200);
    QVERIFY(!model->loading());
    QVERIFY(!model->complete());
    QVERIFY(model->error().contains(QStringLiteral("timed out")));

    const int framesAfterTimeout = transport->writtenFrames().size();
    QVERIFY(controller.sendMessage(QStringLiteral("/list")));
    QCOMPARE(transport->writtenFrames().size(), framesAfterTimeout + 1);
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("LIST\r\n"));
    QVERIFY(model->loading());
    QVERIFY(model->error().isEmpty());

    transport->injectBytes(
        QByteArrayLiteral(":server 323 omairc :End of /LIST\r\n"));
    QVERIFY(model->loading());
    QVERIFY(!model->complete());
    QCOMPARE(model->rowCount(), 0);

    transport->injectBytes(
        QByteArrayLiteral(":server 321 omairc Channel :Users  Name\r\n"
                          ":server 322 omairc #linux 42 :Kernel discussion\r\n"
                          ":server 323 omairc :End of /LIST\r\n"));
    QVERIFY(model->complete());
    QVERIFY(!model->loading());
    QCOMPARE(model->sourceCount(), 1);
    QCOMPARE(model->rowCount(), 1);
    QCOMPARE(model->field(0, QStringLiteral("channel")).toString(),
             QStringLiteral("#linux"));
    QCOMPARE(model->field(0, QStringLiteral("users")).toInt(), 42);

    QVERIFY(controller.joinListedChannel(QStringLiteral("#linux")));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("JOIN #linux\r\n"));
}

void CommandTest::listIdleTimeoutLateEndAfterRetryStartDoesNotComplete()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(), transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    welcome(transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    controller.setChannelListIdleTimeoutMs(30);
    QVERIFY(controller.sendMessage(QStringLiteral("/list")));
    auto *model = qobject_cast<ChannelListModel *>(controller.channelList());
    QVERIFY(model);
    QVERIFY(model->loading());
    QTest::qWait(200);
    QVERIFY(!model->loading());
    QVERIFY(!model->complete());
    QVERIFY(model->error().contains(QStringLiteral("timed out")));

    const int framesAfterTimeout = transport->writtenFrames().size();
    QVERIFY(controller.sendMessage(QStringLiteral("/list")));
    QCOMPARE(transport->writtenFrames().size(), framesAfterTimeout + 1);
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("LIST\r\n"));
    QVERIFY(model->loading());
    QVERIFY(model->error().isEmpty());

    transport->injectBytes(
        QByteArrayLiteral(":server 321 omairc Channel :Users  Name\r\n"));
    QVERIFY(model->loading());
    QVERIFY(!model->complete());

    transport->injectBytes(
        QByteArrayLiteral(":server 323 omairc :End of /LIST\r\n"));
    QVERIFY(model->loading());
    QVERIFY(!model->complete());
    QCOMPARE(model->rowCount(), 0);

    transport->injectBytes(
        QByteArrayLiteral(":server 322 omairc #linux 42 :Kernel discussion\r\n"
                          ":server 323 omairc :End of /LIST\r\n"));
    QVERIFY(model->complete());
    QVERIFY(!model->loading());
    QCOMPARE(model->sourceCount(), 1);
    QCOMPARE(model->rowCount(), 1);
    QCOMPARE(model->field(0, QStringLiteral("channel")).toString(),
             QStringLiteral("#linux"));
    QCOMPARE(model->field(0, QStringLiteral("users")).toInt(), 42);
}

void CommandTest::listLoadingPresentsOwnNetwork()
{
    IrcController controller;
    auto *transportA = new FakeIrcTransport;
    auto *transportB = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("libera")), transportA));
    QVERIFY(controller.addSession(config(QStringLiteral("oftc")), transportB));
    QVERIFY(controller.start(QStringLiteral("libera")));
    QVERIFY(controller.start(QStringLiteral("oftc")));
    welcome(transportA);
    welcome(transportB);
    transportA->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    transportB->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#lab\r\n"));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("/list")));
    QCOMPARE(transportA->writtenFrames().last(), QByteArrayLiteral("LIST\r\n"));
    auto *model = qobject_cast<ChannelListModel *>(controller.channelList());
    QVERIFY(model);
    QVERIFY(model->loading());
    QCOMPARE(model->networkId(), QStringLiteral("libera"));

    controller.selectConversation(QStringLiteral("oftc"), QStringLiteral("#lab"));
    QVERIFY(controller.sendMessage(QStringLiteral("/list")));
    QCOMPARE(transportB->writtenFrames().last(), QByteArrayLiteral("LIST\r\n"));
    transportB->injectBytes(
        QByteArrayLiteral(":server 322 omairc #lab 7 :Lab\r\n"
                          ":server 323 omairc :End of /LIST\r\n"));
    QVERIFY(model->complete());
    QCOMPARE(model->networkId(), QStringLiteral("oftc"));
    QCOMPARE(model->field(0, QStringLiteral("channel")).toString(),
             QStringLiteral("#lab"));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("/list")));
    QVERIFY(model->loading());
    QCOMPARE(model->networkId(), QStringLiteral("libera"));
    QCOMPARE(model->rowCount(), 0);
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QCOMPARE(transportA->writtenFrames().last(), QByteArrayLiteral("LIST\r\n"));

    transportA->injectBytes(
        QByteArrayLiteral(":server 322 omairc #linux 42 :Kernel discussion\r\n"
                          ":server 323 omairc :End of /LIST\r\n"));
    QVERIFY(model->complete());
    QCOMPARE(model->networkId(), QStringLiteral("libera"));
    QCOMPARE(model->field(0, QStringLiteral("channel")).toString(),
             QStringLiteral("#linux"));
    QVERIFY(controller.joinListedChannel(QStringLiteral("#linux")));
    QCOMPARE(transportA->writtenFrames().last(), QByteArrayLiteral("JOIN #linux\r\n"));
    QVERIFY(transportB->writtenFrames().last() != QByteArrayLiteral("JOIN #linux\r\n"));
}

void CommandTest::slashProjectClosed()
{
    const QStringList closedInputs = {
        QString(),
        QStringLiteral("/"),
        QStringLiteral("/ "),
        QStringLiteral("//hi"),
        QStringLiteral("///x"),
        QStringLiteral("/join #omarchy"),
        QStringLiteral("hello"),
    };
    for (const QString& input : closedInputs) {
        QVERIFY2(!IrcSlashComplete::project(input, IrcComposerSurface::Conversation).isOpen(),
                 qPrintable(input));
    }

    const auto doubled = IrcSlashComplete::project(
        QStringLiteral("//join"), IrcComposerSurface::Conversation);
    QVERIFY(!doubled.isOpen());
    QCOMPARE(IrcCommand::parse(QStringLiteral("//join")).verb, IrcCommand::Verb::Say);
    QCOMPARE(IrcCommand::parse(QStringLiteral("//hi")).verb, IrcCommand::Verb::Say);
    QCOMPARE(IrcCommand::parse(QStringLiteral("///x")).verb, IrcCommand::Verb::Say);
}

void CommandTest::slashProjectOpen()
{
    const auto join = IrcSlashComplete::project(
        QStringLiteral("/jo"), IrcComposerSurface::Conversation);
    QVERIFY(join.isOpen());
    QCOMPARE(join.hits().first().label, QStringLiteral("/join"));
    QCOMPARE(join.hits().first().usage, QStringLiteral("/join [channel] [key][, ...]"));

    const auto alias = IrcSlashComplete::project(
        QStringLiteral("/j"), IrcComposerSurface::Conversation);
    QVERIFY(alias.isOpen());
    QCOMPARE(alias.hits().first().label, QStringLiteral("/join"));

    const auto meStatus = IrcSlashComplete::project(
        QStringLiteral("/me"), IrcComposerSurface::Status);
    QVERIFY(meStatus.isOpen());
    QCOMPARE(meStatus.hits().first().label, QStringLiteral("/mode"));
    QVERIFY(!meStatus.containsLabel(QStringLiteral("/me")));
    const auto meConversation = IrcSlashComplete::project(
        QStringLiteral("/me"), IrcComposerSurface::Conversation);
    QVERIFY(meConversation.isOpen());
    QCOMPARE(meConversation.hits().first().label, QStringLiteral("/me"));

    const auto leave = IrcSlashComplete::project(
        QStringLiteral("/LEAVE"), IrcComposerSurface::Conversation);
    QVERIFY(leave.isOpen());
    QCOMPARE(leave.hits().first().label, QStringLiteral("/part"));

    const auto list = IrcSlashComplete::project(
        QStringLiteral("/li"), IrcComposerSurface::Conversation);
    QVERIFY(list.containsLabel(QStringLiteral("/list")));

    const auto msg = IrcSlashComplete::project(
        QStringLiteral("/msg"), IrcComposerSurface::Conversation);
    QVERIFY(msg.isOpen());
    QCOMPARE(msg.hits().first().label, QStringLiteral("/msg"));
    QCOMPARE(msg.hits().first().usage, QStringLiteral("/msg <nick> <text>"));

    const auto statusTopic = IrcSlashComplete::project(
        QStringLiteral("/t"), IrcComposerSurface::Status);
    QVERIFY(!statusTopic.containsLabel(QStringLiteral("/topic")));
    QVERIFY(statusTopic.isOpen());
    QCOMPARE(statusTopic.hits().first().label, QStringLiteral("/time"));
    const auto conversationTopic = IrcSlashComplete::project(
        QStringLiteral("/t"), IrcComposerSurface::Conversation);
    QVERIFY(conversationTopic.isOpen());
    QCOMPARE(conversationTopic.hits().first().label, QStringLiteral("/topic"));
    QVERIFY(conversationTopic.containsLabel(QStringLiteral("/time")));
    QVERIFY(!conversationTopic.containsLabel(QStringLiteral("/notice")));

    const auto ping = IrcSlashComplete::project(
        QStringLiteral("/pi"), IrcComposerSurface::Conversation);
    QVERIFY(ping.isOpen());
    QCOMPARE(ping.hits().first().label, QStringLiteral("/ping"));
    QCOMPARE(ping.hits().first().usage, QStringLiteral("/ping [nick]"));

    const auto ignore = IrcSlashComplete::project(
        QStringLiteral("/ig"), IrcComposerSurface::Status);
    QVERIFY(ignore.isOpen());
    QCOMPARE(ignore.hits().first().label, QStringLiteral("/ignore"));
    QVERIFY(ignore.containsLabel(QStringLiteral("/ignored")));

    const auto monitor = IrcSlashComplete::project(
        QStringLiteral("/mon"), IrcComposerSurface::Status);
    QVERIFY(monitor.isOpen());
    QCOMPARE(monitor.hits().first().label, QStringLiteral("/monitor"));
    QVERIFY(monitor.containsLabel(QStringLiteral("/monitored")));

    const auto mute = IrcSlashComplete::project(
        QStringLiteral("/mu"), IrcComposerSurface::Status);
    QVERIFY(mute.isOpen());
    QCOMPARE(mute.hits().first().label, QStringLiteral("/mute"));
    QVERIFY(mute.containsLabel(QStringLiteral("/muted")));

    const auto highlight = IrcSlashComplete::project(
        QStringLiteral("/high"), IrcComposerSurface::Status);
    QVERIFY(highlight.isOpen());
    QCOMPARE(highlight.hits().first().label, QStringLiteral("/highlight"));
    QVERIFY(highlight.containsLabel(QStringLiteral("/highlights")));

    const auto autoaway = IrcSlashComplete::project(
        QStringLiteral("/auto"), IrcComposerSurface::Status);
    QVERIFY(autoaway.isOpen());
    QVERIFY(autoaway.containsLabel(QStringLiteral("/autoaway")));

    const auto invite = IrcSlashComplete::project(
        QStringLiteral("/inv"), IrcComposerSurface::Conversation);
    QVERIFY(invite.isOpen());
    QVERIFY(invite.containsLabel(QStringLiteral("/invite")));
    QCOMPARE(invite.hits().first().label, QStringLiteral("/invite"));

    const auto disconnect = IrcSlashComplete::project(
        QStringLiteral("/quit"), IrcComposerSurface::Conversation);
    QVERIFY(disconnect.isOpen());
    QCOMPARE(disconnect.hits().first().label, QStringLiteral("/disconnect"));
    QCOMPARE(disconnect.hits().first().usage, QStringLiteral("/disconnect [reason]"));

    const auto queryPrefix = IrcSlashComplete::project(
        QStringLiteral("/q"), IrcComposerSurface::Conversation);
    QVERIFY(queryPrefix.isOpen());
    QCOMPARE(queryPrefix.hits().first().label, QStringLiteral("/query"));
    QVERIFY(!queryPrefix.containsLabel(QStringLiteral("/disconnect")));

    const auto opConversation = IrcSlashComplete::project(
        QStringLiteral("/o"), IrcComposerSurface::Conversation);
    QVERIFY(opConversation.isOpen());
    QVERIFY(opConversation.containsLabel(QStringLiteral("/op")));
    QCOMPARE(opConversation.hits().first().label, QStringLiteral("/op"));
    const auto opStatus = IrcSlashComplete::project(
        QStringLiteral("/o"), IrcComposerSurface::Status);
    QVERIFY(!opStatus.containsLabel(QStringLiteral("/op")));
    QVERIFY(!opStatus.isOpen());
}

void CommandTest::slashSessionKeys()
{
    IrcSlashSession session;

    session.sync(QStringLiteral("/j"), false);
    QVERIFY(session.open());
    QCOMPARE(session.selectedIndex(), 0);
    const auto tab = session.routeKey(int(Qt::Key_Tab), int(Qt::NoModifier));
    QVERIFY(tab.accepted);
    QCOMPARE(tab.insertion, QStringLiteral("/join "));
    session.sync(tab.insertion, false);
    QVERIFY(!session.open());

    session.sync(QStringLiteral("/close"), false);
    QVERIFY(session.open());
    const auto enterClose = session.routeKey(int(Qt::Key_Return), int(Qt::NoModifier));
    QVERIFY(!enterClose.accepted);
    QVERIFY(enterClose.insertion.isEmpty());

    session.sync(QStringLiteral("/j"), false);
    QVERIFY(session.open());
    const auto enterAlias = session.routeKey(int(Qt::Key_Enter), int(Qt::NoModifier));
    QVERIFY(!enterAlias.accepted);

    session.sync(QStringLiteral("/jo"), false);
    QVERIFY(session.open());
    const auto enterPrefix = session.routeKey(int(Qt::Key_Return), int(Qt::NoModifier));
    QVERIFY(enterPrefix.accepted);
    QCOMPARE(enterPrefix.insertion, QStringLiteral("/join "));

    session.sync(QStringLiteral("  /jo"), false);
    QVERIFY(session.open());
    QCOMPARE(session.activate(0), QStringLiteral("  /join "));

    session.sync(QStringLiteral("/jo"), false);
    QVERIFY(session.open());
    const auto escape = session.routeKey(int(Qt::Key_Escape), int(Qt::NoModifier));
    QVERIFY(escape.accepted);
    QVERIFY(!session.open());
    session.sync(QStringLiteral("/jo"), false);
    QVERIFY(!session.open());
    session.sync(QStringLiteral("/j"), false);
    QVERIFY(session.open());
    session.sync(QStringLiteral("/"), false);
    QVERIFY(!session.open());
    session.sync(QStringLiteral("/jo"), false);
    QVERIFY(session.open());

    session.sync(QStringLiteral("//"), false);
    QVERIFY(!session.open());
    const auto doubled = session.routeKey(int(Qt::Key_Tab), int(Qt::NoModifier));
    QVERIFY(!doubled.accepted);
    QVERIFY(doubled.insertion.isEmpty());
}

int runCommandTests(int argc, char **argv)
{
    CommandTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_command.moc"
