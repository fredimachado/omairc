#include "omaircipchandler.h"

#include "irc/irccontroller.h"
#include "irc/ircsession.h"

#include <QJsonArray>
#include <QJsonObject>
#include <QMetaEnum>

#include <optional>

OmaircIpcHandler::OmaircIpcHandler(IrcController *controller, RaiseFn raiseFn)
    : m_controller(controller)
    , m_raiseFn(std::move(raiseFn))
{
}

void OmaircIpcHandler::setController(IrcController *controller)
{
    m_controller = controller;
}

void OmaircIpcHandler::setRaiseFn(RaiseFn raiseFn)
{
    m_raiseFn = std::move(raiseFn);
}

QByteArray OmaircIpcHandler::handleLine(const QByteArray &line) const
{
    OmaircIpc::ParseError error;
    const std::optional<OmaircIpc::Request> request =
        OmaircIpc::parseRequest(line, &error);
    if (!request)
        return OmaircIpc::errorResponse(error.message);
    return handle(*request);
}

QStringList OmaircIpcHandler::networkIds() const
{
    if (!m_controller)
        return {};
    return m_controller->networkIds();
}

OmaircIpc::ConnectionInfo OmaircIpcHandler::infoFor(const QString &networkId) const
{
    OmaircIpc::ConnectionInfo info;
    info.id = networkId;
    if (!m_controller)
        return info;

    IrcSession *session = m_controller->session(networkId);
    if (!session)
        return info;

    info.host = session->host();
    info.port = int(session->port());
    info.tls = session->tlsEnabled();
    info.nick = session->nick();
    const QMetaEnum meta = QMetaEnum::fromType<IrcSession::State>();
    info.state = OmaircIpc::stateLabel(
        QString::fromLatin1(meta.valueToKey(int(session->state()))));
    info.selected = m_controller->selectedNetworkId() == networkId;
    info.lastError = m_controller->lastErrorForNetwork(networkId);
    return info;
}

QVector<OmaircIpc::ConnectionInfo> OmaircIpcHandler::connectionInfos() const
{
    QVector<OmaircIpc::ConnectionInfo> infos;
    for (const QString &networkId : networkIds())
        infos.append(infoFor(networkId));
    return infos;
}

QByteArray OmaircIpcHandler::handle(const OmaircIpc::Request &request) const
{
    switch (request.command) {
    case OmaircIpc::Command::Raise:
        if (m_raiseFn)
            m_raiseFn();
        return OmaircIpc::okResponse();
    case OmaircIpc::Command::Connections:
        return OmaircIpc::okConnections(connectionInfos());
    case OmaircIpc::Command::Status: {
        const OmaircIpc::ResolveResult resolved =
            OmaircIpc::resolveNetworkId(request.networkId, networkIds());
        if (!resolved.ok)
            return OmaircIpc::errorResponse(resolved.error);
        return OmaircIpc::okStatus(infoFor(resolved.networkId));
    }
    case OmaircIpc::Command::Send: {
        if (!m_controller)
            return OmaircIpc::errorResponse(
                QStringLiteral("No IRC controller in this process"));
        const OmaircIpc::ResolveResult resolved =
            OmaircIpc::resolveNetworkId(request.networkId, networkIds());
        if (!resolved.ok)
            return OmaircIpc::errorResponse(resolved.error);
        if (!m_controller->sendToTarget(
                resolved.networkId, request.target, request.text)) {
            const QString error = m_controller->lastErrorForNetwork(resolved.networkId).isEmpty()
                ? QStringLiteral("Failed to send message")
                : m_controller->lastErrorForNetwork(resolved.networkId);
            return OmaircIpc::errorResponse(error);
        }
        return OmaircIpc::okResponse();
    }
    case OmaircIpc::Command::Read:
    case OmaircIpc::Command::Names:
    case OmaircIpc::Command::Conversations: {
        if (!m_controller)
            return OmaircIpc::errorResponse(
                QStringLiteral("No IRC controller in this process"));
        const OmaircIpc::ResolveResult resolved =
            OmaircIpc::resolveNetworkId(request.networkId, networkIds());
        if (!resolved.ok)
            return OmaircIpc::errorResponse(resolved.error);
        if (request.command == OmaircIpc::Command::Read)
            return handleRead(request, resolved.networkId);
        if (request.command == OmaircIpc::Command::Names)
            return handleNames(request, resolved.networkId);
        return handleConversations(resolved.networkId);
    }
    }
    return OmaircIpc::errorResponse(QStringLiteral("Unknown command"));
}

