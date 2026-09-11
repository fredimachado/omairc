#pragma once

#include "ircnetworkprofile.h"
#include "ircprofilestore.h"
#include "../storage/credentialstore.h"

#include <QAbstractListModel>
#include <QHash>
#include <QList>
#include <QObject>
#include <QString>
#include <QVector>

#include <functional>
#include <optional>

class IrcConnection;
class IrcController;
class IrcTransport;
struct IrcSessionConfig;

class NetworkListModel : public QAbstractListModel
{
    Q_OBJECT

public:
    enum Role {
        NetworkIdRole = Qt::UserRole + 1,
        DisplayNameRole,
        StoredRole,
        SelectedRole,
    };

    explicit NetworkListModel(IrcConnection &owner, QObject *parent = nullptr);

    int rowCount(const QModelIndex &parent = QModelIndex()) const override;
    QVariant data(const QModelIndex &index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;

    void resetRows();

private:
    IrcConnection &m_owner;
};

class IrcConnection : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QAbstractItemModel* networks READ networks CONSTANT)
    Q_PROPERTY(QString selectedNetworkId READ selectedNetworkId NOTIFY selectedNetworkChanged)
    Q_PROPERTY(bool canAdd READ canAdd NOTIFY draftChanged)
    Q_PROPERTY(bool canRemove READ canRemove NOTIFY selectedNetworkChanged)
    Q_PROPERTY(QString host READ host WRITE setHost NOTIFY draftChanged)
    Q_PROPERTY(int port READ port WRITE setPort NOTIFY draftChanged)
    Q_PROPERTY(bool tlsEnabled READ tlsEnabled WRITE setTlsEnabled NOTIFY draftChanged)
    Q_PROPERTY(bool connectOnStartup READ connectOnStartup WRITE setConnectOnStartup NOTIFY draftChanged)
    Q_PROPERTY(QString nick READ nick WRITE setNick NOTIFY draftChanged)
    Q_PROPERTY(QString username READ username WRITE setUsername NOTIFY draftChanged)
    Q_PROPERTY(QString realname READ realname WRITE setRealname NOTIFY draftChanged)
    Q_PROPERTY(QString autojoin READ autojoin WRITE setAutojoin NOTIFY draftChanged)
    Q_PROPERTY(bool passwordSet READ passwordSet NOTIFY draftChanged)
    Q_PROPERTY(bool nickServSet READ nickServSet NOTIFY draftChanged)
    Q_PROPERTY(CredentialStore::State credentialState READ credentialState
               NOTIFY credentialStateChanged)
    Q_PROPERTY(QString credentialError READ credentialError NOTIFY credentialStateChanged)
    Q_PROPERTY(QString credentialStatus READ credentialStatus NOTIFY credentialStateChanged)
    Q_PROPERTY(bool canForgetPassword READ canForgetPassword
               NOTIFY credentialStateChanged)
    Q_PROPERTY(bool canForgetNickServ READ canForgetNickServ
               NOTIFY credentialStateChanged)
    Q_PROPERTY(QString problem READ problem NOTIFY draftChanged)
    Q_PROPERTY(bool dirty READ dirty NOTIFY draftChanged)
    Q_PROPERTY(QString displayName READ displayName NOTIFY draftChanged)
    Q_PROPERTY(bool setupRequired READ setupRequired NOTIFY setupRequiredChanged)
    Q_PROPERTY(bool focusPassword READ focusPassword NOTIFY focusPasswordChanged)
    Q_PROPERTY(bool focusNickServ READ focusNickServ NOTIFY focusNickServChanged)

