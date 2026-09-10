#include <QCoreApplication>
#include <QFile>
#include <QList>
#include <QSettings>
#include <QTemporaryDir>
#include <QTest>

#include "fakeirctransport.h"
#include "storage/credentialstore.h"
#include "ircconnection.h"
#include "irccontroller.h"

#include <memory>
#include <utility>
#include <vector>

class FakeCredentialStore;

class ConnectionTest : public QObject
{
    Q_OBJECT

private slots:
    void init();
    void setupRequiredUntilCompleteProfileIsSaved();
    void loadingStoredProfileDoesNotConnectUntilActivate();
    void connectOnStartupDraftAppliesAndDiscards();
    void applyIsIdempotentForTheSameSecret();
    void profileKeyChangePersistsExistingPassword();
    void passwordChangeRebuildsTheSession();
    void discardRestoresStoredDraft();
    void tlsSwitchLeavesPortAlone();
    void authenticationFailureFocusesPassword();
    void applyWritesPasswordToStoreNotSettings();
    void credentialStoreLoadsPasswordAsynchronously();
    void startupActivationWaitsForCredentialRead();
    void startupActivationWaitsForUsableCredentialState();
    void startupActivationDoesNotConnectAfterCredentialError();
    void emptyPasswordDoesNotDeleteStoredCredential();
    void editedPasswordShowsPendingSaveStatus();
    void removingStoredPasswordDoesNotReconnect();
    void forgettingPasswordBeforeRemovingStoredPasswordReportsFailure();
    void missingStoredPasswordWithSessionPasswordIsNotReportedAsMissing();
    void missingPasswordDoesNotShowCredentialStatus();
    void unavailableCredentialStoreUsesSessionOnlyState();
    void applyDuringCredentialReadMigratesLoadedPassword();
    void lateCredentialReadDoesNotReplaceEditedPassword();
    void forgetDuringKeyMigrationStillRemovesPreviousKey();
    void startupActivationRecoversAfterCredentialError();
    void credentialErrorWithoutPasswordIsNotReportedAsSessionOnly();
    void applyDuringCredentialReadConnectsOnceAfterLoad();
    void forgetFailureKeepsForgetAvailable();
    void staleReadAfterKeyChangeStillRemovesObsoleteKey();
    void applyReportsWriteFailure();
    void obsoleteKeyRemovalFailureIsRetriedOnApply();
    void failedWriteEmptyApplyKeepsSessionPassword();

private:
    IrcConnection::TransportFactory capturingFactory();
    CredentialStore &credentialStore();
    void fillCompleteDraft(IrcConnection &connection);

    std::unique_ptr<QTemporaryDir> m_dir;
    QList<FakeIrcTransport *> m_transports;
    std::vector<std::unique_ptr<FakeCredentialStore>> m_credentialStores;
};

class FakeCredentialStore final : public CredentialStore
{
public:
    explicit FakeCredentialStore(State readState, QString password = {},
                                 State removeState = State::Missing,
                                 QString removeMessage = {},
                                 QObject *parent = nullptr)
        : CredentialStore(parent)
        , m_readState(readState)
        , m_password(std::move(password))
        , m_removeState(removeState)
        , m_removeMessage(std::move(removeMessage))
    {
    }

    void read(const CredentialKey &) override
    {
        emit readFinished(State::Loading, {}, {});
        if (m_holdRead)
            return;
        QMetaObject::invokeMethod(this, [this]() {
            emitHeldRead();
        }, Qt::QueuedConnection);
    }

    void write(const CredentialKey &key, const QString &password) override
    {
        m_writtenKeys.append(key);
        m_writtenPassword = password;
        ++m_writeCalls;
        QMetaObject::invokeMethod(this, [this]() {
            emit writeFinished(m_writeState, m_writeMessage);
        }, Qt::QueuedConnection);
    }

