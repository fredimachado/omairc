#include "irccapabilitynegotiation.h"

#include <optional>

namespace
{
bool acceptsAnyValue(const QString&)
{
    return true;
}

bool acceptsSaslValue(const QString& value)
{
    return value.isEmpty()
        || value.split(QLatin1Char(','), Qt::SkipEmptyParts)
               .contains(QStringLiteral("PLAIN"), Qt::CaseInsensitive);
}

QString tokenName(const QString& token)
{
    QString name = token.section(QLatin1Char('='), 0, 0);
    if (name.startsWith(QLatin1Char('-')))
        name.remove(0, 1);
    return name;
}

QString tokenValue(const QString& token)
{
    return token.contains(QLatin1Char('='))
        ? token.section(QLatin1Char('='), 1)
        : QString{};
}
}

struct IrcCapabilityNegotiation::Wanted
{
    IrcCapability capability;
    QLatin1String token;
    std::optional<IrcCapability> dependency;
    bool needsCredentials;
    bool (*acceptsValue)(const QString& value);
};

namespace
{
using Wanted = IrcCapabilityNegotiation::Wanted;

const Wanted wantedTable[] = {
    {IrcCapability::Sasl, QLatin1String("sasl"),
     std::nullopt, true, acceptsSaslValue},
    {IrcCapability::AwayNotify, QLatin1String("away-notify"),
     std::nullopt, false, acceptsAnyValue},
    {IrcCapability::Batch, QLatin1String("batch"),
     std::nullopt, false, acceptsAnyValue},
    {IrcCapability::MemberMetadata, QLatin1String("draft/metadata-2"),
     IrcCapability::Batch, false, acceptsAnyValue},
    {IrcCapability::MessageTags, QLatin1String("message-tags"),
     std::nullopt, false, acceptsAnyValue},
};

const Wanted *wantedFor(const QString& name)
{
    for (const Wanted& wanted : wantedTable) {
        if (name.compare(wanted.token, Qt::CaseInsensitive) == 0)
            return &wanted;
    }
    return nullptr;
}
}

IrcCapabilityNegotiation::IrcCapabilityNegotiation(bool saslCredentialsAvailable)
{
    reset(saslCredentialsAvailable);
}

void IrcCapabilityNegotiation::reset(bool saslCredentialsAvailable)
{
    m_advertised = {};
    m_enabled = {};
    m_outstanding = {};
    m_saslCredentialsAvailable = saslCredentialsAvailable;
}

void IrcCapabilityNegotiation::advertise(const QStringList& tokens)
{
    for (const QString& token : tokens) {
        const Wanted *wanted = wantedFor(tokenName(token));
        if (!wanted || !wanted->acceptsValue(tokenValue(token)))
            continue;
        m_advertised.insert(wanted->capability);
    }
}

void IrcCapabilityNegotiation::withdraw(const QStringList& tokens)
{
    for (const QString& token : tokens) {
        const Wanted *wanted = wantedFor(tokenName(token));
        if (!wanted)
            continue;
        m_advertised.remove(wanted->capability);
        m_enabled.remove(wanted->capability);
        m_outstanding.remove(wanted->capability);
    }
}

bool IrcCapabilityNegotiation::isRequestable(const Wanted& wanted) const
{
    if (!m_advertised.contains(wanted.capability))
        return false;
    if (m_enabled.contains(wanted.capability)
        || m_outstanding.contains(wanted.capability)) {
        return false;
    }
    if (wanted.needsCredentials && !m_saslCredentialsAvailable)
        return false;
    if (wanted.dependency) {
        return m_advertised.contains(*wanted.dependency)
            || m_enabled.contains(*wanted.dependency);
    }
    return true;
}

IrcCapabilityNegotiation::Request IrcCapabilityNegotiation::takeRequest()
{
    Request request;
    QStringList presence;

    for (const Wanted& wanted : wantedTable) {
        if (!isRequestable(wanted))
            continue;
        // SASL and message-tags each keep a line so a NAK of one cannot
        // refuse the other or the presence bundle.
        if (wanted.capability == IrcCapability::Sasl) {
            request.lines.append(wanted.token);
            request.requestsSasl = true;
        } else if (wanted.capability == IrcCapability::MessageTags) {
            request.lines.append(wanted.token);
        } else {
            presence.append(wanted.token);
        }
        m_outstanding.insert(wanted.capability);
    }

    if (!presence.isEmpty())
        request.lines.append(presence.join(QLatin1Char(' ')));
    return request;
}

IrcCapabilitySet IrcCapabilityNegotiation::acknowledge(const QStringList& tokens)
{
    IrcCapabilitySet newlyEnabled;
    for (const QString& token : tokens) {
        const Wanted *wanted = wantedFor(tokenName(token));
        if (!wanted)
            continue;
        m_outstanding.remove(wanted->capability);
        if (token.startsWith(QLatin1Char('-'))) {
            m_enabled.remove(wanted->capability);
            continue;
        }
        if (m_enabled.contains(wanted->capability))
            continue;
        m_enabled.insert(wanted->capability);
        newlyEnabled.insert(wanted->capability);
    }
    return newlyEnabled;
}

IrcCapabilitySet IrcCapabilityNegotiation::reject(const QStringList& tokens)
{
    IrcCapabilitySet refused;
    for (const QString& token : tokens) {
        const Wanted *wanted = wantedFor(tokenName(token));
        if (!wanted || !m_outstanding.contains(wanted->capability))
            continue;
        m_outstanding.remove(wanted->capability);
        refused.insert(wanted->capability);
    }
    return refused;
}

IrcCapabilitySet IrcCapabilityNegotiation::abandonOutstanding()
{
    const IrcCapabilitySet abandoned = m_outstanding;
    m_outstanding = {};
    return abandoned;
}

bool IrcCapabilityNegotiation::settled() const noexcept
{
    return m_outstanding.isEmpty();
}

IrcCapabilitySet IrcCapabilityNegotiation::enabled() const noexcept
{
    return m_enabled;
}
