#include "omaircipchandler.h"

#include "irc/irccontroller.h"
#include "irc/ircsession.h"

#include <QMetaEnum>

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
    }
    return OmaircIpc::errorResponse(QStringLiteral("Unknown command"));
}
