#include "ircsessionmanager.h"

IrcSessionManager::IrcSessionManager(QObject *parent)
    : QObject(parent)
{
}

IrcSession *IrcSessionManager::createSession(const IrcSessionConfig &config,
                                             IrcTransport *transport,
                                             IrcReconnectTimer *reconnectTimer)
{
    if (!transport || config.networkId.isEmpty()
        || m_sessions.contains(config.networkId)) {
        return nullptr;
    }

    auto *session = new IrcSession(config, transport, reconnectTimer, nullptr, this);
    m_sessions.insert(config.networkId, session);
    connect(session, &QObject::destroyed, this,
            [this, session, networkId = config.networkId] {
        if (m_sessions.value(networkId) == session)
            m_sessions.remove(networkId);
    });
    return session;
}

IrcSession *IrcSessionManager::findSession(const QString &networkId) const
{
    return m_sessions.value(networkId, nullptr);
}

QStringList IrcSessionManager::networkIds() const
{
    QStringList ids = m_sessions.keys();
    ids.sort();
    return ids;
}

bool IrcSessionManager::activateSession(const QString &networkId)
{
    IrcSession *session = findSession(networkId);
    if (!session)
        return false;
    session->start();
    return isLive(session);
}

bool IrcSessionManager::stopSession(const QString &networkId)
{
    IrcSession *session = findSession(networkId);
    if (!session)
        return false;
    session->stop();
    return true;
}

bool IrcSessionManager::discardSession(const QString &networkId)
{
    IrcSession *session = findSession(networkId);
    if (!session)
        return false;
    session->stop();
    m_sessions.remove(networkId);
    session->deleteLater();
    return true;
}

bool IrcSessionManager::isLive(const IrcSession *session) const
{
    return session->state() != IrcSession::State::Idle
        && session->state() != IrcSession::State::Failed;
}
