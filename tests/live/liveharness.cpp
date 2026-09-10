#include "liveharness.h"

#include "memberlistmodel.h"
#include "messagelistmodel.h"

#include <QAbstractItemModel>
#include <QCoreApplication>
#include <QDeadlineTimer>
#include <QFile>
#include <QSslCertificate>
#include <QSslConfiguration>
#include <QThread>
#include <QUuid>

#include <atomic>

namespace
{
QString env(const char *key)
{
    return QString::fromLocal8Bit(qgetenv(key));
}

quint16 envPort(const QString &key)
{
    bool ok = false;
    const quint16 port = env(key.toUtf8().constData()).toUShort(&ok);
    return ok ? port : 0;
}

QString certDir()
{
    const QString fromEnv = env("OMAIRC_LIVE_CERT_DIR");
    if (!fromEnv.isEmpty())
        return fromEnv;
    return QStringLiteral(LIVE_CERT_DIR);
}
}

QString liveHost()
{
    const QString host = env("OMAIRC_LIVE_HOST");
    return host.isEmpty() ? QStringLiteral("127.0.0.1") : host;
}

QSslConfiguration liveSslConfiguration()
{
    QFile file(certDir() + QStringLiteral("/live.pem"));
    QSslConfiguration config = QSslConfiguration::defaultConfiguration();
    if (!file.open(QIODevice::ReadOnly))
        return config;
    const QSslCertificate certificate(file.readAll(), QSsl::Pem);
    if (certificate.isNull())
        return config;
    QList<QSslCertificate> authorities = config.caCertificates();
    authorities.append(certificate);
    config.setCaCertificates(authorities);
    return config;
}

QString uniqueNick(int maxLength)
{
    static std::atomic<int> sequence{0};
    const int serial = sequence.fetch_add(1) + static_cast<int>(QCoreApplication::applicationPid() % 4096);
    QString nick = QStringLiteral("o%1").arg(serial, 4, 16, QLatin1Char('0'));
    if (maxLength > 0 && nick.size() > maxLength)
        nick.truncate(maxLength);
    return nick;
}

QString uniqueChannel()
{
    static std::atomic<int> sequence{0};
    return QStringLiteral("#c%1").arg(sequence.fetch_add(1), 6, 16, QLatin1Char('0'));
}

QVector<LiveDaemonInfo> liveDaemons()
{
    QVector<LiveDaemonInfo> daemons;
    const QStringList names = env("OMAIRC_LIVE_DAEMONS")
                                  .split(QLatin1Char(','), Qt::SkipEmptyParts);
    for (QString name : names) {
        name = name.trimmed().toLower();
        LiveDaemonInfo info;
        info.name = name;
        if (name == QLatin1String("ergo")) {
            info.plainPort = envPort(QStringLiteral("OMAIRC_LIVE_ERGO_PLAIN_PORT"));
            info.tlsPort = envPort(QStringLiteral("OMAIRC_LIVE_ERGO_TLS_PORT"));
            info.sasl = true;
            info.awayNotify = true;
            info.messageTags = true;
            info.metadata = true;
            info.mapping = IrcCaseMapping::Kind::Ascii;
            info.nickLength = 32;
        } else if (name == QLatin1String("solanum")) {
            info.plainPort = envPort(QStringLiteral("OMAIRC_LIVE_SOLANUM_PLAIN_PORT"));
            info.tlsPort = envPort(QStringLiteral("OMAIRC_LIVE_SOLANUM_TLS_PORT"));
            info.awayNotify = true;
            info.messageTags = true;
            info.mapping = IrcCaseMapping::Kind::Rfc1459;
            info.nickLength = 16;
        } else if (name == QLatin1String("ngircd")) {
            info.plainPort = envPort(QStringLiteral("OMAIRC_LIVE_NGIRCD_PLAIN_PORT"));
            info.wantedCapsAdvertised = false;
            info.mapping = IrcCaseMapping::Kind::Ascii;
            info.nickLength = 9;
        } else {
            continue;
        }
        if (info.plainPort == 0 && info.tlsPort == 0)
            continue;
        daemons.append(info);
    }
    return daemons;
}

