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
#include "conversationlistmodel.h"
#include "ircprofilestore.h"

#include <memory>
#include <utility>
#include <vector>

class FakeCredentialStore;

namespace
{
QByteArray decodedSaslPayload(const QByteArray &frame)
{
    const qsizetype prefix = qsizetype(sizeof("AUTHENTICATE ") - 1);
    const qsizetype trailer = qsizetype(sizeof("\r\n") - 1);
    return QByteArray::fromBase64(frame.mid(prefix, frame.size() - prefix - trailer));
}
}

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
    void discardAfterAddRestoresPreviousNetwork();
    void discardOnFirstRunRestoresSuggested();
    void tlsSwitchLeavesPortAlone();
    void authenticationFailureFocusesPassword();
    void applyWritesPasswordToStoreNotSettings();
    void credentialStoreLoadsPasswordAsynchronously();
    void startupActivationWaitsForCredentialRead();
    void startupActivationWaitsForUsableCredentialState();
    void startupActivationDoesNotConnectAfterCredentialError();
    void startupSkipsWhenSavedSecretIsUnavailable();
    void startupSkipsWhenSavedSecretIsMissing();
    void missingReadKeepsSecretSaved();
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
    void typedPasswordUnblocksStaleCredentialRead();
    void obsoleteKeyRemovalKeepsCurrentSecretAvailable();
    void failedMigrationWriteRetriesBeforeObsoleteDelete();
    void discardRestoresTypedPassword();
    void sessionOnlyPasswordCanBeForgotten();
    void discardAfterFailedWriteKeepsError();
    void discardCompensatesPendingPasswordWrite();
    void discardCompensatesPendingFirstPasswordWrite();
    void discardCompensatesPendingMigrationWrite();
    void staleWriteErrorIsNotOverwrittenByRead();
    void discardDuringLoadAdoptsStoredPassword();
    void discardDuringReadDoesNotDeleteStoredPassword();
    void failedMigrationDiscardRetriesWriteBeforeObsoleteDelete();
    void accountAndBouncerNetworkLoginAsOneName();
    void twoProfilesApplyIndependently();
    void removeSelectedDropsSessionAndStore();
    void removeSelectedDeletesStoredSecret();
    void usernameChangePersistsExistingPassword();
    void nickChangeKeepsKeyWhenUsernameIsSet();
    void nickChangePersistsExistingPasswordWhenUsernameIsEmpty();
    void activateStartupStartsEveryMarkedProfile();
    void passwordsStayIsolatedPerNetwork();
    void authenticationFailureDoesNotReplaceDirtyDraft();
    void forgetNickServLeavesPassword();
    void forgetPasswordLeavesNickServ();
    void hostChangeMigratesBothSecrets();
    void unavailableStoreNamesBothSessionOnlySecrets();
    void startupSkipsWhenSavedNickServIsMissing();
    void startupFocusesPasswordBeforeNickServWhenBothMissing();

private:
    IrcConnection::TransportFactory capturingFactory();
    CredentialStore &credentialStore();
    void fillCompleteDraft(IrcConnection &connection,
                           const QString &host = QStringLiteral("irc.example"));

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
        , m_nickServReadState(readState)
        , m_password(std::move(password))
        , m_removeState(removeState)
        , m_removeMessage(std::move(removeMessage))
    {
    }

    void read(const CredentialKey &key) override
    {
        emit readFinished(State::Loading, {}, {});
        if (m_holdRead && key.purpose.isEmpty())
            return;
        QMetaObject::invokeMethod(this, [this, key]() {
            emitHeldRead(key);
        }, Qt::QueuedConnection);
    }

    void write(const CredentialKey &key, const QString &password) override
    {
        m_writtenKeys.append(key);
        if (key.purpose.isEmpty())
            m_writtenPassword = password;
        else
            m_writtenNickServPassword = password;
        ++m_writeCalls;
        if (m_holdWrite) {
            m_heldWrite = true;
            return;
        }
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
    void setHoldWrite(bool hold) { m_holdWrite = hold; }
    void completeHeldWrite()
    {
        m_holdWrite = false;
        if (!m_heldWrite)
            return;
        m_heldWrite = false;
        emit writeFinished(m_writeState, m_writeMessage);
    }
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
        emitHeldRead({});
    }

    void setNickServRead(State state, QString password = {})
    {
        m_nickServReadState = state;
        m_nickServPassword = std::move(password);
    }

    QString writtenNickServPassword() const { return m_writtenNickServPassword; }

