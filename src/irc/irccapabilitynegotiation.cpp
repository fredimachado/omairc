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
    {IrcCapability::MultiPrefix, QLatin1String("multi-prefix"),
     std::nullopt, false, acceptsAnyValue},
    {IrcCapability::Chghost, QLatin1String("chghost"),
     std::nullopt, false, acceptsAnyValue},
    {IrcCapability::CapNotify, QLatin1String("cap-notify"),
     std::nullopt, false, acceptsAnyValue},
    {IrcCapability::EchoMessage, QLatin1String("echo-message"),
     std::nullopt, false, acceptsAnyValue},
    {IrcCapability::ChatHistory, QLatin1String("chathistory"),
     IrcCapability::Batch, false, acceptsAnyValue},
    {IrcCapability::ChatHistory, QLatin1String("draft/chathistory"),
     IrcCapability::Batch, false, acceptsAnyValue},
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
    m_advertisedTokens.clear();
    m_enabledTokens.clear();
    m_outstandingTokens.clear();
    m_saslCredentialsAvailable = saslCredentialsAvailable;
}

void IrcCapabilityNegotiation::advertise(const QStringList& tokens)
{
    for (const QString& token : tokens) {
        const Wanted *wanted = wantedFor(tokenName(token));
        if (!wanted || !wanted->acceptsValue(tokenValue(token)))
            continue;
        m_advertised.insert(wanted->capability);
        m_advertisedTokens.insert(tokenName(token).toCaseFolded());
    }
}

void IrcCapabilityNegotiation::withdraw(const QStringList& tokens)
{
    for (const QString& token : tokens) {
        const Wanted *wanted = wantedFor(tokenName(token));
        if (!wanted)
            continue;
        const QString name = tokenName(token).toCaseFolded();
        m_advertisedTokens.remove(name);
        m_enabledTokens.remove(name);
        m_outstandingTokens.remove(name);
        if (!hasToken(m_advertisedTokens, wanted->capability))
            m_advertised.remove(wanted->capability);
        if (!hasToken(m_enabledTokens, wanted->capability))
            m_enabled.remove(wanted->capability);
        if (!hasToken(m_outstandingTokens, wanted->capability))
            m_outstanding.remove(wanted->capability);
    }
}

bool IrcCapabilityNegotiation::hasToken(const QSet<QString>& tokens,
                                       IrcCapability capability) const
{
    for (const Wanted& row : wantedTable) {
        if (row.capability != capability)
            continue;
        if (tokens.contains(QString(row.token).toCaseFolded()))
            return true;
    }
    return false;
}

bool IrcCapabilityNegotiation::isRequestable(const Wanted& wanted) const
{
    if (!m_advertisedTokens.contains(QString(wanted.token).toCaseFolded()))
        return false;
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
        m_outstandingTokens.insert(QString(wanted.token).toCaseFolded());
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
        const QString name = tokenName(token).toCaseFolded();
        m_outstandingTokens.remove(name);
        if (!hasToken(m_outstandingTokens, wanted->capability))
            m_outstanding.remove(wanted->capability);
        if (token.startsWith(QLatin1Char('-'))) {
            m_enabledTokens.remove(name);
            if (!hasToken(m_enabledTokens, wanted->capability))
                m_enabled.remove(wanted->capability);
            continue;
        }
        m_enabledTokens.insert(name);
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
        if (!wanted)
            continue;
        const QString name = tokenName(token).toCaseFolded();
        if (!m_outstandingTokens.contains(name))
            continue;
        m_outstandingTokens.remove(name);
        m_advertisedTokens.remove(name);
        if (!hasToken(m_outstandingTokens, wanted->capability))
            m_outstanding.remove(wanted->capability);
        if (!hasToken(m_advertisedTokens, wanted->capability))
            m_advertised.remove(wanted->capability);
        refused.insert(wanted->capability);
    }
    return refused;
}

IrcCapabilitySet IrcCapabilityNegotiation::abandonOutstanding()
{
    const IrcCapabilitySet abandoned = m_outstanding;
    m_outstanding = {};
    m_outstandingTokens.clear();
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
