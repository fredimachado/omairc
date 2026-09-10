#include "ircstatusentry.h"

#include "irctcp.h"
#include "ircwiretext.h"

#include <QStringList>

#include <optional>
#include <string_view>

namespace
{
QString commandOf(const IrcMessage& message)
{
    return ircWireText(message.command).toUpper();
}

bool isSecretVerb(const QString& verb)
{
    return verb == QStringLiteral("PASS") || verb == QStringLiteral("AUTHENTICATE");
}

QString redactedSecret(const QString& verb)
{
    return verb + QStringLiteral(" ***");
}

struct IrcStandardReplyVerb
{
    const char *command = nullptr;
    IrcLogSeverity severity = IrcLogSeverity::Info;
};

constexpr IrcStandardReplyVerb kStandardReplyVerbs[] = {
    {"FAIL", IrcLogSeverity::Alert},
    {"WARN", IrcLogSeverity::Info},
    {"NOTE", IrcLogSeverity::Info},
};

const IrcStandardReplyVerb *standardReplyVerb(const QString& command)
{
    for (const IrcStandardReplyVerb& row : kStandardReplyVerbs) {
        if (command == QLatin1String(row.command))
            return &row;
    }
    return nullptr;
}

QString redactedOutgoingJoin(const QString& display)
{
    const QStringList parts = display.split(QLatin1Char(' '), Qt::SkipEmptyParts);
    if (parts.size() <= 2)
        return display;
    return QStringLiteral("JOIN %1 ***").arg(parts.at(1));
}

IrcLogSeverity severityFor(const QString& command)
{
    if (command == QStringLiteral("PING")
        || command == QStringLiteral("PONG")
        || command == QStringLiteral("CAP")
        || command == QStringLiteral("AUTHENTICATE")) {
        return IrcLogSeverity::Trace;
    }
    if (const auto *reply = standardReplyVerb(command))
        return reply->severity;
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
    if (standardReplyVerb(command))
        return true;
    if (command == QStringLiteral("PRIVMSG")
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
        return ircWireText(message.parameters.back());

    QStringList parts;
    parts.reserve(int(message.parameters.size()));
    for (const std::string& parameter : message.parameters)
        parts.append(ircWireText(parameter));
    if (command[0].isDigit() && parts.size() >= 2)
        parts.removeFirst();
    return parts.join(QLatin1Char(' '));
}

enum class IrcWhoisLayout {
    User,
    Channels,
    Server,
    Away,
    Idle,
    End,
    NickRest,
    Account,
    NoSuchNick,
    NoSuchServer,
};

struct IrcWhoisNumericSpec
{
    int code = 0;
    const char *label = nullptr;
    IrcWhoisLayout layout = IrcWhoisLayout::NickRest;
};

struct FormattedWhois
{
    QString label;
    QString text;
    QString nick;
    bool terminal = false;
};

constexpr IrcWhoisNumericSpec kWhoisNumerics[] = {
    {311, "whois", IrcWhoisLayout::User},
    {319, "whois", IrcWhoisLayout::Channels},
    {312, "whois", IrcWhoisLayout::Server},
    {301, "whois", IrcWhoisLayout::Away},
    {317, "whois", IrcWhoisLayout::Idle},
    {318, "whois", IrcWhoisLayout::End},
    {276, "whois", IrcWhoisLayout::NickRest},
    {307, "whois", IrcWhoisLayout::NickRest},
    {313, "whois", IrcWhoisLayout::NickRest},
    {320, "whois", IrcWhoisLayout::NickRest},
    {330, "whois", IrcWhoisLayout::Account},
    {338, "whois", IrcWhoisLayout::NickRest},
    {378, "whois", IrcWhoisLayout::NickRest},
    {379, "whois", IrcWhoisLayout::NickRest},
    {671, "whois", IrcWhoisLayout::NickRest},
    {401, nullptr, IrcWhoisLayout::NoSuchNick},
    {402, nullptr, IrcWhoisLayout::NoSuchServer},
};

QString parameterAt(const QStringList& params, int index)
{
    if (index < 0 || index >= params.size())
        return {};
    return params.at(index);
}

const IrcWhoisNumericSpec *whoisSpecFor(int code)
{
    for (const IrcWhoisNumericSpec& row : kWhoisNumerics) {
        if (row.code == code)
            return &row;
    }
    return nullptr;
}

std::optional<FormattedWhois> formatWhois(const IrcMessage& message)
{
    const QString command = commandOf(message);
    if (command.size() != 3)
        return std::nullopt;
    bool ok = false;
    const int code = command.toInt(&ok);
    if (!ok)
        return std::nullopt;
    const IrcWhoisNumericSpec *spec = whoisSpecFor(code);
    if (!spec)
        return std::nullopt;

    QStringList params;
    params.reserve(int(message.parameters.size()));
    for (const std::string& parameter : message.parameters)
        params.append(ircWireText(parameter));

    QString text;
    switch (spec->layout) {
    case IrcWhoisLayout::User: {
        if (params.size() < 6)
            return std::nullopt;
        const QString nick = parameterAt(params, 1);
        const QString user = parameterAt(params, 2);
        const QString host = parameterAt(params, 3);
        const QString realname = parameterAt(params, 5);
        text = QStringLiteral("%1 is %2@%3 (%4)").arg(nick, user, host, realname);
        break;
    }
    case IrcWhoisLayout::Channels: {
        if (params.size() < 3)
            return std::nullopt;
        text = QStringLiteral("%1 is on %2")
                   .arg(parameterAt(params, 1), parameterAt(params, 2));
        break;
    }
    case IrcWhoisLayout::Server: {
        if (params.size() < 4)
            return std::nullopt;
        text = QStringLiteral("%1 using %2 (%3)")
                   .arg(parameterAt(params, 1),
                        parameterAt(params, 2),
                        parameterAt(params, 3));
        break;
    }
    case IrcWhoisLayout::Away: {
        if (params.size() < 3)
            return std::nullopt;
        text = QStringLiteral("%1 is away: %2")
                   .arg(parameterAt(params, 1), parameterAt(params, 2));
        break;
    }
    case IrcWhoisLayout::Idle: {
        if (params.size() < 3)
            return std::nullopt;
        text = QStringLiteral("%1 idle %2s")
                   .arg(parameterAt(params, 1), parameterAt(params, 2));
        if (params.size() >= 5 && !parameterAt(params, 3).isEmpty())
            text += QStringLiteral(", signon %1").arg(parameterAt(params, 3));
        break;
    }
    case IrcWhoisLayout::End: {
        if (params.size() < 2)
            return std::nullopt;
        text = QStringLiteral("End of WHOIS for %1").arg(parameterAt(params, 1));
        break;
    }
    case IrcWhoisLayout::NickRest: {
        if (params.size() < 3)
            return std::nullopt;
        text = parameterAt(params, 1) + QLatin1Char(' ')
            + params.mid(2).join(QLatin1Char(' '));
        break;
    }
    case IrcWhoisLayout::Account: {
        if (params.size() < 3)
            return std::nullopt;
        text = QStringLiteral("%1 is logged in as %2")
                   .arg(parameterAt(params, 1), parameterAt(params, 2));
        break;
    }
    case IrcWhoisLayout::NoSuchNick: {
        if (params.size() < 2)
            return std::nullopt;
        text = QStringLiteral("No such nick: %1").arg(parameterAt(params, 1));
        break;
    }
    case IrcWhoisLayout::NoSuchServer: {
        if (params.size() < 2)
            return std::nullopt;
        text = QStringLiteral("No such server: %1").arg(parameterAt(params, 1));
        break;
    }
    }

    return FormattedWhois{
        spec->label ? QString::fromLatin1(spec->label) : command,
        text,
        parameterAt(params, 1),
        spec->code == 318 || spec->code == 401 || spec->code == 402,
    };
}

class IrcNoticeSpeaker
{
public:
    static std::optional<IrcNoticeSpeaker> tryMake(QString token)
    {
        const QString trimmed = token.trimmed();
        if (trimmed.isEmpty())
            return std::nullopt;
        if (trimmed.compare(QLatin1String("*"), Qt::CaseInsensitive) == 0)
            return std::nullopt;
        return IrcNoticeSpeaker(trimmed);
    }

    const QString& token() const
    {
        return m_token;
    }

private:
    explicit IrcNoticeSpeaker(QString token)
        : m_token(std::move(token))
    {
    }

    QString m_token;
};

enum class IrcNoticeCopyKind {
    Wrapped,
    Bare,
};

struct IrcNoticeStatusCopy
{
    QString label;
    IrcNoticeCopyKind kind = IrcNoticeCopyKind::Bare;
    std::optional<IrcNoticeSpeaker> speaker;
    QString body;

    QString text() const
    {
        if (kind == IrcNoticeCopyKind::Wrapped && speaker.has_value())
            return QStringLiteral("-%1- %2").arg(speaker->token(), body);
        return body;
    }
};

IrcNoticeStatusCopy wrappedNotice(IrcNoticeSpeaker speaker, QString body)
{
    return IrcNoticeStatusCopy{
        QStringLiteral("NOTICE"),
        IrcNoticeCopyKind::Wrapped,
        std::move(speaker),
        std::move(body),
    };
}

IrcNoticeStatusCopy bareNotice(QString body)
{
    return IrcNoticeStatusCopy{
        QStringLiteral("NOTICE"),
        IrcNoticeCopyKind::Bare,
        std::nullopt,
        std::move(body),
    };
}

struct IrcNoticeWireParts
{
    std::optional<QString> prefixNick;
    std::optional<QString> prefixRaw;
    QString target;
    QString body;
};

std::optional<IrcNoticeSpeaker> resolveIncomingNoticeSpeaker(const IrcNoticeWireParts& parts)
{
    if (parts.prefixNick) {
        if (auto speaker = IrcNoticeSpeaker::tryMake(*parts.prefixNick))
            return speaker;
    }
    if (parts.prefixRaw) {
        if (auto speaker = IrcNoticeSpeaker::tryMake(*parts.prefixRaw))
            return speaker;
    }
    return IrcNoticeSpeaker::tryMake(parts.target);
}

std::optional<IrcNoticeWireParts> parseIncomingNotice(const IrcMessage& message)
{
    if (commandOf(message) != QStringLiteral("NOTICE"))
        return std::nullopt;
    if (message.parameters.empty())
        return std::nullopt;

    IrcNoticeWireParts parts;
    if (message.prefix) {
        if (!message.prefix->nick.empty())
            parts.prefixNick = ircWireText(message.prefix->nick);
        if (!message.prefix->raw.empty())
            parts.prefixRaw = ircWireText(message.prefix->raw);
    }
    parts.target = ircWireText(message.parameters.front());
    if (message.parameters.size() >= 2)
        parts.body = ircWireText(message.parameters.back());
    return parts;
}

IrcNoticeStatusCopy presentIncomingNotice(const IrcNoticeWireParts& parts)
{
    if (auto speaker = resolveIncomingNoticeSpeaker(parts))
        return wrappedNotice(std::move(*speaker), parts.body);
    return bareNotice(parts.body);
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

IrcWhoisLine::IrcWhoisLine(QString nick, QString text, Progress progress)
    : m_nick(std::move(nick))
    , m_text(std::move(text))
    , m_progress(progress)
{
}

const QString& IrcWhoisLine::nick() const noexcept
{
    return m_nick;
}

const QString& IrcWhoisLine::text() const noexcept
{
    return m_text;
}

IrcWhoisLine::Progress IrcWhoisLine::progress() const noexcept
{
    return m_progress;
}

bool IrcWhoisLine::terminal() const noexcept
{
    return m_progress == Progress::Terminal;
}

IrcStatusEntry::IrcStatusEntry(QString networkId,
                               QDateTime timestamp,
                               IrcLogSource source,
                               IrcLogSeverity severity,
                               QString label,
                               QString text,
                               std::optional<IrcWhoisLine> whoisLine)
    : m_networkId(std::move(networkId))
    , m_timestamp(std::move(timestamp))
    , m_source(source)
    , m_severity(severity)
    , m_label(std::move(label))
    , m_text(std::move(text))
    , m_whoisLine(std::move(whoisLine))
{
}

IrcStatusEntry IrcStatusEntry::incoming(const QString& networkId, const IrcMessage& message)
{
    const QString command = commandOf(message);
    if (command == QStringLiteral("PRIVMSG") && message.parameters.size() >= 2) {
        if (const auto request = parseCtcpRequest(
                ircWireText(message.parameters.back()))) {
            const QString sender = message.prefix && !message.prefix->nick.empty()
                ? ircWireText(message.prefix->nick)
                : QStringLiteral("unknown");
            const QString text = request->argument.isEmpty()
                ? request->command
                : request->command + QLatin1Char(' ') + request->argument;
            return IrcStatusEntry(networkId,
                                  QDateTime::currentDateTimeUtc(),
                                  IrcLogSource::Server,
                                  IrcLogSeverity::Info,
                                  QStringLiteral("CTCP"),
                                  QStringLiteral("%1 from %2").arg(text, sender));
        }
    }
    if (const auto formatted = formatWhois(message)) {
        std::optional<IrcWhoisLine> line;
        if (!formatted->nick.isEmpty() && !formatted->text.isEmpty()) {
            line = IrcWhoisLine(
                formatted->nick,
                formatted->text,
                formatted->terminal ? IrcWhoisLine::Progress::Terminal
                                    : IrcWhoisLine::Progress::Detail);
        }
        return IrcStatusEntry(networkId,
                              QDateTime::currentDateTimeUtc(),
                              IrcLogSource::Server,
                              severityFor(command),
                              formatted->label,
                              formatted->text,
                              std::move(line));
    }
    if (const auto parts = parseIncomingNotice(message)) {
        const IrcNoticeStatusCopy copy = presentIncomingNotice(*parts);
        return IrcStatusEntry(networkId,
                              QDateTime::currentDateTimeUtc(),
                              IrcLogSource::Server,
                              severityFor(command),
                              copy.label,
                              copy.text());
    }
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

    const QString display = ircWireText(
        std::string_view(wire.constData(), std::size_t(wire.size())));
    const QString verb = firstToken(QStringView(display));
    QString text = display;
    if (isSecretVerb(verb))
        text = redactedSecret(verb);
    else if (verb == QStringLiteral("JOIN"))
        text = redactedOutgoingJoin(display);
    return IrcStatusEntry(networkId,
                          QDateTime::currentDateTimeUtc(),
                          IrcLogSource::Client,
                          severityFor(verb),
                          verb,
                          std::move(text));
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

const IrcWhoisLine *IrcStatusEntry::whoisLine() const noexcept
{
    return m_whoisLine ? &*m_whoisLine : nullptr;
}