private:
    void emitHeldRead(const CredentialKey &key = {})
    {
        if (!key.purpose.isEmpty()) {
            if (m_nickServReadState == State::Available && m_nickServPassword.isEmpty())
                emit readFinished(State::Missing, {}, {});
            else
                emit readFinished(m_nickServReadState, m_nickServPassword, {});
            return;
        }
        if (m_readState == State::Available && m_password.isEmpty())
            emit readFinished(State::Missing, {}, {});
        else
            emit readFinished(m_readState, m_password, {});
    }

    State m_readState;
    State m_nickServReadState = State::Missing;
    QString m_password;
    QString m_nickServPassword;
    QString m_writtenPassword;
    QString m_writtenNickServPassword;
    QList<CredentialKey> m_writtenKeys;
    QList<CredentialKey> m_removedKeys;
    State m_removeState;
    QString m_removeMessage;
    State m_writeState = State::Available;
    QString m_writeMessage;
    int m_writeCalls = 0;
    int m_removeCalls = 0;
    bool m_holdRead = false;
    bool m_holdWrite = false;
    bool m_heldWrite = false;
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

void ConnectionTest::fillCompleteDraft(IrcConnection &connection, const QString &host)
{
    connection.setHost(host);
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

void ConnectionTest::discardAfterAddRestoresPreviousNetwork()
{
    IrcController controller;
    IrcConnection connection(controller, capturingFactory(), credentialStore());
    fillCompleteDraft(connection, QStringLiteral("irc.example"));
    QVERIFY(connection.apply());
    const QString firstId = connection.selectedNetworkId();

    QVERIFY(connection.add());
    fillCompleteDraft(connection, QStringLiteral("irc.oftc.net"));
    connection.setNick(QStringLiteral("oak"));
    QVERIFY(connection.apply());
    QCOMPARE(connection.networks()->rowCount(), 2);

    connection.select(firstId);
    QCOMPARE(connection.selectedNetworkId(), firstId);
    QVERIFY(connection.add());
    QCOMPARE(connection.networks()->rowCount(), 3);
    QCOMPARE(connection.host(), QString());
    QVERIFY(connection.dirty());
    QVERIFY(!connection.canAdd());

    connection.discard();
    QCOMPARE(connection.selectedNetworkId(), firstId);
    QCOMPARE(connection.host(), QStringLiteral("irc.example"));
    QCOMPARE(connection.nick(), QStringLiteral("omairc"));
    QCOMPARE(connection.networks()->rowCount(), 2);
    QVERIFY(!connection.dirty());
    QVERIFY(connection.canAdd());
}

void ConnectionTest::discardOnFirstRunRestoresSuggested()
{
    IrcController controller;
    IrcConnection connection(controller, capturingFactory(), credentialStore());
    QVERIFY(connection.setupRequired());
    QCOMPARE(connection.host(), QStringLiteral("irc.libera.chat"));
    connection.setHost(QStringLiteral("irc.changed"));
    connection.setAutojoin(QString());
    connection.discard();
    QCOMPARE(connection.host(), QStringLiteral("irc.libera.chat"));
    QCOMPARE(connection.autojoin(), QStringLiteral("#omarchy"));
    QVERIFY(connection.setupRequired());
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
    QTRY_VERIFY(IrcProfileStore().profiles().first().secretSaved);

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

    IrcNetworkProfile stored = IrcProfileStore().profiles().first();
    stored.secretSaved = false;
    IrcProfileStore().save(stored);

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

void ConnectionTest::startupSkipsWhenSavedSecretIsUnavailable()
{
    {
        IrcController seedController;
        IrcConnection seed(seedController, capturingFactory(), credentialStore());
        fillCompleteDraft(seed);
        seed.setConnectOnStartup(true);
        seed.setPassword(QStringLiteral("stored-secret"));
        QVERIFY(seed.apply());
        QTRY_VERIFY(IrcProfileStore().profiles().first().secretSaved);
    }
    m_transports.clear();

    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Unavailable);
    IrcConnection connection(controller, capturingFactory(), *store);
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Unavailable);
    QVERIFY(!connection.activateStartup());
    QCOMPARE(m_transports.size(), 0);
    QVERIFY(connection.focusPassword());
}

void ConnectionTest::startupSkipsWhenSavedSecretIsMissing()
{
    {
        IrcController seedController;
        IrcConnection seed(seedController, capturingFactory(), credentialStore());
        fillCompleteDraft(seed);
        seed.setConnectOnStartup(true);
        seed.setPassword(QStringLiteral("stored-secret"));
        QVERIFY(seed.apply());
        QTRY_VERIFY(IrcProfileStore().profiles().first().secretSaved);
    }
    m_transports.clear();

    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Missing);
    IrcConnection connection(controller, capturingFactory(), *store);
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Missing);
    QVERIFY(!connection.activateStartup());
    QCOMPARE(m_transports.size(), 0);
    QVERIFY(connection.focusPassword());
}

