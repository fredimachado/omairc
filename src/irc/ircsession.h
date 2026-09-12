#pragma once

#include <QElapsedTimer>
#include <QHash>
#include <QMetaType>
#include <QObject>
#include <QSet>
#include <QString>
#include <QStringList>
#include <QTimer>

#include <functional>
#include <optional>

#include "irccapability.h"
#include "irccapabilitynegotiation.h"
#include "irccasemapping.h"
#include "ircframer.h"
#include "irchistorybatch.h"
#include "ircmessage.h"
#include "ircstatusentry.h"
#include "ircsts.h"
#include "irctransport.h"
#include "irctyping.h"
#include "irctypingpublisher.h"

#include <vector>

class IrcChannelModeRequest;
class IrcJoinTarget;

class IrcReconnectTimer : public QObject
{
    Q_OBJECT

public:
    explicit IrcReconnectTimer(QObject *parent = nullptr);

    virtual void start(int delayMilliseconds);
    virtual void cancel();

signals:
    void fired();

private:
    QTimer m_timer;
};

class IrcReachabilitySource : public QObject
{
    Q_OBJECT

public:
    explicit IrcReachabilitySource(QObject *parent = nullptr);

signals:
    void reachable();
};

struct IrcSessionConfig
{
    QString networkId;
    QString host;
    quint16 port = 6697;
    bool tlsEnabled = true;
    QString nick;
    QString username;
    QString realname;
    QString password;
    QString nickServPassword;
    QString saslAccount;  // empty falls back to nick

    QStringList autojoinChannels;
    bool reconnectEnabled = true;
    int reconnectBaseDelayMilliseconds = 1000;
    int reconnectMaximumDelayMilliseconds = 30000;
    int capabilityTimeoutMilliseconds = 10000;
    int pingTimeoutMilliseconds = 60000;
};

class IrcSession : public QObject
{
    Q_OBJECT

public:
    enum class State {
        Idle,
        Connecting,
        StsUpgrading,
        CapLs,
        CapReq,
        Sasl,
        Registering,
        Registered,
        Closing,
        Reconnecting,
        Failed,
    };
    Q_ENUM(State)

    enum class ErrorKind {
        Protocol,
        Authentication,
        Tls,
        Network,
        Registration,
    };
    Q_ENUM(ErrorKind)

    IrcSession(const IrcSessionConfig &config,
               IrcTransport *transport,
               IrcReconnectTimer *reconnectTimer = nullptr,
               IrcReconnectTimer *capabilityTimer = nullptr,
               QObject *parent = nullptr,
               IrcReconnectTimer *pingTimer = nullptr,
               IrcReachabilitySource *reachability = nullptr);
    ~IrcSession() override;

    QString networkId() const;
    QString host() const;
    quint16 port() const;
    bool tlsEnabled() const;
    QString nick() const;
    QString channelTypes() const;
    State state() const;
    int reconnectAttempt() const;
    IrcCapabilitySet capabilities() const;
    bool historyPending() const;

    using IgnoreFilter = std::function<bool(const IrcMessage&, const QString&)>;
    void setIgnoreFilter(IgnoreFilter filter);

public slots:
    void start();
    void stop();
    bool sendPrivmsg(const QString& target, const QString& body);
    bool sendNotice(const QString& target, const QString& body);
    bool sendChannelMode(const IrcChannelModeRequest& request);
    bool sendAction(const QString& target, const QString& body);
    bool sendTyping(const QString& target, IrcTypingPhase phase);
    bool join(const IrcJoinTarget& target);
    bool part(const QString& channel);
    bool kick(const QString& channel, const QString& nick, const QString& reason = {});
    bool invite(const QString& nick, const QString& channel);
    bool setTopic(const QString& channel, const QString& topic);
    bool setAway(const QString& reason = {});
    bool clearAway();
    bool changeNick(const QString& nick);
    bool quit(const QString& reason = {});
    bool whois(const QString& nick);
    bool sendRaw(const QString& line);

signals:
    void stateChanged(IrcSession::State state);
    void errorOccurred(const QString &networkId,
                       IrcSession::ErrorKind kind,
                       const QString &message);
    void registered(const QString &networkId);
    void reconnectScheduled(const QString &networkId,
                            int delayMilliseconds,
                            int attempt);
    void messageReceived(const QString& networkId, const IrcMessage& message);
    void historyBatchReceived(const QString& networkId, const IrcHistoryBatch& batch);
    void statusEntry(const IrcStatusEntry& entry);
    void capabilitiesChanged(const QString& networkId,
                             IrcCapabilitySet capabilities);
    void autojoinChannelsChanged(const QString &networkId,
                                 const QStringList &channels);

private:
    void setState(State state);
    void beginCapabilityNegotiation();
    void requestCapabilities();
    void endCapabilityNegotiation();
    void publishCapabilities();
    void subscribeToMemberMetadata();
    void probeChannelAway(const QString& channel);
    void handleMetadataSyncLater(const IrcMessage &message);
    void sendRegistration();
    bool tryRegistrationNickFallback();
    void sendLine(const QByteArray &line);
    void handleBytes(const QByteArray &bytes);
    void handleMessage(const IrcMessage &message);
    void handleBatch(const IrcMessage &message);
    bool captureInBatch(const IrcMessage &message);
    void closeBatch(const QString& reference);
    void requestChannelHistory(const QString& channel);
    void forgetChannelHistory(const QString& channel);
    void dropHistoryBatches(const QString& channel);
    void bumpHistoryGeneration(const QString& channel);
    int historyGeneration(const QString& channel) const;
    QString foldChannel(const QString& channel) const;
    void ignoreBatch(const QString& reference);
    void clearHistoryPending(const QString& channel);
    bool nicksEqual(const QString& left, const QString& right) const;
    bool isHistoryBatch(const QString& type, const QString& parent) const;
    bool answersPendingHistory(const QString& channel) const;
    bool hasOpenCurrentHistoryBatch(const QString& channel) const;
    void abandonHistoryRequests();
    void handleChatHistoryFail(const IrcMessage& message);

