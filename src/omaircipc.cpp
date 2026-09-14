#include "omaircipc.h"

#include <QJsonDocument>
#include <QJsonParseError>

#include <limits>
#include <type_traits>

namespace OmaircIpc {
namespace {

QByteArray toLine(const QJsonObject &object)
{
    return QJsonDocument(object).toJson(QJsonDocument::Compact);
}

QByteArray okArray(const char *key, const QJsonArray &array)
{
    QJsonObject object;
    object.insert(QStringLiteral("ok"), true);
    object.insert(QLatin1String(key), array);
    return toLine(object);
}

bool hasWindowKey(const QJsonObject &object, const char *key)
{
    return object.contains(QLatin1String(key));
}

int windowKeyCount(const QJsonObject &object)
{
    return int(hasWindowKey(object, "last"))
        + int(hasWindowKey(object, "since"))
        + int(hasWindowKey(object, "unread"));
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

std::optional<qint64> scaledMs(qint64 count, qint64 factor)
{
    if (count > std::numeric_limits<qint64>::max() / factor)
        return std::nullopt;
    return count * factor;
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
    } else if (cmd == QLatin1String("read")) {
        request.command = Command::Read;
        request.networkId = object.value(QStringLiteral("network")).toString().trimmed();
        request.target = object.value(QStringLiteral("target")).toString().trimmed();
        if (windowKeyCount(object) > 1) {
            if (error)
                error->message = QStringLiteral(
                    "last, since, and unread cannot be combined");
            return std::nullopt;
        }
        if (hasWindowKey(object, "unread")) {
            if (!object.value(QStringLiteral("unread")).toBool()) {
                if (error)
                    error->message = QStringLiteral("unread must be true");
                return std::nullopt;
            }
            request.window = UnreadWindow{};
        } else if (hasWindowKey(object, "since")) {
            const QString token = object.value(QStringLiteral("since")).toString();
            const std::optional<SinceWindow> since = parseSince(token);
            if (!since) {
                if (error)
                    error->message = QStringLiteral("Invalid since value");
                return std::nullopt;
            }
            request.window = *since;
        } else if (hasWindowKey(object, "last")) {
            const int count = object.value(QStringLiteral("last")).toInt(-1);
            if (count < 1 || count > 100) {
                if (error)
                    error->message = QStringLiteral("last must be between 1 and 100");
                return std::nullopt;
            }
            request.window = LastWindow{count};
        } else {
            request.window = LastWindow{50};
        }
    } else if (cmd == QLatin1String("names")) {
        request.command = Command::Names;
        request.networkId = object.value(QStringLiteral("network")).toString().trimmed();
        request.target = object.value(QStringLiteral("target")).toString().trimmed();
        if (request.target.isEmpty()) {
            if (error)
                error->message = QStringLiteral("names requires a target");
            return std::nullopt;
        }
    } else if (cmd == QLatin1String("conversations")) {
        request.command = Command::Conversations;
        request.networkId = object.value(QStringLiteral("network")).toString().trimmed();
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
    case Command::Read:
        object.insert(QStringLiteral("cmd"), QStringLiteral("read"));
        if (!request.networkId.isEmpty())
            object.insert(QStringLiteral("network"), request.networkId);
        if (!request.target.isEmpty())
            object.insert(QStringLiteral("target"), request.target);
        std::visit([&object](const auto &window) {
            using T = std::decay_t<decltype(window)>;
            if constexpr (std::is_same_v<T, LastWindow>) {
                object.insert(QStringLiteral("last"), window.count);
            } else if constexpr (std::is_same_v<T, SinceWindow>) {
                if (!window.token.isEmpty())
                    object.insert(QStringLiteral("since"), window.token);
                else
                    object.insert(QStringLiteral("since"),
                                  window.cutoffUtc.toUTC().toString(Qt::ISODateWithMs));
            } else {
                object.insert(QStringLiteral("unread"), true);
            }
        }, request.window);
        break;
    case Command::Names:
        object.insert(QStringLiteral("cmd"), QStringLiteral("names"));
        if (!request.networkId.isEmpty())
            object.insert(QStringLiteral("network"), request.networkId);
        object.insert(QStringLiteral("target"), request.target);
        break;
    case Command::Conversations:
        object.insert(QStringLiteral("cmd"), QStringLiteral("conversations"));
        if (!request.networkId.isEmpty())
            object.insert(QStringLiteral("network"), request.networkId);
        break;
    }
    return toLine(object);
}

std::optional<qint64> durationMs(const QString &token)
{
    if (token.size() < 2)
        return std::nullopt;
    const QChar unit = token.back();
    bool ok = false;
    const qint64 count = token.left(token.size() - 1).toLongLong(&ok);
    if (!ok || count < 1)
        return std::nullopt;
    if (unit == QLatin1Char('s'))
        return scaledMs(count, 1000);
    if (unit == QLatin1Char('m'))
        return scaledMs(count, 60 * 1000);
    if (unit == QLatin1Char('h'))
        return scaledMs(count, 60 * 60 * 1000);
    if (unit == QLatin1Char('d'))
        return scaledMs(count, qint64(24) * 60 * 60 * 1000);
    return std::nullopt;
}

std::optional<SinceWindow> parseSince(const QString &token)
{
    if (const std::optional<qint64> ms = durationMs(token)) {
        SinceWindow since;
        since.cutoffUtc = QDateTime::currentDateTimeUtc().addMSecs(-*ms);
        since.token = token;
        return since;
    }
    QDateTime parsed = QDateTime::fromString(token, Qt::ISODateWithMs);
    if (!parsed.isValid())
        parsed = QDateTime::fromString(token, Qt::ISODate);
    if (!parsed.isValid())
        return std::nullopt;
    SinceWindow since;
    since.cutoffUtc = parsed.toUTC();
    return since;
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

QByteArray okMessages(const QJsonArray &messages)
{
    return okArray("messages", messages);
}

QByteArray okMembers(const QJsonArray &members)
{
    return okArray("members", members);
}

QByteArray okConversations(const QJsonArray &conversations)
{
    return okArray("conversations", conversations);
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

QJsonArray responseMessages(const QByteArray &line)
{
    QString unused;
    const QJsonObject object = parseObject(line, &unused);
    return object.value(QStringLiteral("messages")).toArray();
}

QJsonArray responseMembers(const QByteArray &line)
{
    QString unused;
    const QJsonObject object = parseObject(line, &unused);
    return object.value(QStringLiteral("members")).toArray();
}

QJsonArray responseConversations(const QByteArray &line)
{
    QString unused;
    const QJsonObject object = parseObject(line, &unused);
    return object.value(QStringLiteral("conversations")).toArray();
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
