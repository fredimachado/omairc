#include <QDir>
#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QTemporaryDir>
#include <QTest>

#include "irccasemapping.h"
#include "ircconversationlog.h"
#include "ircstoragepath.h"
#include "ircwiretext.h"
#include "omaircclipcursor.h"

class StoragePathTest : public QObject
{
    Q_OBJECT

private slots:
    void encodesReservedCharacters();
    void percentEncodingIsInjective();
    void encodesTrailingDotAndSpace();
    void encodesEmptyAndDotSegments();
    void encodesDeviceNames();
    void encodesDottedDeviceNames();
    void capsSegmentLengthWithExtension();
    void preservesNetworkCaseAndLowercaseHex();
    void accentedCharactersEncodeDistinctly();
    void targetSegmentFollowsCaseMapping();
    void migratesLegacyTranscriptPaths();
    void migratesLegacyTranscriptWhenNewNetworkDirExists();
    void transcriptPathPreservesNormalizedTarget();
    void retriesLegacyTranscriptMigrationAfterRenameFailure();
    void doesNotOverwriteExistingTranscriptDuringMigration();
    void migratesTranscriptFilesWithoutRenamingSharedNetworkDir();
    void migratesLegacyTranscriptForDeviceLikeNames();
    void collapsesEquivalentCursorFiles();
    void keepsNewestLegacyCursorAcrossEncodings();
    void cursorWinnerUsesMsgidAndSequence();
    void cursorPathStableAcrossCaseMappingKnown();
    void ignoresNewStyleCursorEncodingAsLegacy();
    void doesNotCollapseBracketCursorsBeforeCaseMappingKnown();
    void collapsesBracketCursorsWhenCaseMappingKnown();
    void decodesOmaircStorageSegments();
    void nulTargetsDoNotSharePaths();
};

void StoragePathTest::encodesReservedCharacters()
{
    QCOMPARE(omaircStorageSegment(QStringLiteral("#omarchy")),
             QStringLiteral("#omarchy"));
    QCOMPARE(omaircStorageSegment(QStringLiteral("alice")), QStringLiteral("alice"));
    QCOMPARE(omaircStorageSegment(QStringLiteral("libera")), QStringLiteral("libera"));
    QCOMPARE(omaircStorageSegment(QStringLiteral("net-1")), QStringLiteral("net-1"));
    QCOMPARE(omaircStorageSegment(QStringLiteral("#a|b")),
             QStringLiteral("#a%7cb"));
}

void StoragePathTest::percentEncodingIsInjective()
{
    QVERIFY(omaircStorageSegment(QStringLiteral("a/b"))
            != omaircStorageSegment(QStringLiteral("a%2fb")));
}

void StoragePathTest::encodesTrailingDotAndSpace()
{
    QCOMPARE(omaircStorageSegment(QStringLiteral("tail.")),
             QStringLiteral("tail%2e"));
    QCOMPARE(omaircStorageSegment(QStringLiteral("tail ")),
             QStringLiteral("tail%20"));
}

void StoragePathTest::encodesEmptyAndDotSegments()
{
    QCOMPARE(omaircStorageSegment({}), QStringLiteral("_"));
    QCOMPARE(omaircStorageSegment(QStringLiteral(".")), QStringLiteral("_"));
    QCOMPARE(omaircStorageSegment(QStringLiteral("..")), QStringLiteral("_"));
}

void StoragePathTest::encodesDeviceNames()
{
    QCOMPARE(omaircStorageSegment(QStringLiteral("con")),
             QStringLiteral("%63on"));
    QCOMPARE(omaircStorageSegment(QStringLiteral("con"), QStringLiteral(".json")),
             QStringLiteral("%63on"));
    QCOMPARE(omaircStorageSegment(QStringLiteral("aux")),
             QStringLiteral("%61ux"));
    QCOMPARE(omaircStorageSegment(QStringLiteral("nul")),
             QStringLiteral("%6eul"));
    QCOMPARE(omaircStorageSegment(QStringLiteral("com1")),
             QStringLiteral("%63om1"));
    QCOMPARE(omaircStorageSegment(QStringLiteral("lpt9")),
             QStringLiteral("%6cpt9"));
    QCOMPARE(omaircStorageSegment(QStringLiteral("prn")),
             QStringLiteral("%70rn"));
    QCOMPARE(omaircStorageSegment(QStringLiteral("con.")),
             QStringLiteral("%63on%2e"));
}

