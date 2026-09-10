#include <QTest>

#include "storage/secretservicecredentialstore.h"

class SecretServiceTest : public QObject
{
    Q_OBJECT

private slots:
    void noBackendIsUnavailable();
    void notImplementedIsUnavailable();
    void otherErrorsAreError();
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

void SecretServiceTest::otherErrorsAreError()
{
    QCOMPARE(SecretServiceCredentialStore::stateForError(QKeychain::OtherError),
             CredentialStore::State::Error);
    QCOMPARE(SecretServiceCredentialStore::stateForError(QKeychain::EntryNotFound),
             CredentialStore::State::Error);
    QCOMPARE(SecretServiceCredentialStore::stateForError(QKeychain::AccessDenied),
             CredentialStore::State::Error);
}

int runSecretServiceTests(int argc, char **argv)
{
    SecretServiceTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_secretservice.moc"