    void remove(const CredentialKey &key) override
    {
        m_removedKeys.append(key);
        ++m_removeCalls;
        QMetaObject::invokeMethod(this, [this]() {
            emit writeFinished(m_removeState, m_removeMessage);
        }, Qt::QueuedConnection);
    }

    QString writtenPassword() const { return m_writtenPassword; }
    QList<CredentialKey> writtenKeys() const { return m_writtenKeys; }
    QList<CredentialKey> removedKeys() const { return m_removedKeys; }
    int writeCalls() const { return m_writeCalls; }
    int removeCalls() const { return m_removeCalls; }
    void setHoldRead(bool hold) { m_holdRead = hold; }
    void setWriteResult(State state, QString message = {})
    {
        m_writeState = state;
        m_writeMessage = std::move(message);
    }
    void setRemoveResult(State state, QString message = {})
    {
        m_removeState = state;
        m_removeMessage = std::move(message);
    }
    void completeHeldRead()
    {
        emitHeldRead();
    }

private:
    void emitHeldRead()
    {
        if (m_readState == State::Available && m_password.isEmpty())
            emit readFinished(State::Missing, {}, {});
        else
            emit readFinished(m_readState, m_password, {});
    }

    State m_readState;
    QString m_password;
    QString m_writtenPassword;
    QList<CredentialKey> m_writtenKeys;
    QList<CredentialKey> m_removedKeys;
    State m_removeState;
    QString m_removeMessage;
    State m_writeState = State::Available;
    QString m_writeMessage;
    int m_writeCalls = 0;
    int m_removeCalls = 0;
    bool m_holdRead = false;
};

void ConnectionTest::init()
{
    m_dir = std::make_unique<QTemporaryDir>();
    QVERIFY(m_dir->isValid());
    qputenv("XDG_CONFIG_HOME", m_dir->path().toUtf8());
    QSettings::setPath(QSettings::NativeFormat, QSettings::UserScope, m_dir->path());
    QCoreApplication::setOrganizationName(QStringLiteral("omairc"));
    QCoreApplication::setApplicationName(QStringLiteral("omairc"));
    m_transports.clear();
    m_credentialStores.clear();
}

IrcConnection::TransportFactory ConnectionTest::capturingFactory()
{
    return [this]() {
        auto *transport = new FakeIrcTransport;
        m_transports.append(transport);
        return transport;
    };
}

CredentialStore &ConnectionTest::credentialStore()
{
    m_credentialStores.push_back(std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Missing));
    return *m_credentialStores.back();
}

void ConnectionTest::fillCompleteDraft(IrcConnection &connection)
{
    connection.setHost(QStringLiteral("irc.example"));
    connection.setPort(6697);
    connection.setTlsEnabled(true);
    connection.setNick(QStringLiteral("omairc"));
    connection.setUsername(QStringLiteral("omairc"));
    connection.setRealname(QStringLiteral("Omairc User"));
    connection.setAutojoin(QStringLiteral("#omarchy"));
}

void ConnectionTest::setupRequiredUntilCompleteProfileIsSaved()
{
    IrcController controller;
    IrcConnection connection(controller, capturingFactory(), credentialStore());
    QVERIFY(connection.setupRequired());
    QCOMPARE(connection.host(), QStringLiteral("irc.libera.chat"));
    QCOMPARE(connection.displayName(), QStringLiteral("irc.libera.chat"));
    QVERIFY(!connection.activate());
    QCOMPARE(m_transports.size(), 0);

    fillCompleteDraft(connection);
    QVERIFY(connection.apply());
    QVERIFY(!connection.setupRequired());
    QCOMPARE(m_transports.size(), 1);
    QCOMPARE(m_transports.first()->connectionState(),
             IrcTransport::ConnectionState::Connecting);
}

