#include <QDir>
#include <QFile>
#include <QSet>
#include <QTest>

#include "ircframer.h"
#include "ircparser.h"

#include <optional>
#include <string>
#include <string_view>
#include <vector>

namespace {
struct ExpectedTag
{
    std::string name;
    std::optional<std::string> value;
};

struct ExpectedFrame
{
    IrcError error;
    std::string command;
    std::vector<ExpectedTag> tags;
    std::optional<IrcPrefix> prefix;
    std::vector<std::string> parameters;
};

struct ExpectedFault
{
    IrcError error;
    std::size_t byteCount;
    std::string preview;
};

struct CorpusCase
{
    const char *fileName;
    std::vector<ExpectedFrame> frames;
    std::vector<ExpectedFault> faults;
};

const std::vector<CorpusCase> &corpusCases()
{
    static const std::vector<CorpusCase> cases{
        {"01-classic-privmsg.txt",
         {{IrcError::None, "PRIVMSG", {}, IrcPrefix{"n!u@h", "n", "u", "h"},
           {"#c", "hello world"}}}, {}},
        {"02-tagged-privmsg.txt",
         {{IrcError::None, "PRIVMSG", {{"msgid", "abc"}, {"+typing", "active"}},
           IrcPrefix{"n!u@h", "n", "u", "h"}, {"#c", "tagged"}}}, {}},
        {"03-latin1-e9-body.txt",
         {{IrcError::None, "PRIVMSG", {}, IrcPrefix{"n!u@h", "n", "u", "h"},
           {"#c", std::string{'c', 'a', 'f', '\xe9'}}}}, {}},
        {"04-overlong.txt",
         {{IrcError::None, std::string(513, 'X'), {}, std::nullopt, {}}}, {}},
        {"05-nul.txt", {},
         {{IrcError::InvalidCharacter, 18,
           std::string("PRIVMSG #c :hel\0lo", 18)}}},
    };
    return cases;
}

const CorpusCase &findCase(const QString &fileName)
{
    for (const CorpusCase &testCase : corpusCases()) {
        if (fileName == QLatin1String(testCase.fileName))
            return testCase;
    }
    Q_UNREACHABLE_RETURN(corpusCases().front());
}
}

class CorpusTest : public QObject
{
    Q_OBJECT

private slots:
    void registryMatchesDirectory();
    void replaysCorpus_data();
    void replaysCorpus();
};

void CorpusTest::registryMatchesDirectory()
{
    const QDir dir(QStringLiteral(TEST_CORPUS_DIR));
    QVERIFY(dir.exists());

    QSet<QString> actual;
    const QStringList files = dir.entryList({QStringLiteral("*.txt")}, QDir::Files,
                                            QDir::Name);
    for (const QString &file : files)
        actual.insert(file);

    QSet<QString> registered;
    for (const CorpusCase &testCase : corpusCases()) {
        const QString name = QString::fromLatin1(testCase.fileName);
        QVERIFY2(!registered.contains(name), qPrintable("duplicate corpus case: " + name));
        registered.insert(name);
    }
    QCOMPARE(actual, registered);
}

void CorpusTest::replaysCorpus_data()
{
    QTest::addColumn<QString>("fileName");
    QTest::addColumn<int>("chunking");

    static const char *chunkNames[] = {"whole", "halves", "bytes"};
    for (const CorpusCase &testCase : corpusCases()) {
        const int chunkingCount = testCase.faults.empty() ? 3 : 1;
        for (int chunking = 0; chunking < chunkingCount; ++chunking) {
            const QString fileName = QString::fromLatin1(testCase.fileName);
            const QByteArray row = fileName.toLatin1() + ':' + chunkNames[chunking];
            QTest::newRow(row.constData()) << fileName << chunking;
        }
    }
}

void CorpusTest::replaysCorpus()
{
    QFETCH(QString, fileName);
    QFETCH(int, chunking);
    const CorpusCase &expected = findCase(fileName);

    QFile file(QDir(QStringLiteral(TEST_CORPUS_DIR)).filePath(fileName));
    QVERIFY2(file.open(QIODevice::ReadOnly), qPrintable(fileName));
    const QByteArray bytes = file.readAll();

    std::vector<IrcFrameFault> faults;
    std::vector<std::string> frames;
    IrcFramer framer;
    const auto feed = [&](qsizetype offset, qsizetype size) {
        const IrcFrameResult result = framer.feed(std::string_view(
            bytes.constData() + offset, static_cast<std::size_t>(size)));
        frames.insert(frames.end(), result.frames.begin(), result.frames.end());
        faults.insert(faults.end(), result.faults.begin(), result.faults.end());
    };
    if (chunking == 0) {
        feed(0, bytes.size());
    } else if (chunking == 1) {
        const qsizetype half = bytes.size() / 2;
        feed(0, half);
        feed(half, bytes.size() - half);
    } else {
        for (qsizetype i = 0; i < bytes.size(); ++i)
            feed(i, 1);
    }

    QCOMPARE(frames.size(), expected.frames.size());
    QCOMPARE(faults.size(), expected.faults.size());
    for (std::size_t i = 0; i < faults.size(); ++i) {
        QCOMPARE(faults[i].error, expected.faults[i].error);
        QCOMPARE(faults[i].byteCount, expected.faults[i].byteCount);
        QCOMPARE(faults[i].preview, expected.faults[i].preview);
    }

    for (std::size_t i = 0; i < frames.size(); ++i) {
        const ExpectedFrame &wanted = expected.frames[i];
        const IrcParseResult parsed = IrcParser::parse(frames[i]);
        QCOMPARE(parsed.error, wanted.error);
        if (wanted.error != IrcError::None) {
            QVERIFY(!parsed);
            continue;
        }
        QVERIFY(parsed);
        const IrcMessage &message = *parsed.value;
        QCOMPARE(message.command, wanted.command);
        QCOMPARE(message.parameters, wanted.parameters);
        QCOMPARE(message.tags.size(), wanted.tags.size());
        for (std::size_t tag = 0; tag < message.tags.size(); ++tag) {
            QCOMPARE(message.tags[tag].name, wanted.tags[tag].name);
            QCOMPARE(message.tags[tag].value, wanted.tags[tag].value);
        }
        QCOMPARE(message.prefix.has_value(), wanted.prefix.has_value());
        if (wanted.prefix) {
            QCOMPARE(message.prefix->raw, wanted.prefix->raw);
            QCOMPARE(message.prefix->nick, wanted.prefix->nick);
            QCOMPARE(message.prefix->user, wanted.prefix->user);
            QCOMPARE(message.prefix->host, wanted.prefix->host);
        }
    }
}

int runCorpusTests(int argc, char **argv)
{
    CorpusTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_corpus.moc"