void ConnectionTest::missingReadKeepsSecretSaved()
{
    {
        IrcController seedController;
        IrcConnection seed(seedController, capturingFactory(), credentialStore());
        fillCompleteDraft(seed);
        seed.setPassword(QStringLiteral("stored-secret"));
        QVERIFY(seed.apply());
        QTRY_VERIFY(IrcProfileStore().profiles().first().secretSaved);
    }

    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Missing);
    IrcConnection connection(controller, capturingFactory(), *store);
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Missing);
    QCOMPARE(IrcProfileStore().profiles().first().secretSaved, true);
    QVERIFY(!connection.passwordSet());
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
    QTRY_COMPARE(IrcProfileStore().profiles().first().secretSaved, false);
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
    const int writesBeforeResave = store->writeCalls();
    QVERIFY(connection.apply());
    QTRY_COMPARE(store->writeCalls(), writesBeforeResave + 1);
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
    QCOMPARE(connection.credentialStatus(),
             QStringLiteral("could not remove the previous saved password"));
    QVERIFY(connection.passwordSet());

    store->setRemoveResult(CredentialStore::State::Missing);
    QVERIFY(connection.apply());
    QTRY_COMPARE(store->removeCalls(), 2);
    QCOMPARE(store->removedKeys().constLast().host, QStringLiteral("irc.example"));
    QCOMPARE(store->writeCalls(), 1);
    QTRY_COMPARE(connection.credentialError(), QString());
    QCOMPARE(connection.credentialStatus(), QStringLiteral("password saved securely"));
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

void ConnectionTest::typedPasswordUnblocksStaleCredentialRead()
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

    connection.setPassword(QStringLiteral("typed-secret"));
    connection.activateOnStartup();
    QCOMPARE(m_transports.size(), 0);

    store->completeHeldRead();
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Missing);
    QVERIFY(connection.passwordSet());
    QCOMPARE(connection.credentialStatus(),
             QStringLiteral("password changed; apply to save securely"));
    QTRY_COMPARE(m_transports.size(), 1);
}

void ConnectionTest::obsoleteKeyRemovalKeepsCurrentSecretAvailable()
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

    connection.setHost(QStringLiteral("irc.changed"));
    QVERIFY(connection.apply());
    QTRY_COMPARE(store->writeCalls(), 1);
    QTRY_COMPARE(store->removeCalls(), 1);
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Available);
    QVERIFY(connection.canForgetPassword());
    QCOMPARE(connection.credentialStatus(), QStringLiteral("password saved securely"));
}

void ConnectionTest::failedMigrationWriteRetriesBeforeObsoleteDelete()
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
    store->setWriteResult(CredentialStore::State::Error,
                          QStringLiteral("write failed"));
    IrcConnection connection(controller, capturingFactory(), *store);
    fillCompleteDraft(connection);
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Available);

    connection.setHost(QStringLiteral("irc.changed"));
    QVERIFY(connection.apply());
    QTRY_COMPARE(store->writeCalls(), 1);
    QCOMPARE(store->removeCalls(), 0);
    QVERIFY(connection.passwordSet());
    QTRY_COMPARE(connection.credentialStatus(),
                 QStringLiteral("secure storage error; password is session-only"));

    store->setWriteResult(CredentialStore::State::Available);
    QVERIFY(connection.apply());
    QTRY_COMPARE(store->writeCalls(), 2);
    QTRY_COMPARE(store->removeCalls(), 1);
    QCOMPARE(store->writtenKeys().constLast().host, QStringLiteral("irc.changed"));
    QCOMPARE(store->removedKeys().constLast().host, QStringLiteral("irc.example"));
}

void ConnectionTest::discardRestoresTypedPassword()
{
    IrcController controller;
    IrcConnection connection(controller, capturingFactory(), credentialStore());
    fillCompleteDraft(connection);
    QVERIFY(connection.apply());
    connection.setPassword(QStringLiteral("typed-secret"));
    QVERIFY(connection.passwordSet());
    connection.discard();
    QVERIFY(!connection.passwordSet());
    QVERIFY(connection.apply());
    QCOMPARE(m_credentialStores.back()->writeCalls(), 0);
}

void ConnectionTest::sessionOnlyPasswordCanBeForgotten()
{
    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Unavailable);
    IrcConnection connection(controller, capturingFactory(), *store);
    fillCompleteDraft(connection);
    connection.setPassword(QStringLiteral("session-secret"));
    QVERIFY(connection.canForgetPassword());
    connection.forgetPassword();
    QVERIFY(!connection.passwordSet());
    QVERIFY(!connection.canForgetPassword());
}

void ConnectionTest::discardAfterFailedWriteKeepsError()
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

    connection.discard();
    QVERIFY(!connection.passwordSet());
    QCOMPARE(connection.credentialState(), CredentialStore::State::Error);
    QCOMPARE(connection.credentialStatus(),
             QStringLiteral("secure storage error; password is not saved"));
}

