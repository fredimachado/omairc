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

QStringView skipSpaces(QStringView text)
{
    while (!text.isEmpty() && text.front() == QLatin1Char(' '))
        text = text.mid(1);
    return text;
}

std::optional<IrcWireCommand> tokenize(QStringView line)
{
    QStringView rest = skipSpaces(line);
    if (rest.isEmpty())
        return std::nullopt;

    if (rest.startsWith(QLatin1Char('@'))) {
        const qsizetype space = rest.indexOf(QLatin1Char(' '));
        if (space < 0)
            return std::nullopt;
        rest = skipSpaces(rest.mid(space + 1));
        if (rest.isEmpty())
            return std::nullopt;
    }

    if (rest.startsWith(QLatin1Char(':'))) {
        const qsizetype space = rest.indexOf(QLatin1Char(' '));
        if (space < 0)
            return std::nullopt;
        rest = skipSpaces(rest.mid(space + 1));
        if (rest.isEmpty())
            return std::nullopt;
    }

    const qsizetype verbEnd = rest.indexOf(QLatin1Char(' '));
    IrcWireCommand command;
    command.verb = (verbEnd < 0 ? rest : rest.left(verbEnd)).toString().toUpper();
    if (command.verb.isEmpty())
        return std::nullopt;
    rest = verbEnd < 0 ? QStringView() : skipSpaces(rest.mid(verbEnd + 1));

    while (!rest.isEmpty()) {
        if (rest.startsWith(QLatin1Char(':'))) {
            command.parameters.append(rest.mid(1).toString());
            break;
        }
        const qsizetype space = rest.indexOf(QLatin1Char(' '));
        if (space < 0) {
            command.parameters.append(rest.toString());
            break;
        }
        command.parameters.append(rest.left(space).toString());
        rest = skipSpaces(rest.mid(space + 1));
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

std::optional<IrcWireCommand> maskServiceRequestBody(const IrcWireCommand& command)
{
    if (command.parameters.size() != 2)
        return std::nullopt;
    if (!hasServiceTarget(command.parameters.at(0)))
        return std::nullopt;
    const QStringList tokens =
        command.parameters.at(1).split(QLatin1Char(' '), Qt::SkipEmptyParts);
    if (tokens.size() < 2)
        return std::nullopt;
    const QString word = tokens.at(0).toUpper();
    if (!isServicePasswordCommand(word))
        return std::nullopt;
    IrcWireCommand masked = command;
    masked.parameters[1] = word + QStringLiteral(" ***");
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
    const std::optional<IrcWireCommand> masked = mask(viewOf(message));
    if (!masked)
        return std::nullopt;
    return IrcMaskedCommand{masked->verb, masked->parameters};
}
