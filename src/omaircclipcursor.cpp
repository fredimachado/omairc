#include "omaircclipcursor.h"

#include "irc/irccasemapping.h"
#include "irc/ircstoragepath.h"

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonParseError>
#include <QSaveFile>
#include <QStandardPaths>

#include <string_view>

namespace {

const IrcCaseMapping kAsciiCaseMapping(IrcCaseMapping::Kind::Ascii);

const IrcCaseMapping &cursorSegmentMapping(const IrcCaseMapping &mapping,
                                           bool caseMappingKnown)
{
    return caseMappingKnown ? mapping : kAsciiCaseMapping;
}

bool cursorTargetsEquivalent(const QString &left,
                             const QString &right,
                             const IrcCaseMapping &mapping,
                             bool caseMappingKnown)
{
    const QByteArray leftUtf8 = left.toUtf8();
    const QByteArray rightUtf8 = right.toUtf8();
    const std::string_view leftView(leftUtf8.constData(), leftUtf8.size());
    const std::string_view rightView(rightUtf8.constData(), rightUtf8.size());
    if (caseMappingKnown)
        return mapping.equals(leftView, rightView);
    return kAsciiCaseMapping.equals(leftView, rightView);
}

constexpr QFileDevice::Permissions kOwnerDir =
    QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ExeOwner;
constexpr QFileDevice::Permissions kOwnerFile =
    QFileDevice::ReadOwner | QFileDevice::WriteOwner;

const QString kJsonExtension = QStringLiteral(".json");

bool tightenOwnerDir(const QString &path)
{
    return QFile::setPermissions(path, kOwnerDir);
}

bool atOrUnderRoot(const QString &path, const QString &root)
{
    const QString cleanPath = QDir::cleanPath(path);
    const QString cleanRoot = QDir::cleanPath(root);
    return cleanPath == cleanRoot
        || cleanPath.startsWith(cleanRoot + QLatin1Char('/'));
}

bool prepareTree(const QString &filePath)
{
    const QString dir = QFileInfo(filePath).absolutePath();
    if (!QDir().mkpath(dir))
        return false;
    const QString root =
        QDir(OmaircCliCursorStore::defaultRoot()).absolutePath();
    QDir walk(dir);
    while (true) {
        const QString current = walk.absolutePath();
        if (!atOrUnderRoot(current, root))
            break;
        tightenOwnerDir(current);
        if (QDir::cleanPath(current) == QDir::cleanPath(root))
            break;
        if (!walk.cdUp())
            break;
    }
    return true;
}

std::optional<OmaircCliCursor> readCursorFile(const QString &path)
{
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly))
        return std::nullopt;
    QJsonParseError error;
    const QJsonDocument document = QJsonDocument::fromJson(file.readAll(), &error);
    if (error.error != QJsonParseError::NoError || !document.isObject())
        return std::nullopt;
    const QJsonObject object = document.object();
    OmaircCliCursor cursor;
    cursor.timestamp = QDateTime::fromString(
        object.value(QStringLiteral("timestamp")).toString(), Qt::ISODateWithMs);
    if (!cursor.timestamp.isValid())
        return std::nullopt;
    cursor.timestamp = cursor.timestamp.toUTC();
    cursor.msgid = object.value(QStringLiteral("msgid")).toString();
    cursor.sequence = object.value(QStringLiteral("sequence")).toInteger();
    return cursor;
}

void migrateNetworkCursorRoot(const QString &root, const QString &networkId)
{
    QDir rootDir(root);
    const QString legacyNetworkDir = legacyStorageSegment(networkId);
    const QString newNetworkDir = omaircStorageSegment(networkId);
    if (legacyNetworkDir != newNetworkDir
        && legacyStorageDirExists(rootDir, legacyNetworkDir)
        && !rootDir.exists(newNetworkDir)) {
        rootDir.rename(legacyNetworkDir, newNetworkDir);
    }

    const QString legacyRootCursor =
        QDir(root).filePath(legacyStorageSegment(networkId) + kJsonExtension);
    const QString newRootCursor = QDir(root).filePath(
        omaircStorageSegment(networkId, kJsonExtension) + kJsonExtension);
    if (legacyStoragePathExists(legacyRootCursor, networkId, kJsonExtension)
        && !QFile::exists(newRootCursor)) {
        QFile::rename(legacyRootCursor, newRootCursor);
    }
}