void ConnectionTest::loadingStoredProfileDoesNotConnectUntilActivate()
{
    {
        IrcController controller;
        IrcConnection connection(controller, capturingFactory(), credentialStore());
        fillCompleteDraft(connection);
        QVERIFY(connection.apply());
    }
    m_transports.clear();

    IrcController controller;
    IrcConnection connection(controller, capturingFactory(), credentialStore());
    QVERIFY(!connection.setupRequired());
    QCOMPARE(connection.host(), QStringLiteral("irc.example"));
    QCOMPARE(m_transports.size(), 0);

    QVERIFY(connection.activate());
    QCOMPARE(m_transports.size(), 1);
    QCOMPARE(m_transports.first()->connectionState(),
             IrcTransport::ConnectionState::Connecting);
}

void ConnectionTest::connectOnStartupDraftAppliesAndDiscards()
{
    IrcController controller;
    IrcConnection connection(controller, capturingFactory(), credentialStore());
    fillCompleteDraft(connection);
    QVERIFY(!connection.connectOnStartup());
    connection.setConnectOnStartup(true);
    QVERIFY(connection.apply());
    QCOMPARE(connection.connectOnStartup(), true);

    IrcConnection reloaded(controller, capturingFactory(), credentialStore());
    QCOMPARE(reloaded.connectOnStartup(), true);
    reloaded.setConnectOnStartup(false);
    reloaded.discard();
    QCOMPARE(reloaded.connectOnStartup(), true);
}

void ConnectionTest::applyIsIdempotentForTheSameSecret()
{
    IrcController controller;
    IrcConnection connection(controller, capturingFactory(), credentialStore());
    fillCompleteDraft(connection);
    QVERIFY(connection.apply());
    QVERIFY(connection.apply());
    QCOMPARE(m_transports.size(), 1);
}

void ConnectionTest::profileKeyChangePersistsExistingPassword()
{
    {
        IrcController seedController;
        IrcConnection seed(seedController, capturingFactory(), credentialStore());
        fillCompleteDraft(seed);
        QVERIFY(seed.apply());
    }

    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Available, QStringLiteral("stored-secret"));
    IrcConnection connection(controller, capturingFactory(), *store);
    fillCompleteDraft(connection);
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Available);
    QVERIFY(connection.apply());
    QTRY_COMPARE(store->writeCalls(), 0);

    connection.setHost(QStringLiteral("irc.changed"));
    QVERIFY(connection.apply());
    QTRY_COMPARE(store->writeCalls(), 1);
    QCOMPARE(store->writtenPassword(), QStringLiteral("stored-secret"));
}

void ConnectionTest::passwordChangeRebuildsTheSession()
{
    IrcController controller;
    IrcConnection connection(controller, capturingFactory(), credentialStore());
    fillCompleteDraft(connection);
    QVERIFY(connection.apply());
    connection.setPassword(QStringLiteral("secret"));
    QVERIFY(connection.passwordSet());
    QVERIFY(connection.apply());
    QCOMPARE(m_transports.size(), 2);
}

void ConnectionTest::discardRestoresStoredDraft()
{
    IrcController controller;
    IrcConnection connection(controller, capturingFactory(), credentialStore());
    fillCompleteDraft(connection);
    QVERIFY(connection.apply());

    connection.setHost(QStringLiteral("irc.changed"));
    QVERIFY(connection.dirty());
    connection.discard();
    QCOMPARE(connection.host(), QStringLiteral("irc.example"));
    QVERIFY(!connection.dirty());
}

void ConnectionTest::tlsSwitchLeavesPortAlone()
{
    IrcController controller;
    IrcConnection connection(controller, capturingFactory(), credentialStore());
    QCOMPARE(connection.port(), 6697);
    connection.setTlsEnabled(false);
    QCOMPARE(connection.port(), 6697);
    connection.setTlsEnabled(true);
    QCOMPARE(connection.port(), 6697);
}

