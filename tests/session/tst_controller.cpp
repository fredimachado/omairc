#include <QAbstractItemModel>
#include <QCoreApplication>
#include <QDateTime>
#include <QDir>
#include <QDirIterator>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonObject>
#include <QSettings>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QTest>
#include <QVariantMap>

#include <memory>
#include <time.h>

#include "fakeirctransport.h"
#include "testsettings.h"
#include "irccapability.h"
#include "ircconversationlog.h"
#include "irccontroller.h"
#include "ircopendirect.h"
#include "ircplaybacktime.h"
#include "ircmessage.h"
#include "memberlistmodel.h"
#include "messagelistmodel.h"
#include "networklogmodel.h"

class FakeReconnectTimer : public IrcReconnectTimer
{
public:
    using IrcReconnectTimer::IrcReconnectTimer;

    void start(int delayMilliseconds) override
    {
        active = true;
        delays.append(delayMilliseconds);
    }

    void cancel() override
    {
        active = false;
        ++cancelCount;
    }

    void fire()
    {
        if (!active)
            return;
        active = false;
        emit fired();
    }

    QList<int> delays;
    bool active = false;
    int cancelCount = 0;
};

namespace
{
class ScopedTimeZone
{
public:
    explicit ScopedTimeZone(const char *zone)
        : m_hadTz(qEnvironmentVariableIsSet("TZ"))
        , m_previous(qgetenv("TZ"))
    {
        qputenv("TZ", zone);
        tzset();
    }

    ~ScopedTimeZone()
    {
        if (m_hadTz)
            qputenv("TZ", m_previous);
        else
            qunsetenv("TZ");
        tzset();
    }

private:
    bool m_hadTz;
    QByteArray m_previous;
};

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

int rowForTarget(const QAbstractItemModel *model, const QString& target)
{
    for (int row = 0; row < model->rowCount(); ++row) {
        if (roleAt(model, row, ConversationListModel::ConversationRole) == target)
            return row;
    }
    return -1;
}

int rowForNetworkTarget(const QAbstractItemModel *model,
                        const QString& networkId,
                        const QString& target)
{
    for (int row = 0; row < model->rowCount(); ++row) {
        if (roleAt(model, row, ConversationListModel::NetworkIdRole) == networkId
            && roleAt(model, row, ConversationListModel::ConversationRole) == target)
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

int frameCount(const QByteArrayList& frames, const QByteArray& frame)
{
    int count = 0;
    for (const QByteArray& written : frames) {
        if (written == frame)
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

bool logHasLabel(QAbstractItemModel *lines, const QString& label)
{
    for (int row = 0; row < lines->rowCount(); ++row) {
        if (lines->data(lines->index(row, 0), NetworkLogModel::LabelRole).toString()
            == label)
            return true;
    }
    return false;
}

bool logHasLabeledText(QAbstractItemModel *lines,
                       const QString& label,
                       const QString& text)
{
    for (int row = 0; row < lines->rowCount(); ++row) {
        if (lines->data(lines->index(row, 0), NetworkLogModel::LabelRole).toString()
                == label
            && lines->data(lines->index(row, 0), NetworkLogModel::TextRole).toString()
                == text)
            return true;
    }
    return false;
}

QStringList logTexts(QAbstractItemModel *lines)
{
    QStringList texts;
    if (!lines)
        return texts;
    for (int row = 0; row < lines->rowCount(); ++row) {
        texts.append(
            lines->data(lines->index(row, 0), NetworkLogModel::TextRole).toString());
    }
    return texts;
}

QByteArray aliceMetadataWelcome()
{
    return QByteArrayLiteral(
        ":server CAP omairc LS :away-notify batch draft/metadata-2\r\n"
        ":server CAP omairc ACK :away-notify batch draft/metadata-2\r\n"
        ":server 001 omairc :Welcome\r\n"
        ":omairc!u@h JOIN :#omarchy\r\n"
        ":server 353 omairc = #omarchy :@omairc Alice\r\n"
        ":server 366 omairc #omarchy :End of NAMES\r\n"
        ":server 761 omairc Alice display-name * :Alice Docs\r\n"
        ":server 761 omairc Alice pronouns * :she/her\r\n"
        ":server 761 omairc Alice status * :writing docs\r\n"
        ":server 761 omairc Alice bot * :PacketBot\r\n"
        ":server 761 omairc Alice homepage * :https://example.com/alice\r\n"
        ":server 761 omairc Alice color * :#aabbcc\r\n"
        ":server 761 omairc Alice avatar * :https://example.com/alice.png\r\n");
}

QStringList aliceWhoisMetadataLines()
{
    return {
        QStringLiteral("Alice is also known as Alice Docs"),
        QStringLiteral("Alice pronouns she/her"),
        QStringLiteral("Alice status writing docs"),
        QStringLiteral("Alice is a bot (PacketBot)"),
        QStringLiteral("Alice homepage https://example.com/alice"),
        QStringLiteral("Alice color #aabbcc"),
        QStringLiteral("Alice avatar https://example.com/alice.png"),
    };
}

bool selectedBodiesContain(QAbstractItemModel *messages, const QString& needle)
{
    if (!messages)
        return false;
    for (int row = 0; row < messages->rowCount(); ++row) {
        if (roleAt(messages, row, MessageListModel::BodyRole).toString().contains(needle))
            return true;
    }
    return false;
}

QStringList selectedBodies(const QAbstractItemModel *messages)
{
    QStringList bodies;
    if (!messages)
        return bodies;
    for (int row = 0; row < messages->rowCount(); ++row)
        bodies.append(roleAt(messages, row, MessageListModel::BodyRole).toString());
    return bodies;
}

QStringList messageBodies(const QAbstractItemModel *messages)
{
    QStringList bodies;
    if (!messages)
        return bodies;
    for (int row = 0; row < messages->rowCount(); ++row) {
        if (roleAt(messages, row, MessageListModel::KindRole).toString()
            != QLatin1String("message")) {
            continue;
        }
        bodies.append(roleAt(messages, row, MessageListModel::BodyRole).toString());
    }
    return bodies;
}

QStringList transcriptBodies(const QAbstractItemModel *messages)
{
    QStringList bodies;
    if (!messages)
        return bodies;
    for (int row = 0; row < messages->rowCount(); ++row) {
        const QString kind =
            roleAt(messages, row, MessageListModel::KindRole).toString();
        const QString body =
            roleAt(messages, row, MessageListModel::BodyRole).toString();
        if (kind == QLatin1String("message") || body.endsWith(QStringLiteral(" joined")))
            bodies.append(body);
    }
    return bodies;
}

int bodyRow(const QAbstractItemModel *messages, const QString& body)
{
    if (!messages)
        return -1;
    for (int row = 0; row < messages->rowCount(); ++row) {
        if (roleAt(messages, row, MessageListModel::BodyRole).toString() == body)
            return row;
    }
    return -1;
}

bool hasEventBody(const QAbstractItemModel *messages, const QString& body)
{
    const int row = bodyRow(messages, body);
    return row >= 0
        && roleAt(messages, row, MessageListModel::KindRole).toString()
            == QStringLiteral("event");
}

bool hasWhoisBody(const QAbstractItemModel *messages, const QString& body)
{
    const int row = bodyRow(messages, body);
    return row >= 0
        && roleAt(messages, row, MessageListModel::KindRole).toString()
            == QStringLiteral("whois");
}

QByteArray namesBurst(int nickCount, int perLine)
{
    QByteArray bytes = QByteArrayLiteral(":omairc!u@h JOIN :#big\r\n");
    QStringList batch;
    batch.reserve(perLine);
    for (int i = 0; i < nickCount; ++i) {
        batch.append(QStringLiteral("n%1").arg(i, 4, 10, QLatin1Char('0')));
        if (batch.size() == perLine) {
            bytes += QByteArrayLiteral(":server 353 omairc = #big :");
            bytes += batch.join(QLatin1Char(' ')).toUtf8();
            bytes += QByteArrayLiteral("\r\n");
            batch.clear();
        }
    }
    if (!batch.isEmpty()) {
        bytes += QByteArrayLiteral(":server 353 omairc = #big :");
        bytes += batch.join(QLatin1Char(' ')).toUtf8();
        bytes += QByteArrayLiteral("\r\n");
    }
    bytes += QByteArrayLiteral(":server 366 omairc #big :End of NAMES\r\n");
    return bytes;
}

class ScopedTranscriptRoot
{
public:
    explicit ScopedTranscriptRoot(const QString& root)
        : m_had(qEnvironmentVariableIsSet("OMAIRC_TRANSCRIPT_ROOT"))
        , m_previous(qgetenv("OMAIRC_TRANSCRIPT_ROOT"))
    {
        qputenv("OMAIRC_TRANSCRIPT_ROOT", root.toUtf8());
    }

    ~ScopedTranscriptRoot()
    {
        if (m_had)
            qputenv("OMAIRC_TRANSCRIPT_ROOT", m_previous);
        else
            qunsetenv("OMAIRC_TRANSCRIPT_ROOT");
    }

private:
    bool m_had;
    QByteArray m_previous;
};

bool treeContains(const QString& root, const QString& needle)
{
    QDirIterator it(root, QDir::Files, QDirIterator::Subdirectories);
    while (it.hasNext()) {
        const QString path = it.next();
        QFile file(path);
        if (!file.open(QIODevice::ReadOnly | QIODevice::Text))
            continue;
        if (QString::fromUtf8(file.readAll()).contains(needle))
            return true;
    }
    return false;
}

QStringList jsonlBodies(const QString& path)
{
    QStringList bodies;
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly | QIODevice::Text))
        return bodies;
    while (!file.atEnd()) {
        const QJsonDocument document = QJsonDocument::fromJson(file.readLine());
        if (!document.isObject())
            continue;
        bodies.append(document.object().value(QStringLiteral("body")).toString());
    }
    return bodies;
}

QByteArray whoBurst(int nickCount)
{
    QByteArray bytes;
    for (int i = 0; i < nickCount; ++i) {
        bytes += QByteArrayLiteral(":server 352 omairc #big u h s ");
        bytes += QStringLiteral("n%1").arg(i, 4, 10, QLatin1Char('0')).toUtf8();
        bytes += QByteArrayLiteral(" G :0 r\r\n");
    }
    bytes += QByteArrayLiteral(":server 315 omairc #big :End of WHO\r\n");
    return bytes;
}

QByteArray previousNickPlaybackBatch(bool includeAfter)
{
    QByteArray batch = QByteArrayLiteral(
        ":znc.in BATCH +q znc.in/playback lena\r\n"
        "@batch=q;time=2024-03-09T16:00:00.100Z "
        ":lena!u@h PRIVMSG oldnick :before\r\n"
        "@batch=q;time=2024-03-09T16:00:00.620Z "
        ":oldnick!u@h PRIVMSG lena :mine\r\n");
    if (includeAfter) {
        batch += QByteArrayLiteral(
            "@batch=q;time=2024-03-09T16:00:01.500Z "
            ":lena!u@h PRIVMSG omairc :after\r\n");
    }
    batch += QByteArrayLiteral(
        "@batch=q;time=2024-03-09T16:00:02.000Z "
        ":stranger!u@h PRIVMSG oldnick :elsewhere\r\n"
        "@batch=q;time=2024-03-09T16:00:04.000Z "
        ":lena!u@h PRIVMSG #other :channel\r\n"
        ":znc.in BATCH -q\r\n");
    return batch;
}
}

class ControllerTest : public QObject
{
    Q_OBJECT

private slots:
    void init();
    void reducesTrafficAndRoutesOutboundByNetwork();
    void liberaConnectCreatesChannelNotAuthDirect();
    void incomingNoticeStaysOnStatus();
    void statusKeepListLeavesTranscriptIntact();
    void incomingActionUsesActionKindAndStripsCtcp();
    void emptyNetworkIdDoesNotSwitch();
    void presenceCapabilitiesGateAwayAndStatus();
    void peerMetadataEpochBumpsOnInboundMetadata();
    void defaultPrefixPaintsLabelNotNick();
    void channelCloseSlashIsWrongScope();
    void partDefaultsToSelectedChannel();
    void failedJoin448PartDismissesWithoutPart();
    void failedInviteJoinPartDismissesWithoutPart();
    void joinedPartSendsAndDropsSelected();
    void partMissingChannelStillSends();
    void partNonSelectedUnjoinedDropsWithoutPart();
    void partUnjoinedDropsMute();
    void partUnjoinedDelayedJoinSendsPart();
    void rejoinClearsCancelledPendingJoin();
    void partNonSelectedUnjoinedDelayedJoinKeepsSelection();
    void partFromDirectIsWrongScope();
    void kickDefaultsToSelectedChannel();
    void kickFromDirectIsWrongScope();
    void incomingInviteStaysOnStatusWithoutConversation();
    void emptyJoinAcceptsLatestInvite();
    void emptyJoinFromStatusUsesPendingInvite();
    void joinOtherDoesNotConsumePendingInvite();
    void joinOpensLastNamedChannel();
    void failedInviteJoinKeepsPending();
    void sessionDropClearsPendingInvite();
    void inviteDefaultsToSelectedChannel();
    void inviteFromDirectAndStatusNeedsChannel();
    void implicitStatusInviteStaysOnFocusedNetwork();
    void statusPartDefaultsToSelectedChannel();
    void partImplicitUsesSelectedSession();
    void topicUsesSelectedSession();
    void topicFromDirectIsWrongScope();
    void emptyTopicDoesNotWrite();
    void closeDirectMessageDropsAndSelectsNeighbor();
    void closeDirectMessageInvokableOnChannelIsSilent();
    void closeDirectMessageWhileDisconnected();
    void clearingLastDirectNotifiesConnectionStatus();
    void selectingOfflineNetworkNotifiesConnectionStatus();
    void openStatusWithoutSessionNotifiesConnectionStatus();
    void selfAwayFollowsNumericsAndUnawaysAfterChat();
    void selfAwayRefreshesOurOwnMemberRow();
    void closeLastDirectKeepsSelfAway();
    void queryOpensDirectWithoutPrivmsg();
    void queryAliceCreatesDirectRowWithoutPrivmsg();
    void queryWithTextSendsPrivmsg();
    void queryChannelAndEmptyAreRefused();
    void noticeSendsWithoutSelecting();
    void noticeEchoesExistingDirect();
    void noticeEchoesSelectedChannel();
    void noticeMissingTokensAreRefused();
    void statusNoticeStaysOpen();
    void statusNoticeMissingBodyStaysOpen();
    void disconnectedNoticeIsNotConnected();
    void noticeDoesNotUnaway();
    void msgSendsWithoutSelecting();
    void msgEchoesExistingDirect();
    void msgDoesNotFocusExistingDirect();
    void msgEchoesSelectedGhostQuery();
    void msgMissingTokensAreRefused();
    void msgChannelIsRefused();
    void statusMsgStaysOpen();
    void disconnectedMsgIsNotConnected();
    void msgClearsAway();
    void explicitChatClearsAwayOnOriginNetwork();
    void lastErrorIsolatedByNetwork();
    void closeDirectMessageClearsOriginNetworkError();
    void awayWaitsForNumericThenChatUnaways();
    void statusQueryClosesStatus();
    void statusQueryChannelStaysOpen();
    void statusSubmitLastAcceptedFollowsOutcome();
    void disconnectedQuerySelectsBareNotText();
    void statusQueryWithNoNetworkDoesNotCrash();
    void conversationClearWipesMessages();
    void ghostClearIsSent();
    void statusClearLeavesConversationMessages();
    void selectedPrivmsgInsertsMessageRow();
    void largeChannelJoinDoesNotResetModelsPerNick();
    void isupportBurstDoesNotResetModels();
    void otherChannelNamesDoesNotSnapshotJoiningMembers();
    void namesBurstFlushesTypingClearedByChat();
    void chatDuringNamesUpdatesMessagesWithoutMemberReset();
    void incomingNickRetargetsSelectedDirect();
    void incomingNickCaseOnlyRetargetsDirect();
    void welcomeAssignedNickRoutesDirectMessages();
    void echoIfPresentUsesAssignedNick();
    void echoMessageAckSkipsLocalPrivmsg();
    void echoMessageAbsentStillEchoesLocally();
    void echoMessageAckSkipsMsgEcho();
    void longMeAndNoticeSplitAcrossFrames();
    void longTopicStillRefuses();
    void echoMessageLongPrivmsgShowsEachChunkOnce();
    void msgEchoDoesNotOpenMissingDirect();
    void incomingNickservPrivmsgDoesNotOpenDirect();
    void bouncerAttachOpensOnlyPeerAuthoredDirects();
    void keptSelfReplayRemembersOpenQuery();
    void bouncerQueryReplayDirectAppearsInConversationModel();
    void pseudoClientPrivmsgOpensNoConversation();
    void zncQuietSendDoesNotOpenDirect();
    void pseudoClientReplayBatchOpensNoConversation();
    void routableNicksOpenDirectsAndPseudoClientsDoNot();
    void conversationCreateMatrix();
    void statusMsgNickservIdentifyDoesNotOpenDirect();
    void nsIdentifyDoesNotOpenDirectOrLeakSecret();
    void automaticIdentifyDoesNotOpenNickServDirect();
    void mentionArrivedOnSelectedBuffer();
    void mentionArrivedOnDirectMessageWithoutNick();
    void mentionArrivedCarriesNetworkTargetAndMsgid();
    void revealConversationSelectsExistingAndRecreatesClosedDirect();
    void chghostLeavesMemberNickAndRanks();
    void twoSessionsStartTogether();
    void startingBackgroundNetworkDoesNotStealStatus();
    void quitWhileReconnecting();
    void disconnectLeavesOtherNetworkLive();
    void quitAliasIdlesFocusedNetwork();
    void disconnectWhileIdleIsRefused();
    void statusJoinUsesConsoleNetwork();
    void implicitStatusPartStaysOnFocusedNetwork();
    void implicitStatusKickStaysOnFocusedNetwork();
    void selectConversationByIdUsesCompositeKey();
    void networkIconUrlComesFromIsupport();
    void networkIconUrlClearsWhenReconnectOmitsDraftIcon();
    void forgetNetworkDropsGhostRowsAndLog();
    void backgroundChatBumpsConversationEpoch();
    void backgroundPlaybackBumpsUnreadAndMention();
    void focusedChannelPlaybackPlantsUnreadMark();
    void chatHistoryBatchShowsBodyAndTime();
    void chatHistoryAndLiveTimeUseLocalWallClock();
    void whoisFromChannelCopiesStatusLinesAsEvents();
    void whoisFromChannelIncludesStoredMetadata();
    void statusWhoisIncludesStoredMetadataNotTranscript();
    void failedWhoisDoesNotEmitStoredMetadata();
    void whoisInterleavesByAskingBuffer();
    void statusWhoisSupersedesConversationWatch();
    void unsolicitedWhoisStaysOnStatus();
    void emptyChannelWhoisNamesANick();
    void emptyDirectWhoisDefaultsAndRoutes();
    void terminalWhoisClearsWatch();
    void failedPrivmsgDoesNotStealWhoisWatch();
    void whoisFailureBeforeDeliveryDoesNotStealWatch();
    void closedDirectWhoisDoesNotResurrect();
    void whoisEventDoesNotCollapseWithJoin();
    void ctcpFromChannelCopiesReplyAsWhoisEvent();
    void ctcpReplyDuringSendCopiesIntoAskingTranscript();
    void statusCtcpStaysOnStatus();
    void emptyChannelCtcpNamesANick();
    void emptyDirectCtcpDefaultsAndRoutes();
    void unsolicitedCtcpReplyStaysOnStatus();
    void transcriptPersistsAndReloadsMuted();
    void transcriptHydrateDoesNotNotify();
    void transcriptSkipsSecretsAndKeepsSessionOnWriteError();
    void transcriptClearLeavesFile();
    void zncPlaybackPlayUsesStoredServerTime();
    void zncPlaybackPlaysUnstampedAutojoinAndLaterJoin();
    void zncPlaybackJoinRetriesUntilBatchIsKept();
    void zncPlaybackEmptyStoreWithAutojoinPlaysWildcard();
    void zncPlaybackRestoredDirectWithoutStamp();
    void playbackBatchOverCeilingKeepsNewestLines();
    void queryPlaybackPeerBatchFollowsHeldSelfLines();
    void queryPlaybackKeepsLinesFromPreviousNick();
    void queryPlaybackPreviousNickEchoIsOwnLine();
    void networkWithoutPlaybackCapDoesNotPlay();
    void zncPlaybackLateCapPlaysOnce();
    void engagedQueryPlaybackPlaysAfterRestore();
    void selfOnlyStoredQueryPlaybackLandsAfterMotd();
    void selfOnlyUnstoredQueryPlaybackDoesNotAdvanceClock();
    void playbackReplaySkipsLinesAlreadyInTheTranscript();
    void preJoinChannelPlaybackSplicesAboveSelfJoin();
    void heldPlaybackBatchesSpliceInOrder();
    void preJoinChannelPlaybackWithoutJoinDoesNotAdvanceClock();
    void preJoinPlaybackLeavesAnchorForChatHistory();
    void openChannelPlaybackBeforeRejoinSplicesAboveNewJoin();
    void joinedChannelPlaybackSplicesAfterAnchorIsGone();
    void selfEchoWithoutConversationDoesNotAdvancePlaybackClock();
    void emptyChatHistoryLeavesAnchorForPlayback();
    void ephemeralControllerSkipsPlaybackSettings();
    void playbackTimeStoreRoundTripsNewestAndRekey();
    void forgetNetworkDropsPlaybackTimes();
    void zncPlaybackBeforeFirstPlayUsesEmptySnapshot();
    void zncPlaybackBeforeFirstPlayKeepsSavedStamp();
    void zncPlaybackDiscoversUnknownOfflineDirect();
    void zncPlaybackWildcardSkipsStampedClearedDirect();
    void channelPlaybackPreviousNickDoesNotBumpUnread();

private:
    std::unique_ptr<QTemporaryDir> m_settingsDir;
};

void ControllerTest::init()
{
    m_settingsDir = std::make_unique<QTemporaryDir>();
    QVERIFY(m_settingsDir->isValid());
    TestSettings::isolate(m_settingsDir->path());
    QCoreApplication::setOrganizationName(QStringLiteral("omairc"));
    QCoreApplication::setApplicationName(QStringLiteral("omairc"));
}

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
    QCOMPARE(roleAt(members, 0, MemberListModel::NickRole), QStringLiteral("omairc"));
    QCOMPARE(roleAt(members, 0, MemberListModel::LabelRole), QStringLiteral("@omairc"));
    QCOMPARE(roleAt(members, 1, MemberListModel::NickRole), QStringLiteral("Alice"));
    QCOMPARE(roleAt(members, 1, MemberListModel::LabelRole), QStringLiteral("+Alice"));
    QCOMPARE(roleAt(members, 2, MemberListModel::NickRole), QStringLiteral("Bob"));
    QCOMPARE(roleAt(members, 2, MemberListModel::LabelRole), QStringLiteral("Bob"));
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

    transportA->injectBytes(
        QByteArrayLiteral(":op!u@h MODE #chan +o Alice\r\n"
                          ":op!u@h MODE #chan -o Alice\r\n"));
    QCOMPARE(roleAt(members, 1, MemberListModel::NickRole), QStringLiteral("Alice"));
    QCOMPARE(roleAt(members, 1, MemberListModel::LabelRole), QStringLiteral("+Alice"));

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

void ControllerTest::liberaConnectCreatesChannelNotAuthDirect()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSessionConfig sessionConfig = config(QStringLiteral("libera"));
    sessionConfig.autojoinChannels = {QStringLiteral("#omarchy")};
    IrcSession *session = controller.addSession(sessionConfig, transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral("NOTICE AUTH :*** Looking up your hostname...\r\n"
                          "NOTICE AUTH :*** Checking Ident\r\n"
                          "NOTICE AUTH :*** No Ident response\r\n"
                          "NOTICE AUTH :*** Found your hostname\r\n"
                          ":copper.libera.chat CAP omairc LS :multi-prefix\r\n"
                          ":copper.libera.chat 001 omairc :Welcome\r\n"
                          ":copper.libera.chat 005 omairc CHANTYPES=# PREFIX=(ov)@+ "
                          "NETWORK=Libera.Chat :are supported by this server\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":copper.libera.chat 353 omairc = #omarchy :@omairc\r\n"
                          ":copper.libera.chat 366 omairc #omarchy :End of NAMES\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(conversations->rowCount(), 1);
    QCOMPARE(roleAt(conversations, 0, ConversationListModel::ConversationRole),
             QStringLiteral("#omarchy"));
    QCOMPARE(roleAt(conversations, 0, ConversationListModel::DirectRole), false);
    QCOMPARE(roleAt(conversations, 0, ConversationListModel::NetworkIdRole),
             QStringLiteral("libera"));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QVERIFY(controller.isChannel());

    auto *lines = controller.console()->lines();
    QVERIFY(lines);
    bool authNotice = false;
    for (int row = 0; row < lines->rowCount(); ++row) {
        const QString text =
            lines->data(lines->index(row, 0), NetworkLogModel::TextRole).toString();
        if (text.contains(QStringLiteral("Looking up your hostname")))
            authNotice = true;
    }
    QVERIFY(authNotice);
}

void ControllerTest::incomingNoticeStaysOnStatus()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSessionConfig sessionConfig = config(QStringLiteral("libera"));
    sessionConfig.autojoinChannels = {QStringLiteral("#omarchy")};
    IrcSession *session = controller.addSession(sessionConfig, transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral("NOTICE AUTH :*** Looking up your hostname...\r\n"
                          ":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":server 005 omairc CHANTYPES=# PREFIX=(ov)@+ "
                          ":are supported by this server\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :@omairc\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"
                          ":NickServ!NickServ@services NOTICE omairc :Please identify\r\n"
                          ":bot!u@h NOTICE #omarchy :heads up\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("NickServ")), -1);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("AUTH")), -1);

    auto *lines = controller.console()->lines();
    QVERIFY(lines);
    QVERIFY(logContains(lines, QStringLiteral("-NickServ-")));
    QVERIFY(logContains(lines, QStringLiteral("-AUTH-")));
    QVERIFY(logContains(lines, QStringLiteral("-bot- heads up")));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    for (int row = 0; row < messages->rowCount(); ++row) {
        const QString body =
            roleAt(messages, row, MessageListModel::BodyRole).toString();
        QVERIFY(!body.contains(QStringLiteral("heads up")));
    }
}

void ControllerTest::statusKeepListLeavesTranscriptIntact()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSessionConfig sessionConfig = config(QStringLiteral("libera"));
    sessionConfig.autojoinChannels = {};
    IrcSession *session = controller.addSession(sessionConfig, transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(
            ":server CAP omairc LS :batch chathistory\r\n"
            ":server CAP omairc ACK :batch chathistory\r\n"
            ":server 001 omairc :Welcome\r\n"
            ":server 372 omairc :- motd line\r\n"
            ":omairc!u@h JOIN :#omarchy\r\n"
            ":alice!u@h JOIN :#omarchy\r\n"
            ":alice!u@h PRIVMSG #omarchy :hello there\r\n"
            "PING :abc\r\n"
            ":server PONG :abc\r\n"
            ":server 353 omairc = #omarchy :@omairc alice\r\n"
            ":server 366 omairc #omarchy :End of NAMES\r\n"
            ":alice!u@h PRIVMSG #omarchy :\x01"
            "ACTION waves\x01\r\n"
            ":NickServ!NickServ@services NOTICE omairc :Please identify\r\n"
            ":irc.host BATCH +hx chathistory #omarchy\r\n"
            "@batch=hx :alice!u@h PRIVMSG #omarchy :older replay\r\n"
            ":irc.host BATCH -hx\r\n"
            ":alice!u@h PART #omarchy :bye\r\n"));

    auto *lines = controller.console()->lines();
    QVERIFY(lines);
    QVERIFY(logContains(lines, QStringLiteral("Welcome")));
    QVERIFY(logContains(lines, QStringLiteral("motd line")));
    QVERIFY(logContains(lines, QStringLiteral("-NickServ- Please identify")));
    QVERIFY(logHasLabel(lines, QStringLiteral("001")));
    QVERIFY(logHasLabel(lines, QStringLiteral("CAP")));
    QVERIFY(logContains(lines, QStringLiteral("Server supports: batch | chathistory")));
    QVERIFY(logHasLabel(lines, QStringLiteral("372")));
    QVERIFY(!logHasLabel(lines, QStringLiteral("PING")));
    QVERIFY(!logHasLabel(lines, QStringLiteral("PONG")));
    QVERIFY(!logHasLabel(lines, QStringLiteral("JOIN")));
    QVERIFY(!logHasLabel(lines, QStringLiteral("PRIVMSG")));
    QVERIFY(!logHasLabel(lines, QStringLiteral("353")));
    QVERIFY(!logHasLabel(lines, QStringLiteral("ACTION")));
    QVERIFY(!logHasLabel(lines, QStringLiteral("CTCP")));
    QVERIFY(!logHasLabel(lines, QStringLiteral("BATCH")));
    QVERIFY(!logContains(lines, QStringLiteral("hello there")));
    QVERIFY(!logContains(lines, QStringLiteral("older replay")));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("hello there")));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("waves")));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("omairc joined")));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("alice joined")));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("alice left")));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("older replay")));
}

void ControllerTest::incomingActionUsesActionKindAndStripsCtcp()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSessionConfig sessionConfig = config(QStringLiteral("libera"));
    sessionConfig.autojoinChannels = {QStringLiteral("#omarchy")};
    IrcSession *session = controller.addSession(sessionConfig, transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":server 005 omairc CHANTYPES=# PREFIX=(ov)@+ "
                          ":are supported by this server\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :@omairc MetaNova\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));
    transport->injectBytes(
        QByteArray(":MetaNova!u@h PRIVMSG #omarchy :\x01"
                   "ACTION feeds jvaztap\x01\r\n"));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(messages->rowCount() > 0);
    const int last = messages->rowCount() - 1;
    QCOMPARE(roleAt(messages, last, MessageListModel::AuthorRole),
             QStringLiteral("MetaNova"));
    QCOMPARE(roleAt(messages, last, MessageListModel::BodyRole),
             QStringLiteral("feeds jvaztap"));
    QCOMPARE(roleAt(messages, last, MessageListModel::KindRole),
             QStringLiteral("action"));
}

void ControllerTest::emptyNetworkIdDoesNotSwitch()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    controller.selectConversation(QString(), QStringLiteral("AUTH"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
}

void ControllerTest::presenceCapabilitiesGateAwayAndStatus()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :away-notify batch draft/metadata-2\r\n"
                          ":server CAP omairc ACK :away-notify batch draft/metadata-2\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":server 005 omairc CHANTYPES=# PREFIX=(ov)@+ "
                          ":are supported by this server\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :@omairc +Alice Bob\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(controller.hasAwayPresence());
    QVERIFY(controller.hasMemberStatus());

    auto *members = qobject_cast<QAbstractItemModel *>(controller.members());
    QCOMPARE(members->rowCount(), 3);
    QCOMPARE(roleAt(members, 0, MemberListModel::NickRole), QStringLiteral("omairc"));
    QCOMPARE(roleAt(members, 0, MemberListModel::LabelRole), QStringLiteral("@omairc"));
    QCOMPARE(roleAt(members, 1, MemberListModel::NickRole), QStringLiteral("Alice"));
    QCOMPARE(roleAt(members, 1, MemberListModel::LabelRole), QStringLiteral("+Alice"));
    QCOMPARE(roleAt(members, 1, MemberListModel::StatusRole), QString());
    QCOMPARE(roleAt(members, 1, MemberListModel::AwayRole), false);

    transport->injectBytes(
        QByteArrayLiteral(":server 352 omairc #omarchy u h server Alice G :0 real\r\n"
                          ":server 761 omairc Alice status * :writing docs\r\n"));
    QCOMPARE(roleAt(members, 1, MemberListModel::AwayRole), true);
    QCOMPARE(roleAt(members, 1, MemberListModel::StatusRole),
             QStringLiteral("writing docs"));

    const QVariantMap alice = controller.peerMetadata(QStringLiteral("libera"),
                                                      QStringLiteral("Alice"));
    QCOMPARE(alice.value(QStringLiteral("status")).toString(),
             QStringLiteral("writing docs"));
    QCOMPARE(alice.value(QStringLiteral("bot")).toBool(), false);
    QVERIFY(alice.value(QStringLiteral("avatar")).toString().isEmpty());

    const QVariantMap empty = controller.peerMetadata(QString(), QString());
    QCOMPARE(empty.value(QStringLiteral("bot")).toBool(), false);
    QCOMPARE(empty.value(QStringLiteral("status")).toString(), QString());
    QCOMPARE(empty.value(QStringLiteral("avatar")).toString(), QString());
    QCOMPARE(empty.value(QStringLiteral("displayName")).toString(), QString());

    transport->injectBytes(QByteArrayLiteral(":Alice!u@h AWAY\r\n"));
    QCOMPARE(roleAt(members, 1, MemberListModel::AwayRole), false);

    transport->injectBytes(QByteArrayLiteral(":server 766 omairc Alice status :no key\r\n"));
    QCOMPARE(roleAt(members, 1, MemberListModel::StatusRole), QString());

    transport->injectBytes(
        QByteArrayLiteral(":server 761 omairc Alice status * :writing docs\r\n"
                          ":Alice!u@h AWAY :lunch\r\n"
                          ":server CAP omairc DEL :away-notify draft/metadata-2\r\n"));
    QVERIFY(!controller.hasAwayPresence());
    QVERIFY(!controller.hasMemberStatus());
    QCOMPARE(roleAt(members, 1, MemberListModel::AwayRole), false);
    QCOMPARE(roleAt(members, 1, MemberListModel::StatusRole), QString());
}

void ControllerTest::peerMetadataEpochBumpsOnInboundMetadata()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch draft/metadata-2\r\n"
                          ":server CAP omairc ACK :batch draft/metadata-2\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":server 005 omairc CHANTYPES=# PREFIX=(ov)@+ "
                          ":are supported by this server\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :@omairc Alice\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));
    QCOMPARE(controller.peerMetadataEpoch(), 0);
    QSignalSpy spy(&controller, &IrcController::peerMetadataChanged);
    transport->injectBytes(
        QByteArrayLiteral(":server 761 omairc Alice bot * :PacketBot\r\n"));
    QCOMPARE(controller.peerMetadataEpoch(), 1);
    QCOMPARE(spy.count(), 1);
    QCOMPARE(controller.peerMetadata(QStringLiteral("libera"), QStringLiteral("Alice"))
                 .value(QStringLiteral("bot"))
                 .toBool(),
             true);
}

void ControllerTest::defaultPrefixPaintsLabelNotNick()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :~owner @+Alice +Bob @\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    auto *members = qobject_cast<QAbstractItemModel *>(controller.members());
    QCOMPARE(members->rowCount(), 3);
    QCOMPARE(roleAt(members, 0, MemberListModel::NickRole), QStringLiteral("owner"));
    QCOMPARE(roleAt(members, 0, MemberListModel::LabelRole), QStringLiteral("~owner"));
    QCOMPARE(roleAt(members, 1, MemberListModel::NickRole), QStringLiteral("Alice"));
    QCOMPARE(roleAt(members, 1, MemberListModel::LabelRole), QStringLiteral("@Alice"));
    QCOMPARE(roleAt(members, 2, MemberListModel::NickRole), QStringLiteral("Bob"));
    QCOMPARE(roleAt(members, 2, MemberListModel::LabelRole), QStringLiteral("+Bob"));

    transport->injectBytes(
        QByteArrayLiteral(":op!u@h MODE #omarchy -o Alice\r\n"
                          ":op!u@h MODE #omarchy +o Ghost\r\n"
                          ":Alice!u@h JOIN :#omarchy\r\n"));
    QCOMPARE(members->rowCount(), 3);
    QCOMPARE(roleAt(members, 0, MemberListModel::NickRole), QStringLiteral("owner"));
    QCOMPARE(roleAt(members, 1, MemberListModel::NickRole), QStringLiteral("Alice"));
    QCOMPARE(roleAt(members, 1, MemberListModel::LabelRole), QStringLiteral("+Alice"));
}

