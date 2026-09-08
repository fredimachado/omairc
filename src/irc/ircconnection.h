#pragma once

#include "ircnetworkprofile.h"
#include "ircprofilestore.h"
#include "credentialstore.h"

#include <QObject>
#include <QString>

#include <functional>
#include <optional>

class IrcController;
class IrcTransport;
struct IrcSessionConfig;

class IrcConnection : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QString host READ host WRITE setHost NOTIFY draftChanged)
    Q_PROPERTY(int port READ port WRITE setPort NOTIFY draftChanged)
    Q_PROPERTY(bool tlsEnabled READ tlsEnabled WRITE setTlsEnabled NOTIFY draftChanged)
    Q_PROPERTY(bool connectOnStartup READ connectOnStartup WRITE setConnectOnStartup NOTIFY draftChanged)
    Q_PROPERTY(QString nick READ nick WRITE setNick NOTIFY draftChanged)
    Q_PROPERTY(QString username READ username WRITE setUsername NOTIFY draftChanged)
    Q_PROPERTY(QString realname READ realname WRITE setRealname NOTIFY draftChanged)
    Q_PROPERTY(QString autojoin READ autojoin WRITE setAutojoin NOTIFY draftChanged)
    Q_PROPERTY(bool passwordSet READ passwordSet NOTIFY draftChanged)
    Q_PROPERTY(CredentialStore::State credentialState READ credentialState
               NOTIFY credentialStateChanged)
    Q_PROPERTY(QString credentialError READ credentialError NOTIFY credentialStateChanged)
    Q_PROPERTY(QString credentialStatus READ credentialStatus NOTIFY credentialStateChanged)
    Q_PROPERTY(QString problem READ problem NOTIFY draftChanged)
    Q_PROPERTY(bool dirty READ dirty NOTIFY draftChanged)
    Q_PROPERTY(QString displayName READ displayName NOTIFY draftChanged)
    Q_PROPERTY(bool setupRequired READ setupRequired NOTIFY setupRequiredChanged)
    Q_PROPERTY(bool focusPassword READ focusPassword NOTIFY focusPasswordChanged)

public:
    using TransportFactory = std::function<IrcTransport *()>;
    using CredentialStoreFactory = std::function<CredentialStore *()>;

    explicit IrcConnection(IrcController &controller, QObject *parent = nullptr);
    IrcConnection(IrcController &controller,
                  TransportFactory transportFactory,
                  QObject *parent = nullptr);
    IrcConnection(IrcController &controller,
                  TransportFactory transportFactory,
                  CredentialStoreFactory credentialStoreFactory,
                  QObject *parent = nullptr);

    QString host() const;
    int port() const;
    bool tlsEnabled() const;
    bool connectOnStartup() const;
    QString nick() const;
    QString username() const;
    QString realname() const;
    QString autojoin() const;
    bool passwordSet() const;
    CredentialStore::State credentialState() const;
    QString credentialError() const;
    QString credentialStatus() const;
    QString problem() const;
    bool dirty() const;
    QString displayName() const;
    bool setupRequired() const;
    bool focusPassword() const;

    void setHost(const QString &host);
    void setPort(int port);
    void setTlsEnabled(bool enabled);
    void setConnectOnStartup(bool enabled);
    void setNick(const QString &nick);
    void setUsername(const QString &username);
    void setRealname(const QString &realname);
    void setAutojoin(const QString &channels);

    Q_INVOKABLE void setPassword(const QString &password);
    Q_INVOKABLE bool apply();
    Q_INVOKABLE void discard();
    bool activate();
    void activateOnStartup();

signals:
    void draftChanged();
    void setupRequiredChanged();
    void focusPasswordChanged();
    void credentialStateChanged();

private:
    void restoreDraft();
    std::optional<IrcSessionConfig> sessionConfigFor(
        const IrcNetworkProfile &profile) const;
    bool reconcile(const IrcNetworkProfile &profile);
    CredentialKey credentialKey(const IrcNetworkProfile &profile) const;

    struct Applied {
        IrcNetworkProfile profile;
        quint64 secretRevision = 0;
    };

    IrcController &m_controller;
    TransportFactory m_transportFactory;
    IrcProfileStore m_store;
    CredentialStoreFactory m_credentialStoreFactory;
    CredentialStore *m_credentialStore = nullptr;
    IrcNetworkProfile m_draft;
    IrcNetworkProfile m_stored;
    QString m_password;
    quint64 m_secretRevision = 0;
    std::optional<Applied> m_applied;
    bool m_focusPassword = false;
    bool m_passwordEdited = false;
    CredentialStore::State m_credentialState = CredentialStore::State::Missing;
    QString m_credentialError;
    QMetaObject::Connection m_startupActivationConnection;
};
