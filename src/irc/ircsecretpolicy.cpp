#include "ircsecretpolicy.h"

#include "ircservicenick.h"
#include "ircwiretext.h"

#include <QStringList>

#include <algorithm>
#include <string>
#include <vector>

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

QString alnumOnlyUpper(const QString& token)
{
    QString alnum;
    alnum.reserve(token.size());
    for (const QChar ch : token) {
        if (ch >= QLatin1Char('A') && ch <= QLatin1Char('Z'))
            alnum.append(ch);
        else if (ch >= QLatin1Char('a') && ch <= QLatin1Char('z'))
            alnum.append(ch.toUpper());
        else if (ch >= QLatin1Char('0') && ch <= QLatin1Char('9'))
            alnum.append(ch);
    }
    return alnum;
}

int letterDistance(const QString& left, const QString& right)
{
    const int rows = left.size();
    const int cols = right.size();
    if (rows == 0)
        return cols;
    if (cols == 0)
        return rows;
    std::vector<int> previous(std::size_t(cols + 1));
    std::vector<int> current(std::size_t(cols + 1));
    for (int column = 0; column <= cols; ++column)
        previous[std::size_t(column)] = column;
    for (int row = 1; row <= rows; ++row) {
        current[0] = row;
        for (int column = 1; column <= cols; ++column) {
            const int cost = left.at(row - 1) == right.at(column - 1) ? 0 : 1;
            current[std::size_t(column)] = std::min({current[std::size_t(column - 1)] + 1,
                                                     previous[std::size_t(column)] + 1,
                                                     previous[std::size_t(column - 1)] + cost});
        }
        previous.swap(current);
    }
    return previous[std::size_t(cols)];
}

QString nearestServicePassword(const QString& word)
{
    const QString alnum = alnumOnlyUpper(word);
    if (alnum.size() < 4)
        return {};
    const char *best = nullptr;
    int bestDistance = 2;
    for (const char *command : kServicePasswordCommands) {
        const int distance = letterDistance(alnum, QLatin1String(command));
        if (distance < bestDistance) {
            bestDistance = distance;
            best = command;
        } else if (distance == bestDistance) {
            best = nullptr;
        }
    }
    if (!best || bestDistance > 1)
        return {};
    return QLatin1String(best);
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

bool modeLetterConsumesParam(QChar letter, bool adding)
{
    if (letter == QLatin1Char('k') || letter == QLatin1Char('o')
        || letter == QLatin1Char('v') || letter == QLatin1Char('h')
        || letter == QLatin1Char('a') || letter == QLatin1Char('q')
        || letter == QLatin1Char('b') || letter == QLatin1Char('e')
        || letter == QLatin1Char('I')) {
        return true;
    }
    return letter == QLatin1Char('l') && adding;
}

bool isRfcDefaultModeLetter(QChar letter, bool adding)
{
    if (modeLetterConsumesParam(letter, adding))
        return true;
    return letter == QLatin1Char('i') || letter == QLatin1Char('m')
        || letter == QLatin1Char('n') || letter == QLatin1Char('p')
        || letter == QLatin1Char('s') || letter == QLatin1Char('t');
}

std::optional<IrcWireCommand> maskKeyedModeTail(const IrcWireCommand& command,
                                                int visibleParameters)
{
    if (visibleParameters < 2 || command.parameters.size() <= visibleParameters)
        return std::nullopt;
    const QString modes = command.parameters.at(visibleParameters - 1);
    bool adding = true;
    bool hasKey = false;
    bool ambiguous = false;
    for (const QChar letter : modes) {
        if (letter == QLatin1Char('+')) {
            adding = true;
            continue;
        }
        if (letter == QLatin1Char('-')) {
            adding = false;
            continue;
        }
        if (letter == QLatin1Char('k'))
            hasKey = true;
        else if (!isRfcDefaultModeLetter(letter, adding))
            ambiguous = true;
    }
    if (!hasKey)
        return std::nullopt;
    if (ambiguous) {
        IrcWireCommand masked = command;
        masked.parameters = command.parameters.mid(0, visibleParameters);
        masked.parameters.append(QStringLiteral("***"));
        return masked;
    }

    IrcWireCommand masked = command;
    bool maskedKey = false;
    int argument = visibleParameters;
    adding = true;
    for (const QChar letter : modes) {
        if (letter == QLatin1Char('+')) {
            adding = true;
            continue;
        }
        if (letter == QLatin1Char('-')) {
            adding = false;
            continue;
        }
        if (!modeLetterConsumesParam(letter, adding))
            continue;
        if (argument >= masked.parameters.size())
            break;
        if (letter == QLatin1Char('k')) {
            masked.parameters[argument] = QStringLiteral("***");
            maskedKey = true;
        }
        ++argument;
    }
    if (!maskedKey)
        return std::nullopt;
    return masked;
}

struct IrcTargetMask
{
    QStringView nick;
    QStringView host;
};

IrcTargetMask splitTargetMask(QStringView target)
{
    IrcTargetMask parts;
    const qsizetype bang = target.indexOf(QLatin1Char('!'));
    const qsizetype at = target.indexOf(QLatin1Char('@'), bang < 0 ? 0 : bang + 1);
    if (bang >= 0) {
        parts.nick = target.left(bang);
        if (at >= 0)
            parts.host = target.mid(at + 1);
    } else if (at >= 0) {
        parts.nick = target.left(at);
        parts.host = target.mid(at + 1);
    } else {
        parts.nick = target;
    }
    return parts;
}

bool hasServiceTarget(QStringView targets, QStringView channelTypes)
{
    qsizetype start = 0;
    while (start <= targets.size()) {
        const qsizetype comma = targets.indexOf(QLatin1Char(','), start);
        const QStringView piece = comma < 0
            ? targets.mid(start)
            : targets.mid(start, comma - start);
        const IrcTargetMask mask = splitTargetMask(piece);
        if (ircIsServiceIdentity(mask.nick, mask.host, channelTypes))
            return true;
        if (comma < 0)
            break;
        start = comma + 1;
    }
    return false;
}

QStringList serviceBodyTokens(const QString& body)
{
    QString normalized = body.trimmed();
    if (!normalized.isEmpty() && normalized.front() == QChar(1))
        normalized.remove(0, 1);
    if (!normalized.isEmpty() && normalized.back() == QChar(1))
        normalized.chop(1);
    normalized = normalized.trimmed();
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
    const QString near = nearestServicePassword(first);
    if (!near.isEmpty())
        return near;
    if (tokens.size() >= 3 && first == QLatin1String("SET")) {
        const QString second = alnumOnlyUpper(tokens.at(1));
        if (second == QLatin1String("PASSWORD")
            || letterDistance(second, QLatin1String("PASSWORD")) == 1) {
            return QStringLiteral("SET PASSWORD");
        }
    }
    return {};
}

std::optional<IrcWireCommand> maskServiceRequestBody(const IrcWireCommand& command,
                                                     QStringView channelTypes)
{
    if (command.parameters.size() < 2)
        return std::nullopt;
    if (!hasServiceTarget(command.parameters.at(0), channelTypes))
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

std::optional<IrcWireCommand> mask(const IrcWireCommand& command, QStringView channelTypes)
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
        return maskServiceRequestBody(command, channelTypes);
    }
    return std::nullopt;
}