const LiveDaemonInfo *liveDaemon(const QString &name)
{
    static const QVector<LiveDaemonInfo> daemons = liveDaemons();
    for (const LiveDaemonInfo &daemon : daemons) {
        if (daemon.name == name)
            return &daemon;
    }
    return nullptr;
}

bool waitUntil(const std::function<bool()> &predicate, int timeoutMs)
{
    QDeadlineTimer deadline(timeoutMs);
    while (!deadline.hasExpired()) {
        if (predicate())
            return true;
        QCoreApplication::processEvents();
        QThread::msleep(20);
    }
    return predicate();
}

namespace
{
QString fromUtf8(const std::string &value)
{
    return QString::fromUtf8(value.data(), qsizetype(value.size()));
}

bool commandIs(const IrcMessage &message, const QString &command)
{
    return fromUtf8(message.command).compare(command, Qt::CaseInsensitive) == 0;
}

QString lastParam(const IrcMessage &message)
{
    if (message.parameters.empty())
        return {};
    return fromUtf8(message.parameters.back());
}

QString nickOf(const IrcMessage &message)
{
    if (!message.prefix)
        return {};
    return fromUtf8(message.prefix->nick);
}

QString channelOf(const IrcMessage &message)
{
    if (commandIs(message, QStringLiteral("332"))) {
        if (message.parameters.size() < 2)
            return {};
        return fromUtf8(message.parameters[1]);
    }
    if (message.parameters.empty())
        return {};
    return fromUtf8(message.parameters.front());
}
}

bool messageHasCommand(const QVector<IrcMessage> &messages, const QString &command)
{
    for (const IrcMessage &message : messages) {
        if (commandIs(message, command))
            return true;
    }
    return false;
}

int messageCommandCount(const QVector<IrcMessage> &messages, const QString &command)
{
    int count = 0;
    for (const IrcMessage &message : messages) {
        if (commandIs(message, command))
            ++count;
    }
    return count;
}

QString liveClassicGap(const QVector<IrcMessage> &incoming,
                       const LiveClassicSighting &want)
{
    bool sawPrivmsg = false;
    bool sawNotice = false;
    bool sawAction = false;
    bool sawTopic = false;
    bool sawPart = false;
    const QString actionTrailing = QChar(1) + QLatin1String("ACTION ") + want.action
        + QChar(1);
    QStringList seen;

    for (const IrcMessage &message : incoming) {
        const QString command = fromUtf8(message.command);
        if (seen.size() < 24 && !seen.contains(command))
            seen.append(command);
        const bool sameChannel = channelOf(message).compare(want.channel, Qt::CaseInsensitive)
            == 0;
        const bool sameNick = nickOf(message).compare(want.otherNick, Qt::CaseInsensitive)
            == 0;
        if (commandIs(message, QStringLiteral("PRIVMSG")) && sameChannel) {
            const QString trailing = lastParam(message);
            if (trailing == want.privmsg)
                sawPrivmsg = true;
            if (trailing == actionTrailing)
                sawAction = true;
        }
        if (commandIs(message, QStringLiteral("NOTICE")) && sameChannel
            && lastParam(message) == want.notice) {
            sawNotice = true;
        }
        if ((commandIs(message, QStringLiteral("TOPIC"))
             || commandIs(message, QStringLiteral("332")))
            && sameChannel && lastParam(message) == want.topic) {
            sawTopic = true;
        }
        if (commandIs(message, QStringLiteral("PART")) && sameNick && sameChannel)
            sawPart = true;
    }

    QStringList missing;
    if (!sawPrivmsg)
        missing.append(QStringLiteral("privmsg"));
    if (!sawNotice)
        missing.append(QStringLiteral("notice"));
    if (!sawAction)
        missing.append(QStringLiteral("action"));
    if (!sawTopic)
        missing.append(QStringLiteral("topic"));
    if (!sawPart)
        missing.append(QStringLiteral("part"));
    if (missing.isEmpty())
        return {};
    return missing.join(QLatin1Char(' ')) + QLatin1String(" seen=")
        + seen.join(QLatin1Char(','));
}

