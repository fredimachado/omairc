#include "secretservicecredentialstore.h"

#include <qt6keychain/keychain.h>

namespace
{
QString keyName(const CredentialKey &key)
{
    const auto encode = [](const QString &value) {
        const QString length = QString::number(value.toUtf8().size());
        return length + QLatin1Char(':') + value;
    };
    return QStringLiteral("omairc/v1/")
        + encode(key.networkId) + QLatin1Char('/')
        + encode(key.username) + QLatin1Char('/')
        + encode(key.host);
}

CredentialStore::State stateForError(const QKeychain::Error error)
{
    return error == QKeychain::NoBackendAvailable
            || error == QKeychain::NotImplemented
        ? CredentialStore::State::Unavailable
        : CredentialStore::State::Error;
}
}

SecretServiceCredentialStore::SecretServiceCredentialStore(QObject *parent)
    : CredentialStore(parent)
{
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
