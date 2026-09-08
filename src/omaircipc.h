#pragma once

#include <QByteArray>
#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>
#include <QVector>

#include <optional>

namespace OmaircIpc {

enum class Command {
    Raise,
    Connections,
    Status,
    Send,
};

struct Request {
    Command command = Command::Raise;
    QString networkId;
    QString target;
    QString text;
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

std::optional<Request> parseRequest(const QByteArray &line, ParseError *error = nullptr);
QByteArray encodeRequest(const Request &request);

ResolveResult resolveNetworkId(const QString &requested,
                               const QStringList &availableIds);

QByteArray okResponse();
QByteArray okConnections(const QVector<ConnectionInfo> &connections);
QByteArray okStatus(const ConnectionInfo &status);
QByteArray errorResponse(const QString &message);

bool responseOk(const QByteArray &line);
QString responseError(const QByteArray &line);
QJsonArray responseConnections(const QByteArray &line);
QJsonObject responseStatus(const QByteArray &line);

QString stateLabel(const QString &sessionStateName);

} // namespace OmaircIpc
