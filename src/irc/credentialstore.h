#pragma once

#include <QObject>
#include <QString>

struct CredentialKey
{
    QString networkId;
    QString username;
    QString host;
};

class CredentialStore : public QObject
{
    Q_OBJECT

public:
    enum class State {
        Loading,
        Available,
        Missing,
        Unavailable,
        Error,
        SessionOnly,
    };
    Q_ENUM(State)

    explicit CredentialStore(QObject *parent = nullptr) : QObject(parent) {}
    ~CredentialStore() override = default;

    virtual void read(const CredentialKey &key) = 0;
    virtual void write(const CredentialKey &key, const QString &password) = 0;
    virtual void remove(const CredentialKey &key) = 0;

signals:
    void readFinished(CredentialStore::State state, const QString &password,
                      const QString &message);
    void writeFinished(CredentialStore::State state, const QString &message);
};
