#include "ircconversationlog.h"

#include "ircsecretpolicy.h"

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonParseError>
#include <QStandardPaths>
#include <QTemporaryDir>

#include <optional>

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

QString safeSegment(QString name)
{
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
                                    const QString &target) const
{
    return QDir(QDir(m_root).filePath(safeSegment(networkId)))
        .filePath(safeSegment(target));
}

bool IrcConversationLog::append(const QString &networkId,
                                const QString &target,
                                const IrcTranscriptLine &line)
{
    if (line.kind.isEmpty() || line.body.isEmpty())
        return true;
    if (!IrcSecretPolicy::allowsTranscript(line.body))
        return true;
    if (!IrcSecretPolicy::allowsTranscript(line.author))
        return true;
    const QByteArray bytes = formatLine(line);
    if (appendBytes(pathFor(networkId, target), bytes))
        return true;
    qWarning("Could not append the conversation log for %s %s",
             qUtf8Printable(networkId), qUtf8Printable(target));
    return false;
}

std::vector<IrcTranscriptLine> IrcConversationLog::readTail(
    const QString &networkId, const QString &target, int maxLines) const
{
    std::vector<IrcTranscriptLine> lines;
    if (maxLines <= 0)
        return lines;
    QFile file(pathFor(networkId, target));
    if (!file.open(QIODevice::ReadOnly | QIODevice::Text))
        return lines;
    while (!file.atEnd()) {
        const QByteArray raw = file.readLine().trimmed();
        if (raw.isEmpty())
            continue;
        const std::optional<IrcTranscriptLine> parsed = parseLine(raw);
        if (!parsed)
            continue;
        if (!IrcSecretPolicy::allowsTranscript(parsed->body)
            || !IrcSecretPolicy::allowsTranscript(parsed->author)) {
            continue;
        }
        lines.push_back(*parsed);
        if (int(lines.size()) > maxLines)
            lines.erase(lines.begin());
    }
    return lines;
}