void StoragePathTest::encodesDottedDeviceNames()
{
    QCOMPARE(omaircStorageSegment(QStringLiteral("nul.tar.gz")),
             QStringLiteral("%6eul.tar.gz"));
    QCOMPARE(omaircStorageSegment(QStringLiteral("com1.tar.gz")),
             QStringLiteral("%63om1.tar.gz"));
    QCOMPARE(omaircStorageSegment(QStringLiteral("aux.foo.bar")),
             QStringLiteral("%61ux.foo.bar"));
    QCOMPARE(omaircStorageSegment(QStringLiteral("con.txt.")),
             QStringLiteral("%63on.txt%2e"));
    QCOMPARE(omaircStorageSegment(QStringLiteral("com¹")),
             QStringLiteral("%63om%c2%b9"));
    QCOMPARE(omaircStorageSegment(QStringLiteral("lpt³")),
             QStringLiteral("%6cpt%c2%b3"));
}

void StoragePathTest::capsSegmentLengthWithExtension()
{
    const QString extension = QStringLiteral(".json");
    const QString withinLimit(250, QLatin1Char('a'));
    const QString segmentWithin =
        omaircStorageSegment(withinLimit, extension);
    QVERIFY(segmentWithin.startsWith(QLatin1Char('a')));
    QCOMPARE(segmentWithin.size() + extension.size(), 255);

    const QString overLimit(251, QLatin1Char('a'));
    const QString segmentOver = omaircStorageSegment(overLimit, extension);
    QVERIFY(segmentOver.startsWith(QLatin1Char('h')));
    QVERIFY(segmentOver.size() + extension.size() <= 255);

    const IrcCaseMapping mapping;
    const QString targetSegment =
        omaircTargetSegment(overLimit, mapping, extension);
    QCOMPARE(targetSegment, segmentOver);
}

void StoragePathTest::preservesNetworkCaseAndLowercaseHex()
{
    QVERIFY(omaircStorageSegment(QStringLiteral("Ab"))
            != omaircStorageSegment(QStringLiteral("ab")));
    QCOMPARE(omaircStorageSegment(QStringLiteral("#a|b")),
             QStringLiteral("#a%7cb"));
}

void StoragePathTest::accentedCharactersEncodeDistinctly()
{
    const QString lower = omaircStorageSegment(QStringLiteral("é"));
    const QString upper = omaircStorageSegment(QStringLiteral("É"));
    QVERIFY(lower != upper);
    QCOMPARE(lower, QStringLiteral("%c3%a9"));
    QCOMPARE(upper, QStringLiteral("%c3%89"));
}

void StoragePathTest::targetSegmentFollowsCaseMapping()
{
    const IrcCaseMapping rfc1459(IrcCaseMapping::Kind::Rfc1459);
    const IrcCaseMapping ascii(IrcCaseMapping::Kind::Ascii);
    const IrcCaseMapping strict(IrcCaseMapping::Kind::Rfc1459Strict);

    QCOMPARE(omaircTargetSegment(QStringLiteral("#Chan"), rfc1459),
             omaircTargetSegment(QStringLiteral("#chan"), rfc1459));
    QCOMPARE(omaircTargetSegment(QStringLiteral("#Chan"), ascii),
             omaircTargetSegment(QStringLiteral("#chan"), ascii));

    QVERIFY(omaircTargetSegment(QStringLiteral("#foo["), rfc1459)
            == omaircTargetSegment(QStringLiteral("#foo{"), rfc1459));
    QVERIFY(omaircTargetSegment(QStringLiteral("#foo["), ascii)
            != omaircTargetSegment(QStringLiteral("#foo{"), ascii));

    QCOMPARE(omaircTargetSegment(QStringLiteral("#foo^"), rfc1459),
             omaircTargetSegment(QStringLiteral("#foo~"), rfc1459));
    QVERIFY(omaircTargetSegment(QStringLiteral("#foo^"), strict)
            != omaircTargetSegment(QStringLiteral("#foo~"), strict));
}