void ConnectionTest::discardCompensatesPendingPasswordWrite()
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
    store->setHoldWrite(true);
    IrcConnection connection(controller, capturingFactory(), *store);
    fillCompleteDraft(connection);
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Available);

    connection.setPassword(QStringLiteral("discarded-secret"));
    QVERIFY(connection.apply());
    QCOMPARE(store->writeCalls(), 1);
    QCOMPARE(store->writtenPassword(), QStringLiteral("discarded-secret"));

    connection.discard();
    QVERIFY(connection.passwordSet());
    QCOMPARE(store->writeCalls(), 1);

    store->completeHeldWrite();
    QTRY_COMPARE(store->writeCalls(), 2);
    QCOMPARE(store->writtenPassword(), QStringLiteral("stored-secret"));
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Available);
    QCOMPARE(connection.credentialStatus(), QStringLiteral("password saved securely"));
}

void ConnectionTest::discardCompensatesPendingFirstPasswordWrite()
{
    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Missing);
    store->setHoldWrite(true);
    IrcConnection connection(controller, capturingFactory(), *store);
    fillCompleteDraft(connection);
    connection.setPassword(QStringLiteral("discarded-secret"));
    QVERIFY(connection.apply());
    QCOMPARE(store->writeCalls(), 1);
    QCOMPARE(store->writtenPassword(), QStringLiteral("discarded-secret"));

    connection.discard();
    QVERIFY(!connection.passwordSet());
    QCOMPARE(store->removeCalls(), 0);

    store->completeHeldWrite();
    QTRY_COMPARE(store->removeCalls(), 1);
    QVERIFY(!connection.passwordSet());
}

void ConnectionTest::discardCompensatesPendingMigrationWrite()
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
    store->setHoldWrite(true);
    IrcConnection connection(controller, capturingFactory(), *store);
    fillCompleteDraft(connection);
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Available);

    connection.setHost(QStringLiteral("irc.changed"));
    connection.setPassword(QStringLiteral("discarded-secret"));
    QVERIFY(connection.apply());
    QCOMPARE(store->writeCalls(), 1);
    QCOMPARE(store->writtenPassword(), QStringLiteral("discarded-secret"));
    QCOMPARE(store->writtenKeys().constLast().host, QStringLiteral("irc.changed"));

    connection.discard();
    QCOMPARE(connection.host(), QStringLiteral("irc.changed"));
    QVERIFY(connection.passwordSet());
    QCOMPARE(store->writeCalls(), 1);

    store->completeHeldWrite();
    QTRY_COMPARE(store->writeCalls(), 2);
    QCOMPARE(store->writtenPassword(), QStringLiteral("stored-secret"));
    QCOMPARE(store->writtenKeys().constLast().host, QStringLiteral("irc.changed"));
    QTRY_COMPARE(store->removeCalls(), 1);
    QCOMPARE(store->removedKeys().constLast().host, QStringLiteral("irc.example"));
}

void ConnectionTest::staleWriteErrorIsNotOverwrittenByRead()
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
    store->setHoldRead(true);
    store->setWriteResult(CredentialStore::State::Error,
                          QStringLiteral("write failed"));
    IrcConnection connection(controller, capturingFactory(), *store);
    QCOMPARE(connection.credentialState(), CredentialStore::State::Loading);

    connection.setPassword(QStringLiteral("new-secret"));
    QVERIFY(connection.apply());
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Error);
    QCOMPARE(connection.credentialError(), QStringLiteral("write failed"));

    store->completeHeldRead();
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Error);
    QCOMPARE(connection.credentialError(), QStringLiteral("write failed"));
    QCOMPARE(connection.credentialStatus(),
             QStringLiteral("secure storage error; password is session-only"));
}

void ConnectionTest::discardDuringLoadAdoptsStoredPassword()
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
    store->setHoldRead(true);
    IrcConnection connection(controller, capturingFactory(), *store);
    QCOMPARE(connection.credentialState(), CredentialStore::State::Loading);

    connection.setPassword(QStringLiteral("typed-secret"));
    QVERIFY(connection.passwordSet());
    connection.discard();
    QVERIFY(!connection.passwordSet());

    store->completeHeldRead();
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Available);
    QVERIFY(connection.passwordSet());
    QCOMPARE(connection.credentialStatus(), QStringLiteral("password saved securely"));
    QCOMPARE(store->writeCalls(), 0);
}

void ConnectionTest::discardDuringReadDoesNotDeleteStoredPassword()
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
    store->setHoldRead(true);
    store->setHoldWrite(true);
    IrcConnection connection(controller, capturingFactory(), *store);
    QCOMPARE(connection.credentialState(), CredentialStore::State::Loading);

    connection.setPassword(QStringLiteral("discarded-secret"));
    QVERIFY(connection.apply());
    QCOMPARE(store->writeCalls(), 1);
    QCOMPARE(store->writtenPassword(), QStringLiteral("discarded-secret"));

    connection.discard();
    QCOMPARE(store->removeCalls(), 0);

    store->completeHeldWrite();
    QCOMPARE(store->removeCalls(), 0);

    store->completeHeldRead();
    QTRY_COMPARE(store->writeCalls(), 2);
    QCOMPARE(store->writtenPassword(), QStringLiteral("stored-secret"));
    QCOMPARE(store->removeCalls(), 0);
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Available);
    QVERIFY(connection.passwordSet());
}

