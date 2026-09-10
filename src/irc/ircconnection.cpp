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

IrcConnection::IrcConnection(IrcController &controller,
                             CredentialStore &credentialStore,
                             QObject *parent)
    : IrcConnection(controller, defaultTransport, credentialStore, parent)
{
}

IrcConnection::IrcConnection(IrcController &controller,
                             TransportFactory transportFactory,
                             CredentialStore &credentialStore,
                             QObject *parent)
    : QObject(parent)
    , m_controller(controller)
    , m_transportFactory(std::move(transportFactory))
    , m_credentialStore(credentialStore)
{
    if (!m_transportFactory)
        m_transportFactory = defaultTransport;
    connect(&m_credentialStore, &CredentialStore::readFinished, this,
                [this](CredentialStore::State state, const QString &password,
                       const QString &message) {
            handleCredentialRead(state, password, message);
        });
    connect(&m_credentialStore, &CredentialStore::writeFinished, this,
                [this](CredentialStore::State state, const QString &message) {
            if (m_credentialOperations.isEmpty())
                return;
            const CredentialOperation operation = m_credentialOperations.takeFirst();
            const CredentialKey currentKey = credentialKey(m_stored);
            const bool removingObsoleteKey =
                operation.kind == CredentialOperation::Kind::Remove
                && operation.key != currentKey;
            if (removingObsoleteKey) {
                if (state != CredentialStore::State::Missing
                    && state != CredentialStore::State::Available) {
                    rememberObsoleteKey(operation.key);
                    m_obsoleteRemovalError = message;
                } else if (m_obsoleteKeys.isEmpty()) {
                    m_obsoleteRemovalError.clear();
                }
            } else if (operation.revision == m_secretRevision) {
                adoptBackendState(state, message);
                if (state == CredentialStore::State::Available) {
                    m_passwordEdited = false;
                } else if (operation.kind == CredentialOperation::Kind::Write) {
                    m_passwordEdited = true;
                }
                if (operation.kind == CredentialOperation::Kind::Write
                    && state == CredentialStore::State::Available) {
                    m_secretMayBeStored = true;
                    m_persistedPassword = operation.password;
                } else if (operation.kind == CredentialOperation::Kind::Remove
                           && state == CredentialStore::State::Missing) {
                    m_secretMayBeStored = false;
                    m_persistedPassword.clear();
                }
            }
            if ((operation.kind == CredentialOperation::Kind::Write
                 && state == CredentialStore::State::Available)
                || (operation.kind == CredentialOperation::Kind::Remove
                    && !removingObsoleteKey
                    && state == CredentialStore::State::Missing)) {
                flushObsoleteKeys(operation.revision);
            }
            processCredentialOperations();
            emit credentialStateChanged();
        });
    m_stored = firstStoredProfile(m_store.profiles());
    if (m_stored.networkId.isEmpty())
        m_draft = IrcNetworkProfile::suggested();
    else
        m_draft = m_stored;
    if (!m_stored.networkId.isEmpty()) {
        m_credentialReadInFlight = true;
        m_credentialReadRevision = m_secretRevision;
        m_credentialStore.read(credentialKey(m_stored));
    }

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

CredentialStore::State IrcConnection::credentialState() const
{
    return m_credentialState;
}

QString IrcConnection::credentialError() const
{
    if (!m_obsoleteRemovalError.isEmpty())
        return m_obsoleteRemovalError;
    return m_credentialError;
}

QString IrcConnection::credentialStatus() const
{
    if (!m_obsoleteKeys.isEmpty() && !m_obsoleteRemovalError.isEmpty())
        return QStringLiteral("could not remove the previous saved password");
    if (m_passwordEdited && !m_password.isEmpty()
        && (m_credentialState == CredentialStore::State::Available
            || m_credentialState == CredentialStore::State::Missing)) {
        return QStringLiteral("password changed; apply to save securely");
    }
    switch (m_credentialState) {
    case CredentialStore::State::Loading:
        return QStringLiteral("checking secure storage");
    case CredentialStore::State::Available:
        return QStringLiteral("password saved securely");
    case CredentialStore::State::Missing:
        if (!m_password.isEmpty())
            return QStringLiteral("password is session-only until applied");
        return {};
    case CredentialStore::State::Unavailable:
        if (!m_password.isEmpty())
            return QStringLiteral("secure storage unavailable; password is session-only");
        return QStringLiteral("secure storage unavailable");
    case CredentialStore::State::Error:
        if (!m_password.isEmpty())
            return QStringLiteral("secure storage error; password is session-only");
        return QStringLiteral("secure storage error; password is not saved");
    case CredentialStore::State::SessionOnly:
        return QStringLiteral("secure storage unavailable; password is session-only");
    }
    return {};
}

bool IrcConnection::canForgetPassword() const
{
    return m_secretMayBeStored || !m_password.isEmpty();
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
    if (password.isEmpty() && !m_password.isEmpty())
        return;
    if (m_password == password)
        return;
    m_password = password;
    m_passwordEdited = true;
    if (m_credentialState == CredentialStore::State::Unavailable) {
        m_credentialState = CredentialStore::State::SessionOnly;
    } else if (password.isEmpty()) {
        m_credentialState = CredentialStore::State::Missing;
    }
    ++m_secretRevision;
    if (m_focusPassword) {
        m_focusPassword = false;
        emit focusPasswordChanged();
    }
    emit credentialStateChanged();
    emit draftChanged();
}

void IrcConnection::forgetPassword()
{
    if (m_password.isEmpty() && m_credentialState == CredentialStore::State::Missing)
        return;
    m_password.clear();
    m_passwordEdited = true;
    m_credentialState = CredentialStore::State::Missing;
    ++m_secretRevision;
    emit credentialStateChanged();
    emit draftChanged();
}

void IrcConnection::removeStoredPassword()
{
    if (m_stored.networkId.isEmpty())
        return;
    queueCredentialRemoval(credentialKey(m_stored), m_secretRevision);
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

    const CredentialKey previousCredentialKey = credentialKey(m_stored);
    const CredentialKey nextCredentialKey = credentialKey(profile);
    m_store.save(profile);
    m_stored = profile;
    m_draft = profile;
    emit draftChanged();
    const bool credentialKeyChanged = previousCredentialKey != nextCredentialKey;
    if (credentialKeyChanged) {
        rememberObsoleteKey(previousCredentialKey);
        m_secretMayBeStored = false;
    }
    if (!m_password.isEmpty()
        && (m_passwordEdited || credentialKeyChanged || !m_secretMayBeStored)) {
        queueCredentialWrite(nextCredentialKey, m_password, m_secretRevision);
    } else if (m_passwordEdited && m_password.isEmpty()) {
        queueCredentialRemoval(nextCredentialKey, m_secretRevision);
    } else if (!m_obsoleteKeys.isEmpty() && !m_credentialReadInFlight) {
        flushObsoleteKeys(m_secretRevision);
        processCredentialOperations();
    }
    if (wasSetup)
        emit setupRequiredChanged();
    if (m_credentialReadInFlight && !m_passwordEdited) {
        m_reconcileWhenReadSettles = true;
        return true;
    }
    return reconcile(profile);
}

void IrcConnection::processCredentialOperations()
{
    if (m_credentialOperations.isEmpty())
        return;
    const CredentialOperation &operation = m_credentialOperations.first();
    if (operation.kind == CredentialOperation::Kind::Write)
        m_credentialStore.write(operation.key, operation.password);
    else
        m_credentialStore.remove(operation.key);
}

void IrcConnection::rememberObsoleteKey(const CredentialKey &key)
{
    if (key.networkId.isEmpty() || m_obsoleteKeys.contains(key))
        return;
    m_obsoleteKeys.append(key);
}

CredentialStore::State IrcConnection::overlayState(
    CredentialStore::State backend) const
{
    if (backend == CredentialStore::State::Unavailable && !m_password.isEmpty())
        return CredentialStore::State::SessionOnly;
    return backend;
}

void IrcConnection::adoptBackendState(CredentialStore::State state,
                                      const QString &message)
{
    m_credentialError = message;
    if (state == CredentialStore::State::Loading) {
        m_credentialState = state;
        return;
    }
    m_backendState = state;
    m_credentialState = overlayState(m_backendState);
}

void IrcConnection::handleCredentialRead(CredentialStore::State state,
                                         const QString &password,
                                         const QString &message)
{
    if (state == CredentialStore::State::Loading) {
        adoptBackendState(state, message);
        emit credentialStateChanged();
        emit draftChanged();
        return;
    }
    m_credentialReadInFlight = false;
    const bool stale = m_secretRevision != m_credentialReadRevision;
    if (stale && m_passwordEdited) {
        const bool keepWriteResult =
            m_backendState == CredentialStore::State::Error
            || m_backendState == CredentialStore::State::Unavailable;
        if (!keepWriteResult) {
            const auto terminal =
                (state == CredentialStore::State::Available
                 || state == CredentialStore::State::Missing)
                ? CredentialStore::State::Missing : state;
            adoptBackendState(terminal, message);
        }
    } else if (stale && !m_persistedPassword.isEmpty()) {
        if (m_restoreStoreAfterRead) {
            m_restoreStoreAfterRead = false;
            compensatePersistedSecret();
        }
    } else {
        settleCredentialRead(state, password, message);
    }
    emit credentialStateChanged();
    emit draftChanged();
    if (m_reconcileWhenReadSettles) {
        m_reconcileWhenReadSettles = false;
        reconcile(m_stored);
    }
}

void IrcConnection::settleCredentialRead(CredentialStore::State state,
                                         const QString &password,
                                         const QString &message)
{
    adoptBackendState(state, message);
    if (state == CredentialStore::State::Available && !m_passwordEdited) {
        m_password = password;
        m_persistedPassword = password;
        m_secretMayBeStored = true;
        ++m_secretRevision;
        if (!m_obsoleteKeys.isEmpty())
            queueCredentialWrite(credentialKey(m_stored), m_password,
                                 m_secretRevision);
    } else if (state == CredentialStore::State::Missing) {
        m_secretMayBeStored = false;
    }
    if (m_restoreStoreAfterRead) {
        m_restoreStoreAfterRead = false;
        compensatePersistedSecret();
    }
}

void IrcConnection::compensatePersistedSecret()
{
    const CredentialKey key = credentialKey(m_stored);
    if (m_password.isEmpty())
        queueCredentialRemoval(key, m_secretRevision);
    else
        queueCredentialWrite(key, m_password, m_secretRevision);
}

void IrcConnection::flushObsoleteKeys(quint64 revision)
{
    const CredentialKey currentKey = credentialKey(m_stored);
    const QList<CredentialKey> obsolete = m_obsoleteKeys;
    m_obsoleteKeys.clear();
    for (const CredentialKey &key : obsolete) {
        if (key == currentKey)
            continue;
        m_credentialOperations.append(
            {CredentialOperation::Kind::Remove, key, {}, revision});
    }
}

void IrcConnection::queueCredentialWrite(
    const CredentialKey &key, const QString &password, quint64 revision)
{
    const bool wasEmpty = m_credentialOperations.isEmpty();
    m_credentialOperations.append(
        {CredentialOperation::Kind::Write, key, password, revision});
    if (wasEmpty)
        processCredentialOperations();
}

void IrcConnection::queueCredentialRemoval(const CredentialKey &key, quint64 revision)
{
    const bool wasEmpty = m_credentialOperations.isEmpty();
    m_credentialOperations.append(
        {CredentialOperation::Kind::Remove, key, {}, revision});
    if (wasEmpty)
        processCredentialOperations();
}

void IrcConnection::discard()
{
    restoreDraft();
    if (m_password != m_persistedPassword || m_passwordEdited) {
        const bool compensatePendingStore = !m_credentialOperations.isEmpty();
        m_password = m_persistedPassword;
        m_passwordEdited = false;
        m_credentialState = overlayState(m_backendState);
        ++m_secretRevision;
        if (compensatePendingStore) {
            if (m_credentialReadInFlight)
                m_restoreStoreAfterRead = true;
            else
                compensatePersistedSecret();
        }
        emit credentialStateChanged();
    }
    emit draftChanged();
}

bool IrcConnection::activate()
{
    if (m_startupActivationConnection) {
        QObject::disconnect(m_startupActivationConnection);
        m_startupActivationConnection = {};
    }
    if (!m_stored.isComplete())
        return false;
    m_draft = m_stored;
    emit draftChanged();
    return reconcile(m_stored);
}

void IrcConnection::activateOnStartup()
{
    if (m_startupActivationConnection)
        return;
    if (m_credentialState == CredentialStore::State::Loading) {
        m_startupActivationConnection = connect(
            this, &IrcConnection::credentialStateChanged, this, [this]() {
                if (m_credentialState == CredentialStore::State::Available
                    || m_credentialState == CredentialStore::State::Missing
                    || m_credentialState == CredentialStore::State::Unavailable
                    || m_credentialState == CredentialStore::State::SessionOnly) {
                    activate();
                } else if (m_credentialState == CredentialStore::State::Error) {
                    QObject::disconnect(m_startupActivationConnection);
                    m_startupActivationConnection = {};
                }
            });
        return;
    }
    if (m_credentialState == CredentialStore::State::Error)
        return;
    activate();
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

CredentialKey IrcConnection::credentialKey(const IrcNetworkProfile &profile) const
{
    return {profile.networkId,
            profile.username.isEmpty() ? profile.nick : profile.username,
            profile.host};
}
