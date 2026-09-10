#pragma once

#include "credentialstore.h"

#include <qt6keychain/keychain.h>

class SecretServiceCredentialStore final : public CredentialStore
{
    Q_OBJECT

public:
    explicit SecretServiceCredentialStore(QObject *parent = nullptr);

    void read(const CredentialKey &key) override;
    void write(const CredentialKey &key, const QString &password) override;
    void remove(const CredentialKey &key) override;

    static State stateForError(QKeychain::Error error);
};
