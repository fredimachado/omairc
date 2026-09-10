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
    , m_networks(*this)
{
    if (!m_transportFactory)
        m_transportFactory = defaultTransport;

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
    return !secretFor(m_selectedNetworkId).password.isEmpty();
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
    emit draftChanged();
    refreshRoster();
    return true;
}

void IrcConnection::setPassword(const QString &password)
{
    IrcDraftSecret &secret = secretFor(m_selectedNetworkId);
    if (secret.password == password)
        return;
    secret.password = password;
    ++secret.revision;
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
    if (wasSetup)
        emit setupRequiredChanged();
    refreshRoster();
    pushNetworkOrder();
    return reconcile(profile);
}

void IrcConnection::discard()
{
    restoreDraft();
    emit draftChanged();
    refreshRoster();
}

bool IrcConnection::removeSelected()
{
    if (!canRemove())
        return false;

    const bool wasSetup = setupRequired();
    const QString id = m_selectedNetworkId;
    int removedIndex = 0;
    for (int i = 0; i < m_stored.size(); ++i) {
        if (m_stored.at(i).networkId == id) {
            removedIndex = i;
            break;
        }
    }

    m_controller.discardSession(id);
    m_controller.forgetNetworkState(id);
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
    emit draftChanged();
    if (wasSetup != setupRequired())
        emit setupRequiredChanged();
    refreshRoster();
    pushNetworkOrder();
    return true;
}

bool IrcConnection::activate()
{
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
    bool started = false;
    for (const IrcNetworkProfile &profile : m_stored) {
        if (!profile.connectOnStartup || !profile.isComplete())
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
