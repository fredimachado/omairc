#pragma once

#include <QHostAddress>
#include <QString>
#include <QUrl>

QUrl ircResolvedAvatarUrl(const QString& raw, int pixelSize);
int ircAvatarFetchPixelSize(int layoutPixels);
bool ircAvatarUrlIsSafe(const QUrl& url);
bool ircHostAddressIsUnsafe(const QHostAddress& address);