void ConnectionTest::failedMigrationDiscardRetriesWriteBeforeObsoleteDelete()
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
    store->setWriteResult(CredentialStore::State::Error,
                          QStringLiteral("write failed"));
    IrcConnection connection(controller, capturingFactory(), *store);
    fillCompleteDraft(connection);
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Available);

    connection.setHost(QStringLiteral("irc.changed"));
    QVERIFY(connection.apply());
    QTRY_COMPARE(store->writeCalls(), 1);
    QCOMPARE(store->removeCalls(), 0);
    QTRY_COMPARE(connection.credentialStatus(),
                 QStringLiteral("secure storage error; password is session-only"));

    connection.discard();
    store->setWriteResult(CredentialStore::State::Available);
    QVERIFY(connection.apply());
    QTRY_COMPARE(store->writeCalls(), 2);
    QTRY_COMPARE(store->removeCalls(), 1);
    QCOMPARE(store->writtenKeys().constLast().host, QStringLiteral("irc.changed"));
    QCOMPARE(store->writtenPassword(), QStringLiteral("stored-secret"));
    QCOMPARE(store->removedKeys().constLast().host, QStringLiteral("irc.example"));
}

void ConnectionTest::accountAndBouncerNetworkLoginAsOneName()
{
    IrcController controller;
    IrcConnection connection(controller, capturingFactory());
    fillCompleteDraft(connection);
    connection.setAccount(QStringLiteral("joe"));
    connection.setBouncerNetwork(QStringLiteral("libera"));
    connection.setPassword(QStringLiteral("super-secret"));
    QVERIFY(connection.apply());
    QCOMPARE(m_transports.size(), 1);

    m_transports.last()->completeConnect();
    m_transports.last()->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :sasl=PLAIN\r\n"
                          ":server CAP omairc ACK :sasl\r\n"
                          "AUTHENTICATE +\r\n"));
    QCOMPARE(decodedSaslPayload(m_transports.last()->writtenFrames().last()),
             QByteArray("joe/libera\0joe/libera\0super-secret", 34));

    const IrcNetworkProfile stored = IrcProfileStore().profiles().first();
    QCOMPARE(stored.account, QStringLiteral("joe"));
    QCOMPARE(stored.bouncerNetwork, QStringLiteral("libera"));
    QCOMPARE(stored.saslAccount(), QStringLiteral("joe/libera"));

    QSettings settings;
    QFile file(settings.fileName());
    QVERIFY(file.open(QIODevice::ReadOnly | QIODevice::Text));
    const QString contents = QString::fromUtf8(file.readAll());
    QVERIFY(!contents.contains(QLatin1String("password"), Qt::CaseInsensitive));
    QVERIFY(!contents.contains(QLatin1String("super-secret")));
}

void ConnectionTest::twoProfilesApplyIndependently()
{
    IrcController controller;
    IrcConnection connection(controller, capturingFactory(), credentialStore());
    fillCompleteDraft(connection, QStringLiteral("irc.example"));
    QVERIFY(connection.apply());
    QCOMPARE(m_transports.size(), 1);
    FakeIrcTransport *first = m_transports.first();

    QVERIFY(connection.canAdd());
    QVERIFY(connection.add());
    fillCompleteDraft(connection, QStringLiteral("irc.oftc.net"));
    connection.setNick(QStringLiteral("oak"));
    QVERIFY(connection.apply());
    QCOMPARE(m_transports.size(), 2);
    QCOMPARE(m_transports.first(), first);
    QCOMPARE(first->connectionState(), IrcTransport::ConnectionState::Connecting);
    QCOMPARE(m_transports.last()->connectionState(),
             IrcTransport::ConnectionState::Connecting);
    QCOMPARE(connection.networks()->rowCount(), 2);
}