public:
    using TransportFactory = std::function<IrcTransport *()>;

    IrcConnection(IrcController &controller, CredentialStore &credentialStore,
                  QObject *parent = nullptr);
    IrcConnection(IrcController &controller,
                  TransportFactory transportFactory,
                  CredentialStore &credentialStore,
                  QObject *parent = nullptr);

    QAbstractItemModel *networks();
    QString selectedNetworkId() const;
    bool canAdd() const;
    bool canRemove() const;

    QString host() const;
    int port() const;
    bool tlsEnabled() const;
    bool connectOnStartup() const;
    QString nick() const;
    QString username() const;
    QString realname() const;
    QString autojoin() const;
    bool passwordSet() const;
    bool nickServSet() const;
    CredentialStore::State credentialState() const;
    QString credentialError() const;
    QString credentialStatus() const;
    bool canForgetPassword() const;
    bool canForgetNickServ() const;
    QString problem() const;
    bool dirty() const;
    QString displayName() const;
    bool setupRequired() const;
    bool focusPassword() const;
    bool focusNickServ() const;

    void setHost(const QString &host);
    void setPort(int port);
    void setTlsEnabled(bool enabled);
    void setConnectOnStartup(bool enabled);
    void setNick(const QString &nick);
    void setUsername(const QString &username);
    void setRealname(const QString &realname);
    void setAutojoin(const QString &channels);

    Q_INVOKABLE void select(const QString &networkId);
    Q_INVOKABLE bool add();
    Q_INVOKABLE void setPassword(const QString &password);
    Q_INVOKABLE void setNickServPassword(const QString &password);
    Q_INVOKABLE void forgetPassword();
    Q_INVOKABLE void forgetNickServ();
    Q_INVOKABLE void removeStoredPassword();
    Q_INVOKABLE void removeStoredNickServ();
    Q_INVOKABLE bool apply();
    Q_INVOKABLE void discard();
    Q_INVOKABLE bool removeSelected();
    bool activate();
    bool activateStartup();
    void activateOnStartup();

signals:
    void selectedNetworkChanged();
    void networksChanged();
    void draftChanged();
    void setupRequiredChanged();
    void focusPasswordChanged();
    void focusNickServChanged();
    void credentialStateChanged();