void ControllerTest::channelCloseSlashIsWrongScope()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("hello")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG #omarchy :hello\r\n"));
    const int framesBefore = transport->writtenFrames().size();

    QVERIFY(!controller.sendMessage(QStringLiteral("/close")));
    QCOMPARE(controller.lastError(),
             QStringLiteral("Close applies to direct messages"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("PART")));
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("QUIT")));
}

void ControllerTest::partDefaultsToSelectedChannel()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    QVERIFY(!controller.sendMessage(QStringLiteral("/part")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));

    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QVERIFY(rowForTarget(conversations, QStringLiteral("#omarchy")) >= 0);

    QVERIFY(controller.sendMessage(QStringLiteral("/part")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PART #omarchy\r\n"));
    QVERIFY(rowForTarget(conversations, QStringLiteral("#omarchy")) < 0);

    QVERIFY(!controller.sendMessage(QStringLiteral("/leave")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));

    QVERIFY(controller.console()->submit(QStringLiteral("/part #desktop leftover")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PART #desktop\r\n"));
}

void ControllerTest::failedJoin448PartDismissesWithoutPart()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/join #bad")));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("JOIN #bad\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#bad"));
    transport->injectBytes(
        QByteArrayLiteral(
            ":server 448 omairc #bad :Channel name contains illegal characters\r\n"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(hasEventBody(
        messages, QStringLiteral("Channel name contains illegal characters")));
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QVERIFY(rowForTarget(conversations, QStringLiteral("#bad")) >= 0);
    QVERIFY(rowForTarget(conversations, QStringLiteral("#omarchy")) >= 0);

    const int framesBefore = transport->writtenFrames().size();
    QVERIFY(controller.sendMessage(QStringLiteral("/part")));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("PART")));
    QVERIFY(rowForTarget(conversations, QStringLiteral("#bad")) < 0);
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
}

void ControllerTest::failedInviteJoinPartDismissesWithoutPart()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    transport->injectBytes(QByteArrayLiteral(":alice!u@h INVITE omairc :#lab\r\n"));
    QVERIFY(controller.sendMessage(QStringLiteral("/join")));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("JOIN #lab\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#lab"));
    transport->injectBytes(
        QByteArrayLiteral(":server 473 omairc #lab :Cannot join channel (+i)\r\n"));
    QVERIFY(logHasLabel(controller.console()->lines(), QStringLiteral("473")));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#lab"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(hasEventBody(messages, QStringLiteral("Cannot join channel (+i)")));

    QVERIFY(controller.sendMessage(QStringLiteral("/join")));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("JOIN #lab\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QVERIFY(rowForTarget(conversations, QStringLiteral("#lab")) >= 0);
    const int framesBefore = transport->writtenFrames().size();
    QVERIFY(controller.sendMessage(QStringLiteral("/part")));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("PART")));
    QVERIFY(rowForTarget(conversations, QStringLiteral("#lab")) < 0);
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
}

void ControllerTest::joinedPartSendsAndDropsSelected()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"
                          ":omairc!u@h JOIN :#lab\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QVERIFY(rowForTarget(conversations, QStringLiteral("#omarchy")) >= 0);
    QVERIFY(rowForTarget(conversations, QStringLiteral("#lab")) >= 0);

    QVERIFY(controller.sendMessage(QStringLiteral("/part")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PART #omarchy\r\n"));
    QVERIFY(rowForTarget(conversations, QStringLiteral("#omarchy")) < 0);
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#lab"));
}

void ControllerTest::partMissingChannelStillSends()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);

    QVERIFY(controller.sendMessage(QStringLiteral("/part #neveropened")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PART #neveropened\r\n"));
    QVERIFY(rowForTarget(conversations, QStringLiteral("#omarchy")) >= 0);
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QCOMPARE(rowForTarget(conversations, QStringLiteral("#neveropened")), -1);
}

void ControllerTest::partNonSelectedUnjoinedDropsWithoutPart()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/join #ghost")));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("JOIN #ghost\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#ghost"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QVERIFY(rowForTarget(conversations, QStringLiteral("#ghost")) >= 0);

    const int framesBefore = transport->writtenFrames().size();
    QVERIFY(controller.sendMessage(QStringLiteral("/part #ghost")));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("PART")));
    QVERIFY(rowForTarget(conversations, QStringLiteral("#ghost")) < 0);
    QVERIFY(rowForTarget(conversations, QStringLiteral("#omarchy")) >= 0);
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
}

void ControllerTest::partUnjoinedDropsMute()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/join #ghost")));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("JOIN #ghost\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#ghost"));
    QVERIFY(controller.sendMessage(QStringLiteral("/mute")));
    QVERIFY(IrcMuteStore().contains(
        QStringLiteral("libera"), QStringLiteral("#ghost"), IrcCaseMapping()));
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    const int mutedRow = rowForTarget(conversations, QStringLiteral("#ghost"));
    QVERIFY(mutedRow >= 0);
    QCOMPARE(roleAt(conversations, mutedRow, ConversationListModel::MutedRole), true);

    const int framesBefore = transport->writtenFrames().size();
    QVERIFY(controller.sendMessage(QStringLiteral("/part")));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("PART")));
    QVERIFY(rowForTarget(conversations, QStringLiteral("#ghost")) < 0);
    QVERIFY(!IrcMuteStore().contains(
        QStringLiteral("libera"), QStringLiteral("#ghost"), IrcCaseMapping()));

    QVERIFY(controller.sendMessage(QStringLiteral("/join #ghost")));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("JOIN #ghost\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#ghost"));
    const int rejoined = rowForTarget(conversations, QStringLiteral("#ghost"));
    QVERIFY(rejoined >= 0);
    QCOMPARE(roleAt(conversations, rejoined, ConversationListModel::MutedRole), false);
    QVERIFY(!IrcMuteStore().contains(
        QStringLiteral("libera"), QStringLiteral("#ghost"), IrcCaseMapping()));
}

void ControllerTest::partUnjoinedDelayedJoinSendsPart()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/join #lab")));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("JOIN #lab\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#lab"));
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QVERIFY(rowForTarget(conversations, QStringLiteral("#lab")) >= 0);

    QVERIFY(controller.sendMessage(QStringLiteral("/part")));
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("PART")));
    QVERIFY(rowForTarget(conversations, QStringLiteral("#lab")) < 0);
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#lab\r\n"));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("PART #lab\r\n"));
    QVERIFY(rowForTarget(conversations, QStringLiteral("#lab")) < 0);
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
}

void ControllerTest::rejoinClearsCancelledPendingJoin()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/join #lab")));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("JOIN #lab\r\n"));
    QVERIFY(controller.sendMessage(QStringLiteral("/part")));
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("PART")));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/join #lab")));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("JOIN #lab\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#lab"));
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QVERIFY(rowForTarget(conversations, QStringLiteral("#lab")) >= 0);

    const int framesBefore = transport->writtenFrames().size();
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#lab\r\n"));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("PART")));
    QVERIFY(rowForTarget(conversations, QStringLiteral("#lab")) >= 0);
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#lab"));
    QCOMPARE(controller.peopleCount(), 1);
}

void ControllerTest::partNonSelectedUnjoinedDelayedJoinKeepsSelection()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/join #lab")));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("JOIN #lab\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#lab"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QVERIFY(rowForTarget(conversations, QStringLiteral("#lab")) >= 0);

    QVERIFY(controller.sendMessage(QStringLiteral("/part #lab")));
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("PART")));
    QVERIFY(rowForTarget(conversations, QStringLiteral("#lab")) < 0);
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    transport->injectBytes(QByteArrayLiteral(":alice!u@h JOIN :#lab\r\n"));
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("PART")));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#lab\r\n"));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("PART #lab\r\n"));
    QVERIFY(rowForTarget(conversations, QStringLiteral("#lab")) < 0);
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
}

void ControllerTest::partFromDirectIsWrongScope()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"
                          ":lena!u@h PRIVMSG omairc :hi\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    const int framesBefore = transport->writtenFrames().size();

    QVERIFY(!controller.sendMessage(QStringLiteral("/part")));
    QCOMPARE(controller.lastError(), QStringLiteral("Part applies to channels"));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("PART")));

    QVERIFY(controller.sendMessage(QStringLiteral("/part #omarchy")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PART #omarchy\r\n"));
}

void ControllerTest::kickDefaultsToSelectedChannel()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    QVERIFY(!controller.sendMessage(QStringLiteral("/kick")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QVERIFY(!controller.sendMessage(QStringLiteral("/kick bob")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));

    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(!controller.sendMessage(QStringLiteral("/kick")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QVERIFY(!controller.sendMessage(QStringLiteral("/kick #omarchy")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));

    QVERIFY(controller.sendMessage(QStringLiteral("/kick bob")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("KICK #omarchy bob\r\n"));

    QVERIFY(controller.sendMessage(QStringLiteral("/kick bob spam")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("KICK #omarchy bob :spam\r\n"));

    QVERIFY(controller.sendMessage(QStringLiteral("/kick #desktop alice leftover")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("KICK #desktop alice :leftover\r\n"));
}

void ControllerTest::kickFromDirectIsWrongScope()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"
                          ":lena!u@h PRIVMSG omairc :hi\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    const int framesBefore = transport->writtenFrames().size();

    QVERIFY(!controller.sendMessage(QStringLiteral("/kick")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QVERIFY(!controller.sendMessage(QStringLiteral("/kick bob")));
    QCOMPARE(controller.lastError(), QStringLiteral("Kick applies to channels"));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("KICK")));

    QVERIFY(controller.sendMessage(QStringLiteral("/kick #omarchy bob")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("KICK #omarchy bob\r\n"));

    QVERIFY(controller.sendMessage(QStringLiteral("/kick #omarchy bob spam")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("KICK #omarchy bob :spam\r\n"));
}

void ControllerTest::incomingInviteStaysOnStatusWithoutConversation()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    transport->injectBytes(QByteArrayLiteral(":alice!u@h INVITE omairc :#lab\r\n"));

    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("alice invited you to #lab")));
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("#lab")), -1);
    QVERIFY(rowForTarget(conversations, QStringLiteral("#omarchy")) >= 0);
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("JOIN #lab")));

    QVERIFY(controller.sendMessage(QStringLiteral("/join #lab")));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("JOIN #lab\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#lab"));
    QVERIFY(rowForTarget(conversations, QStringLiteral("#lab")) >= 0);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#lab\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#lab"));
}

void ControllerTest::emptyJoinAcceptsLatestInvite()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(!controller.sendMessage(QStringLiteral("/join")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));

    transport->injectBytes(QByteArrayLiteral(":alice!u@h INVITE omairc :#lab\r\n"));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("alice invited you to #lab")));

    transport->injectBytes(QByteArrayLiteral(":bob!u@h INVITE omairc :#desk\r\n"));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("bob invited you to #desk")));

    QVERIFY(controller.sendMessage(QStringLiteral("/join")));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("JOIN #desk\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#desk"));
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#desk\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#desk"));
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QVERIFY(rowForTarget(conversations, QStringLiteral("#desk")) >= 0);

    QVERIFY(!controller.sendMessage(QStringLiteral("/join")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
}

void ControllerTest::emptyJoinFromStatusUsesPendingInvite()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    controller.openStatus(QStringLiteral("libera"));

    transport->injectBytes(QByteArrayLiteral(":alice!u@h INVITE omairc :#lab\r\n"));
    QVERIFY(controller.console()->submit(QStringLiteral("/join")));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("JOIN #lab\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#lab"));
    QVERIFY(!controller.console()->isOpen());
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#lab\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#lab"));
    QVERIFY(!controller.console()->isOpen());
}

void ControllerTest::joinOtherDoesNotConsumePendingInvite()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    transport->injectBytes(QByteArrayLiteral(":alice!u@h INVITE omairc :#lab\r\n"));
    QVERIFY(controller.sendMessage(QStringLiteral("/join #other")));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("JOIN #other\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#other"));
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#other\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#other"));

    QVERIFY(controller.sendMessage(QStringLiteral("/join")));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("JOIN #lab\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#lab"));
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#lab\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#lab"));
}

void ControllerTest::joinOpensLastNamedChannel()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/join #alpha,#beta,#gamma")));
    QCOMPARE(transport->writtenFrames().mid(transport->writtenFrames().size() - 3),
             QByteArrayList({
                 QByteArrayLiteral("JOIN #alpha\r\n"),
                 QByteArrayLiteral("JOIN #beta\r\n"),
                 QByteArrayLiteral("JOIN #gamma\r\n"),
             }));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#gamma"));
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QVERIFY(rowForTarget(conversations, QStringLiteral("#gamma")) >= 0);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("#alpha")), -1);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("#beta")), -1);
    transport->injectBytes(
        QByteArrayLiteral(":server 473 omairc #alpha :Cannot join channel (+i)\r\n"));
    QCOMPARE(rowForTarget(conversations, QStringLiteral("#alpha")), -1);
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#gamma"));

    QVERIFY(controller.sendMessage(QStringLiteral("/j lab")));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("JOIN #lab\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#lab"));
}

void ControllerTest::failedInviteJoinKeepsPending()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    transport->injectBytes(QByteArrayLiteral(":alice!u@h INVITE omairc :#lab\r\n"));
    QVERIFY(controller.sendMessage(QStringLiteral("/join")));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("JOIN #lab\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#lab"));
    transport->injectBytes(
        QByteArrayLiteral(":server 473 omairc #lab :Cannot join channel (+i)\r\n"));
    QVERIFY(logHasLabel(controller.console()->lines(), QStringLiteral("473")));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#lab"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(hasEventBody(messages, QStringLiteral("Cannot join channel (+i)")));

    QVERIFY(controller.sendMessage(QStringLiteral("/join")));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("JOIN #lab\r\n"));
}

void ControllerTest::sessionDropClearsPendingInvite()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    transport->injectBytes(QByteArrayLiteral(":alice!u@h INVITE omairc :#lab\r\n"));
    transport->remoteClose();
    QCOMPARE(session->state(), IrcSession::State::Failed);

    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    QVERIFY(!controller.sendMessage(QStringLiteral("/join")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("JOIN #lab")));
}

void ControllerTest::inviteDefaultsToSelectedChannel()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    QVERIFY(!controller.sendMessage(QStringLiteral("/invite")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QVERIFY(!controller.sendMessage(QStringLiteral("/invite bob")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));

    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(!controller.sendMessage(QStringLiteral("/invite")));
    QCOMPARE(controller.lastError(), QStringLiteral("Invite applies to channels"));

    QVERIFY(controller.sendMessage(QStringLiteral("/invite bob")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("INVITE bob #omarchy\r\n"));

    QVERIFY(controller.sendMessage(QStringLiteral("/invite bob #lab")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("INVITE bob #lab\r\n"));

    QVERIFY(!controller.sendMessage(QStringLiteral("/invite #lab")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QVERIFY(!controller.sendMessage(QStringLiteral("/invite bob #lab extra")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
}

void ControllerTest::inviteFromDirectAndStatusNeedsChannel()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"
                          ":lena!u@h PRIVMSG omairc :hi\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    const int framesBefore = transport->writtenFrames().size();

    QVERIFY(!controller.sendMessage(QStringLiteral("/invite")));
    QCOMPARE(controller.lastError(), QStringLiteral("Invite applies to channels"));
    QVERIFY(!controller.sendMessage(QStringLiteral("/invite bob")));
    QCOMPARE(controller.lastError(), QStringLiteral("Invite applies to channels"));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("INVITE")));

    QVERIFY(controller.sendMessage(QStringLiteral("/invite bob #lab")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("INVITE bob #lab\r\n"));

    controller.openStatus(QStringLiteral("libera"));
    QVERIFY(controller.console()->submit(QStringLiteral("/invite bob")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("Invite applies to channels")));
    QVERIFY(controller.console()->submit(QStringLiteral("/invite bob #desktop")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("INVITE bob #desktop\r\n"));
}

void ControllerTest::implicitStatusInviteStaysOnFocusedNetwork()
{
    IrcController controller;
    auto *transportA = new FakeIrcTransport;
    auto *transportB = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("network-a")), transportA));
    QVERIFY(controller.addSession(config(QStringLiteral("network-b")), transportB));
    registerSession(controller.session(QStringLiteral("network-a")), transportA);
    registerSession(controller.session(QStringLiteral("network-b")), transportB);
    transportA->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#chan\r\n"));
    transportB->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#lab\r\n"));
    controller.selectConversation(QStringLiteral("network-a"), QStringLiteral("#chan"));
    controller.openStatus(QStringLiteral("network-b"));

    QVERIFY(controller.console()->submit(QStringLiteral("/invite bob")));
    QVERIFY(!framesContain(transportA->writtenFrames(), QByteArrayLiteral("INVITE")));
    QVERIFY(!framesContain(transportB->writtenFrames(), QByteArrayLiteral("INVITE")));

    QVERIFY(controller.console()->submit(QStringLiteral("/invite bob #lab")));
    QCOMPARE(transportB->writtenFrames().last(),
             QByteArrayLiteral("INVITE bob #lab\r\n"));
    QVERIFY(!framesContain(transportA->writtenFrames(), QByteArrayLiteral("INVITE")));
}

void ControllerTest::statusPartDefaultsToSelectedChannel()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.console()->submit(QStringLiteral("/part")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PART #omarchy\r\n"));
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QVERIFY(rowForTarget(conversations, QStringLiteral("#omarchy")) < 0);
}

void ControllerTest::partImplicitUsesSelectedSession()
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
    registerSession(sessionA, transportA);
    transportA->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));

    registerSession(sessionB, transportB);
    transportB->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("network-b"),
                                  QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/part")));
    QCOMPARE(transportB->writtenFrames().last(),
             QByteArrayLiteral("PART #omarchy\r\n"));
    QVERIFY(!framesContain(transportA->writtenFrames(), QByteArrayLiteral("PART")));
    QCOMPARE(controller.selectedNetworkId(), QStringLiteral("network-a"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
}

void ControllerTest::topicUsesSelectedSession()
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
    registerSession(sessionA, transportA);
    transportA->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));

    registerSession(sessionB, transportB);
    transportB->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("network-b"),
                                  QStringLiteral("#omarchy"));

    const QString topicBefore = controller.topic();
    QVERIFY(controller.sendMessage(QStringLiteral("/topic from b")));
    QCOMPARE(transportB->writtenFrames().last(),
             QByteArrayLiteral("TOPIC #omarchy :from b\r\n"));
    QVERIFY(!framesContain(transportA->writtenFrames(), QByteArrayLiteral("TOPIC")));
    QCOMPARE(controller.topic(), topicBefore);

    transportB->injectBytes(
        QByteArrayLiteral(":omairc!u@h TOPIC #omarchy :from b\r\n"));
    QCOMPARE(controller.topic(), QStringLiteral("from b"));
}

void ControllerTest::topicFromDirectIsWrongScope()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"
                          ":lena!u@h PRIVMSG omairc :hi\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    const int framesBefore = transport->writtenFrames().size();

    QVERIFY(!controller.sendMessage(QStringLiteral("/topic hello")));
    QCOMPARE(controller.lastError(), QStringLiteral("Topic applies to channels"));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("TOPIC")));
}

void ControllerTest::emptyTopicDoesNotWrite()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    const int framesBefore = transport->writtenFrames().size();

    QVERIFY(controller.sendMessage(QStringLiteral("/topic")));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("TOPIC")));
}

void ControllerTest::closeDirectMessageDropsAndSelectsNeighbor()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":omairc!u@h JOIN :#desktop\r\n"
                          ":omairc!u@h PART #desktop\r\n"
                          ":lena!u@h PRIVMSG omairc :hi\r\n"
                          ":zed!u@h PRIVMSG omairc :later\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(conversations->rowCount(), 4);
    QCOMPARE(roleAt(conversations, 0, ConversationListModel::ConversationRole),
             QStringLiteral("#omarchy"));
    QCOMPARE(roleAt(conversations, 1, ConversationListModel::ConversationRole),
             QStringLiteral("#desktop"));
    QCOMPARE(roleAt(conversations, 2, ConversationListModel::ConversationRole),
             QStringLiteral("lena"));
    QCOMPARE(roleAt(conversations, 3, ConversationListModel::ConversationRole),
             QStringLiteral("zed"));
    QVERIFY(rowForTarget(conversations, QStringLiteral("#desktop")) >= 0);

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    const int framesBeforeClose = transport->writtenFrames().size();
    QVERIFY(controller.sendMessage(QStringLiteral("/close")));
    QCOMPARE(controller.lastError(), QString());
    QCOMPARE(controller.selectedTarget(), QStringLiteral("zed"));
    QVERIFY(rowForTarget(conversations, QStringLiteral("lena")) < 0);
    QVERIFY(rowForTarget(conversations, QStringLiteral("zed")) >= 0);
    QVERIFY(rowForTarget(conversations, QStringLiteral("#desktop")) >= 0);
    QCOMPARE(transport->writtenFrames().size(), framesBeforeClose);
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("PART")));
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("QUIT")));

    QVERIFY(controller.sendMessage(QStringLiteral("/close")));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#desktop"));
    QVERIFY(rowForTarget(conversations, QStringLiteral("zed")) < 0);
    QVERIFY(rowForTarget(conversations, QStringLiteral("#omarchy")) >= 0);
    QVERIFY(rowForTarget(conversations, QStringLiteral("#desktop")) >= 0);
}

void ControllerTest::closeDirectMessageInvokableOnChannelIsSilent()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(!controller.sendMessage(QStringLiteral("/nope")));
    const QString errorBefore = controller.lastError();
    QCOMPARE(errorBefore, QStringLiteral("Unknown command: /nope"));

    controller.closeDirectMessage();
    QCOMPARE(controller.lastError(), errorBefore);
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
}

void ControllerTest::closeDirectMessageWhileDisconnected()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":lena!u@h PRIVMSG omairc :hi\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("lena"));
    transport->remoteClose();
    QVERIFY(session->state() != IrcSession::State::Registered);

    QVERIFY(controller.sendMessage(QStringLiteral("/close")));
    QCOMPARE(controller.selectedTarget(), QString());
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QCOMPARE(conversations->rowCount(), 0);
}

void ControllerTest::clearingLastDirectNotifiesConnectionStatus()
{
    IrcController controller;
    auto *transportA = new FakeIrcTransport;
    auto *transportB = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("network-a")), transportA));
    QVERIFY(controller.addSession(config(QStringLiteral("network-b")), transportB));
    registerSession(controller.session(QStringLiteral("network-b")), transportB);
    transportB->injectBytes(QByteArrayLiteral(":bob!u@h PRIVMSG omairc :hi\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("bob"));
    QCOMPARE(controller.focusedNetworkId(), QStringLiteral("network-b"));
    QCOMPARE(controller.connectionStatus(), QStringLiteral("Connected"));

    // Point Status at the offline network without changing the selected DM.
    // Closing the last direct falls back to that console network.
    controller.console()->setNetwork(QStringLiteral("network-a"));
    QCOMPARE(controller.focusedNetworkId(), QStringLiteral("network-b"));
    QCOMPARE(controller.connectionStatus(), QStringLiteral("Connected"));

    QSignalSpy statusSpy(&controller, &IrcController::statusChanged);
    QVERIFY(controller.sendMessage(QStringLiteral("/close")));
    QCOMPARE(controller.selectedTarget(), QString());
    QCOMPARE(controller.focusedNetworkId(), QStringLiteral("network-a"));
    QCOMPARE(controller.connectionStatus(), QStringLiteral("Offline"));
    QVERIFY(statusSpy.count() >= 1);
}

void ControllerTest::selectingOfflineNetworkNotifiesConnectionStatus()
{
    IrcController controller;
    auto *transportA = new FakeIrcTransport;
    auto *transportB = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("network-a")), transportA));
    QVERIFY(controller.addSession(config(QStringLiteral("network-b")), transportB));
    registerSession(controller.session(QStringLiteral("network-a")), transportA);
    transportA->injectBytes(QByteArrayLiteral(":alice!u@h PRIVMSG omairc :hi\r\n"));
    QCOMPARE(controller.connectionStatus(), QStringLiteral("Connected"));

    QSignalSpy statusSpy(&controller, &IrcController::statusChanged);
    controller.selectConversation(QStringLiteral("network-b"), QStringLiteral("bob"));
    QCOMPARE(controller.focusedNetworkId(), QStringLiteral("network-b"));
    QCOMPARE(controller.connectionStatus(), QStringLiteral("Offline"));
    QVERIFY(statusSpy.count() >= 1);
}

void ControllerTest::openStatusWithoutSessionNotifiesConnectionStatus()
{
    IrcController controller;
    auto *transportA = new FakeIrcTransport;
    auto *transportB = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("network-a")), transportA));
    QVERIFY(controller.addSession(config(QStringLiteral("network-b")), transportB));
    registerSession(controller.session(QStringLiteral("network-a")), transportA);
    transportA->injectBytes(QByteArrayLiteral(":alice!u@h PRIVMSG omairc :hi\r\n"));
    QCOMPARE(controller.connectionStatus(), QStringLiteral("Connected"));

    QSignalSpy statusSpy(&controller, &IrcController::statusChanged);
    controller.openStatus(QStringLiteral("network-b"));
    QCOMPARE(controller.focusedNetworkId(), QStringLiteral("network-b"));
    QCOMPARE(controller.connectionStatus(), QStringLiteral("Offline"));
    QVERIFY(statusSpy.count() >= 1);
}

void ControllerTest::selfAwayFollowsNumericsAndUnawaysAfterChat()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    QVERIFY(!controller.selfAway());

    transport->injectBytes(
        QByteArrayLiteral(":server 306 omairc :You have been marked as being away\r\n"));
    QVERIFY(controller.selfAway());

    transport->injectBytes(
        QByteArrayLiteral(":server 305 omairc :You are no longer marked as being away\r\n"));
    QVERIFY(!controller.selfAway());

    transport->injectBytes(
        QByteArrayLiteral(":server 005 omairc CHANTYPES=# PREFIX=(ov)@+ "
                          ":are supported by this server\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :@omairc\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"
                          ":server 352 omairc #omarchy u h server omairc G :0 real\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(!controller.selfAway());

    transport->injectBytes(
        QByteArrayLiteral(":server 306 omairc :You have been marked as being away\r\n"));
    QVERIFY(controller.selfAway());

    QVERIFY(controller.sendMessage(QStringLiteral("hello")));
    QCOMPARE(transport->writtenFrames().at(transport->writtenFrames().size() - 2),
             QByteArrayLiteral("PRIVMSG #omarchy :hello\r\n"));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("AWAY\r\n"));
    QVERIFY(controller.selfAway());

    QVERIFY(controller.sendMessage(QStringLiteral("again")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG #omarchy :again\r\n"));
    QCOMPARE(transport->writtenFrames().count(QByteArrayLiteral("AWAY\r\n")), 1);
    QVERIFY(controller.selfAway());

    transport->injectBytes(
        QByteArrayLiteral(":server 305 omairc :You are no longer marked as being away\r\n"
                          ":server 306 omairc :You have been marked as being away\r\n"));
    QVERIFY(controller.selfAway());
    QVERIFY(controller.sendMessage(QStringLiteral("third")));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("AWAY\r\n"));
    QCOMPARE(transport->writtenFrames().count(QByteArrayLiteral("AWAY\r\n")), 2);

    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc NEW :away-notify\r\n"
                          ":server CAP omairc ACK :away-notify\r\n"
                          ":server CAP omairc DEL :away-notify\r\n"));
    QVERIFY(!controller.hasAwayPresence());
    QVERIFY(controller.selfAway());
}

void ControllerTest::selfAwayRefreshesOurOwnMemberRow()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :away-notify\r\n"
                          ":server CAP omairc ACK :away-notify\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":server 005 omairc CHANTYPES=# PREFIX=(ov)@+ "
                          ":are supported by this server\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :@omairc +Alice\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    auto *members = qobject_cast<QAbstractItemModel *>(controller.members());
    QVERIFY(members);
    QCOMPARE(roleAt(members, 0, MemberListModel::NickRole), QStringLiteral("omairc"));
    QCOMPARE(roleAt(members, 0, MemberListModel::AwayRole), false);
    QCOMPARE(roleAt(members, 1, MemberListModel::AwayRole), false);

    QSignalSpy changed(members, &QAbstractItemModel::dataChanged);
    transport->injectBytes(
        QByteArrayLiteral(":server 306 omairc :You have been marked as being away\r\n"));
    QVERIFY(controller.selfAway());
    QVERIFY2(!changed.isEmpty(),
             "the member row for our own nick must refresh when self-away changes");
    QCOMPARE(roleAt(members, 0, MemberListModel::AwayRole), true);
    QCOMPARE(roleAt(members, 1, MemberListModel::AwayRole), false);

    transport->injectBytes(
        QByteArrayLiteral(":server 305 omairc :You are no longer marked as being away\r\n"));
    QVERIFY(!controller.selfAway());
    QCOMPARE(roleAt(members, 0, MemberListModel::AwayRole), false);
}

void ControllerTest::explicitChatClearsAwayOnOriginNetwork()
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
    registerSession(sessionA, transportA);
    registerSession(sessionB, transportB);
    transportA->injectBytes(
        QByteArrayLiteral(":server 306 omairc :You have been marked as being away\r\n"));
    QVERIFY(controller.selfAway());
    controller.selectConversation(QStringLiteral("network-b"), QStringLiteral("bob"));

    QVERIFY(controller.sendToTarget(QStringLiteral("network-a"),
                                    QStringLiteral("bob"),
                                    QStringLiteral("hello")));
    QCOMPARE(transportA->writtenFrames().last(),
             QByteArrayLiteral("AWAY\r\n"));
}

void ControllerTest::lastErrorIsolatedByNetwork()
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
    registerSession(sessionB, transportB);
    controller.selectConversation(QStringLiteral("network-b"), QStringLiteral("bob"));

    QVERIFY(!controller.sendToTarget(QStringLiteral("network-a"),
                                     QStringLiteral("bob"),
                                     QStringLiteral("hello")));
    QCOMPARE(controller.lastErrorForNetwork(QStringLiteral("network-a")),
             QStringLiteral("Not connected"));
    QCOMPARE(controller.lastError(), QString());
}

void ControllerTest::closeDirectMessageClearsOriginNetworkError()
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
    registerSession(sessionA, transportA);
    registerSession(sessionB, transportB);
    transportA->injectBytes(QByteArrayLiteral(":alice!u@h PRIVMSG omairc :hello\r\n"));
    transportB->injectBytes(QByteArrayLiteral(":bob!u@h PRIVMSG omairc :hello\r\n"));

    controller.selectConversation(QStringLiteral("network-b"), QStringLiteral("bob"));
    QVERIFY(!controller.sendMessage(QStringLiteral("/part")));
    QCOMPARE(controller.lastErrorForNetwork(QStringLiteral("network-b")),
             QStringLiteral("Part applies to channels"));

    transportA->remoteClose();
    controller.selectConversation(QStringLiteral("network-a"), QStringLiteral("alice"));
    QVERIFY(!controller.sendMessage(QStringLiteral("/part")));
    QCOMPARE(controller.lastErrorForNetwork(QStringLiteral("network-a")),
             QStringLiteral("Not connected"));

    controller.closeDirectMessage();
    QCOMPARE(controller.selectedNetworkId(), QStringLiteral("network-b"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("bob"));
    QCOMPARE(controller.lastErrorForNetwork(QStringLiteral("network-a")),
             QString());
    QCOMPARE(controller.lastErrorForNetwork(QStringLiteral("network-b")),
             QStringLiteral("Part applies to channels"));
}

void ControllerTest::closeLastDirectKeepsSelfAway()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":server 306 omairc :You have been marked as being away\r\n"
                          ":zed!u@h PRIVMSG omairc :later\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("zed"));
    QVERIFY(controller.selfAway());
    QVERIFY(controller.sendMessage(QStringLiteral("/close")));
    QCOMPARE(controller.selectedTarget(), QString());
    QVERIFY(controller.selfAway());
}

void ControllerTest::queryOpensDirectWithoutPrivmsg()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/query lena")));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("lena"));
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("PRIVMSG lena")));
}

void ControllerTest::queryAliceCreatesDirectRowWithoutPrivmsg()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/query alice")));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("alice"));
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("PRIVMSG alice")));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    const int row = rowForTarget(conversations, QStringLiteral("alice"));
    QVERIFY(row >= 0);
    QCOMPARE(roleAt(conversations, row, ConversationListModel::DirectRole), true);

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QCOMPARE(messages->rowCount(), 0);
}

void ControllerTest::queryWithTextSendsPrivmsg()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/query lena hello")));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("lena"));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG lena :hello\r\n"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QCOMPARE(messages->rowCount(), 1);
    QCOMPARE(roleAt(messages, 0, MessageListModel::AuthorRole),
             QStringLiteral("omairc"));
    QCOMPARE(roleAt(messages, 0, MessageListModel::BodyRole),
             QStringLiteral("hello"));
    QCOMPARE(roleAt(messages, 0, MessageListModel::KindRole),
             QStringLiteral("message"));
}

void ControllerTest::queryChannelAndEmptyAreRefused()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    const int framesBefore = transport->writtenFrames().size();

    QVERIFY(!controller.sendMessage(QStringLiteral("/query #omarchy")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);

    QVERIFY(!controller.sendMessage(QStringLiteral("/query")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);
}

void ControllerTest::noticeSendsWithoutSelecting()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/notice lena later")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("NOTICE lena :later\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("lena")), -1);
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
}

void ControllerTest::noticeEchoesExistingDirect()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":lena!u@h PRIVMSG omairc :hi\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/notice lena later")));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("NOTICE lena :later\r\n"));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(messages->rowCount() > 0);
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::BodyRole),
             QStringLiteral("later"));
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::KindRole),
             QStringLiteral("notice"));
}

void ControllerTest::noticeEchoesSelectedChannel()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/notice #omarchy heads up")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("NOTICE #omarchy :heads up\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(messages->rowCount() > 0);
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::BodyRole),
             QStringLiteral("heads up"));
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::KindRole),
             QStringLiteral("notice"));
}

void ControllerTest::noticeMissingTokensAreRefused()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    const int framesBefore = transport->writtenFrames().size();

    QVERIFY(!controller.sendMessage(QStringLiteral("/notice")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QVERIFY(!framesContain(transport->writtenFrames().mid(framesBefore),
                           QByteArrayLiteral("NOTICE")));

    QVERIFY(!controller.sendMessage(QStringLiteral("/notice lena")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QVERIFY(!framesContain(transport->writtenFrames().mid(framesBefore),
                           QByteArrayLiteral("NOTICE")));
}

void ControllerTest::statusNoticeStaysOpen()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    IrcStatusConsole *console = controller.console();
    console->setOpen(true);
    QVERIFY(console->submit(QStringLiteral("/notice #omarchy ping")));
    QVERIFY(console->isOpen());
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("NOTICE #omarchy :ping\r\n"));
}

void ControllerTest::statusNoticeMissingBodyStaysOpen()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    IrcStatusConsole *console = controller.console();
    console->setOpen(true);
    const int framesBefore = transport->writtenFrames().size();
    QVERIFY(console->submit(QStringLiteral("/notice lena")));
    QVERIFY(console->isOpen());
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QVERIFY(logContains(console->lines(), QStringLiteral("Command was refused")));
    QVERIFY(!framesContain(transport->writtenFrames().mid(framesBefore),
                           QByteArrayLiteral("NOTICE")));
}