void StoragePathTest::migratesLegacyTranscriptPaths()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString root = dir.filePath(QStringLiteral("logs"));
    QVERIFY(QDir().mkpath(root));

    const QString legacyNetworkDir =
        QDir(root).filePath(legacyStorageSegment(QStringLiteral("Libera")));
    QVERIFY(QDir().mkpath(legacyNetworkDir));
    const QString legacyFile =
        QDir(legacyNetworkDir).filePath(legacyStorageSegment(QStringLiteral("#a|b")));
    {
        QFile file(legacyFile);
        QVERIFY(file.open(QIODevice::WriteOnly));
        file.write("legacy\n");
    }

    IrcConversationLog log(root);
    const IrcCaseMapping mapping;
    const QString path =
        log.pathFor(QStringLiteral("Libera"), QStringLiteral("#a|b"), mapping);
    QCOMPARE(path, QDir(QDir(root).filePath(omaircStorageSegment(QStringLiteral("Libera"))))
                        .filePath(omaircWireStorageSegment(QStringLiteral("#a|b"))));
    QVERIFY(QFile::exists(path));
    QVERIFY(!QFile::exists(legacyFile));
    QVERIFY(!QDir(root).exists(legacyStorageSegment(QStringLiteral("Libera"))));
}

void StoragePathTest::migratesLegacyTranscriptWhenNewNetworkDirExists()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString root = dir.filePath(QStringLiteral("logs"));
    QVERIFY(QDir().mkpath(root));

    const QString legacyNetworkDir =
        QDir(root).filePath(legacyStorageSegment(QStringLiteral("Libera")));
    QVERIFY(QDir().mkpath(legacyNetworkDir));
    const QString legacyFile =
        QDir(legacyNetworkDir).filePath(legacyStorageSegment(QStringLiteral("#a|b")));
    {
        QFile file(legacyFile);
        QVERIFY(file.open(QIODevice::WriteOnly));
        file.write("legacy\n");
    }

    const QString newNetworkDir =
        QDir(root).filePath(omaircStorageSegment(QStringLiteral("Libera")));
    QVERIFY(QDir().mkpath(newNetworkDir));

    IrcConversationLog log(root);
    const IrcCaseMapping mapping;
    const QString path =
        log.pathFor(QStringLiteral("Libera"), QStringLiteral("#a|b"), mapping);
    QCOMPARE(path, QDir(newNetworkDir).filePath(
                        omaircWireStorageSegment(QStringLiteral("#a|b"))));
    QVERIFY(QFile::exists(path));
    QVERIFY(!QFile::exists(legacyFile));
}

void StoragePathTest::transcriptPathPreservesNormalizedTarget()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString root = dir.filePath(QStringLiteral("logs"));
    QVERIFY(QDir().mkpath(root));

    IrcConversationLog log(root);
    const IrcCaseMapping rfc1459;
    const QString path =
        log.pathFor(QStringLiteral("libera"), QStringLiteral("#Omarchy"), rfc1459);
    const QString expectedSegment =
        omaircWireStorageSegment(QStringLiteral("#Omarchy"));
    QCOMPARE(path, QDir(QDir(root).filePath(omaircStorageSegment(QStringLiteral("libera"))))
                        .filePath(expectedSegment));
    QVERIFY(expectedSegment != omaircTargetSegment(QStringLiteral("#Omarchy"), rfc1459));
}

void StoragePathTest::retriesLegacyTranscriptMigrationAfterRenameFailure()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString root = dir.filePath(QStringLiteral("logs"));
    QVERIFY(QDir().mkpath(root));

    const QString legacyNetworkDir =
        QDir(root).filePath(legacyStorageSegment(QStringLiteral("Libera")));
    QVERIFY(QDir().mkpath(legacyNetworkDir));
    const QString legacyFile =
        QDir(legacyNetworkDir).filePath(legacyStorageSegment(QStringLiteral("#a|b")));
    {
        QFile file(legacyFile);
        QVERIFY(file.open(QIODevice::WriteOnly));
        file.write("legacy\n");
    }

    const QString newPath =
        QDir(QDir(root).filePath(omaircStorageSegment(QStringLiteral("Libera"))))
            .filePath(omaircWireStorageSegment(QStringLiteral("#a|b")));
    QVERIFY(QDir().mkpath(newPath));

    IrcConversationLog log(root);
    const IrcCaseMapping mapping;
    const QString path =
        log.pathFor(QStringLiteral("Libera"), QStringLiteral("#a|b"), mapping);
    QCOMPARE(path, newPath);
    QVERIFY(QFile::exists(legacyFile));

    const QString secondPath =
        log.pathFor(QStringLiteral("Libera"), QStringLiteral("#a|b"), mapping);
    QCOMPARE(secondPath, newPath);
    QVERIFY(QFile::exists(legacyFile));

    QVERIFY(QDir(newPath).removeRecursively());
    const QString migratedPath =
        log.pathFor(QStringLiteral("Libera"), QStringLiteral("#a|b"), mapping);
    QCOMPARE(migratedPath, newPath);
    QVERIFY(QFile::exists(migratedPath));
    QVERIFY(!QFile::exists(legacyFile));
}