void ConnectionTest::removeSelectedDropsSessionAndStore()
{
    IrcController controller;
    IrcConnection connection(controller, capturingFactory(), credentialStore());
    fillCompleteDraft(connection, QStringLiteral("irc.example"));
    QVERIFY(connection.apply());
    QVERIFY(connection.add());
    fillCompleteDraft(connection, QStringLiteral("irc.oftc.net"));
    connection.setNick(QStringLiteral("oak"));
    QVERIFY(connection.apply());

    const QString oftcId = connection.selectedNetworkId();
    QCOMPARE(m_transports.size(), 2);
    m_transports.at(0)->completeConnect();
    m_transports.at(0)->injectBytes(
        QByteArrayLiteral(":server CAP omairc LS :multi-prefix\r\n"
                          ":server 001 omairc :Welcome\r\n"
                          ":omairc!u@h JOIN :#chan\r\n"));
    m_transports.at(1)->completeConnect();
    m_transports.at(1)->injectBytes(
        QByteArrayLiteral(":server CAP oak LS :multi-prefix\r\n"
                          ":server 001 oak :Welcome\r\n"
                          ":oak!u@h JOIN :#lab\r\n"));
    controller.selectConversation(oftcId, QStringLiteral("#lab"));
    QCOMPARE(controller.conversations()->rowCount(), 2);

    QVERIFY(connection.removeSelected());
    QCOMPARE(connection.networks()->rowCount(), 1);
    QCOMPARE(controller.session(oftcId), nullptr);
    QCOMPARE(controller.conversations()->rowCount(), 1);
    QCOMPARE(controller.conversations()->data(
                 controller.conversations()->index(0, 0),
                 ConversationListModel::ConversationRole),
             QStringLiteral("#chan"));
    QCOMPARE(IrcProfileStore().profiles().size(), 1);
    QCOMPARE(IrcProfileStore().profiles().first().host,
             QStringLiteral("irc.example"));
}

void ConnectionTest::removeSelectedDeletesStoredSecret()
{
    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Missing);
    IrcConnection connection(controller, capturingFactory(), *store);
    fillCompleteDraft(connection, QStringLiteral("irc.example"));
    QVERIFY(connection.apply());
    QVERIFY(connection.add());
    fillCompleteDraft(connection, QStringLiteral("irc.oftc.net"));
    connection.setNick(QStringLiteral("oak"));
    connection.setPassword(QStringLiteral("oftc-secret"));
    QVERIFY(connection.apply());
    QTRY_COMPARE(store->writeCalls(), 1);
    QCOMPARE(store->writtenKeys().constLast().host, QStringLiteral("irc.oftc.net"));
    const QString oftcId = connection.selectedNetworkId();
    QVERIFY(connection.removeSelected());
    QTRY_COMPARE(store->removeCalls(), 2);
    QCOMPARE(store->removedKeys().constFirst().networkId, oftcId);
    QCOMPARE(store->removedKeys().constFirst().host, QStringLiteral("irc.oftc.net"));
    QCOMPARE(store->removedKeys().constFirst().purpose, QString());
    QCOMPARE(store->removedKeys().constLast().purpose, QStringLiteral("nickserv"));
    QCOMPARE(store->removedKeys().constLast().networkId, oftcId);
}

void ConnectionTest::usernameChangePersistsExistingPassword()
{
    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Missing);
    IrcConnection connection(controller, capturingFactory(), *store);
    fillCompleteDraft(connection);
    connection.setPassword(QStringLiteral("stored-secret"));
    QVERIFY(connection.apply());
    QTRY_COMPARE(store->writeCalls(), 1);
    QCOMPARE(store->writtenKeys().constFirst().username, QStringLiteral("omairc"));

    connection.setUsername(QStringLiteral("other"));
    QVERIFY(connection.apply());
    QTRY_COMPARE(store->writeCalls(), 2);
    QTRY_COMPARE(store->removeCalls(), 1);
    QCOMPARE(store->writtenPassword(), QStringLiteral("stored-secret"));
    QCOMPARE(store->writtenKeys().constLast().username, QStringLiteral("other"));
    QCOMPARE(store->removedKeys().constLast().username, QStringLiteral("omairc"));
}

void ConnectionTest::nickChangeKeepsKeyWhenUsernameIsSet()
{
    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Missing);
    IrcConnection connection(controller, capturingFactory(), *store);
    fillCompleteDraft(connection);
    connection.setPassword(QStringLiteral("stored-secret"));
    QVERIFY(connection.apply());
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Available);
    QTRY_COMPARE(store->writeCalls(), 1);

    connection.setNick(QStringLiteral("oak"));
    QVERIFY(connection.apply());
    QCoreApplication::processEvents();
    QCOMPARE(store->writeCalls(), 1);
    QCOMPARE(store->removeCalls(), 0);
}

void ConnectionTest::nickChangePersistsExistingPasswordWhenUsernameIsEmpty()
{
    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Missing);
    IrcConnection connection(controller, capturingFactory(), *store);
    fillCompleteDraft(connection);
    connection.setUsername(QString());
    connection.setPassword(QStringLiteral("stored-secret"));
    QVERIFY(connection.apply());
    QTRY_COMPARE(store->writeCalls(), 1);
    QCOMPARE(store->writtenKeys().constFirst().username, QStringLiteral("omairc"));

    connection.setNick(QStringLiteral("oak"));
    QVERIFY(connection.apply());
    QTRY_COMPARE(store->writeCalls(), 2);
    QTRY_COMPARE(store->removeCalls(), 1);
    QCOMPARE(store->writtenPassword(), QStringLiteral("stored-secret"));
    QCOMPARE(store->writtenKeys().constLast().username, QStringLiteral("oak"));
    QCOMPARE(store->removedKeys().constLast().username, QStringLiteral("omairc"));
}