void ConnectionTest::authenticationFailureFocusesPassword()
{
    IrcController controller;
    IrcConnection connection(controller, capturingFactory(), credentialStore());
    fillCompleteDraft(connection);
    connection.setPassword(QStringLiteral("secret"));
    QVERIFY(connection.apply());
    QVERIFY(!connection.focusPassword());

    m_transports.last()->completeConnect();
    m_transports.last()->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :sasl\r\n"
                          ":server CAP omairc ACK :sasl\r\n"
                          ":server 904 omairc :SASL failed\r\n"));
    QVERIFY(connection.focusPassword());
}

void ConnectionTest::applyWritesPasswordToStoreNotSettings()
{
    IrcController controller;
    IrcConnection connection(controller, capturingFactory(), credentialStore());
    fillCompleteDraft(connection);
    connection.setPassword(QStringLiteral("super-secret"));
    QVERIFY(connection.apply());
    QTRY_COMPARE(m_credentialStores.back()->writeCalls(), 1);
    QCOMPARE(m_credentialStores.back()->writtenPassword(),
             QStringLiteral("super-secret"));

    QSettings settings;
    QFile file(settings.fileName());
    QVERIFY(file.open(QIODevice::ReadOnly | QIODevice::Text));
    const QString contents = QString::fromUtf8(file.readAll());
    QVERIFY(!contents.contains(QLatin1String("password"), Qt::CaseInsensitive));
    QVERIFY(!contents.contains(QLatin1String("super-secret")));
}

void ConnectionTest::credentialStoreLoadsPasswordAsynchronously()
{
    {
        IrcController controller;
        auto store = std::make_unique<FakeCredentialStore>(
            CredentialStore::State::Available, QStringLiteral("stored-secret"));
        IrcConnection connection(controller, capturingFactory(), *store);
        fillCompleteDraft(connection);
        QVERIFY(connection.apply());
    }

    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Available, QStringLiteral("stored-secret"));
    IrcConnection connection(controller, capturingFactory(), *store);
    QVERIFY(connection.credentialState() == CredentialStore::State::Loading
            || connection.credentialState() == CredentialStore::State::Available);
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Available);
    QVERIFY(connection.passwordSet());
    QCOMPARE(connection.credentialStatus(), QStringLiteral("password saved securely"));
}

void ConnectionTest::startupActivationWaitsForCredentialRead()
{
    {
        IrcController controller;
        IrcConnection connection(controller, capturingFactory(), credentialStore());
        fillCompleteDraft(connection);
        connection.setConnectOnStartup(true);
        QVERIFY(connection.apply());
    }
    m_transports.clear();

    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Available, QStringLiteral("stored-secret"));
    IrcConnection connection(controller, capturingFactory(), *store);
    QCOMPARE(connection.credentialState(), CredentialStore::State::Loading);

    connection.activateOnStartup();
    QCOMPARE(m_transports.size(), 0);

    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Available);
    QCOMPARE(m_transports.size(), 1);

    m_transports.clear();
    IrcController missingController;
    auto missingStore = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Missing);
    IrcConnection missingConnection(missingController, capturingFactory(), *missingStore);
    missingConnection.activateOnStartup();
    QCOMPARE(m_transports.size(), 0);
    QTRY_COMPARE(missingConnection.credentialState(), CredentialStore::State::Missing);
    QCOMPARE(m_transports.size(), 1);
}

void ConnectionTest::startupActivationWaitsForUsableCredentialState()
{
    {
        IrcController seedController;
        IrcConnection seed(seedController, capturingFactory(), credentialStore());
        fillCompleteDraft(seed);
        seed.setConnectOnStartup(true);
        QVERIFY(seed.apply());
    }
    m_transports.clear();

    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Error);
    IrcConnection connection(controller, capturingFactory(), *store);
    connection.activateOnStartup();
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Error);
    QCOMPARE(m_transports.size(), 0);

    m_transports.clear();
    IrcController unavailableController;
    auto unavailableStore = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Unavailable);
    IrcConnection unavailableConnection(unavailableController, capturingFactory(),
                                        *unavailableStore);
    unavailableConnection.activateOnStartup();
    QTRY_COMPARE(unavailableConnection.credentialState(),
                 CredentialStore::State::Unavailable);
    QCOMPARE(m_transports.size(), 1);
}