void StoragePathTest::migratesLegacyTranscriptForDeviceLikeNames()
{
#ifndef Q_OS_WIN
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString root = dir.filePath(QStringLiteral("logs"));
    QVERIFY(QDir().mkpath(root));

    const QString legacyNetworkDir =
        QDir(root).filePath(legacyStorageSegment(QStringLiteral("net-1")));
    QVERIFY(QDir().mkpath(legacyNetworkDir));
    const QString legacyFile =
        QDir(legacyNetworkDir).filePath(legacyStorageSegment(QStringLiteral("con")));
    {
        QFile file(legacyFile);
        QVERIFY(file.open(QIODevice::WriteOnly));
        file.write("legacy\n");
    }

    IrcConversationLog log(root);
    const IrcCaseMapping mapping;
    const QString path =
        log.pathFor(QStringLiteral("net-1"), QStringLiteral("con"), mapping);
    QCOMPARE(path, QDir(QDir(root).filePath(omaircStorageSegment(QStringLiteral("net-1"))))
                        .filePath(omaircStorageSegment(QStringLiteral("con"))));
    QVERIFY(QFile::exists(path));
    QVERIFY(!QFile::exists(legacyFile));
#endif
}

void StoragePathTest::collapsesEquivalentCursorFiles()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    qputenv("OMAIRC_CURSOR_ROOT", dir.path().toUtf8());

    const QString networkDir =
        QDir(dir.path()).filePath(omaircStorageSegment(QStringLiteral("net-1")));
    QVERIFY(QDir().mkpath(networkDir));

    const QString olderPath =
        QDir(networkDir).filePath(QStringLiteral("#Omarchy.json"));
    const QString newerPath =
        QDir(networkDir).filePath(QStringLiteral("#omarchy.json"));
    {
        QFile older(olderPath);
        QVERIFY(older.open(QIODevice::WriteOnly));
        QJsonObject object;
        object.insert(QStringLiteral("timestamp"),
                      QStringLiteral("2011-10-19T16:40:00.000Z"));
        object.insert(QStringLiteral("sequence"), 1);
        older.write(QJsonDocument(object).toJson(QJsonDocument::Compact));
        older.write("\n");
    }
    {
        QFile newer(newerPath);
        QVERIFY(newer.open(QIODevice::WriteOnly));
        QJsonObject object;
        object.insert(QStringLiteral("timestamp"),
                      QStringLiteral("2011-10-19T16:42:00.000Z"));
        object.insert(QStringLiteral("sequence"), 2);
        newer.write(QJsonDocument(object).toJson(QJsonDocument::Compact));
        newer.write("\n");
    }

    OmaircCliCursorStore store;
    const IrcCaseMapping mapping;
    const std::optional<OmaircCliCursor> loaded =
        store.load(QStringLiteral("net-1"), QStringLiteral("#omarchy"), mapping,
                   true);
    QVERIFY(loaded.has_value());
    QCOMPARE(loaded->sequence, 2);
    QVERIFY(!QFile::exists(olderPath));

    const QString canonicalPath = QDir(networkDir).filePath(
        omaircWireStorageSegment(QStringLiteral("#omarchy"), QStringLiteral(".json"))
        + QStringLiteral(".json"));
    QVERIFY(QFile::exists(canonicalPath));
    QVERIFY(!QFile::exists(newerPath) || canonicalPath == newerPath);

    qunsetenv("OMAIRC_CURSOR_ROOT");
}

