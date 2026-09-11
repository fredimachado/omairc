#include "ircconnection.h"

#include "irccontroller.h"
#include "qtirctransport.h"

#include <algorithm>

namespace
{
IrcTransport *defaultTransport()
{
    return new QtIrcTransport;
}

bool profileLess(const IrcNetworkProfile &left, const IrcNetworkProfile &right)
{
    const int host = QString::compare(left.host.trimmed(), right.host.trimmed(),
                                      Qt::CaseInsensitive);
    if (host != 0)
        return host < 0;
    const int nick = QString::compare(left.nick.trimmed(), right.nick.trimmed(),
                                      Qt::CaseInsensitive);
    if (nick != 0)
        return nick < 0;
    return left.networkId < right.networkId;
}
}

NetworkListModel::NetworkListModel(IrcConnection &owner, QObject *parent)
    : QAbstractListModel(parent)
    , m_owner(owner)
{
}

int NetworkListModel::rowCount(const QModelIndex &parent) const
{
    return parent.isValid() ? 0 : m_owner.rosterRows().size();
}

QVariant NetworkListModel::data(const QModelIndex &index, int role) const
{
    const QVector<IrcConnection::RosterRow> rows = m_owner.rosterRows();
    if (!index.isValid() || index.row() < 0 || index.row() >= rows.size())
        return {};
    const IrcConnection::RosterRow &row = rows.at(index.row());
    switch (role) {
    case NetworkIdRole:
        return row.networkId;
    case DisplayNameRole:
    case Qt::DisplayRole:
        return row.displayName;
    case StoredRole:
        return row.stored;
    case SelectedRole:
        return row.selected;
    default:
        return {};
    }
}

QHash<int, QByteArray> NetworkListModel::roleNames() const
{
    return {
        {NetworkIdRole, "networkId"},
        {DisplayNameRole, "displayName"},
        {StoredRole, "stored"},
        {SelectedRole, "selected"},
    };
}

void NetworkListModel::resetRows()
{
    beginResetModel();
    endResetModel();
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
    , m_networks(*this)
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
        handleCredentialWrite(state, message);
    });

    loadStored();
    if (!m_stored.isEmpty()) {
        IrcNetworkProfile chosen = m_stored.first();
        for (const IrcNetworkProfile &profile : m_stored) {
            if (profile.isComplete()) {
                chosen = profile;
                break;
            }
        }
        m_selectedNetworkId = chosen.networkId;
        m_draft = chosen;
    } else {
        m_draft = IrcNetworkProfile::suggested();
        m_selectedNetworkId = m_draft.networkId;
    }
    pushNetworkOrder();

    for (const IrcNetworkProfile &profile : m_stored) {
        if (profile.networkId.isEmpty())
            continue;
        startCredentialRead(profile);
    }

    connect(&m_controller, &IrcController::errorOccurred, this,
            [this](const QString &networkId, IrcSession::ErrorKind kind, const QString &) {
        if (kind != IrcSession::ErrorKind::Authentication)
            return;
        if (!networkId.isEmpty() && networkId != m_selectedNetworkId) {
            if (dirty())
                return;
            selectStored(networkId);
        }
        if (m_focusPassword)
            return;
        m_focusPassword = true;
        emit focusPasswordChanged();
    });
}

QAbstractItemModel *IrcConnection::networks()
{
    return &m_networks;
}

QString IrcConnection::selectedNetworkId() const
{
    return m_selectedNetworkId;
}

bool IrcConnection::canAdd() const
{
    return !setupRequired() && isStored(m_selectedNetworkId) && !dirty();
}

bool IrcConnection::canRemove() const
{
    return isStored(m_selectedNetworkId);
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
    return !selectedSecret().password.isEmpty();
}

CredentialStore::State IrcConnection::credentialState() const
{
    return selectedSecret().credentialState;
}

