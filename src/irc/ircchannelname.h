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

// Channel names in text. The prefix comes from features, never a hard-coded
// CHANTYPES. An empty CHANTYPES matches nothing. A token starts with an
// advertised channel type, only at the start of text, after whitespace or an
// opening delimiter, or immediately after a PREFIX rank character. The span
// does not include that rank. The rank comes from features, never a
// hard-coded "@". The token runs until a space, comma, colon, or control
// character. Trailing .,;:!? and a trailing " or ' are stripped one mark at
// a time, not as a pair. Do not count " as a bracket. An unmatched trailing
// ), ], }, or > is stripped; > pairs with <. #foo<bar> stays #foo<bar>.
// "#desktop" becomes #desktop. <#desktop> becomes #desktop, and the > is
// outside the span. #foo's stays #foo's. The result is empty unless
// features.isChannel accepts it and it is at least two characters.
QVector<IrcChannelNameSpan> ircChannelNameSpans(const QString& text,
                                                const IrcServerFeatures& features);

// The channel name whose span contains index, or empty.
QString ircChannelNameAt(const QString& text, int index,
                         const IrcServerFeatures& features);
