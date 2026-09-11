#include <QTest>

#include "storage/secretservicecredentialstore.h"

class SecretServiceTest : public QObject
{
    Q_OBJECT

private slots:
    void noBackendIsUnavailable();
    void notImplementedIsUnavailable();
    void otherErrorIsError();
    void accessDeniedIsError();
    void keyNameIsLengthPrefixed();
    void keyNameDoesNotCollideOnSlashBoundaries();
};

void SecretServiceTest::noBackendIsUnavailable()
{
    QCOMPARE(SecretServiceCredentialStore::stateForError(QKeychain::NoBackendAvailable),
             CredentialStore::State::Unavailable);
}

void SecretServiceTest::notImplementedIsUnavailable()
{
    QCOMPARE(SecretServiceCredentialStore::stateForError(QKeychain::NotImplemented),
             CredentialStore::State::Unavailable);
}

void SecretServiceTest::otherErrorIsError()
{
    QCOMPARE(SecretServiceCredentialStore::stateForError(QKeychain::OtherError),
             CredentialStore::State::Error);
}

void SecretServiceTest::accessDeniedIsError()
{
    QCOMPARE(SecretServiceCredentialStore::stateForError(QKeychain::AccessDenied),
             CredentialStore::State::Error);
    QCOMPARE(SecretServiceCredentialStore::stateForError(QKeychain::EntryNotFound),
             CredentialStore::State::Error);
}

void SecretServiceTest::keyNameIsLengthPrefixed()
{
    QCOMPARE(SecretServiceCredentialStore::keyName(
                 {QStringLiteral("nid"), QStringLiteral("user"),
                  QStringLiteral("irc.example")}),
             QStringLiteral("omairc/v1/3:nid/4:user/11:irc.example"));
}

void SecretServiceTest::keyNameDoesNotCollideOnSlashBoundaries()
{
    const QString splitHost = SecretServiceCredentialStore::keyName(
        {QStringLiteral("id"), QStringLiteral("a"), QStringLiteral("b/c")});
    const QString splitUser = SecretServiceCredentialStore::keyName(
        {QStringLiteral("id"), QStringLiteral("a/b"), QStringLiteral("c")});
    QVERIFY(splitHost != splitUser);
    QCOMPARE(splitHost, QStringLiteral("omairc/v1/2:id/1:a/3:b/c"));
    QCOMPARE(splitUser, QStringLiteral("omairc/v1/2:id/3:a/b/1:c"));
}

int runSecretServiceTests(int argc, char **argv)
{
    SecretServiceTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_secretservice.moc"
