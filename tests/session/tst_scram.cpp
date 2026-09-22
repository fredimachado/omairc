#include <QElapsedTimer>
#include <QTest>

#include "ircsaslscram.h"

namespace
{
const QByteArray kNonce = QByteArrayLiteral("rOprNGfwEbeRWgbNEkqO");
const QByteArray kClientFirst = QByteArrayLiteral(
    "n,,n=user,r=rOprNGfwEbeRWgbNEkqO");
const QByteArray kServerFirst = QByteArrayLiteral(
    "r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,"
    "s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096");
const QByteArray kClientFinal = QByteArrayLiteral(
    "c=biws,r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,"
    "p=dHzbZapWIk4jUhN+Ute9ytag9zjfMHgsqmmiz7AndVQ=");
const QByteArray kServerFinal = QByteArrayLiteral(
    "v=6rriTRBi23WpRR/wtup+mMhUZUn/dB5nLTJRsjl95G4=");
const QByteArray kSalt = QByteArrayLiteral("W22ZaJ0SNY7soEsUEjb6gQ==");

bool hidesPassword(const IrcSaslScram::Result& result)
{
    return !result.error.contains(QStringLiteral("pencil"))
        && !result.message.contains("pencil");
}

QByteArray serverFirstWithIteration(const QByteArray& iteration)
{
    return QByteArrayLiteral(
               "r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,s=")
        + kSalt + QByteArrayLiteral(",i=") + iteration;
}
}

class ScramTest : public QObject
{
    Q_OBJECT

private slots:
    void rfc7677VectorVerifiesServerSignature();
    void wrongServerSignatureFails();
    void serverNonceMustContinueClientNonce();
    void nonceEqualsSignsBelongToTheValue();
    void iterationCountBelowMinimumFails();
    void iterationCountAboveMaximumFailsWithoutPbkdf2();
    void usernameEscapesEqualsAndComma();
    void accountOrPasswordWithControlBytesIsRejected();
    void serverErrorAttributeFails();
    void missingOrGarbageAttributeFails();
    void serverFinalBeforeServerFirstFails();
    void secondCallInTheWrongStateFails();
    void generatedNonceIsPrintableAndHasNoComma();
};

void ScramTest::rfc7677VectorVerifiesServerSignature()
{
    IrcSaslScram scram;
    const IrcSaslScram::Result started = scram.start(
        QStringLiteral("user"), QStringLiteral("pencil"), kNonce);
    QVERIFY(started.ok());
    QCOMPARE(started.message, kClientFirst);
    QVERIFY(hidesPassword(started));

    const IrcSaslScram::Result clientFinal = scram.takeServerFirst(kServerFirst);
    QVERIFY(clientFinal.ok());
    QCOMPARE(clientFinal.message, kClientFinal);
    QVERIFY(hidesPassword(clientFinal));

    const IrcSaslScram::Result serverFinal = scram.takeServerFinal(kServerFinal);
    QVERIFY(serverFinal.ok());
    QVERIFY(serverFinal.message.isEmpty());
    QVERIFY(hidesPassword(serverFinal));
}

void ScramTest::wrongServerSignatureFails()
{
    IrcSaslScram scram;
    QVERIFY(scram.start(QStringLiteral("user"), QStringLiteral("pencil"), kNonce).ok());
    QVERIFY(scram.takeServerFirst(kServerFirst).ok());

    const IrcSaslScram::Result rejected = scram.takeServerFinal(QByteArrayLiteral(
        "v=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="));
    QVERIFY(!rejected.ok());
    QVERIFY(rejected.message.isEmpty());
    QVERIFY(hidesPassword(rejected));
}

void ScramTest::serverNonceMustContinueClientNonce()
{
    IrcSaslScram scram;
    QVERIFY(scram.start(QStringLiteral("user"), QStringLiteral("pencil"), kNonce).ok());

    const QByteArray serverFirst = QByteArrayLiteral(
        "r=XXXXXXXXXXXXXXXXXXXX%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,s=")
        + kSalt + QByteArrayLiteral(",i=4096");
    const IrcSaslScram::Result rejected = scram.takeServerFirst(serverFirst);
    QVERIFY(!rejected.ok());
    QVERIFY(rejected.message.isEmpty());
    QVERIFY(hidesPassword(rejected));
}