QByteArray OmaircIpcHandler::handleRead(const OmaircIpc::Request &request,
                                       const QString &networkId) const
{
    IrcController::CliReadQuery query;
    std::optional<OmaircCliCursor> loaded;
    if (std::holds_alternative<OmaircIpc::UnreadWindow>(request.window)) {
        loaded = m_cursors.load(networkId, request.target);
        if (loaded) {
            query.mode = IrcController::CliReadQuery::Mode::After;
            query.afterUtc = loaded->timestamp;
            query.afterMsgid = loaded->msgid;
            query.afterSequence = loaded->sequence;
        } else {
            query.mode = IrcController::CliReadQuery::Mode::Last;
            query.last = 50;
        }
    } else if (const auto *since =
                   std::get_if<OmaircIpc::SinceWindow>(&request.window)) {
        query.mode = IrcController::CliReadQuery::Mode::Since;
        query.sinceUtc = since->cutoffUtc;
    } else {
        const auto last = std::get<OmaircIpc::LastWindow>(request.window);
        query.mode = IrcController::CliReadQuery::Mode::Last;
        query.last = last.count;
    }

    const auto result =
        m_controller->snapshotMessages(networkId, request.target, query);
    if (const auto *error = std::get_if<QString>(&result))
        return OmaircIpc::errorResponse(*error);
    const auto &snapshot = std::get<IrcController::CliMessageSnapshot>(result);
    const QVector<IrcController::CliMessage> &lines = snapshot.lines;

    if (std::holds_alternative<OmaircIpc::UnreadWindow>(request.window)
        && !lines.isEmpty()) {
        OmaircCliCursor cursor;
        const IrcController::CliMessage &newest = lines.constLast();
        cursor.timestamp = newest.timestamp;
        cursor.msgid = newest.msgid;
        cursor.sequence = newest.sequence;
        if (!m_cursors.save(networkId, request.target, cursor)) {
            qWarning("Could not save the CLI cursor for %s %s",
                     qUtf8Printable(networkId),
                     qUtf8Printable(request.target));
        }
    }

    QJsonArray messages;
    for (const IrcController::CliMessage &line : lines) {
        QJsonObject row;
        OmaircIpc::putText(row, QStringLiteral("network"), line.networkId);
        OmaircIpc::putText(row, QStringLiteral("target"), line.target);
        OmaircIpc::putText(row, QStringLiteral("sender"), line.sender);
        OmaircIpc::putText(row, QStringLiteral("timestamp"),
                           line.timestamp.toUTC().toString(Qt::ISODateWithMs));
        OmaircIpc::putText(row, QStringLiteral("message"), line.message);
        OmaircIpc::putText(row, QStringLiteral("kind"), line.kind);
        OmaircIpc::putText(row, QStringLiteral("msgid"), line.msgid);
        OmaircIpc::putFlag(row, QStringLiteral("mention"), line.mention);
        messages.append(row);
    }
    return OmaircIpc::okMessages(messages, snapshot.truncated);
}

QByteArray OmaircIpcHandler::handleNames(const OmaircIpc::Request &request,
                                        const QString &networkId) const
{
    const auto result = m_controller->snapshotMembers(networkId, request.target);
    if (const auto *error = std::get_if<QString>(&result))
        return OmaircIpc::errorResponse(*error);
    QJsonArray members;
    for (const IrcController::CliMember &member :
         std::get<QVector<IrcController::CliMember>>(result)) {
        QJsonObject row;
        OmaircIpc::putText(row, QStringLiteral("nick"), member.nick);
        OmaircIpc::putText(row, QStringLiteral("label"), member.label);
        OmaircIpc::putFlag(row, QStringLiteral("away"), member.away);
        OmaircIpc::putText(row, QStringLiteral("status"), member.status);
        members.append(row);
    }
    return OmaircIpc::okMembers(members);
}

QByteArray OmaircIpcHandler::handleConversations(const QString &networkId) const
{
    const auto result = m_controller->snapshotConversations(networkId);
    if (const auto *error = std::get_if<QString>(&result))
        return OmaircIpc::errorResponse(*error);
    QJsonArray conversations;
    for (const IrcController::CliConversation &conversation :
         std::get<QVector<IrcController::CliConversation>>(result)) {
        QJsonObject row;
        OmaircIpc::putText(row, QStringLiteral("target"), conversation.target);
        OmaircIpc::putFlag(row, QStringLiteral("channel"), conversation.channel);
        if (conversation.channel)
            OmaircIpc::putText(row, QStringLiteral("topic"), conversation.topic);
        row.insert(QStringLiteral("unread"), conversation.unread);
        OmaircIpc::putFlag(row, QStringLiteral("mention"), conversation.mention);
        conversations.append(row);
    }
    return OmaircIpc::okConversations(conversations);
}
