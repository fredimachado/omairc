#include "ircavatarurl.h"

#include <QtGlobal>

namespace
{
constexpr int kMaximumAvatarUrlLength = 2048;

QString strippedHost(const QUrl& url)
{
    QString host = url.host();
    while (host.endsWith(QLatin1Char('.')))
        host.chop(1);
    return host;
}

bool hostLooksLocal(const QString& host)
{
    return host.compare(QLatin1String("localhost"), Qt::CaseInsensitive) == 0
        || host.endsWith(QLatin1String(".local"), Qt::CaseInsensitive)
        || host.endsWith(QLatin1String(".localhost"), Qt::CaseInsensitive);
}
}

bool ircHostAddressIsUnsafe(const QHostAddress& address)
{
    if (address.isNull()
        || address.isLoopback()
        || address.isLinkLocal()
        || address.isMulticast()
        || address.isBroadcast()
        || address.isPrivateUse()
        || address.isUniqueLocalUnicast()
        || !address.isGlobal()) {
        return true;
    }

    bool ok = false;
    const quint32 ipv4 = address.toIPv4Address(&ok);
    if (!ok)
        return false;
    if ((ipv4 & 0xffc00000u) == 0x64400000u)
        return true;
    if ((ipv4 & 0xff000000u) == 0)
        return true;
    return false;
}

int ircAvatarFetchPixelSize(int layoutPixels)
{
    if (layoutPixels <= 0)
        return 0;
    const int rounded = layoutPixels;
    const int distance32 = qAbs(rounded - 32);
    const int distance64 = qAbs(rounded - 64);
    return distance32 <= distance64 ? 32 : 64;
}

QUrl ircResolvedAvatarUrl(const QString& raw, int pixelSize)
{
    const QString trimmed = raw.trimmed();
    if (trimmed.isEmpty())
        return {};
    QString substituted = trimmed;
    substituted.replace(QLatin1String("{size}"),
                        QString::number(qMax(16, pixelSize)));
    if (substituted.size() > kMaximumAvatarUrlLength)
        return {};
    const QUrl url(substituted, QUrl::StrictMode);
    if (!url.isValid() || url.isRelative())
        return {};
    return url;
}

bool ircAvatarUrlIsSafe(const QUrl& url)
{
    if (!url.isValid() || url.isRelative() || url.isEmpty())
        return false;
    if (url.scheme().compare(QLatin1String("https"), Qt::CaseInsensitive) != 0)
        return false;
    if (url.toString().size() > kMaximumAvatarUrlLength)
        return false;
    if (!url.userInfo().isEmpty()
        || !url.userName().isEmpty()
        || !url.password().isEmpty()) {
        return false;
    }
    const QString host = strippedHost(url);
    if (host.isEmpty())
        return false;
    if (hostLooksLocal(host))
        return false;
    const int port = url.port(-1);
    if (port != -1 && port != 443)
        return false;

    const QHostAddress address(host);
    if (!address.isNull())
        return !ircHostAddressIsUnsafe(address);
    return true;
}

QHostAddress ircSelectSafeAvatarAddress(const QList<QHostAddress>& addresses)
{
    for (const QHostAddress& address : addresses) {
        if (!ircHostAddressIsUnsafe(address))
            return address;
    }
    return {};
}

QUrl ircAvatarUrlPinnedToAddress(const QUrl& url, const QHostAddress& address)
{
    if (!url.isValid() || address.isNull())
        return {};
    QUrl pinned = url;
    pinned.setHost(address.toString());
    if (!pinned.isValid() || pinned.host().isEmpty())
        return {};
    return pinned;
}
