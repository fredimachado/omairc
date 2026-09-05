#include "livepeer.h"

#include "ircparser.h"
#include "liveharness.h"

#include <QObject>

#include <string_view>

RawIrcPeer::RawIrcPeer(const QString &host,
                       quint16 port,
                       bool tls,
                       const QSslConfiguration &ssl,
                       const QString &nick,
                       QObject *parent)
    : QObject(parent)
    , nick(nick)
    , m_transport(tls ? std::make_unique<QtIrcTransport>(ssl)
                      : std::make_unique<QtIrcTransport>())
{
    QObject::connect(m_transport.get(), &IrcTransport::bytesReceived,
                     this, &RawIrcPeer::handleBytes);
    m_transport->connectToHost(host, port, tls);
}

RawIrcPeer::~RawIrcPeer()
{
    if (m_transport)
        m_transport->shutdown();
}

void RawIrcPeer::handleBytes(const QByteArray &bytes)
{
    const IrcFrameResult framed = m_framer.feed(
        std::string_view(bytes.constData(), static_cast<std::size_t>(bytes.size())));
    for (const std::string &frame : framed.frames) {
        const IrcParseResult parsed = IrcParser::parse(frame);
        if (!parsed || !parsed.value)
            continue;
        const IrcMessage message = *parsed.value;
        incoming.append(message);
        answerPing(message);
    }
}

void RawIrcPeer::answerPing(const IrcMessage &message)
{
    if (message.command != "PING")
        return;
    const QString token = message.parameters.empty()
        ? QString()
        : QString::fromStdString(message.parameters.back());
    writeLine(QStringLiteral("PONG :%1").arg(token));
}

void RawIrcPeer::writeLine(const QString &line)
{
    QString wire = line;
    if (!wire.endsWith(QLatin1String("\r\n")))
        wire += QLatin1String("\r\n");
    m_transport->write(wire.toUtf8());
}

bool RawIrcPeer::waitForCommand(const QString &command, int timeoutMs)
{
    return waitUntil([this, command] {
        return messageHasCommand(incoming, command);
    }, timeoutMs);
}

bool RawIrcPeer::waitForCap(const QString &verb, int timeoutMs)
{
    return waitUntil([this, verb] {
        return capHas(verb);
    }, timeoutMs);
}

bool RawIrcPeer::capHas(const QString &verb) const
{
    for (const IrcMessage &message : incoming) {
        if (message.command != "CAP")
            continue;
        for (const std::string &parameter : message.parameters) {
            if (QString::fromStdString(parameter)
                    .compare(verb, Qt::CaseInsensitive)
                == 0) {
                return true;
            }
        }
    }
    return false;
}

QStringList RawIrcPeer::advertisedCaps() const
{
    QStringList caps;
    for (const IrcMessage &message : incoming) {
        if (message.command != "CAP")
            continue;
        bool listing = false;
        for (const std::string &parameter : message.parameters) {
            const QString token = QString::fromStdString(parameter);
            if (token.compare(QLatin1String("LS"), Qt::CaseInsensitive) == 0) {
                listing = true;
                continue;
            }
            if (!listing)
                continue;
            if (token == QLatin1Char('*'))
                continue;
            const QStringList names = token.split(QLatin1Char(' '), Qt::SkipEmptyParts);
            for (QString name : names) {
                name = name.section(QLatin1Char('='), 0, 0);
                if (!name.isEmpty() && !caps.contains(name, Qt::CaseInsensitive))
                    caps.append(name);
            }
        }
    }
    return caps;
}

bool RawIrcPeer::waitRegistered(int timeoutMs)
{
    writeLine(QStringLiteral("CAP LS 302"));
    waitForCap(QStringLiteral("LS"), 4000);
    const QStringList advertised = advertisedCaps();
    const QStringList wanted{
        QStringLiteral("message-tags"),
        QStringLiteral("away-notify"),
        QStringLiteral("batch"),
        QStringLiteral("draft/metadata-2"),
    };
    QStringList request;
    for (const QString &cap : wanted) {
        if (advertised.contains(cap, Qt::CaseInsensitive))
            request.append(cap);
    }
    if (!request.isEmpty()) {
        writeLine(QStringLiteral("CAP REQ :%1").arg(request.join(QLatin1Char(' '))));
        waitUntil([this] { return capHas(QStringLiteral("ACK")) || capHas(QStringLiteral("NAK")); },
                  4000);
    }
    writeLine(QStringLiteral("CAP END"));
    writeLine(QStringLiteral("NICK %1").arg(nick));
    writeLine(QStringLiteral("USER %1 0 * :live peer").arg(nick));
    return waitForCommand(QStringLiteral("001"), timeoutMs);
}

bool RawIrcPeer::join(const QString &channel, int timeoutMs)
{
    writeLine(QStringLiteral("JOIN %1").arg(channel));
    return waitUntil([this, channel] {
        for (const IrcMessage &message : incoming) {
            if (message.command == "JOIN" && !message.parameters.empty()
                && QString::fromStdString(message.parameters.front())
                        .compare(channel, Qt::CaseInsensitive)
                    == 0) {
                return true;
            }
            if (message.command == "366" && message.parameters.size() >= 2
                && QString::fromStdString(message.parameters[1])
                        .compare(channel, Qt::CaseInsensitive)
                    == 0) {
                return true;
            }
        }
        return false;
    }, timeoutMs);
}
