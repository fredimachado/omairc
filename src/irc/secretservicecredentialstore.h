#pragma once

#include "credentialstore.h"

class SecretServiceCredentialStore final : public CredentialStore
{
    Q_OBJECT

public:
    explicit SecretServiceCredentialStore(QObject *parent = nullptr);

    void read(const CredentialKey &key) override;
    void write(const CredentialKey &key, const QString &password) override;
    void remove(const CredentialKey &key) override;

};