QString IrcConnection::credentialError() const
{
    const IrcDraftSecret &secret = selectedSecret();
    if (!secret.obsoleteRemovalError.isEmpty())
        return secret.obsoleteRemovalError;
    return secret.credentialError;
}

QString IrcConnection::credentialStatus() const
{
    const IrcDraftSecret &secret = selectedSecret();
    if (!secret.obsoleteKeys.isEmpty() && !secret.obsoleteRemovalError.isEmpty())
        return QStringLiteral("could not remove the previous saved password");
    if (secret.edited && !secret.password.isEmpty()
        && (secret.credentialState == CredentialStore::State::Available
            || secret.credentialState == CredentialStore::State::Missing)) {
        return QStringLiteral("password changed; apply to save securely");
    }
    switch (secret.credentialState) {
    case CredentialStore::State::Loading:
        return QStringLiteral("checking secure storage");
    case CredentialStore::State::Available:
        return QStringLiteral("password saved securely");
    case CredentialStore::State::Missing:
        if (!secret.password.isEmpty())
            return QStringLiteral("password is session-only until applied");
        return {};
    case CredentialStore::State::Unavailable:
        if (!secret.password.isEmpty())
            return QStringLiteral("secure storage unavailable; password is session-only");
        return QStringLiteral("secure storage unavailable");
    case CredentialStore::State::Error:
        if (!secret.password.isEmpty())
            return QStringLiteral("secure storage error; password is session-only");
        return QStringLiteral("secure storage error; password is not saved");
    case CredentialStore::State::SessionOnly:
        return QStringLiteral("secure storage unavailable; password is session-only");
    }
    return {};
}

bool IrcConnection::canForgetPassword() const
{
    const IrcDraftSecret &secret = selectedSecret();
    return secret.mayBeStored || !secret.password.isEmpty();
}

QString IrcConnection::problem() const
{
    return IrcNetworkProfile::problemText(m_draft.validate());
}

bool IrcConnection::dirty() const
{
    if (!isStored(m_selectedNetworkId))
        return true;
    return m_draft.normalized() != storedProfile(m_selectedNetworkId).normalized();
}

QString IrcConnection::displayName() const
{
    return rosterDisplayName(m_draft);
}

