#pragma once

#include <QHostAddress>
#include <QList>
#include <QString>
#include <QUrl>

QString ircAvatarMetadataValue(const QString& input);
QUrl ircResolvedAvatarUrl(const QString& raw, int pixelSize);
int ircAvatarFetchPixelSize(int layoutPixels);
bool ircAvatarUrlIsSafe(const QUrl& url);
bool ircHostAddressIsUnsafe(const QHostAddress& address);

// First global-unicast address, or a null address when none are safe.
QHostAddress ircSelectSafeAvatarAddress(const QList<QHostAddress>& addresses);
// Rewrite url's host to address so QNAM connects to that IP (no second DNS).
QUrl ircAvatarUrlPinnedToAddress(const QUrl& url, const QHostAddress& address);
