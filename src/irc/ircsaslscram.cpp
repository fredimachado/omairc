#include "ircsaslscram.h"

#include <QCryptographicHash>
#include <QMessageAuthenticationCode>
#include <QPasswordDigestor>
#include <QRandomGenerator>

namespace
{
constexpr qint64 kMinimumIterations = 4096;
constexpr qint64 kMaximumIterations = 1000000;

IrcSaslScram::Result failure(const QString& error)
{
    IrcSaslScram::Result result;
    result.error = error;
    return result;
}

IrcSaslScram::Result success(const QByteArray& message)
{
    IrcSaslScram::Result result;
    result.message = message;
    return result;
}

void wipe(QByteArray& bytes)
{
    bytes.fill('\0');
    bytes.clear();
}

bool containsUnsendable(const QString& text)
{
    for (const QChar character : text) {
        const char16_t code = character.unicode();
        if (code == u'\0' || code == u'\r' || code == u'\n')
            return true;
    }
    return false;
}

bool acceptableNonce(const QByteArray& nonce)
{
    if (nonce.isEmpty())
        return false;
    for (const char byte : nonce) {
        const auto code = static_cast<unsigned char>(byte);
        if (code < 0x21 || code > 0x7E || code == ',')
            return false;
    }
    return true;
}

QByteArray generateNonce()
{
    QByteArray raw(16, Qt::Uninitialized);
    auto *generator = QRandomGenerator::system();
    for (qsizetype index = 0; index < raw.size(); ++index)
        raw[index] = static_cast<char>(generator->bounded(256));
    return raw.toBase64();
}

// SASLprep is out of scope because this account is the existing IRC login token.
QByteArray escapeSaslName(const QByteArray& name)
{
    QByteArray escaped;
    escaped.reserve(name.size());
    for (const char byte : name) {
        if (byte == '=')
            escaped += "=3D";
        else if (byte == ',')
            escaped += "=2C";
        else
            escaped += byte;
    }
    return escaped;
}

bool sameBytes(const QByteArray& left, const QByteArray& right)
{
    if (left.size() != right.size())
        return false;
    unsigned char difference = 0;
    for (qsizetype index = 0; index < left.size(); ++index) {
        difference = static_cast<unsigned char>(
            difference
            | (static_cast<unsigned char>(left.at(index))
               ^ static_cast<unsigned char>(right.at(index))));
    }
    return difference == 0;
}

QByteArray xorBytes(const QByteArray& left, const QByteArray& right)
{
    if (left.size() != right.size())
        return {};
    QByteArray out(left.size(), '\0');
    for (qsizetype index = 0; index < left.size(); ++index) {
        out[index] = static_cast<char>(
            static_cast<unsigned char>(left.at(index))
            ^ static_cast<unsigned char>(right.at(index)));
    }
    return out;
}

struct Attributes
{
    QByteArray nonce;
    QByteArray salt;
    QByteArray iterations;
    QByteArray verifier;
    bool serverError = false;
};

bool keyListed(const char *allowed, char key)
{
    for (const char *cursor = allowed; *cursor != '\0'; ++cursor) {
        if (*cursor == key)
            return true;
    }
    return false;
}

bool parseAttributes(const QByteArray& message,
                     const char *allowed,
                     Attributes *attributes,
                     QString *error)
{
    if (message.isEmpty()) {
        *error = QStringLiteral("server message is missing SCRAM attributes");
        return false;
    }

    qsizetype start = 0;
    while (start < message.size()) {
        const qsizetype comma = message.indexOf(',', start);
        const qsizetype end = comma < 0 ? message.size() : comma;
        const QByteArray part = message.mid(start, end - start);
        if (part.size() < 3 || part.at(1) != '=') {
            *error = QStringLiteral("server message has a malformed SCRAM attribute");
            return false;
        }
        const char key = part.at(0);
        const QByteArray value = part.mid(2);
        if (value.isEmpty()) {
            *error = QStringLiteral("server message has a malformed SCRAM attribute");
            return false;
        }
        if (key == 'e') {
            if (attributes->serverError) {
                *error = QStringLiteral("server message repeats a SCRAM attribute");
                return false;
            }
            // The server text is discarded. A hostile peer could echo the
            // password there, and error strings must not carry it.
            attributes->serverError = true;
        } else if (!keyListed(allowed, key)) {
            *error = QStringLiteral("server message has a malformed SCRAM attribute");
            return false;
        } else {
            QByteArray *slot = nullptr;
            if (key == 'r')
                slot = &attributes->nonce;
            else if (key == 's')
                slot = &attributes->salt;
            else if (key == 'i')
                slot = &attributes->iterations;
            else if (key == 'v')
                slot = &attributes->verifier;
            if (!slot || !slot->isEmpty()) {
                *error = QStringLiteral("server message repeats a SCRAM attribute");
                return false;
            }
            *slot = value;
        }
        if (comma < 0)
            break;
        start = comma + 1;
        if (start >= message.size()) {
            *error = QStringLiteral("server message has a malformed SCRAM attribute");
            return false;
        }
    }
    return true;
}

bool decodeBase64(const QByteArray& text, QByteArray *decoded, QString *error)
{
    const QByteArray::FromBase64Result result = QByteArray::fromBase64Encoding(text);
    if (result.decodingStatus != QByteArray::Base64DecodingStatus::Ok
        || result.decoded.isEmpty()) {
        *error = QStringLiteral("server message has a malformed SCRAM attribute");
        return false;
    }
    *decoded = result.decoded;
    return true;
}

bool parseIterationCount(const QByteArray& text, int *iterations, QString *error)
{
    if (text.isEmpty()) {
        *error = QStringLiteral("server message has a malformed SCRAM attribute");
        return false;
    }
    for (const char byte : text) {
        if (byte < '0' || byte > '9') {
            *error = QStringLiteral("server message has a malformed SCRAM attribute");
            return false;
        }
    }
    bool ok = false;
    const qint64 parsed = text.toLongLong(&ok);
    if (!ok) {
        *error = QStringLiteral("server message has a malformed SCRAM attribute");
        return false;
    }
    if (parsed < kMinimumIterations || parsed > kMaximumIterations) {
        *error = QStringLiteral("iteration count is outside the allowed range");
        return false;
    }
    *iterations = static_cast<int>(parsed);
    return true;
}
}

