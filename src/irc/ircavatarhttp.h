#pragma once

#include <QByteArray>

QByteArray ircAvatarHttpHeaderValue(const QByteArray& headerBlock,
                                      const QByteArray& name);
bool ircAvatarHttpTransferEncodingIsChunked(const QByteArray& headerBlock);
QByteArray ircAvatarHttpDecodeChunkedBody(const QByteArray& raw, bool *ok);
