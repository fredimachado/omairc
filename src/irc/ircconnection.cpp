#include "ircconnection.h"

#include "irccontroller.h"
#include "qtirctransport.h"

namespace
{
IrcTransport *defaultTransport()
{
    return new QtIrcTransport;
}

IrcNetworkProfile firstStoredProfile(const QList<IrcNetworkProfile> &profiles)
{
    for (const IrcNetworkProfile &profile : profiles) {
        if (profile.isComplete())
            return profile;
    }
    if (!profiles.isEmpty())
        return profiles.first();
    return {};
}
}

IrcConnection::IrcConnection(IrcController &controller, QObject *parent)
    : IrcConnection(controller, defaultTransport, parent)
{
}

IrcConnection::IrcConnection(IrcController &controller,
                             TransportFactory transportFactory,
                             QObject *parent)
    : QObject(parent)
    , m_controller(controller)
    , m_transportFactory(std::move(transportFactory))
{
    if (!m_transportFactory)
        m_transportFactory = defaultTransport;

    m_stored = firstStoredProfile(m_store.profiles());
    if (m_stored.networkId.isEmpty())
        m_draft = IrcNetworkProfile::suggested();
    else
        m_draft = m_stored;

    connect(&m_controller, &IrcController::errorOccurred, this,
            [this](const QString &, IrcSession::ErrorKind kind, const QString &) {
        if (kind != IrcSession::ErrorKind::Authentication || m_focusPassword)
            return;
        m_focusPassword = true;
        emit focusPasswordChanged();
    });
}

QString IrcConnection::host() const
{
    return m_draft.host;
}

int IrcConnection::port() const
{
    return int(m_draft.port);
}

bool IrcConnection::tlsEnabled() const
{
    return m_draft.tlsEnabled;
}

bool IrcConnection::connectOnStartup() const
{
    return m_draft.connectOnStartup;
}

QString IrcConnection::nick() const
{
    return m_draft.nick;
}

QString IrcConnection::username() const
{
    return m_draft.username;
}

QString IrcConnection::realname() const
{
    return m_draft.realname;
}

QString IrcConnection::autojoin() const
{
    return m_draft.autojoinChannels.join(QLatin1Char(' '));
}

bool IrcConnection::passwordSet() const
{
    return !m_password.isEmpty();
}

QString IrcConnection::problem() const
{
    return IrcNetworkProfile::problemText(m_draft.validate());
}

bool IrcConnection::dirty() const
{
    return m_draft.normalized() != m_stored.normalized();
}

QString IrcConnection::displayName() const
{
    const QString host = m_draft.host.trimmed();
    if (!host.isEmpty())
        return host;
    const QString storedHost = m_stored.host.trimmed();
    if (!storedHost.isEmpty())
        return storedHost;
    return QStringLiteral("Omarchy IRC");
}

bool IrcConnection::setupRequired() const
{
    return !m_stored.isComplete();
}

bool IrcConnection::focusPassword() const
{
    return m_focusPassword;
}

void IrcConnection::setHost(const QString &host)
{
    if (m_draft.host == host)
        return;
    m_draft.host = host;
    emit draftChanged();
}

void IrcConnection::setPort(int port)
{
    const int bounded = qBound(0, port, 65535);
    const quint16 next = quint16(bounded);
    if (m_draft.port == next)
        return;
    m_draft.port = next;
    emit draftChanged();
}

void IrcConnection::setTlsEnabled(bool enabled)
{
    if (m_draft.tlsEnabled == enabled)
        return;
    m_draft.tlsEnabled = enabled;
    emit draftChanged();
}

void IrcConnection::setConnectOnStartup(bool enabled)
{
    if (m_draft.connectOnStartup == enabled)
        return;
    m_draft.connectOnStartup = enabled;
    emit draftChanged();
}

void IrcConnection::setNick(const QString &nick)
{
    if (m_draft.nick == nick)
        return;
    m_draft.nick = nick;
    emit draftChanged();
}

void IrcConnection::setUsername(const QString &username)
{
    if (m_draft.username == username)
        return;
    m_draft.username = username;
    emit draftChanged();
}

void IrcConnection::setRealname(const QString &realname)
{
    if (m_draft.realname == realname)
        return;
    m_draft.realname = realname;
    emit draftChanged();
}

void IrcConnection::setAutojoin(const QString &channels)
{
    const QStringList next = IrcNetworkProfile::parseAutojoin(channels);
    if (m_draft.autojoinChannels == next)
        return;
    m_draft.autojoinChannels = next;
    emit draftChanged();
}

void IrcConnection::setPassword(const QString &password)
{
    if (m_password == password)
        return;
    m_password = password;
    ++m_secretRevision;
    if (m_focusPassword) {
        m_focusPassword = false;
        emit focusPasswordChanged();
    }
    emit draftChanged();
}

bool IrcConnection::apply()
{
    const bool wasSetup = setupRequired();
    const IrcNetworkProfile profile = m_draft.normalized();
    if (!profile.isComplete()) {
        m_draft = profile;
        emit draftChanged();
        return false;
    }

    m_store.save(profile);
    m_stored = profile;
    m_draft = profile;
    emit draftChanged();
    if (wasSetup)
        emit setupRequiredChanged();
    return reconcile(profile);
}

void IrcConnection::discard()
{
    restoreDraft();
    emit draftChanged();
}

bool IrcConnection::activate()
{
    if (!m_stored.isComplete())
        return false;
    m_draft = m_stored;
    emit draftChanged();
    return reconcile(m_stored);
}

void IrcConnection::restoreDraft()
{
    if (m_stored.networkId.isEmpty()) {
        const QString id = m_draft.networkId;
        m_draft = IrcNetworkProfile::suggested();
        if (!id.isEmpty())
            m_draft.networkId = id;
        return;
    }
    m_draft = m_stored;
}

std::optional<IrcSessionConfig> IrcConnection::sessionConfigFor(
    const IrcNetworkProfile &profile) const
{
    if (!profile.isComplete())
        return std::nullopt;

    IrcSessionConfig config;
    config.networkId = profile.networkId;
    config.host = profile.host;
    config.port = profile.port;
    config.tlsEnabled = profile.tlsEnabled;
    config.nick = profile.nick;
    config.username = profile.username.isEmpty() ? profile.nick : profile.username;
    config.realname = profile.realname.isEmpty() ? profile.nick : profile.realname;
    config.password = m_password;
    config.autojoinChannels = profile.autojoinChannels;
    return config;
}

bool IrcConnection::reconcile(const IrcNetworkProfile &profile)
{
    const Applied candidate{profile, m_secretRevision};
    if (m_applied && m_applied->profile == candidate.profile
        && m_applied->secretRevision == candidate.secretRevision) {
        return m_controller.start(profile.networkId);
    }

    m_controller.discardSession(profile.networkId);
    const std::optional<IrcSessionConfig> config = sessionConfigFor(profile);
    if (!config || !m_transportFactory)
        return false;

    IrcTransport *transport = m_transportFactory();
    if (!transport)
        return false;

    if (!m_controller.addSession(*config, transport))
        return false;

    m_applied = candidate;
    return m_controller.start(profile.networkId);
}
