#include "ircstoragepath.h"

#include "irccasemapping.h"
#include "ircwiretext.h"

#include <QDir>
#include <QFile>
#include <QCryptographicHash>

#include <string_view>

namespace
{

bool isSafeStorageByte(unsigned char byte, bool isLastByte)
{
    if (byte >= 'a' && byte <= 'z')
        return true;
    if (byte >= '0' && byte <= '9')
        return true;
    switch (byte) {
    case '#':
    case '&':
    case '+':
    case '-':
    case '_':
    case '{':
    case '}':
    case '~':
    case '[':
    case ']':
    case '^':
        return true;
    case '.':
        return !isLastByte;
    default:
        return false;
    }
}

QString percentEncodeByte(unsigned char byte)
{
    return QStringLiteral("%")
        + QString::number(byte, 16).rightJustified(2, QLatin1Char('0'));
}

QString encodeStorageBytes(QByteArrayView utf8)
{
    QString encoded;
    encoded.reserve(int(utf8.size()) * 3);
    for (qsizetype index = 0; index < utf8.size(); ++index) {
        const unsigned char byte = static_cast<unsigned char>(utf8.at(index));
        if (isSafeStorageByte(byte, index == utf8.size() - 1))
            encoded.append(QLatin1Char(static_cast<char>(byte)));
        else
            encoded.append(percentEncodeByte(byte));
    }
    return encoded;
}

bool deviceBaseMatches(QStringView stem)
{
    while (!stem.isEmpty()
           && (stem.back() == QLatin1Char(' ') || stem.back() == QLatin1Char('.'))) {
        stem.chop(1);
    }
    if (stem.isEmpty())
        return false;

    const QString lower = stem.toString().toLower();
    if (lower == QLatin1String("con") || lower == QLatin1String("prn")
        || lower == QLatin1String("aux") || lower == QLatin1String("nul")) {
        return true;
    }

    if (lower.size() != 4)
        return false;

    const bool com = lower.startsWith(QLatin1String("com"));
    const bool lpt = lower.startsWith(QLatin1String("lpt"));
    if (!com && !lpt)
        return false;

    const QChar suffix = lower.at(3);
    if (suffix >= QLatin1Char('0') && suffix <= QLatin1Char('9'))
        return true;
    return suffix == QLatin1Char(u'\u00B9') || suffix == QLatin1Char(u'\u00B2')
        || suffix == QLatin1Char(u'\u00B3');
}

QStringView deviceBaseToken(QStringView fullName)
{
    while (!fullName.isEmpty()
           && (fullName.back() == QLatin1Char(' ')
               || fullName.back() == QLatin1Char('.'))) {
        fullName.chop(1);
    }
    const qsizetype dot = fullName.indexOf(QLatin1Char('.'));
    if (dot >= 0)
        return fullName.first(dot);
    return fullName;
}

bool isWin32DeviceName(const QString &segment, const QString &extension)
{
    return deviceBaseMatches(deviceBaseToken(QStringView(segment + extension)));
}

QString encodeWithLeadingEscape(QByteArrayView utf8)
{
    if (utf8.isEmpty())
        return {};
    return percentEncodeByte(static_cast<unsigned char>(utf8.at(0)))
        + encodeStorageBytes(utf8.sliced(1));
}

QString hashLongSegment(const QByteArray &utf8)
{
    return QStringLiteral("h")
        + QString::fromLatin1(
            QCryptographicHash::hash(utf8, QCryptographicHash::Sha256).toHex());
}

} // namespace

QString legacyStorageSegment(QString name)
{
    name.replace(QLatin1Char('%'), QLatin1String("%25"));
    name.replace(QLatin1Char('/'), QLatin1String("%2f"));
    name.replace(QLatin1Char('\\'), QLatin1String("%5c"));
    name.replace(QChar(0), QLatin1String("%00"));
    if (name.isEmpty() || name == QLatin1String(".") || name == QLatin1String(".."))
        return QStringLiteral("_");
    return name;
}

QString decodeLegacySegment(QString text)
{
    text.replace(QLatin1String("%00"), QString(QChar(0)));
    text.replace(QLatin1String("%5c"), QLatin1String("\\"));
    text.replace(QLatin1String("%2f"), QLatin1String("/"));
    text.replace(QLatin1String("%25"), QLatin1String("%"));
    return text;
}

QString omaircStorageSegment(QString text, QString extensionForDeviceCheck)
{
    if (text.isEmpty() || text == QLatin1String(".") || text == QLatin1String(".."))
        return QStringLiteral("_");

    const QByteArray utf8 = text.toUtf8();
    QString segment;
    if (isWin32DeviceName(text, extensionForDeviceCheck))
        segment = encodeWithLeadingEscape(utf8);
    else
        segment = encodeStorageBytes(utf8);
    if (segment.size() + extensionForDeviceCheck.size() > 255)
        return hashLongSegment(utf8);
    return segment;
}

QString omaircTargetSegment(QString target,
                          const IrcCaseMapping &mapping,
                          QString extensionForDeviceCheck)
{
    const QByteArray utf8 = target.toUtf8();
    const std::string normalized = mapping.normalize(
        std::string_view(utf8.constData(), utf8.size()));
    return omaircStorageSegment(ircWireText(normalized), extensionForDeviceCheck);
}

bool legacyStoragePathExists(const QString &path,
                             const QString &segment,
                             QString extensionForDeviceCheck)
{
#ifdef Q_OS_WIN
    if (isWin32DeviceName(segment, extensionForDeviceCheck))
        return false;
#else
    Q_UNUSED(segment);
    Q_UNUSED(extensionForDeviceCheck);
#endif
    return QFile::exists(path);
}

bool legacyStorageDirExists(const QDir &dir,
                            const QString &segment,
                            QString extensionForDeviceCheck)
{
#ifdef Q_OS_WIN
    if (isWin32DeviceName(segment, extensionForDeviceCheck))
        return false;
#else
    Q_UNUSED(extensionForDeviceCheck);
#endif
    return dir.exists(segment);
}