void ControllerTest::disconnectedNoticeIsNotConnected()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    transport->remoteClose();
    QVERIFY(session->state() != IrcSession::State::Registered);
    const int framesBefore = transport->writtenFrames().size();

    QVERIFY(!controller.sendMessage(QStringLiteral("/notice lena later")));
    QCOMPARE(controller.lastError(), QStringLiteral("Not connected"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);
    QVERIFY(!framesContain(transport->writtenFrames().mid(framesBefore),
                           QByteArrayLiteral("NOTICE")));
}

void ControllerTest::noticeDoesNotUnaway()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 306 omairc :You have been marked as being away\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(controller.selfAway());
    const int awayBefore = transport->writtenFrames().count(QByteArrayLiteral("AWAY\r\n"));

    QVERIFY(controller.sendMessage(QStringLiteral("/notice lena later")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("NOTICE lena :later\r\n"));
    QCOMPARE(transport->writtenFrames().count(QByteArrayLiteral("AWAY\r\n")),
             awayBefore);
    QVERIFY(controller.selfAway());
}

void ControllerTest::msgSendsWithoutSelecting()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    const int selectedRows = messages->rowCount();

    QVERIFY(controller.sendMessage(QStringLiteral("/msg lena hello")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG lena :hello\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("lena")), -1);
    QCOMPARE(messages->rowCount(), selectedRows);
    for (int row = 0; row < messages->rowCount(); ++row) {
        QVERIFY(roleAt(messages, row, MessageListModel::BodyRole)
                != QStringLiteral("hello"));
    }
}

void ControllerTest::msgEchoesExistingDirect()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":lena!u@h PRIVMSG omairc :hi\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/msg lena later")));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG lena :later\r\n"));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(messages->rowCount() > 0);
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::BodyRole),
             QStringLiteral("later"));
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::KindRole),
             QStringLiteral("message"));
}

void ControllerTest::msgDoesNotFocusExistingDirect()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":lena!u@h PRIVMSG omairc :hi\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/msg lena later")));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
}

void ControllerTest::msgEchoesSelectedGhostQuery()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/query lena")));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("lena"));

    QVERIFY(controller.sendMessage(QStringLiteral("/msg lena hello")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG lena :hello\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("lena"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(messages->rowCount() > 0);
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::BodyRole),
             QStringLiteral("hello"));
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::KindRole),
             QStringLiteral("message"));
}

void ControllerTest::msgMissingTokensAreRefused()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    const int framesBefore = transport->writtenFrames().size();

    QVERIFY(!controller.sendMessage(QStringLiteral("/msg")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QVERIFY(!framesContain(transport->writtenFrames().mid(framesBefore),
                           QByteArrayLiteral("PRIVMSG")));

    QVERIFY(!controller.sendMessage(QStringLiteral("/msg lena")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QVERIFY(!framesContain(transport->writtenFrames().mid(framesBefore),
                           QByteArrayLiteral("PRIVMSG")));
}

void ControllerTest::msgChannelIsRefused()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    const int framesBefore = transport->writtenFrames().size();

    QVERIFY(!controller.sendMessage(QStringLiteral("/msg #omarchy hello")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);
    QVERIFY(!framesContain(transport->writtenFrames().mid(framesBefore),
                           QByteArrayLiteral("PRIVMSG")));
}

void ControllerTest::statusMsgStaysOpen()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    IrcStatusConsole *console = controller.console();
    console->setOpen(true);
    QVERIFY(console->submit(QStringLiteral("/msg NickServ help")));
    QVERIFY(console->isOpen());
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG NickServ :help\r\n"));
}

void ControllerTest::disconnectedMsgIsNotConnected()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    transport->remoteClose();
    QVERIFY(session->state() != IrcSession::State::Registered);
    const int framesBefore = transport->writtenFrames().size();

    QVERIFY(!controller.sendMessage(QStringLiteral("/msg lena hello")));
    QCOMPARE(controller.lastError(), QStringLiteral("Not connected"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);
    QVERIFY(!framesContain(transport->writtenFrames().mid(framesBefore),
                           QByteArrayLiteral("PRIVMSG")));
}

void ControllerTest::msgClearsAway()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 306 omairc :You have been marked as being away\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(controller.selfAway());
    const int awayBefore = transport->writtenFrames().count(QByteArrayLiteral("AWAY\r\n"));

    QVERIFY(controller.sendMessage(QStringLiteral("/msg lena hello")));
    QCOMPARE(transport->writtenFrames().at(transport->writtenFrames().size() - 2),
             QByteArrayLiteral("PRIVMSG lena :hello\r\n"));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("AWAY\r\n"));
    QCOMPARE(transport->writtenFrames().count(QByteArrayLiteral("AWAY\r\n")),
             awayBefore + 1);
    QVERIFY(controller.selfAway());
}

void ControllerTest::awayWaitsForNumericThenChatUnaways()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(!controller.selfAway());

    QVERIFY(controller.sendMessage(QStringLiteral("/away lunch")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("AWAY :lunch\r\n"));
    QVERIFY(!controller.selfAway());

    transport->injectBytes(
        QByteArrayLiteral(":server 306 omairc :You have been marked as being away\r\n"));
    QVERIFY(controller.selfAway());

    QVERIFY(controller.sendMessage(QStringLiteral("hello")));
    QCOMPARE(transport->writtenFrames().at(transport->writtenFrames().size() - 2),
             QByteArrayLiteral("PRIVMSG #omarchy :hello\r\n"));
    QCOMPARE(transport->writtenFrames().last(), QByteArrayLiteral("AWAY\r\n"));
    QCOMPARE(transport->writtenFrames().count(QByteArrayLiteral("AWAY\r\n")), 1);
    QVERIFY(controller.selfAway());

    QVERIFY(controller.sendMessage(QStringLiteral("again")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG #omarchy :again\r\n"));
    QCOMPARE(transport->writtenFrames().count(QByteArrayLiteral("AWAY\r\n")), 1);
    QVERIFY(controller.selfAway());
}

void ControllerTest::statusQueryClosesStatus()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    IrcStatusConsole *console = controller.console();
    console->setOpen(true);
    QVERIFY(console->isOpen());
    QVERIFY(console->submit(QStringLiteral("/query lena")));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("lena"));
    QVERIFY(!console->isOpen());
}

void ControllerTest::statusQueryChannelStaysOpen()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    IrcStatusConsole *console = controller.console();
    console->setOpen(true);
    QVERIFY(console->submit(QStringLiteral("/query #desktop")));
    QVERIFY(console->isOpen());
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QVERIFY(logContains(console->lines(), QStringLiteral("Command was refused")));
}

void ControllerTest::statusSubmitLastAcceptedFollowsOutcome()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    IrcStatusConsole *console = controller.console();
    QVERIFY(console->lastSubmitAccepted());
    QVERIFY(console->submit(QStringLiteral("/query")));
    QVERIFY(!console->lastSubmitAccepted());
    QVERIFY(logContains(console->lines(), QStringLiteral("Command was refused")));
    QVERIFY(console->submit(QStringLiteral("/query lena")));
    QVERIFY(console->lastSubmitAccepted());
}

void ControllerTest::disconnectedQuerySelectsBareNotText()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    transport->remoteClose();
    QVERIFY(session->state() != IrcSession::State::Registered);

    QVERIFY(controller.sendMessage(QStringLiteral("/query lena")));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("lena"));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QVERIFY(!controller.sendMessage(QStringLiteral("/query lena hello")));
    QCOMPARE(controller.lastError(), QStringLiteral("Not connected"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
}

void ControllerTest::statusQueryWithNoNetworkDoesNotCrash()
{
    IrcController empty;
    QVERIFY(empty.console()->submit(QStringLiteral("/query")));
    QVERIFY(empty.console()->submit(QStringLiteral("/query lena")));
}

void ControllerTest::conversationClearWipesMessages()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("hello")));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(messages->rowCount() > 0);
    QVERIFY(rowForTarget(conversations, QStringLiteral("#omarchy")) >= 0);
    const int framesBefore = transport->writtenFrames().size();

    QVERIFY(controller.sendMessage(QStringLiteral("/clear")));
    QCOMPARE(controller.lastError(), QString());
    QCOMPARE(messages->rowCount(), 0);
    QVERIFY(rowForTarget(conversations, QStringLiteral("#omarchy")) >= 0);
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QCOMPARE(transport->writtenFrames().size(), framesBefore);
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("PART")));
    QVERIFY(!framesContain(transport->writtenFrames(), QByteArrayLiteral("QUIT")));

    QVERIFY(controller.sendMessage(QStringLiteral("/clear")));
    QCOMPARE(controller.lastError(), QString());
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QCOMPARE(messages->rowCount(), 0);
}

void ControllerTest::ghostClearIsSent()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    controller.openDirectMessage(QStringLiteral("ghost"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("ghost"));
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QVERIFY(rowForTarget(conversations, QStringLiteral("ghost")) >= 0);

    QVERIFY(controller.sendMessage(QStringLiteral("/clear")));
    QCOMPARE(controller.lastError(), QString());
    QCOMPARE(controller.selectedTarget(), QStringLiteral("ghost"));
    QVERIFY(rowForTarget(conversations, QStringLiteral("ghost")) >= 0);
}

void ControllerTest::statusClearLeavesConversationMessages()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("hello")));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    const int conversationRows = messages->rowCount();
    QVERIFY(conversationRows > 0);
    IrcStatusConsole *console = controller.console();
    QVERIFY(console->lines()->rowCount() > 0);

    QVERIFY(console->submit(QStringLiteral("/clear")));
    QCOMPARE(console->lines()->rowCount(), 0);
    QCOMPARE(messages->rowCount(), conversationRows);
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
}

void ControllerTest::selectedPrivmsgInsertsMessageRow()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :omairc Alice\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    const int rowsAfterJoin = messages->rowCount();
    QVERIFY(rowsAfterJoin > 0);

    QSignalSpy resets(messages, &QAbstractItemModel::modelReset);
    QSignalSpy inserts(messages, &QAbstractItemModel::rowsInserted);
    transport->injectBytes(
        QByteArrayLiteral(":Alice!u@h PRIVMSG #omarchy :hello\r\n"));

    QCOMPARE(resets.size(), 0);
    QCOMPARE(inserts.size(), 1);
    QCOMPARE(inserts.at(0).at(1).toInt(), rowsAfterJoin);
    QCOMPARE(inserts.at(0).at(2).toInt(), rowsAfterJoin);
    QCOMPARE(messages->rowCount(), rowsAfterJoin + 1);
    QCOMPARE(roleAt(messages, rowsAfterJoin, MessageListModel::BodyRole),
             QStringLiteral("hello"));

    QVERIFY(controller.sendMessage(QStringLiteral("own line")));
    QCOMPARE(resets.size(), 0);
    QCOMPARE(inserts.size(), 2);
    QCOMPARE(messages->rowCount(), rowsAfterJoin + 2);
    QCOMPARE(roleAt(messages, rowsAfterJoin + 1, MessageListModel::BodyRole),
             QStringLiteral("own line"));
}

void ControllerTest::largeChannelJoinDoesNotResetModelsPerNick()
{
    constexpr int nickCount = 400;
    constexpr int perLine = 20;
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :away-notify\r\n"
                          ":server CAP omairc ACK :away-notify\r\n"
                          ":server 001 omairc :Welcome\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    auto *members = qobject_cast<QAbstractItemModel *>(controller.members());
    QSignalSpy conversationResets(conversations, &QAbstractItemModel::modelReset);
    QSignalSpy conversationDataChanges(conversations, &QAbstractItemModel::dataChanged);
    QSignalSpy messageResets(messages, &QAbstractItemModel::modelReset);
    QSignalSpy memberResets(members, &QAbstractItemModel::modelReset);
    QSignalSpy memberDataChanges(members, &QAbstractItemModel::dataChanged);

    transport->injectBytes(namesBurst(nickCount, perLine));
    QCOMPARE(members->rowCount(), nickCount);
    QCOMPARE(controller.peopleCount(), nickCount);
    QCOMPARE(controller.peopleCount(), members->rowCount());
    QVERIFY(controller.hasAwayPresence());
    QVERIFY(transport->writtenFrames().contains(QByteArrayLiteral("WHO #big\r\n")));
    QCOMPARE(conversationResets.size(), 1);
    QCOMPARE(conversationDataChanges.size(), 1);
    QCOMPARE(messageResets.size(), 1);
    QCOMPARE(memberResets.size(), 1);

    const int memberResetsAfterNames = memberResets.size();
    const int conversationResetsAfterNames = conversationResets.size();
    const int messageResetsAfterNames = messageResets.size();
    memberDataChanges.clear();
    transport->injectBytes(whoBurst(nickCount));
    QCOMPARE(roleAt(members, 0, MemberListModel::AwayRole), true);
    QCOMPARE(roleAt(members, nickCount - 1, MemberListModel::AwayRole), true);
    QCOMPARE(memberResets.size(), memberResetsAfterNames);
    QCOMPARE(conversationResets.size(), conversationResetsAfterNames);
    QCOMPARE(messageResets.size(), messageResetsAfterNames);
    QCOMPARE(memberDataChanges.size(), nickCount);

    QSignalSpy memberInserts(members, &QAbstractItemModel::rowsInserted);
    QSignalSpy memberRemoves(members, &QAbstractItemModel::rowsRemoved);
    transport->injectBytes(QByteArrayLiteral(":zoe!u@h JOIN :#big\r\n"));
    QCOMPARE(members->rowCount(), nickCount + 1);
    QCOMPARE(controller.peopleCount(), nickCount + 1);
    QCOMPARE(memberResets.size(), memberResetsAfterNames);
    QCOMPARE(conversationResets.size(), conversationResetsAfterNames);
    QCOMPARE(messageResets.size(), messageResetsAfterNames);
    QCOMPARE(memberInserts.size(), 1);
    QCOMPARE(roleAt(members, memberInserts.at(0).at(1).toInt(),
                    MemberListModel::NickRole),
             QStringLiteral("zoe"));

    transport->injectBytes(QByteArrayLiteral(":zoe!u@h PART #big\r\n"));
    QCOMPARE(members->rowCount(), nickCount);
    QCOMPARE(controller.peopleCount(), nickCount);
    QCOMPARE(memberResets.size(), memberResetsAfterNames);
    QCOMPARE(memberRemoves.size(), 1);
}

void ControllerTest::isupportBurstDoesNotResetModels()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :omairc Alice\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QVERIFY(controller.isChannel());

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    auto *members = qobject_cast<QAbstractItemModel *>(controller.members());
    QSignalSpy conversationResets(conversations, &QAbstractItemModel::modelReset);
    QSignalSpy messageResets(messages, &QAbstractItemModel::modelReset);
    QSignalSpy memberResets(members, &QAbstractItemModel::modelReset);

    transport->injectBytes(
        QByteArrayLiteral(":server 005 omairc CHANTYPES=# PREFIX=(v)+ "
                          ":are supported by this server\r\n"
                          ":server 005 omairc CHANMODES=eIbq,k,flj,CFL "
                          ":are supported by this server\r\n"
                          ":server 005 omairc NICKLEN=16 "
                          ":are supported by this server\r\n"));
    QCOMPARE(conversationResets.size(), 0);
    QCOMPARE(messageResets.size(), 0);
    QCOMPARE(memberResets.size(), 0);

    transport->injectBytes(QByteArrayLiteral(":op!u@h MODE #omarchy +o Alice\r\n"));
    QCOMPARE(memberResets.size(), 0);
    QCOMPARE(roleAt(members, 0, MemberListModel::NickRole), QStringLiteral("Alice"));
    QCOMPARE(roleAt(members, 0, MemberListModel::LabelRole), QStringLiteral("Alice"));

    transport->injectBytes(
        QByteArrayLiteral(":server 005 omairc CHANTYPES=$ "
                          ":are supported by this server\r\n"));
    QCOMPARE(conversationResets.size(), 0);
    QCOMPARE(messageResets.size(), 0);
    QCOMPARE(memberResets.size(), 0);
    QVERIFY(!controller.isChannel());

    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :$odd\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("$odd"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("$odd"));
    QVERIFY(controller.isChannel());
}

void ControllerTest::otherChannelNamesDoesNotSnapshotJoiningMembers()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :away-notify\r\n"
                          ":server CAP omairc ACK :away-notify\r\n"
                          ":server 001 omairc :Welcome\r\n"));

    auto *members = qobject_cast<QAbstractItemModel *>(controller.members());
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#big\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#big"));
    QCOMPARE(members->rowCount(), 1);
    QCOMPARE(controller.peopleCount(), 1);
    QCOMPARE(controller.peopleCount(), members->rowCount());
    QCOMPARE(roleAt(members, 0, MemberListModel::NickRole), QStringLiteral("omairc"));

    QSignalSpy memberResets(members, &QAbstractItemModel::modelReset);
    QSignalSpy selection(&controller, &IrcController::selectionChanged);
    transport->injectBytes(
        QByteArrayLiteral(":server 353 omairc = #big :n0000 n0001\r\n"
                          ":omairc!u@h JOIN :#other\r\n"
                          ":server 353 omairc = #other :omairc\r\n"
                          ":server 366 omairc #other :End of NAMES\r\n"));

    QCOMPARE(members->rowCount(), 1);
    QCOMPARE(controller.peopleCount(), members->rowCount());
    QCOMPARE(controller.peopleCount(), 1);
    QCOMPARE(selection.size(), 0);
    QCOMPARE(memberResets.size(), 0);

    transport->injectBytes(
        QByteArrayLiteral(":n0000!u@h PRIVMSG #big :while-other-ended\r\n"));
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::BodyRole),
             QStringLiteral("while-other-ended"));
    QCOMPARE(members->rowCount(), 1);
    QCOMPARE(controller.peopleCount(), members->rowCount());
    QCOMPARE(memberResets.size(), 0);

    transport->injectBytes(
        QByteArrayLiteral(":server 366 omairc #big :End of NAMES\r\n"));
    QCOMPARE(members->rowCount(), 2);
    QCOMPARE(controller.peopleCount(), 2);
    QCOMPARE(controller.peopleCount(), members->rowCount());
    QCOMPARE(memberResets.size(), 0);
    QVERIFY(selection.size() >= 1);
}

void ControllerTest::namesBurstFlushesTypingClearedByChat()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :message-tags\r\n"
                          ":server CAP omairc ACK :message-tags\r\n"
                          ":server 001 omairc :Welcome\r\n"));

    QSignalSpy typing(&controller, &IrcController::typingChanged);
    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#big\r\n"
                          ":server 353 omairc = #big :alice\r\n"
                          "@+typing=active :alice!u@h TAGMSG #big\r\n"));
    QCOMPARE(controller.typingNicks(), QStringList{QStringLiteral("alice")});
    QVERIFY(typing.size() >= 1);

    transport->injectBytes(
        QByteArrayLiteral(":alice!u@h PRIVMSG #big :hello\r\n"));
    QVERIFY(controller.typingNicks().isEmpty());
    const int typingBeforeNamesEnd = typing.size();

    transport->injectBytes(
        QByteArrayLiteral(":server 366 omairc #big :End of NAMES\r\n"));
    QVERIFY(controller.typingNicks().isEmpty());
    QVERIFY(typing.size() > typingBeforeNamesEnd);
}

void ControllerTest::chatDuringNamesUpdatesMessagesWithoutMemberReset()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :away-notify\r\n"
                          ":server CAP omairc ACK :away-notify\r\n"
                          ":server 001 omairc :Welcome\r\n"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    auto *members = qobject_cast<QAbstractItemModel *>(controller.members());
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#big\r\n"));
    QCOMPARE(controller.peopleCount(), members->rowCount());
    QCOMPARE(members->rowCount(), 1);

    QSignalSpy memberResets(members, &QAbstractItemModel::modelReset);
    transport->injectBytes(
        QByteArrayLiteral(":server 353 omairc = #big :omairc n0000 n0001 alice\r\n"));
    QCOMPARE(controller.peopleCount(), members->rowCount());
    QCOMPARE(members->rowCount(), 1);
    QCOMPARE(memberResets.size(), 0);

    const int rowsAfterJoin = messages->rowCount();
    transport->injectBytes(
        QByteArrayLiteral(":alice!u@h PRIVMSG #big :during-names\r\n"));
    QCOMPARE(messages->rowCount(), rowsAfterJoin + 1);
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::BodyRole),
             QStringLiteral("during-names"));
    QCOMPARE(controller.peopleCount(), members->rowCount());
    QCOMPARE(memberResets.size(), 0);

    QVERIFY(controller.sendMessage(QStringLiteral("local-during-names")));
    QCOMPARE(messages->rowCount(), rowsAfterJoin + 2);
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::BodyRole),
             QStringLiteral("local-during-names"));
    QCOMPARE(controller.peopleCount(), members->rowCount());
    QCOMPARE(memberResets.size(), 0);

    transport->injectBytes(
        QByteArrayLiteral(":server 366 omairc #big :End of NAMES\r\n"));
    QCOMPARE(members->rowCount(), 4);
    QCOMPARE(controller.peopleCount(), 4);
    QCOMPARE(controller.peopleCount(), members->rowCount());
    QCOMPARE(memberResets.size(), 0);
}

void ControllerTest::incomingNickRetargetsSelectedDirect()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :omairc Alice\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"
                          ":Alice!u@h PRIVMSG omairc :hi\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("Alice"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("Alice"));
    QCOMPARE(controller.currentNick(), QStringLiteral("omairc"));

    transport->injectBytes(QByteArrayLiteral(":Alice!u@h NICK :Alicia\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QCOMPARE(controller.selectedTarget(), QStringLiteral("Alicia"));
    QVERIFY(rowForTarget(conversations, QStringLiteral("Alice")) < 0);
    QVERIFY(rowForTarget(conversations, QStringLiteral("Alicia")) >= 0);
    QCOMPARE(messages->rowCount(), 2);
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::KindRole),
             QStringLiteral("event"));
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::BodyRole),
             QStringLiteral("Alice is now Alicia"));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *members = qobject_cast<QAbstractItemModel *>(controller.members());
    QCOMPARE(roleAt(members, 0, MemberListModel::NickRole), QStringLiteral("Alicia"));
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::BodyRole),
             QStringLiteral("Alice is now Alicia"));
    QCOMPARE(messages->rowCount(), 2);
    QCOMPARE(roleAt(messages, 0, MessageListModel::BodyRole),
             QStringLiteral("omairc joined"));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("Alicia"));
    QVERIFY(controller.sendMessage(QStringLiteral("hello")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG Alicia :hello\r\n"));

    transport->injectBytes(QByteArrayLiteral(":omairc!u@h NICK :fred\r\n"));
    QCOMPARE(session->nick(), QStringLiteral("fred"));
    QCOMPARE(controller.currentNick(), QStringLiteral("fred"));
}

void ControllerTest::incomingNickCaseOnlyRetargetsDirect()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":Alice!u@h PRIVMSG omairc :hi\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("Alice"));

    transport->injectBytes(QByteArrayLiteral(":Alice!u@h NICK :ALICE\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QCOMPARE(controller.selectedTarget(), QStringLiteral("ALICE"));
    QVERIFY(rowForTarget(conversations, QStringLiteral("ALICE")) >= 0);
    QVERIFY(controller.sendMessage(QStringLiteral("hello")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG ALICE :hello\r\n"));
}

void ControllerTest::welcomeAssignedNickRoutesDirectMessages()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSessionConfig sessionConfig = config(QStringLiteral("libera"));
    sessionConfig.nick = QStringLiteral("omairc-very-long-name");
    IrcSession *session = controller.addSession(sessionConfig, transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc-very-long-name LS :multi-prefix\r\n"
                          ":server 001 omairc-truncated :Welcome\r\n"
                          ":Alice!u@h PRIVMSG omairc-truncated :hi\r\n"
                          ":Bob!u@h PRIVMSG omairc-very-long-name :nope\r\n"));
    QCOMPARE(session->nick(), QStringLiteral("omairc-truncated"));
    QCOMPARE(controller.currentNick(), QStringLiteral("omairc-truncated"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("Alice"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QVERIFY(rowForTarget(conversations, QStringLiteral("Alice")) >= 0);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("Bob")), -1);
}

void ControllerTest::echoIfPresentUsesAssignedNick()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSessionConfig sessionConfig = config(QStringLiteral("libera"));
    sessionConfig.nick = QStringLiteral("omairc-very-long-name");
    IrcSession *session = controller.addSession(sessionConfig, transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc-very-long-name LS :multi-prefix\r\n"
                          ":server 001 omairc-truncated :Welcome\r\n"
                          ":omairc-truncated!u@h JOIN :#omarchy\r\n"
                          ":lena!u@h PRIVMSG omairc-truncated :hi\r\n"));
    QCOMPARE(session->nick(), QStringLiteral("omairc-truncated"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/msg lena later")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG lena :later\r\n"));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(messages->rowCount() > 0);
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::BodyRole),
             QStringLiteral("later"));
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::AuthorRole),
             QStringLiteral("omairc-truncated"));
}

void ControllerTest::echoMessageAckSkipsLocalPrivmsg()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :echo-message\r\n"
                          ":server CAP omairc ACK :echo-message\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :omairc Alice\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));
    QVERIFY(session->capabilities().contains(IrcCapability::EchoMessage));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    const int rowsAfterJoin = messages->rowCount();
    QVERIFY(rowsAfterJoin > 0);

    QVERIFY(controller.sendMessage(QStringLiteral("hello")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG #omarchy :hello\r\n"));
    QCOMPARE(messages->rowCount(), rowsAfterJoin);

    QVERIFY(controller.sendMessage(QStringLiteral("/me waves")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArray("PRIVMSG #omarchy :\x01" "ACTION waves\x01\r\n"));
    QCOMPARE(messages->rowCount(), rowsAfterJoin);

    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h PRIVMSG #omarchy :hello\r\n"));
    QCOMPARE(messages->rowCount(), rowsAfterJoin + 1);
    QCOMPARE(roleAt(messages, rowsAfterJoin, MessageListModel::BodyRole),
             QStringLiteral("hello"));
    QCOMPARE(roleAt(messages, rowsAfterJoin, MessageListModel::AuthorRole),
             QStringLiteral("omairc"));
}

void ControllerTest::echoMessageAbsentStillEchoesLocally()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :echo-message\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :omairc Alice\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));
    QVERIFY(!session->capabilities().contains(IrcCapability::EchoMessage));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    const int rowsAfterJoin = messages->rowCount();

    QVERIFY(controller.sendMessage(QStringLiteral("hello")));
    QCOMPARE(messages->rowCount(), rowsAfterJoin + 1);
    QCOMPARE(roleAt(messages, rowsAfterJoin, MessageListModel::BodyRole),
             QStringLiteral("hello"));
    QCOMPARE(roleAt(messages, rowsAfterJoin, MessageListModel::AuthorRole),
             QStringLiteral("omairc"));
}

void ControllerTest::echoMessageAckSkipsMsgEcho()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :echo-message\r\n"
                          ":server CAP omairc ACK :echo-message\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":lena!u@h PRIVMSG omairc :hi\r\n"));
    QVERIFY(session->capabilities().contains(IrcCapability::EchoMessage));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    const int rowsBeforeMsg = messages->rowCount();

    QVERIFY(controller.sendMessage(QStringLiteral("/msg lena later")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG lena :later\r\n"));
    QCOMPARE(messages->rowCount(), rowsBeforeMsg);

    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h PRIVMSG lena :later\r\n"));
    QCOMPARE(messages->rowCount(), rowsBeforeMsg + 1);
    QCOMPARE(roleAt(messages, messages->rowCount() - 1, MessageListModel::BodyRole),
             QStringLiteral("later"));
}

void ControllerTest::longMeAndNoticeSplitAcrossFrames()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    const QString first(400, QLatin1Char('a'));
    const QString second(200, QLatin1Char('b'));
    const QString body = first + QLatin1Char(' ') + second;

    const int beforeAction = transport->writtenFrames().size();
    QVERIFY(controller.sendMessage(QStringLiteral("/me ") + body));
    const QByteArrayList actionFrames =
        transport->writtenFrames().mid(beforeAction);
    QCOMPARE(actionFrames.size(), 2);
    QCOMPARE(actionFrames.at(0),
             QByteArray("PRIVMSG #omarchy :\x01" "ACTION ")
                 + first.toLatin1() + QByteArray("\x01\r\n"));
    QCOMPARE(actionFrames.at(1),
             QByteArray("PRIVMSG #omarchy :\x01" "ACTION ")
                 + second.toLatin1() + QByteArray("\x01\r\n"));
    for (const QByteArray& frame : actionFrames) {
        QVERIFY(frame.endsWith("\r\n"));
        QVERIFY(frame.size() <= int(IrcProtocol::maxClassicFrameBytes));
    }

    const int beforeNotice = transport->writtenFrames().size();
    QVERIFY(controller.sendMessage(QStringLiteral("/notice #omarchy ") + body));
    const QByteArrayList noticeFrames =
        transport->writtenFrames().mid(beforeNotice);
    QCOMPARE(noticeFrames.size(), 2);
    QCOMPARE(noticeFrames.at(0),
             QByteArrayLiteral("NOTICE #omarchy :") + first.toLatin1()
                 + QByteArrayLiteral("\r\n"));
    QCOMPARE(noticeFrames.at(1),
             QByteArrayLiteral("NOTICE #omarchy :") + second.toLatin1()
                 + QByteArrayLiteral("\r\n"));
    for (const QByteArray& frame : noticeFrames) {
        QVERIFY(frame.endsWith("\r\n"));
        QVERIFY(frame.size() <= int(IrcProtocol::maxClassicFrameBytes));
    }
}

void ControllerTest::longTopicStillRefuses()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    const int before = transport->writtenFrames().size();
    QVERIFY(!controller.sendMessage(QStringLiteral("/topic ")
                                    + QString(600, QLatin1Char('t'))));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QCOMPARE(transport->writtenFrames().size(), before);
    QVERIFY(!framesContain(transport->writtenFrames().mid(before),
                           QByteArrayLiteral("TOPIC")));
}

void ControllerTest::echoMessageLongPrivmsgShowsEachChunkOnce()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :echo-message\r\n"
                          ":server CAP omairc ACK :echo-message\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :omairc Alice\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));
    QVERIFY(session->capabilities().contains(IrcCapability::EchoMessage));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    const int rowsAfterJoin = messages->rowCount();

    const QString first(400, QLatin1Char('a'));
    const QString second(200, QLatin1Char('b'));
    const QString body = first + QLatin1Char(' ') + second;
    QVERIFY(controller.sendMessage(body));
    QCOMPARE(messages->rowCount(), rowsAfterJoin);

    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h PRIVMSG #omarchy :") + first.toLatin1()
        + QByteArrayLiteral("\r\n")
        + QByteArrayLiteral(":omairc!u@h PRIVMSG #omarchy :") + second.toLatin1()
        + QByteArrayLiteral("\r\n"));
    QCOMPARE(messages->rowCount(), rowsAfterJoin + 2);
    QCOMPARE(roleAt(messages, rowsAfterJoin, MessageListModel::BodyRole), first);
    QCOMPARE(roleAt(messages, rowsAfterJoin + 1, MessageListModel::BodyRole), second);
    QCOMPARE(roleAt(messages, rowsAfterJoin, MessageListModel::AuthorRole),
             QStringLiteral("omairc"));
    QCOMPARE(roleAt(messages, rowsAfterJoin + 1, MessageListModel::AuthorRole),
             QStringLiteral("omairc"));
}

void ControllerTest::msgEchoDoesNotOpenMissingDirect()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :echo-message\r\n"
                          ":server CAP omairc ACK :echo-message\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    QVERIFY(session->capabilities().contains(IrcCapability::EchoMessage));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/msg lena hello")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG lena :hello\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h PRIVMSG lena :hello\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("lena")), -1);

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    for (int row = 0; row < messages->rowCount(); ++row) {
        QVERIFY(roleAt(messages, row, MessageListModel::BodyRole)
                != QStringLiteral("hello"));
    }
}

void ControllerTest::incomingNickservPrivmsgDoesNotOpenDirect()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":NickServ!NickServ@services PRIVMSG omairc "
                          ":This nickname is registered.\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("NickServ")), -1);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("nickserv")), -1);
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("This nickname is registered.")));
}

void ControllerTest::bouncerAttachOpensOnlyPeerAuthoredDirects()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch echo-message\r\n"
                          ":server CAP omairc ACK :batch echo-message\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    QVERIFY(session->capabilities().contains(IrcCapability::Batch));
    QVERIFY(session->capabilities().contains(IrcCapability::EchoMessage));
    QVERIFY(!session->capabilities().contains(IrcCapability::ChatHistory));

    IrcStatusConsole *console = controller.console();
    console->setOpen(true);
    QVERIFY(console->submit(QStringLiteral("/msg lena hi")));
    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h PRIVMSG lena :hi\r\n"
                          ":NickServ!NickServ@services PRIVMSG omairc "
                          ":This nickname is registered.\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("lena")), -1);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("NickServ")), -1);

    transport->injectBytes(
        QByteArrayLiteral(":znc.in BATCH +q1 znc.in/playback lena\r\n"
                          "@batch=q1 :omairc!u@h PRIVMSG lena :hi\r\n"
                          ":znc.in BATCH -q1\r\n"));
    QCOMPARE(rowForTarget(conversations, QStringLiteral("lena")), -1);

    transport->injectBytes(
        QByteArrayLiteral(":znc.in BATCH +q2 znc.in/playback dana\r\n"
                          "@batch=q2 :dana!u@h PRIVMSG omairc :morning\r\n"
                          "@batch=q2 :omairc!u@h PRIVMSG dana :morning back\r\n"
                          ":znc.in BATCH -q2\r\n"));
    QVERIFY(rowForTarget(conversations, QStringLiteral("dana")) >= 0);

    transport->injectBytes(
        QByteArrayLiteral(":rio!u@h PRIVMSG omairc :live hello\r\n"));
    QVERIFY(rowForTarget(conversations, QStringLiteral("rio")) >= 0);

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("dana"));
    auto *danaMessages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QCOMPARE(selectedBodies(danaMessages),
             QStringList({QStringLiteral("morning"),
                          QStringLiteral("morning back")}));
    QCOMPARE(roleAt(danaMessages, 0, MessageListModel::OriginRole).toString(),
             QStringLiteral("replay"));
    QCOMPARE(roleAt(danaMessages, 1, MessageListModel::OriginRole).toString(),
             QStringLiteral("replay"));

    transport->injectBytes(
        QByteArrayLiteral(
            ":znc.in BATCH +c znc.in/playback #omarchy\r\n"
            "@batch=c :***!znc@znc.in PRIVMSG #omarchy :Buffer Playback...\r\n"
            "@batch=c :lena!u@h PRIVMSG #omarchy :yesterday\r\n"
            "@batch=c :***!znc@znc.in PRIVMSG #omarchy :Playback Complete.\r\n"
            ":znc.in BATCH -c\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *roomMessages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QCOMPARE(selectedBodies(roomMessages),
             QStringList({QStringLiteral("yesterday"),
                          QStringLiteral("omairc joined")}));

    transport->injectBytes(
        QByteArrayLiteral(":server 376 omairc :End of MOTD\r\n"));
    QCOMPARE(rowForTarget(conversations, QStringLiteral("lena")), -1);
    QVERIFY(rowForTarget(conversations, QStringLiteral("dana")) >= 0);
    const IrcCaseMapping mapping;
    QVERIFY(IrcOpenDirectStore().listed(QStringLiteral("libera"), mapping)
                .contains(QStringLiteral("dana")));
    QVERIFY(!IrcOpenDirectStore().listed(QStringLiteral("libera"), mapping)
                 .contains(QStringLiteral("lena")));
}

