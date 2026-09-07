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
    connect(session, &IrcSession::stateChanged, this, [this, session](IrcSession::State state) {
        if (m_activeSession == session
            && (state == IrcSession::State::Idle || state == IrcSession::State::Failed)) {
            m_activeSession = nullptr;
        }
    });
    connect(session, &QObject::destroyed, this, [this, session, networkId = config.networkId] {
        m_sessions.remove(networkId);
        if (m_activeSession == session)
            m_activeSession = nullptr;
    });
    return session;
}

IrcSession *IrcSessionManager::findSession(const QString &networkId) const
{
    return m_sessions.value(networkId, nullptr);
}

QStringList IrcSessionManager::networkIds() const
{
    return m_sessions.keys();
}

bool IrcSessionManager::activateSession(const QString &networkId)
{
    IrcSession *session = findSession(networkId);
    if (!session)
        return false;
    if (m_activeSession && m_activeSession != session && isLive(m_activeSession)) {
        emit activationRefused(networkId, m_activeSession->networkId());
        return false;
    }

    m_activeSession = session;
    session->start();
    return isLive(session);
}

bool IrcSessionManager::stopSession(const QString &networkId)
{
    IrcSession *session = findSession(networkId);
    if (!session)
        return false;
    session->stop();
    if (m_activeSession == session)
        m_activeSession = nullptr;
    return true;
}

bool IrcSessionManager::discardSession(const QString &networkId)
{
    IrcSession *session = findSession(networkId);
    if (!session)
        return false;
    session->stop();
    m_sessions.remove(networkId);
    if (m_activeSession == session)
        m_activeSession = nullptr;
    session->deleteLater();
    return true;
}

QString IrcSessionManager::activeNetworkId() const
{
    return m_activeSession ? m_activeSession->networkId() : QString{};
}

bool IrcSessionManager::isLive(const IrcSession *session) const
{
    return session->state() != IrcSession::State::Idle
        && session->state() != IrcSession::State::Failed;
}