void IrcSaslScram::reset()
{
    wipe(m_password);
    wipe(m_serverKey);
    m_authMessage.clear();
    m_clientNonce.clear();
    m_clientFirstBare.clear();
    m_step = Step::Idle;
}

IrcSaslScram::Result IrcSaslScram::start(const QString& account,
                                         const QString& password,
                                         const QByteArray& clientNonce)
{
    reset();
    if (account.isEmpty() || containsUnsendable(account) || containsUnsendable(password))
        return failure(QStringLiteral("account or password cannot be sent"));

    QByteArray nonce = clientNonce;
    if (nonce.isEmpty())
        nonce = generateNonce();
    if (!acceptableNonce(nonce))
        return failure(QStringLiteral("client nonce cannot be sent"));

    const QByteArray escaped = escapeSaslName(account.toUtf8());
    if (escaped.isEmpty())
        return failure(QStringLiteral("account or password cannot be sent"));

    m_clientNonce = nonce;
    m_clientFirstBare = QByteArrayLiteral("n=") + escaped + QByteArrayLiteral(",r=") + nonce;
    m_password = password.toUtf8();
    m_step = Step::AwaitServerFirst;
    return success(QByteArrayLiteral("n,,") + m_clientFirstBare);
}

IrcSaslScram::Result IrcSaslScram::takeServerFirst(const QByteArray& message)
{
    if (m_step != Step::AwaitServerFirst)
        return failure(QStringLiteral(
            "SCRAM exchange is not waiting for the server first message"));

    Attributes attributes;
    QString error;
    if (!parseAttributes(message, "rsi", &attributes, &error)) {
        reset();
        return failure(error);
    }
    if (attributes.serverError) {
        reset();
        return failure(QStringLiteral("the server rejected authentication"));
    }
    if (attributes.nonce.isEmpty() || attributes.salt.isEmpty()
        || attributes.iterations.isEmpty()) {
        reset();
        return failure(QStringLiteral("server message is missing SCRAM attributes"));
    }
    if (!attributes.nonce.startsWith(m_clientNonce)) {
        reset();
        return failure(QStringLiteral("server nonce does not continue the client nonce"));
    }

    int iterations = 0;
    if (!parseIterationCount(attributes.iterations, &iterations, &error)) {
        reset();
        return failure(error);
    }

    QByteArray salt;
    if (!decodeBase64(attributes.salt, &salt, &error)) {
        reset();
        return failure(error);
    }

    // Qt 6.5 renamed QPasswordDigestor::hash to deriveKeyPbkdf2.
    const int digestLength = QCryptographicHash::hashLength(QCryptographicHash::Sha256);
    QByteArray saltedPassword = QPasswordDigestor::deriveKeyPbkdf2(
        QCryptographicHash::Sha256, m_password, salt, iterations,
        quint64(digestLength));
    wipe(m_password);
    if (saltedPassword.size() != digestLength) {
        wipe(saltedPassword);
        reset();
        return failure(QStringLiteral("SCRAM proof could not be derived"));
    }

    QByteArray clientKey = QMessageAuthenticationCode::hash(
        QByteArrayLiteral("Client Key"), saltedPassword, QCryptographicHash::Sha256);
    QByteArray storedKey = QCryptographicHash::hash(
        clientKey, QCryptographicHash::Sha256);
    m_serverKey = QMessageAuthenticationCode::hash(
        QByteArrayLiteral("Server Key"), saltedPassword, QCryptographicHash::Sha256);
    wipe(saltedPassword);

    const QByteArray withoutProof =
        QByteArrayLiteral("c=")
        + QByteArrayLiteral("n,,").toBase64()
        + QByteArrayLiteral(",r=")
        + attributes.nonce;
    m_authMessage = m_clientFirstBare + ',' + message + ',' + withoutProof;

    QByteArray clientSignature = QMessageAuthenticationCode::hash(
        m_authMessage, storedKey, QCryptographicHash::Sha256);
    QByteArray proof = xorBytes(clientKey, clientSignature);
    wipe(clientKey);
    wipe(storedKey);
    wipe(clientSignature);
    if (proof.size() != digestLength) {
        wipe(proof);
        reset();
        return failure(QStringLiteral("SCRAM proof could not be derived"));
    }

    const QByteArray clientFinal = withoutProof + QByteArrayLiteral(",p=") + proof.toBase64();
    wipe(proof);
    m_step = Step::AwaitServerFinal;
    return success(clientFinal);
}

IrcSaslScram::Result IrcSaslScram::takeServerFinal(const QByteArray& message)
{
    if (m_step != Step::AwaitServerFinal)
        return failure(QStringLiteral(
            "SCRAM exchange is not waiting for the server final message"));

    Attributes attributes;
    QString error;
    if (!parseAttributes(message, "v", &attributes, &error)) {
        reset();
        return failure(error);
    }
    if (attributes.serverError) {
        reset();
        return failure(QStringLiteral("the server rejected authentication"));
    }
    if (attributes.verifier.isEmpty()) {
        reset();
        return failure(QStringLiteral("server message is missing SCRAM attributes"));
    }

    QByteArray verifier;
    if (!decodeBase64(attributes.verifier, &verifier, &error)) {
        reset();
        return failure(error);
    }

    QByteArray signature = QMessageAuthenticationCode::hash(
        m_authMessage, m_serverKey, QCryptographicHash::Sha256);
    const bool matches = sameBytes(signature, verifier);
    wipe(signature);
    wipe(verifier);
    reset();
    if (!matches)
        return failure(QStringLiteral("server signature does not match"));
    return success({});
}
