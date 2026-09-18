#pragma once

#include <QByteArray>
#include <QDateTime>
#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>
#include <QVector>

#include <optional>
#include <variant>

namespace OmaircIpc {

enum class Command {
    Raise,
    Connections,
    Status,
    Send,
    Read,
    Names,
    Conversations,
};

struct LastWindow {
    int count = 50;
};

struct SinceWindow {
    QDateTime cutoffUtc;
    QString token;
};

struct UnreadWindow {};

using ReadWindow = std::variant<LastWindow, SinceWindow, UnreadWindow>;

struct Request {
    Command command = Command::Raise;
    QString networkId;
    QString target;
    QString text;
    ReadWindow window = LastWindow{};
};

struct ParseError {
    QString message;
};

struct ResolveResult {
    bool ok = false;
    QString networkId;
    QString error;
};

struct ConnectionInfo {
    QString id;
    QString name;
    QString host;
    int port = 0;
    bool tls = false;
    QString nick;
    QString state;
    bool selected = false;
    QString lastError;
};

QByteArray raisePing();
bool isRaisePing(const QByteArray &payload);

std::optional<qint64> durationMs(const QString &token);
std::optional<SinceWindow> parseSince(const QString &token);

std::optional<Request> parseRequest(const QByteArray &line, ParseError *error = nullptr);
QByteArray encodeRequest(const Request &request);

ResolveResult resolveNetworkId(const QString &requested,
                               const QStringList &availableIds);

// Agent-facing rows omit empty strings and false flags. Absence means that
// default. Keep top-level `"ok": false` on errors; do not use these for it.
inline void putText(QJsonObject &object, const QString &key, const QString &value)
{
    if (!value.isEmpty())
        object.insert(key, value);
}

inline void putFlag(QJsonObject &object, const QString &key, bool value)
{
    if (value)
        object.insert(key, true);
}

QByteArray okResponse();
QByteArray okConnections(const QVector<ConnectionInfo> &connections);
QByteArray okStatus(const ConnectionInfo &status);
QByteArray okMessages(const QJsonArray &messages, bool truncated = false);
bool responseTruncated(const QByteArray &line);
QByteArray okMembers(const QJsonArray &members);
QByteArray okConversations(const QJsonArray &conversations);
QByteArray errorResponse(const QString &message);
QByteArray uncertainResponse(const QString &message);

bool responseHasOk(const QByteArray &line);
bool responseOk(const QByteArray &line);
bool responseUncertain(const QByteArray &line);
QString responseError(const QByteArray &line);
QJsonArray responseConnections(const QByteArray &line);
QJsonObject responseStatus(const QByteArray &line);
QJsonArray responseMessages(const QByteArray &line);
QJsonArray responseMembers(const QByteArray &line);
QJsonArray responseConversations(const QByteArray &line);

QString stateLabel(const QString &sessionStateName);

}