void ScramTest::nonceEqualsSignsBelongToTheValue()
{
    const QByteArray nonce = QByteArrayLiteral("abc");
    IrcSaslScram scram;
    QVERIFY(scram.start(QStringLiteral("user"), QStringLiteral("pencil"), nonce).ok());

    const QByteArray serverFirst = QByteArrayLiteral("r=abc=def=ghi,s=")
        + kSalt + QByteArrayLiteral(",i=4096");
    const IrcSaslScram::Result clientFinal = scram.takeServerFirst(serverFirst);
    QVERIFY(clientFinal.ok());
    QVERIFY(clientFinal.message.startsWith(QByteArrayLiteral("c=biws,r=abc=def=ghi,p=")));
    QVERIFY(hidesPassword(clientFinal));
}

void ScramTest::iterationCountBelowMinimumFails()
{
    IrcSaslScram scram;
    QVERIFY(scram.start(QStringLiteral("user"), QStringLiteral("pencil"), kNonce).ok());

    const IrcSaslScram::Result rejected = scram.takeServerFirst(
        serverFirstWithIteration(QByteArrayLiteral("4095")));
    QVERIFY(!rejected.ok());
    QVERIFY(rejected.message.isEmpty());
    QVERIFY(hidesPassword(rejected));
}

void ScramTest::iterationCountAboveMaximumFailsWithoutPbkdf2()
{
    IrcSaslScram scram;
    QVERIFY(scram.start(QStringLiteral("user"), QStringLiteral("pencil"), kNonce).ok());

    QElapsedTimer timer;
    timer.start();
    const IrcSaslScram::Result rejected = scram.takeServerFirst(
        serverFirstWithIteration(QByteArrayLiteral("1000001")));
    const qint64 elapsed = timer.elapsed();
    QVERIFY(!rejected.ok());
    QVERIFY(rejected.message.isEmpty());
    QVERIFY(hidesPassword(rejected));
    QVERIFY2(elapsed < 200, "iteration count above 1000000 must fail before PBKDF2");
}

void ScramTest::usernameEscapesEqualsAndComma()
{
    IrcSaslScram scram;
    const IrcSaslScram::Result started = scram.start(
        QStringLiteral("a=b,c"), QStringLiteral("pencil"), QByteArrayLiteral("nonce"));
    QVERIFY(started.ok());
    QCOMPARE(started.message, QByteArrayLiteral("n,,n=a=3Db=2Cc,r=nonce"));
    QVERIFY(hidesPassword(started));
}

void ScramTest::accountOrPasswordWithControlBytesIsRejected()
{
    QString accountWithNul = QStringLiteral("us");
    accountWithNul.append(QChar(u'\0'));
    accountWithNul.append(QStringLiteral("er"));
    const QList<QString> accounts = {
        QStringLiteral("user\rname"),
        QStringLiteral("user\nname"),
        QString(QChar(u'\0')),
        accountWithNul,
    };
    for (const QString& account : accounts) {
        IrcSaslScram scram;
        const IrcSaslScram::Result rejected = scram.start(account, QStringLiteral("pencil"));
        QVERIFY(!rejected.ok());
        QVERIFY(rejected.message.isEmpty());
        QVERIFY(hidesPassword(rejected));
        QVERIFY(!scram.takeServerFirst(kServerFirst).ok());
    }

    QString passwordWithNul = QStringLiteral("pencil");
    passwordWithNul.append(QChar(u'\0'));
    const QList<QString> passwords = {
        QStringLiteral("pencil\r"),
        QStringLiteral("pencil\n"),
        passwordWithNul,
    };
    for (const QString& password : passwords) {
        IrcSaslScram scram;
        const IrcSaslScram::Result rejected = scram.start(QStringLiteral("user"), password);
        QVERIFY(!rejected.ok());
        QVERIFY(rejected.message.isEmpty());
        QVERIFY(!rejected.error.contains(QStringLiteral("pencil")));
    }
}