void ControllerTest::keptSelfReplayRemembersOpenQuery()
{
    const IrcCaseMapping mapping;
    const QByteArray caps =
        QByteArrayLiteral(":server CAP omairc LS :batch znc.in/playback\r\n"
                          ":server CAP omairc ACK :batch znc.in/playback\r\n");
    {
        IrcController controller;
        auto *transport = new FakeIrcTransport;
        QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
        QVERIFY(controller.start(QStringLiteral("libera")));
        transport->completeConnect();
        transport->injectBytes(caps);
        transport->injectBytes(QByteArrayLiteral(
            ":server 001 omairc :Welcome\r\n"
            ":lena!u@h PRIVMSG omairc :ping\r\n"));

        auto *conversations =
            qobject_cast<QAbstractItemModel *>(controller.conversations());
        QVERIFY(conversations);
        QVERIFY(rowForTarget(conversations, QStringLiteral("lena")) >= 0);
        QVERIFY(!IrcOpenDirectStore().listed(QStringLiteral("libera"), mapping)
                     .contains(QStringLiteral("lena")));

        transport->injectBytes(QByteArrayLiteral(
            ":znc.in BATCH +q znc.in/playback lena\r\n"
            "@batch=q;time=2024-03-09T16:00:00.620Z :omairc!u@h PRIVMSG lena :pong\r\n"
            ":znc.in BATCH -q\r\n"));
        QVERIFY(IrcOpenDirectStore().listed(QStringLiteral("libera"), mapping)
                    .contains(QStringLiteral("lena")));
        controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
        auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
        QVERIFY(messages);
        const int pong = bodyRow(messages, QStringLiteral("pong"));
        QVERIFY(pong >= 0);
        QCOMPARE(roleAt(messages, pong, MessageListModel::OriginRole),
                 QStringLiteral("replay"));
    }

    IrcController again;
    auto *transport = new FakeIrcTransport;
    QVERIFY(again.addSession(config(QStringLiteral("libera")), transport));
    QVERIFY(again.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(caps);
    transport->injectBytes(QByteArrayLiteral(":server 001 omairc :Welcome\r\n"));
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(again.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("lena")), -1);

    transport->injectBytes(QByteArrayLiteral(":server 376 omairc :End of MOTD\r\n"));
    QVERIFY(rowForTarget(conversations, QStringLiteral("lena")) >= 0);
    QVERIFY(IrcOpenDirectStore().listed(QStringLiteral("libera"), mapping)
                .contains(QStringLiteral("lena")));
}

void ControllerTest::bouncerQueryReplayDirectAppearsInConversationModel()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch\r\n"
                          ":server CAP omairc ACK :batch\r\n"
                          ":server 001 omairc :Welcome\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(conversations->rowCount(), 0);

    transport->injectBytes(
        QByteArrayLiteral(":znc.in BATCH +q znc.in/playback dana\r\n"
                          "@batch=q :dana!u@h PRIVMSG omairc :morning\r\n"
                          ":znc.in BATCH -q\r\n"));

    const int row = rowForTarget(conversations, QStringLiteral("dana"));
    QVERIFY(row >= 0);
    QCOMPARE(roleAt(conversations, row, ConversationListModel::ConversationRole)
                 .toString(),
             QStringLiteral("dana"));
    QCOMPARE(roleAt(conversations, row, ConversationListModel::DirectRole).toBool(),
             true);
    QCOMPARE(roleAt(conversations, row, ConversationListModel::UnreadRole).toInt(), 0);
}

void ControllerTest::pseudoClientPrivmsgOpensNoConversation()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":*status!znc@znc.in PRIVMSG omairc "
                          ":You have 1 network attached\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(conversations->rowCount(), 0);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("*status")), -1);
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("You have 1 network attached")));
}

void ControllerTest::zncQuietSendDoesNotOpenDirect()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :echo-message\r\n"
                          ":server CAP omairc ACK :echo-message\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    QVERIFY(session->capabilities().contains(IrcCapability::EchoMessage));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    IrcStatusConsole *console = controller.console();
    console->setOpen(true);
    QVERIFY(controller.sendMessage(QStringLiteral("/znc ListMods")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG *status :ListMods\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h PRIVMSG *status :ListMods\r\n"
                          ":*status!znc@znc.in PRIVMSG omairc :Modules: playback\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("*status")), -1);
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QVERIFY(logContains(console->lines(), QStringLiteral("Modules: playback")));
}

void ControllerTest::pseudoClientReplayBatchOpensNoConversation()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch echo-message\r\n"
                          ":server CAP omairc ACK :batch echo-message\r\n"
                          ":server 001 omairc :Welcome\r\n"));

    // A bouncer keeps a query buffer for its own module, and our own commands
    // to it are echoed back inside that buffer. The sender guard cannot drop
    // those lines, because we are a routable sender, so creation has to fail
    // on the missing peer author instead.
    transport->injectBytes(
        QByteArrayLiteral(":znc.in BATCH +s znc.in/playback *status\r\n"
                          "@batch=s :omairc!u@h PRIVMSG *status :listnetworks\r\n"
                          "@batch=s :*status!znc@znc.in PRIVMSG omairc :Network: libera\r\n"
                          ":znc.in BATCH -s\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("*status")), -1);
    QCOMPARE(conversations->rowCount(), 0);
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("Network: libera")));
}

void ControllerTest::routableNicksOpenDirectsAndPseudoClientsDoNot()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":server 005 omairc CHANTYPES=# "
                          ":are supported by this server\r\n"
                          ":0day!u@h PRIVMSG omairc :one\r\n"
                          ":[bob]!u@h PRIVMSG omairc :two\r\n"
                          ":nick_!u@h PRIVMSG omairc :three\r\n"
                          ":{x}!u@h PRIVMSG omairc :four\r\n"
                          ":|away!u@h PRIVMSG omairc :five\r\n"
                          ":^bob^!u@h PRIVMSG omairc :six\r\n"
                          ":*status!znc@znc.in PRIVMSG omairc :seven\r\n"
                          ":*playback!znc@znc.in PRIVMSG omairc :eight\r\n"
                          ":***!znc@znc.in PRIVMSG omairc :nine\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QStringList opened;
    for (int row = 0; row < conversations->rowCount(); ++row) {
        opened.append(
            roleAt(conversations, row, ConversationListModel::ConversationRole)
                .toString());
    }
    opened.sort();
    QCOMPARE(opened,
             QStringList({QStringLiteral("0day"), QStringLiteral("[bob]"),
                          QStringLiteral("^bob^"), QStringLiteral("nick_"),
                          QStringLiteral("{x}"), QStringLiteral("|away")}));
}

void ControllerTest::conversationCreateMatrix()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :echo-message\r\n"
                          ":server CAP omairc ACK :echo-message\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    QVERIFY(session->capabilities().contains(IrcCapability::EchoMessage));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);

    QVERIFY(controller.sendMessage(QStringLiteral("/msg lena hello")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG lena :hello\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QCOMPARE(rowForTarget(conversations, QStringLiteral("lena")), -1);

    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h PRIVMSG lena :hello\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));
    QCOMPARE(rowForTarget(conversations, QStringLiteral("lena")), -1);

    transport->injectBytes(
        QByteArrayLiteral(":NickServ!NickServ@services PRIVMSG omairc "
                          ":This nickname is registered.\r\n"));
    QCOMPARE(rowForTarget(conversations, QStringLiteral("NickServ")), -1);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("nickserv")), -1);
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("This nickname is registered.")));

    transport->injectBytes(
        QByteArrayLiteral(":alice!u@h PRIVMSG omairc :hi\r\n"));
    QVERIFY(rowForTarget(conversations, QStringLiteral("alice")) >= 0);
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/query bob")));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("bob"));
    QVERIFY(rowForTarget(conversations, QStringLiteral("bob")) >= 0);
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("PRIVMSG bob")));
}

void ControllerTest::statusMsgNickservIdentifyDoesNotOpenDirect()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :echo-message\r\n"
                          ":server CAP omairc ACK :echo-message\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    QVERIFY(session->capabilities().contains(IrcCapability::EchoMessage));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    IrcStatusConsole *console = controller.console();
    console->setOpen(true);
    QVERIFY(console->submit(
        QStringLiteral("/msg nickserv identify my_nick s3cret")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG nickserv :identify my_nick s3cret\r\n"));
    QVERIFY(console->isOpen());
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    transport->injectBytes(
        QByteArrayLiteral(":NickServ!NickServ@services PRIVMSG omairc "
                          ":You are now identified for my_nick.\r\n"
                          ":omairc!u@h PRIVMSG nickserv :identify my_nick s3cret\r\n"));

    QVERIFY(console->isOpen());
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("NickServ")), -1);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("nickserv")), -1);

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("s3cret")));
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("identify my_nick")));
    QVERIFY(logContains(console->lines(), QStringLiteral("IDENTIFY ***")));
    QVERIFY(!logContains(console->lines(), QStringLiteral("s3cret")));
}

void ControllerTest::nsIdentifyDoesNotOpenDirectOrLeakSecret()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :echo-message\r\n"
                          ":server CAP omairc ACK :echo-message\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    QVERIFY(session->capabilities().contains(IrcCapability::EchoMessage));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    IrcStatusConsole *console = controller.console();
    console->setOpen(true);
    QVERIFY(controller.sendMessage(QStringLiteral("/ns identify hunter2")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG NickServ :identify hunter2\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    transport->injectBytes(
        QByteArrayLiteral(":NickServ!NickServ@services PRIVMSG omairc "
                          ":You are now identified.\r\n"
                          ":omairc!u@h PRIVMSG NickServ :identify hunter2\r\n"));

    QVERIFY(console->submit(QStringLiteral("/cs identify hunter2")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG ChanServ :identify hunter2\r\n"));
    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h PRIVMSG ChanServ :identify hunter2\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("NickServ")), -1);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("ChanServ")), -1);
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("hunter2")));
    QVERIFY(logContains(console->lines(), QStringLiteral("IDENTIFY ***")));
    QVERIFY(!logContains(console->lines(), QStringLiteral("hunter2")));
}

void ControllerTest::automaticIdentifyDoesNotOpenNickServDirect()
{
    IrcSessionConfig sessionConfig = config(QStringLiteral("libera"));
    sessionConfig.nickServPassword = QStringLiteral("nick-secret");
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(sessionConfig, transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :echo-message\r\n"
                          ":server CAP omairc ACK :echo-message\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":omairc!u@h PRIVMSG NickServ :IDENTIFY nick-secret\r\n"
                          ":NickServ!NickServ@services NOTICE omairc "
                          ":You are now identified.\r\n"));
    QVERIFY(transport->writtenFrames().contains(
        QByteArrayLiteral("PRIVMSG NickServ :IDENTIFY nick-secret\r\n")));
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("NickServ")), -1);
    QVERIFY(!logContains(controller.console()->lines(),
                         QStringLiteral("nick-secret")));
}

void ControllerTest::mentionArrivedOnSelectedBuffer()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    QSignalSpy spy(&controller, &IrcController::mentionArrived);
    transport->injectBytes(
        QByteArrayLiteral(":Alice!u@h PRIVMSG #omarchy :omairc: ping\r\n"));
    QCOMPARE(spy.count(), 1);
    QCOMPARE(spy.at(0).at(0).toString(), QStringLiteral("Alice"));
    QCOMPARE(spy.at(0).at(1).toString(), QStringLiteral("omairc: ping"));

    transport->injectBytes(
        QByteArrayLiteral(":Alice!u@h PRIVMSG #omarchy :hello\r\n"));
    QCOMPARE(spy.count(), 1);

    transport->injectBytes(
        QByteArrayLiteral(":omairc!u@h PRIVMSG #omarchy :omairc: self\r\n"));
    QCOMPARE(spy.count(), 1);

    transport->injectBytes(
        QByteArray(":Alice!u@h PRIVMSG #omarchy :\x01"
                   "ACTION pokes omairc\x01\r\n"));
    QCOMPARE(spy.count(), 2);
    QCOMPARE(spy.at(1).at(0).toString(), QStringLiteral("Alice"));
    QCOMPARE(spy.at(1).at(1).toString(), QStringLiteral("pokes omairc"));
}

void ControllerTest::mentionArrivedOnDirectMessageWithoutNick()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    QSignalSpy spy(&controller, &IrcController::mentionArrived);
    transport->injectBytes(
        QByteArrayLiteral(":Alice!u@h PRIVMSG omairc :hello\r\n"));
    QCOMPARE(spy.count(), 1);
    QCOMPARE(spy.at(0).at(0).toString(), QStringLiteral("Alice"));
    QCOMPARE(spy.at(0).at(1).toString(), QStringLiteral("hello"));

    transport->injectBytes(
        QByteArrayLiteral(":Alice!u@h PRIVMSG #omarchy :hello\r\n"));
    QCOMPARE(spy.count(), 1);

    transport->injectBytes(
        QByteArray(":Alice!u@h PRIVMSG omairc :\x01"
                   "ACTION waves\x01\r\n"));
    QCOMPARE(spy.count(), 2);
    QCOMPARE(spy.at(1).at(0).toString(), QStringLiteral("Alice"));
    QCOMPARE(spy.at(1).at(1).toString(), QStringLiteral("waves"));
}

void ControllerTest::mentionArrivedCarriesNetworkTargetAndMsgid()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    QSignalSpy spy(&controller, &IrcController::mentionArrived);
    transport->injectBytes(
        QByteArrayLiteral("@msgid=mid-1 :Alice!u@h PRIVMSG #omarchy :omairc: ping\r\n"));
    QCOMPARE(spy.count(), 1);
    QCOMPARE(spy.at(0).at(0).toString(), QStringLiteral("Alice"));
    QCOMPARE(spy.at(0).at(1).toString(), QStringLiteral("omairc: ping"));
    QCOMPARE(spy.at(0).at(2).toString(), QStringLiteral("libera"));
    QCOMPARE(spy.at(0).at(3).toString(), QStringLiteral("#omarchy"));
    QCOMPARE(spy.at(0).at(4).toString(), QStringLiteral("mid-1"));

    transport->injectBytes(
        QByteArrayLiteral("@msgid=dm-7 :Alice!u@h PRIVMSG omairc :hello\r\n"));
    QCOMPARE(spy.count(), 2);
    QCOMPARE(spy.at(1).at(0).toString(), QStringLiteral("Alice"));
    QCOMPARE(spy.at(1).at(1).toString(), QStringLiteral("hello"));
    QCOMPARE(spy.at(1).at(2).toString(), QStringLiteral("libera"));
    QCOMPARE(spy.at(1).at(3).toString(), QStringLiteral("Alice"));
    QCOMPARE(spy.at(1).at(4).toString(), QStringLiteral("dm-7"));
}

void ControllerTest::revealConversationSelectsExistingAndRecreatesClosedDirect()
{
    IrcController controller;
    auto *transportA = new FakeIrcTransport;
    auto *transportB = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("libera")), transportA));
    QVERIFY(controller.addSession(config(QStringLiteral("oftc")), transportB));
    QVERIFY(controller.start(QStringLiteral("libera")));
    QVERIFY(controller.start(QStringLiteral("oftc")));
    registerSession(controller.session(QStringLiteral("libera")), transportA);
    registerSession(controller.session(QStringLiteral("oftc")), transportB);
    transportA->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    transportB->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#lab\r\n"));
    controller.selectConversation(QStringLiteral("oftc"), QStringLiteral("#lab"));
    QCOMPARE(controller.selectedNetworkId(), QStringLiteral("oftc"));

    controller.revealConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QCOMPARE(controller.selectedNetworkId(), QStringLiteral("libera"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    transportA->injectBytes(
        QByteArrayLiteral(":Alice!u@h PRIVMSG omairc :hello\r\n"));
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QVERIFY(rowForTarget(conversations, QStringLiteral("Alice")) >= 0);

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("Alice"));
    controller.closeDirectMessage();
    QVERIFY(rowForTarget(conversations, QStringLiteral("Alice")) < 0);

    controller.selectConversation(QStringLiteral("oftc"), QStringLiteral("#lab"));
    QCOMPARE(controller.selectedNetworkId(), QStringLiteral("oftc"));
    controller.revealConversation(QStringLiteral("libera"), QStringLiteral("Alice"));
    QCOMPARE(controller.selectedNetworkId(), QStringLiteral("libera"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("Alice"));
    QVERIFY(rowForTarget(conversations, QStringLiteral("Alice")) >= 0);

    const QString stillAlice = controller.selectedTarget();
    controller.revealConversation(QStringLiteral("libera"), QStringLiteral("#missing"));
    QCOMPARE(controller.selectedTarget(), stillAlice);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("#missing")), -1);
}

void ControllerTest::chghostLeavesMemberNickAndRanks()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :chghost\r\n"
                          ":server CAP omairc ACK :chghost\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":server 005 omairc CHANTYPES=# PREFIX=(ov)@+ "
                          ":are supported by this server\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :@omairc +Alice\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));

    auto *members = qobject_cast<QAbstractItemModel *>(controller.members());
    QVERIFY(members);
    QCOMPARE(members->rowCount(), 2);
    QCOMPARE(roleAt(members, 1, MemberListModel::NickRole), QStringLiteral("Alice"));
    QCOMPARE(roleAt(members, 1, MemberListModel::LabelRole), QStringLiteral("+Alice"));

    transport->injectBytes(
        QByteArrayLiteral(":Alice!olduser@oldhost CHGHOST newuser newhost\r\n"
                          ":Alice!newuser@newhost CHGHOST\r\n"));
    QCOMPARE(members->rowCount(), 2);
    QCOMPARE(roleAt(members, 1, MemberListModel::NickRole), QStringLiteral("Alice"));
    QCOMPARE(roleAt(members, 1, MemberListModel::LabelRole), QStringLiteral("+Alice"));
}

void ControllerTest::twoSessionsStartTogether()
{
    IrcController controller;
    auto *transportA = new FakeIrcTransport;
    auto *transportB = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("network-a")), transportA));
    QVERIFY(controller.addSession(config(QStringLiteral("network-b")), transportB));
    QVERIFY(controller.start(QStringLiteral("network-a")));
    QVERIFY(controller.start(QStringLiteral("network-b")));
    QCOMPARE(transportA->connectionState(), IrcTransport::ConnectionState::Connecting);
    QCOMPARE(transportB->connectionState(), IrcTransport::ConnectionState::Connecting);

    registerSession(controller.session(QStringLiteral("network-a")), transportA);
    registerSession(controller.session(QStringLiteral("network-b")), transportB);
    transportA->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#alpha\r\n"));
    transportB->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#lab\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(conversations->rowCount(), 2);
    QCOMPARE(controller.session(QStringLiteral("network-a"))->state(),
             IrcSession::State::Registered);
    QCOMPARE(controller.session(QStringLiteral("network-b"))->state(),
             IrcSession::State::Registered);
}

void ControllerTest::startingBackgroundNetworkDoesNotStealStatus()
{
    IrcController controller;
    auto *transportA = new FakeIrcTransport;
    auto *transportB = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("network-a")), transportA));
    QVERIFY(controller.addSession(config(QStringLiteral("network-b")), transportB));
    QVERIFY(controller.start(QStringLiteral("network-a")));
    registerSession(controller.session(QStringLiteral("network-a")), transportA);
    transportA->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#chan\r\n"));
    controller.selectConversation(QStringLiteral("network-a"), QStringLiteral("#chan"));
    controller.openStatus(QStringLiteral("network-a"));
    QCOMPARE(controller.focusedNetworkId(), QStringLiteral("network-a"));
    QCOMPARE(controller.console()->networkId(), QStringLiteral("network-a"));
    QVERIFY(controller.console()->isOpen());

    QVERIFY(controller.start(QStringLiteral("network-b")));
    QCOMPARE(controller.focusedNetworkId(), QStringLiteral("network-a"));
    QCOMPARE(controller.console()->networkId(), QStringLiteral("network-a"));
    QVERIFY(controller.console()->isOpen());
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#chan"));
    QCOMPARE(controller.selectedNetworkId(), QStringLiteral("network-a"));
}

void ControllerTest::quitWhileReconnecting()
{
    IrcController controller;
    IrcSessionConfig sessionConfig = config(QStringLiteral("libera"));
    sessionConfig.reconnectEnabled = true;
    auto *transport = new FakeIrcTransport;
    auto *timer = new FakeReconnectTimer;
    IrcSession *session = controller.addSession(sessionConfig, transport, timer);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    registerSession(session, transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    transport->remoteClose();
    QCOMPARE(session->state(), IrcSession::State::Reconnecting);
    QVERIFY(timer->active);

    QVERIFY(controller.sendMessage(QStringLiteral("/quit")));
    QCOMPARE(session->state(), IrcSession::State::Idle);
    QVERIFY(!timer->active);
}

void ControllerTest::disconnectLeavesOtherNetworkLive()
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
    registerSession(sessionA, transportA);
    registerSession(sessionB, transportB);
    transportA->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#alpha\r\n"));
    transportB->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#lab\r\n"));
    controller.selectConversation(QStringLiteral("network-a"), QStringLiteral("#alpha"));

    QVERIFY(controller.sendMessage(QStringLiteral("/disconnect")));
    QCOMPARE(sessionA->state(), IrcSession::State::Idle);
    QCOMPARE(controller.session(QStringLiteral("network-a")), sessionA);
    QCOMPARE(controller.session(QStringLiteral("network-b")), sessionB);
    QCOMPARE(sessionB->state(), IrcSession::State::Registered);
    QCOMPARE(controller.connectionStatusFor(QStringLiteral("network-a")),
             QStringLiteral("Offline"));
    QVERIFY(framesContain(transportA->writtenFrames(), QByteArrayLiteral("QUIT")));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(conversations->rowCount(), 2);
    QVERIFY(rowForTarget(conversations, QStringLiteral("#alpha")) >= 0);
    QVERIFY(rowForTarget(conversations, QStringLiteral("#lab")) >= 0);

    controller.openStatus(QStringLiteral("network-a"));
    QVERIFY(controller.console()->isOpen());
    QCOMPARE(controller.console()->networkId(), QStringLiteral("network-a"));

    controller.selectConversation(QStringLiteral("network-b"), QStringLiteral("#lab"));
    transportB->injectBytes(QByteArrayLiteral(":zed!u@h PRIVMSG #lab :still here\r\n"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("still here")));
    QCOMPARE(sessionB->state(), IrcSession::State::Registered);
}

void ControllerTest::quitAliasIdlesFocusedNetwork()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    registerSession(session, transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/quit")));
    QCOMPARE(session->state(), IrcSession::State::Idle);
    QCOMPARE(controller.session(QStringLiteral("libera")), session);
    QCOMPARE(controller.conversations()->rowCount(), 1);
    QVERIFY(framesContain(transport->writtenFrames(), QByteArrayLiteral("QUIT")));
}

void ControllerTest::disconnectWhileIdleIsRefused()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    registerSession(session, transport);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("/disconnect")));
    QCOMPARE(session->state(), IrcSession::State::Idle);

    QVERIFY(!controller.sendMessage(QStringLiteral("/disconnect")));
    QCOMPARE(controller.lastError(), QStringLiteral("Command was refused"));
    QCOMPARE(session->state(), IrcSession::State::Idle);
    QCOMPARE(controller.session(QStringLiteral("libera")), session);
}

void ControllerTest::statusJoinUsesConsoleNetwork()
{
    IrcController controller;
    auto *transportA = new FakeIrcTransport;
    auto *transportB = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("network-a")), transportA));
    QVERIFY(controller.addSession(config(QStringLiteral("network-b")), transportB));
    registerSession(controller.session(QStringLiteral("network-a")), transportA);
    registerSession(controller.session(QStringLiteral("network-b")), transportB);
    transportA->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#chan\r\n"));
    controller.selectConversation(QStringLiteral("network-a"), QStringLiteral("#chan"));
    controller.openStatus(QStringLiteral("network-b"));
    QCOMPARE(controller.focusedNetworkId(), QStringLiteral("network-b"));
    QCOMPARE(controller.selectedNetworkId(), QStringLiteral("network-a"));

    QVERIFY(controller.console()->submit(QStringLiteral("/join #lab")));
    QCOMPARE(transportB->writtenFrames().last(),
             QByteArrayLiteral("JOIN #lab\r\n"));
    QVERIFY(!framesContain(transportA->writtenFrames(),
                           QByteArrayLiteral("JOIN #lab")));
}

void ControllerTest::implicitStatusPartStaysOnFocusedNetwork()
{
    IrcController controller;
    auto *transportA = new FakeIrcTransport;
    auto *transportB = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("network-a")), transportA));
    QVERIFY(controller.addSession(config(QStringLiteral("network-b")), transportB));
    registerSession(controller.session(QStringLiteral("network-a")), transportA);
    registerSession(controller.session(QStringLiteral("network-b")), transportB);
    transportA->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#chan\r\n"
                                              ":omairc!u@h JOIN :#lab\r\n"));
    transportB->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#lab\r\n"));
    controller.selectConversation(QStringLiteral("network-a"), QStringLiteral("#chan"));
    controller.openStatus(QStringLiteral("network-b"));
    QCOMPARE(controller.focusedNetworkId(), QStringLiteral("network-b"));
    QCOMPARE(controller.selectedNetworkId(), QStringLiteral("network-a"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#chan"));

    QVERIFY(controller.console()->submit(QStringLiteral("/part")));
    QVERIFY(!framesContain(transportA->writtenFrames(), QByteArrayLiteral("PART")));
    QVERIFY(!framesContain(transportB->writtenFrames(), QByteArrayLiteral("PART")));

    QVERIFY(controller.console()->submit(QStringLiteral("/part #lab")));
    QCOMPARE(transportB->writtenFrames().last(),
             QByteArrayLiteral("PART #lab\r\n"));
    QVERIFY(!framesContain(transportA->writtenFrames(), QByteArrayLiteral("PART")));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForNetworkTarget(conversations, QStringLiteral("network-b"),
                                 QStringLiteral("#lab")),
             -1);
    QVERIFY(rowForNetworkTarget(conversations, QStringLiteral("network-a"),
                                QStringLiteral("#lab"))
            >= 0);
    QCOMPARE(controller.selectedNetworkId(), QStringLiteral("network-a"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#chan"));
}

void ControllerTest::implicitStatusKickStaysOnFocusedNetwork()
{
    IrcController controller;
    auto *transportA = new FakeIrcTransport;
    auto *transportB = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("network-a")), transportA));
    QVERIFY(controller.addSession(config(QStringLiteral("network-b")), transportB));
    registerSession(controller.session(QStringLiteral("network-a")), transportA);
    registerSession(controller.session(QStringLiteral("network-b")), transportB);
    transportA->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#chan\r\n"));
    transportB->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#lab\r\n"));
    controller.selectConversation(QStringLiteral("network-a"), QStringLiteral("#chan"));
    controller.openStatus(QStringLiteral("network-b"));

    QVERIFY(controller.console()->submit(QStringLiteral("/kick bob")));
    QVERIFY(!framesContain(transportA->writtenFrames(), QByteArrayLiteral("KICK")));
    QVERIFY(!framesContain(transportB->writtenFrames(), QByteArrayLiteral("KICK")));

    QVERIFY(controller.console()->submit(QStringLiteral("/kick #lab bob")));
    QCOMPARE(transportB->writtenFrames().last(),
             QByteArrayLiteral("KICK #lab bob\r\n"));
    QVERIFY(!framesContain(transportA->writtenFrames(), QByteArrayLiteral("KICK")));
}

void ControllerTest::selectConversationByIdUsesCompositeKey()
{
    IrcController controller;
    auto *transportA = new FakeIrcTransport;
    auto *transportB = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("network-a")), transportA));
    QVERIFY(controller.addSession(config(QStringLiteral("network-b")), transportB));
    registerSession(controller.session(QStringLiteral("network-a")), transportA);
    registerSession(controller.session(QStringLiteral("network-b")), transportB);
    transportA->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#chan\r\n"));
    transportB->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#chan\r\n"));

    controller.selectConversationById(QStringLiteral("network-b\n#chan"));
    QCOMPARE(controller.selectedNetworkId(), QStringLiteral("network-b"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#chan"));
    QCOMPARE(controller.selectedConversationId(), QStringLiteral("network-b\n#chan"));
    QVERIFY(!controller.console()->isOpen());

    controller.selectConversationById(QStringLiteral("#chan"));
    QCOMPARE(controller.selectedNetworkId(), QStringLiteral("network-b"));
    QCOMPARE(controller.selectedConversationId(), QStringLiteral("network-b\n#chan"));
}

void ControllerTest::networkIconUrlComesFromIsupport()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    const QString networkId = QStringLiteral("libera");
    IrcSession *session = controller.addSession(config(networkId), transport);
    QVERIFY(session);
    registerSession(session, transport);
    QCOMPARE(controller.networkIconUrl(networkId), QString());

    transport->injectBytes(
        QByteArrayLiteral(":server 005 omairc draft/ICON=https://example.org/icon.svg "
                          "CHANTYPES=# PREFIX=(ov)@+ "
                          ":are supported by this server\r\n"));
    QCOMPARE(controller.networkIconUrl(networkId),
             QStringLiteral("https://example.org/icon.svg"));

    transport->injectBytes(
        QByteArrayLiteral(":server 005 omairc CHANMODES=beI,k,l,ps CASEMAPPING=rfc1459 "
                          ":are supported by this server\r\n"));
    QCOMPARE(controller.networkIconUrl(networkId),
             QStringLiteral("https://example.org/icon.svg"));

    controller.forgetNetworkState(networkId);
    QCOMPARE(controller.networkIconUrl(networkId), QString());
}

void ControllerTest::networkIconUrlClearsWhenReconnectOmitsDraftIcon()
{
    IrcController controller;
    IrcSessionConfig sessionConfig = config(QStringLiteral("libera"));
    sessionConfig.reconnectEnabled = true;
    auto *transport = new FakeIrcTransport;
    auto *timer = new FakeReconnectTimer;
    IrcSession *session = controller.addSession(sessionConfig, transport, timer);
    QVERIFY(session);
    registerSession(session, transport);

    transport->injectBytes(
        QByteArrayLiteral(":server 005 omairc draft/ICON=https://example.org/icon.svg "
                          "CHANTYPES=# PREFIX=(ov)@+ "
                          ":are supported by this server\r\n"));
    QCOMPARE(controller.networkIconUrl(QStringLiteral("libera")),
             QStringLiteral("https://example.org/icon.svg"));

    transport->remoteClose();
    QCOMPARE(session->state(), IrcSession::State::Reconnecting);
    QVERIFY(timer->active);
    timer->fire();
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    QCOMPARE(session->state(), IrcSession::State::Registered);
    QCOMPARE(controller.networkIconUrl(QStringLiteral("libera")), QString());

    transport->injectBytes(
        QByteArrayLiteral(":server 005 omairc CHANTYPES=# PREFIX=(ov)@+ "
                          "CASEMAPPING=rfc1459 "
                          ":are supported by this server\r\n"));
    QCOMPARE(controller.networkIconUrl(QStringLiteral("libera")), QString());
}

void ControllerTest::forgetNetworkDropsGhostRowsAndLog()
{
    IrcController controller;
    auto *transportA = new FakeIrcTransport;
    auto *transportB = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("network-a")), transportA));
    QVERIFY(controller.addSession(config(QStringLiteral("network-b")), transportB));
    registerSession(controller.session(QStringLiteral("network-a")), transportA);
    registerSession(controller.session(QStringLiteral("network-b")), transportB);
    transportA->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#alpha\r\n"));
    transportB->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#lab\r\n"
                          "NOTICE AUTH :*** Looking up your hostname...\r\n"));
    controller.selectConversation(QStringLiteral("network-b"), QStringLiteral("#lab"));
    controller.openStatus(QStringLiteral("network-b"));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("Looking up your hostname")));

    QVERIFY(controller.discardSession(QStringLiteral("network-b")));
    controller.forgetNetworkState(QStringLiteral("network-b"));
    QCOMPARE(controller.session(QStringLiteral("network-b")), nullptr);

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(conversations->rowCount(), 1);
    QCOMPARE(roleAt(conversations, 0, ConversationListModel::ConversationIdRole),
             QStringLiteral("network-a\n#alpha"));
    QCOMPARE(controller.selectedTarget(), QString());
    QVERIFY(!logContains(controller.console()->lines(),
                         QStringLiteral("Looking up your hostname")));
}

void ControllerTest::backgroundChatBumpsConversationEpoch()
{
    IrcController controller;
    auto *transportA = new FakeIrcTransport;
    auto *transportB = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("network-a")), transportA));
    QVERIFY(controller.addSession(config(QStringLiteral("network-b")), transportB));
    QVERIFY(controller.start(QStringLiteral("network-a")));
    registerSession(controller.session(QStringLiteral("network-a")), transportA);
    transportA->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#alpha\r\n"));
    controller.selectConversation(QStringLiteral("network-a"), QStringLiteral("#alpha"));

    QVERIFY(controller.start(QStringLiteral("network-b")));
    registerSession(controller.session(QStringLiteral("network-b")), transportB);
    transportB->injectBytes(
        QByteArrayLiteral(":omairc!u@h JOIN :#lab\r\n"
                          ":server 353 omairc = #lab :@omairc\r\n"
                          ":server 366 omairc #lab :End of NAMES\r\n"));

    const int epoch = controller.conversationEpoch();
    QCOMPARE(controller.unreadCountFor(QStringLiteral("network-b")), 0);
    QSignalSpy spy(&controller, &IrcController::conversationStateChanged);

    transportB->injectBytes(
        QByteArrayLiteral(":zed!u@h PRIVMSG #lab :ping\r\n"));

    QVERIFY(controller.conversationEpoch() > epoch);
    QVERIFY(spy.count() >= 1);
    QCOMPARE(controller.unreadCountFor(QStringLiteral("network-b")), 1);
    QVERIFY(!controller.mentionFor(QStringLiteral("network-b")));
}

