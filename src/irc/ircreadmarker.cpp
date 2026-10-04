#include "ircreadmarker.h"

#include "ircmessage.h"
#include "ircwiretext.h"

namespace
{
QString parameter(const IrcMessage& message, std::size_t index)
{
    if (index >= message.parameters.size())
        return {};
    return ircWireText(message.parameters[index]);
}
}

std::optional<std::optional<QDateTime>> parseIrcReadMarkerParameter(
    const QString& parameter)
{
    if (parameter == QLatin1String("*"))
        return std::optional<QDateTime>{};
    if (!parameter.startsWith(QLatin1String("timestamp=")))
        return std::nullopt;
    const QString raw = parameter.mid(QStringLiteral("timestamp=").size());
    if (raw.isEmpty())
        return std::nullopt;
    QDateTime parsed = QDateTime::fromString(raw, Qt::ISODateWithMs);
    if (!parsed.isValid())
        parsed = QDateTime::fromString(raw, Qt::ISODate);
    if (!parsed.isValid())
        return std::nullopt;
    return parsed.toUTC();
}

QString formatIrcReadMarkerTimestamp(const QDateTime& when)
{
    return QStringLiteral("timestamp=%1")
        .arg(when.toUTC().toString(Qt::ISODateWithMs));
}

std::optional<std::pair<QString, std::optional<QDateTime>>> parseIrcReadMarkerLine(
    const IrcMessage& message,
    const QString& command)
{
    if (QString::compare(QString::fromLatin1(message.command.data()),
                         command, Qt::CaseInsensitive)
        != 0) {
        return std::nullopt;
    }
    if (message.parameters.empty())
        return std::nullopt;
    const QString target = parameter(message, 0);
    if (target.isEmpty())
        return std::nullopt;
    if (message.parameters.size() < 2)
        return std::make_pair(target, std::optional<QDateTime>{});
    const std::optional<std::optional<QDateTime>> marker =
        parseIrcReadMarkerParameter(parameter(message, 1));
    if (!marker)
        return std::nullopt;
    return std::make_pair(target, *marker);
}