void StoragePathTest::keepsNewestLegacyCursorAcrossEncodings()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    qputenv("OMAIRC_CURSOR_ROOT", dir.path().toUtf8());

    const QString networkDir =
        QDir(dir.path()).filePath(omaircStorageSegment(QStringLiteral("net-1")));
    QVERIFY(QDir().mkpath(networkDir));

    const QString olderPath =
        QDir(networkDir).filePath(legacyStorageSegment(QStringLiteral("#foo|bar"))
                                + QStringLiteral(".json"));
    const QString newerPath =
        QDir(networkDir).filePath(QStringLiteral("#Foo|bar.json"));
    {
        QFile older(olderPath);
        QVERIFY(older.open(QIODevice::WriteOnly));
        QJsonObject object;
        object.insert(QStringLiteral("timestamp"),
                      QStringLiteral("2011-10-19T16:40:00.000Z"));
        object.insert(QStringLiteral("sequence"), 1);
        older.write(QJsonDocument(object).toJson(QJsonDocument::Compact));
        older.write("\n");
    }
    {
        QFile newer(newerPath);
        QVERIFY(newer.open(QIODevice::WriteOnly));
        QJsonObject object;
        object.insert(QStringLiteral("timestamp"),
                      QStringLiteral("2011-10-19T16:42:00.000Z"));
        object.insert(QStringLiteral("sequence"), 2);
        newer.write(QJsonDocument(object).toJson(QJsonDocument::Compact));
        newer.write("\n");
    }

    OmaircCliCursorStore store;
    const IrcCaseMapping mapping;
    const std::optional<OmaircCliCursor> loaded =
        store.load(QStringLiteral("net-1"), QStringLiteral("#foo|bar"), mapping,
                   true);
    QVERIFY(loaded.has_value());
    QCOMPARE(loaded->sequence, 2);
    QVERIFY(!QFile::exists(olderPath));
    QVERIFY(!QFile::exists(newerPath));

    const QString canonicalPath = QDir(networkDir).filePath(
        omaircWireStorageSegment(QStringLiteral("#foo|bar"), QStringLiteral(".json"))
        + QStringLiteral(".json"));
    QVERIFY(QFile::exists(canonicalPath));

    qunsetenv("OMAIRC_CURSOR_ROOT");
}

void StoragePathTest::ignoresNewStyleCursorEncodingAsLegacy()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    qputenv("OMAIRC_CURSOR_ROOT", dir.path().toUtf8());

    const QString networkDir =
        QDir(dir.path()).filePath(omaircStorageSegment(QStringLiteral("net-1")));
    QVERIFY(QDir().mkpath(networkDir));

    const QString newStylePath =
        QDir(networkDir).filePath(omaircWireStorageSegment(QStringLiteral("#foo|bar"),
                                                           QStringLiteral(".json"))
                          + QStringLiteral(".json"));
    {
        QFile file(newStylePath);
        QVERIFY(file.open(QIODevice::WriteOnly));
        QJsonObject object;
        object.insert(QStringLiteral("timestamp"),
                      QStringLiteral("2011-10-19T16:42:00.000Z"));
        object.insert(QStringLiteral("sequence"), 42);
        file.write(QJsonDocument(object).toJson(QJsonDocument::Compact));
        file.write("\n");
    }

    OmaircCliCursorStore store;
    const IrcCaseMapping mapping;
    const std::optional<OmaircCliCursor> loaded =
        store.load(QStringLiteral("net-1"), QStringLiteral("#foo|bar"), mapping,
                   true);
    QVERIFY(loaded.has_value());
    QCOMPARE(loaded->sequence, 42);
    QVERIFY(QFile::exists(newStylePath));
    QCOMPARE(newStylePath,
             QDir(networkDir).filePath(QStringLiteral("#foo%7cbar.json")));

    qunsetenv("OMAIRC_CURSOR_ROOT");
}

