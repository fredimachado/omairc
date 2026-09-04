#include "ircstatusentry.h"

#include <QStringList>

namespace
{
QString fromUtf8(const std::string& value)
{
    return QString::fromUtf8(value.data(), qsizetype(value.size()));
}

QString commandOf(const IrcMessage& message)
{
    return fromUtf8(message.command).toUpper();
}

bool isSecretVerb(const QString& verb)
{
    return verb == QStringLiteral("PASS") || verb == QStringLiteral("AUTHENTICATE");
}

QString redactedSecret(const QString& verb)
{
    return verb + QStringLiteral(" ***");
}

IrcLogSeverity severityFor(const QString& command)
{
    if (command == QStringLiteral("PING")
        || command == QStringLiteral("PONG")
        || command == QStringLiteral("CAP")
        || command == QStringLiteral("AUTHENTICATE")) {
        return IrcLogSeverity::Trace;
    }
    if (command == QStringLiteral("ERROR"))
        return IrcLogSeverity::Alert;
    if (command.size() == 3 && command[0].isDigit()
        && (command[0] == QLatin1Char('4') || command[0] == QLatin1Char('5'))) {
        return IrcLogSeverity::Alert;
    }
    return IrcLogSeverity::Info;
}

bool trailingBodyOnly(const QString& command)
{
    if (command == QStringLiteral("NOTICE") || command == QStringLiteral("PRIVMSG")
        || command == QStringLiteral("PING") || command == QStringLiteral("PONG")
        || command == QStringLiteral("ERROR")) {
        return true;
    }
    if (command.size() != 3 || !command[0].isDigit())
        return false;
    const int code = command.toInt();
    return (code >= 1 && code <= 5) || code == 372 || code == 375 || code == 376
        || code == 422;
}

QString incomingText(const IrcMessage& message, const QString& command)
{
    if (isSecretVerb(command))
        return redactedSecret(command);
    if (message.parameters.empty())
        return command;

    if (trailingBodyOnly(command))
        return fromUtf8(message.parameters.back());

    QStringList parts;
    parts.reserve(int(message.parameters.size()));
    for (const std::string& parameter : message.parameters)
        parts.append(fromUtf8(parameter));
    if (command[0].isDigit() && parts.size() >= 2)
        parts.removeFirst();
    return parts.join(QLatin1Char(' '));
}

QString firstToken(QStringView line)
{
    const QStringView trimmed = line.trimmed();
    if (trimmed.isEmpty())
        return {};
    const qsizetype space = trimmed.indexOf(QLatin1Char(' '));
    return (space < 0 ? trimmed : trimmed.left(space)).toString().toUpper();
}
}

IrcStatusEntry::IrcStatusEntry(QString networkId,
                               QDateTime timestamp,
                               IrcLogSource source,
                               IrcLogSeverity severity,
                               QString label,
                               QString text)
    : m_networkId(std::move(networkId))
    , m_timestamp(std::move(timestamp))
    , m_source(source)
    , m_severity(severity)
    , m_label(std::move(label))
    , m_text(std::move(text))
{
}

IrcStatusEntry IrcStatusEntry::incoming(const QString& networkId, const IrcMessage& message)
{
    const QString command = commandOf(message);
    return IrcStatusEntry(networkId,
                          QDateTime::currentDateTimeUtc(),
                          IrcLogSource::Server,
                          severityFor(command),
                          command,
                          incomingText(message, command));
}

IrcStatusEntry IrcStatusEntry::outgoing(const QString& networkId, const QByteArray& line)
{
    QByteArray wire = line;
    if (wire.endsWith("\r\n"))
        wire.chop(2);
    else if (wire.endsWith('\n'))
        wire.chop(1);

    const QString display = QString::fromUtf8(wire);
    const QString verb = firstToken(QStringView(display));
    return IrcStatusEntry(networkId,
                          QDateTime::currentDateTimeUtc(),
                          IrcLogSource::Client,
                          severityFor(verb),
                          verb,
                          isSecretVerb(verb) ? redactedSecret(verb) : display);
}

IrcStatusEntry IrcStatusEntry::lifecycle(const QString& networkId,
                                         IrcLogSeverity severity,
                                         const QString& label,
                                         const QString& text)
{
    return IrcStatusEntry(networkId,
                          QDateTime::currentDateTimeUtc(),
                          IrcLogSource::Local,
                          severity,
                          label,
                          text);
}

IrcStatusEntry IrcStatusEntry::outcome(const QString& networkId, const QString& text)
{
    return IrcStatusEntry(networkId,
                          QDateTime::currentDateTimeUtc(),
                          IrcLogSource::Local,
                          IrcLogSeverity::Info,
                          QStringLiteral("command"),
                          text);
}

QString IrcStatusEntry::networkId() const
{
    return m_networkId;
}

QDateTime IrcStatusEntry::timestamp() const
{
    return m_timestamp;
}

IrcLogSource IrcStatusEntry::source() const
{
    return m_source;
}

IrcLogSeverity IrcStatusEntry::severity() const
{
    return m_severity;
}

QString IrcStatusEntry::label() const
{
    return m_label;
}

QString IrcStatusEntry::text() const
{
    return m_text;
}
