#include "ircfilelink.h"

#include <QUrl>

namespace
{
QString fromView(std::string_view text)
{
    return QString::fromUtf8(text.data(), static_cast<qsizetype>(text.size()));
}

QString strippedHost(QString host)
{
    while (host.endsWith(QLatin1Char('.')))
        host.chop(1);
    return host;
}

bool hostsMatch(const QString& left, const QString& right)
{
    const QString a = strippedHost(left);
    const QString b = strippedHost(right);
    return !a.isEmpty() && a.compare(b, Qt::CaseInsensitive) == 0;
}

bool hasControlCharacter(std::string_view text)
{
    for (unsigned char character : text) {
        if (character < 0x20 || character == 0x7F)
            return true;
    }
    return false;
}

bool webScheme(const QString& scheme)
{
    return scheme.compare(QLatin1String("https"), Qt::CaseInsensitive) == 0
        || scheme.compare(QLatin1String("http"), Qt::CaseInsensitive) == 0;
}
}

std::optional<std::string> absoluteFileLink(std::string_view endpoint,
                                            std::string_view location)
{
    if (endpoint.empty() || location.empty())
        return std::nullopt;
    if (hasControlCharacter(endpoint) || hasControlCharacter(location))
        return std::nullopt;

    const QUrl base = QUrl(fromView(endpoint));
    if (!base.isValid() || !webScheme(base.scheme()) || base.host().isEmpty())
        return std::nullopt;
    const QUrl resolved = base.resolved(QUrl(fromView(location)));
    if (!resolved.isValid() || !webScheme(resolved.scheme()))
        return std::nullopt;
    if (resolved.host().isEmpty() || !resolved.userInfo().isEmpty())
        return std::nullopt;

    QUrl clean = resolved;
    clean.setUserInfo({});
    clean.setFragment({});
    const QString text = clean.toString(QUrl::FullyEncoded);
    if (text.isEmpty() || text.contains(QLatin1Char(' '))
            || text.contains(QLatin1Char('\n')) || text.contains(QLatin1Char('\r')))
        return std::nullopt;
    return text.toStdString();
}

bool fileHostSendsBasicAuth(std::string_view serverHost,
                            std::string_view uploadUrl,
                            bool serverEncrypted)
{
    if (serverHost.empty() || uploadUrl.empty())
        return false;
    const QUrl url(fromView(uploadUrl));
    if (!url.isValid() || url.host().isEmpty() || !url.userInfo().isEmpty())
        return false;
    const bool https = url.scheme().compare(QLatin1String("https"), Qt::CaseInsensitive) == 0;
    const bool http = url.scheme().compare(QLatin1String("http"), Qt::CaseInsensitive) == 0;
    if (!https && !http)
        return false;
    if (serverEncrypted && !https)
        return false;
    return hostsMatch(fromView(serverHost), url.host());
}

std::string basicAuthorizationValue(std::string_view user, std::string_view secret)
{
    if (user.empty() || secret.empty())
        return {};
    if (hasControlCharacter(user) || hasControlCharacter(secret))
        return {};
    const QByteArray token = QByteArray(user.data(), static_cast<qsizetype>(user.size()))
        + ':'
        + QByteArray(secret.data(), static_cast<qsizetype>(secret.size()));
    return std::string("Basic ") + token.toBase64().toStdString();
}
