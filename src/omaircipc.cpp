#include "omaircipc.h"

#include <QJsonDocument>
#include <QJsonParseError>

namespace OmaircIpc {
namespace {

QByteArray toLine(const QJsonObject &object)
{
    return QJsonDocument(object).toJson(QJsonDocument::Compact);
}

QJsonObject parseObject(const QByteArray &line, QString *error)
{
    QJsonParseError parseError;
    const QJsonDocument document =
        QJsonDocument::fromJson(line.trimmed(), &parseError);
    if (parseError.error != QJsonParseError::NoError || !document.isObject()) {
        if (error)
            *error = QStringLiteral("Invalid JSON request");
        return {};
    }
    return document.object();
}

QJsonObject connectionObject(const ConnectionInfo &info)
{
    QJsonObject object;
    object.insert(QStringLiteral("id"), info.id);
    object.insert(QStringLiteral("host"), info.host);
    object.insert(QStringLiteral("port"), info.port);
    object.insert(QStringLiteral("tls"), info.tls);
    object.insert(QStringLiteral("nick"), info.nick);
    object.insert(QStringLiteral("state"), info.state);
    object.insert(QStringLiteral("selected"), info.selected);
    if (!info.lastError.isEmpty())
        object.insert(QStringLiteral("lastError"), info.lastError);
    return object;
}

}

QByteArray raisePing()
{
    return QByteArrayLiteral("!");
}

bool isRaisePing(const QByteArray &payload)
{
    const QByteArray trimmed = payload.trimmed();
    return trimmed == QByteArrayLiteral("!");
}

std::optional<Request> parseRequest(const QByteArray &line, ParseError *error)
{
    const QByteArray trimmed = line.trimmed();
    if (trimmed.isEmpty()) {
        if (error)
            error->message = QStringLiteral("Empty request");
        return std::nullopt;
    }
    if (isRaisePing(trimmed)) {
        Request request;
        request.command = Command::Raise;
        return request;
    }

    QString parseMessage;
    const QJsonObject object = parseObject(trimmed, &parseMessage);
    if (object.isEmpty() && !parseMessage.isEmpty()) {
        if (error)
            error->message = parseMessage;
        return std::nullopt;
    }

    const QString cmd = object.value(QStringLiteral("cmd")).toString().trimmed();
    Request request;
    if (cmd == QLatin1String("raise")) {
        request.command = Command::Raise;
    } else if (cmd == QLatin1String("connections") || cmd == QLatin1String("list")) {
        request.command = Command::Connections;
    } else if (cmd == QLatin1String("status")) {
        request.command = Command::Status;
        request.networkId = object.value(QStringLiteral("network")).toString().trimmed();
    } else if (cmd == QLatin1String("send")) {
        request.command = Command::Send;
        request.networkId = object.value(QStringLiteral("network")).toString().trimmed();
        request.target = object.value(QStringLiteral("target")).toString().trimmed();
        request.text = object.value(QStringLiteral("text")).toString();
        if (request.target.isEmpty()) {
            if (error)
                error->message = QStringLiteral("send requires a target");
            return std::nullopt;
        }
        if (request.text.isEmpty()) {
            if (error)
                error->message = QStringLiteral("send requires text");
            return std::nullopt;
        }
    } else if (cmd.isEmpty()) {
        if (error)
            error->message = QStringLiteral("Missing cmd");
        return std::nullopt;
    } else {
        if (error)
            error->message = QStringLiteral("Unknown command: %1").arg(cmd);
        return std::nullopt;
    }
    return request;
}

QByteArray encodeRequest(const Request &request)
{
    QJsonObject object;
    switch (request.command) {
    case Command::Raise:
        object.insert(QStringLiteral("cmd"), QStringLiteral("raise"));
        break;
    case Command::Connections:
        object.insert(QStringLiteral("cmd"), QStringLiteral("connections"));
        break;
    case Command::Status:
        object.insert(QStringLiteral("cmd"), QStringLiteral("status"));
        if (!request.networkId.isEmpty())
            object.insert(QStringLiteral("network"), request.networkId);
        break;
    case Command::Send:
        object.insert(QStringLiteral("cmd"), QStringLiteral("send"));
        if (!request.networkId.isEmpty())
            object.insert(QStringLiteral("network"), request.networkId);
        object.insert(QStringLiteral("target"), request.target);
        object.insert(QStringLiteral("text"), request.text);
        break;
    }
    return toLine(object);
}

ResolveResult resolveNetworkId(const QString &requested,
                               const QStringList &availableIds)
{
    ResolveResult result;
    if (!requested.isEmpty()) {
        if (!availableIds.contains(requested)) {
            result.error = QStringLiteral("Unknown network id '%1'.")
                               .arg(requested);
            return result;
        }
        result.ok = true;
        result.networkId = requested;
        return result;
    }

    if (availableIds.isEmpty()) {
        result.error = QStringLiteral("No connections are available.");
        return result;
    }
    if (availableIds.size() > 1) {
        result.error = QStringLiteral(
            "Multiple connections are available; specify a network id.");
        return result;
    }

    result.ok = true;
    result.networkId = availableIds.constFirst();
    return result;
}

QByteArray okResponse()
{
    QJsonObject object;
    object.insert(QStringLiteral("ok"), true);
    return toLine(object);
}

QByteArray okConnections(const QVector<ConnectionInfo> &connections)
{
    QJsonArray array;
    for (const ConnectionInfo &info : connections)
        array.append(connectionObject(info));
    QJsonObject object;
    object.insert(QStringLiteral("ok"), true);
    object.insert(QStringLiteral("connections"), array);
    return toLine(object);
}

QByteArray okStatus(const ConnectionInfo &status)
{
    QJsonObject object;
    object.insert(QStringLiteral("ok"), true);
    object.insert(QStringLiteral("status"), connectionObject(status));
    return toLine(object);
}

QByteArray errorResponse(const QString &message)
{
    QJsonObject object;
    object.insert(QStringLiteral("ok"), false);
    object.insert(QStringLiteral("error"), message);
    return toLine(object);
}

bool responseOk(const QByteArray &line)
{
    QString unused;
    const QJsonObject object = parseObject(line, &unused);
    return object.value(QStringLiteral("ok")).toBool(false);
}

QString responseError(const QByteArray &line)
{
    QString unused;
    const QJsonObject object = parseObject(line, &unused);
    return object.value(QStringLiteral("error")).toString();
}

QJsonArray responseConnections(const QByteArray &line)
{
    QString unused;
    const QJsonObject object = parseObject(line, &unused);
    return object.value(QStringLiteral("connections")).toArray();
}

QJsonObject responseStatus(const QByteArray &line)
{
    QString unused;
    const QJsonObject object = parseObject(line, &unused);
    return object.value(QStringLiteral("status")).toObject();
}

QString stateLabel(const QString &sessionStateName)
{
    if (sessionStateName == QLatin1String("Idle")
        || sessionStateName == QLatin1String("Failed")) {
        return QStringLiteral("Offline");
    }
    if (sessionStateName == QLatin1String("Connecting")
        || sessionStateName == QLatin1String("StsUpgrading")
        || sessionStateName == QLatin1String("CapLs")
        || sessionStateName == QLatin1String("CapReq")
        || sessionStateName == QLatin1String("Sasl")
        || sessionStateName == QLatin1String("Registering")) {
        return QStringLiteral("Connecting");
    }
    if (sessionStateName == QLatin1String("Registered"))
        return QStringLiteral("Connected");
    if (sessionStateName == QLatin1String("Closing"))
        return QStringLiteral("Disconnecting");
    if (sessionStateName == QLatin1String("Reconnecting"))
        return QStringLiteral("Reconnecting");
    return QStringLiteral("Offline");
}

}
