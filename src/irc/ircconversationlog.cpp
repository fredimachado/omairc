#include "ircconversationlog.h"

#include "ircsecretpolicy.h"
#include "ircstoragepath.h"

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonParseError>
#include <QIODevice>
#include <QStandardPaths>
#include <QTemporaryDir>

#include <algorithm>
#include <optional>
#include <string_view>

namespace
{
constexpr QFileDevice::Permissions kOwnerDir =
    QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ExeOwner;
constexpr QFileDevice::Permissions kOwnerFile =
    QFileDevice::ReadOwner | QFileDevice::WriteOwner;

QString flattenText(QString text)
{
    text.replace(QLatin1Char('\n'), QLatin1Char(' '));
    text.replace(QLatin1Char('\r'), QLatin1Char(' '));
    return text;
}

bool tightenOwnerDir(const QString &path)
{
    return QFile::setPermissions(path, kOwnerDir);
}

bool prepareTree(const QString &filePath)
{
    const QString dir = QFileInfo(filePath).absolutePath();
    if (!QDir().mkpath(dir))
        return false;
    QDir walk(dir);
    tightenOwnerDir(walk.absolutePath());
    if (walk.cdUp())
        tightenOwnerDir(walk.absolutePath());
    if (walk.cdUp())
        tightenOwnerDir(walk.absolutePath());
    return true;
}

QByteArray formatLine(const IrcTranscriptLine &line)
{
    QJsonObject object;
    const QDateTime when = line.timestamp.isValid()
        ? line.timestamp.toUTC()
        : QDateTime::currentDateTimeUtc();
    object.insert(QStringLiteral("timestamp"),
                  when.toString(Qt::ISODateWithMs));
    object.insert(QStringLiteral("author"), flattenText(line.author));
    object.insert(QStringLiteral("kind"), line.kind);
    object.insert(QStringLiteral("body"), flattenText(line.body));
    if (!line.msgid.isEmpty())
        object.insert(QStringLiteral("msgid"), line.msgid);
    return QJsonDocument(object).toJson(QJsonDocument::Compact) + '\n';
}

std::optional<IrcTranscriptLine> parseLine(const QByteArray &raw)
{
    QJsonParseError error;
    const QJsonDocument document = QJsonDocument::fromJson(raw, &error);
    if (error.error != QJsonParseError::NoError || !document.isObject())
        return std::nullopt;
    const QJsonObject object = document.object();
    const QString kind = object.value(QStringLiteral("kind")).toString();
    const QString body = object.value(QStringLiteral("body")).toString();
    if (kind.isEmpty())
        return std::nullopt;
    IrcTranscriptLine line;
    line.timestamp = QDateTime::fromString(
        object.value(QStringLiteral("timestamp")).toString(), Qt::ISODateWithMs);
    line.author = object.value(QStringLiteral("author")).toString();
    line.kind = kind;
    line.body = body;
    line.msgid = object.value(QStringLiteral("msgid")).toString();
    return line;
}

bool appendBytes(const QString &path, const QByteArray &line)
{
    if (!prepareTree(path))
        return false;
    QFile file(path);
    if (!file.open(QIODevice::WriteOnly | QIODevice::Append))
        return false;
    if (!file.setPermissions(kOwnerFile))
        return false;
    return file.write(line) == line.size();
}

QString xdgLogsRoot()
{
    QString state = QString::fromUtf8(qgetenv("XDG_STATE_HOME"));
    if (state.isEmpty())
        state = QStandardPaths::writableLocation(QStandardPaths::GenericStateLocation);
    if (state.isEmpty())
        state = QStandardPaths::writableLocation(QStandardPaths::TempLocation);
    return QDir(state).filePath(QStringLiteral("omairc/logs"));
}

void migrateLegacyTranscriptPath(const QString &root,
                                 const QString &networkId,
                                 const QString &target,
                                 const QString &newPath)
{
    const QString legacyNetworkDir = legacyStorageSegment(networkId);
    const QString newNetworkDir = omaircStorageSegment(networkId);
    QDir rootDir(root);
    QString networkDirPath = rootDir.filePath(legacyNetworkDir);
    if (legacyNetworkDir != newNetworkDir) {
        const bool sharedNetworkDir = storageDirSegmentsShareLocation(
            rootDir, legacyNetworkDir, newNetworkDir);
        if (!sharedNetworkDir
            && legacyStorageDirExists(rootDir, legacyNetworkDir)
            && !rootDir.exists(newNetworkDir)) {
            if (rootDir.rename(legacyNetworkDir, newNetworkDir))
                networkDirPath = rootDir.filePath(newNetworkDir);
        } else if (rootDir.exists(newNetworkDir)) {
            networkDirPath = rootDir.filePath(newNetworkDir);
        }
    } else if (rootDir.exists(newNetworkDir)) {
        networkDirPath = rootDir.filePath(newNetworkDir);
    }

    const auto tryMigrateFile = [&](const QString &legacyPath) {
        if (!legacyStoragePathExists(legacyPath, target))
            return;
        if (QFile::exists(newPath) || storagePathsSameFile(legacyPath, newPath))
            return;
        prepareTree(newPath);
        QFile::rename(legacyPath, newPath);
    };

    const QString legacyPath =
        QDir(networkDirPath).filePath(legacyStorageSegment(target));
    tryMigrateFile(legacyPath);

    if (legacyNetworkDir != newNetworkDir
        && legacyStorageDirExists(rootDir, legacyNetworkDir)) {
        const QString legacyDirPath = rootDir.filePath(legacyNetworkDir);
        const QString legacyPathInLegacyDir =
            QDir(legacyDirPath).filePath(legacyStorageSegment(target));
        tryMigrateFile(legacyPathInLegacyDir);
    }
}
}