private:
    friend class NetworkListModel;

    struct IrcDraftSecret {
        QString password;
        QString persistedPassword;
        quint64 revision = 0;
        bool edited = false;
        bool mayBeStored = false;
        bool readInFlight = false;
        quint64 readRevision = 0;
        bool reconcileWhenReadSettles = false;
        bool restoreStoreAfterRead = false;
        CredentialStore::State backendState = CredentialStore::State::Missing;
        CredentialStore::State credentialState = CredentialStore::State::Missing;
        QString credentialError;
        QList<CredentialKey> obsoleteKeys;
        QString obsoleteRemovalError;
    };

    struct IrcAppliedSession {
        IrcNetworkProfile profile;
        quint64 secretRevision = 0;
        quint64 nickServRevision = 0;
    };

    struct RosterRow {
        QString networkId;
        QString displayName;
        bool stored = false;
        bool selected = false;
    };

    struct CredentialOperation {
        enum class Kind {
            Write,
            Remove,
        };

        Kind kind;
        CredentialKey key;
        QString password;
        quint64 revision = 0;
    };

    void restoreDraft();
    void processCredentialOperations();
    void rememberObsoleteKey(IrcDraftSecret &secret, const CredentialKey &key);
    void flushObsoleteKeys(IrcDraftSecret &secret, const CredentialKey &currentKey,
                           quint64 revision);
    CredentialStore::State overlayState(CredentialStore::State backend,
                                        const QString &password) const;
    void adoptBackendState(IrcDraftSecret &secret, CredentialStore::State state,
                           const QString &message);
    void handleCredentialRead(CredentialStore::State state, const QString &password,
                              const QString &message);
    void handleCredentialWrite(CredentialStore::State state, const QString &message);
    void settleCredentialRead(IrcDraftSecret &secret, const IrcNetworkProfile &profile,
                              const CredentialKey &key, CredentialStore::State state,
                              const QString &password, const QString &message);
    void compensatePersistedSecret(IrcDraftSecret &secret, const CredentialKey &key);
    void queueCredentialWrite(const CredentialKey &key, const QString &password,
                              quint64 revision);
    void queueCredentialRemoval(const CredentialKey &key, quint64 revision);
    void startCredentialRead(const IrcNetworkProfile &profile);
    std::optional<IrcSessionConfig> sessionConfigFor(
        const IrcNetworkProfile &profile) const;
    bool reconcile(const IrcNetworkProfile &profile);
    CredentialKey credentialKey(const IrcNetworkProfile &profile) const;
    CredentialKey nickServCredentialKey(const IrcNetworkProfile &profile) const;
    void applySecret(IrcDraftSecret &secret, const CredentialKey &previousKey,
                     const CredentialKey &nextKey, bool rememberEmptyObsolete);
    QString secretStatus(const IrcDraftSecret &secret, const QString &noun) const;
    bool secretIsIdle(const IrcDraftSecret &secret) const;
    bool secretReadPending(const IrcDraftSecret &secret) const;
    bool secretReadBlocksApply(const IrcDraftSecret &secret, bool tracked) const;
    bool secretSlotTracked(const IrcDraftSecret &secret, bool saved) const;
    void markSessionOnlyIfStoreUnavailable(IrcDraftSecret &secret,
                                           const IrcDraftSecret &other) const;
    bool secretUnreadable(const IrcDraftSecret &secret, bool saved,
                          bool required = false) const;
    void maybeReconcileAfterRead(const QString &networkId);
    void clearFocusPassword();
    void clearFocusNickServ();
    void requestStartupSecret(const IrcNetworkProfile &profile);
    bool startupCredentialsPending() const;
    bool startupCredentialsPending(const IrcNetworkProfile &profile) const;
    bool startupConnectAllowed(const IrcNetworkProfile &profile) const;
    void requestStartupPassword(const IrcNetworkProfile &profile);
    void persistSavedFlag(const CredentialKey &key, bool saved);
    bool startMarkedStartupProfiles();
    void loadStored();
    void sortStored();
    void selectStored(const QString &networkId);
    void pushNetworkOrder();
    void refreshRoster();
    bool isStored(const QString &networkId) const;
    IrcNetworkProfile storedProfile(const QString &networkId) const;
    QString rosterDisplayName(const IrcNetworkProfile &profile) const;
    QVector<RosterRow> rosterRows() const;
    const IrcDraftSecret &secretFor(const QString &networkId) const;
    IrcDraftSecret &secretFor(const QString &networkId);
    const IrcDraftSecret &nickServSecretFor(const QString &networkId) const;
    IrcDraftSecret &nickServSecretFor(const QString &networkId);
    const IrcDraftSecret &selectedSecret() const;
    IrcDraftSecret &selectedSecret();
    const IrcDraftSecret &selectedNickServSecret() const;
    IrcDraftSecret &selectedNickServSecret();
    IrcDraftSecret &secretForKey(const CredentialKey &key);
    bool hasQueuedOperationFor(const QString &networkId) const;
    bool hasQueuedOperationFor(const CredentialKey &key) const;

    IrcController &m_controller;
    TransportFactory m_transportFactory;
    IrcProfileStore m_store;
    CredentialStore &m_credentialStore;
    QList<IrcNetworkProfile> m_stored;
    IrcNetworkProfile m_draft;
    QString m_selectedNetworkId;
    QHash<QString, IrcDraftSecret> m_secrets;
    QHash<QString, IrcDraftSecret> m_nickServSecrets;
    QHash<QString, IrcAppliedSession> m_applied;
    NetworkListModel m_networks;
    bool m_focusPassword = false;
    bool m_focusNickServ = false;
    QList<CredentialOperation> m_credentialOperations;
    QList<CredentialKey> m_pendingReads;
    bool m_operationInFlight = false;
    QMetaObject::Connection m_startupActivationConnection;
};
