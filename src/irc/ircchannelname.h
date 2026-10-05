#pragma once

#include "ircserverfeatures.h"

#include <QString>
#include <QVector>

// One channel name inside a message or a topic. start and end are indexes into
// the QString the name was scanned from, and end is exclusive. The name is the
// trimmed token, so trailing sentence punctuation is outside the span.
struct IrcChannelNameSpan
{
    int start = 0;
    int end = 0;
    QString name;
};

// Channel names in text, using features for the prefix. An empty CHANTYPES
// matches nothing. A token starts with an advertised channel type, only at
// the start of text or after whitespace or an opening delimiter, and runs
// until a space, comma, colon, or control character. Trailing .,;:!? and an
// unmatched trailing bracket are stripped. The result is empty unless
// features.isChannel accepts it and it is at least two characters.
QVector<IrcChannelNameSpan> ircChannelNameSpans(const QString& text,
                                                const IrcServerFeatures& features);

// The channel name whose span contains index, or empty.
QString ircChannelNameAt(const QString& text, int index,
                         const IrcServerFeatures& features);