struct CursorCandidate
{
    QString path;
    OmaircCliCursor cursor;
};

QString canonicalPathOf(const QString &path)
{
    const QString canonical = QFileInfo(path).canonicalFilePath();
    return canonical.isEmpty() ? QDir::cleanPath(path) : canonical;
}

bool sameFile(const QString &a, const QString &b)
{
    const QString canonicalA = canonicalPathOf(a);
    const QString canonicalB = canonicalPathOf(b);
    return !canonicalA.isEmpty() && canonicalA == canonicalB;
}

void collectEquivalentCursorCandidates(const QString &networkDirPath,
                                       const QString &target,
                                       const IrcCaseMapping &mapping,
                                       bool caseMappingKnown,
                                       QVector<CursorCandidate> &matches)
{
    QDir networkDir(networkDirPath);
    if (!networkDir.exists())
        return;

    const QStringList files =
        networkDir.entryList({QStringLiteral("*.json")}, QDir::Files);
    for (const QString &fileName : files) {
        QString stem = fileName;
        if (!stem.endsWith(kJsonExtension))
            continue;
        stem.chop(kJsonExtension.size());
        if (legacyStorageSegment(decodeLegacySegment(stem)) != stem)
            continue;
        const QString decoded = decodeLegacySegment(stem);
        if (!cursorTargetsEquivalent(decoded, target, mapping, caseMappingKnown))
            continue;
        const QString path = networkDir.filePath(fileName);
        bool alreadyListed = false;
        for (const CursorCandidate &existing : matches) {
            if (sameFile(existing.path, path)) {
                alreadyListed = true;
                break;
            }
        }
        if (alreadyListed)
            continue;
        const std::optional<OmaircCliCursor> cursor = readCursorFile(path);
        if (!cursor)
            continue;
        matches.push_back({path, *cursor});
    }
}

void collapseEquivalentCursorFiles(const QString &root,
                                   const QString &networkId,
                                   const QString &target,
                                   const IrcCaseMapping &mapping,
                                   bool caseMappingKnown,
                                   const QString &canonicalPath)
{
    QVector<CursorCandidate> matches;
    const QString legacyNetworkDir = legacyStorageSegment(networkId);
    const QString newNetworkDir = omaircStorageSegment(networkId);
    collectEquivalentCursorCandidates(
        QDir(root).filePath(legacyNetworkDir), target, mapping, caseMappingKnown,
        matches);
    if (legacyNetworkDir != newNetworkDir) {
        collectEquivalentCursorCandidates(
            QDir(root).filePath(newNetworkDir), target, mapping, caseMappingKnown,
            matches);
    }

    if (QFile::exists(canonicalPath)) {
        bool alreadyListed = false;
        for (const CursorCandidate &candidate : matches) {
            if (sameFile(candidate.path, canonicalPath)) {
                alreadyListed = true;
                break;
            }
        }
        if (!alreadyListed) {
            const std::optional<OmaircCliCursor> cursor =
                readCursorFile(canonicalPath);
            if (cursor)
                matches.push_back({canonicalPath, *cursor});
        }
    }

    if (matches.isEmpty())
        return;

    auto newest = matches.begin();
    for (auto it = matches.begin() + 1; it != matches.end(); ++it) {
        if (it->cursor.timestamp > newest->cursor.timestamp)
            newest = it;
    }

    if (!sameFile(newest->path, canonicalPath)) {
        prepareTree(canonicalPath);
        if (QFile::exists(canonicalPath) && !sameFile(canonicalPath, newest->path)
            && !QFile::remove(canonicalPath)) {
            return;
        }
        if (!QFile::rename(newest->path, canonicalPath))
            return;
    }

    for (const CursorCandidate &candidate : matches) {
        if (sameFile(candidate.path, canonicalPath))
            continue;
        QFile::remove(candidate.path);
    }
}