void ControllerTest::backgroundPlaybackBumpsUnreadAndMention()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch\r\n"
                          ":server CAP omairc ACK :batch\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":omairc!u@h JOIN :#lab\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    const int lab = rowForTarget(conversations, QStringLiteral("#lab"));
    QVERIFY(lab >= 0);
    QCOMPARE(roleAt(conversations, lab, ConversationListModel::UnreadRole).toInt(),
             0);
    QVERIFY(!controller.mentionFor(QStringLiteral("libera")));

    transport->injectBytes(
        QByteArrayLiteral(
            ":znc.in BATCH +c znc.in/playback #lab\r\n"
            "@batch=c :***!znc@znc.in PRIVMSG #lab :Buffer Playback...\r\n"
            "@batch=c :lena!u@h PRIVMSG #lab :yesterday\r\n"
            "@batch=c :zed!u@h PRIVMSG #lab :omairc: ping\r\n"
            "@batch=c :***!znc@znc.in PRIVMSG #lab :Playback Complete.\r\n"
            ":znc.in BATCH -c\r\n"));

    QCOMPARE(roleAt(conversations, lab, ConversationListModel::UnreadRole).toInt(),
             2);
    QVERIFY(roleAt(conversations, lab, ConversationListModel::MentionRole).toBool());
    QVERIFY(controller.mentionFor(QStringLiteral("libera")));
    QCOMPARE(controller.unreadCountFor(QStringLiteral("libera")), 2);

    transport->injectBytes(
        QByteArrayLiteral(":rio!u@h PRIVMSG #lab :live now\r\n"));
    QCOMPARE(roleAt(conversations, lab, ConversationListModel::UnreadRole).toInt(),
             3);

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#lab"));
    QCOMPARE(roleAt(conversations, lab, ConversationListModel::UnreadRole).toInt(),
             0);
    QVERIFY(!controller.mentionFor(QStringLiteral("libera")));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QCOMPARE(roleAt(messages, 0, MessageListModel::BodyRole),
             QStringLiteral("yesterday"));
    QCOMPARE(roleAt(messages, 0, MessageListModel::OriginRole),
             QStringLiteral("replay"));
    QCOMPARE(roleAt(messages, 1, MessageListModel::BodyRole),
             QStringLiteral("omairc: ping"));
    QCOMPARE(roleAt(messages, 1, MessageListModel::OriginRole),
             QStringLiteral("replay"));
}

void ControllerTest::focusedChannelPlaybackPlantsUnreadMark()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString root = dir.filePath(QStringLiteral("omairc/logs"));
    ScopedTranscriptRoot scope(root);

    {
        IrcController controller;
        auto *transport = new FakeIrcTransport;
        QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
        QVERIFY(controller.start(QStringLiteral("libera")));
        transport->completeConnect();
        transport->injectBytes(
            QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                              ":server 001 omairc :Welcome\r\n"
                              ":omairc!u@h JOIN :#omarchy\r\n"
                              "@msgid=old-1 :alice!u@h PRIVMSG #omarchy :older line\r\n"));
    }

    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch\r\n"
                          ":server CAP omairc ACK :batch\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":omairc!u@h JOIN :#lab\r\n"));
    QCOMPARE(controller.selectedTarget(), QStringLiteral("#omarchy"));

    transport->injectBytes(
        QByteArrayLiteral(
            ":znc.in BATCH +c znc.in/playback #omarchy\r\n"
            "@batch=c;msgid=new-1 :lena!u@h PRIVMSG #omarchy :since-logoff\r\n"
            "@batch=c;msgid=new-2 :zed!u@h PRIVMSG #omarchy :also-new\r\n"
            ":znc.in BATCH -c\r\n"
            ":znc.in BATCH +l znc.in/playback #lab\r\n"
            "@batch=l;msgid=lab-1 :rio!u@h PRIVMSG #lab :lab-new\r\n"
            ":znc.in BATCH -l\r\n"));

    auto *messages = qobject_cast<MessageListModel *>(controller.messages());
    QVERIFY(messages);
    const int olderRow = bodyRow(messages, QStringLiteral("older line"));
    const int sinceRow = bodyRow(messages, QStringLiteral("since-logoff"));
    const int markRow = messages->unreadMarkRow();
    QVERIFY(olderRow >= 0);
    QVERIFY(sinceRow > olderRow);
    QVERIFY(markRow >= 0);
    QCOMPARE(markRow, sinceRow - 1);
    QCOMPARE(roleAt(messages, markRow, MessageListModel::KindRole).toString(),
             QStringLiteral("unread"));
    QCOMPARE(roleAt(messages, sinceRow, MessageListModel::OriginRole).toString(),
             QStringLiteral("replay"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(roleAt(conversations,
                    rowForTarget(conversations, QStringLiteral("#omarchy")),
                    ConversationListModel::UnreadRole).toInt(),
             0);
    QCOMPARE(roleAt(conversations,
                    rowForTarget(conversations, QStringLiteral("#lab")),
                    ConversationListModel::UnreadRole).toInt(),
             1);
}

void ControllerTest::chatHistoryBatchShowsBodyAndTime()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory\r\n"
                          ":server CAP omairc ACK :batch chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    transport->injectBytes(
        QByteArrayLiteral(
            ":irc.host BATCH +hx chathistory #omarchy\r\n"
            "@batch=hx;time=2011-10-19T16:40:51.620Z;msgid=old :alice!u@h PRIVMSG #omarchy :older\r\n"
            ":irc.host BATCH -hx\r\n"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    const int older = bodyRow(messages, QStringLiteral("older"));
    const int joined = bodyRow(messages, QStringLiteral("omairc joined"));
    QCOMPARE(older, 0);
    QVERIFY(joined > older);
    const QDateTime replayed = QDateTime::fromString(
        QStringLiteral("2011-10-19T16:40:51.620Z"), Qt::ISODateWithMs);
    QCOMPARE(roleAt(messages, older, MessageListModel::TimeRole),
             replayed.toLocalTime().toString(QStringLiteral("HH:mm")));
    QCOMPARE(roleAt(messages, older, MessageListModel::OriginRole),
             QStringLiteral("replay"));
    QCOMPARE(roleAt(messages, joined, MessageListModel::BodyRole),
             QStringLiteral("omairc joined"));
    QCOMPARE(controller.unreadCountFor(QStringLiteral("libera")), 0);
}

void ControllerTest::chatHistoryAndLiveTimeUseLocalWallClock()
{
    const ScopedTimeZone brisbane("Australia/Brisbane");
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory\r\n"
                          ":server CAP omairc ACK :batch chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    transport->injectBytes(
        QByteArrayLiteral(
            ":irc.host BATCH +hx chathistory #omarchy\r\n"
            "@batch=hx;time=2026-09-11T03:30:15.000Z;msgid=old "
            ":alice!u@h PRIVMSG #omarchy :[13:30:15] hello\r\n"
            ":irc.host BATCH -hx\r\n"
            "@time=2026-09-11T03:30:15.000Z :bob!u@h PRIVMSG #omarchy :live now\r\n"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    const int hello = bodyRow(messages, QStringLiteral("[13:30:15] hello"));
    const int joined = bodyRow(messages, QStringLiteral("omairc joined"));
    const int live = bodyRow(messages, QStringLiteral("live now"));
    QVERIFY(hello >= 0);
    QVERIFY(joined > hello);
    QVERIFY(live > joined);
    QCOMPARE(roleAt(messages, hello, MessageListModel::OriginRole),
             QStringLiteral("replay"));
    QCOMPARE(roleAt(messages, hello, MessageListModel::TimeRole),
             QStringLiteral("13:30"));
    QCOMPARE(roleAt(messages, live, MessageListModel::OriginRole),
             QStringLiteral("live"));
    QCOMPARE(roleAt(messages, live, MessageListModel::TimeRole),
             QStringLiteral("13:30"));
}

void ControllerTest::whoisFromChannelCopiesStatusLinesAsEvents()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                 transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/whois lena")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("WHOIS lena lena\r\n"));

    transport->injectBytes(
        QByteArrayLiteral(":irc 311 omairc lena ~lena user/host * :Lena\r\n"
                          ":irc 319 omairc lena :#omarchy\r\n"
                          ":irc 318 omairc lena :End of /WHOIS list.\r\n"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    const QStringList expected = {
        QStringLiteral("lena is ~lena@user/host (Lena)"),
        QStringLiteral("lena is on #omarchy"),
        QStringLiteral("End of WHOIS for lena"),
    };
    QCOMPARE(selectedBodies(messages).mid(selectedBodies(messages).size() - 3),
             expected);
    for (const QString& body : expected) {
        QVERIFY(hasWhoisBody(messages, body));
        QVERIFY(logContains(controller.console()->lines(), body));
    }
}

void ControllerTest::whoisFromChannelIncludesStoredMetadata()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                 transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(aliceMetadataWelcome());
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/whois Alice")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("WHOIS Alice Alice\r\n"));
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("METADATA * GET")));

    transport->injectBytes(
        QByteArrayLiteral(":irc 311 omairc Alice ~alice user/host * :Alice\r\n"
                          ":irc 319 omairc Alice :#omarchy\r\n"
                          ":irc 318 omairc Alice :End of /WHOIS list.\r\n"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    const QStringList extras = aliceWhoisMetadataLines();
    QStringList expected = {QStringLiteral("Alice is ~alice@user/host (Alice)")};
    expected += extras;
    expected << QStringLiteral("Alice is on #omarchy")
             << QStringLiteral("End of WHOIS for Alice");
    QCOMPARE(selectedBodies(messages).mid(selectedBodies(messages).size()
                                          - expected.size()),
             expected);
    auto *lines = controller.console()->lines();
    for (const QString& body : expected) {
        QVERIFY(hasWhoisBody(messages, body));
        QVERIFY(logHasLabeledText(lines, QStringLiteral("whois"), body));
    }
}

void ControllerTest::statusWhoisIncludesStoredMetadataNotTranscript()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                 transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(aliceMetadataWelcome());
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    const QStringList before = selectedBodies(messages);

    controller.openStatus(QStringLiteral("libera"));
    QVERIFY(controller.console()->submit(QStringLiteral("/whois Alice")));

    transport->injectBytes(
        QByteArrayLiteral(":irc 311 omairc Alice ~alice user/host * :Alice\r\n"
                          ":irc 318 omairc Alice :End of /WHOIS list.\r\n"));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QCOMPARE(selectedBodies(messages), before);
    QVERIFY(!selectedBodiesContain(messages,
                                   QStringLiteral("Alice is ~alice@user/host (Alice)")));
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("End of WHOIS for Alice")));
    auto *lines = controller.console()->lines();
    QVERIFY(logHasLabeledText(lines, QStringLiteral("whois"),
                              QStringLiteral("Alice is ~alice@user/host (Alice)")));
    QVERIFY(logHasLabeledText(lines, QStringLiteral("whois"),
                              QStringLiteral("End of WHOIS for Alice")));
    for (const QString& body : aliceWhoisMetadataLines()) {
        QVERIFY(!selectedBodiesContain(messages, body));
        QVERIFY(logHasLabeledText(lines, QStringLiteral("whois"), body));
    }
    const QStringList texts = logTexts(lines);
    const int user = texts.indexOf(QStringLiteral("Alice is ~alice@user/host (Alice)"));
    const int knownAs =
        texts.indexOf(QStringLiteral("Alice is also known as Alice Docs"));
    const int end = texts.indexOf(QStringLiteral("End of WHOIS for Alice"));
    QVERIFY(user >= 0);
    QVERIFY(knownAs > user);
    QVERIFY(end > knownAs);
}

void ControllerTest::failedWhoisDoesNotEmitStoredMetadata()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                 transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(aliceMetadataWelcome());
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/whois Alice")));
    transport->injectBytes(
        QByteArrayLiteral(":irc 401 omairc Alice :No such nick/channel\r\n"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(hasWhoisBody(messages, QStringLiteral("No such nick: Alice")));
    for (const QString& body : aliceWhoisMetadataLines()) {
        QVERIFY(!selectedBodiesContain(messages, body));
        QVERIFY(!logContains(controller.console()->lines(), body));
    }
}

void ControllerTest::whoisInterleavesByAskingBuffer()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                 transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":omairc!u@h JOIN :#help\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("/whois Lena")));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#help"));
    QVERIFY(controller.sendMessage(QStringLiteral("/whois sam")));

    transport->injectBytes(
        QByteArrayLiteral(":irc 311 omairc sam ~s h * :Sam\r\n"
                          ":irc 311 omairc lena ~l h * :Lena\r\n"
                          ":irc 318 omairc lena :End of WHOIS\r\n"
                          ":irc 318 omairc sam :End of WHOIS\r\n"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QCOMPARE(controller.unreadCountFor(QStringLiteral("libera")), 0);

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("lena is ~l@h (Lena)")));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("End of WHOIS for lena")));
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("sam")));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#help"));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("sam is ~s@h (Sam)")));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("End of WHOIS for sam")));
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("lena")));

    auto *lines = controller.console()->lines();
    QVERIFY(logContains(lines, QStringLiteral("lena is ~l@h (Lena)")));
    QVERIFY(logContains(lines, QStringLiteral("End of WHOIS for lena")));
    QVERIFY(logContains(lines, QStringLiteral("sam is ~s@h (Sam)")));
    QVERIFY(logContains(lines, QStringLiteral("End of WHOIS for sam")));
}

void ControllerTest::statusWhoisSupersedesConversationWatch()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                 transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("/whois lena")));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    const QStringList before = selectedBodies(messages);

    controller.openStatus(QStringLiteral("libera"));
    QVERIFY(controller.console()->submit(QStringLiteral("/whois lena")));

    transport->injectBytes(
        QByteArrayLiteral(":irc 311 omairc lena ~lena user/host * :Lena\r\n"
                          ":irc 318 omairc lena :End of /WHOIS list.\r\n"));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QCOMPARE(selectedBodies(messages), before);
    QVERIFY(!selectedBodiesContain(messages,
                                   QStringLiteral("lena is ~lena@user/host (Lena)")));
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("End of WHOIS for lena")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("lena is ~lena@user/host (Lena)")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("End of WHOIS for lena")));
}

void ControllerTest::unsolicitedWhoisStaysOnStatus()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                 transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :away-notify\r\n"
                          ":server CAP omairc ACK :away-notify\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":server 353 omairc = #omarchy :@omairc Alice\r\n"
                          ":server 366 omairc #omarchy :End of NAMES\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    auto *members = qobject_cast<QAbstractItemModel *>(controller.members());
    QVERIFY(messages);
    QVERIFY(members);
    const QStringList before = selectedBodies(messages);
    QCOMPARE(roleAt(members, 1, MemberListModel::NickRole), QStringLiteral("Alice"));
    QCOMPARE(roleAt(members, 1, MemberListModel::AwayRole), false);

    transport->injectBytes(
        QByteArrayLiteral(":irc 311 omairc lena ~lena user/host * :Lena\r\n"
                          ":irc 301 omairc Alice :gone fishing\r\n"
                          ":irc 401 omairc missing :No such nick/channel\r\n"));

    QCOMPARE(selectedBodies(messages), before);
    QVERIFY(!selectedBodiesContain(messages,
                                   QStringLiteral("lena is ~lena@user/host (Lena)")));
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("Alice is away: gone fishing")));
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("No such nick: missing")));
    QCOMPARE(roleAt(members, 1, MemberListModel::AwayRole), false);
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("lena is ~lena@user/host (Lena)")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("Alice is away: gone fishing")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("No such nick: missing")));
}

void ControllerTest::emptyChannelWhoisNamesANick()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                 transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    const int before = transport->writtenFrames().size();
    QVERIFY(!controller.sendMessage(QStringLiteral("/whois")));
    QCOMPARE(controller.lastError(), QStringLiteral("Name a nick"));
    QCOMPARE(transport->writtenFrames().size(), before);
    QVERIFY(!framesContain(transport->writtenFrames().mid(before),
                           QByteArrayLiteral("WHOIS")));
}

void ControllerTest::emptyDirectWhoisDefaultsAndRoutes()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                 transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":lena!u@h PRIVMSG omairc :hi\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));

    QVERIFY(controller.sendMessage(QStringLiteral("/whois")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("WHOIS lena lena\r\n"));

    transport->injectBytes(
        QByteArrayLiteral(":irc 311 omairc lena ~lena user/host * :Lena\r\n"
                          ":irc 318 omairc lena :End of /WHOIS list.\r\n"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(hasWhoisBody(messages, QStringLiteral("lena is ~lena@user/host (Lena)")));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("End of WHOIS for lena")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("lena is ~lena@user/host (Lena)")));
}

void ControllerTest::terminalWhoisClearsWatch()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                 transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);

    QVERIFY(controller.sendMessage(QStringLiteral("/whois lena")));
    transport->injectBytes(
        QByteArrayLiteral(":irc 318 omairc lena :End of /WHOIS list.\r\n"));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("End of WHOIS for lena")));
    const QStringList after318 = selectedBodies(messages);
    transport->injectBytes(
        QByteArrayLiteral(":irc 311 omairc lena ~lena user/host * :Lena\r\n"));
    QCOMPARE(selectedBodies(messages), after318);
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("lena is ~lena@user/host (Lena)")));

    QVERIFY(controller.sendMessage(QStringLiteral("/whois missing")));
    transport->injectBytes(
        QByteArrayLiteral(":irc 401 omairc missing :No such nick/channel\r\n"));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("No such nick: missing")));
    const QStringList after401 = selectedBodies(messages);
    transport->injectBytes(
        QByteArrayLiteral(":irc 311 omairc missing ~m h * :Missing\r\n"));
    QCOMPARE(selectedBodies(messages), after401);
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("missing is ~m@h (Missing)")));

    QVERIFY(controller.sendMessage(QStringLiteral("/whois ghost")));
    transport->injectBytes(
        QByteArrayLiteral(":irc 402 omairc ghost :No such server\r\n"));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("No such server: ghost")));
    const QStringList after402 = selectedBodies(messages);
    transport->injectBytes(
        QByteArrayLiteral(":irc 311 omairc ghost ~g h * :Ghost\r\n"));
    QCOMPARE(selectedBodies(messages), after402);
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("ghost is ~g@h (Ghost)")));
}

void ControllerTest::failedPrivmsgDoesNotStealWhoisWatch()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                 transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);

    QVERIFY(controller.sendMessage(QStringLiteral("/whois lena")));
    QVERIFY(controller.sendMessage(QStringLiteral("/msg lena hello")));
    QVERIFY(controller.sendMessage(QStringLiteral("/msg lena again")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArrayLiteral("PRIVMSG lena :again\r\n"));

    transport->injectBytes(
        QByteArrayLiteral(":irc 401 omairc lena :No such nick/channel\r\n"
                          ":irc 401 omairc lena :No such nick/channel\r\n"));
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("No such nick: lena")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("No such nick: lena")));

    transport->injectBytes(
        QByteArrayLiteral(":irc 311 omairc lena ~lena user/host * :Lena\r\n"
                          ":irc 318 omairc lena :End of /WHOIS list.\r\n"));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("lena is ~lena@user/host (Lena)")));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("End of WHOIS for lena")));
}

void ControllerTest::whoisFailureBeforeDeliveryDoesNotStealWatch()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                 transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);

    QVERIFY(controller.sendMessage(QStringLiteral("/whois lena")));
    QVERIFY(controller.sendMessage(QStringLiteral("/msg lena hello")));

    transport->injectBytes(
        QByteArrayLiteral(":irc 401 omairc lena :No such nick/channel\r\n"
                          ":irc 401 omairc lena :No such nick/channel\r\n"));
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("No such nick: lena")));

    transport->injectBytes(
        QByteArrayLiteral(":irc 311 omairc lena ~lena user/host * :Lena\r\n"
                          ":irc 318 omairc lena :End of /WHOIS list.\r\n"));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("lena is ~lena@user/host (Lena)")));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("End of WHOIS for lena")));
}

void ControllerTest::closedDirectWhoisDoesNotResurrect()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                 transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":lena!u@h PRIVMSG omairc :hi\r\n"));
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QVERIFY(rowForTarget(conversations, QStringLiteral("lena")) >= 0);

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    QVERIFY(controller.sendMessage(QStringLiteral("/whois")));
    controller.closeDirectMessage();
    QVERIFY(rowForTarget(conversations, QStringLiteral("lena")) < 0);
    const int rows = conversations->rowCount();

    transport->injectBytes(
        QByteArrayLiteral(":irc 311 omairc lena ~lena user/host * :Lena\r\n"
                          ":irc 318 omairc lena :End of /WHOIS list.\r\n"));

    QCOMPARE(conversations->rowCount(), rows);
    QVERIFY(rowForTarget(conversations, QStringLiteral("lena")) < 0);
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(!selectedBodiesContain(messages,
                                   QStringLiteral("lena is ~lena@user/host (Lena)")));
}

void ControllerTest::whoisEventDoesNotCollapseWithJoin()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                 transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("/whois lena")));
    transport->injectBytes(
        QByteArrayLiteral(":irc 311 omairc lena ~lena user/host * :Lena\r\n"
                          ":irc 318 omairc lena :End of /WHOIS list.\r\n"
                          ":alice!u@h JOIN :#omarchy\r\n"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(hasWhoisBody(messages, QStringLiteral("lena is ~lena@user/host (Lena)")));
    QVERIFY(hasWhoisBody(messages, QStringLiteral("End of WHOIS for lena")));
    QVERIFY(hasEventBody(messages, QStringLiteral("alice joined")));
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("End of WHOIS for lena, alice joined")));
    QCOMPARE(bodyRow(messages, QStringLiteral("End of WHOIS for lena")) + 1,
             bodyRow(messages, QStringLiteral("alice joined")));
}

void ControllerTest::ctcpFromChannelCopiesReplyAsWhoisEvent()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                 transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/version lena")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArray("PRIVMSG lena :\x01" "VERSION\x01\r\n"));

    transport->injectBytes(
        QByteArray(":lena!u@h NOTICE omairc :\x01VERSION Omairc 0.4.0\x01\r\n"));

    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(hasWhoisBody(messages,
                         QStringLiteral("VERSION reply from lena: Omairc 0.4.0")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("VERSION reply from lena: Omairc 0.4.0")));

    QVERIFY(controller.sendMessage(QStringLiteral("/time lena")));
    transport->injectBytes(
        QByteArray(":lena!u@h NOTICE omairc :\x01"
                   "TIME Tue, 15 Sep 2026 12:00:00 +0000\x01\r\n"));
    QVERIFY(hasWhoisBody(
        messages,
        QStringLiteral("TIME reply from lena: Tue, 15 Sep 2026 12:00:00 +0000")));

    QVERIFY(controller.sendMessage(QStringLiteral("/ping lena")));
    const QByteArray pingFrame = transport->writtenFrames().last();
    const QByteArray pingPrefix = QByteArray("PRIVMSG lena :\x01" "PING ");
    QVERIFY(pingFrame.startsWith(pingPrefix));
    QVERIFY(pingFrame.endsWith(QByteArray("\x01\r\n")));
    const QByteArray token = pingFrame.mid(
        pingPrefix.size(), pingFrame.size() - pingPrefix.size() - 3);
    transport->injectBytes(QByteArray(":lena!u@h NOTICE omairc :\x01PING ")
                           + token + QByteArray("\x01\r\n"));
    bool sawPing = false;
    for (const QString& body : selectedBodies(messages)) {
        if (body.startsWith(QStringLiteral("PING reply from lena: "))
            && body.endsWith(QStringLiteral(" ms"))) {
            sawPing = true;
        }
    }
    QVERIFY(sawPing);
}

void ControllerTest::ctcpReplyDuringSendCopiesIntoAskingTranscript()
{
    class ImmediateCtcpTransport : public FakeIrcTransport
    {
    public:
        using FakeIrcTransport::FakeIrcTransport;

        void write(const QByteArray& frame) override
        {
            FakeIrcTransport::write(frame);
            if (!frame.startsWith("PRIVMSG lena :"))
                return;
            injectBytes(
                QByteArray(":lena!u@h NOTICE omairc :\x01VERSION Omairc 0.4.0\x01\r\n"));
        }
    };

    IrcController controller;
    auto *transport = new ImmediateCtcpTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                 transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    QVERIFY(controller.sendMessage(QStringLiteral("/version lena")));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(hasWhoisBody(messages,
                         QStringLiteral("VERSION reply from lena: Omairc 0.4.0")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("VERSION reply from lena: Omairc 0.4.0")));
}

void ControllerTest::statusCtcpStaysOnStatus()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                 transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    const QStringList before = selectedBodies(messages);

    controller.openStatus(QStringLiteral("libera"));
    QVERIFY(controller.console()->submit(QStringLiteral("/version lena")));
    transport->injectBytes(
        QByteArray(":lena!u@h NOTICE omairc :\x01VERSION Omairc 0.4.0\x01\r\n"));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QCOMPARE(selectedBodies(messages), before);
    QVERIFY(!selectedBodiesContain(messages,
                                   QStringLiteral("VERSION reply from lena: Omairc 0.4.0")));
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("VERSION reply from lena: Omairc 0.4.0")));
}

void ControllerTest::emptyChannelCtcpNamesANick()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                 transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    const int before = transport->writtenFrames().size();
    QVERIFY(!controller.sendMessage(QStringLiteral("/ping")));
    QCOMPARE(controller.lastError(), QStringLiteral("Name a nick"));
    QCOMPARE(transport->writtenFrames().size(), before);
}

void ControllerTest::emptyDirectCtcpDefaultsAndRoutes()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                 transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":lena!u@h PRIVMSG omairc :hi\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    QVERIFY(controller.sendMessage(QStringLiteral("/version")));
    QCOMPARE(transport->writtenFrames().last(),
             QByteArray("PRIVMSG lena :\x01" "VERSION\x01\r\n"));
    transport->injectBytes(
        QByteArray(":lena!u@h NOTICE omairc :\x01VERSION HexChat 2.16\x01\r\n"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(hasWhoisBody(messages,
                         QStringLiteral("VERSION reply from lena: HexChat 2.16")));
}

void ControllerTest::unsolicitedCtcpReplyStaysOnStatus()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                 transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    const QStringList before = selectedBodies(messages);

    transport->injectBytes(
        QByteArray(":lena!u@h NOTICE omairc :\x01VERSION Omairc 0.4.0\x01\r\n"));
    QCOMPARE(selectedBodies(messages), before);
    QVERIFY(logContains(controller.console()->lines(),
                        QStringLiteral("VERSION reply from lena: Omairc 0.4.0")));
}

void ControllerTest::transcriptPersistsAndReloadsMuted()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QByteArray previousState = qgetenv("XDG_STATE_HOME");
    qputenv("XDG_STATE_HOME", dir.path().toUtf8());
    QCOMPARE(IrcConversationLog::defaultRoot(),
             QDir(dir.path()).filePath(QStringLiteral("omairc/logs")));
    if (previousState.isEmpty())
        qunsetenv("XDG_STATE_HOME");
    else
        qputenv("XDG_STATE_HOME", previousState);
    const QString root = dir.filePath(QStringLiteral("omairc/logs"));
    ScopedTranscriptRoot scope(root);
    IrcConversationLog paths(root);
    const QString channelPath =
        paths.pathFor(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    const QString directPath =
        paths.pathFor(QStringLiteral("libera"), QStringLiteral("alice"));

    {
        IrcController controller;
        auto *transport = new FakeIrcTransport;
        IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                    transport);
        QVERIFY(session);
        QVERIFY(controller.start(QStringLiteral("libera")));
        transport->completeConnect();
        transport->injectBytes(
            QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                              ":server 001 omairc :Welcome\r\n"
                              ":omairc!u@h JOIN :#omarchy\r\n"
                              "@msgid=chan-1 :alice!u@h PRIVMSG #omarchy :hello channel\r\n"));
        controller.selectConversation(QStringLiteral("libera"),
                                      QStringLiteral("#omarchy"));
        QVERIFY(controller.sendMessage(QStringLiteral("own line")));
        controller.openDirectMessage(QStringLiteral("alice"));
        QVERIFY(controller.sendMessage(QStringLiteral("hello alice")));
        transport->injectBytes(
            QByteArrayLiteral("@msgid=dm-1 :alice!u@h PRIVMSG omairc :reply\r\n"));
    }

    QVERIFY(QFileInfo::exists(channelPath));
    QVERIFY(QFileInfo::exists(directPath));
    const QFileDevice::Permissions bits = QFileInfo(channelPath).permissions();
    QVERIFY(bits & QFileDevice::ReadOwner);
    QVERIFY(bits & QFileDevice::WriteOwner);
    QVERIFY(!(bits & QFileDevice::ReadGroup));
    QVERIFY(!(bits & QFileDevice::ReadOther));

    QFile channelFile(channelPath);
    QVERIFY(channelFile.open(QIODevice::ReadOnly | QIODevice::Text));
    const QJsonDocument first = QJsonDocument::fromJson(channelFile.readLine());
    QVERIFY(first.isObject());
    const QJsonObject object = first.object();
    QVERIFY(object.contains(QStringLiteral("timestamp")));
    QVERIFY(object.contains(QStringLiteral("author")));
    QVERIFY(object.contains(QStringLiteral("kind")));
    QVERIFY(object.contains(QStringLiteral("body")));
    QVERIFY(jsonlBodies(channelPath).contains(QStringLiteral("hello channel")));
    QVERIFY(jsonlBodies(directPath).contains(QStringLiteral("hello alice")));
    QVERIFY(jsonlBodies(directPath).contains(QStringLiteral("reply")));

    IrcController reloaded;
    auto *transport = new FakeIrcTransport;
    QSignalSpy mentions(&reloaded, &IrcController::mentionArrived);
    IrcSession *session = reloaded.addSession(config(QStringLiteral("libera")),
                                              transport);
    QVERIFY(session);
    QVERIFY(reloaded.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          "@msgid=chan-1 :alice!u@h PRIVMSG #omarchy :hello channel\r\n"));
    reloaded.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(reloaded.messages());
    QVERIFY(messages);
    const int hello = bodyRow(messages, QStringLiteral("hello channel"));
    QVERIFY(hello >= 0);
    QCOMPARE(roleAt(messages, hello, MessageListModel::OriginRole).toString(),
             QStringLiteral("replay"));
    QCOMPARE(roleAt(messages, hello, MessageListModel::MsgidRole).toString(),
             QStringLiteral("chan-1"));
    QVERIFY(selectedBodies(messages).count(QStringLiteral("hello channel")) == 1);
    QCOMPARE(mentions.count(), 0);

    reloaded.openDirectMessage(QStringLiteral("alice"));
    QCOMPARE(reloaded.selectedTarget(), QStringLiteral("alice"));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("hello alice")));
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("reply")));
    const int reply = bodyRow(messages, QStringLiteral("reply"));
    QVERIFY(reply >= 0);
    QCOMPARE(roleAt(messages, reply, MessageListModel::OriginRole).toString(),
             QStringLiteral("replay"));
    QCOMPARE(mentions.count(), 0);
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(reloaded.conversations());
    const int aliceRow = rowForTarget(conversations, QStringLiteral("alice"));
    QVERIFY(aliceRow >= 0);
    QCOMPARE(roleAt(conversations, aliceRow, ConversationListModel::UnreadRole).toInt(),
             0);
}

void ControllerTest::transcriptHydrateDoesNotNotify()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString root = dir.filePath(QStringLiteral("omairc/logs"));
    ScopedTranscriptRoot scope(root);

    {
        IrcController controller;
        auto *transport = new FakeIrcTransport;
        QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
        QVERIFY(controller.start(QStringLiteral("libera")));
        transport->completeConnect();
        transport->injectBytes(
            QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                              ":server 001 omairc :Welcome\r\n"
                              ":omairc!u@h JOIN :#omarchy\r\n"
                              ":omairc!u@h JOIN :#lab\r\n"
                              ":zed!u@h PRIVMSG #lab :omairc: ping\r\n"));
    }

    IrcController reloaded;
    auto *transport = new FakeIrcTransport;
    QSignalSpy mentions(&reloaded, &IrcController::mentionArrived);
    QVERIFY(reloaded.addSession(config(QStringLiteral("libera")), transport));
    QVERIFY(reloaded.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":omairc!u@h JOIN :#lab\r\n"));
    QCOMPARE(mentions.count(), 0);
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(reloaded.conversations());
    const int lab = rowForTarget(conversations, QStringLiteral("#lab"));
    QVERIFY(lab >= 0);
    QCOMPARE(roleAt(conversations, lab, ConversationListModel::UnreadRole).toInt(),
             0);
    QVERIFY(!roleAt(conversations, lab, ConversationListModel::MentionRole).toBool());
    reloaded.selectConversation(QStringLiteral("libera"), QStringLiteral("#lab"));
    auto *messages = qobject_cast<QAbstractItemModel *>(reloaded.messages());
    const int ping = bodyRow(messages, QStringLiteral("omairc: ping"));
    QVERIFY(ping >= 0);
    QCOMPARE(roleAt(messages, ping, MessageListModel::OriginRole).toString(),
             QStringLiteral("replay"));
    QCOMPARE(mentions.count(), 0);
}

void ControllerTest::transcriptSkipsSecretsAndKeepsSessionOnWriteError()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString root = dir.filePath(QStringLiteral("omairc/logs"));
    ScopedTranscriptRoot scope(root);

    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("visible chat")));
    QVERIFY(controller.sendMessage(QStringLiteral("IDENTIFY hunter2")));
    QVERIFY(controller.sendMessage(QStringLiteral("/raw PASS s3cret")));
    QVERIFY(controller.sendMessage(QStringLiteral("/join #locked roomkey")));
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#locked\r\n"));

    QVERIFY(treeContains(root, QStringLiteral("visible chat")));
    QVERIFY(!treeContains(root, QStringLiteral("hunter2")));
    QVERIFY(!treeContains(root, QStringLiteral("s3cret")));
    QVERIFY(!treeContains(root, QStringLiteral("roomkey")));
    QCOMPARE(session->state(), IrcSession::State::Registered);

    QTemporaryDir blocked;
    QVERIFY(blocked.isValid());
    const QString badRoot = blocked.filePath(QStringLiteral("not-a-dir"));
    QFile blocker(badRoot);
    QVERIFY(blocker.open(QIODevice::WriteOnly));
    QVERIFY(blocker.write("nope") == 4);
    blocker.close();
    ScopedTranscriptRoot badScope(badRoot);
    IrcController doomed;
    auto *badTransport = new FakeIrcTransport;
    IrcSession *badSession =
        doomed.addSession(config(QStringLiteral("libera")), badTransport);
    QVERIFY(badSession);
    QVERIFY(doomed.start(QStringLiteral("libera")));
    badTransport->completeConnect();
    badTransport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    doomed.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(doomed.sendMessage(QStringLiteral("still live")));
    QCOMPARE(badSession->state(), IrcSession::State::Registered);
    auto *messages = qobject_cast<QAbstractItemModel *>(doomed.messages());
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("still live")));
}

void ControllerTest::transcriptClearLeavesFile()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString root = dir.filePath(QStringLiteral("omairc/logs"));
    ScopedTranscriptRoot scope(root);
    IrcConversationLog paths(root);
    const QString channelPath =
        paths.pathFor(QStringLiteral("libera"), QStringLiteral("#omarchy"));

    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QVERIFY(controller.sendMessage(QStringLiteral("keep me")));
    QVERIFY(jsonlBodies(channelPath).contains(QStringLiteral("keep me")));
    QVERIFY(controller.sendMessage(QStringLiteral("/clear")));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QCOMPARE(messages->rowCount(), 0);
    QVERIFY(jsonlBodies(channelPath).contains(QStringLiteral("keep me")));

    IrcController missing;
    auto *emptyTransport = new FakeIrcTransport;
    QVERIFY(missing.addSession(config(QStringLiteral("libera")), emptyTransport));
    QVERIFY(missing.start(QStringLiteral("libera")));
    emptyTransport->completeConnect();
    emptyTransport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#fresh\r\n"));
    missing.selectConversation(QStringLiteral("libera"), QStringLiteral("#fresh"));
    auto *fresh = qobject_cast<QAbstractItemModel *>(missing.messages());
    QCOMPARE(selectedBodies(fresh), QStringList{QStringLiteral("omairc joined")});
}

