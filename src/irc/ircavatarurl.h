#pragma once

#include <QHostAddress>
#include <QString>
#include <QUrl>

QUrl ircResolvedAvatarUrl(const QString& raw, int pixelSize);
bool ircAvatarUrlIsSafe(const QUrl& url);
bool ircHostAddressIsUnsafe(const QHostAddress& address);