void migrateCursorTarget(const QString &root,
                       const QString &networkId,
                       const QString &target,
                       const IrcCaseMapping &mapping,
                       bool caseMappingKnown,
                       const QString &canonicalPath)
{
    migrateNetworkCursorRoot(root, networkId);
    collapseEquivalentCursorFiles(
        root, networkId, target, mapping, caseMappingKnown, canonicalPath);
}

}

QString OmaircCliCursorStore::defaultRoot()
{
    const QString scoped = QString::fromUtf8(qgetenv("OMAIRC_CURSOR_ROOT"));
    if (!scoped.isEmpty())
        return scoped;
    QString state = QString::fromUtf8(qgetenv("XDG_STATE_HOME"));
    if (state.isEmpty())
        state = QStandardPaths::writableLocation(QStandardPaths::GenericStateLocation);
    if (state.isEmpty())
        state = QStandardPaths::writableLocation(QStandardPaths::TempLocation);
    return QDir(state).filePath(QStringLiteral("omairc/cli-cursors"));
}

QString OmaircCliCursorStore::pathFor(const QString &networkId,
                                      const QString &target,
                                      const IrcCaseMapping &mapping,
                                      bool caseMappingKnown) const
{
    const QString root = defaultRoot();
    QString canonicalPath;
    if (target.isEmpty()) {
        canonicalPath = QDir(root).filePath(
            omaircStorageSegment(networkId, kJsonExtension) + kJsonExtension);
        migrateNetworkCursorRoot(root, networkId);
        return canonicalPath;
    }

    const IrcCaseMapping &segmentMapping =
        cursorSegmentMapping(mapping, caseMappingKnown);
    canonicalPath =
        QDir(QDir(root).filePath(omaircStorageSegment(networkId)))
            .filePath(omaircTargetSegment(target, segmentMapping, kJsonExtension)
                      + kJsonExtension);
    migrateCursorTarget(
        root, networkId, target, mapping, caseMappingKnown, canonicalPath);
    return canonicalPath;
}

std::optional<OmaircCliCursor> OmaircCliCursorStore::load(
    const QString &networkId,
    const QString &target,
    const IrcCaseMapping &mapping,
    bool caseMappingKnown) const
{
    return readCursorFile(pathFor(networkId, target, mapping, caseMappingKnown));
}

bool OmaircCliCursorStore::save(const QString &networkId,
                                const QString &target,
                                const IrcCaseMapping &mapping,
                                bool caseMappingKnown,
                                const OmaircCliCursor &cursor) const
{
    const QString path = pathFor(networkId, target, mapping, caseMappingKnown);
    if (!prepareTree(path))
        return false;
    QJsonObject object;
    const QDateTime when = cursor.timestamp.isValid()
        ? cursor.timestamp.toUTC()
        : QDateTime::currentDateTimeUtc();
    object.insert(QStringLiteral("timestamp"),
                  when.toString(Qt::ISODateWithMs));
    if (!cursor.msgid.isEmpty())
        object.insert(QStringLiteral("msgid"), cursor.msgid);
    object.insert(QStringLiteral("sequence"), cursor.sequence);
    QSaveFile file(path);
    if (!file.open(QIODevice::WriteOnly))
        return false;
    const QByteArray line =
        QJsonDocument(object).toJson(QJsonDocument::Compact) + '\n';
    if (file.write(line) != line.size())
        return false;
    if (!file.commit())
        return false;
    return QFile::setPermissions(path, kOwnerFile);
}
