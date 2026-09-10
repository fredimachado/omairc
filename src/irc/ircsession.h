#pragma once

#include <QElapsedTimer>
#include <QHash>
#include <QObject>
#include <QSet>
#include <QString>
#include <QStringList>
#include <QTimer>

#include <functional>

#include "irccapability.h"
#include "irccapabilitynegotiation.h"
#include "ircframer.h"
#include "ircstatusentry.h"
#include "irctransport.h"
#include "irctyping.h"
#include "irctypingpublisher.h"

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
    State state() const;
    int reconnectAttempt() const;
    IrcCapabilitySet capabilities() const;

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
    bool setTopic(const QString& channel, const QString& topic);
    bool setAway(const QString& reason = {});
    bool clearAway();
    bool changeNick(const QString& nick);
    bool quit(const QString& reason = {});
    bool whois(const QString& nick);

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
    void statusEntry(const IrcStatusEntry& entry);
    void capabilitiesChanged(const QString& networkId,
                             IrcCapabilitySet capabilities);

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
    bool allowCtcpReply(const QString &nick);
    void handleCap(const IrcMessage &message);
    void handleAuthenticate(const IrcMessage &message);
    void handleWelcome(const IrcMessage &message);
    bool sendCommand(const QString& command);
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
    QString m_nick;
    IrcTransport *m_transport;
    IrcReconnectTimer *m_reconnectTimer;
    IrcReconnectTimer *m_capabilityTimer;
    IrcReconnectTimer *m_pingTimer;
    IrcReachabilitySource *m_reachability;
    PingWatchdog m_pingWatchdog = PingWatchdog::Off;
    IrcFramer m_framer;
    IrcCapabilityNegotiation m_capabilities;
    IrcCapabilitySet m_publishedCapabilities;
    IrcTypingPublisher m_typing;
    QSet<QString> m_openBatches;
    QHash<QString, QElapsedTimer> m_ctcpReplyClock;
    IgnoreFilter m_ignoreFilter;
    State m_state = State::Idle;
    bool m_expectedDisconnect = false;
    bool m_reconnectAfterDisconnect = false;
    bool m_registrationSent = false;
    RegistrationNick m_registrationNick = RegistrationNick::Configured;
    bool m_saslRequested = false;
    bool m_saslPending = false;
    bool m_capabilityNegotiationEnded = false;
    int m_reconnectAttempt = 0;
    quint32 m_reportedRetryErrors = 0;
};