void ConnectionTest::startupActivationDoesNotConnectAfterCredentialError()
{
    {
        IrcController seedController;
        IrcConnection seed(seedController, capturingFactory(), credentialStore());
        fillCompleteDraft(seed);
        seed.setConnectOnStartup(true);
        QVERIFY(seed.apply());
    }
    m_transports.clear();

    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Error);
    IrcConnection connection(controller, capturingFactory(), *store);
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Error);

    connection.activateOnStartup();

    QCOMPARE(m_transports.size(), 0);
}

void ConnectionTest::emptyPasswordDoesNotDeleteStoredCredential()
{
    {
        IrcController seedController;
        IrcConnection seed(seedController, capturingFactory(), credentialStore());
        fillCompleteDraft(seed);
        QVERIFY(seed.apply());
    }

    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Available, QStringLiteral("stored-secret"));
    IrcConnection connection(controller, capturingFactory(), *store);
    fillCompleteDraft(connection);
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Available);
    QVERIFY(connection.canForgetPassword());
    QVERIFY(connection.apply());

    connection.setPassword(QString());
    QVERIFY(connection.apply());
    QCOMPARE(store->writeCalls(), 0);
    QCOMPARE(store->removeCalls(), 0);

    connection.forgetPassword();
    QVERIFY(connection.canForgetPassword());
    QVERIFY(connection.apply());
    QTRY_VERIFY(!connection.canForgetPassword());
    QCOMPARE(store->removeCalls(), 1);
}

void ConnectionTest::editedPasswordShowsPendingSaveStatus()
{
    {
        IrcController seedController;
        IrcConnection seed(seedController, capturingFactory(), credentialStore());
        fillCompleteDraft(seed);
        QVERIFY(seed.apply());
    }
    m_transports.clear();

    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Available, QStringLiteral("stored-secret"));
    IrcConnection connection(controller, capturingFactory(), *store);
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Available);

    connection.setPassword(QStringLiteral("new-secret"));
    QCOMPARE(connection.credentialStatus(),
             QStringLiteral("password changed; apply to save securely"));
}

void ConnectionTest::removingStoredPasswordDoesNotReconnect()
{
    {
        IrcController seedController;
        IrcConnection seed(seedController, capturingFactory(), credentialStore());
        fillCompleteDraft(seed);
        QVERIFY(seed.apply());
    }
    m_transports.clear();

    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Available, QStringLiteral("stored-secret"));
    IrcConnection connection(controller, capturingFactory(), *store);
    fillCompleteDraft(connection);
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Available);
    QVERIFY(connection.apply());
    QCOMPARE(m_transports.size(), 1);

    connection.removeStoredPassword();
    QCOMPARE(m_transports.size(), 1);
    QCOMPARE(store->removeCalls(), 1);
}

void ConnectionTest::forgettingPasswordBeforeRemovingStoredPasswordReportsFailure()
{
    {
        IrcController seedController;
        IrcConnection seed(seedController, capturingFactory(), credentialStore());
        fillCompleteDraft(seed);
        QVERIFY(seed.apply());
    }

    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Available, QStringLiteral("stored-secret"),
        CredentialStore::State::Error, QStringLiteral("remove failed"));
    IrcConnection connection(controller, capturingFactory(), *store);
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Available);

    connection.forgetPassword();
    connection.removeStoredPassword();

    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Error);
    QCOMPARE(connection.credentialError(), QStringLiteral("remove failed"));
    QVERIFY(connection.canForgetPassword());
}

