#include "ircavatarhttp.h"

QByteArray ircAvatarHttpHeaderValue(const QByteArray& headerBlock,
                                      const QByteArray& name)
{
    const QByteArray needle = name.toLower() + ": ";
    int offset = 0;
    if (headerBlock.startsWith("\r\n"))
        offset = 2;
    while (offset < headerBlock.size()) {
        const int lineEnd = headerBlock.indexOf("\r\n", offset);
        const int lineLength =
            lineEnd < 0 ? headerBlock.size() - offset : lineEnd - offset;
        const QByteArray line = headerBlock.mid(offset, lineLength);
        if (line.isEmpty())
            break;
        if (line.size() >= needle.size()
            && line.left(needle.size()).toLower() == needle) {
            return line.mid(needle.size()).trimmed();
        }
        if (lineEnd < 0)
            break;
        offset = lineEnd + 2;
    }
    return {};
}

bool ircAvatarHttpTransferEncodingIsChunked(const QByteArray& headerBlock)
{
    return ircAvatarHttpHeaderValue(headerBlock, "Transfer-Encoding")
        .toLower()
        .contains("chunked");
}

QByteArray ircAvatarHttpDecodeChunkedBody(const QByteArray& raw, bool *ok)
{
    QByteArray output;
    int offset = 0;
    while (offset < raw.size()) {
        const int lineEnd = raw.indexOf("\r\n", offset);
        if (lineEnd < 0) {
            if (ok)
                *ok = false;
            return {};
        }
        bool parsed = false;
        const qint64 chunkSize =
            raw.mid(offset, lineEnd - offset).toLongLong(&parsed, 16);
        if (!parsed || chunkSize < 0) {
            if (ok)
                *ok = false;
            return {};
        }
        offset = lineEnd + 2;
        if (chunkSize == 0) {
            if (ok)
                *ok = true;
            return output;
        }
        if (offset + chunkSize > raw.size()) {
            if (ok)
                *ok = false;
            return {};
        }
        output += raw.mid(offset, int(chunkSize));
        offset += int(chunkSize);
        if (offset + 2 > raw.size() || raw.mid(offset, 2) != "\r\n") {
            if (ok)
                *ok = false;
            return {};
        }
        offset += 2;
    }
    if (ok)
        *ok = false;
    return {};
}
