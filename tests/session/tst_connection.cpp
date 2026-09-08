#include <QCoreApplication>
#include <QFile>
#include <QSettings>
#include <QTemporaryDir>
#include <QTest>

#include "fakeirctransport.h"
#include "credentialstore.h"
#include "ircconnection.h"
#include "irccontroller.h"

#include <memory>

class ConnectionTest : public QObject
{
    Q_OBJECT

private slots:
    void init();
    void setupRequiredUntilCompleteProfileIsSaved();
    void loadingStoredProfileDoesNotConnectUntilActivate();
    void connectOnStartupDraftAppliesAndDiscards();
    void applyIsIdempotentForTheSameSecret();
    void passwordChangeRebuildsTheSession();
    void discardRestoresStoredDraft();
    void tlsSwitchLeavesPortAlone();
    void authenticationFailureFocusesPassword();
    void applyDoesNotWritePassword();
    void credentialStoreLoadsPasswordAsynchronously();
    void unavailableCredentialStoreUsesSessionOnlyState();

private:
    IrcConnection::TransportFactory capturingFactory();
    void fillCompleteDraft(IrcConnection &connection);

    std::unique_ptr<QTemporaryDir> m_dir;
    QList<FakeIrcTransport *> m_transports;
};

class FakeCredentialStore final : public CredentialStore
{
public:
    explicit FakeCredentialStore(State readState, QString password = {},
                                 QObject *parent = nullptr)
        : CredentialStore(parent)
        , m_readState(readState)
        , m_password(std::move(password))
    {
    }

    void read(const CredentialKey &) override
    {
        emit readFinished(State::Loading, {}, {});
        QMetaObject::invokeMethod(this, [this]() {
            emit readFinished(m_readState, m_password, {});
        }, Qt::QueuedConnection);
    }

    void write(const CredentialKey &, const QString &password) override
    {
        m_writtenPassword = password;
        QMetaObject::invokeMethod(this, [this]() {
            emit writeFinished(State::Available, {});
        }, Qt::QueuedConnection);
    }

    QString writtenPassword() const { return m_writtenPassword; }

private:
    State m_readState;
    QString m_password;
    QString m_writtenPassword;
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
}

IrcConnection::TransportFactory ConnectionTest::capturingFactory()
{
    return [this]() {
        auto *transport = new FakeIrcTransport;
        m_transports.append(transport);
        return transport;
    };
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
    IrcConnection connection(controller, capturingFactory());
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
        IrcConnection connection(controller, capturingFactory());
        fillCompleteDraft(connection);
        QVERIFY(connection.apply());
    }
    m_transports.clear();

    IrcController controller;
    IrcConnection connection(controller, capturingFactory());
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
    IrcConnection connection(controller, capturingFactory());
    fillCompleteDraft(connection);
    QVERIFY(!connection.connectOnStartup());
    connection.setConnectOnStartup(true);
    QVERIFY(connection.apply());
    QCOMPARE(connection.connectOnStartup(), true);

    IrcConnection reloaded(controller, capturingFactory());
    QCOMPARE(reloaded.connectOnStartup(), true);
    reloaded.setConnectOnStartup(false);
    reloaded.discard();
    QCOMPARE(reloaded.connectOnStartup(), true);
}

void ConnectionTest::applyIsIdempotentForTheSameSecret()
{
    IrcController controller;
    IrcConnection connection(controller, capturingFactory());
    fillCompleteDraft(connection);
    QVERIFY(connection.apply());
    QVERIFY(connection.apply());
    QCOMPARE(m_transports.size(), 1);
}

void ConnectionTest::passwordChangeRebuildsTheSession()
{
    IrcController controller;
    IrcConnection connection(controller, capturingFactory());
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
    IrcConnection connection(controller, capturingFactory());
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
    IrcConnection connection(controller, capturingFactory());
    QCOMPARE(connection.port(), 6697);
    connection.setTlsEnabled(false);
    QCOMPARE(connection.port(), 6697);
    connection.setTlsEnabled(true);
    QCOMPARE(connection.port(), 6697);
}

void ConnectionTest::authenticationFailureFocusesPassword()
{
    IrcController controller;
    IrcConnection connection(controller, capturingFactory());
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

void ConnectionTest::applyDoesNotWritePassword()
{
    IrcController controller;
    IrcConnection connection(controller, capturingFactory());
    fillCompleteDraft(connection);
    connection.setPassword(QStringLiteral("super-secret"));
    QVERIFY(connection.apply());

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
        IrcConnection connection(controller, capturingFactory(),
                                 []() {
            return new FakeCredentialStore(CredentialStore::State::Available,
                                           QStringLiteral("stored-secret"));
        });
        fillCompleteDraft(connection);
        QVERIFY(connection.apply());
    }

    IrcController controller;
    auto *store = new FakeCredentialStore(CredentialStore::State::Available,
                                          QStringLiteral("stored-secret"));
    IrcConnection connection(controller, capturingFactory(),
                             [store]() { return store; });
    QVERIFY(connection.credentialState() == CredentialStore::State::Loading
            || connection.credentialState() == CredentialStore::State::Available);
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Available);
    QVERIFY(connection.passwordSet());
    QCOMPARE(connection.credentialStatus(), QStringLiteral("password saved securely"));
}

void ConnectionTest::unavailableCredentialStoreUsesSessionOnlyState()
{
    {
        IrcController controller;
        IrcConnection connection(controller, capturingFactory());
        fillCompleteDraft(connection);
        QVERIFY(connection.apply());
    }

    IrcController controller;
    IrcConnection connection(controller, capturingFactory(), []() {
        return new FakeCredentialStore(CredentialStore::State::Unavailable);
    });
    QTRY_COMPARE(connection.credentialState(), CredentialStore::State::Unavailable);
    QVERIFY(!connection.passwordSet());
    QCOMPARE(connection.credentialStatus(),
             QStringLiteral("secure storage unavailable; password is session-only"));
    connection.setPassword(QStringLiteral("session-secret"));
    QVERIFY(connection.passwordSet());
}

int runConnectionTests(int argc, char **argv)
{
    ConnectionTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_connection.moc"
