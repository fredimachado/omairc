#include <QCoreApplication>
#include <QFile>
#include <QSettings>
#include <QTemporaryDir>
#include <QTest>

#include "fakeirctransport.h"
#include "ircconnection.h"
#include "irccontroller.h"
#include "conversationlistmodel.h"
#include "ircprofilestore.h"

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
    void twoProfilesApplyIndependently();
    void removeSelectedDropsSessionAndStore();
    void activateStartupStartsEveryMarkedProfile();
    void passwordsStayIsolatedPerNetwork();
    void authenticationFailureDoesNotReplaceDirtyDraft();

private:
    IrcConnection::TransportFactory capturingFactory();
    void fillCompleteDraft(IrcConnection &connection,
                           const QString &host = QStringLiteral("irc.example"));

    std::unique_ptr<QTemporaryDir> m_dir;
    QList<FakeIrcTransport *> m_transports;
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

void ConnectionTest::twoProfilesApplyIndependently()
{
    IrcController controller;
    IrcConnection connection(controller, capturingFactory());
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
    IrcConnection connection(controller, capturingFactory());
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

void ConnectionTest::activateStartupStartsEveryMarkedProfile()
{
    {
        IrcController controller;
        IrcConnection connection(controller, capturingFactory());
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
    IrcConnection connection(controller, capturingFactory());
    QVERIFY(connection.activateStartup());
    QCOMPARE(m_transports.size(), 2);
}

void ConnectionTest::passwordsStayIsolatedPerNetwork()
{
    IrcController controller;
    IrcConnection connection(controller, capturingFactory());
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
    IrcConnection connection(controller, capturingFactory());
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

int runConnectionTests(int argc, char **argv)
{
    ConnectionTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_connection.moc"
