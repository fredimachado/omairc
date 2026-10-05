#include "ircchannelname.h"

#include <QStringView>

#include <string>

namespace
{
std::string utf8(const QString& value)
{
    const QByteArray bytes = value.toUtf8();
    return std::string(bytes.constData(), std::size_t(bytes.size()));
}

bool isAsciiMember(QChar character, std::string_view letters)
{
    if (character.unicode() > 0x7F)
        return false;
    return letters.find(static_cast<char>(character.unicode())) != std::string_view::npos;
}

bool stopsChannelName(QChar character)
{
    const char16_t code = character.unicode();
    if (code < 0x20 || code == 0x7F)
        return true;
    if (character.isSpace())
        return true;
    return character == QLatin1Char(',') || character == QLatin1Char(':');
}

bool isChannelBoundary(QChar character)
{
    if (character.isSpace())
        return true;
    switch (character.unicode()) {
    case '(':
    case '[':
    case '{':
    case '<':
    case '"':
    case '\'':
    case ',':
        return true;
    default:
        return false;
    }
}

QString trimChannelName(const QString& raw)
{
    int end = raw.size();
    while (end > 0) {
        const QChar character = raw.at(end - 1);
        if (QStringLiteral(".,;:!?\"'").contains(character)) {
            --end;
            continue;
        }
        QChar open;
        if (character == QLatin1Char(')'))
            open = QLatin1Char('(');
        else if (character == QLatin1Char(']'))
            open = QLatin1Char('[');
        else if (character == QLatin1Char('}'))
            open = QLatin1Char('{');
        else if (character == QLatin1Char('>'))
            open = QLatin1Char('<');
        else
            break;
        int opens = 0;
        int closes = 0;
        for (int index = 0; index < end; ++index) {
            const QChar current = raw.at(index);
            if (current == open)
                ++opens;
            else if (current == character)
                ++closes;
        }
        if (closes > opens) {
            --end;
            continue;
        }
        break;
    }
    return raw.left(end);
}
}

QVector<IrcChannelNameSpan> ircChannelNameSpans(const QString& text,
                                                const IrcServerFeatures& features)
{
    QVector<IrcChannelNameSpan> spans;
    const std::string_view types = features.channelTypes();
    if (text.isEmpty() || types.empty())
        return spans;

    for (int index = 0; index < text.size();) {
        const QChar character = text.at(index);
        const bool rankBefore = index > 0
            && isAsciiMember(text.at(index - 1), features.prefixSymbols());
        const bool boundary = index == 0 || rankBefore
            || isChannelBoundary(text.at(index - 1));
        if (!boundary || !isAsciiMember(character, types)) {
            ++index;
            continue;
        }
        int rawEnd = index + 1;
        while (rawEnd < text.size() && !stopsChannelName(text.at(rawEnd)))
            ++rawEnd;
        const QString name = trimChannelName(text.mid(index, rawEnd - index));
        if (name.size() >= 2 && features.isChannel(utf8(name))) {
            IrcChannelNameSpan span;
            span.start = index;
            span.end = index + name.size();
            span.name = name;
            spans.append(span);
        }
        index = rawEnd;
    }
    return spans;
}

QString ircChannelNameAt(const QString& text, int index,
                         const IrcServerFeatures& features)
{
    if (text.isEmpty() || index < 0 || index >= text.size())
        return {};
    const QVector<IrcChannelNameSpan> spans = ircChannelNameSpans(text, features);
    for (const IrcChannelNameSpan& span : spans) {
        if (index >= span.start && index < span.end)
            return span.name;
    }
    return {};
}
