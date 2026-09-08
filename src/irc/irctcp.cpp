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
