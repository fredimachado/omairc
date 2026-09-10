#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QTest>

#include "ircframer.h"
#include "ircparser.h"

#include <string>
#include <string_view>

class CorpusTest : public QObject
{
    Q_OBJECT

private slots:
    void replaysCorpus();
};

void CorpusTest::replaysCorpus()
{
    const QDir dir(QStringLiteral(TEST_CORPUS_DIR));
    QVERIFY(dir.exists());

    const QFileInfoList files = dir.entryInfoList(QDir::Files, QDir::Name);
    QVERIFY(files.size() > 1);

    auto hasName = [&](const QString &name) {
        for (const QFileInfo &info : files) {
            if (info.fileName() == name)
                return true;
        }
        return false;
    };
    QVERIFY(hasName(QStringLiteral("01-classic-privmsg.txt")));
    QVERIFY(hasName(QStringLiteral("02-tagged-privmsg.txt")));
    QVERIFY(hasName(QStringLiteral("03-latin1-e9-body.txt")));

    int successfulParses = 0;
    for (const QFileInfo &info : files) {
        QFile file(info.absoluteFilePath());
        QVERIFY2(file.open(QIODevice::ReadOnly), qPrintable(info.fileName()));
        const QByteArray bytes = file.readAll();

        IrcFramer framer;
        const IrcFrameResult framed = framer.feed(
            std::string_view(bytes.constData(), static_cast<std::size_t>(bytes.size())));

        for (const std::string &frame : framed.frames) {
            const IrcParseResult parsed = IrcParser::parse(frame);
            if (!parsed) {
                QVERIFY(parsed.error != IrcError::None);
                continue;
            }

            ++successfulParses;
            const IrcMessage &message = *parsed.value;
            const QString name = info.fileName();
            if (name == QStringLiteral("01-classic-privmsg.txt")) {
                QCOMPARE(message.command, std::string("PRIVMSG"));
                QVERIFY(!message.parameters.empty());
                QCOMPARE(message.parameters.back(), std::string("hello world"));
            } else if (name == QStringLiteral("02-tagged-privmsg.txt")) {
                QVERIFY(!message.tags.empty());
                QCOMPARE(message.command, std::string("PRIVMSG"));
            } else if (name == QStringLiteral("03-latin1-e9-body.txt")) {
                QVERIFY(!message.parameters.empty());
                const std::string &body = message.parameters.back();
                QVERIFY(body.find('\xe9') != std::string::npos);
                const std::string expectedBody{'c', 'a', 'f', '\xe9'};
                QCOMPARE(body, expectedBody);
            }
        }
    }
    QVERIFY(successfulParses > 0);
}

int runCorpusTests(int argc, char **argv)
{
    CorpusTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_corpus.moc"
