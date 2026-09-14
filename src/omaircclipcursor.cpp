#include "omaircclipcursor.h"

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonParseError>
#include <QSaveFile>
#include <QStandardPaths>

namespace {

constexpr QFileDevice::Permissions kOwnerDir =
    QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ExeOwner;
constexpr QFileDevice::Permissions kOwnerFile =
    QFileDevice::ReadOwner | QFileDevice::WriteOwner;

QString safeSegment(QString name)
{
    // Encode '%' first so a/b and a%2fb do not share a path. rfc1459
    // #Chan and #chan stay distinct cursor files.
    name.replace(QLatin1Char('%'), QLatin1String("%25"));
    name.replace(QLatin1Char('/'), QLatin1String("%2f"));
    name.replace(QLatin1Char('\\'), QLatin1String("%5c"));
    name.replace(QChar(0), QLatin1String("%00"));
    if (name.isEmpty() || name == QLatin1String(".") || name == QLatin1String(".."))
        return QStringLiteral("_");
    return name;
}

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
                                      const QString &target) const
{
    const QString root = defaultRoot();
    if (target.isEmpty())
        return QDir(root).filePath(safeSegment(networkId) + QLatin1String(".json"));
    return QDir(QDir(root).filePath(safeSegment(networkId)))
        .filePath(safeSegment(target) + QLatin1String(".json"));
}

std::optional<OmaircCliCursor> OmaircCliCursorStore::load(
    const QString &networkId, const QString &target) const
{
    QFile file(pathFor(networkId, target));
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

bool OmaircCliCursorStore::save(const QString &networkId,
                                const QString &target,
                                const OmaircCliCursor &cursor) const
{
    const QString path = pathFor(networkId, target);
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