bool IrcConnection::setupRequired() const
{
    for (const IrcNetworkProfile &profile : m_stored) {
        if (profile.isComplete())
            return false;
    }
    return true;
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
    refreshRoster();
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
    refreshRoster();
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

void IrcConnection::select(const QString &networkId)
{
    if (networkId.isEmpty() || networkId == m_selectedNetworkId)
        return;
    if (dirty())
        return;
    if (!isStored(networkId) && networkId != m_draft.networkId)
        return;
    selectStored(networkId);
}

bool IrcConnection::add()
{
    if (!canAdd())
        return false;
    m_draft = IrcNetworkProfile::create();
    m_draft.port = 6697;
    m_draft.tlsEnabled = true;
    m_selectedNetworkId = m_draft.networkId;
    if (m_focusPassword) {
        m_focusPassword = false;
        emit focusPasswordChanged();
    }
    emit selectedNetworkChanged();
    emit credentialStateChanged();
    emit draftChanged();
    refreshRoster();
    return true;
}

void IrcConnection::setPassword(const QString &password)
{
    IrcDraftSecret &secret = selectedSecret();
    if (password.isEmpty() && !secret.password.isEmpty())
        return;
    if (secret.password == password)
        return;
    secret.password = password;
    secret.edited = true;
    if (secret.credentialState == CredentialStore::State::Unavailable) {
        secret.credentialState = CredentialStore::State::SessionOnly;
    } else if (password.isEmpty()) {
        secret.credentialState = CredentialStore::State::Missing;
    }
    ++secret.revision;
    if (m_focusPassword) {
        m_focusPassword = false;
        emit focusPasswordChanged();
    }
    emit credentialStateChanged();
    emit draftChanged();
}

void IrcConnection::forgetPassword()
{
    IrcDraftSecret &secret = selectedSecret();
    if (secret.password.isEmpty()
        && secret.credentialState == CredentialStore::State::Missing)
        return;
    secret.password.clear();
    secret.edited = true;
    secret.credentialState = CredentialStore::State::Missing;
    ++secret.revision;
    emit credentialStateChanged();
    emit draftChanged();
}

void IrcConnection::removeStoredPassword()
{
    const IrcNetworkProfile stored = storedProfile(m_selectedNetworkId);
    if (stored.networkId.isEmpty())
        return;
    queueCredentialRemoval(credentialKey(stored), selectedSecret().revision);
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

    const CredentialKey previousCredentialKey =
        credentialKey(storedProfile(m_selectedNetworkId));
    const CredentialKey nextCredentialKey = credentialKey(profile);
    m_store.save(profile);
    bool found = false;
    for (IrcNetworkProfile &stored : m_stored) {
        if (stored.networkId == profile.networkId) {
            stored = profile;
            found = true;
            break;
        }
    }
    if (!found)
        m_stored.append(profile);
    sortStored();
    m_draft = profile;
    m_selectedNetworkId = profile.networkId;
    emit draftChanged();
    emit selectedNetworkChanged();

    IrcDraftSecret &secret = secretFor(profile.networkId);
    const bool credentialKeyChanged = previousCredentialKey != nextCredentialKey;
    if (credentialKeyChanged) {
        rememberObsoleteKey(secret, previousCredentialKey);
        secret.mayBeStored = false;
    }
    if (!secret.password.isEmpty()
        && (secret.edited || credentialKeyChanged || !secret.mayBeStored)) {
        queueCredentialWrite(nextCredentialKey, secret.password, secret.revision);
    } else if (secret.edited && secret.password.isEmpty()) {
        queueCredentialRemoval(nextCredentialKey, secret.revision);
    } else if (!secret.obsoleteKeys.isEmpty() && !secret.readInFlight) {
        flushObsoleteKeys(secret, nextCredentialKey, secret.revision);
        processCredentialOperations();
    }
    if (wasSetup)
        emit setupRequiredChanged();
    refreshRoster();
    pushNetworkOrder();
    if (secret.readInFlight && !secret.edited) {
        secret.reconcileWhenReadSettles = true;
        return true;
    }
    return reconcile(profile);
}

void IrcConnection::processCredentialOperations()
{
    if (m_operationInFlight || m_credentialOperations.isEmpty())
        return;
    m_operationInFlight = true;
    const CredentialOperation &operation = m_credentialOperations.first();
    if (operation.kind == CredentialOperation::Kind::Write)
        m_credentialStore.write(operation.key, operation.password);
    else
        m_credentialStore.remove(operation.key);
}

void IrcConnection::rememberObsoleteKey(IrcDraftSecret &secret, const CredentialKey &key)
{
    if (key.networkId.isEmpty() || secret.obsoleteKeys.contains(key))
        return;
    secret.obsoleteKeys.append(key);
}

CredentialStore::State IrcConnection::overlayState(
    CredentialStore::State backend, const QString &password) const
{
    if (backend == CredentialStore::State::Unavailable && !password.isEmpty())
        return CredentialStore::State::SessionOnly;
    return backend;
}

void IrcConnection::adoptBackendState(IrcDraftSecret &secret,
                                      CredentialStore::State state,
                                      const QString &message)
{
    secret.credentialError = message;
    if (state == CredentialStore::State::Loading) {
        secret.credentialState = state;
        return;
    }
    secret.backendState = state;
    secret.credentialState = overlayState(secret.backendState, secret.password);
}

void IrcConnection::handleCredentialRead(CredentialStore::State state,
                                         const QString &password,
                                         const QString &message)
{
    if (m_pendingReads.isEmpty())
        return;

    const CredentialKey key = m_pendingReads.first();
    IrcDraftSecret &secret = secretFor(key.networkId);
    const IrcNetworkProfile profile = storedProfile(key.networkId);
    const bool selected = key.networkId == m_selectedNetworkId;
    if (state == CredentialStore::State::Loading) {
        adoptBackendState(secret, state, message);
        emit credentialStateChanged();
        if (selected)
            emit draftChanged();
        return;
    }

    m_pendingReads.takeFirst();
    secret.readInFlight = false;
    const bool stale = secret.revision != secret.readRevision;
    if (stale && secret.edited) {
        const bool keepWriteResult =
            secret.backendState == CredentialStore::State::Error
            || secret.backendState == CredentialStore::State::Unavailable;
        if (!keepWriteResult) {
            const auto terminal =
                (state == CredentialStore::State::Available
                 || state == CredentialStore::State::Missing)
                ? CredentialStore::State::Missing : state;
            adoptBackendState(secret, terminal, message);
        }
    } else if (stale && !secret.persistedPassword.isEmpty()) {
        if (secret.restoreStoreAfterRead) {
            secret.restoreStoreAfterRead = false;
            compensatePersistedSecret(secret, profile);
        }
    } else {
        settleCredentialRead(secret, profile, state, password, message);
    }
    emit credentialStateChanged();
    if (selected)
        emit draftChanged();
    if (secret.reconcileWhenReadSettles) {
        secret.reconcileWhenReadSettles = false;
        if (profile.isComplete())
            reconcile(profile);
    }
    if (!m_pendingReads.isEmpty())
        m_credentialStore.read(m_pendingReads.first());
}

void IrcConnection::handleCredentialWrite(CredentialStore::State state,
                                          const QString &message)
{
    if (m_credentialOperations.isEmpty())
        return;
    const CredentialOperation operation = m_credentialOperations.takeFirst();
    m_operationInFlight = false;
    IrcDraftSecret &secret = secretFor(operation.key.networkId);
    const CredentialKey currentKey =
        credentialKey(storedProfile(operation.key.networkId));
    const bool removingObsoleteKey =
        operation.kind == CredentialOperation::Kind::Remove
        && operation.key != currentKey;
    if (removingObsoleteKey) {
        if (state != CredentialStore::State::Missing
            && state != CredentialStore::State::Available) {
            rememberObsoleteKey(secret, operation.key);
            secret.obsoleteRemovalError = message;
        } else if (secret.obsoleteKeys.isEmpty()) {
            secret.obsoleteRemovalError.clear();
        }
    } else if (operation.revision == secret.revision) {
        adoptBackendState(secret, state, message);
        if (state == CredentialStore::State::Available) {
            secret.edited = false;
        } else if (operation.kind == CredentialOperation::Kind::Write) {
            secret.edited = true;
        }
        if (operation.kind == CredentialOperation::Kind::Write
            && state == CredentialStore::State::Available) {
            secret.mayBeStored = true;
            secret.persistedPassword = operation.password;
        } else if (operation.kind == CredentialOperation::Kind::Remove
                   && state == CredentialStore::State::Missing) {
            secret.mayBeStored = false;
            secret.persistedPassword.clear();
        }
    }
    if ((operation.kind == CredentialOperation::Kind::Write
         && state == CredentialStore::State::Available)
        || (operation.kind == CredentialOperation::Kind::Remove
            && !removingObsoleteKey
            && state == CredentialStore::State::Missing)) {
        flushObsoleteKeys(secret, currentKey, operation.revision);
    }
    processCredentialOperations();
    emit credentialStateChanged();
}

void IrcConnection::settleCredentialRead(IrcDraftSecret &secret,
                                         const IrcNetworkProfile &profile,
                                         CredentialStore::State state,
                                         const QString &password,
                                         const QString &message)
{
    adoptBackendState(secret, state, message);
    if (state == CredentialStore::State::Available && !secret.edited) {
        secret.password = password;
        secret.persistedPassword = password;
        secret.mayBeStored = true;
        ++secret.revision;
        if (!secret.obsoleteKeys.isEmpty())
            queueCredentialWrite(credentialKey(profile), secret.password,
                                 secret.revision);
    } else if (state == CredentialStore::State::Missing) {
        secret.mayBeStored = false;
    }
    if (secret.restoreStoreAfterRead) {
        secret.restoreStoreAfterRead = false;
        compensatePersistedSecret(secret, profile);
    }
}

void IrcConnection::compensatePersistedSecret(IrcDraftSecret &secret,
                                             const IrcNetworkProfile &profile)
{
    const CredentialKey key = credentialKey(profile);
    if (secret.password.isEmpty())
        queueCredentialRemoval(key, secret.revision);
    else
        queueCredentialWrite(key, secret.password, secret.revision);
}

void IrcConnection::flushObsoleteKeys(IrcDraftSecret &secret,
                                      const CredentialKey &currentKey,
                                      quint64 revision)
{
    const QList<CredentialKey> obsolete = secret.obsoleteKeys;
    secret.obsoleteKeys.clear();
    for (const CredentialKey &key : obsolete) {
        if (key == currentKey)
            continue;
        m_credentialOperations.append(
            {CredentialOperation::Kind::Remove, key, {}, revision});
    }
}

void IrcConnection::startCredentialRead(const IrcNetworkProfile &profile)
{
    IrcDraftSecret &secret = secretFor(profile.networkId);
    secret.readInFlight = true;
    secret.readRevision = secret.revision;
    secret.credentialState = CredentialStore::State::Loading;
    const CredentialKey key = credentialKey(profile);
    m_pendingReads.append(key);
    if (m_pendingReads.size() == 1)
        m_credentialStore.read(key);
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
    IrcDraftSecret &secret = selectedSecret();
    if (secret.password != secret.persistedPassword || secret.edited) {
        const bool compensatePendingStore = hasQueuedOperationFor(m_selectedNetworkId);
        secret.password = secret.persistedPassword;
        secret.edited = false;
        secret.credentialState = overlayState(secret.backendState, secret.password);
        ++secret.revision;
        if (compensatePendingStore) {
            if (secret.readInFlight)
                secret.restoreStoreAfterRead = true;
            else
                compensatePersistedSecret(secret, storedProfile(m_selectedNetworkId));
        }
        emit credentialStateChanged();
    }
    emit draftChanged();
    refreshRoster();
}

bool IrcConnection::removeSelected()
{
    if (!canRemove())
        return false;

    const bool wasSetup = setupRequired();
    const QString id = m_selectedNetworkId;
    const IrcNetworkProfile removed = storedProfile(id);
    int removedIndex = 0;
    for (int i = 0; i < m_stored.size(); ++i) {
        if (m_stored.at(i).networkId == id) {
            removedIndex = i;
            break;
        }
    }

    m_controller.discardSession(id);
    m_controller.forgetNetworkState(id);
    if (!removed.networkId.isEmpty())
        queueCredentialRemoval(credentialKey(removed), secretFor(id).revision);
    m_secrets.remove(id);
    m_applied.remove(id);
    m_store.remove(id);
    m_stored.erase(std::remove_if(m_stored.begin(), m_stored.end(),
                                  [&id](const IrcNetworkProfile &profile) {
                                      return profile.networkId == id;
                                  }),
                   m_stored.end());

    QString nextId;
    if (!m_stored.isEmpty()) {
        const int nextIndex = qMin(removedIndex, m_stored.size() - 1);
        nextId = m_stored.at(nextIndex).networkId;
        m_draft = m_stored.at(nextIndex);
        m_selectedNetworkId = nextId;
    } else {
        m_draft = IrcNetworkProfile::suggested();
        m_selectedNetworkId = m_draft.networkId;
    }
    if (m_focusPassword) {
        m_focusPassword = false;
        emit focusPasswordChanged();
    }
    emit selectedNetworkChanged();
    emit credentialStateChanged();
    emit draftChanged();
    if (wasSetup != setupRequired())
        emit setupRequiredChanged();
    refreshRoster();
    pushNetworkOrder();
    return true;
}

bool IrcConnection::activate()
{
    if (m_startupActivationConnection) {
        QObject::disconnect(m_startupActivationConnection);
        m_startupActivationConnection = {};
    }
    if (!isStored(m_selectedNetworkId))
        return false;
    const IrcNetworkProfile profile = storedProfile(m_selectedNetworkId);
    if (!profile.isComplete())
        return false;
    m_draft = profile;
    emit draftChanged();
    return reconcile(profile);
}

bool IrcConnection::activateStartup()
{
    if (m_startupActivationConnection)
        return false;
    if (startupCredentialsPending()) {
        m_startupActivationConnection = connect(
            this, &IrcConnection::credentialStateChanged, this, [this]() {
                if (startupCredentialsPending())
                    return;
                QObject::disconnect(m_startupActivationConnection);
                m_startupActivationConnection = {};
                startMarkedStartupProfiles();
            });
        return false;
    }
    return startMarkedStartupProfiles();
}

void IrcConnection::activateOnStartup()
{
    if (m_startupActivationConnection)
        return;
    const IrcDraftSecret &secret = selectedSecret();
    if (secret.credentialState == CredentialStore::State::Loading
        || secret.readInFlight) {
        m_startupActivationConnection = connect(
            this, &IrcConnection::credentialStateChanged, this, [this]() {
                const IrcDraftSecret &current = selectedSecret();
                if (current.credentialState == CredentialStore::State::Available
                    || current.credentialState == CredentialStore::State::Missing
                    || current.credentialState == CredentialStore::State::Unavailable
                    || current.credentialState == CredentialStore::State::SessionOnly) {
                    activate();
                } else if (current.credentialState == CredentialStore::State::Error) {
                    QObject::disconnect(m_startupActivationConnection);
                    m_startupActivationConnection = {};
                }
            });
        return;
    }
    if (secret.credentialState == CredentialStore::State::Error)
        return;
    activate();
}

bool IrcConnection::startupCredentialsPending() const
{
    for (const IrcNetworkProfile &profile : m_stored) {
        if (!profile.connectOnStartup || !profile.isComplete())
            continue;
        const IrcDraftSecret &secret = secretFor(profile.networkId);
        if (secret.readInFlight
            || secret.credentialState == CredentialStore::State::Loading) {
            return true;
        }
    }
    return false;
}

bool IrcConnection::startMarkedStartupProfiles()
{
    bool started = false;
    for (const IrcNetworkProfile &profile : m_stored) {
        if (!profile.connectOnStartup || !profile.isComplete())
            continue;
        if (secretFor(profile.networkId).credentialState == CredentialStore::State::Error)
            continue;
        started = reconcile(profile) || started;
    }
    return started;
}

void IrcConnection::restoreDraft()
{
    if (!isStored(m_selectedNetworkId)) {
        const QString id = m_selectedNetworkId;
        m_draft = IrcNetworkProfile::create();
        m_draft.networkId = id;
        m_draft.port = 6697;
        m_draft.tlsEnabled = true;
        return;
    }
    m_draft = storedProfile(m_selectedNetworkId);
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
    config.password = secretFor(profile.networkId).password;
    config.autojoinChannels = profile.autojoinChannels;
    return config;
}

bool IrcConnection::reconcile(const IrcNetworkProfile &profile)
{
    const IrcAppliedSession candidate{profile, secretFor(profile.networkId).revision};
    const auto applied = m_applied.constFind(profile.networkId);
    if (applied != m_applied.cend()
        && applied->profile == candidate.profile
        && applied->secretRevision == candidate.secretRevision) {
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

    m_applied.insert(profile.networkId, candidate);
    return m_controller.start(profile.networkId);
}

CredentialKey IrcConnection::credentialKey(const IrcNetworkProfile &profile) const
{
    return {profile.networkId,
            profile.username.isEmpty() ? profile.nick : profile.username,
            profile.host};
}

void IrcConnection::loadStored()
{
    m_stored = m_store.profiles();
    sortStored();
}

void IrcConnection::sortStored()
{
    std::sort(m_stored.begin(), m_stored.end(), profileLess);
}

void IrcConnection::selectStored(const QString &networkId)
{
    if (isStored(networkId))
        m_draft = storedProfile(networkId);
    m_selectedNetworkId = networkId;
    if (m_focusPassword) {
        m_focusPassword = false;
        emit focusPasswordChanged();
    }
    emit selectedNetworkChanged();
    emit credentialStateChanged();
    emit draftChanged();
    refreshRoster();
}

void IrcConnection::pushNetworkOrder()
{
    QStringList order;
    for (const IrcNetworkProfile &profile : m_stored)
        order.append(profile.networkId);
    m_controller.setNetworkOrder(order);
}

void IrcConnection::refreshRoster()
{
    m_networks.resetRows();
    emit networksChanged();
}

bool IrcConnection::isStored(const QString &networkId) const
{
    for (const IrcNetworkProfile &profile : m_stored) {
        if (profile.networkId == networkId)
            return true;
    }
    return false;
}

IrcNetworkProfile IrcConnection::storedProfile(const QString &networkId) const
{
    for (const IrcNetworkProfile &profile : m_stored) {
        if (profile.networkId == networkId)
            return profile;
    }
    return {};
}

QString IrcConnection::rosterDisplayName(const IrcNetworkProfile &profile) const
{
    const QString host = profile.host.trimmed();
    if (host.isEmpty())
        return QStringLiteral("New network");
    bool clash = false;
    for (const IrcNetworkProfile &other : m_stored) {
        if (other.networkId == profile.networkId)
            continue;
        if (other.host.trimmed().compare(host, Qt::CaseInsensitive) == 0) {
            clash = true;
            break;
        }
    }
    if (clash && !profile.nick.trimmed().isEmpty())
        return host + QStringLiteral(" · ") + profile.nick.trimmed();
    return host;
}

QVector<IrcConnection::RosterRow> IrcConnection::rosterRows() const
{
    QVector<RosterRow> rows;
    rows.reserve(m_stored.size() + 1);
    for (const IrcNetworkProfile &profile : m_stored) {
        const IrcNetworkProfile shown =
            profile.networkId == m_selectedNetworkId ? m_draft : profile;
        rows.append({profile.networkId, rosterDisplayName(shown), true,
                     profile.networkId == m_selectedNetworkId});
    }
    if (!isStored(m_selectedNetworkId) && !m_selectedNetworkId.isEmpty()) {
        rows.append({m_selectedNetworkId, rosterDisplayName(m_draft), false, true});
    }
    return rows;
}

const IrcConnection::IrcDraftSecret &IrcConnection::secretFor(const QString &networkId) const
{
    static const IrcDraftSecret empty;
    const auto found = m_secrets.constFind(networkId);
    return found == m_secrets.cend() ? empty : found.value();
}

IrcConnection::IrcDraftSecret &IrcConnection::secretFor(const QString &networkId)
{
    return m_secrets[networkId];
}

const IrcConnection::IrcDraftSecret &IrcConnection::selectedSecret() const
{
    return secretFor(m_selectedNetworkId);
}

IrcConnection::IrcDraftSecret &IrcConnection::selectedSecret()
{
    return secretFor(m_selectedNetworkId);
}

bool IrcConnection::hasQueuedOperationFor(const QString &networkId) const
{
    for (const CredentialOperation &operation : m_credentialOperations) {
        if (operation.key.networkId == networkId)
            return true;
    }
    return false;
}
