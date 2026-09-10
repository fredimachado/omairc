#include "ircsecretpolicy.h"

#include "ircservicenick.h"
#include "ircwiretext.h"

#include <QStringList>

#include <string>

namespace
{
enum class IrcSecretShape {
    TailAfterParameters,
    KeyedModeTail,
    ServiceRequestBody,
};

struct IrcSecretRule
{
    const char *verb = nullptr;
    IrcSecretShape shape = IrcSecretShape::TailAfterParameters;
    int visibleParameters = 0;
};

constexpr IrcSecretRule kSecretMap[] = {
    {"PASS", IrcSecretShape::TailAfterParameters, 0},
    {"AUTHENTICATE", IrcSecretShape::TailAfterParameters, 0},
    {"OPER", IrcSecretShape::TailAfterParameters, 1},
    {"JOIN", IrcSecretShape::TailAfterParameters, 1},
    {"MODE", IrcSecretShape::KeyedModeTail, 2},
    {"324", IrcSecretShape::KeyedModeTail, 3},
    {"PRIVMSG", IrcSecretShape::ServiceRequestBody, 1},
    {"NOTICE", IrcSecretShape::ServiceRequestBody, 1},
};

constexpr const char *kServicePasswordCommands[] = {
    "IDENTIFY", "REGISTER", "GHOST", "RECOVER", "RELEASE", "REGAIN", "SETPASS",
};

constexpr bool verbIsUppercase(const char *verb)
{
    if (verb == nullptr || verb[0] == '\0')
        return false;
    for (const char *cursor = verb; *cursor != '\0'; ++cursor) {
        if (*cursor >= 'a' && *cursor <= 'z')
            return false;
    }
    return true;
}

constexpr bool verbsEqual(const char *left, const char *right)
{
    if (left == nullptr || right == nullptr)
        return left == right;
    while (*left != '\0' && *right != '\0') {
        if (*left != *right)
            return false;
        ++left;
        ++right;
    }
    return *left == *right;
}

constexpr bool secretMapIsWellFormed()
{
    constexpr int count = int(sizeof(kSecretMap) / sizeof(kSecretMap[0]));
    for (int index = 0; index < count; ++index) {
        const IrcSecretRule& row = kSecretMap[index];
        if (!verbIsUppercase(row.verb) || row.visibleParameters < 0)
            return false;
        switch (row.shape) {
        case IrcSecretShape::ServiceRequestBody:
            if (row.visibleParameters != 1)
                return false;
            break;
        case IrcSecretShape::KeyedModeTail:
            if (row.visibleParameters < 2)
                return false;
            break;
        case IrcSecretShape::TailAfterParameters:
            break;
        }
        for (int earlier = 0; earlier < index; ++earlier) {
            if (verbsEqual(kSecretMap[earlier].verb, row.verb))
                return false;
        }
    }
    return true;
}

static_assert(secretMapIsWellFormed(),
              "kSecretMap rows must be uppercase, unique, and match their shape");

const IrcSecretRule *ruleFor(const QString& verb)
{
    for (const IrcSecretRule& row : kSecretMap) {
        if (verb == QLatin1String(row.verb))
            return &row;
    }
    return nullptr;
}

bool isServicePasswordCommand(const QString& word)
{
    for (const char *command : kServicePasswordCommands) {
        if (word == QLatin1String(command))
            return true;
    }
    return false;
}

struct IrcWireCommand
{
    QString verb;
    QStringList parameters;
};

QStringView skipFieldSpaces(QStringView text)
{
    while (!text.isEmpty()
           && (text.front() == QLatin1Char(' ') || text.front() == QLatin1Char('\t'))) {
        text = text.mid(1);
    }
    return text;
}

qsizetype fieldBreak(QStringView text)
{
    for (qsizetype index = 0; index < text.size(); ++index) {
        const QChar ch = text.at(index);
        if (ch == QLatin1Char(' ') || ch == QLatin1Char('\t'))
            return index;
    }
    return -1;
}

std::optional<IrcWireCommand> tokenize(QStringView line)
{
    QStringView rest = skipFieldSpaces(line);
    if (rest.isEmpty())
        return std::nullopt;

    if (rest.startsWith(QLatin1Char('@'))) {
        const qsizetype space = fieldBreak(rest);
        if (space < 0)
            return std::nullopt;
        rest = skipFieldSpaces(rest.mid(space + 1));
        if (rest.isEmpty())
            return std::nullopt;
    }

    if (rest.startsWith(QLatin1Char(':'))) {
        const qsizetype space = fieldBreak(rest);
        if (space < 0)
            return std::nullopt;
        rest = skipFieldSpaces(rest.mid(space + 1));
        if (rest.isEmpty())
            return std::nullopt;
    }

    const qsizetype verbEnd = fieldBreak(rest);
    IrcWireCommand command;
    command.verb = (verbEnd < 0 ? rest : rest.left(verbEnd)).toString().toUpper();
    if (command.verb.isEmpty())
        return std::nullopt;
    rest = verbEnd < 0 ? QStringView() : skipFieldSpaces(rest.mid(verbEnd + 1));

    while (!rest.isEmpty()) {
        if (rest.startsWith(QLatin1Char(':'))) {
            command.parameters.append(rest.mid(1).toString());
            break;
        }
        const qsizetype space = fieldBreak(rest);
        if (space < 0) {
            command.parameters.append(rest.toString());
            break;
        }
        command.parameters.append(rest.left(space).toString());
        rest = skipFieldSpaces(rest.mid(space + 1));
    }
    return command;
}

IrcWireCommand viewOf(const IrcMessage& message)
{
    IrcWireCommand command;
    command.verb = ircWireText(message.command).toUpper();
    command.parameters.reserve(int(message.parameters.size()));
    for (const std::string& parameter : message.parameters)
        command.parameters.append(ircWireText(parameter));
    return command;
}

QString trailingSeparator(const QString& trailing)
{
    if (trailing.isEmpty() || trailing.startsWith(QLatin1Char(':'))
        || trailing.contains(QLatin1Char(' '))) {
        return QStringLiteral(" :");
    }
    return QStringLiteral(" ");
}

QString formatLine(const IrcWireCommand& command)
{
    if (command.parameters.isEmpty())
        return command.verb;

    QStringList head = command.parameters;
    const QString trailing = head.takeLast();
    QString line = command.verb;
    if (!head.isEmpty())
        line += QLatin1Char(' ') + head.join(QLatin1Char(' '));
    return line + trailingSeparator(trailing) + trailing;
}

std::optional<IrcWireCommand> maskTailAfterParameters(const IrcWireCommand& command,
                                                      int visibleParameters)
{
    if (command.parameters.size() <= visibleParameters)
        return std::nullopt;
    IrcWireCommand masked = command;
    masked.parameters = command.parameters.mid(0, visibleParameters);
    masked.parameters.append(QStringLiteral("***"));
    return masked;
}

std::optional<IrcWireCommand> maskKeyedModeTail(const IrcWireCommand& command,
                                                int visibleParameters)
{
    if (visibleParameters < 2 || command.parameters.size() <= visibleParameters)
        return std::nullopt;
    const int modeIndex = visibleParameters - 1;
    // RFC channel key is 'k'; 'K' is a different letter and must stay visible.
    if (!command.parameters.at(modeIndex).contains(QLatin1Char('k')))
        return std::nullopt;
    IrcWireCommand masked = command;
    masked.parameters = command.parameters.mid(0, visibleParameters);
    masked.parameters.append(QStringLiteral("***"));
    return masked;
}

bool hasServiceTarget(QStringView targets)
{
    qsizetype start = 0;
    while (start <= targets.size()) {
        const qsizetype comma = targets.indexOf(QLatin1Char(','), start);
        const QStringView piece = comma < 0
            ? targets.mid(start)
            : targets.mid(start, comma - start);
        if (ircIsServiceIdentity(piece, QStringView()))
            return true;
        if (comma < 0)
            break;
        start = comma + 1;
    }
    return false;
}

QStringList serviceBodyTokens(const QString& body)
{
    QString normalized = body;
    if (!normalized.isEmpty() && normalized.front() == QChar(1))
        normalized.remove(0, 1);
    if (!normalized.isEmpty() && normalized.back() == QChar(1))
        normalized.chop(1);
    normalized.replace(QLatin1Char('\t'), QLatin1Char(' '));
    return normalized.split(QLatin1Char(' '), Qt::SkipEmptyParts);
}

QString matchedServiceCommand(const QStringList& tokens)
{
    if (tokens.size() < 2)
        return {};
    const QString first = tokens.at(0).toUpper();
    if (isServicePasswordCommand(first))
        return first;
    if (tokens.size() >= 3 && first == QLatin1String("SET")
        && tokens.at(1).compare(QLatin1String("PASSWORD"), Qt::CaseInsensitive) == 0) {
        return QStringLiteral("SET PASSWORD");
    }
    return {};
}

std::optional<IrcWireCommand> maskServiceRequestBody(const IrcWireCommand& command)
{
    if (command.parameters.size() < 2)
        return std::nullopt;
    if (!hasServiceTarget(command.parameters.at(0)))
        return std::nullopt;
    const QString body = command.parameters.mid(1).join(QLatin1Char(' '));
    const QString word = matchedServiceCommand(serviceBodyTokens(body));
    if (word.isEmpty())
        return std::nullopt;
    IrcWireCommand masked = command;
    masked.parameters = QStringList{command.parameters.at(0),
                                    word + QStringLiteral(" ***")};
    return masked;
}

std::optional<IrcWireCommand> mask(const IrcWireCommand& command)
{
    const IrcSecretRule *rule = ruleFor(command.verb);
    if (!rule)
        return std::nullopt;
    switch (rule->shape) {
    case IrcSecretShape::TailAfterParameters:
        return maskTailAfterParameters(command, rule->visibleParameters);
    case IrcSecretShape::KeyedModeTail:
        return maskKeyedModeTail(command, rule->visibleParameters);
    case IrcSecretShape::ServiceRequestBody:
        return maskServiceRequestBody(command);
    }
    return std::nullopt;
}
}

std::optional<QString> IrcSecretPolicy::redactWireLine(QStringView line)
{
    const std::optional<IrcWireCommand> parsed = tokenize(line);
    if (!parsed)
        return std::nullopt;
    const std::optional<IrcWireCommand> masked = mask(*parsed);
    if (!masked)
        return std::nullopt;
    return formatLine(*masked);
}

std::optional<IrcMaskedCommand> IrcSecretPolicy::redactMessage(const IrcMessage& message)
{
    const QString verb = ircWireText(message.command).toUpper();
    const IrcSecretRule *rule = ruleFor(verb);
    if (!rule)
        return std::nullopt;
    if (rule->shape == IrcSecretShape::ServiceRequestBody) {
        if (message.parameters.size() < 2)
            return std::nullopt;
        if (!hasServiceTarget(ircWireText(message.parameters.front())))
            return std::nullopt;
    } else if (int(message.parameters.size()) <= rule->visibleParameters) {
        return std::nullopt;
    }
    const std::optional<IrcWireCommand> masked = mask(viewOf(message));
    if (!masked)
        return std::nullopt;
    return IrcMaskedCommand{masked->verb, masked->parameters};
}