void ConnectionTest::missingStoredPasswordWithSessionPasswordIsNotReportedAsMissing()
{
    {
        IrcController seedController;
        IrcConnection seed(seedController, capturingFactory(), credentialStore());
        fillCompleteDraft(seed);
        QVERIFY(seed.apply());
    }
    m_transports.clear();

    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Available, QStringLiteral("stored-secret"));
    IrcConnection connection(controller, capturingFactory(), *store);
    fillCompleteDraft(connection);
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Available);
    QVERIFY(connection.apply());

    connection.removeStoredPassword();
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Missing);
    QVERIFY(connection.passwordSet());
    QCOMPARE(connection.credentialStatus(),
             QStringLiteral("password is session-only until applied"));
}

void ConnectionTest::missingPasswordDoesNotShowCredentialStatus()
{
    {
        IrcController seedController;
        IrcConnection seed(seedController, capturingFactory(), credentialStore());
        fillCompleteDraft(seed);
        QVERIFY(seed.apply());
    }

    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Missing);
    IrcConnection connection(controller, capturingFactory(), *store);
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Missing);
    QVERIFY(!connection.passwordSet());
    QCOMPARE(connection.credentialStatus(), QString());
}

void ConnectionTest::unavailableCredentialStoreUsesSessionOnlyState()
{
    {
        IrcController controller;
        IrcConnection connection(controller, capturingFactory(), credentialStore());
        fillCompleteDraft(connection);
        QVERIFY(connection.apply());
    }

    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Unavailable);
    IrcConnection connection(controller, capturingFactory(), *store);
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Unavailable);
    QVERIFY(!connection.passwordSet());
    QCOMPARE(connection.credentialStatus(),
             QStringLiteral("secure storage unavailable"));
    connection.setPassword(QStringLiteral("session-secret"));
    QVERIFY(connection.passwordSet());
    QCOMPARE(connection.credentialState(), CredentialStore::State::SessionOnly);
    QCOMPARE(connection.credentialStatus(),
             QStringLiteral("secure storage unavailable; password is session-only"));
}

void ConnectionTest::applyDuringCredentialReadMigratesLoadedPassword()
{
    {
        IrcController seedController;
        IrcConnection seed(seedController, capturingFactory(), credentialStore());
        fillCompleteDraft(seed);
        QVERIFY(seed.apply());
    }
    m_transports.clear();

    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Available, QStringLiteral("stored-secret"));
    store->setHoldRead(true);
    IrcConnection connection(controller, capturingFactory(), *store);
    QCOMPARE(connection.credentialState(), CredentialStore::State::Loading);

    connection.setHost(QStringLiteral("irc.changed"));
    QVERIFY(connection.apply());
    QCOMPARE(store->writeCalls(), 0);
    QCOMPARE(m_transports.size(), 0);

    store->completeHeldRead();
    QTRY_COMPARE(store->writeCalls(), 1);
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Available);
    QCOMPARE(store->writtenPassword(), QStringLiteral("stored-secret"));
    QTRY_COMPARE(store->removeCalls(), 1);
    QCOMPARE(m_transports.size(), 1);
    QCOMPARE(store->writtenKeys().constLast().host, QStringLiteral("irc.changed"));
    QCOMPARE(store->removedKeys().constLast().host, QStringLiteral("irc.example"));
}

void ConnectionTest::lateCredentialReadDoesNotReplaceEditedPassword()
{
    {
        IrcController seedController;
        IrcConnection seed(seedController, capturingFactory(), credentialStore());
        fillCompleteDraft(seed);
        QVERIFY(seed.apply());
    }
    m_transports.clear();

    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Available, QStringLiteral("old-secret"));
    store->setHoldRead(true);
    IrcConnection connection(controller, capturingFactory(), *store);
    QCOMPARE(connection.credentialState(), CredentialStore::State::Loading);

    connection.setPassword(QStringLiteral("new-secret"));
    QVERIFY(connection.apply());
    QTRY_COMPARE(store->writeCalls(), 1);
    QCOMPARE(store->writtenPassword(), QStringLiteral("new-secret"));
    QCOMPARE(m_transports.size(), 1);

    store->completeHeldRead();
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Available);
    QCOMPARE(store->writeCalls(), 1);
    QCOMPARE(store->writtenPassword(), QStringLiteral("new-secret"));
    QCOMPARE(m_transports.size(), 1);
}