void StoragePathTest::doesNotCollapseBracketCursorsBeforeCaseMappingKnown()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    qputenv("OMAIRC_CURSOR_ROOT", dir.path().toUtf8());

    const QString networkDir =
        QDir(dir.path()).filePath(omaircStorageSegment(QStringLiteral("net-1")));
    QVERIFY(QDir().mkpath(networkDir));

    const QString bracketPath = QDir(networkDir).filePath(QStringLiteral("#foo[.json"));
    const QString bracePath = QDir(networkDir).filePath(QStringLiteral("#foo{.json"));
    {
        QFile bracket(bracketPath);
        QVERIFY(bracket.open(QIODevice::WriteOnly));
        QJsonObject object;
        object.insert(QStringLiteral("timestamp"),
                      QStringLiteral("2011-10-19T16:40:00.000Z"));
        object.insert(QStringLiteral("sequence"), 1);
        bracket.write(QJsonDocument(object).toJson(QJsonDocument::Compact));
        bracket.write("\n");
    }
    {
        QFile brace(bracePath);
        QVERIFY(brace.open(QIODevice::WriteOnly));
        QJsonObject object;
        object.insert(QStringLiteral("timestamp"),
                      QStringLiteral("2011-10-19T16:42:00.000Z"));
        object.insert(QStringLiteral("sequence"), 2);
        brace.write(QJsonDocument(object).toJson(QJsonDocument::Compact));
        brace.write("\n");
    }

    OmaircCliCursorStore store;
    const IrcCaseMapping mapping;
    const std::optional<OmaircCliCursor> loadedBracket =
        store.load(QStringLiteral("net-1"), QStringLiteral("#foo["), mapping,
                   false);
    QVERIFY(loadedBracket.has_value());
    QCOMPARE(loadedBracket->sequence, 1);
    QVERIFY(QFile::exists(bracketPath));
    QVERIFY(QFile::exists(bracePath));

    const IrcCaseMapping ascii(IrcCaseMapping::Kind::Ascii);
    const std::optional<OmaircCliCursor> loadedAscii =
        store.load(QStringLiteral("net-1"), QStringLiteral("#foo["), ascii,
                   true);
    QVERIFY(loadedAscii.has_value());
    QCOMPARE(loadedAscii->sequence, 1);
    QVERIFY(QFile::exists(bracketPath));
    QVERIFY(QFile::exists(bracePath));

    qunsetenv("OMAIRC_CURSOR_ROOT");
}

void StoragePathTest::collapsesBracketCursorsWhenCaseMappingKnown()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    qputenv("OMAIRC_CURSOR_ROOT", dir.path().toUtf8());

    const QString networkDir =
        QDir(dir.path()).filePath(omaircStorageSegment(QStringLiteral("net-1")));
    QVERIFY(QDir().mkpath(networkDir));

    const QString bracketPath = QDir(networkDir).filePath(QStringLiteral("#foo[.json"));
    const QString bracePath = QDir(networkDir).filePath(QStringLiteral("#foo{.json"));
    {
        QFile bracket(bracketPath);
        QVERIFY(bracket.open(QIODevice::WriteOnly));
        QJsonObject object;
        object.insert(QStringLiteral("timestamp"),
                      QStringLiteral("2011-10-19T16:40:00.000Z"));
        object.insert(QStringLiteral("sequence"), 1);
        bracket.write(QJsonDocument(object).toJson(QJsonDocument::Compact));
        bracket.write("\n");
    }
    {
        QFile brace(bracePath);
        QVERIFY(brace.open(QIODevice::WriteOnly));
        QJsonObject object;
        object.insert(QStringLiteral("timestamp"),
                      QStringLiteral("2011-10-19T16:42:00.000Z"));
        object.insert(QStringLiteral("sequence"), 2);
        brace.write(QJsonDocument(object).toJson(QJsonDocument::Compact));
        brace.write("\n");
    }

    OmaircCliCursorStore store;
    const IrcCaseMapping mapping;
    const std::optional<OmaircCliCursor> loaded =
        store.load(QStringLiteral("net-1"), QStringLiteral("#foo["), mapping,
                   true);
    QVERIFY(loaded.has_value());
    QCOMPARE(loaded->sequence, 2);
    QVERIFY(QFile::exists(bracketPath));
    QVERIFY(!QFile::exists(bracePath));

    const QString canonicalPath = QDir(networkDir).filePath(
        omaircWireStorageSegment(QStringLiteral("#foo["), QStringLiteral(".json"))
        + QStringLiteral(".json"));
    QVERIFY(QFile::exists(canonicalPath));
    QCOMPARE(canonicalPath, QDir(networkDir).filePath(QStringLiteral("#foo[.json")));

    qunsetenv("OMAIRC_CURSOR_ROOT");
}

