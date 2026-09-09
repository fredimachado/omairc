#include "omairccli.h"

#include "omaircipc.h"
#include "singleinstance.h"

#include <QElapsedTimer>
#include <QLocalSocket>
#include <QString>
#include <QStringList>
#include <QStringView>

#include <array>
#include <optional>
#include <stdio.h>
#include <variant>

namespace OmaircCli {
namespace {

constexpr int kMaxIpcLineBytes = 64 * 1024;
constexpr int kResponseTimeoutMs = 3000;

enum class NetworkOption {
    Forbidden,
    Optional,
};

enum class OperandShape {
    None,
    SendTargetAndText,
};

enum class OptionBoundary {
    AllArguments,
    FirstOperand,
};

struct CommandGrammar {
    NetworkOption network;
    OperandShape operands;
    OptionBoundary optionBoundary;
    bool acceptsDoubleDash;
};

struct CommandSpec {
    OmaircIpc::Command ipcCommand;
    const char *canonicalName;
    const char *alias;
    const char *summary;
    CommandGrammar grammar;
};

struct RootOptionSpec {
    const char *spelling;
    const char *alias;
    const char *description;
};

struct CliSchema {
    const char *programName;
    const char *description;
    std::array<RootOptionSpec, 3> rootOptions;
    std::array<CommandSpec, 4> commands;
};

constexpr CliSchema kSchema{
    "omairc",
    "A dead-simple IRC client for Omarchy.",
    {{
        {"--mock", nullptr,
         "Open the local prototype UI without connecting."},
        {"--help", "-h", "Show this help."},
        {"--version", nullptr, "Show the version."},
    }},
    {{
        {
            OmaircIpc::Command::Connections,
            "connections",
            "list",
            "List IRC connections.",
            {NetworkOption::Forbidden, OperandShape::None,
             OptionBoundary::AllArguments, false},
        },
        {
            OmaircIpc::Command::Status,
            "status",
            nullptr,
            "Show connection status.",
            {NetworkOption::Optional, OperandShape::None,
             OptionBoundary::AllArguments, false},
        },
        {
            OmaircIpc::Command::Send,
            "send",
            nullptr,
            "Send a message to a channel or nick.",
            {NetworkOption::Optional, OperandShape::SendTargetAndText,
             OptionBoundary::FirstOperand, true},
        },
        {
            OmaircIpc::Command::Raise,
            "raise",
            nullptr,
            "Raise the running Omairc window.",
            {NetworkOption::Forbidden, OperandShape::None,
             OptionBoundary::AllArguments, false},
        },
    }},
};

struct ParsedFields {
    QString networkId;
    QStringList operands;
};

struct HelpRequested {};

struct ParseFailure {
    QString message;
};

using ScanResult = std::variant<HelpRequested, ParsedFields, ParseFailure>;

void writeStdout(const QByteArray &line)
{
    fwrite(line.constData(), 1, size_t(line.size()), stdout);
    fputc('\n', stdout);
    fflush(stdout);
}

void writeStderr(const QString &message)
{
    const QByteArray bytes = message.toUtf8();
    fwrite(bytes.constData(), 1, size_t(bytes.size()), stderr);
    fputc('\n', stderr);
    fflush(stderr);
}

int fail(const QString &message, int code = 1)
{
    writeStdout(OmaircIpc::errorResponse(message));
    writeStderr(message);
    return code;
}

bool isFlag(const QString &arg)
{
    return arg.startsWith(QLatin1Char('-'));
}

bool isHelpToken(const QString &arg)
{
    return arg == QLatin1String("-h") || arg == QLatin1String("--help");
}

bool isRootOptionToken(const QString &arg)
{
    return arg == QLatin1String("--mock")
        || arg == QLatin1String("--version") || isHelpToken(arg);
}

QStringList decodeArguments(int argc, char *const argv[])
{
    QStringList args;
    args.reserve(argc);
    for (int i = 0; i < argc; ++i) {
        if (argv && argv[i])
            args.append(QString::fromLocal8Bit(argv[i]));
        else
            args.append(QString());
    }
    return args;
}

const CommandSpec *findCommand(QStringView spelling)
{
    for (const CommandSpec &spec : kSchema.commands) {
        if (spelling == QLatin1String(spec.canonicalName))
            return &spec;
        if (spec.alias && spelling == QLatin1String(spec.alias))
            return &spec;
    }
    return nullptr;
}

QString commandSynopsis(const CommandSpec &spec)
{
    QString line = QLatin1String(kSchema.programName);
    line += QLatin1Char(' ');
    line += QLatin1String(spec.canonicalName);
    if (spec.grammar.network == NetworkOption::Optional)
        line += QStringLiteral(" [--network ID]");
    if (spec.grammar.acceptsDoubleDash)
        line += QStringLiteral(" [--]");
    if (spec.grammar.operands == OperandShape::SendTargetAndText)
        line += QStringLiteral(" TARGET TEXT...");
    return line;
}

QString optionDisplayName(const RootOptionSpec &option)
{
    if (option.alias) {
        return QLatin1String(option.alias) + QStringLiteral(", ")
            + QLatin1String(option.spelling);
    }
    return QLatin1String(option.spelling);
}

QString commandDisplayName(const CommandSpec &spec)
{
    if (spec.alias) {
        return QLatin1String(spec.canonicalName) + QStringLiteral(", ")
            + QLatin1String(spec.alias);
    }
    return QLatin1String(spec.canonicalName);
}

QByteArray renderColumns(const QStringList &labels, const QStringList &texts)
{
    int width = 0;
    for (const QString &label : labels)
        width = qMax(width, int(label.size()));

    QByteArray out;
    for (int i = 0; i < labels.size(); ++i) {
        out += "  ";
        out += labels.at(i).toUtf8();
        out += QByteArray(width - int(labels.at(i).size()) + 2, ' ');
        out += texts.at(i).toUtf8();
        out += '\n';
    }
    return out;
}

QByteArray renderRootHelp(const CliSchema &schema)
{
    QByteArray out = "Usage: ";
    out += schema.programName;
    for (const RootOptionSpec &option : schema.rootOptions) {
        out += " [";
        out += option.spelling;
        out += ']';
    }
    out += " [command]\n\n";
    out += schema.description;
    out += "\n\nOptions:\n";

    QStringList optionLabels;
    QStringList optionTexts;
    for (const RootOptionSpec &option : schema.rootOptions) {
        optionLabels.append(optionDisplayName(option));
        optionTexts.append(QLatin1String(option.description));
    }
    out += renderColumns(optionLabels, optionTexts);

    out += "\nCommands:\n";
    QStringList commandLabels;
    QStringList commandTexts;
    for (const CommandSpec &spec : schema.commands) {
        commandLabels.append(commandDisplayName(spec));
        commandTexts.append(QLatin1String(spec.summary));
    }
    out += renderColumns(commandLabels, commandTexts);
    out += "\nRun 'omairc <command> --help' for command help.\n";
    return out;
}

QByteArray renderCommandHelp(const CommandSpec &spec)
{
    QByteArray out = "Usage: ";
    out += commandSynopsis(spec).toUtf8();
    out += "\n\n";
    out += spec.summary;
    out += "\n\nOptions:\n";

    QStringList labels;
    QStringList texts;
    if (spec.grammar.network == NetworkOption::Optional) {
        labels.append(QStringLiteral("--network ID"));
        texts.append(QStringLiteral("Select a connection by id."));
    }
    labels.append(QStringLiteral("-h, --help"));
    texts.append(QStringLiteral("Show this help."));
    if (spec.grammar.acceptsDoubleDash) {
        labels.append(QStringLiteral("--"));
        texts.append(QStringLiteral("End option processing."));
    }
    out += renderColumns(labels, texts);
    return out;
}

ScanResult scanCommand(const CommandSpec &spec,
                       const QStringList &arguments,
                       const QString &invokedName)
{
    ParsedFields fields;
    bool optionsAccepted = true;
    for (int i = 0; i < arguments.size(); ++i) {
        const QString &arg = arguments.at(i);
        if (optionsAccepted && isHelpToken(arg))
            return HelpRequested{};
        if (optionsAccepted && arg == QLatin1String("--")) {
            if (!spec.grammar.acceptsDoubleDash)
                return ParseFailure{QStringLiteral("Unknown option: --")};
            optionsAccepted = false;
            continue;
        }
        if (optionsAccepted && arg == QLatin1String("--network")) {
            if (spec.grammar.network == NetworkOption::Forbidden)
                return ParseFailure{QStringLiteral("Unknown option: --network")};
            if (i + 1 >= arguments.size() || isFlag(arguments.at(i + 1))) {
                return ParseFailure{QStringLiteral("--network requires an id")};
            }
            fields.networkId = arguments.at(++i);
            continue;
        }
        if (optionsAccepted && isFlag(arg))
            return ParseFailure{QStringLiteral("Unknown option: %1").arg(arg)};
        fields.operands.append(arg);
        if (spec.grammar.optionBoundary == OptionBoundary::FirstOperand)
            optionsAccepted = false;
    }

    if (spec.grammar.operands == OperandShape::None) {
        if (!fields.operands.isEmpty()) {
            return ParseFailure{
                QStringLiteral("%1 takes no arguments").arg(invokedName)};
        }
        return fields;
    }

    if (fields.operands.isEmpty())
        return ParseFailure{QStringLiteral("send requires a target and text")};
    if (fields.operands.size() < 2) {
        return ParseFailure{
            QStringLiteral("send requires text after the target")};
    }
    return fields;
}

OmaircIpc::Request makeRequest(const CommandSpec &spec, ParsedFields fields)
{
    OmaircIpc::Request request;
    request.command = spec.ipcCommand;
    request.networkId = fields.networkId;
    if (spec.grammar.operands == OperandShape::SendTargetAndText) {
        request.target = fields.operands.takeFirst();
        request.text = fields.operands.join(QLatin1Char(' '));
    }
    return request;
}

TerminalExit successfulText(QByteArray text)
{
    if (!text.endsWith('\n'))
        text += '\n';
    TerminalExit finished;
    finished.standardOutput = text;
    finished.code = 0;
    return finished;
}

TerminalExit usageFailure(const QString &message)
{
    TerminalExit finished;
    finished.standardOutput = OmaircIpc::errorResponse(message) + '\n';
    finished.standardError = message.toUtf8() + '\n';
    finished.code = 1;
    return finished;
}

StartupPlan planCommand(const CommandSpec &spec,
                        const QStringList &arguments,
                        const QString &invokedName)
{
    const ScanResult scan = scanCommand(spec, arguments, invokedName);
    if (std::holds_alternative<HelpRequested>(scan))
        return successfulText(renderCommandHelp(spec));
    if (const auto *failure = std::get_if<ParseFailure>(&scan))
        return usageFailure(failure->message);
    return ControlRequest{
        makeRequest(spec, std::get<ParsedFields>(scan))};
}

std::optional<QByteArray> readLine(QLocalSocket &socket)
{
    QElapsedTimer timer;
    timer.start();
    QByteArray buffer;
    while (true) {
        buffer += socket.readAll();
        const qsizetype newline = buffer.indexOf('\n');
        if (newline >= 0) {
            if (newline > kMaxIpcLineBytes)
                return std::nullopt;
            return buffer.left(newline).trimmed();
        }
        if (buffer.size() > kMaxIpcLineBytes)
            return std::nullopt;

        const qint64 remaining = kResponseTimeoutMs - timer.elapsed();
        if (remaining <= 0
            || !socket.waitForReadyRead(int(qMin<qint64>(remaining, 100))))
            return std::nullopt;
    }
}

int sendRequest(const OmaircIpc::Request &request)
{
    QLocalSocket socket;
    socket.connectToServer(SingleInstance::socketPath());
    if (!socket.waitForConnected(1000)) {
        return fail(QStringLiteral(
            "Omairc is not running (no local socket). "
            "Start the client first."));
    }

    const QByteArray payload = OmaircIpc::encodeRequest(request) + '\n';
    if (socket.write(payload) != payload.size()
        || !socket.waitForBytesWritten(1000)) {
        return fail(QStringLiteral("Failed to write to Omairc socket"));
    }

    const std::optional<QByteArray> response = readLine(socket);
    if (!response)
        return fail(QStringLiteral("Invalid or missing response from Omairc"));
    if (response->isEmpty())
        return fail(QStringLiteral("No response from Omairc"));

    writeStdout(*response);
    return OmaircIpc::responseOk(*response) ? 0 : 1;
}

}

StartupPlan plan(int argc, char *const argv[], const char *version)
{
    const QStringList decoded = decodeArguments(argc, argv);
    const QStringList tokens =
        decoded.size() > 0 ? decoded.mid(1) : QStringList();

    if (tokens.isEmpty())
        return GuiLaunch{false};

    if (const CommandSpec *spec = findCommand(tokens.constFirst()))
        return planCommand(*spec, tokens.mid(1), tokens.constFirst());

    bool mock = false;
    bool help = false;
    bool onlyRootOptions = true;
    for (const QString &token : tokens) {
        if (token == QLatin1String("--mock"))
            mock = true;
        else if (isHelpToken(token))
            help = true;
        else if (!isRootOptionToken(token))
            onlyRootOptions = false;
    }
    if (onlyRootOptions && help)
        return successfulText(renderRootHelp(kSchema));
    if (tokens.size() == 1
        && tokens.constFirst() == QLatin1String("--version")) {
        QByteArray text = QByteArrayLiteral("omairc ");
        if (version)
            text += version;
        text += '\n';
        return successfulText(text);
    }
    if (isHelpToken(tokens.constFirst()) ||
        tokens.constFirst() == QLatin1String("--version")) {
        return usageFailure(
            QStringLiteral("Unexpected argument: %1").arg(tokens.at(1)));
    }
    return GuiLaunch{mock};
}

int execute(const ControlRequest &control)
{
    return sendRequest(control.request);
}

}
