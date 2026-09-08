#include "omairccli.h"

#include "omaircipc.h"
#include "singleinstance.h"

#include <QCoreApplication>
#include <QLocalSocket>
#include <QStringList>

#include <optional>
#include <stdio.h>
#include <string.h>

namespace OmaircCli {
namespace {

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

QByteArray readLine(QLocalSocket &socket)
{
    while (!socket.canReadLine()) {
        if (!socket.waitForReadyRead(3000))
            return {};
    }
    return socket.readLine().trimmed();
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

    const QByteArray response = readLine(socket);
    if (response.isEmpty())
        return fail(QStringLiteral("No response from Omairc"));

    writeStdout(response);
    return OmaircIpc::responseOk(response) ? 0 : 1;
}

std::optional<OmaircIpc::Request> parseArgsImpl(const QStringList &args,
                                                QString &error)
{
    if (args.isEmpty()) {
        error = QStringLiteral("Missing command");
        return std::nullopt;
    }

    const QString command = args.constFirst();
    OmaircIpc::Request request;

    if (command == QLatin1String("raise")) {
        if (args.size() != 1) {
            error = QStringLiteral("raise takes no arguments");
            return std::nullopt;
        }
        request.command = OmaircIpc::Command::Raise;
        return request;
    }

    if (command == QLatin1String("connections")
        || command == QLatin1String("list")) {
        if (args.size() != 1) {
            error = QStringLiteral("%1 takes no arguments").arg(command);
            return std::nullopt;
        }
        request.command = OmaircIpc::Command::Connections;
        return request;
    }

    if (command == QLatin1String("status")) {
        request.command = OmaircIpc::Command::Status;
        for (int i = 1; i < args.size(); ++i) {
            const QString &arg = args.at(i);
            if (arg == QLatin1String("--network")) {
                if (i + 1 >= args.size() || isFlag(args.at(i + 1))) {
                    error = QStringLiteral("--network requires an id");
                    return std::nullopt;
                }
                request.networkId = args.at(++i);
                continue;
            }
            error = QStringLiteral("Unexpected argument: %1").arg(arg);
            return std::nullopt;
        }
        return request;
    }

    if (command == QLatin1String("send")) {
        request.command = OmaircIpc::Command::Send;
        QStringList positional;
        bool acceptOptions = true;
        for (int i = 1; i < args.size(); ++i) {
            const QString &arg = args.at(i);
            if (acceptOptions && arg == QLatin1String("--")) {
                acceptOptions = false;
                continue;
            }
            if (acceptOptions && arg == QLatin1String("--network")) {
                if (i + 1 >= args.size() || isFlag(args.at(i + 1))) {
                    error = QStringLiteral("--network requires an id");
                    return std::nullopt;
                }
                request.networkId = args.at(++i);
                continue;
            }
            if (acceptOptions && isFlag(arg)) {
                error = QStringLiteral("Unknown option: %1").arg(arg);
                return std::nullopt;
            }
            positional.append(arg);
            acceptOptions = false;
        }
        if (positional.isEmpty()) {
            error = QStringLiteral("send requires a target and text");
            return std::nullopt;
        }
        if (positional.size() < 2) {
            error = QStringLiteral("send requires text after the target");
            return std::nullopt;
        }
        request.target = positional.takeFirst();
        request.text = positional.join(QLatin1Char(' '));
        return request;
    }

    error = QStringLiteral("Unknown command: %1").arg(command);
    return std::nullopt;
}

}

std::optional<OmaircIpc::Request> parseArgs(const QStringList &args,
                                            QString &error)
{
    return parseArgsImpl(args, error);
}

bool looksLikeCommand(int argc, char **argv)
{
    if (argc < 2 || !argv[1])
        return false;
    const char *arg = argv[1];
    if (strcmp(arg, "connections") == 0 || strcmp(arg, "list") == 0
        || strcmp(arg, "status") == 0 || strcmp(arg, "send") == 0
        || strcmp(arg, "raise") == 0) {
        return true;
    }
    return false;
}

int run(QCoreApplication &app)
{
    QString error;
    const std::optional<OmaircIpc::Request> request =
        parseArgs(app.arguments().mid(1), error);
    if (!request)
        return fail(error);
    return sendRequest(*request);
}

}