void ConnectionTest::activateStartupStartsEveryMarkedProfile()
{
    {
        IrcController controller;
        IrcConnection connection(controller, capturingFactory(), credentialStore());
        fillCompleteDraft(connection, QStringLiteral("irc.example"));
        connection.setConnectOnStartup(true);
        QVERIFY(connection.apply());
        QVERIFY(connection.add());
        fillCompleteDraft(connection, QStringLiteral("irc.oftc.net"));
        connection.setNick(QStringLiteral("oak"));
        connection.setConnectOnStartup(true);
        QVERIFY(connection.apply());
        QVERIFY(connection.add());
        fillCompleteDraft(connection, QStringLiteral("irc.libera.chat"));
        connection.setNick(QStringLiteral("leaf"));
        connection.setConnectOnStartup(false);
        QVERIFY(connection.apply());
    }
    m_transports.clear();

    IrcController controller;
    IrcConnection connection(controller, capturingFactory(), credentialStore());
    connection.activateStartup();
    QTRY_COMPARE(m_transports.size(), 2);
}

void ConnectionTest::passwordsStayIsolatedPerNetwork()
{
    IrcController controller;
    IrcConnection connection(controller, capturingFactory(), credentialStore());
    fillCompleteDraft(connection, QStringLiteral("irc.example"));
    connection.setPassword(QStringLiteral("alpha-secret"));
    QVERIFY(connection.apply());
    QVERIFY(connection.passwordSet());

    QVERIFY(connection.add());
    fillCompleteDraft(connection, QStringLiteral("irc.oftc.net"));
    connection.setNick(QStringLiteral("oak"));
    QVERIFY(!connection.passwordSet());
    connection.setPassword(QStringLiteral("beta-secret"));
    QVERIFY(connection.apply());
    QVERIFY(connection.passwordSet());

    connection.select(IrcProfileStore().profiles().first().networkId);
    QVERIFY(connection.passwordSet());
}

void ConnectionTest::authenticationFailureDoesNotReplaceDirtyDraft()
{
    IrcController controller;
    IrcConnection connection(controller, capturingFactory(), credentialStore());
    fillCompleteDraft(connection, QStringLiteral("irc.example"));
    connection.setPassword(QStringLiteral("alpha-secret"));
    QVERIFY(connection.apply());
    const QString firstId = connection.selectedNetworkId();

    QVERIFY(connection.add());
    fillCompleteDraft(connection, QStringLiteral("irc.oftc.net"));
    connection.setNick(QStringLiteral("oak"));
    connection.setPassword(QStringLiteral("beta-secret"));
    QVERIFY(connection.apply());

    connection.select(firstId);
    QCOMPARE(connection.selectedNetworkId(), firstId);
    connection.setHost(QStringLiteral("irc.changed"));
    QVERIFY(connection.dirty());
    QVERIFY(!connection.focusPassword());

    QCOMPARE(m_transports.size(), 2);
    m_transports.at(1)->completeConnect();
    m_transports.at(1)->injectBytes(
        QByteArrayLiteral(":server CAP oak LS :sasl\r\n"
                          ":server CAP oak ACK :sasl\r\n"
                          ":server 904 oak :SASL failed\r\n"));

    QCOMPARE(connection.selectedNetworkId(), firstId);
    QCOMPARE(connection.host(), QStringLiteral("irc.changed"));
    QVERIFY(connection.dirty());
    QVERIFY(!connection.focusPassword());
}

void ConnectionTest::forgetNickServLeavesPassword()
{
    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Missing);
    IrcConnection connection(controller, capturingFactory(), *store);
    fillCompleteDraft(connection);
    connection.setPassword(QStringLiteral("server-secret"));
    connection.setNickServPassword(QStringLiteral("nickserv-secret"));
    QVERIFY(connection.apply());
    QTRY_COMPARE(store->writeCalls(), 2);
    QCOMPARE(store->writtenPassword(), QStringLiteral("server-secret"));
    QCOMPARE(store->writtenNickServPassword(), QStringLiteral("nickserv-secret"));
    QTRY_VERIFY(IrcProfileStore().profiles().first().secretSaved);
    QTRY_VERIFY(IrcProfileStore().profiles().first().nickServSaved);

    connection.forgetNickServ();
    connection.removeStoredNickServ();
    QTRY_COMPARE(store->removeCalls(), 1);
    QCOMPARE(store->removedKeys().constLast().purpose, QStringLiteral("nickserv"));
    QVERIFY(connection.passwordSet());
    QVERIFY(!connection.nickServSet());
    QTRY_COMPARE(IrcProfileStore().profiles().first().secretSaved, true);
    QTRY_COMPARE(IrcProfileStore().profiles().first().nickServSaved, false);
}