    enum class ReplayKind {
        ChatHistory,      // an answer to a CHATHISTORY we sent
        BouncerPlayback,  // volunteered by the bouncer on attach
    };

    static std::optional<ReplayKind> replayKindFor(const QString& batchType) noexcept;
    bool replayEnabled(ReplayKind kind) const;

    bool selfPrefixed(const IrcMessage& message) const;
    bool selfIs(const QString& nick) const;
    void recordAutojoin(const QString &channel, bool joined);
    bool allowCtcpReply(const QString &nick);
    void handleCap(const IrcMessage &message);
    void applyCachedSts();
    void handleStsAdvertisement(const std::optional<IrcStsAdvertisement> &advertisement);
    void beginStsUpgrade(quint16 port);
    bool shouldFinishStsUpgrade() const;
    void rescheduleStsExpiry();
    void handleAuthenticate(const IrcMessage &message);
    void handleWelcome(const IrcMessage &message);
    void applyIsupport(const IrcMessage &message);
    bool sendCommand(const QString& command);
    bool sendTrailingBody(const QString& prefix,
                          const QString& body,
                          const QString& suffix = {});
    void fail(ErrorKind kind, const QString &message, bool reconnect);
    void scheduleReconnect();
    void beginReconnectAttempt();
    void resetForConnection();
    void armPingWatchdog();
    void cancelPingWatchdog();
    void onPingWatchdogFired();
    bool pongMatchesWatchdog(const IrcMessage &message) const;
    int reconnectDelay() const;

    enum class PingWatchdog {
        Off,
        Watching,
        Probing,
    };

    enum class RegistrationNick {
        Configured,
        Underscore,
        Digit,
    };

    const IrcSessionConfig m_config;
    quint16 m_port = 0;
    bool m_tlsEnabled = true;
    QStringList m_autojoinChannels;
    QString m_nick;
    IrcTransport *m_transport;
    IrcReconnectTimer *m_reconnectTimer;
    IrcReconnectTimer *m_capabilityTimer;
    IrcReconnectTimer *m_pingTimer;
    IrcReachabilitySource *m_reachability;
    PingWatchdog m_pingWatchdog = PingWatchdog::Off;
    IrcFramer m_framer;
    IrcCapabilityNegotiation m_capabilities;
    IrcStsStore m_sts;
    std::optional<IrcStsAdvertisement> m_pendingSts;
    std::optional<qint64> m_stsDuration;
    IrcCapabilitySet m_publishedCapabilities;
    IrcTypingPublisher m_typing;
    struct OpenBatch
    {
        QString type;
        QString parent;
        QString replayRoot;
        std::optional<ReplayKind> kind;
        IrcHistoryBatch collected;
        int generation = 0;
    };
    QHash<QString, OpenBatch> m_openBatches;
    QSet<QString> m_ignoredBatches;
    QHash<QString, int> m_historyGeneration;
    QSet<QString> m_historyAsked;
    QHash<QString, int> m_historyPending;
    IrcCaseMapping m_caseMapping{IrcCaseMapping::Kind::Rfc1459};
    static constexpr int kHistoryLimit = 100;
    int m_historyLimit = kHistoryLimit;
    static constexpr int kHistoryBufferCeiling = 256;
    static constexpr int kMaxOpenBatches = 16;
    static constexpr int kMaxIgnoredBatches = 32;
    QHash<QString, QElapsedTimer> m_ctcpReplyClock;
    IgnoreFilter m_ignoreFilter;
    State m_state = State::Idle;
    bool m_expectedDisconnect = false;
    bool m_reconnectAfterDisconnect = false;
    bool m_registrationSent = false;
    RegistrationNick m_registrationNick = RegistrationNick::Configured;
    bool m_saslRequested = false;
    bool m_saslPending = false;
    bool m_saslSucceeded = false;
    bool m_capabilityNegotiationEnded = false;
    bool m_capabilityListSeen = false;
    QString m_channelTypes;
    int m_reconnectAttempt = 0;
    quint32 m_reportedRetryErrors = 0;
};