void ConnectionTest::forgetDuringKeyMigrationStillRemovesPreviousKey()
{
    {
        IrcController seedController;
        IrcConnection seed(seedController, capturingFactory(), credentialStore());
        fillCompleteDraft(seed);
        QVERIFY(seed.apply());
    }

    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Available, QStringLiteral("stored-secret"));
    IrcConnection connection(controller, capturingFactory(), *store);
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Available);

    connection.setHost(QStringLiteral("irc.changed"));
    QVERIFY(connection.apply());
    connection.forgetPassword();
    connection.removeStoredPassword();

    QTRY_COMPARE(store->writeCalls(), 1);
    QTRY_COMPARE(store->removeCalls(), 2);
}

void ConnectionTest::startupActivationRecoversAfterCredentialError()
{
    {
        IrcController seedController;
        IrcConnection seed(seedController, capturingFactory(), credentialStore());
        fillCompleteDraft(seed);
        seed.setConnectOnStartup(true);
        QVERIFY(seed.apply());
    }
    m_transports.clear();

    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Error);
    IrcConnection connection(controller, capturingFactory(), *store);

    connection.activateOnStartup();
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Error);
    QCOMPARE(m_transports.size(), 0);

    connection.activateOnStartup();
    QCOMPARE(m_transports.size(), 0);

    connection.activate();
    QCOMPARE(m_transports.size(), 1);
}

void ConnectionTest::credentialErrorWithoutPasswordIsNotReportedAsSessionOnly()
{
    {
        IrcController seedController;
        IrcConnection seed(seedController, capturingFactory(), credentialStore());
        fillCompleteDraft(seed);
        QVERIFY(seed.apply());
    }

    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Error);
    IrcConnection connection(controller, capturingFactory(), *store);
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Error);
    QVERIFY(!connection.passwordSet());
    QCOMPARE(connection.credentialStatus(),
             QStringLiteral("secure storage error; password is not saved"));
}

void ConnectionTest::applyDuringCredentialReadConnectsOnceAfterLoad()
{
    {
        IrcController seedController;
        IrcConnection seed(seedController, capturingFactory(), credentialStore());
        fillCompleteDraft(seed);
        QVERIFY(seed.apply());
    }
    m_transports.clear();

    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Available, QStringLiteral("stored-secret"));
    store->setHoldRead(true);
    IrcConnection connection(controller, capturingFactory(), *store);
    QCOMPARE(connection.credentialState(), CredentialStore::State::Loading);

    QVERIFY(connection.apply());
    QCOMPARE(m_transports.size(), 0);

    store->completeHeldRead();
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Available);
    QCOMPARE(m_transports.size(), 1);
    QVERIFY(connection.passwordSet());
}

void ConnectionTest::forgetFailureKeepsForgetAvailable()
{
    {
        IrcController seedController;
        IrcConnection seed(seedController, capturingFactory(), credentialStore());
        fillCompleteDraft(seed);
        QVERIFY(seed.apply());
    }

    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Available, QStringLiteral("stored-secret"),
        CredentialStore::State::Error, QStringLiteral("remove failed"));
    IrcConnection connection(controller, capturingFactory(), *store);
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Available);
    QVERIFY(connection.canForgetPassword());

    connection.forgetPassword();
    connection.removeStoredPassword();
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Error);
    QVERIFY(connection.canForgetPassword());

    connection.removeStoredPassword();
    QTRY_COMPARE(store->removeCalls(), 2);
}