void ControllerTest::zncPlaybackPlayUsesStoredServerTime()
{
    const QDateTime room = QDateTime::fromString(
        QStringLiteral("2024-03-09T16:00:00.620Z"), Qt::ISODateWithMs);
    const QDateTime direct = QDateTime::fromString(
        QStringLiteral("2024-03-09T16:00:01.500Z"), Qt::ISODateWithMs);
    QVERIFY(room.isValid());
    QVERIFY(direct.isValid());
    QCOMPARE(ircPlaybackPlayStamp(room), QStringLiteral("1710000000.620"));
    QCOMPARE(ircPlaybackPlayStamp(direct), QStringLiteral("1710000001.500"));

    const QByteArray playZero =
        QByteArrayLiteral("ZNC *playback PLAY * 0\r\n");
    const QByteArray playRoom =
        QByteArrayLiteral("ZNC *playback PLAY #omarchy 1710000000.620\r\n");
    const QByteArray playDirect =
        QByteArrayLiteral("ZNC *playback PLAY lena 1710000001.500\r\n");
    const QByteArray playStamp =
        QByteArrayLiteral("ZNC *playback PLAY * 1710000001.500\r\n");

    {
        IrcController controller;
        IrcSessionConfig sessionConfig = config(QStringLiteral("libera"));
        sessionConfig.reconnectEnabled = true;
        auto *transport = new FakeIrcTransport;
        auto *timer = new FakeReconnectTimer;
        IrcSession *session = controller.addSession(sessionConfig, transport, timer);
        QVERIFY(session);
        QVERIFY(controller.start(QStringLiteral("libera")));
        transport->completeConnect();
        transport->injectBytes(
            QByteArrayLiteral(":server CAP omairc LS :batch znc.in/playback\r\n"
                              ":server CAP omairc ACK :batch znc.in/playback\r\n"));
        QVERIFY(!framesContain(transport->writtenFrames(),
                               QByteArrayLiteral("*playback PLAY")));
        transport->injectBytes(QByteArrayLiteral(":server 001 omairc :Welcome\r\n"));
        QCOMPARE(session->state(), IrcSession::State::Registered);
        QVERIFY(!framesContain(transport->writtenFrames(),
                               QByteArrayLiteral("*playback PLAY")));
        transport->injectBytes(
            QByteArrayLiteral(":server 376 omairc :End of MOTD\r\n"));
        QCOMPARE(frameCount(transport->writtenFrames(), playZero), 1);
        QVERIFY(!framesContain(transport->writtenFrames(),
                               QByteArrayLiteral("PRIVMSG *playback")));

        transport->injectBytes(QByteArrayLiteral(
            "@time=nope :alice!u@h PRIVMSG #omarchy :badclock\r\n"
            ":bob!u@h PRIVMSG #omarchy :untagged\r\n"
            "@time=2024-03-09T16:00:00.620Z :lena!u@h PRIVMSG #omarchy :room\r\n"
            "@time=2024-03-09T16:00:01.500Z :lena!u@h PRIVMSG omairc :dm\r\n"
            "@time=2024-03-09T18:00:00.000Z :NickServ!NickServ@services "
            "PRIVMSG omairc :registered\r\n"
            "@time=2024-03-09T19:00:00.000Z :*status!znc@znc.in PRIVMSG omairc "
            ":attached\r\n"));

        auto *conversations =
            qobject_cast<QAbstractItemModel *>(controller.conversations());
        QVERIFY(conversations);
        QCOMPARE(rowForTarget(conversations, QStringLiteral("*status")), -1);
        QCOMPARE(rowForTarget(conversations, QStringLiteral("*playback")), -1);
        QCOMPARE(rowForTarget(conversations, QStringLiteral("NickServ")), -1);
        QVERIFY(rowForTarget(conversations, QStringLiteral("lena")) >= 0);

        transport->remoteClose();
        QCOMPARE(session->state(), IrcSession::State::Reconnecting);
        QVERIFY(timer->active);
        timer->fire();
        transport->completeConnect();
        transport->injectBytes(
            QByteArrayLiteral(":server CAP omairc LS :batch znc.in/playback\r\n"
                              ":server CAP omairc ACK :batch znc.in/playback\r\n"
                              ":server 001 omairc :Welcome\r\n"));
        QCOMPARE(frameCount(transport->writtenFrames(), playZero), 1);
        QCOMPARE(frameCount(transport->writtenFrames(), playRoom), 0);
        QCOMPARE(frameCount(transport->writtenFrames(), playDirect), 0);
        QCOMPARE(frameCount(transport->writtenFrames(), playStamp), 0);
        transport->injectBytes(
            QByteArrayLiteral(":server 376 omairc :End of MOTD\r\n"));
        QCOMPARE(frameCount(transport->writtenFrames(), playZero), 2);
        QCOMPARE(frameCount(transport->writtenFrames(), playRoom), 1);
        QCOMPARE(frameCount(transport->writtenFrames(), playDirect), 1);
        QCOMPARE(frameCount(transport->writtenFrames(), playStamp), 0);
        QVERIFY(!framesContain(transport->writtenFrames(),
                               QByteArrayLiteral("PRIVMSG *playback")));
        QVERIFY(!framesContain(transport->writtenFrames(),
                               QByteArrayLiteral("PLAY NickServ")));
        QVERIFY(!framesContain(transport->writtenFrames(),
                               QByteArrayLiteral("PLAY *status")));
    }

    IrcController again;
    auto *transport = new FakeIrcTransport;
    QVERIFY(again.addSession(config(QStringLiteral("libera")), transport));
    QVERIFY(again.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch znc.in/playback\r\n"
                          ":server CAP omairc ACK :batch znc.in/playback\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("*playback PLAY")));
    transport->injectBytes(
        QByteArrayLiteral(":server 376 omairc :End of MOTD\r\n"));
    QCOMPARE(frameCount(transport->writtenFrames(), playRoom), 1);
    QCOMPARE(frameCount(transport->writtenFrames(), playDirect), 1);
    QCOMPARE(frameCount(transport->writtenFrames(), playStamp), 0);
    QCOMPARE(frameCount(transport->writtenFrames(), playZero), 1);
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("PRIVMSG *playback")));
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("PLAY NickServ")));
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("PLAY *status")));
}

void ControllerTest::zncPlaybackPlaysUnstampedAutojoinAndLaterJoin()
{
    const QDateTime room = QDateTime::fromString(
        QStringLiteral("2024-03-09T16:00:00.620Z"), Qt::ISODateWithMs);
    const QDateTime direct = QDateTime::fromString(
        QStringLiteral("2024-03-09T16:00:01.500Z"), Qt::ISODateWithMs);
    QVERIFY(room.isValid());
    QVERIFY(direct.isValid());
    QCOMPARE(ircPlaybackPlayStamp(room), QStringLiteral("1710000000.620"));
    QCOMPARE(ircPlaybackPlayStamp(direct), QStringLiteral("1710000001.500"));

    const QByteArray caps =
        QByteArrayLiteral(":server CAP omairc LS :batch znc.in/playback\r\n"
                          ":server CAP omairc ACK :batch znc.in/playback\r\n");
    const QByteArray playRoom =
        QByteArrayLiteral("ZNC *playback PLAY #omarchy 1710000000.620\r\n");
    const QByteArray playDirect =
        QByteArrayLiteral("ZNC *playback PLAY lena 1710000001.500\r\n");
    const QByteArray playQuiet =
        QByteArrayLiteral("ZNC *playback PLAY #quiet 0\r\n");
    const QByteArray playExtra =
        QByteArrayLiteral("ZNC *playback PLAY #extra 0\r\n");
    const QByteArray playEarly =
        QByteArrayLiteral("ZNC *playback PLAY #early 0\r\n");
    const QByteArray playZero =
        QByteArrayLiteral("ZNC *playback PLAY * 0\r\n");
    const QByteArray playWildcard =
        QByteArrayLiteral("ZNC *playback PLAY * 1710000001.500\r\n");

    {
        IrcController controller;
        auto *transport = new FakeIrcTransport;
        QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
        QVERIFY(controller.start(QStringLiteral("libera")));
        transport->completeConnect();
        transport->injectBytes(caps);
        transport->injectBytes(QByteArrayLiteral(
            ":server 001 omairc :Welcome\r\n"
            ":server 376 omairc :End of MOTD\r\n"
            "@time=2024-03-09T16:00:00.620Z :lena!u@h PRIVMSG #omarchy :room\r\n"
            "@time=2024-03-09T16:00:01.500Z :lena!u@h PRIVMSG omairc :dm\r\n"
            "@time=2024-03-09T18:00:00.000Z :NickServ!NickServ@services "
            "PRIVMSG omairc :registered\r\n"));
    }

    {
        IrcSessionConfig sessionConfig = config(QStringLiteral("libera"));
        sessionConfig.autojoinChannels = {QStringLiteral("#omarchy"),
                                          QStringLiteral("#quiet")};
        IrcController controller;
        auto *transport = new FakeIrcTransport;
        QVERIFY(controller.addSession(sessionConfig, transport));
        QVERIFY(controller.start(QStringLiteral("libera")));
        transport->completeConnect();
        transport->injectBytes(caps);
        transport->injectBytes(QByteArrayLiteral(":server 001 omairc :Welcome\r\n"));
        QVERIFY(!framesContain(transport->writtenFrames(),
                               QByteArrayLiteral("*playback PLAY")));

        transport->injectBytes(
            QByteArrayLiteral(":server 376 omairc :End of MOTD\r\n"));
        QCOMPARE(frameCount(transport->writtenFrames(), playRoom), 1);
        QCOMPARE(frameCount(transport->writtenFrames(), playDirect), 1);
        QCOMPARE(frameCount(transport->writtenFrames(), playQuiet), 1);
        QCOMPARE(frameCount(transport->writtenFrames(), playZero), 1);
        QCOMPARE(frameCount(transport->writtenFrames(), playWildcard), 0);
        QCOMPARE(frameCount(transport->writtenFrames(), playExtra), 0);
        QVERIFY(!framesContain(transport->writtenFrames(),
                               QByteArrayLiteral("PRIVMSG *playback")));
        QVERIFY(!framesContain(transport->writtenFrames(),
                               QByteArrayLiteral("PLAY #omarchy 0")));
        QVERIFY(!framesContain(transport->writtenFrames(),
                               QByteArrayLiteral("PLAY NickServ")));

        transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#extra\r\n"));
        QCOMPARE(frameCount(transport->writtenFrames(), playExtra), 1);
        QCOMPARE(frameCount(transport->writtenFrames(), playRoom), 1);
        QCOMPARE(frameCount(transport->writtenFrames(), playQuiet), 1);

        transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"
                                                 ":omairc!u@h JOIN :#quiet\r\n"
                                                 ":omairc!u@h JOIN :#extra\r\n"));
        QCOMPARE(frameCount(transport->writtenFrames(), playRoom), 2);
        QCOMPARE(frameCount(transport->writtenFrames(), playQuiet), 2);
        QCOMPARE(frameCount(transport->writtenFrames(), playExtra), 2);
        QVERIFY(!framesContain(transport->writtenFrames(),
                               QByteArrayLiteral("PLAY #omarchy 0")));

        transport->injectBytes(QByteArrayLiteral(
            ":znc.in BATCH +kept znc.in/playback #quiet\r\n"
            "@batch=kept;time=2024-03-09T16:00:04.000Z "
            ":lena!u@h PRIVMSG #quiet :kept\r\n"
            ":znc.in BATCH -kept\r\n"));
        transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#quiet\r\n"
                                                 ":omairc!u@h JOIN :#omarchy\r\n"
                                                 ":omairc!u@h JOIN :#extra\r\n"));
        QCOMPARE(frameCount(transport->writtenFrames(), playQuiet), 2);
        QCOMPARE(frameCount(transport->writtenFrames(), playRoom), 3);
        QCOMPARE(frameCount(transport->writtenFrames(), playExtra), 3);
        QVERIFY(!framesContain(transport->writtenFrames(),
                               QByteArrayLiteral("PRIVMSG *status :*playback")));
        QVERIFY(!framesContain(transport->writtenFrames(),
                               QByteArrayLiteral("PRIVMSG *playback")));
    }

    IrcController early;
    auto *transport = new FakeIrcTransport;
    QVERIFY(early.addSession(config(QStringLiteral("libera")), transport));
    QVERIFY(early.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(caps);
    transport->injectBytes(QByteArrayLiteral(
        ":server 001 omairc :Welcome\r\n"
        ":omairc!u@h JOIN :#early\r\n"));
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("*playback PLAY")));
    transport->injectBytes(QByteArrayLiteral(":server 376 omairc :End of MOTD\r\n"));
    QCOMPARE(frameCount(transport->writtenFrames(), playRoom), 1);
    QCOMPARE(frameCount(transport->writtenFrames(), playDirect), 1);
    QCOMPARE(frameCount(transport->writtenFrames(), playEarly), 1);
    QCOMPARE(frameCount(transport->writtenFrames(), playZero), 1);
    QCOMPARE(frameCount(transport->writtenFrames(), playWildcard), 0);
}

void ControllerTest::zncPlaybackJoinRetriesUntilBatchIsKept()
{
    const QByteArray caps =
        QByteArrayLiteral(":server CAP omairc LS :batch znc.in/playback\r\n"
                          ":server CAP omairc ACK :batch znc.in/playback\r\n");
    const QByteArray playZero =
        QByteArrayLiteral("ZNC *playback PLAY * 0\r\n");
    const QByteArray playFresh =
        QByteArrayLiteral("ZNC *playback PLAY #fresh 0\r\n");
    const QByteArray playHeld =
        QByteArrayLiteral("ZNC *playback PLAY #held 0\r\n");
    const QByteArray playDup =
        QByteArrayLiteral("ZNC *playback PLAY #dup 0\r\n");
    const QByteArray playEmpty =
        QByteArrayLiteral("ZNC *playback PLAY #empty 0\r\n");

    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(caps);
    transport->injectBytes(QByteArrayLiteral(":server 001 omairc :Welcome\r\n"));
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("*playback PLAY")));
    transport->injectBytes(QByteArrayLiteral(":server 376 omairc :End of MOTD\r\n"));
    QCOMPARE(frameCount(transport->writtenFrames(), playZero), 1);

    transport->injectBytes(QByteArrayLiteral(
        ":znc.in BATCH +held znc.in/playback #held\r\n"
        "@batch=held;time=2024-03-09T16:00:00.620Z "
        ":lena!u@h PRIVMSG #held :before join\r\n"
        ":znc.in BATCH -held\r\n"));
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#held\r\n"));
    QCOMPARE(frameCount(transport->writtenFrames(), playHeld), 0);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#held\r\n"));
    QCOMPARE(frameCount(transport->writtenFrames(), playHeld), 0);

    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#fresh\r\n"));
    QCOMPARE(frameCount(transport->writtenFrames(), playFresh), 1);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#fresh\r\n"));
    QCOMPARE(frameCount(transport->writtenFrames(), playFresh), 2);
    transport->injectBytes(QByteArrayLiteral(
        ":znc.in BATCH +fresh znc.in/playback #fresh\r\n"
        "@batch=fresh;time=2024-03-09T16:00:01.500Z "
        ":lena!u@h PRIVMSG #fresh :kept\r\n"
        ":znc.in BATCH -fresh\r\n"));
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#fresh\r\n"));
    QCOMPARE(frameCount(transport->writtenFrames(), playFresh), 2);

    transport->injectBytes(QByteArrayLiteral(
        ":omairc!u@h JOIN :#dup\r\n"
        "@time=2024-03-09T16:00:04.000Z :lena!u@h PRIVMSG #dup :already\r\n"
        ":znc.in BATCH +dup znc.in/playback #dup\r\n"
        "@batch=dup;time=2024-03-09T16:00:04.000Z "
        ":Lena!u@h PRIVMSG #dup :already\r\n"
        ":znc.in BATCH -dup\r\n"));
    QCOMPARE(frameCount(transport->writtenFrames(), playDup), 1);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#dup\r\n"));
    QCOMPARE(frameCount(transport->writtenFrames(), playDup), 1);

    transport->injectBytes(QByteArrayLiteral(
        ":omairc!u@h JOIN :#empty\r\n"
        ":znc.in BATCH +empty znc.in/playback #empty\r\n"
        ":znc.in BATCH -empty\r\n"));
    QCOMPARE(frameCount(transport->writtenFrames(), playEmpty), 1);
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#empty\r\n"));
    QCOMPARE(frameCount(transport->writtenFrames(), playEmpty), 2);

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("*playback")), -1);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("*status")), -1);
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("PRIVMSG *playback")));
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("PRIVMSG *status :*playback")));
    QCOMPARE(frameCount(transport->writtenFrames(), playZero), 1);
}

void ControllerTest::zncPlaybackEmptyStoreWithAutojoinPlaysWildcard()
{
    const QByteArray caps =
        QByteArrayLiteral(":server CAP omairc LS :batch znc.in/playback\r\n"
                          ":server CAP omairc ACK :batch znc.in/playback\r\n");
    const QByteArray playZero =
        QByteArrayLiteral("ZNC *playback PLAY * 0\r\n");
    const QByteArray playChannel =
        QByteArrayLiteral("ZNC *playback PLAY #omarchy 0\r\n");

    IrcSessionConfig sessionConfig = config(QStringLiteral("libera"));
    sessionConfig.autojoinChannels = {QStringLiteral("#omarchy")};
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(sessionConfig, transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(caps);
    transport->injectBytes(QByteArrayLiteral(":server 001 omairc :Welcome\r\n"));
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("*playback PLAY")));

    transport->injectBytes(QByteArrayLiteral(":server 376 omairc :End of MOTD\r\n"));
    QCOMPARE(frameCount(transport->writtenFrames(), playZero), 1);
    QCOMPARE(frameCount(transport->writtenFrames(), playChannel), 0);
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("PLAY #omarchy")));
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("PRIVMSG *playback")));

    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    QCOMPARE(frameCount(transport->writtenFrames(), playZero), 1);
    QCOMPARE(frameCount(transport->writtenFrames(), playChannel), 1);

    transport->injectBytes(QByteArrayLiteral(
        ":znc.in BATCH +kept znc.in/playback #omarchy\r\n"
        "@batch=kept;time=2024-03-09T16:00:04.000Z "
        ":lena!u@h PRIVMSG #omarchy :kept\r\n"
        ":znc.in BATCH -kept\r\n"));
    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    QCOMPARE(frameCount(transport->writtenFrames(), playChannel), 1);
    QCOMPARE(frameCount(transport->writtenFrames(), playZero), 1);
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("PRIVMSG *status :*playback")));
}

void ControllerTest::zncPlaybackRestoredDirectWithoutStamp()
{
    const QDateTime room = QDateTime::fromString(
        QStringLiteral("2024-03-09T16:00:00.620Z"), Qt::ISODateWithMs);
    QVERIFY(room.isValid());
    QCOMPARE(ircPlaybackPlayStamp(room), QStringLiteral("1710000000.620"));
    const IrcCaseMapping mapping;
    const QByteArray caps =
        QByteArrayLiteral(":server CAP omairc LS :batch znc.in/playback\r\n"
                          ":server CAP omairc ACK :batch znc.in/playback\r\n");
    const QByteArray playRoom =
        QByteArrayLiteral("ZNC *playback PLAY #omarchy 1710000000.620\r\n");
    const QByteArray playLena =
        QByteArrayLiteral("ZNC *playback PLAY lena 0\r\n");
    const QByteArray playZero =
        QByteArrayLiteral("ZNC *playback PLAY * 0\r\n");

    {
        IrcController seeder;
        auto *transport = new FakeIrcTransport;
        QVERIFY(seeder.addSession(config(QStringLiteral("libera")), transport));
        QVERIFY(seeder.start(QStringLiteral("libera")));
        transport->completeConnect();
        transport->injectBytes(QByteArrayLiteral(
            ":server CAP omairc LS :multi-prefix\r\n"
            ":server 001 omairc :Welcome\r\n"
            "@time=2024-03-09T16:00:00.620Z :alice!u@h PRIVMSG #omarchy :room\r\n"));
    }
    QCOMPARE(ircPlaybackPlayStamp(
                 IrcPlaybackTimeStore().noted(QStringLiteral("libera"),
                                              QStringLiteral("#omarchy"),
                                              mapping)),
             QStringLiteral("1710000000.620"));
    QVERIFY(IrcOpenDirectStore().listed(QStringLiteral("libera"), mapping).isEmpty());

    {
        IrcController controller;
        auto *transport = new FakeIrcTransport;
        QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
        QVERIFY(controller.start(QStringLiteral("libera")));
        transport->completeConnect();
        transport->injectBytes(caps);
        transport->injectBytes(QByteArrayLiteral(":server 001 omairc :Welcome\r\n"));
        QVERIFY(!framesContain(transport->writtenFrames(),
                               QByteArrayLiteral("*playback PLAY")));
        transport->injectBytes(
            QByteArrayLiteral(":server 376 omairc :End of MOTD\r\n"));
        QCOMPARE(frameCount(transport->writtenFrames(), playRoom), 1);
        QCOMPARE(frameCount(transport->writtenFrames(), playLena), 0);
        QCOMPARE(frameCount(transport->writtenFrames(), playZero), 1);
        QVERIFY(!framesContain(transport->writtenFrames(),
                               QByteArrayLiteral("PLAY lena")));
        auto *conversations =
            qobject_cast<QAbstractItemModel *>(controller.conversations());
        QVERIFY(conversations);
        QCOMPARE(rowForTarget(conversations, QStringLiteral("lena")), -1);
    }

    QVERIFY(IrcOpenDirectStore().add(QStringLiteral("libera"),
                                     QStringLiteral("lena"),
                                     mapping));
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(caps);
    transport->injectBytes(QByteArrayLiteral(":server 001 omairc :Welcome\r\n"));
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("*playback PLAY")));
    transport->injectBytes(QByteArrayLiteral(":server 376 omairc :End of MOTD\r\n"));
    QCOMPARE(frameCount(transport->writtenFrames(), playLena), 1);
    QCOMPARE(frameCount(transport->writtenFrames(), playRoom), 1);
    QCOMPARE(frameCount(transport->writtenFrames(), playZero), 1);
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("PLAY lena 1710000000.620")));
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("PRIVMSG *playback")));
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QVERIFY(rowForTarget(conversations, QStringLiteral("lena")) >= 0);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("ghost")), -1);
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("PLAY ghost")));
}

void ControllerTest::playbackBatchOverCeilingKeepsNewestLines()
{
    constexpr int ceiling = 256;
    constexpr int count = ceiling + 44;
    const int firstKept = count - ceiling;
    const int newest = count - 1;
    const int droppedByOldAlgorithm = ceiling;
    const auto at = [](int ms) {
        return QDateTime::fromString(
            QStringLiteral("2024-03-09T16:00:00.%1Z")
                .arg(ms, 3, 10, QLatin1Char('0')),
            Qt::ISODateWithMs);
    };
    const QDateTime newestAt = at(newest);
    const QDateTime liveAt = at(droppedByOldAlgorithm + 14);
    QVERIFY(newestAt.isValid());
    QVERIFY(liveAt.isValid());
    QVERIFY(liveAt > at(ceiling - 1));
    QVERIFY(liveAt < newestAt);

    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(QByteArrayLiteral(
        ":server CAP omairc LS :batch znc.in/playback\r\n"
        ":server CAP omairc ACK :batch znc.in/playback\r\n"
        ":server 001 omairc :Welcome\r\n"
        ":server 376 omairc :End of MOTD\r\n"
        ":omairc!u@h JOIN :#omarchy\r\n"));

    QByteArray batch = QByteArrayLiteral(":znc.in BATCH +pb znc.in/playback #omarchy\r\n");
    for (int index = 0; index < count; ++index) {
        batch += QByteArrayLiteral("@batch=pb;time=2024-03-09T16:00:00.");
        batch += QByteArray::number(index).rightJustified(3, '0');
        batch += QByteArrayLiteral("Z :lena!u@h PRIVMSG #omarchy :p");
        batch += QByteArray::number(index).rightJustified(3, '0');
        batch += QByteArrayLiteral("\r\n");
    }
    batch += QByteArrayLiteral(":znc.in BATCH -pb\r\n");
    transport->injectBytes(batch);

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    const QStringList bodies = messageBodies(messages);
    QCOMPARE(bodies.size(), ceiling);
    QCOMPARE(bodies.constFirst(),
             QStringLiteral("p%1").arg(firstKept, 3, 10, QLatin1Char('0')));
    QCOMPARE(bodies.constLast(),
             QStringLiteral("p%1").arg(newest, 3, 10, QLatin1Char('0')));
    QVERIFY(!bodies.contains(QStringLiteral("p000")));
    QVERIFY(!bodies.contains(
        QStringLiteral("p%1").arg(firstKept - 1, 3, 10, QLatin1Char('0'))));
    QVERIFY(bodies.contains(
        QStringLiteral("p%1").arg(droppedByOldAlgorithm, 3, 10, QLatin1Char('0'))));

    const IrcCaseMapping mapping;
    const QString newestStamp = ircPlaybackPlayStamp(newestAt);
    QCOMPARE(ircPlaybackPlayStamp(
                 IrcPlaybackTimeStore().noted(QStringLiteral("libera"),
                                              QStringLiteral("#omarchy"),
                                              mapping)),
             newestStamp);
    QVERIFY(newestStamp != ircPlaybackPlayStamp(at(ceiling - 1)));

    const int liveMs = droppedByOldAlgorithm + 14;
    transport->injectBytes(
        QByteArrayLiteral("@time=2024-03-09T16:00:00.")
        + QByteArray::number(liveMs).rightJustified(3, '0')
        + QByteArrayLiteral("Z :lena!u@h PRIVMSG #omarchy :live-middle\r\n"));
    QVERIFY(messageBodies(messages).contains(QStringLiteral("live-middle")));
    QCOMPARE(ircPlaybackPlayStamp(
                 IrcPlaybackTimeStore().noted(QStringLiteral("libera"),
                                              QStringLiteral("#omarchy"),
                                              mapping)),
             newestStamp);
    QVERIFY(newestStamp != ircPlaybackPlayStamp(liveAt));
}

void ControllerTest::queryPlaybackPeerBatchFollowsHeldSelfLines()
{
    const QByteArray playLena =
        QByteArrayLiteral("ZNC *playback PLAY lena 1710000001.500\r\n");
    const QByteArray playZero =
        QByteArrayLiteral("ZNC *playback PLAY * 0\r\n");

    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(QByteArrayLiteral(
        ":server CAP omairc LS :batch znc.in/playback\r\n"
        ":server CAP omairc ACK :batch znc.in/playback\r\n"
        ":server 001 omairc :Welcome\r\n"
        ":znc.in BATCH +self znc.in/playback lena\r\n"
        "@batch=self;time=2024-03-09T16:00:00.620Z "
        ":omairc!u@h PRIVMSG lena :held-self\r\n"
        ":znc.in BATCH -self\r\n"
        ":znc.in BATCH +empty znc.in/playback lena\r\n"
        ":znc.in BATCH -empty\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("lena")), -1);
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("*playback PLAY")));

    transport->injectBytes(QByteArrayLiteral(
        ":znc.in BATCH +peer znc.in/playback lena\r\n"
        "@batch=peer;time=2024-03-09T16:00:01.500Z "
        ":lena!u@h PRIVMSG omairc :from-peer\r\n"
        ":znc.in BATCH -peer\r\n"));
    const int opened = rowForTarget(conversations, QStringLiteral("lena"));
    QVERIFY(opened >= 0);
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QCOMPARE(messageBodies(messages),
             QStringList({QStringLiteral("held-self"),
                          QStringLiteral("from-peer")}));
    QCOMPARE(roleAt(messages, bodyRow(messages, QStringLiteral("held-self")),
                    MessageListModel::AuthorRole),
             QStringLiteral("omairc"));
    QCOMPARE(roleAt(messages, bodyRow(messages, QStringLiteral("from-peer")),
                    MessageListModel::AuthorRole),
             QStringLiteral("lena"));
    QCOMPARE(roleAt(messages, bodyRow(messages, QStringLiteral("held-self")),
                    MessageListModel::OriginRole),
             QStringLiteral("replay"));
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("*playback PLAY")));

    transport->injectBytes(
        QByteArrayLiteral(":server 422 omairc :MOTD File is missing\r\n"));
    QCOMPARE(messageBodies(messages),
             QStringList({QStringLiteral("held-self"),
                          QStringLiteral("from-peer")}));
    QCOMPARE(frameCount(transport->writtenFrames(), playLena), 0);
    QCOMPARE(frameCount(transport->writtenFrames(), playZero), 1);
    QVERIFY(!IrcPlaybackTimeStore().newest(QStringLiteral("libera")));
    QCOMPARE(rowForTarget(conversations, QStringLiteral("*playback")), -1);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("*status")), -1);
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("PRIVMSG *playback")));
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("PRIVMSG *status :*playback")));
}

void ControllerTest::queryPlaybackKeepsLinesFromPreviousNick()
{
    const IrcCaseMapping mapping;
    const QDateTime mineAt = QDateTime::fromString(
        QStringLiteral("2024-03-09T16:00:00.620Z"), Qt::ISODateWithMs);
    const QDateTime afterAt = QDateTime::fromString(
        QStringLiteral("2024-03-09T16:00:01.500Z"), Qt::ISODateWithMs);
    const QDateTime channelAt = QDateTime::fromString(
        QStringLiteral("2024-03-09T16:00:04.000Z"), Qt::ISODateWithMs);
    QVERIFY(mineAt.isValid());
    QVERIFY(afterAt.isValid());
    QVERIFY(channelAt.isValid());
    const QString mineStamp = ircPlaybackPlayStamp(mineAt);
    const QString afterStamp = ircPlaybackPlayStamp(afterAt);
    const QString channelStamp = ircPlaybackPlayStamp(channelAt);
    QCOMPARE(mineStamp, QStringLiteral("1710000000.620"));
    QCOMPARE(afterStamp, QStringLiteral("1710000001.500"));
    QVERIFY(channelStamp != afterStamp);
    QVERIFY(channelAt > afterAt);
    const QByteArray caps = QByteArrayLiteral(
        ":server CAP omairc LS :batch znc.in/playback\r\n"
        ":server CAP omairc ACK :batch znc.in/playback\r\n");

    {
        IrcController controller;
        auto *transport = new FakeIrcTransport;
        QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
        QVERIFY(controller.start(QStringLiteral("libera")));
        transport->completeConnect();
        transport->injectBytes(caps);
        transport->injectBytes(QByteArrayLiteral(
            ":server 001 omairc :Welcome\r\n"
            ":server 376 omairc :End of MOTD\r\n"));
        transport->injectBytes(previousNickPlaybackBatch(true));

        auto *conversations =
            qobject_cast<QAbstractItemModel *>(controller.conversations());
        QVERIFY(conversations);
        QVERIFY(rowForTarget(conversations, QStringLiteral("lena")) >= 0);
        QCOMPARE(rowForTarget(conversations, QStringLiteral("#other")), -1);
        QCOMPARE(rowForTarget(conversations, QStringLiteral("stranger")), -1);
        QCOMPARE(rowForTarget(conversations, QStringLiteral("oldnick")), -1);

        controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
        auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
        QVERIFY(messages);
        QCOMPARE(selectedBodies(messages),
                 QStringList({QStringLiteral("before"),
                              QStringLiteral("mine"),
                              QStringLiteral("after")}));
        const QStringList authors{QStringLiteral("lena"),
                                  QStringLiteral("oldnick"),
                                  QStringLiteral("lena")};
        for (int row = 0; row < authors.size(); ++row) {
            QCOMPARE(roleAt(messages, row, MessageListModel::AuthorRole), authors.at(row));
            QCOMPARE(roleAt(messages, row, MessageListModel::OriginRole),
                     QStringLiteral("replay"));
        }

        transport->injectBytes(QByteArrayLiteral(
            ":lena!u@h PRIVMSG oldnick :live-before\r\n"
            ":oldnick!u@h PRIVMSG lena :live-mine\r\n"));
        QCOMPARE(selectedBodies(messages),
                 QStringList({QStringLiteral("before"),
                              QStringLiteral("mine"),
                              QStringLiteral("after")}));

        QCOMPARE(ircPlaybackPlayStamp(
                     IrcPlaybackTimeStore().noted(QStringLiteral("libera"),
                                                  QStringLiteral("lena"),
                                                  mapping)),
                 afterStamp);
        QCOMPARE(ircPlaybackPlayStamp(
                     IrcPlaybackTimeStore().newest(QStringLiteral("libera"))),
                 afterStamp);
        QVERIFY(afterStamp != channelStamp);
    }

    IrcPlaybackTimeStore().forget(QStringLiteral("libera"));

    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(caps);
    transport->injectBytes(QByteArrayLiteral(
        ":server 001 omairc :Welcome\r\n"
        ":server 376 omairc :End of MOTD\r\n"));
    transport->injectBytes(previousNickPlaybackBatch(false));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QCOMPARE(selectedBodies(messages),
             QStringList({QStringLiteral("before"),
                          QStringLiteral("mine")}));
    QCOMPARE(roleAt(messages, 0, MessageListModel::AuthorRole), QStringLiteral("lena"));
    QCOMPARE(roleAt(messages, 1, MessageListModel::AuthorRole),
             QStringLiteral("oldnick"));
    QCOMPARE(roleAt(messages, 0, MessageListModel::OriginRole), QStringLiteral("replay"));
    QCOMPARE(roleAt(messages, 1, MessageListModel::OriginRole), QStringLiteral("replay"));
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("channel")));
    QVERIFY(!selectedBodiesContain(messages, QStringLiteral("elsewhere")));
    QCOMPARE(ircPlaybackPlayStamp(
                 IrcPlaybackTimeStore().noted(QStringLiteral("libera"),
                                              QStringLiteral("lena"),
                                              mapping)),
             mineStamp);
    QCOMPARE(ircPlaybackPlayStamp(
                 IrcPlaybackTimeStore().newest(QStringLiteral("libera"))),
             mineStamp);
}

