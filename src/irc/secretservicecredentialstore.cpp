#include "secretservicecredentialstore.h"

#include <qt6keychain/keychain.h>

namespace
{
QString keyName(const CredentialKey &key)
{
    return QStringLiteral("%1/%2/%3").arg(
        key.networkId, key.username, key.host);
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
            emit readFinished(State::Unavailable, {}, job->errorString());
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
        emit writeFinished(job->error() == QKeychain::NoError
                               ? State::Available : State::Unavailable,
                           job->errorString());
        job->deleteLater();
    });
    job->start();
}

void SecretServiceCredentialStore::remove(const CredentialKey &key)
{
    auto *job = new QKeychain::DeletePasswordJob(QStringLiteral("omairc"), this);
    job->setKey(keyName(key));
    connect(job, &QKeychain::Job::finished, this, [this, job]() {
        emit writeFinished(job->error() == QKeychain::NoError
                               || job->error() == QKeychain::EntryNotFound
                               ? State::Missing : State::Unavailable,
                           job->errorString());
        job->deleteLater();
    });
    job->start();
}