void ConnectionTest::staleReadAfterKeyChangeStillRemovesObsoleteKey()
{
    {
        IrcController seedController;
        IrcConnection seed(seedController, capturingFactory(), credentialStore());
        fillCompleteDraft(seed);
        QVERIFY(seed.apply());
    }
    m_transports.clear();

    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Available, QStringLiteral("old-secret"));
    store->setHoldRead(true);
    IrcConnection connection(controller, capturingFactory(), *store);
    QCOMPARE(connection.credentialState(), CredentialStore::State::Loading);

    connection.setHost(QStringLiteral("irc.changed"));
    QVERIFY(connection.apply());
    QCOMPARE(m_transports.size(), 0);

    connection.setPassword(QStringLiteral("new-secret"));
    QVERIFY(connection.apply());
    QTRY_COMPARE(store->writeCalls(), 1);
    QCOMPARE(store->writtenPassword(), QStringLiteral("new-secret"));
    QCOMPARE(store->writtenKeys().constLast().host, QStringLiteral("irc.changed"));
    QCOMPARE(m_transports.size(), 1);

    store->completeHeldRead();
    QTRY_COMPARE(store->removeCalls(), 1);
    QCOMPARE(store->removedKeys().constLast().host, QStringLiteral("irc.example"));
    QCOMPARE(m_transports.size(), 1);
}

void ConnectionTest::applyReportsWriteFailure()
{
    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Missing);
    store->setWriteResult(CredentialStore::State::Error,
                          QStringLiteral("write failed"));
    IrcConnection connection(controller, capturingFactory(), *store);
    fillCompleteDraft(connection);
    connection.setPassword(QStringLiteral("session-secret"));
    QVERIFY(connection.apply());
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Error);
    QCOMPARE(connection.credentialError(), QStringLiteral("write failed"));
    QVERIFY(connection.passwordSet());
    QCOMPARE(connection.credentialStatus(),
             QStringLiteral("secure storage error; password is session-only"));
}

void ConnectionTest::obsoleteKeyRemovalFailureIsRetriedOnApply()
{
    {
        IrcController seedController;
        IrcConnection seed(seedController, capturingFactory(), credentialStore());
        fillCompleteDraft(seed);
        QVERIFY(seed.apply());
    }

    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Available, QStringLiteral("stored-secret"),
        CredentialStore::State::Error, QStringLiteral("remove failed"));
    IrcConnection connection(controller, capturingFactory(), *store);
    fillCompleteDraft(connection);
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Available);

    connection.setHost(QStringLiteral("irc.changed"));
    QVERIFY(connection.apply());
    QTRY_COMPARE(store->writeCalls(), 1);
    QTRY_COMPARE(store->removeCalls(), 1);
    QCOMPARE(store->removedKeys().constLast().host, QStringLiteral("irc.example"));
    QTRY_COMPARE(connection.credentialError(), QStringLiteral("remove failed"));
    QVERIFY(connection.passwordSet());

    store->setRemoveResult(CredentialStore::State::Missing);
    QVERIFY(connection.apply());
    QTRY_COMPARE(store->removeCalls(), 2);
    QCOMPARE(store->removedKeys().constLast().host, QStringLiteral("irc.example"));
    QCOMPARE(store->writeCalls(), 1);
}

void ConnectionTest::failedWriteEmptyApplyKeepsSessionPassword()
{
    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Missing);
    store->setWriteResult(CredentialStore::State::Error,
                          QStringLiteral("write failed"));
    IrcConnection connection(controller, capturingFactory(), *store);
    fillCompleteDraft(connection);
    connection.setPassword(QStringLiteral("session-secret"));
    QVERIFY(connection.apply());
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Error);
    QVERIFY(connection.passwordSet());

    connection.setPassword(QString());
    QVERIFY(connection.passwordSet());
    QVERIFY(connection.apply());
    QCOMPARE(store->removeCalls(), 0);
    QTRY_COMPARE(store->writeCalls(), 2);
    QVERIFY(connection.passwordSet());
}

int runConnectionTests(int argc, char **argv)
{
    ConnectionTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_connection.moc"
