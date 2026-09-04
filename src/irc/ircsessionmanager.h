#pragma once

#include "ircsession.h"

#include <QHash>
#include <QObject>

class IrcSessionManager : public QObject
{
    Q_OBJECT

public:
    explicit IrcSessionManager(QObject *parent = nullptr);

    IrcSession *createSession(const IrcSessionConfig &config,
                              IrcTransport *transport,
                              IrcReconnectTimer *reconnectTimer = nullptr);
    IrcSession *findSession(const QString &networkId) const;

    // Refuses activation until the currently active network has stopped.
    bool activateSession(const QString &networkId);
    bool stopSession(const QString &networkId);
    QString activeNetworkId() const;

signals:
    void activationRefused(const QString &networkId,
                           const QString &activeNetworkId);

private:
    bool isLive(const IrcSession *session) const;

    QHash<QString, IrcSession *> m_sessions;
    IrcSession *m_activeSession = nullptr;
};
