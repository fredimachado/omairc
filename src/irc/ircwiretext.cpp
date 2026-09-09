#include "ircwiretext.h"

#include <QByteArray>

QString ircWireText(std::string_view bytes)
{
    const QByteArray view = QByteArray::fromRawData(bytes.data(), qsizetype(bytes.size()));
    if (view.isValidUtf8())
        return QString::fromUtf8(view);
    return QString::fromLatin1(view);
}
