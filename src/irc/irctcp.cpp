#include "irctcp.h"

#include <QChar>
#include <QStringView>

namespace
{
constexpr QChar kDelimiter(1);
}

std::optional<IrcCtcpRequest> parseCtcpRequest(const QString& body)
{
    if (body.size() < 2 || body.front() != kDelimiter || body.back() != kDelimiter)
        return std::nullopt;

    const QStringView payload(body.constData() + 1, body.size() - 2);
    const qsizetype separator = payload.indexOf(QLatin1Char(' '));
    const QString command = (separator < 0 ? payload : payload.left(separator))
                                .toString()
                                .trimmed()
                                .toUpper();
    if (command.isEmpty())
        return std::nullopt;

    return IrcCtcpRequest{
        command,
        separator < 0 ? QString{} : payload.mid(separator + 1).toString(),
    };
}

QString ctcpPayload(const IrcCtcpRequest& request)
{
    const QString body = request.argument.isEmpty()
        ? request.command
        : request.command + QLatin1Char(' ') + request.argument;
    return QChar(1) + body + QChar(1);
}

QString formatCtcpReplyText(const QString& command,
                            const QString& nick,
                            const QString& argument,
                            const QDateTime& now)
{
    if (command == QLatin1String("PING")) {
        bool ok = false;
        const qint64 sentMs = argument.toLongLong(&ok);
        if (ok) {
            const qint64 lag = now.toMSecsSinceEpoch() - sentMs;
            if (lag >= 0 && lag < 24 * 60 * 60 * 1000)
                return QStringLiteral("PING reply from %1: %2 ms").arg(nick).arg(lag);
        }
        if (argument.isEmpty())
            return QStringLiteral("PING reply from %1").arg(nick);
        return QStringLiteral("PING reply from %1: %2").arg(nick, argument);
    }
    if (argument.isEmpty())
        return QStringLiteral("%1 reply from %2").arg(command, nick);
    return QStringLiteral("%1 reply from %2: %3").arg(command, nick, argument);
}