IrcConversationLog::IrcConversationLog()
{
    const QString scoped = QString::fromUtf8(qgetenv("OMAIRC_TRANSCRIPT_ROOT"));
    if (!scoped.isEmpty()) {
        m_root = scoped;
        return;
    }
    m_scratch = std::make_unique<QTemporaryDir>();
    m_root = QDir(m_scratch->path()).filePath(QStringLiteral("omairc/logs"));
}

IrcConversationLog::IrcConversationLog(QString root)
    : m_root(std::move(root))
{
}

IrcConversationLog::~IrcConversationLog() = default;

void IrcConversationLog::setRoot(QString root)
{
    m_scratch.reset();
    m_root = std::move(root);
}

QString IrcConversationLog::defaultRoot()
{
    return xdgLogsRoot();
}

const QString &IrcConversationLog::root() const
{
    return m_root;
}

QString IrcConversationLog::pathFor(const QString &networkId,
                                    const QString &target,
                                    const IrcCaseMapping &mapping) const
{
    Q_UNUSED(mapping);
    const QString newNetworkDir = omaircStorageSegment(networkId);
    const QString newTarget = omaircWireStorageSegment(target);
    const QString newPath =
        QDir(QDir(m_root).filePath(newNetworkDir)).filePath(newTarget);
    migrateLegacyTranscriptPath(m_root, networkId, target, newPath);
    return newPath;
}

bool IrcConversationLog::append(const QString &networkId,
                                const QString &target,
                                const IrcCaseMapping &mapping,
                                const IrcTranscriptLine &line)
{
    if (line.kind.isEmpty() || line.body.isEmpty())
        return true;
    if (!IrcSecretPolicy::allowsTranscript(line.body))
        return true;
    if (!IrcSecretPolicy::allowsTranscript(line.author))
        return true;
    const QByteArray bytes = formatLine(line);
    if (appendBytes(pathFor(networkId, target, mapping), bytes))
        return true;
    qWarning("Could not append the conversation log for %s %s",
             qUtf8Printable(networkId), qUtf8Printable(target));
    return false;
}

std::vector<IrcTranscriptLine> IrcConversationLog::readTail(
    const QString &networkId,
    const QString &target,
    const IrcCaseMapping &mapping,
    int maxLines) const
{
    if (maxLines <= 0)
        return {};
    QFile file(pathFor(networkId, target, mapping));
    if (!file.open(QIODevice::ReadOnly))
        return {};
    return readTail(&file, maxLines);
}

std::vector<IrcTranscriptLine> IrcConversationLog::readTail(
    QIODevice *device, int maxLines)
{
    std::vector<IrcTranscriptLine> lines;
    if (!device || maxLines <= 0 || device->isSequential())
        return lines;

    constexpr qint64 chunkSize = 4096;
    qint64 position = device->size();
    QByteArray pending;
    const auto accept = [&lines, maxLines](QByteArray raw) {
        raw = raw.trimmed();
        if (raw.isEmpty())
            return false;
        const std::optional<IrcTranscriptLine> parsed = parseLine(raw);
        if (!parsed)
            return false;
        if (!IrcSecretPolicy::allowsTranscript(parsed->body)
            || !IrcSecretPolicy::allowsTranscript(parsed->author)) {
            return false;
        }
        lines.push_back(*parsed);
        return int(lines.size()) == maxLines;
    };

    while (position > 0 && int(lines.size()) < maxLines) {
        const qint64 bytes = std::min(position, chunkSize);
        position -= bytes;
        if (!device->seek(position))
            break;
        const QByteArray chunk = device->read(bytes);
        if (chunk.size() != bytes)
            break;
        pending.prepend(chunk);

        qsizetype newline = pending.lastIndexOf('\n');
        while (newline >= 0) {
            const QByteArray raw = pending.sliced(newline + 1);
            pending.truncate(newline);
            if (accept(raw))
                break;
            newline = pending.lastIndexOf('\n');
        }
    }

    if (position == 0 && int(lines.size()) < maxLines)
        accept(pending);
    std::reverse(lines.begin(), lines.end());
    return lines;
}
