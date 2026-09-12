#include "omairccli.h"

#include "omaircipc.h"
#include "singleinstance.h"

#ifndef OMAIRC_VERSION
#error "Build with omairc.pro so OMAIRC_VERSION is defined"
#endif

#include <QCoreApplication>
#include <QElapsedTimer>
#include <QLocalSocket>
#include <QStringList>

#include <optional>
#include <stdio.h>

namespace OmaircCli {
namespace {

constexpr int kMaxIpcLineBytes = 64 * 1024;
constexpr int kResponseTimeoutMs = 3000;
constexpr int kOverviewUsageWidth = 33;

struct CommandSpec {
    CommandId id;
    const char *primaryName;
    const char *aliasName;
    const char *usage;
    const char *summaryLine;
    const char *detailText;
};

const CommandSpec kCommands[] = {
    {
        CommandId::Help,
        "help",
        nullptr,
        "omairc help [command]",
        "Show this help",
        "",
    },
    {
        CommandId::Connections,
        "connections",
        "list",
        "omairc connections",
        "List connections as JSON (alias: list)",
        "Usage: omairc connections\n"
        "\n"
        "List connections as JSON. list is an alias.\n"
        "\n"
        "Each row has id, host, port, tls, nick, state, and selected.\n",
    },
    {
        CommandId::Status,
        "status",
        nullptr,
        "omairc status [--network ID]",
        "Show one connection as JSON",
        "Usage: omairc status [--network ID]\n"
        "\n"
        "Show one connection as JSON.\n"
        "\n"
        "  --network ID   Connection to use. See connections.\n"
        "\n"
        "With one connection, --network may be omitted.\n",
    },
    {
        CommandId::Send,
        "send",
        nullptr,
        "omairc send [--network ID] TARGET TEXT...",
        "Send a message without changing UI selection",
        "Usage: omairc send [--network ID] TARGET TEXT...\n"
        "\n"
        "Send a message to a channel or nick without changing the UI selection.\n"
        "\n"
        "  --network ID   Connection to use. See connections.\n"
        "  --             End options. Later args are the target and text.\n"
        "\n"
        "Examples:\n"
        "  omairc send '#channel' hello\n"
        "  omairc send --network abc '#channel' hello\n"
        "  omairc send '#channel' --help\n"
        "  omairc send -- '#channel' --version\n",
    },
    {
        CommandId::Raise,
        "raise",
        nullptr,
        "omairc raise",
        "Activate the existing window",
        "Usage: omairc raise\n"
        "\n"
        "Activate the existing Omairc window.\n",
    },
};

const CommandSpec *findCommandSpec(const QString &token)
{
    for (const CommandSpec &spec : kCommands) {
        if (token == QLatin1String(spec.primaryName))
            return &spec;
        if (spec.aliasName && token == QLatin1String(spec.aliasName))
            return &spec;
    }
    return nullptr;
}

const CommandSpec *findCommandSpec(CommandId id)
{
    for (const CommandSpec &spec : kCommands) {
        if (spec.id == id)
            return &spec;
    }
    return nullptr;
}

void writeStdoutLine(const QByteArray &line)
{
    fwrite(line.constData(), 1, size_t(line.size()), stdout);
    fputc('\n', stdout);
    fflush(stdout);
}

void writeStdoutRaw(const QString &text)
{
    const QByteArray bytes = text.toUtf8();
    fwrite(bytes.constData(), 1, size_t(bytes.size()), stdout);
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
    writeStdoutLine(OmaircIpc::errorResponse(message));
    writeStderr(message);
    return code;
}

bool isFlag(const QString &arg)
{
    return arg.startsWith(QLatin1Char('-'));
}

bool isHelpFlag(const QString &arg)
{
    return arg == QLatin1String("--help");
}

CliError unexpectedArgument(const QString &arg)
{
    return {QStringLiteral("Unexpected argument: %1").arg(arg)};
}

ParseOutcome parseHelpCommand(const QStringList &args)
{
    if (args.size() == 1)
        return HelpTopic{HelpScope::Overview};
    if (args.size() > 2)
        return CliError{QStringLiteral("help takes at most one argument")};

    const QString &name = args.at(1);
    if (isHelpFlag(name) || name == QLatin1String("help"))
        return HelpTopic{HelpScope::Overview};

    const CommandSpec *spec = findCommandSpec(name);
    if (!spec || spec->id == CommandId::Help)
        return CliError{QStringLiteral("Unknown command: %1").arg(name)};
    return HelpTopic{HelpScope::Command, spec->id};
}

ParseOutcome parseNoArgCommand(const QStringList &args, CommandId id,
                               OmaircIpc::Command ipc)
{
    if (args.size() == 1) {
        OmaircIpc::Request request;
        request.command = ipc;
        return request;
    }
    if (args.size() == 2 && isHelpFlag(args.at(1)))
        return HelpTopic{HelpScope::Command, id};
    return CliError{QStringLiteral("%1 takes no arguments").arg(args.constFirst())};
}

ParseOutcome parseStatusCommand(const QStringList &args)
{
    OmaircIpc::Request request;
    request.command = OmaircIpc::Command::Status;
    for (int i = 1; i < args.size(); ++i) {
        const QString &arg = args.at(i);
        if (isHelpFlag(arg))
            return HelpTopic{HelpScope::Command, CommandId::Status};
        if (arg == QLatin1String("--network")) {
            if (i + 1 >= args.size() || isFlag(args.at(i + 1))) {
                return CliError{QStringLiteral("--network requires an id")};
            }
            request.networkId = args.at(++i);
            continue;
        }
        return unexpectedArgument(arg);
    }
    return request;
}

ParseOutcome parseSendCommand(const QStringList &args)
{
    OmaircIpc::Request request;
    request.command = OmaircIpc::Command::Send;
    QStringList positional;
    bool beforeTarget = true;
    for (int i = 1; i < args.size(); ++i) {
        const QString &arg = args.at(i);
        if (beforeTarget && arg == QLatin1String("--")) {
            beforeTarget = false;
            continue;
        }
        if (beforeTarget && arg == QLatin1String("--network")) {
            if (i + 1 >= args.size() || isFlag(args.at(i + 1))) {
                return CliError{QStringLiteral("--network requires an id")};
            }
            request.networkId = args.at(++i);
            continue;
        }
        if (beforeTarget && isHelpFlag(arg))
            return HelpTopic{HelpScope::Command, CommandId::Send};
        if (beforeTarget && isFlag(arg))
            return CliError{QStringLiteral("Unknown option: %1").arg(arg)};
        positional.append(arg);
        beforeTarget = false;
    }
    if (positional.isEmpty())
        return CliError{QStringLiteral("send requires a target and text")};
    if (positional.size() < 2)
        return CliError{QStringLiteral("send requires text after the target")};
    request.target = positional.takeFirst();
    request.text = positional.join(QLatin1Char(' '));
    return request;
}

std::optional<QByteArray> readLine(QLocalSocket &socket)
{
    QElapsedTimer timer;
    timer.start();
    QByteArray buffer;
    while (true) {
        buffer += socket.read(kMaxIpcLineBytes + 1 - buffer.size());
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

    writeStdoutLine(*response);
    return OmaircIpc::responseOk(*response) ? 0 : 1;
}

}

ParseOutcome parseArgs(const QStringList &args)
{
    if (args.isEmpty())
        return CliError{QStringLiteral("Missing command")};

    const QString &first = args.constFirst();
    if (first == QLatin1String("--help")) {
        if (args.size() != 1)
            return unexpectedArgument(args.at(1));
        return HelpTopic{HelpScope::Overview};
    }
    if (first == QLatin1String("--version")) {
        if (args.size() != 1)
            return unexpectedArgument(args.at(1));
        return VersionRequest{};
    }

    const CommandSpec *spec = findCommandSpec(first);
    if (!spec)
        return CliError{QStringLiteral("Unknown command: %1").arg(first)};

    switch (spec->id) {
    case CommandId::Help:
        return parseHelpCommand(args);
    case CommandId::Connections:
        return parseNoArgCommand(args, spec->id, OmaircIpc::Command::Connections);
    case CommandId::Status:
        return parseStatusCommand(args);
    case CommandId::Send:
        return parseSendCommand(args);
    case CommandId::Raise:
        return parseNoArgCommand(args, spec->id, OmaircIpc::Command::Raise);
    }
    return CliError{QStringLiteral("Unknown command: %1").arg(first)};
}

QString formatHelp(const HelpTopic &topic)
{
    if (topic.scope == HelpScope::Overview) {
        QString text = QStringLiteral(
            "A dead-simple IRC client for Omarchy.\n"
            "\n"
            "Usage:\n"
            "  omairc [--demo-server]          Open the client window\n"
            "  omairc --help                   List commands and GUI flags\n"
            "  omairc --version                Print version and exit\n"
            "\n"
            "Control commands (require a running client):\n");
        for (const CommandSpec &spec : kCommands) {
            if (spec.id == CommandId::Help)
                continue;
            const QString usage = QString::fromUtf8(spec.usage);
            const QString summary = QString::fromUtf8(spec.summaryLine);
            text += QLatin1String("  ");
            if (usage.size() <= kOverviewUsageWidth) {
                text += QStringLiteral("%1").arg(usage, -kOverviewUsageWidth);
                text += summary;
                text += QLatin1Char('\n');
            } else {
                text += usage;
                text += QLatin1Char('\n');
                text += QLatin1String("  ");
                text += QString(kOverviewUsageWidth, QLatin1Char(' '));
                text += summary;
                text += QLatin1Char('\n');
            }
        }
        text += QStringLiteral(
            "\n"
            "Run 'omairc <command> --help' for command detail.\n");
        return text;
    }

    const CommandSpec *spec = findCommandSpec(topic.command);
    if (!spec || spec->id == CommandId::Help)
        return formatHelp(HelpTopic{HelpScope::Overview});
    return QString::fromUtf8(spec->detailText);
}

QString formatVersion()
{
    return QStringLiteral("omairc ") + QLatin1String(OMAIRC_VERSION)
        + QLatin1Char('\n');
}

int printOutcome(const ParseOutcome &outcome)
{
    if (const auto *help = std::get_if<HelpTopic>(&outcome)) {
        writeStdoutRaw(formatHelp(*help));
        return 0;
    }
    if (std::holds_alternative<VersionRequest>(outcome)) {
        writeStdoutRaw(formatVersion());
        return 0;
    }
    if (const auto *error = std::get_if<CliError>(&outcome))
        return fail(error->message);
    return 1;
}

bool looksLikeCommand(int argc, char **argv)
{
    if (argc < 2 || !argv[1])
        return false;
    return findCommandSpec(QString::fromLocal8Bit(argv[1])) != nullptr;
}

int runRequest(QCoreApplication &, const OmaircIpc::Request &request)
{
    return sendRequest(request);
}

}