const IrcSecretRule *nearestSecretRule(const QString& verb)
{
    const QString alnum = alnumOnlyUpper(verb);
    if (alnum.size() < 3)
        return nullptr;
    const IrcSecretRule *best = nullptr;
    int bestDistance = 2;
    for (const IrcSecretRule& row : kSecretMap) {
        const int distance = letterDistance(alnum, QLatin1String(row.verb));
        if (distance < bestDistance) {
            bestDistance = distance;
            best = &row;
        } else if (distance == bestDistance) {
            best = nullptr;
        }
    }
    if (!best || bestDistance > 1)
        return nullptr;
    return best;
}

std::optional<QString> conservativePreviewMask(QStringView line)
{
    const QString alnum = alnumOnlyUpper(line.toString());
    const IrcSecretRule *best = nullptr;
    int bestLength = 0;
    for (const IrcSecretRule& row : kSecretMap) {
        const QString verb = QLatin1String(row.verb);
        if (verb.size() >= 3 && alnum.startsWith(verb) && verb.size() > bestLength) {
            best = &row;
            bestLength = verb.size();
        }
    }
    if (!best || alnum.size() <= bestLength)
        return std::nullopt;
    return QLatin1String(best->verb) + QStringLiteral(" ***");
}
}

std::optional<QString> IrcSecretPolicy::redactWireLine(QStringView line,
                                                       QStringView channelTypes)
{
    const std::optional<IrcWireCommand> parsed = tokenize(line);
    if (!parsed)
        return std::nullopt;
    const std::optional<IrcWireCommand> masked = mask(*parsed, channelTypes);
    if (!masked)
        return std::nullopt;
    return formatLine(*masked);
}

std::optional<QString> IrcSecretPolicy::redactPreviewLine(QStringView line,
                                                         QStringView channelTypes)
{
    if (const auto safe = redactWireLine(line, channelTypes))
        return safe;
    const std::optional<IrcWireCommand> parsed = tokenize(line);
    if (parsed && !parsed->parameters.isEmpty()) {
        const IrcSecretRule *rule = nearestSecretRule(parsed->verb);
        if (rule) {
            IrcWireCommand canonical = *parsed;
            canonical.verb = QLatin1String(rule->verb);
            if (const auto masked = mask(canonical, channelTypes))
                return formatLine(*masked);
        }
    }
    return conservativePreviewMask(line);
}

std::optional<IrcMaskedCommand> IrcSecretPolicy::redactMessage(const IrcMessage& message,
                                                              QStringView channelTypes)
{
    const QString verb = ircWireText(message.command).toUpper();
    const IrcSecretRule *rule = ruleFor(verb);
    if (!rule)
        return std::nullopt;
    if (rule->shape == IrcSecretShape::ServiceRequestBody) {
        if (message.parameters.size() < 2)
            return std::nullopt;
        if (!hasServiceTarget(ircWireText(message.parameters.front()), channelTypes))
            return std::nullopt;
    } else if (verb == QLatin1String("JOIN")) {
        return std::nullopt;
    } else if (int(message.parameters.size()) <= rule->visibleParameters) {
        return std::nullopt;
    }
    const std::optional<IrcWireCommand> masked = mask(viewOf(message), channelTypes);
    if (!masked)
        return std::nullopt;
    return IrcMaskedCommand{masked->verb, masked->parameters};
}