void ConnectionTest::forgetPasswordLeavesNickServ()
{
    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Missing);
    IrcConnection connection(controller, capturingFactory(), *store);
    fillCompleteDraft(connection);
    connection.setPassword(QStringLiteral("server-secret"));
    connection.setNickServPassword(QStringLiteral("nickserv-secret"));
    QVERIFY(connection.apply());
    QTRY_COMPARE(store->writeCalls(), 2);

    connection.forgetPassword();
    connection.removeStoredPassword();
    QTRY_COMPARE(store->removeCalls(), 1);
    QCOMPARE(store->removedKeys().constLast().purpose, QString());
    QVERIFY(!connection.passwordSet());
    QVERIFY(connection.nickServSet());
    QTRY_COMPARE(IrcProfileStore().profiles().first().secretSaved, false);
    QTRY_COMPARE(IrcProfileStore().profiles().first().nickServSaved, true);
}

void ConnectionTest::hostChangeMigratesBothSecrets()
{
    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Missing);
    IrcConnection connection(controller, capturingFactory(), *store);
    fillCompleteDraft(connection);
    connection.setPassword(QStringLiteral("server-secret"));
    connection.setNickServPassword(QStringLiteral("nickserv-secret"));
    QVERIFY(connection.apply());
    QTRY_COMPARE(store->writeCalls(), 2);

    connection.setHost(QStringLiteral("irc.changed"));
    QVERIFY(connection.apply());
    QTRY_COMPARE(store->writeCalls(), 4);
    QTRY_COMPARE(store->removeCalls(), 2);

    bool wrotePassword = false;
    bool wroteNickServ = false;
    bool removedPassword = false;
    bool removedNickServ = false;
    for (const CredentialKey &key : store->writtenKeys()) {
        if (key.host != QStringLiteral("irc.changed"))
            continue;
        if (key.purpose.isEmpty())
            wrotePassword = true;
        else if (key.purpose == QStringLiteral("nickserv"))
            wroteNickServ = true;
    }
    for (const CredentialKey &key : store->removedKeys()) {
        QCOMPARE(key.host, QStringLiteral("irc.example"));
        if (key.purpose.isEmpty())
            removedPassword = true;
        else if (key.purpose == QStringLiteral("nickserv"))
            removedNickServ = true;
    }
    QVERIFY(wrotePassword);
    QVERIFY(wroteNickServ);
    QVERIFY(removedPassword);
    QVERIFY(removedNickServ);
}

void ConnectionTest::unavailableStoreNamesBothSessionOnlySecrets()
{
    {
        IrcController seedController;
        IrcConnection seed(seedController, capturingFactory(), credentialStore());
        fillCompleteDraft(seed);
        QVERIFY(seed.apply());
    }

    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Unavailable);
    IrcConnection connection(controller, capturingFactory(), *store);
    QTRY_COMPARE(connection.credentialStatus(),
                 QStringLiteral("secure storage unavailable"));
    connection.setPassword(QStringLiteral("server-secret"));
    connection.setNickServPassword(QStringLiteral("nickserv-secret"));
    QTRY_COMPARE(connection.credentialStatus(),
                 QStringLiteral(
                     "secure storage unavailable; password and NickServ are session-only"));
}

void ConnectionTest::startupSkipsWhenSavedNickServIsMissing()
{
    IrcNetworkProfile profile = IrcNetworkProfile::create();
    profile.host = QStringLiteral("irc.example");
    profile.nick = QStringLiteral("omairc");
    profile.username = QStringLiteral("omairc");
    profile.realname = QStringLiteral("Omairc User");
    profile.connectOnStartup = true;
    profile.nickServSaved = true;
    IrcProfileStore().save(profile);

    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Missing);
    IrcConnection connection(controller, capturingFactory(), *store);
    QVERIFY(!connection.activateStartup());
    QTRY_VERIFY(connection.focusNickServ());
    QCOMPARE(m_transports.size(), 0);
}

void ConnectionTest::startupFocusesPasswordBeforeNickServWhenBothMissing()
{
    IrcNetworkProfile profile = IrcNetworkProfile::create();
    profile.host = QStringLiteral("irc.example");
    profile.nick = QStringLiteral("omairc");
    profile.username = QStringLiteral("omairc");
    profile.realname = QStringLiteral("Omairc User");
    profile.connectOnStartup = true;
    profile.secretSaved = true;
    profile.nickServSaved = true;
    IrcProfileStore().save(profile);

    IrcController controller;
    auto store = std::make_unique<FakeCredentialStore>(
        CredentialStore::State::Missing);
    IrcConnection connection(controller, capturingFactory(), *store);
    QVERIFY(!connection.activateStartup());
    QTRY_VERIFY(connection.focusPassword());
    QVERIFY(!connection.focusNickServ());
    QCOMPARE(m_transports.size(), 0);
}

int runConnectionTests(int argc, char **argv)
{
    ConnectionTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_connection.moc"
