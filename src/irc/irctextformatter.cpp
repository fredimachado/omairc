#include "irctextformatter.h"

#ifdef QT_QML_LIB
#include <QtQml>
#endif

namespace {

constexpr ushort kBold = 0x02;
constexpr ushort kColor = 0x03;
constexpr ushort kHexColor = 0x04;
constexpr ushort kReset = 0x0f;
constexpr ushort kMonospace = 0x11;
constexpr ushort kReverse = 0x16;
constexpr ushort kItalic = 0x1d;
constexpr ushort kStrikethrough = 0x1e;
constexpr ushort kUnderline = 0x1f;

const auto kWrapperOpen = QLatin1String("<span style=\"white-space: pre-wrap;\">");
const auto kWrapperClose = QLatin1String("</span>");

bool asciiDigit(ushort code)
{
    return code >= '0' && code <= '9';
}

bool asciiHex(ushort code)
{
    return asciiDigit(code)
        || (code >= 'A' && code <= 'F')
        || (code >= 'a' && code <= 'f');
}

ushort unitAt(const QString &text, qsizetype index)
{
    return text.at(index).unicode();
}

// Last consumed index of \x03(?:\d{1,2}(?:,\d{1,2})?)?
qsizetype consumeMircColor(const QString &text, qsizetype index)
{
    const qsizetype n = text.size();
    qsizetype i = index + 1;
    if (i >= n || !asciiDigit(unitAt(text, i)))
        return index;
    ++i;
    if (i < n && asciiDigit(unitAt(text, i)))
        ++i;
    if (i < n && unitAt(text, i) == ','
        && i + 1 < n && asciiDigit(unitAt(text, i + 1))) {
        i += 2;
        if (i < n && asciiDigit(unitAt(text, i)))
            ++i;
    }
    return i - 1;
}

bool sixHexAt(const QString &text, qsizetype start)
{
    const qsizetype n = text.size();
    if (start + 5 >= n)
        return false;
    for (int k = 0; k < 6; ++k) {
        if (!asciiHex(unitAt(text, start + k)))
            return false;
    }
    return true;
}

// Last consumed index of \x04(?:[0-9A-Fa-f]{6}(?:,[0-9A-Fa-f]{6})?)?
qsizetype consumeHexColor(const QString &text, qsizetype index)
{
    const qsizetype n = text.size();
    qsizetype i = index + 1;
    if (!sixHexAt(text, i))
        return index;
    i += 6;
    if (i < n && unitAt(text, i) == ',' && sixHexAt(text, i + 1))
        i += 7;
    return i - 1;
}

void appendEscaped(QString &html, QChar ch)
{
    switch (ch.unicode()) {
    case '&':
        html += QLatin1String("&amp;");
        break;
    case '<':
        html += QLatin1String("&lt;");
        break;
    case '>':
        html += QLatin1String("&gt;");
        break;
    default:
        html += ch;
        break;
    }
}

bool htmlHasDisallowedTag(const QString &html)
{
    const qsizetype n = html.size();
    for (qsizetype i = 0; i < n; ++i) {
        if (unitAt(html, i) != '<')
            continue;
        qsizetype j = i + 1;
        if (j < n && unitAt(html, j) == '/')
            ++j;
        if (j >= n)
            return true;
        const ushort tag = unitAt(html, j);
        if (tag != 'b' && tag != 'i' && tag != 'u')
            return true;
        ++j;
        if (j >= n || unitAt(html, j) != '>')
            return true;
        i = j;
    }
    return false;
}

}

void omaircRegisterIrcTextFormatter()
{
#ifdef QT_QML_LIB
    static bool registered = false;
    if (registered)
        return;
    registered = true;
    qmlRegisterType<IrcTextFormatter>("Omairc.App", 1, 0, "IrcTextFormatter");
#endif
}

#ifdef QT_QML_LIB
static void registerIrcTextFormatterAtStartup()
{
    omaircRegisterIrcTextFormatter();
}

Q_COREAPP_STARTUP_FUNCTION(registerIrcTextFormatterAtStartup)
#endif

IrcTextFormatter::IrcTextFormatter(QObject *parent)
    : QObject(parent)
{
}

QString IrcTextFormatter::stripIrcColors(const QString &text) const
{
    QString out;
    out.reserve(text.size());
    const qsizetype n = text.size();
    for (qsizetype i = 0; i < n; ++i) {
        const ushort code = unitAt(text, i);
        if (code == kColor) {
            i = consumeMircColor(text, i);
            continue;
        }
        if (code == kHexColor) {
            i = consumeHexColor(text, i);
            continue;
        }
        out += text.at(i);
    }
    return out;
}

QString IrcTextFormatter::plainIrcText(const QString &text) const
{
    const QString stripped = stripIrcColors(text);
    QString out;
    out.reserve(stripped.size());
    for (QChar ch : stripped) {
        switch (ch.unicode()) {
        case kBold:
        case kReset:
        case kMonospace:
        case kReverse:
        case kItalic:
        case kStrikethrough:
        case kUnderline:
            break;
        default:
            out += ch;
            break;
        }
    }
    return out;
}

QString IrcTextFormatter::escapeHtml(const QString &text) const
{
    QString out;
    out.reserve(text.size());
    for (QChar ch : text)
        appendEscaped(out, ch);
    return out;
}

bool IrcTextFormatter::hasIrcEmphasis(const QString &text) const
{
    for (QChar ch : text) {
        const ushort code = ch.unicode();
        if (code == kBold || code == kItalic || code == kUnderline)
            return true;
    }
    return false;
}

QString IrcTextFormatter::emphasizedIrcText(const QString &text) const
{
    const QString input = stripIrcColors(text);
    bool bold = false;
    bool italic = false;
    bool underline = false;
    bool openBold = false;
    bool openItalic = false;
    bool openUnderline = false;
    QString html;
    html.reserve(input.size() + 32);
    const qsizetype n = input.size();
    for (qsizetype index = 0; index < n; ++index) {
        const ushort code = unitAt(input, index);
        bool changed = false;
        if (code == kBold) {
            bold = !bold;
            changed = true;
        } else if (code == kItalic) {
            italic = !italic;
            changed = true;
        } else if (code == kUnderline) {
            underline = !underline;
            changed = true;
        } else if (code == kReset) {
            bold = false;
            italic = false;
            underline = false;
            changed = true;
        } else if (code != kReverse && code != kMonospace && code != kStrikethrough) {
            appendEscaped(html, input.at(index));
        }
        if (!changed)
            continue;
        if (openUnderline) {
            html += QLatin1String("</u>");
            openUnderline = false;
        }
        if (openItalic) {
            html += QLatin1String("</i>");
            openItalic = false;
        }
        if (openBold) {
            html += QLatin1String("</b>");
            openBold = false;
        }
        if (bold) {
            html += QLatin1String("<b>");
            openBold = true;
        }
        if (italic) {
            html += QLatin1String("<i>");
            openItalic = true;
        }
        if (underline) {
            html += QLatin1String("<u>");
            openUnderline = true;
        }
    }
    if (openUnderline)
        html += QLatin1String("</u>");
    if (openItalic)
        html += QLatin1String("</i>");
    if (openBold)
        html += QLatin1String("</b>");
    // Defence-in-depth: unreachable while every non-control character goes through escapeHtml.
    if (htmlHasDisallowedTag(html))
        html = escapeHtml(plainIrcText(text));
    return kWrapperOpen + html + kWrapperClose;
}
