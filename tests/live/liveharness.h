#pragma once

#include "irccasemapping.h"
#include "irccontroller.h"
#include "ircmessage.h"
#include "ircserverfeatures.h"
#include "ircsession.h"
#include "ircstatusentry.h"
#include "qtirctransport.h"

#include <QSslConfiguration>
#include <QString>
#include <QStringList>
#include <QVariant>
#include <QVector>

#include <functional>
#include <optional>

struct LiveDaemonInfo
{
    QString name;
    quint16 plainPort = 0;
    quint16 tlsPort = 0;
    bool sasl = false;
    bool awayNotify = false;
    bool messageTags = false;
    bool metadata = false;
    bool wantedCapsAdvertised = true;
    IrcCaseMapping::Kind mapping = IrcCaseMapping::Kind::Rfc1459;
    int nickLength = 16;
};

QString liveHost();
QSslConfiguration liveSslConfiguration();
QString uniqueNick(int maxLength);
QString uniqueChannel();
QVector<LiveDaemonInfo> liveDaemons();
const LiveDaemonInfo *liveDaemon(const QString& name);
bool waitUntil(const std::function<bool()> &predicate, int timeoutMs = 20000);
bool messageHasCommand(const QVector<IrcMessage> &messages, const QString &command);
int messageCommandCount(const QVector<IrcMessage> &messages, const QString &command);

struct LiveClassicSighting
{
    QString channel;
    QString otherNick;
    QString privmsg;
    QString notice;
    QString action;
    QString topic;
};

bool liveClassicComplete(const QVector<IrcMessage> &incoming,
                         const LiveClassicSighting &want);
QString liveClassicGap(const QVector<IrcMessage> &incoming,
                       const LiveClassicSighting &want);

class LiveClient
{
public:
    LiveClient(const LiveDaemonInfo &daemon,
               const QString &nick,
               bool tls,
               const QString &password = {});
    ~LiveClient();

    LiveClient(const LiveClient &) = delete;
    LiveClient &operator=(const LiveClient &) = delete;

    bool waitRegistered(int timeoutMs = 20000);
    bool waitFailed(int timeoutMs = 20000);
    bool hasServerLabel(const QString &label) const;
    bool waitServerLabel(const QString &label, int timeoutMs = 20000);
    const IrcServerFeatures &features() const;
    void selectChannel(const QString &channel);
    QVariant memberRole(const QString &nick, int role);
    QVariant transcriptRole(int row, int role);

    IrcController controller;
    IrcSession *session = nullptr;
    IrcSessionConfig config;
    QVector<IrcMessage> incoming;
    QVector<IrcStatusEntry> status;
    QString lastError;
    std::optional<IrcSession::ErrorKind> lastErrorKind;
};