void StoragePathTest::doesNotOverwriteExistingTranscriptDuringMigration()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString root = dir.filePath(QStringLiteral("logs"));
    QVERIFY(QDir().mkpath(root));

    const QString legacyNetworkDir =
        QDir(root).filePath(legacyStorageSegment(QStringLiteral("Libera")));
    QVERIFY(QDir().mkpath(legacyNetworkDir));
    const QString legacyFile =
        QDir(legacyNetworkDir).filePath(legacyStorageSegment(QStringLiteral("#a|b")));
    {
        QFile file(legacyFile);
        QVERIFY(file.open(QIODevice::WriteOnly));
        file.write("legacy\n");
    }

    const QString newPath =
        QDir(QDir(root).filePath(omaircStorageSegment(QStringLiteral("Libera"))))
            .filePath(omaircWireStorageSegment(QStringLiteral("#a|b")));
    QVERIFY(QDir().mkpath(QFileInfo(newPath).absolutePath()));
    {
        QFile file(newPath);
        QVERIFY(file.open(QIODevice::WriteOnly));
        file.write("newer\n");
    }

    IrcConversationLog log(root);
    const IrcCaseMapping mapping;
    const QString path =
        log.pathFor(QStringLiteral("Libera"), QStringLiteral("#a|b"), mapping);
    QCOMPARE(path, newPath);
    QVERIFY(QFile::exists(legacyFile));

    QFile preserved(newPath);
    QVERIFY(preserved.open(QIODevice::ReadOnly));
    QCOMPARE(preserved.readAll(), QByteArray("newer\n"));
}

void StoragePathTest::migratesTranscriptFilesWithoutRenamingSharedNetworkDir()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    const QString root = dir.filePath(QStringLiteral("logs"));
    QVERIFY(QDir().mkpath(root));

    const QString legacyNetworkDir =
        QDir(root).filePath(legacyStorageSegment(QStringLiteral("Libera")));
    QVERIFY(QDir().mkpath(legacyNetworkDir));
    const QString sharedNetworkDir =
        QDir(root).filePath(omaircStorageSegment(QStringLiteral("libera")));
    QVERIFY(QFile::link(legacyNetworkDir, sharedNetworkDir));

    const QString legacyFile =
        QDir(legacyNetworkDir).filePath(legacyStorageSegment(QStringLiteral("#a|b")));
    {
        QFile file(legacyFile);
        QVERIFY(file.open(QIODevice::WriteOnly));
        file.write("legacy\n");
    }

    IrcConversationLog log(root);
    const IrcCaseMapping mapping;
    const QString path =
        log.pathFor(QStringLiteral("libera"), QStringLiteral("#a|b"), mapping);
    const QString expectedPath =
        QDir(sharedNetworkDir).filePath(omaircWireStorageSegment(QStringLiteral("#a|b")));
    QCOMPARE(path, expectedPath);
    QVERIFY(QFile::exists(path));
    QVERIFY(!QFile::exists(legacyFile));
    QVERIFY(QDir(root).exists(legacyStorageSegment(QStringLiteral("Libera"))));
    QVERIFY(storageDirSegmentsShareLocation(QDir(root),
                                            legacyStorageSegment(QStringLiteral("Libera")),
                                            omaircStorageSegment(QStringLiteral("libera"))));
}