void ControllerTest::queryPlaybackPreviousNickEchoIsOwnLine()
{
    const IrcCaseMapping mapping;
    const QString laterStamp = QStringLiteral("1710000001.500");
    const QByteArray caps = QByteArrayLiteral(
        ":server CAP omairc LS :batch znc.in/playback\r\n"
        ":server CAP omairc ACK :batch znc.in/playback\r\n");
    const auto forgetStores = [&]() {
        IrcPlaybackTimeStore().forget(QStringLiteral("libera"));
        IrcOpenDirectStore().forget(QStringLiteral("libera"));
    };
    const auto lenaStamp = [&]() {
        return IrcPlaybackTimeStore().noted(QStringLiteral("libera"),
                                            QStringLiteral("lena"),
                                            mapping);
    };

    {
        IrcController controller;
        auto *transport = new FakeIrcTransport;
        QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
        QVERIFY(controller.start(QStringLiteral("libera")));
        transport->completeConnect();
        transport->injectBytes(caps);
        transport->injectBytes(QByteArrayLiteral(
            ":server 001 omairc :Welcome\r\n"
            ":znc.in BATCH +q znc.in/playback lena\r\n"
            "@batch=q;time=2024-03-09T16:00:00.620Z "
            ":oldnick!u@h PRIVMSG lena :mine\r\n"
            ":znc.in BATCH -q\r\n"));

        auto *conversations =
            qobject_cast<QAbstractItemModel *>(controller.conversations());
        QVERIFY(conversations);
        QCOMPARE(rowForTarget(conversations, QStringLiteral("lena")), -1);
        QVERIFY(!lenaStamp());

        transport->injectBytes(QByteArrayLiteral(":server 376 omairc :End of MOTD\r\n"));
        QCOMPARE(rowForTarget(conversations, QStringLiteral("lena")), -1);
        QVERIFY(!IrcOpenDirectStore().listed(QStringLiteral("libera"), mapping)
                     .contains(QStringLiteral("lena")));
        QVERIFY(!lenaStamp());
        QVERIFY(!IrcPlaybackTimeStore().newest(QStringLiteral("libera")));
    }

    forgetStores();

    {
        IrcController controller;
        auto *transport = new FakeIrcTransport;
        QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
        QVERIFY(controller.start(QStringLiteral("libera")));
        transport->completeConnect();
        transport->injectBytes(caps);
        transport->injectBytes(QByteArrayLiteral(
            ":server 001 omairc :Welcome\r\n"
            ":znc.in BATCH +self znc.in/playback lena\r\n"
            "@batch=self;time=2024-03-09T16:00:00.620Z "
            ":omairc!u@h PRIVMSG lena :held\r\n"
            ":znc.in BATCH -self\r\n"
            ":znc.in BATCH +echo znc.in/playback lena\r\n"
            "@batch=echo;time=2024-03-09T16:00:01.500Z "
            ":oldnick!u@h PRIVMSG lena :mine\r\n"
            ":znc.in BATCH -echo\r\n"));

        auto *conversations =
            qobject_cast<QAbstractItemModel *>(controller.conversations());
        QVERIFY(conversations);
        QCOMPARE(rowForTarget(conversations, QStringLiteral("lena")), -1);
        QVERIFY(!lenaStamp());

        transport->injectBytes(QByteArrayLiteral(":server 376 omairc :End of MOTD\r\n"));
        QCOMPARE(rowForTarget(conversations, QStringLiteral("lena")), -1);
        QVERIFY(!IrcOpenDirectStore().listed(QStringLiteral("libera"), mapping)
                     .contains(QStringLiteral("lena")));
        QVERIFY(!lenaStamp());
        QVERIFY(!IrcPlaybackTimeStore().newest(QStringLiteral("libera")));
    }

    forgetStores();

    QVERIFY(IrcOpenDirectStore().add(QStringLiteral("libera"),
                                     QStringLiteral("lena"),
                                     mapping));
    {
        IrcController controller;
        auto *transport = new FakeIrcTransport;
        QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
        QVERIFY(controller.start(QStringLiteral("libera")));
        transport->completeConnect();
        transport->injectBytes(caps);
        transport->injectBytes(QByteArrayLiteral(
            ":server 001 omairc :Welcome\r\n"
            ":server 376 omairc :End of MOTD\r\n"));

        auto *conversations =
            qobject_cast<QAbstractItemModel *>(controller.conversations());
        QVERIFY(conversations);
        const int restored = rowForTarget(conversations, QStringLiteral("lena"));
        QVERIFY(restored >= 0);
        QCOMPARE(roleAt(conversations, restored, ConversationListModel::UnreadRole).toInt(),
                 0);
        controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));

        transport->injectBytes(QByteArrayLiteral(
            ":znc.in BATCH +q znc.in/playback lena\r\n"
            "@batch=q;time=2024-03-09T16:00:00.620Z "
            ":oldnick!u@h PRIVMSG lena :mine\r\n"
            "@batch=q;time=2024-03-09T16:00:01.500Z "
            ":lena!u@h PRIVMSG omairc :later\r\n"
            ":znc.in BATCH -q\r\n"));

        const int row = rowForTarget(conversations, QStringLiteral("lena"));
        QVERIFY(row >= 0);
        QCOMPARE(roleAt(conversations, row, ConversationListModel::UnreadRole).toInt(),
                 0);
        auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
        QVERIFY(messages);
        QCOMPARE(messageBodies(messages),
                 QStringList({QStringLiteral("mine"), QStringLiteral("later")}));
        const int mine = bodyRow(messages, QStringLiteral("mine"));
        const int later = bodyRow(messages, QStringLiteral("later"));
        QVERIFY(mine >= 0);
        QVERIFY(later > mine);
        QCOMPARE(roleAt(messages, mine, MessageListModel::AuthorRole),
                 QStringLiteral("oldnick"));
        QCOMPARE(roleAt(messages, later, MessageListModel::AuthorRole),
                 QStringLiteral("lena"));
        QCOMPARE(roleAt(messages, mine, MessageListModel::OriginRole),
                 QStringLiteral("replay"));
        QCOMPARE(roleAt(messages, later, MessageListModel::OriginRole),
                 QStringLiteral("replay"));
        QCOMPARE(ircPlaybackPlayStamp(lenaStamp()), laterStamp);
        QVERIFY(IrcOpenDirectStore().listed(QStringLiteral("libera"), mapping)
                    .contains(QStringLiteral("lena")));
    }

    forgetStores();

    {
        IrcController controller;
        auto *transport = new FakeIrcTransport;
        QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
        QVERIFY(controller.start(QStringLiteral("libera")));
        transport->completeConnect();
        transport->injectBytes(caps);
        transport->injectBytes(QByteArrayLiteral(
            ":server 001 omairc :Welcome\r\n"
            ":znc.in BATCH +q znc.in/playback lena\r\n"
            "@batch=q;time=2024-03-09T16:00:00.100Z "
            ":lena!u@h PRIVMSG oldnick :before\r\n"
            ":znc.in BATCH -q\r\n"));

        auto *conversations =
            qobject_cast<QAbstractItemModel *>(controller.conversations());
        QVERIFY(conversations);
        QVERIFY(rowForTarget(conversations, QStringLiteral("lena")) >= 0);
        controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
        auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
        QVERIFY(messages);
        QCOMPARE(selectedBodies(messages), QStringList({QStringLiteral("before")}));
        QCOMPARE(roleAt(messages, 0, MessageListModel::AuthorRole),
                 QStringLiteral("lena"));
        QCOMPARE(roleAt(messages, 0, MessageListModel::OriginRole),
                 QStringLiteral("replay"));
    }
}

void ControllerTest::networkWithoutPlaybackCapDoesNotPlay()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    IrcSession *session = controller.addSession(config(QStringLiteral("libera")),
                                                transport);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory\r\n"
                          ":server CAP omairc ACK :batch chathistory\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":znc.in BATCH +pb znc.in/playback #omarchy\r\n"
                          "@batch=pb :lena!u@h PRIVMSG #omarchy :yesterday\r\n"
                          ":znc.in BATCH -pb\r\n"));
    QVERIFY(session->capabilities().contains(IrcCapability::Batch));
    QVERIFY(session->capabilities().contains(IrcCapability::ChatHistory));
    QVERIFY(!session->capabilities().contains(IrcCapability::ZncPlayback));
    QVERIFY(framesContain(transport->writtenFrames(),
                          QByteArrayLiteral("CHATHISTORY LATEST #omarchy * 100\r\n")));
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("*playback PLAY")));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QCOMPARE(selectedBodies(messages).count(QStringLiteral("yesterday")), 1);
    QCOMPARE(roleAt(messages, bodyRow(messages, QStringLiteral("yesterday")),
                    MessageListModel::OriginRole),
             QStringLiteral("replay"));
}

void ControllerTest::zncPlaybackLateCapPlaysOnce()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch\r\n"
                          ":server CAP omairc ACK :batch\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("*playback PLAY")));

    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc NEW :znc.in/playback\r\n"));
    QVERIFY(framesContain(transport->writtenFrames(),
                          QByteArrayLiteral("CAP REQ :znc.in/playback\r\n")));
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc ACK :znc.in/playback\r\n"));
    const QByteArray playZero =
        QByteArrayLiteral("ZNC *playback PLAY * 0\r\n");
    QCOMPARE(frameCount(transport->writtenFrames(), playZero), 0);
    transport->injectBytes(
        QByteArrayLiteral(":server 376 omairc :End of MOTD\r\n"));
    QCOMPARE(frameCount(transport->writtenFrames(), playZero), 1);

    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc NEW :znc.in/playback\r\n"
                          ":server CAP omairc ACK :znc.in/playback\r\n"));
    QCOMPARE(frameCount(transport->writtenFrames(), playZero), 1);
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("PRIVMSG *playback")));
}

void ControllerTest::engagedQueryPlaybackPlaysAfterRestore()
{
    const QByteArray playZero =
        QByteArrayLiteral("ZNC *playback PLAY * 0\r\n");
    const QByteArray playEngaged =
        QByteArrayLiteral("ZNC *playback PLAY lena 1710000000.620\r\n");
    const QByteArray caps =
        QByteArrayLiteral(":server CAP omairc LS :batch znc.in/playback\r\n"
                          ":server CAP omairc ACK :batch znc.in/playback\r\n");
    const IrcCaseMapping mapping;

    {
        IrcController controller;
        auto *transport = new FakeIrcTransport;
        QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
        QVERIFY(controller.start(QStringLiteral("libera")));
        transport->completeConnect();
        transport->injectBytes(caps);
        transport->injectBytes(QByteArrayLiteral(
            ":server 001 omairc :Welcome\r\n"
            ":server 376 omairc :End of MOTD\r\n"
            ":znc.in BATCH +q znc.in/playback lena\r\n"
            "@batch=q;time=2024-03-09T16:00:00.100Z :lena!u@h PRIVMSG omairc :ping\r\n"
            "@batch=q;time=2024-03-09T16:00:00.620Z :omairc!u@h PRIVMSG lena :pong\r\n"
            ":znc.in BATCH -q\r\n"));

        auto *conversations =
            qobject_cast<QAbstractItemModel *>(controller.conversations());
        QVERIFY(conversations);
        QVERIFY(rowForTarget(conversations, QStringLiteral("lena")) >= 0);
        QVERIFY(IrcOpenDirectStore().listed(QStringLiteral("libera"), mapping)
                    .contains(QStringLiteral("lena")));
        QCOMPARE(ircPlaybackPlayStamp(
                     IrcPlaybackTimeStore().newest(QStringLiteral("libera"))),
                 QStringLiteral("1710000000.620"));
    }

    IrcController again;
    auto *transport = new FakeIrcTransport;
    QVERIFY(again.addSession(config(QStringLiteral("libera")), transport));
    QVERIFY(again.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(caps);
    transport->injectBytes(QByteArrayLiteral(":server 001 omairc :Welcome\r\n"));
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("*playback PLAY")));
    auto *conversations =
        qobject_cast<QAbstractItemModel *>(again.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("lena")), -1);

    transport->injectBytes(QByteArrayLiteral(":server 376 omairc :End of MOTD\r\n"));
    const int restored = rowForTarget(conversations, QStringLiteral("lena"));
    QVERIFY(restored >= 0);
    QCOMPARE(roleAt(conversations, restored, ConversationListModel::UnreadRole).toInt(),
             0);
    QCOMPARE(frameCount(transport->writtenFrames(), playEngaged), 1);
    QCOMPARE(frameCount(transport->writtenFrames(), playZero), 1);

    transport->injectBytes(QByteArrayLiteral(
        ":znc.in BATCH +later znc.in/playback lena\r\n"
        "@batch=later;time=2024-03-09T16:00:01.500Z :omairc!u@h PRIVMSG lena :later\r\n"
        ":znc.in BATCH -later\r\n"));
    const int row = rowForTarget(conversations, QStringLiteral("lena"));
    QVERIFY(row >= 0);
    QCOMPARE(roleAt(conversations, row, ConversationListModel::UnreadRole).toInt(), 0);
    again.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    auto *messages = qobject_cast<QAbstractItemModel *>(again.messages());
    QVERIFY(messages);
    const int later = bodyRow(messages, QStringLiteral("later"));
    QVERIFY(later >= 0);
    QCOMPARE(roleAt(messages, later, MessageListModel::OriginRole),
             QStringLiteral("replay"));
    QCOMPARE(ircPlaybackPlayStamp(
                 IrcPlaybackTimeStore().newest(QStringLiteral("libera"))),
             QStringLiteral("1710000001.500"));
}

void ControllerTest::selfOnlyStoredQueryPlaybackLandsAfterMotd()
{
    const IrcCaseMapping mapping;
    QVERIFY(IrcOpenDirectStore().add(QStringLiteral("libera"),
                                     QStringLiteral("lena"),
                                     mapping));
    const QByteArray playHeld =
        QByteArrayLiteral("ZNC *playback PLAY lena 1710000001.500\r\n");
    const QByteArray playZero =
        QByteArrayLiteral("ZNC *playback PLAY * 0\r\n");

    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(QByteArrayLiteral(
        ":server CAP omairc LS :batch znc.in/playback\r\n"
        ":server CAP omairc ACK :batch znc.in/playback\r\n"
        ":server 001 omairc :Welcome\r\n"
        ":znc.in BATCH +q znc.in/playback lena\r\n"
        "@batch=q;time=2024-03-09T16:00:01.500Z :omairc!u@h PRIVMSG lena :held\r\n"
        ":znc.in BATCH -q\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("lena")), -1);
    QVERIFY(!IrcPlaybackTimeStore().newest(QStringLiteral("libera")));
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("*playback PLAY")));

    transport->injectBytes(QByteArrayLiteral(":server 376 omairc :End of MOTD\r\n"));
    const int row = rowForTarget(conversations, QStringLiteral("lena"));
    QVERIFY(row >= 0);
    QCOMPARE(roleAt(conversations, row, ConversationListModel::UnreadRole).toInt(), 0);
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    const int held = bodyRow(messages, QStringLiteral("held"));
    QVERIFY(held >= 0);
    QCOMPARE(roleAt(messages, held, MessageListModel::OriginRole),
             QStringLiteral("replay"));
    QVERIFY(!IrcPlaybackTimeStore().newest(QStringLiteral("libera")));
    QCOMPARE(frameCount(transport->writtenFrames(), playHeld), 0);
    QCOMPARE(frameCount(transport->writtenFrames(), playZero), 1);
}

void ControllerTest::selfOnlyUnstoredQueryPlaybackDoesNotAdvanceClock()
{
    const QByteArray playZero =
        QByteArrayLiteral("ZNC *playback PLAY * 0\r\n");
    const IrcCaseMapping mapping;
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(QByteArrayLiteral(
        ":server CAP omairc LS :batch znc.in/playback\r\n"
        ":server CAP omairc ACK :batch znc.in/playback\r\n"
        ":server 001 omairc :Welcome\r\n"
        ":znc.in BATCH +q znc.in/playback ghost\r\n"
        "@batch=q;time=2024-03-09T16:00:01.500Z :omairc!u@h PRIVMSG ghost :alone\r\n"
        ":znc.in BATCH -q\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("ghost")), -1);
    QVERIFY(!IrcPlaybackTimeStore().newest(QStringLiteral("libera")));

    transport->injectBytes(
        QByteArrayLiteral(":server 422 omairc :MOTD File is missing\r\n"));
    QCOMPARE(rowForTarget(conversations, QStringLiteral("ghost")), -1);
    QVERIFY(!IrcPlaybackTimeStore().newest(QStringLiteral("libera")));
    QVERIFY(!IrcOpenDirectStore().listed(QStringLiteral("libera"), mapping)
                 .contains(QStringLiteral("ghost")));
    QCOMPARE(frameCount(transport->writtenFrames(), playZero), 1);

    transport->injectBytes(QByteArrayLiteral(
        ":znc.in BATCH +q2 znc.in/playback ghost\r\n"
        "@batch=q2;time=2024-03-09T16:00:04.000Z :omairc!u@h PRIVMSG ghost :still alone\r\n"
        ":znc.in BATCH -q2\r\n"));
    QCOMPARE(rowForTarget(conversations, QStringLiteral("ghost")), -1);
    QVERIFY(!IrcPlaybackTimeStore().newest(QStringLiteral("libera")));
    QCOMPARE(frameCount(transport->writtenFrames(), playZero), 1);
}

void ControllerTest::playbackReplaySkipsLinesAlreadyInTheTranscript()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString root = dir.filePath(QStringLiteral("omairc/logs"));
    ScopedTranscriptRoot scope(root);
    IrcConversationLog paths(root);
    const QString labPath =
        paths.pathFor(QStringLiteral("libera"), QStringLiteral("#lab"));

    {
        IrcController controller;
        auto *transport = new FakeIrcTransport;
        QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
        QVERIFY(controller.start(QStringLiteral("libera")));
        transport->completeConnect();
        transport->injectBytes(
            QByteArrayLiteral(":server CAP omairc LS :batch\r\n"
                              ":server CAP omairc ACK :batch\r\n"
                              ":server 001 omairc :Welcome\r\n"
                              ":omairc!u@h JOIN :#omarchy\r\n"
                              ":omairc!u@h JOIN :#lab\r\n"
                              "@time=2024-03-09T16:00:00.620Z;msgid=room-1 "
                              ":lena!u@h PRIVMSG #lab :yesterday\r\n"
                              "@time=2024-03-09T16:00:00.620Z;msgid=room-2 "
                              ":lena!u@h PRIVMSG #lab :yesterday\r\n"));

        auto *conversations =
            qobject_cast<QAbstractItemModel *>(controller.conversations());
        QVERIFY(conversations);
        const int lab = rowForTarget(conversations, QStringLiteral("#lab"));
        QVERIFY(lab >= 0);
        QCOMPARE(roleAt(conversations, lab, ConversationListModel::UnreadRole).toInt(),
                 2);

        transport->injectBytes(QByteArrayLiteral(
            ":znc.in BATCH +c znc.in/playback #lab\r\n"
            "@batch=c;msgid=room-1;time=2024-03-09T16:00:02.000Z "
            ":lena!u@h PRIVMSG #lab :different body\r\n"
            "@batch=c;time=2024-03-09T16:00:00.620Z "
            ":Lena!u@h PRIVMSG #lab :yesterday\r\n"
            "@batch=c;time=2024-03-09T16:00:03.000Z "
            ":lena!u@h PRIVMSG #lab :newer\r\n"
            ":znc.in BATCH -c\r\n"));

        QCOMPARE(roleAt(conversations, lab, ConversationListModel::UnreadRole).toInt(),
                 3);
        controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#lab"));
        auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
        QVERIFY(messages);
        const QStringList bodies = selectedBodies(messages);
        QCOMPARE(bodies.count(QStringLiteral("yesterday")), 2);
        QCOMPARE(bodies.count(QStringLiteral("different body")), 0);
        QCOMPARE(bodies.count(QStringLiteral("newer")), 1);
        QCOMPARE(roleAt(messages, bodyRow(messages, QStringLiteral("newer")),
                        MessageListModel::OriginRole),
                 QStringLiteral("replay"));
        QCOMPARE(jsonlBodies(labPath).count(QStringLiteral("yesterday")), 2);
        QCOMPARE(jsonlBodies(labPath).count(QStringLiteral("newer")), 1);
        QCOMPARE(jsonlBodies(labPath).count(QStringLiteral("different body")), 0);
    }

    IrcController restarted;
    auto *transport = new FakeIrcTransport;
    QVERIFY(restarted.addSession(config(QStringLiteral("libera")), transport));
    QVERIFY(restarted.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch\r\n"
                          ":server CAP omairc ACK :batch\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":omairc!u@h JOIN :#lab\r\n"
                          ":znc.in BATCH +c znc.in/playback #lab\r\n"
                          "@batch=c;msgid=room-1;time=2024-03-09T16:00:00.620Z "
                          ":Lena!u@h PRIVMSG #lab :yesterday\r\n"
                          "@batch=c;msgid=replay-new;time=2024-03-09T16:00:04.000Z "
                          ":lena!u@h PRIVMSG #lab :after restart\r\n"
                          ":znc.in BATCH -c\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(restarted.conversations());
    QVERIFY(conversations);
    const int lab = rowForTarget(conversations, QStringLiteral("#lab"));
    QVERIFY(lab >= 0);
    QCOMPARE(roleAt(conversations, lab, ConversationListModel::UnreadRole).toInt(), 1);
    restarted.selectConversation(QStringLiteral("libera"), QStringLiteral("#lab"));
    auto *messages = qobject_cast<QAbstractItemModel *>(restarted.messages());
    QVERIFY(messages);
    const QStringList bodies = selectedBodies(messages);
    QCOMPARE(bodies.count(QStringLiteral("yesterday")), 2);
    QCOMPARE(bodies.count(QStringLiteral("after restart")), 1);
    QCOMPARE(roleAt(messages, bodyRow(messages, QStringLiteral("after restart")),
                    MessageListModel::OriginRole),
             QStringLiteral("replay"));
    QCOMPARE(jsonlBodies(labPath).count(QStringLiteral("yesterday")), 2);
    QCOMPARE(jsonlBodies(labPath).count(QStringLiteral("after restart")), 1);
}

void ControllerTest::preJoinChannelPlaybackSplicesAboveSelfJoin()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch znc.in/playback\r\n"
                          ":server CAP omairc ACK :batch znc.in/playback\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("*playback PLAY")));
    transport->injectBytes(
        QByteArrayLiteral(":server 376 omairc :End of MOTD\r\n"));
    QVERIFY(framesContain(
        transport->writtenFrames(),
        QByteArrayLiteral("ZNC *playback PLAY * 0\r\n")));

    transport->injectBytes(QByteArrayLiteral(
        ":znc.in BATCH +pb znc.in/playback #omarchy\r\n"
        "@batch=pb;time=nope :alice!u@h PRIVMSG #omarchy :badclock\r\n"
        "@batch=pb :bob!u@h PRIVMSG #omarchy :untagged\r\n"
        "@batch=pb;time=2024-03-09T16:00:00.620Z :lena!u@h PRIVMSG #omarchy "
        ":before join\r\n"
        ":znc.in BATCH -pb\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("#omarchy")), -1);
    QVERIFY(!IrcPlaybackTimeStore().newest(QStringLiteral("libera")));

    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    const int row = rowForTarget(conversations, QStringLiteral("#omarchy"));
    QVERIFY(row >= 0);
    QCOMPARE(roleAt(conversations, row, ConversationListModel::UnreadRole).toInt(),
             0);

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    const QStringList chat = messageBodies(messages);
    QCOMPARE(chat,
             QStringList({QStringLiteral("badclock"),
                          QStringLiteral("untagged"),
                          QStringLiteral("before join")}));
    const int replayRow = bodyRow(messages, QStringLiteral("before join"));
    const int joinRow = bodyRow(messages, QStringLiteral("omairc joined"));
    QVERIFY(replayRow >= 0);
    QVERIFY(joinRow > replayRow);
    QCOMPARE(roleAt(messages, replayRow, MessageListModel::OriginRole),
             QStringLiteral("replay"));
    QCOMPARE(roleAt(messages, joinRow, MessageListModel::OriginRole),
             QStringLiteral("live"));
    QCOMPARE(ircPlaybackPlayStamp(
                 IrcPlaybackTimeStore().newest(QStringLiteral("libera"))),
             QStringLiteral("1710000000.620"));
}

void ControllerTest::heldPlaybackBatchesSpliceInOrder()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(QByteArrayLiteral(
        ":server CAP omairc LS :batch znc.in/playback\r\n"
        ":server CAP omairc ACK :batch znc.in/playback\r\n"
        ":server 001 omairc :Welcome\r\n"
        ":server 376 omairc :End of MOTD\r\n"
        ":znc.in BATCH +a znc.in/playback #omarchy\r\n"
        "@batch=a;time=2024-03-09T16:00:00.100Z :lena!u@h PRIVMSG #omarchy "
        ":from first\r\n"
        "@batch=a;time=2024-03-09T16:00:00.620Z :lena!u@h PRIVMSG #omarchy "
        ":earlier\r\n"
        ":znc.in BATCH -a\r\n"
        ":znc.in BATCH +b znc.in/playback #omarchy\r\n"
        "@batch=b;time=2024-03-09T16:00:00.620Z :lena!u@h PRIVMSG #omarchy "
        ":earlier\r\n"
        "@batch=b;time=2024-03-09T16:00:01.500Z :lena!u@h PRIVMSG #omarchy "
        ":later\r\n"
        ":znc.in BATCH -b\r\n"
        ":znc.in BATCH +empty znc.in/playback #omarchy\r\n"
        ":znc.in BATCH -empty\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("#omarchy")), -1);

    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QCOMPARE(transcriptBodies(messages),
             QStringList({QStringLiteral("from first"),
                          QStringLiteral("earlier"),
                          QStringLiteral("later"),
                          QStringLiteral("omairc joined")}));
    QCOMPARE(selectedBodies(messages).count(QStringLiteral("earlier")), 1);
    const int firstRow = bodyRow(messages, QStringLiteral("from first"));
    const int laterRow = bodyRow(messages, QStringLiteral("later"));
    const int joinRow = bodyRow(messages, QStringLiteral("omairc joined"));
    QVERIFY(firstRow >= 0);
    QVERIFY(laterRow > firstRow);
    QVERIFY(joinRow > laterRow);
}

void ControllerTest::preJoinChannelPlaybackWithoutJoinDoesNotAdvanceClock()
{
    const QByteArray playZero =
        QByteArrayLiteral("ZNC *playback PLAY * 0\r\n");
    const QByteArray playKept =
        QByteArrayLiteral("ZNC *playback PLAY #omarchy 1710000000.620\r\n");
    const QByteArray playDropped =
        QByteArrayLiteral("ZNC *playback PLAY * 1710000001.500\r\n");

    {
        IrcSessionConfig sessionConfig = config(QStringLiteral("libera"));
        sessionConfig.reconnectEnabled = true;
        IrcController controller;
        auto *transport = new FakeIrcTransport;
        auto *timer = new FakeReconnectTimer;
        QVERIFY(controller.addSession(sessionConfig, transport, timer));
        QVERIFY(controller.start(QStringLiteral("libera")));
        transport->completeConnect();
        transport->injectBytes(
            QByteArrayLiteral(":server CAP omairc LS :batch znc.in/playback\r\n"
                              ":server CAP omairc ACK :batch znc.in/playback\r\n"
                              ":server 001 omairc :Welcome\r\n"
                              ":znc.in BATCH +pb znc.in/playback #ghost\r\n"
                              "@batch=pb;time=2024-03-09T16:00:01.500Z "
                              ":lena!u@h PRIVMSG #ghost :never joined\r\n"
                              ":znc.in BATCH -pb\r\n"));

        auto *conversations =
            qobject_cast<QAbstractItemModel *>(controller.conversations());
        QVERIFY(conversations);
        QCOMPARE(rowForTarget(conversations, QStringLiteral("#ghost")), -1);
        QVERIFY(!IrcPlaybackTimeStore().newest(QStringLiteral("libera")));
        transport->injectBytes(
            QByteArrayLiteral(":server 376 omairc :End of MOTD\r\n"));
        QCOMPARE(frameCount(transport->writtenFrames(), playZero), 1);
        QVERIFY(!IrcPlaybackTimeStore().newest(QStringLiteral("libera")));

        transport->remoteClose();
        QVERIFY(timer->active);
        timer->fire();
        transport->completeConnect();
        transport->injectBytes(
            QByteArrayLiteral(":server CAP omairc LS :batch znc.in/playback\r\n"
                              ":server CAP omairc ACK :batch znc.in/playback\r\n"
                              ":server 001 omairc :Welcome\r\n"));
        QCOMPARE(frameCount(transport->writtenFrames(), playZero), 1);
        transport->injectBytes(
            QByteArrayLiteral(":server 376 omairc :End of MOTD\r\n"));
        QCOMPARE(frameCount(transport->writtenFrames(), playZero), 2);
        QCOMPARE(frameCount(transport->writtenFrames(), playDropped), 0);

        transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#ghost\r\n"));
        controller.selectConversation(QStringLiteral("libera"),
                                      QStringLiteral("#ghost"));
        auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
        QVERIFY(messages);
        QCOMPARE(selectedBodies(messages),
                 QStringList{QStringLiteral("omairc joined")});
        QVERIFY(!IrcPlaybackTimeStore().newest(QStringLiteral("libera")));
    }

    IrcSessionConfig sessionConfig = config(QStringLiteral("libera"));
    sessionConfig.reconnectEnabled = true;
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    auto *timer = new FakeReconnectTimer;
    QVERIFY(controller.addSession(sessionConfig, transport, timer));
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch znc.in/playback\r\n"
                          ":server CAP omairc ACK :batch znc.in/playback\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":server 376 omairc :End of MOTD\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":znc.in BATCH +kept znc.in/playback #omarchy\r\n"
                          "@batch=kept;time=2024-03-09T16:00:00.620Z "
                          ":lena!u@h PRIVMSG #omarchy :kept\r\n"
                          ":znc.in BATCH -kept\r\n"
                          ":znc.in BATCH +drop znc.in/playback #ghost\r\n"
                          "@batch=drop;time=2024-03-09T16:00:01.500Z "
                          ":lena!u@h PRIVMSG #ghost :never joined\r\n"
                          ":znc.in BATCH -drop\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("#ghost")), -1);
    QCOMPARE(ircPlaybackPlayStamp(
                 IrcPlaybackTimeStore().newest(QStringLiteral("libera"))),
             QStringLiteral("1710000000.620"));

    transport->remoteClose();
    QVERIFY(timer->active);
    timer->fire();
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch znc.in/playback\r\n"
                          ":server CAP omairc ACK :batch znc.in/playback\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    QCOMPARE(frameCount(transport->writtenFrames(), playKept), 0);
    transport->injectBytes(
        QByteArrayLiteral(":server 376 omairc :End of MOTD\r\n"));
    QCOMPARE(frameCount(transport->writtenFrames(), playKept), 1);
    QCOMPARE(frameCount(transport->writtenFrames(), playDropped), 0);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("#ghost")), -1);
}

void ControllerTest::preJoinPlaybackLeavesAnchorForChatHistory()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory znc.in/playback\r\n"
                          ":server CAP omairc ACK :batch chathistory znc.in/playback\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":znc.in BATCH +pb znc.in/playback #omarchy\r\n"
                          "@batch=pb;time=2024-03-09T16:00:00.620Z "
                          ":lena!u@h PRIVMSG #omarchy :from znc\r\n"
                          ":znc.in BATCH -pb\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("#omarchy")), -1);

    transport->injectBytes(QByteArrayLiteral(
        ":omairc!u@h JOIN :#omarchy\r\n"
        ":irc.host BATCH +hx chathistory #omarchy\r\n"
        "@batch=hx;time=2024-03-09T16:00:05.000Z "
        ":alice!u@h PRIVMSG #omarchy :from chathistory\r\n"
        ":irc.host BATCH -hx\r\n"));
    QVERIFY(framesContain(
        transport->writtenFrames(),
        QByteArrayLiteral("CHATHISTORY LATEST #omarchy * 100\r\n")));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QCOMPARE(messageBodies(messages),
             QStringList({QStringLiteral("from znc"),
                          QStringLiteral("from chathistory")}));
    const int zncRow = bodyRow(messages, QStringLiteral("from znc"));
    const int historyRow = bodyRow(messages, QStringLiteral("from chathistory"));
    const int joinRow = bodyRow(messages, QStringLiteral("omairc joined"));
    QVERIFY(zncRow >= 0);
    QVERIFY(historyRow > zncRow);
    QVERIFY(joinRow > historyRow);
    QCOMPARE(roleAt(messages, zncRow, MessageListModel::OriginRole),
             QStringLiteral("replay"));
    QCOMPARE(roleAt(messages, historyRow, MessageListModel::OriginRole),
             QStringLiteral("replay"));
    QCOMPARE(roleAt(messages, joinRow, MessageListModel::OriginRole),
             QStringLiteral("live"));
}

void ControllerTest::openChannelPlaybackBeforeRejoinSplicesAboveNewJoin()
{
    IrcSessionConfig sessionConfig = config(QStringLiteral("libera"));
    sessionConfig.reconnectEnabled = true;
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    auto *timer = new FakeReconnectTimer;
    QVERIFY(controller.addSession(sessionConfig, transport, timer));
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch znc.in/playback\r\n"
                          ":server CAP omairc ACK :batch znc.in/playback\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QVERIFY(rowForTarget(conversations, QStringLiteral("#omarchy")) >= 0);

    transport->remoteClose();
    QVERIFY(timer->active);
    timer->fire();
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch znc.in/playback\r\n"
                          ":server CAP omairc ACK :batch znc.in/playback\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":znc.in BATCH +pb znc.in/playback #omarchy\r\n"
                          "@batch=pb;time=2024-03-09T16:00:04.000Z "
                          ":lena!u@h PRIVMSG #omarchy :while away\r\n"
                          ":znc.in BATCH -pb\r\n"));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QCOMPARE(selectedBodies(messages),
             QStringList{QStringLiteral("omairc joined")});
    QVERIFY(!selectedBodies(messages).contains(QStringLiteral("while away")));

    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    QCOMPARE(transcriptBodies(messages),
             QStringList({QStringLiteral("omairc joined"),
                          QStringLiteral("while away"),
                          QStringLiteral("omairc joined")}));
    const int awayRow = bodyRow(messages, QStringLiteral("while away"));
    QVERIFY(awayRow > 0);
    QCOMPARE(roleAt(messages, awayRow, MessageListModel::OriginRole),
             QStringLiteral("replay"));
    QCOMPARE(roleAt(messages, bodyRow(messages, QStringLiteral("omairc joined")),
                    MessageListModel::OriginRole),
             QStringLiteral("live"));
    int laterJoin = -1;
    for (int row = messages->rowCount() - 1; row > awayRow; --row) {
        if (roleAt(messages, row, MessageListModel::BodyRole).toString()
            == QStringLiteral("omairc joined")) {
            laterJoin = row;
            break;
        }
    }
    QVERIFY(laterJoin > awayRow);
    QCOMPARE(roleAt(messages, laterJoin, MessageListModel::OriginRole),
             QStringLiteral("live"));
}