void ScramTest::serverErrorAttributeFails()
{
    IrcSaslScram scram;
    QVERIFY(scram.start(QStringLiteral("user"), QStringLiteral("pencil"), kNonce).ok());
    const IrcSaslScram::Result first = scram.takeServerFirst(
        QByteArrayLiteral("e=pencil"));
    QVERIFY(!first.ok());
    QVERIFY(first.message.isEmpty());
    QVERIFY(hidesPassword(first));

    IrcSaslScram again;
    QVERIFY(again.start(QStringLiteral("user"), QStringLiteral("pencil"), kNonce).ok());
    QVERIFY(again.takeServerFirst(kServerFirst).ok());
    const IrcSaslScram::Result finalMessage = again.takeServerFinal(
        QByteArrayLiteral("e=pencil"));
    QVERIFY(!finalMessage.ok());
    QVERIFY(finalMessage.message.isEmpty());
    QVERIFY(hidesPassword(finalMessage));
}

void ScramTest::missingOrGarbageAttributeFails()
{
    IrcSaslScram missing;
    QVERIFY(missing.start(QStringLiteral("user"), QStringLiteral("pencil"), kNonce).ok());
    const IrcSaslScram::Result noSalt = missing.takeServerFirst(QByteArrayLiteral(
        "r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,i=4096"));
    QVERIFY(!noSalt.ok());
    QVERIFY(noSalt.message.isEmpty());

    IrcSaslScram garbage;
    QVERIFY(garbage.start(QStringLiteral("user"), QStringLiteral("pencil"), kNonce).ok());
    const IrcSaslScram::Result extra = garbage.takeServerFirst(
        kServerFirst + QByteArrayLiteral(",m=reserved"));
    QVERIFY(!extra.ok());
    QVERIFY(extra.message.isEmpty());
    QVERIFY(hidesPassword(extra));
}

void ScramTest::serverFinalBeforeServerFirstFails()
{
    IrcSaslScram scram;
    QVERIFY(scram.start(QStringLiteral("user"), QStringLiteral("pencil"), kNonce).ok());
    const IrcSaslScram::Result early = scram.takeServerFinal(kServerFinal);
    QVERIFY(!early.ok());
    QVERIFY(early.message.isEmpty());
    QVERIFY(hidesPassword(early));

    const IrcSaslScram::Result clientFinal = scram.takeServerFirst(kServerFirst);
    QVERIFY(clientFinal.ok());
    QCOMPARE(clientFinal.message, kClientFinal);
}

void ScramTest::secondCallInTheWrongStateFails()
{
    IrcSaslScram scram;
    QVERIFY(scram.start(QStringLiteral("user"), QStringLiteral("pencil"), kNonce).ok());
    QVERIFY(scram.takeServerFirst(kServerFirst).ok());
    const IrcSaslScram::Result repeated = scram.takeServerFirst(kServerFirst);
    QVERIFY(!repeated.ok());
    QVERIFY(repeated.message.isEmpty());

    QVERIFY(scram.takeServerFinal(kServerFinal).ok());
    const IrcSaslScram::Result again = scram.takeServerFinal(kServerFinal);
    QVERIFY(!again.ok());
    QVERIFY(again.message.isEmpty());
}

void ScramTest::generatedNonceIsPrintableAndHasNoComma()
{
    IrcSaslScram scram;
    const IrcSaslScram::Result started = scram.start(
        QStringLiteral("user"), QStringLiteral("pencil"));
    QVERIFY(started.ok());
    QVERIFY(started.message.startsWith(QByteArrayLiteral("n,,n=user,r=")));
    const QByteArray nonce = started.message.mid(QByteArrayLiteral("n,,n=user,r=").size());
    QVERIFY(nonce.size() >= 16);
    for (const char byte : nonce) {
        const auto code = static_cast<unsigned char>(byte);
        QVERIFY(code >= 0x21 && code <= 0x7E);
        QVERIFY(code != ',');
    }

    IrcSaslScram other;
    const IrcSaslScram::Result again = other.start(
        QStringLiteral("user"), QStringLiteral("pencil"));
    QVERIFY(again.ok());
    QVERIFY(again.message != started.message);
}

int runScramTests(int argc, char **argv)
{
    ScramTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_scram.moc"