bool liveClassicComplete(const QVector<IrcMessage> &incoming,
                         const LiveClassicSighting &want)
{
    return liveClassicGap(incoming, want).isEmpty();
}

LiveClient::LiveClient(const LiveDaemonInfo &daemon,
                       const QString &nick,
                       bool tls,
                       const QString &password)
{
    config.networkId = QUuid::createUuid().toString(QUuid::WithoutBraces);
    config.host = liveHost();
    config.port = tls ? daemon.tlsPort : daemon.plainPort;
    config.tlsEnabled = tls;
    config.nick = nick;
    config.username = QStringLiteral("omairc");
    config.realname = QStringLiteral("Omairc live");
    config.password = password;
    config.reconnectEnabled = false;
    config.capabilityTimeoutMilliseconds = 8000;

    QtIrcTransport *transport = tls
        ? new QtIrcTransport(liveSslConfiguration())
        : new QtIrcTransport;
    session = controller.addSession(config, transport);
    if (session)
        transport->setParent(session);
    else
        delete transport;

    if (!session)
        return;

    QObject::connect(session, &IrcSession::messageReceived, session,
                     [this](const QString &, const IrcMessage &message) {
                         incoming.append(message);
                     });
    QObject::connect(session, &IrcSession::statusEntry, session,
                     [this](const IrcStatusEntry &entry) {
                         status.append(entry);
                     });
    QObject::connect(session, &IrcSession::errorOccurred, session,
                     [this](const QString &, IrcSession::ErrorKind kind, const QString &message) {
                         lastErrorKind = kind;
                         lastError = message;
                     });
    controller.start(config.networkId);
}

LiveClient::~LiveClient()
{
    if (!config.networkId.isEmpty())
        controller.discardSession(config.networkId);
}

bool LiveClient::waitRegistered(int timeoutMs)
{
    return waitUntil([this] {
        return session && session->state() == IrcSession::State::Registered;
    }, timeoutMs);
}

bool LiveClient::waitFailed(int timeoutMs)
{
    return waitUntil([this] {
        return session && session->state() == IrcSession::State::Failed;
    }, timeoutMs);
}

bool LiveClient::hasServerLabel(const QString &label) const
{
    for (const IrcStatusEntry &entry : status) {
        if (entry.source() == IrcLogSource::Server && entry.label() == label)
            return true;
    }
    return false;
}

bool LiveClient::waitServerLabel(const QString &label, int timeoutMs)
{
    return waitUntil([this, label] { return hasServerLabel(label); }, timeoutMs);
}

const IrcServerFeatures &LiveClient::features() const
{
    return controller.serverFeatures(config.networkId);
}

void LiveClient::selectChannel(const QString &channel)
{
    controller.selectConversation(config.networkId, channel);
}

QVariant LiveClient::memberRole(const QString &nick, int role)
{
    QAbstractItemModel *model = controller.members();
    for (int row = 0; row < model->rowCount(); ++row) {
        const QModelIndex index = model->index(row, 0);
        if (index.data(MemberListModel::NickRole).toString().compare(
                nick, Qt::CaseInsensitive)
            == 0) {
            return index.data(role);
        }
    }
    return {};
}

QVariant LiveClient::transcriptRole(int row, int role)
{
    QAbstractItemModel *model = controller.messages();
    if (!model || row < 0 || row >= model->rowCount())
        return {};
    return model->index(row, 0).data(role);
}
