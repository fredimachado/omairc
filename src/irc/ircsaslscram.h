#pragma once

#include <QByteArray>
#include <QString>

// SCRAM-SHA-256 client (RFC 5802, RFC 7677). The messages are raw SCRAM
// bytes. The IRC session applies SASL base64 framing later.
class IrcSaslScram
{
public:
    struct Result
    {
        QByteArray message;
        QString error;

        bool ok() const { return error.isEmpty(); }
    };

    // An empty clientNonce is generated. Tests pass a nonce so the exchange
    // matches a known vector.
    Result start(const QString& account,
                 const QString& password,
                 const QByteArray& clientNonce = {});
    Result takeServerFirst(const QByteArray& message);
    Result takeServerFinal(const QByteArray& message);

private:
    enum class Step {
        Idle,
        AwaitServerFirst,
        AwaitServerFinal,
    };

    void reset();

    Step m_step = Step::Idle;
    QByteArray m_password;
    QByteArray m_clientNonce;
    QByteArray m_clientFirstBare;
    QByteArray m_authMessage;
    QByteArray m_serverKey;
};