void StoragePathTest::cursorWinnerUsesMsgidAndSequence()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    qputenv("OMAIRC_CURSOR_ROOT", dir.path().toUtf8());

    const QString networkDir =
        QDir(dir.path()).filePath(omaircStorageSegment(QStringLiteral("net-1")));
    QVERIFY(QDir().mkpath(networkDir));

    const QString olderPath =
        QDir(networkDir).filePath(QStringLiteral("#chan.json"));
    const QString newerPath =
        QDir(networkDir).filePath(QStringLiteral("#Chan.json"));
    const QString timestamp = QStringLiteral("2011-10-19T16:40:00.000Z");
    {
        QFile older(olderPath);
        QVERIFY(older.open(QIODevice::WriteOnly));
        QJsonObject object;
        object.insert(QStringLiteral("timestamp"), timestamp);
        object.insert(QStringLiteral("msgid"), QStringLiteral("a"));
        object.insert(QStringLiteral("sequence"), 1);
        older.write(QJsonDocument(object).toJson(QJsonDocument::Compact));
        older.write("\n");
    }
    {
        QFile newer(newerPath);
        QVERIFY(newer.open(QIODevice::WriteOnly));
        QJsonObject object;
        object.insert(QStringLiteral("timestamp"), timestamp);
        object.insert(QStringLiteral("msgid"), QStringLiteral("b"));
        object.insert(QStringLiteral("sequence"), 1);
        newer.write(QJsonDocument(object).toJson(QJsonDocument::Compact));
        newer.write("\n");
    }

    OmaircCliCursorStore store;
    const IrcCaseMapping mapping;
    const std::optional<OmaircCliCursor> loaded =
        store.load(QStringLiteral("net-1"), QStringLiteral("#chan"), mapping, true);
    QVERIFY(loaded.has_value());
    QCOMPARE(loaded->msgid, QStringLiteral("b"));
    QVERIFY(!QFile::exists(newerPath));
    QVERIFY(QFile::exists(olderPath));

    qunsetenv("OMAIRC_CURSOR_ROOT");
}

void StoragePathTest::cursorPathStableAcrossCaseMappingKnown()
{
    QTemporaryDir dir;
    QVERIFY(dir.isValid());
    qputenv("OMAIRC_CURSOR_ROOT", dir.path().toUtf8());

    OmaircCliCursorStore store;
    const IrcCaseMapping mapping;
    const QString expected =
        QDir(QDir(dir.path()).filePath(omaircStorageSegment(QStringLiteral("net-1"))))
            .filePath(omaircWireStorageSegment(QStringLiteral("#foo["),
                                               QStringLiteral(".json"))
                      + QStringLiteral(".json"));

    const std::optional<OmaircCliCursor> beforeKnown =
        store.load(QStringLiteral("net-1"), QStringLiteral("#foo["), mapping, false);
    Q_UNUSED(beforeKnown);
    const std::optional<OmaircCliCursor> afterKnown =
        store.load(QStringLiteral("net-1"), QStringLiteral("#foo["), mapping, true);
    Q_UNUSED(afterKnown);

    OmaircCliCursor cursor;
    cursor.timestamp =
        QDateTime::fromString(QStringLiteral("2011-10-19T16:40:00.000Z"),
                              Qt::ISODateWithMs);
    cursor.sequence = 1;
    QVERIFY(store.save(QStringLiteral("net-1"), QStringLiteral("#foo["), mapping,
                       false, cursor));
    QVERIFY(QFile::exists(expected));

    cursor.sequence = 2;
    QVERIFY(store.save(QStringLiteral("net-1"), QStringLiteral("#foo["), mapping,
                       true, cursor));
    QVERIFY(QFile::exists(expected));
    QVERIFY(!QFile::exists(QDir(QDir(dir.path())
                                    .filePath(omaircStorageSegment(QStringLiteral("net-1"))))
                               .filePath(QStringLiteral("#foo{.json"))));

    qunsetenv("OMAIRC_CURSOR_ROOT");
}

void StoragePathTest::decodesOmaircStorageSegments()
{
    QCOMPARE(decodeOmaircStorageSegment(QStringLiteral("#%4fmarchy")),
             QStringLiteral("#Omarchy"));
    QCOMPARE(decodeOmaircStorageSegment(QStringLiteral("#omarchy")),
             QStringLiteral("#omarchy"));
    QCOMPARE(decodeOmaircStorageSegment(QStringLiteral("#foo%7cbar")),
             QStringLiteral("#foo|bar"));
}

void StoragePathTest::nulTargetsDoNotSharePaths()
{
    const IrcCaseMapping mapping;
    const QString left = QStringLiteral("a") + QChar(0) + QStringLiteral("b");
    const QString right = QStringLiteral("a") + QChar(0) + QChar(0) + QStringLiteral("b");
    QVERIFY(omaircTargetSegment(left, mapping)
            != omaircTargetSegment(right, mapping));
}

int runStoragePathTests(int argc, char **argv)
{
    StoragePathTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_storagepath.moc"