void ControllerTest::joinedChannelPlaybackSplicesAfterAnchorIsGone()
{
    const QDateTime historyAt = QDateTime::fromString(
        QStringLiteral("2024-03-09T16:00:00.620Z"), Qt::ISODateWithMs);
    const QDateTime anchoredAt = QDateTime::fromString(
        QStringLiteral("2024-03-09T16:00:01.500Z"), Qt::ISODateWithMs);
    const QDateTime clearedAt = QDateTime::fromString(
        QStringLiteral("2024-03-09T16:00:04.000Z"), Qt::ISODateWithMs);
    QVERIFY(historyAt.isValid());
    QVERIFY(anchoredAt.isValid());
    QVERIFY(clearedAt.isValid());
    const QString clearedStamp = ircPlaybackPlayStamp(clearedAt);
    const QByteArray playZero =
        QByteArrayLiteral("ZNC *playback PLAY * 0\r\n");
    const QByteArray playOmarchy =
        QByteArrayLiteral("ZNC *playback PLAY #omarchy 1710000001.500\r\n");
    const QByteArray playCleared =
        QByteArrayLiteral("ZNC *playback PLAY #cleared ")
        + clearedStamp.toUtf8() + QByteArrayLiteral("\r\n");
    const QByteArray playWildcard =
        QByteArrayLiteral("ZNC *playback PLAY * ")
        + clearedStamp.toUtf8() + QByteArrayLiteral("\r\n");

    IrcSessionConfig sessionConfig = config(QStringLiteral("libera"));
    sessionConfig.reconnectEnabled = true;
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    auto *timer = new FakeReconnectTimer;
    QVERIFY(controller.addSession(sessionConfig, transport, timer));
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory znc.in/playback\r\n"
                          ":server CAP omairc ACK :batch chathistory znc.in/playback\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":server 376 omairc :End of MOTD\r\n"
                          ":omairc!u@h JOIN :#omarchy\r\n"
                          ":irc.host BATCH +hx chathistory #omarchy\r\n"
                          "@batch=hx;time=2024-03-09T16:00:00.620Z "
                          ":alice!u@h PRIVMSG #omarchy :from history\r\n"
                          ":irc.host BATCH -hx\r\n"
                          ":znc.in BATCH +pb znc.in/playback #omarchy\r\n"
                          "@batch=pb;time=2024-03-09T16:00:01.500Z "
                          ":lena!u@h PRIVMSG #omarchy :after anchor\r\n"
                          ":znc.in BATCH -pb\r\n"));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *anchored = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(anchored);
    QCOMPARE(transcriptBodies(anchored),
             QStringList({QStringLiteral("from history"),
                          QStringLiteral("omairc joined"),
                          QStringLiteral("after anchor")}));
    const int historyRow = bodyRow(anchored, QStringLiteral("from history"));
    const int anchorJoin = bodyRow(anchored, QStringLiteral("omairc joined"));
    const int afterAnchor = bodyRow(anchored, QStringLiteral("after anchor"));
    QVERIFY(historyRow >= 0);
    QVERIFY(anchorJoin > historyRow);
    QVERIFY(afterAnchor > anchorJoin);
    QCOMPARE(roleAt(anchored, historyRow, MessageListModel::OriginRole),
             QStringLiteral("replay"));
    QCOMPARE(roleAt(anchored, afterAnchor, MessageListModel::OriginRole),
             QStringLiteral("replay"));

    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#cleared\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#cleared"));
    QVERIFY(controller.sendMessage(QStringLiteral("/clear")));
    auto *cleared = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(cleared);
    QCOMPARE(cleared->rowCount(), 0);
    transport->injectBytes(QByteArrayLiteral(
        ":znc.in BATCH +pc znc.in/playback #cleared\r\n"
        "@batch=pc;time=2024-03-09T16:00:04.000Z "
        ":lena!u@h PRIVMSG #cleared :after clear\r\n"
        ":znc.in BATCH -pc\r\n"));
    QCOMPARE(selectedBodies(cleared),
             QStringList{QStringLiteral("after clear")});
    QCOMPARE(roleAt(cleared, bodyRow(cleared, QStringLiteral("after clear")),
                    MessageListModel::OriginRole),
             QStringLiteral("replay"));
    QCOMPARE(ircPlaybackPlayStamp(
                 IrcPlaybackTimeStore().newest(QStringLiteral("libera"))),
             clearedStamp);
    QCOMPARE(ircPlaybackPlayStamp(historyAt), QStringLiteral("1710000000.620"));
    QCOMPARE(ircPlaybackPlayStamp(anchoredAt), QStringLiteral("1710000001.500"));

    transport->remoteClose();
    QVERIFY(timer->active);
    timer->fire();
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory znc.in/playback\r\n"
                          ":server CAP omairc ACK :batch chathistory znc.in/playback\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    QCOMPARE(frameCount(transport->writtenFrames(), playZero), 1);
    QCOMPARE(frameCount(transport->writtenFrames(), playOmarchy), 0);
    QCOMPARE(frameCount(transport->writtenFrames(), playCleared), 0);
    QCOMPARE(frameCount(transport->writtenFrames(), playWildcard), 0);
    transport->injectBytes(
        QByteArrayLiteral(":server 376 omairc :End of MOTD\r\n"));
    QCOMPARE(frameCount(transport->writtenFrames(), playZero), 2);
    QCOMPARE(frameCount(transport->writtenFrames(), playOmarchy), 1);
    QCOMPARE(frameCount(transport->writtenFrames(), playCleared), 1);
    QCOMPARE(frameCount(transport->writtenFrames(), playWildcard), 0);
    QCOMPARE(frameCount(transport->writtenFrames(),
                        QByteArrayLiteral("ZNC *playback PLAY #omarchy 0\r\n")),
             1);
    QCOMPARE(frameCount(transport->writtenFrames(),
                        QByteArrayLiteral("ZNC *playback PLAY #cleared 0\r\n")),
             1);
}

void ControllerTest::selfEchoWithoutConversationDoesNotAdvancePlaybackClock()
{
    const QDateTime peerAt = QDateTime::fromString(
        QStringLiteral("2024-03-09T16:00:00.620Z"), Qt::ISODateWithMs);
    const QDateTime keptAt = QDateTime::fromString(
        QStringLiteral("2024-03-09T16:00:01.500Z"), Qt::ISODateWithMs);
    QVERIFY(peerAt.isValid());
    QVERIFY(keptAt.isValid());

    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch echo-message server-time\r\n"
                          ":server CAP omairc ACK :batch echo-message server-time\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          "@time=2024-03-09T16:00:00.620Z :lena!u@h PRIVMSG omairc :older\r\n"
                          "@time=2024-03-09T17:00:00.000Z :omairc!u@h PRIVMSG #ghost "
                          ":echo channel\r\n"
                          "@time=2024-03-09T18:00:00.000Z :omairc!u@h PRIVMSG dana "
                          ":echo query\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QVERIFY(rowForTarget(conversations, QStringLiteral("lena")) >= 0);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("#ghost")), -1);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("dana")), -1);
    QCOMPARE(ircPlaybackPlayStamp(
                 IrcPlaybackTimeStore().newest(QStringLiteral("libera"))),
             ircPlaybackPlayStamp(peerAt));

    transport->injectBytes(QByteArrayLiteral(
        ":omairc!u@h JOIN :#omarchy\r\n"
        "@time=2024-03-09T16:00:01.500Z :omairc!u@h PRIVMSG #omarchy :kept echo\r\n"));
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QVERIFY(selectedBodiesContain(messages, QStringLiteral("kept echo")));
    QCOMPARE(ircPlaybackPlayStamp(
                 IrcPlaybackTimeStore().newest(QStringLiteral("libera"))),
             ircPlaybackPlayStamp(keptAt));
    QCOMPARE(ircPlaybackPlayStamp(peerAt), QStringLiteral("1710000000.620"));
    QCOMPARE(ircPlaybackPlayStamp(keptAt), QStringLiteral("1710000001.500"));
}

void ControllerTest::emptyChatHistoryLeavesAnchorForPlayback()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch chathistory znc.in/playback\r\n"
                          ":server CAP omairc ACK :batch chathistory znc.in/playback\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#empty\r\n"
                          ":irc.host BATCH +empty chathistory #empty\r\n"
                          ":irc.host BATCH -empty\r\n"
                          ":znc.in BATCH +pe znc.in/playback #empty\r\n"
                          "@batch=pe;time=2024-03-09T16:00:04.000Z "
                          ":lena!u@h PRIVMSG #empty :from empty\r\n"
                          ":znc.in BATCH -pe\r\n"
                          ":omairc!u@h JOIN :#deduped\r\n"
                          "@time=2024-03-09T16:00:00.620Z :alice!u@h PRIVMSG #deduped "
                          ":already\r\n"
                          ":irc.host BATCH +dup chathistory #deduped\r\n"
                          "@batch=dup;time=2024-03-09T16:00:00.620Z "
                          ":Alice!u@h PRIVMSG #deduped :already\r\n"
                          ":irc.host BATCH -dup\r\n"
                          ":znc.in BATCH +pd znc.in/playback #deduped\r\n"
                          "@batch=pd;time=2024-03-09T16:00:05.000Z "
                          ":lena!u@h PRIVMSG #deduped :from deduped\r\n"
                          ":znc.in BATCH -pd\r\n"));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#empty"));
    auto *empty = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(empty);
    QCOMPARE(transcriptBodies(empty),
             QStringList({QStringLiteral("from empty"),
                          QStringLiteral("omairc joined")}));
    const int emptyReplay = bodyRow(empty, QStringLiteral("from empty"));
    const int emptyJoin = bodyRow(empty, QStringLiteral("omairc joined"));
    QVERIFY(emptyReplay >= 0);
    QVERIFY(emptyJoin > emptyReplay);
    QCOMPARE(roleAt(empty, emptyReplay, MessageListModel::OriginRole),
             QStringLiteral("replay"));
    QCOMPARE(roleAt(empty, emptyJoin, MessageListModel::OriginRole),
             QStringLiteral("live"));

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#deduped"));
    auto *deduped = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(deduped);
    QCOMPARE(transcriptBodies(deduped),
             QStringList({QStringLiteral("from deduped"),
                          QStringLiteral("omairc joined"),
                          QStringLiteral("already")}));
    QCOMPARE(selectedBodies(deduped).count(QStringLiteral("already")), 1);
    const int dedupedReplay = bodyRow(deduped, QStringLiteral("from deduped"));
    const int dedupedJoin = bodyRow(deduped, QStringLiteral("omairc joined"));
    const int already = bodyRow(deduped, QStringLiteral("already"));
    QVERIFY(dedupedReplay >= 0);
    QVERIFY(dedupedJoin > dedupedReplay);
    QVERIFY(already > dedupedJoin);
    QCOMPARE(roleAt(deduped, dedupedReplay, MessageListModel::OriginRole),
             QStringLiteral("replay"));
    QCOMPARE(roleAt(deduped, already, MessageListModel::OriginRole),
             QStringLiteral("live"));
}

void ControllerTest::ephemeralControllerSkipsPlaybackSettings()
{
    {
        IrcController controller;
        controller.setEphemeral(true);
        auto *transport = new FakeIrcTransport;
        QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
        QVERIFY(controller.start(QStringLiteral("libera")));
        transport->completeConnect();
        transport->injectBytes(
            QByteArrayLiteral(":server CAP omairc LS :batch znc.in/playback\r\n"
                              ":server CAP omairc ACK :batch znc.in/playback\r\n"
                              ":server 001 omairc :Welcome\r\n"));
        QVERIFY(!framesContain(transport->writtenFrames(),
                               QByteArrayLiteral("*playback PLAY")));
        transport->injectBytes(
            QByteArrayLiteral(":server 376 omairc :End of MOTD\r\n"));
        QVERIFY(framesContain(
            transport->writtenFrames(),
            QByteArrayLiteral("ZNC *playback PLAY * 0\r\n")));
        transport->injectBytes(
            QByteArrayLiteral("@time=2024-03-09T16:00:00.620Z :lena!u@h PRIVMSG #omarchy "
                              ":room\r\n"));
    }

    QSettings settings;
    QFile file(settings.fileName());
    if (file.exists()) {
        QVERIFY(file.open(QIODevice::ReadOnly | QIODevice::Text));
        QVERIFY(!QString::fromUtf8(file.readAll()).contains(
            QLatin1String("playbackTimes")));
    }
    QVERIFY(!IrcPlaybackTimeStore().newest(QStringLiteral("libera")));

    IrcController again;
    auto *transport = new FakeIrcTransport;
    QVERIFY(again.addSession(config(QStringLiteral("libera")), transport));
    QVERIFY(again.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :batch znc.in/playback\r\n"
                          ":server CAP omairc ACK :batch znc.in/playback\r\n"
                          ":server 001 omairc :Welcome\r\n"));
    QCOMPARE(frameCount(transport->writtenFrames(),
                        QByteArrayLiteral("ZNC *playback PLAY * 0\r\n")),
             0);
    transport->injectBytes(
        QByteArrayLiteral(":server 376 omairc :End of MOTD\r\n"));
    QCOMPARE(frameCount(transport->writtenFrames(),
                        QByteArrayLiteral("ZNC *playback PLAY * 0\r\n")),
             1);
    QVERIFY(!framesContain(
        transport->writtenFrames(),
        QByteArrayLiteral("ZNC *playback PLAY * 1710000000.620\r\n")));
}

void ControllerTest::playbackTimeStoreRoundTripsNewestAndRekey()
{
    const IrcCaseMapping mapping;
    const QDateTime room = QDateTime::fromString(
        QStringLiteral("2024-03-09T16:00:00.620Z"), Qt::ISODateWithMs);
    const QDateTime later = QDateTime::fromString(
        QStringLiteral("2024-03-09T16:00:01.500Z"), Qt::ISODateWithMs);
    const QDateTime older = QDateTime::fromString(
        QStringLiteral("2024-03-09T15:00:00.000Z"), Qt::ISODateWithMs);
    QVERIFY(room.isValid());
    QVERIFY(later.isValid());
    QVERIFY(older.isValid());
    QCOMPARE(ircPlaybackPlayStamp(room), QStringLiteral("1710000000.620"));
    QCOMPARE(ircPlaybackPlayStamp(std::nullopt), QStringLiteral("0"));

    {
        IrcPlaybackTimeStore store;
        QVERIFY(store.note(QStringLiteral("libera"), QStringLiteral("#omarchy"),
                           room, mapping));
        QVERIFY(!store.note(QStringLiteral("libera"), QStringLiteral("#Omarchy"),
                            room, mapping));
        QVERIFY(!store.note(QStringLiteral("libera"), QStringLiteral("#omarchy"),
                            older, mapping));
        QVERIFY(store.note(QStringLiteral("libera"), QStringLiteral("Lena"),
                           later, mapping));
        QVERIFY(!store.note(QStringLiteral("libera"), QStringLiteral("lena"),
                            later, mapping));
        QVERIFY(store.note(QStringLiteral("keep"), QStringLiteral("#lab"),
                           room, mapping));
        QCOMPARE(ircPlaybackPlayStamp(store.newest(QStringLiteral("libera"))),
                 QStringLiteral("1710000001.500"));
        QVERIFY(store.rekey(QStringLiteral("libera"), QStringLiteral("lena"),
                            QStringLiteral("Helena"), mapping));
        QCOMPARE(ircPlaybackPlayStamp(store.newest(QStringLiteral("libera"))),
                 QStringLiteral("1710000001.500"));
        store.forget(QStringLiteral("libera"));
        QVERIFY(!store.newest(QStringLiteral("libera")));
        QCOMPARE(ircPlaybackPlayStamp(store.newest(QStringLiteral("keep"))),
                 QStringLiteral("1710000000.620"));
    }

    {
        IrcPlaybackTimeStore reloaded;
        QVERIFY(!reloaded.newest(QStringLiteral("libera")));
        QCOMPARE(ircPlaybackPlayStamp(reloaded.newest(QStringLiteral("keep"))),
                 QStringLiteral("1710000000.620"));
        QSettings settings;
        settings.beginGroup(QStringLiteral("playbackTimes"));
        settings.beginGroup(QStringLiteral("keep"));
        const QString stored = settings.value(QStringLiteral("times")).toString();
        QVERIFY(stored.contains(QLatin1String("1710000000620")));
        QVERIFY(stored.contains(QLatin1String("#lab")));
        QVERIFY(!stored.contains(QLatin1String("1710000000.620")));
    }

    {
        IrcPlaybackTimeStore ephemeral;
        ephemeral.setEphemeral(true);
        QVERIFY(ephemeral.note(QStringLiteral("libera"), QStringLiteral("#omarchy"),
                               room, mapping));
        QCOMPARE(ircPlaybackPlayStamp(ephemeral.newest(QStringLiteral("libera"))),
                 QStringLiteral("1710000000.620"));
    }
    QVERIFY(!IrcPlaybackTimeStore().newest(QStringLiteral("libera")));
    QCOMPARE(ircPlaybackPlayStamp(
                 IrcPlaybackTimeStore().newest(QStringLiteral("keep"))),
             QStringLiteral("1710000000.620"));
}

void ControllerTest::forgetNetworkDropsPlaybackTimes()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          "@time=2024-03-09T16:00:00.620Z :lena!u@h PRIVMSG #omarchy "
                          ":room\r\n"));
    QCOMPARE(ircPlaybackPlayStamp(
                 IrcPlaybackTimeStore().newest(QStringLiteral("libera"))),
             QStringLiteral("1710000000.620"));
    controller.forgetNetworkState(QStringLiteral("libera"));
    QVERIFY(!IrcPlaybackTimeStore().newest(QStringLiteral("libera")));
}

void ControllerTest::zncPlaybackBeforeFirstPlayUsesEmptySnapshot()
{
    const QDateTime liveAt = QDateTime::fromString(
        QStringLiteral("2024-03-09T16:00:04.000Z"), Qt::ISODateWithMs);
    const QDateTime laterAt = QDateTime::fromString(
        QStringLiteral("2024-03-09T16:00:05.000Z"), Qt::ISODateWithMs);
    QVERIFY(liveAt.isValid());
    QVERIFY(laterAt.isValid());
    const QString liveStamp = ircPlaybackPlayStamp(liveAt);
    const QString laterStamp = ircPlaybackPlayStamp(laterAt);
    QVERIFY(liveStamp != QStringLiteral("0"));
    QVERIFY(laterStamp != liveStamp);
    const IrcCaseMapping mapping;
    const QByteArray playZero =
        QByteArrayLiteral("ZNC *playback PLAY * 0\r\n");
    const QByteArray playLive =
        QByteArrayLiteral("ZNC *playback PLAY #omarchy ")
        + liveStamp.toUtf8() + QByteArrayLiteral("\r\n");
    const QByteArray playLater =
        QByteArrayLiteral("ZNC *playback PLAY #omarchy ")
        + laterStamp.toUtf8() + QByteArrayLiteral("\r\n");
    const QByteArray playChannelZero =
        QByteArrayLiteral("ZNC *playback PLAY #omarchy 0\r\n");
    const auto roomStamp = [&]() {
        return IrcPlaybackTimeStore().noted(QStringLiteral("libera"),
                                            QStringLiteral("#omarchy"),
                                            mapping);
    };

    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(QByteArrayLiteral(
        ":server CAP omairc LS :batch znc.in/playback\r\n"
        ":server CAP omairc ACK :batch znc.in/playback\r\n"
        ":server 001 omairc :Welcome\r\n"
        ":omairc!u@h JOIN :#omarchy\r\n"
        "@time=2024-03-09T16:00:04.000Z :lena!u@h PRIVMSG #omarchy :live\r\n"));
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("*playback PLAY")));
    QVERIFY(!roomStamp());
    QVERIFY(!IrcPlaybackTimeStore().newest(QStringLiteral("libera")));

    transport->injectBytes(QByteArrayLiteral(":server 376 omairc :End of MOTD\r\n"));
    QCOMPARE(frameCount(transport->writtenFrames(), playZero), 1);
    QCOMPARE(frameCount(transport->writtenFrames(), playLive), 0);
    QVERIFY(!roomStamp());
    const int channelZero =
        frameCount(transport->writtenFrames(), playChannelZero);
    QVERIFY(channelZero >= 1);

    transport->injectBytes(QByteArrayLiteral(
        "@time=2024-03-09T16:00:05.000Z :lena!u@h PRIVMSG #omarchy :later\r\n"));
    QCOMPARE(ircPlaybackPlayStamp(roomStamp()), laterStamp);

    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    QCOMPARE(frameCount(transport->writtenFrames(), playChannelZero), channelZero + 1);
    QCOMPARE(frameCount(transport->writtenFrames(), playLater), 0);
    QCOMPARE(frameCount(transport->writtenFrames(), playLive), 0);
    QCOMPARE(frameCount(transport->writtenFrames(), playZero), 1);
}

void ControllerTest::zncPlaybackBeforeFirstPlayKeepsSavedStamp()
{
    const QDateTime savedAt = QDateTime::fromString(
        QStringLiteral("2024-03-09T16:00:00.620Z"), Qt::ISODateWithMs);
    const QDateTime liveAt = QDateTime::fromString(
        QStringLiteral("2024-03-09T16:00:04.000Z"), Qt::ISODateWithMs);
    const QDateTime laterAt = QDateTime::fromString(
        QStringLiteral("2024-03-09T16:00:05.000Z"), Qt::ISODateWithMs);
    QVERIFY(savedAt.isValid());
    QVERIFY(liveAt.isValid());
    QVERIFY(laterAt.isValid());
    QVERIFY(liveAt > savedAt);
    QVERIFY(laterAt > liveAt);
    const QString savedStamp = ircPlaybackPlayStamp(savedAt);
    const QString liveStamp = ircPlaybackPlayStamp(liveAt);
    const QString laterStamp = ircPlaybackPlayStamp(laterAt);
    QCOMPARE(savedStamp, QStringLiteral("1710000000.620"));
    QVERIFY(liveStamp != savedStamp);
    QVERIFY(laterStamp != liveStamp);
    const IrcCaseMapping mapping;
    QVERIFY(IrcPlaybackTimeStore().note(QStringLiteral("libera"),
                                        QStringLiteral("#omarchy"),
                                        savedAt,
                                        mapping));
    const QByteArray playSaved =
        QByteArrayLiteral("ZNC *playback PLAY #omarchy ")
        + savedStamp.toUtf8() + QByteArrayLiteral("\r\n");
    const QByteArray playLive =
        QByteArrayLiteral("ZNC *playback PLAY #omarchy ")
        + liveStamp.toUtf8() + QByteArrayLiteral("\r\n");
    const QByteArray playLater =
        QByteArrayLiteral("ZNC *playback PLAY #omarchy ")
        + laterStamp.toUtf8() + QByteArrayLiteral("\r\n");
    const QByteArray playZero =
        QByteArrayLiteral("ZNC *playback PLAY * 0\r\n");
    const auto roomStamp = [&]() {
        return ircPlaybackPlayStamp(
            IrcPlaybackTimeStore().noted(QStringLiteral("libera"),
                                         QStringLiteral("#omarchy"),
                                         mapping));
    };

    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(QByteArrayLiteral(
        ":server CAP omairc LS :batch znc.in/playback\r\n"
        ":server CAP omairc ACK :batch znc.in/playback\r\n"
        ":server 001 omairc :Welcome\r\n"
        ":omairc!u@h JOIN :#omarchy\r\n"
        "@time=2024-03-09T16:00:04.000Z :lena!u@h PRIVMSG #omarchy :live\r\n"));
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("*playback PLAY")));
    QCOMPARE(roomStamp(), savedStamp);

    transport->injectBytes(QByteArrayLiteral(":server 376 omairc :End of MOTD\r\n"));
    const int savedCount = frameCount(transport->writtenFrames(), playSaved);
    QVERIFY(savedCount >= 1);
    QCOMPARE(frameCount(transport->writtenFrames(), playLive), 0);
    QCOMPARE(frameCount(transport->writtenFrames(), playZero), 1);
    QCOMPARE(roomStamp(), savedStamp);

    transport->injectBytes(QByteArrayLiteral(
        "@time=2024-03-09T16:00:05.000Z :lena!u@h PRIVMSG #omarchy :later\r\n"));
    QCOMPARE(roomStamp(), laterStamp);

    transport->injectBytes(QByteArrayLiteral(":omairc!u@h JOIN :#omarchy\r\n"));
    QCOMPARE(frameCount(transport->writtenFrames(), playSaved), savedCount + 1);
    QCOMPARE(frameCount(transport->writtenFrames(), playLater), 0);
    QCOMPARE(frameCount(transport->writtenFrames(), playLive), 0);
}

void ControllerTest::zncPlaybackDiscoversUnknownOfflineDirect()
{
    const QDateTime roomAt = QDateTime::fromString(
        QStringLiteral("2024-03-09T16:00:00.620Z"), Qt::ISODateWithMs);
    const QDateTime aliceAt = QDateTime::fromString(
        QStringLiteral("2024-03-09T16:00:01.500Z"), Qt::ISODateWithMs);
    const QDateTime liveAt = QDateTime::fromString(
        QStringLiteral("2024-03-09T16:00:02.000Z"), Qt::ISODateWithMs);
    QVERIFY(roomAt.isValid());
    QVERIFY(aliceAt.isValid());
    QVERIFY(liveAt.isValid());
    const QString roomStamp = ircPlaybackPlayStamp(roomAt);
    const QString aliceStamp = ircPlaybackPlayStamp(aliceAt);
    const QString liveStamp = ircPlaybackPlayStamp(liveAt);
    const IrcCaseMapping mapping;
    const QByteArray caps =
        QByteArrayLiteral(":server CAP omairc LS :batch znc.in/playback\r\n"
                          ":server CAP omairc ACK :batch znc.in/playback\r\n");
    const QByteArray playRoom =
        QByteArrayLiteral("ZNC *playback PLAY #omarchy ")
        + roomStamp.toUtf8() + QByteArrayLiteral("\r\n");
    const QByteArray playZero =
        QByteArrayLiteral("ZNC *playback PLAY * 0\r\n");
    const QByteArray playAlice =
        QByteArrayLiteral("ZNC *playback PLAY alice 0\r\n");

    {
        IrcController seeder;
        auto *transport = new FakeIrcTransport;
        QVERIFY(seeder.addSession(config(QStringLiteral("libera")), transport));
        QVERIFY(seeder.start(QStringLiteral("libera")));
        transport->completeConnect();
        transport->injectBytes(QByteArrayLiteral(
            ":server 001 omairc :Welcome\r\n"
            "@time=2024-03-09T16:00:00.620Z :lena!u@h PRIVMSG #omarchy :room\r\n"));
    }
    QCOMPARE(ircPlaybackPlayStamp(
                 IrcPlaybackTimeStore().noted(QStringLiteral("libera"),
                                              QStringLiteral("#omarchy"),
                                              mapping)),
             roomStamp);
    QVERIFY(!IrcPlaybackTimeStore().noted(QStringLiteral("libera"),
                                          QStringLiteral("alice"),
                                          mapping));

    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(caps);
    transport->injectBytes(QByteArrayLiteral(":server 001 omairc :Welcome\r\n"));
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("*playback PLAY")));
    transport->injectBytes(QByteArrayLiteral(":server 376 omairc :End of MOTD\r\n"));
    QCOMPARE(frameCount(transport->writtenFrames(), playRoom), 1);
    QCOMPARE(frameCount(transport->writtenFrames(), playZero), 1);
    QCOMPARE(frameCount(transport->writtenFrames(), playAlice), 0);
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("PLAY lena")));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    QCOMPARE(rowForTarget(conversations, QStringLiteral("alice")), -1);

    transport->injectBytes(QByteArrayLiteral(
        ":znc.in BATCH +a znc.in/playback alice\r\n"
        "@batch=a;time=2024-03-09T16:00:01.500Z "
        ":alice!u@h PRIVMSG omairc :offline\r\n"
        ":znc.in BATCH -a\r\n"));
    const int aliceRow = rowForTarget(conversations, QStringLiteral("alice"));
    QVERIFY(aliceRow >= 0);
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("alice"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QCOMPARE(selectedBodies(messages), QStringList({QStringLiteral("offline")}));
    QCOMPARE(roleAt(messages, 0, MessageListModel::OriginRole),
             QStringLiteral("replay"));
    QCOMPARE(ircPlaybackPlayStamp(
                 IrcPlaybackTimeStore().noted(QStringLiteral("libera"),
                                              QStringLiteral("#omarchy"),
                                              mapping)),
             roomStamp);
    QCOMPARE(ircPlaybackPlayStamp(
                 IrcPlaybackTimeStore().noted(QStringLiteral("libera"),
                                              QStringLiteral("alice"),
                                              mapping)),
             aliceStamp);

    transport->injectBytes(QByteArrayLiteral(
        "@time=2024-03-09T16:00:02.000Z :alice!u@h PRIVMSG omairc :live\r\n"));
    QCOMPARE(ircPlaybackPlayStamp(
                 IrcPlaybackTimeStore().noted(QStringLiteral("libera"),
                                              QStringLiteral("alice"),
                                              mapping)),
             liveStamp);
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("PLAY ghost")));
}

void ControllerTest::zncPlaybackWildcardSkipsStampedClearedDirect()
{
    const QDateTime lenaAt = QDateTime::fromString(
        QStringLiteral("2024-03-09T16:00:01.500Z"), Qt::ISODateWithMs);
    const QDateTime aliceAt = QDateTime::fromString(
        QStringLiteral("2024-03-09T16:00:02.000Z"), Qt::ISODateWithMs);
    QVERIFY(lenaAt.isValid());
    QVERIFY(aliceAt.isValid());
    const QString lenaStamp = ircPlaybackPlayStamp(lenaAt);
    const IrcCaseMapping mapping;
    const QByteArray caps =
        QByteArrayLiteral(":server CAP omairc LS :batch znc.in/playback\r\n"
                          ":server CAP omairc ACK :batch znc.in/playback\r\n");
    const QByteArray playLena =
        QByteArrayLiteral("ZNC *playback PLAY lena ")
        + lenaStamp.toUtf8() + QByteArrayLiteral("\r\n");
    const QByteArray playZero =
        QByteArrayLiteral("ZNC *playback PLAY * 0\r\n");

    {
        IrcController seeder;
        auto *transport = new FakeIrcTransport;
        QVERIFY(seeder.addSession(config(QStringLiteral("libera")), transport));
        QVERIFY(seeder.start(QStringLiteral("libera")));
        transport->completeConnect();
        transport->injectBytes(QByteArrayLiteral(
            ":server 001 omairc :Welcome\r\n"
            "@time=2024-03-09T16:00:01.500Z :lena!u@h PRIVMSG omairc :read me\r\n"));
    }
    QCOMPARE(ircPlaybackPlayStamp(
                 IrcPlaybackTimeStore().noted(QStringLiteral("libera"),
                                              QStringLiteral("lena"),
                                              mapping)),
             lenaStamp);

    IrcSessionConfig sessionConfig = config(QStringLiteral("libera"));
    sessionConfig.reconnectEnabled = true;
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    auto *timer = new FakeReconnectTimer;
    IrcSession *session = controller.addSession(sessionConfig, transport, timer);
    QVERIFY(session);
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(QByteArrayLiteral(
        ":server 001 omairc :Welcome\r\n"
        ":server 376 omairc :End of MOTD\r\n"
        ":omairc!u@h JOIN :#omarchy\r\n"
        "@time=2024-03-09T16:00:01.500Z :lena!u@h PRIVMSG omairc :read me\r\n"));
    QVERIFY(!framesContain(transport->writtenFrames(),
                           QByteArrayLiteral("*playback PLAY")));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    const int lenaRow = rowForTarget(conversations, QStringLiteral("lena"));
    QVERIFY(lenaRow >= 0);
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QCOMPARE(selectedBodies(messages), QStringList({QStringLiteral("read me")}));
    QVERIFY(controller.sendMessage(QStringLiteral("/clear")));
    QCOMPARE(messages->rowCount(), 0);
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    QCOMPARE(rowForTarget(conversations, QStringLiteral("lena")), lenaRow);

    transport->remoteClose();
    QCOMPARE(session->state(), IrcSession::State::Reconnecting);
    QVERIFY(timer->active);
    timer->fire();
    transport->completeConnect();
    transport->injectBytes(caps);
    transport->injectBytes(QByteArrayLiteral(":server 001 omairc :Welcome\r\n"));
    transport->injectBytes(QByteArrayLiteral(":server 376 omairc :End of MOTD\r\n"));
    QCOMPARE(frameCount(transport->writtenFrames(), playLena), 1);
    QCOMPARE(frameCount(transport->writtenFrames(), playZero), 1);

    transport->injectBytes(QByteArrayLiteral(
        ":znc.in BATCH +old znc.in/playback lena\r\n"
        "@batch=old;time=2024-03-09T16:00:01.500Z "
        ":lena!u@h PRIVMSG omairc :read me\r\n"
        ":znc.in BATCH -old\r\n"
        ":znc.in BATCH +a znc.in/playback alice\r\n"
        "@batch=a;time=2024-03-09T16:00:02.000Z "
        ":alice!u@h PRIVMSG omairc :offline\r\n"
        ":znc.in BATCH -a\r\n"));

    const int lenaAfter = rowForTarget(conversations, QStringLiteral("lena"));
    if (lenaAfter >= 0) {
        QCOMPARE(roleAt(conversations, lenaAfter, ConversationListModel::UnreadRole).toInt(),
                 0);
        controller.selectConversation(QStringLiteral("libera"), QStringLiteral("lena"));
        messages = qobject_cast<QAbstractItemModel *>(controller.messages());
        QVERIFY(messages);
        QCOMPARE(messages->rowCount(), 0);
    }

    const int aliceRow = rowForTarget(conversations, QStringLiteral("alice"));
    QVERIFY(aliceRow >= 0);
    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("alice"));
    messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    QCOMPARE(selectedBodies(messages), QStringList({QStringLiteral("offline")}));
    QCOMPARE(roleAt(conversations, aliceRow, ConversationListModel::UnreadRole).toInt(),
             1);
}

void ControllerTest::channelPlaybackPreviousNickDoesNotBumpUnread()
{
    IrcController controller;
    auto *transport = new FakeIrcTransport;
    QVERIFY(controller.addSession(config(QStringLiteral("libera")), transport));
    QVERIFY(controller.start(QStringLiteral("libera")));
    transport->completeConnect();
    transport->injectBytes(QByteArrayLiteral(
        ":server CAP omairc LS :batch\r\n"
        ":server CAP omairc ACK :batch\r\n"
        ":server 001 oldnick :Welcome\r\n"
        ":oldnick!u@h NICK :omairc\r\n"
        ":omairc!u@h JOIN :#lab\r\n"
        ":omairc!u@h JOIN :#omarchy\r\n"
        ":znc.in BATCH +c znc.in/playback #omarchy\r\n"
        "@batch=c :oldnick!u@h PRIVMSG #omarchy :omairc: mine\r\n"
        "@batch=c :lena!u@h PRIVMSG #omarchy :omairc: ping\r\n"
        ":znc.in BATCH -c\r\n"));

    auto *conversations =
        qobject_cast<QAbstractItemModel *>(controller.conversations());
    QVERIFY(conversations);
    const int room = rowForTarget(conversations, QStringLiteral("#omarchy"));
    QVERIFY(room >= 0);
    QCOMPARE(roleAt(conversations, room, ConversationListModel::UnreadRole).toInt(),
             1);
    QVERIFY(roleAt(conversations, room, ConversationListModel::MentionRole).toBool());
    QCOMPARE(controller.unreadCountFor(QStringLiteral("libera")), 1);

    transport->injectBytes(QByteArrayLiteral(
        ":oldnick!u@h PRIVMSG #omarchy :omairc: live\r\n"));
    QCOMPARE(roleAt(conversations, room, ConversationListModel::UnreadRole).toInt(),
             2);
    QCOMPARE(controller.unreadCountFor(QStringLiteral("libera")), 2);

    controller.selectConversation(QStringLiteral("libera"), QStringLiteral("#omarchy"));
    auto *messages = qobject_cast<QAbstractItemModel *>(controller.messages());
    QVERIFY(messages);
    const int mine = bodyRow(messages, QStringLiteral("omairc: mine"));
    const int ping = bodyRow(messages, QStringLiteral("omairc: ping"));
    const int live = bodyRow(messages, QStringLiteral("omairc: live"));
    QVERIFY(mine >= 0);
    QVERIFY(ping > mine);
    QVERIFY(live >= 0);
    QCOMPARE(roleAt(messages, mine, MessageListModel::AuthorRole),
             QStringLiteral("oldnick"));
    QCOMPARE(roleAt(messages, mine, MessageListModel::OriginRole),
             QStringLiteral("replay"));
    QCOMPARE(roleAt(messages, ping, MessageListModel::AuthorRole),
             QStringLiteral("lena"));
    QCOMPARE(roleAt(messages, ping, MessageListModel::OriginRole),
             QStringLiteral("replay"));
    QCOMPARE(roleAt(messages, live, MessageListModel::AuthorRole),
             QStringLiteral("oldnick"));
    QCOMPARE(roleAt(messages, live, MessageListModel::OriginRole),
             QStringLiteral("live"));
}

int runControllerTests(int argc, char **argv)
{
    ControllerTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_controller.moc"
