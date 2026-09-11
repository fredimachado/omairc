#include "secretservicecredentialstore.h"

#include <qt6keychain/keychain.h>

SecretServiceCredentialStore::SecretServiceCredentialStore(QObject *parent)
    : CredentialStore(parent)
{
}

QString SecretServiceCredentialStore::keyName(const CredentialKey &key)
{
    const auto encode = [](const QString &value) {
        const QString length = QString::number(value.toUtf8().size());
        return length + QLatin1Char(':') + value;
    };
    QString name = QStringLiteral("omairc/v1/")
        + encode(key.networkId) + QLatin1Char('/')
        + encode(key.username) + QLatin1Char('/')
        + encode(key.host);
    if (!key.purpose.isEmpty())
        name += QLatin1Char('/') + encode(key.purpose);
    return name;
}

CredentialStore::State SecretServiceCredentialStore::stateForError(
    QKeychain::Error error)
{
    return error == QKeychain::NoBackendAvailable
            || error == QKeychain::NotImplemented
        ? State::Unavailable
        : State::Error;
}

void SecretServiceCredentialStore::read(const CredentialKey &key)
{
    emit readFinished(State::Loading, {}, {});
    auto *job = new QKeychain::ReadPasswordJob(QStringLiteral("omairc"), this);
    job->setKey(keyName(key));
    connect(job, &QKeychain::Job::finished, this, [this, job]() {
        if (job->error() == QKeychain::NoError) {
            const QString password = job->textData();
            emit readFinished(password.isEmpty() ? State::Missing : State::Available,
                              password, {});
        } else if (job->error() == QKeychain::EntryNotFound) {
            emit readFinished(State::Missing, {}, {});
        } else {
            emit readFinished(stateForError(job->error()), {}, job->errorString());
        }
        job->deleteLater();
    });
    job->start();
}

void SecretServiceCredentialStore::write(const CredentialKey &key,
                                          const QString &password)
{
    auto *job = new QKeychain::WritePasswordJob(QStringLiteral("omairc"), this);
    job->setKey(keyName(key));
    job->setTextData(password);
    connect(job, &QKeychain::Job::finished, this, [this, job]() {
        const bool success = job->error() == QKeychain::NoError;
        emit writeFinished(success ? State::Available : stateForError(job->error()),
                           success ? QString() : job->errorString());
        job->deleteLater();
    });
    job->start();
}

void SecretServiceCredentialStore::remove(const CredentialKey &key)
{
    auto *job = new QKeychain::DeletePasswordJob(QStringLiteral("omairc"), this);
    job->setKey(keyName(key));
    connect(job, &QKeychain::Job::finished, this, [this, job]() {
        const bool missing = job->error() == QKeychain::NoError
            || job->error() == QKeychain::EntryNotFound;
        emit writeFinished(missing ? State::Missing : stateForError(job->error()),
                           missing ? QString() : job->errorString());
        job->deleteLater();
    });
    job->start();
}
