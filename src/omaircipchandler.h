#pragma once

#include "omaircipc.h"

#include <QByteArray>
#include <functional>

class IrcController;

class OmaircIpcHandler
{
public:
    using RaiseFn = std::function<void()>;

    explicit OmaircIpcHandler(IrcController *controller = nullptr,
                              RaiseFn raiseFn = {});

    void setController(IrcController *controller);
    void setRaiseFn(RaiseFn raiseFn);

    QByteArray handleLine(const QByteArray &line) const;

private:
    QVector<OmaircIpc::ConnectionInfo> connectionInfos() const;
    OmaircIpc::ConnectionInfo infoFor(const QString &networkId) const;
    QStringList networkIds() const;
    QByteArray handle(const OmaircIpc::Request &request) const;

    IrcController *m_controller = nullptr;
    RaiseFn m_raiseFn;
};
