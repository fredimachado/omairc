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

QString foldedName(const QString& token)
{
    return tokenName(token).toCaseFolded();
}
}

struct IrcCapabilityNegotiation::Wanted
{
    IrcCapability capability;
    QLatin1String token;
    std::optional<IrcCapability> dependency;
    bool needsCredentials;
    // A token on its own CAP REQ line cannot be refused by a NAK aimed at
    // another token in the same request.
    bool ownRequestLine;
    bool (*acceptsValue)(const QString& value);
};

namespace
{
using Wanted = IrcCapabilityNegotiation::Wanted;

const Wanted wantedTable[] = {
    {IrcCapability::Sasl, QLatin1String("sasl"),
     std::nullopt, true, true, acceptsSaslValue},
    {IrcCapability::AwayNotify, QLatin1String("away-notify"),
     std::nullopt, false, false, acceptsAnyValue},
    {IrcCapability::Batch, QLatin1String("batch"),
     std::nullopt, false, false, acceptsAnyValue},
    {IrcCapability::MemberMetadata, QLatin1String("draft/metadata-2"),
     IrcCapability::Batch, false, false, acceptsAnyValue},
    {IrcCapability::MessageTags, QLatin1String("message-tags"),
     std::nullopt, false, true, acceptsAnyValue},
    {IrcCapability::MultiPrefix, QLatin1String("multi-prefix"),
     std::nullopt, false, false, acceptsAnyValue},
    {IrcCapability::Chghost, QLatin1String("chghost"),
     std::nullopt, false, false, acceptsAnyValue},
    {IrcCapability::CapNotify, QLatin1String("cap-notify"),
     std::nullopt, false, false, acceptsAnyValue},
    {IrcCapability::EchoMessage, QLatin1String("echo-message"),
     std::nullopt, false, false, acceptsAnyValue},
    {IrcCapability::ChatHistory, QLatin1String("chathistory"),
     IrcCapability::Batch, false, false, acceptsAnyValue},
    {IrcCapability::ChatHistory, QLatin1String("draft/chathistory"),
     IrcCapability::Batch, false, false, acceptsAnyValue},
};

const Wanted *wantedFor(const QString& name)
{
    for (const Wanted& wanted : wantedTable) {
        if (name.compare(wanted.token, Qt::CaseInsensitive) == 0)
            return &wanted;
    }
    return nullptr;
}

QString foldedToken(const Wanted& wanted)
{
    return QString(wanted.token).toCaseFolded();
}
}

IrcCapabilityNegotiation::IrcCapabilityNegotiation(bool saslCredentialsAvailable)
{
    reset(saslCredentialsAvailable);
}

void IrcCapabilityNegotiation::reset(bool saslCredentialsAvailable)
{
    m_tokens.clear();
    m_saslCredentialsAvailable = saslCredentialsAvailable;
}

std::optional<IrcCapabilityNegotiation::TokenState>
IrcCapabilityNegotiation::stateOf(const QString& foldedToken) const
{
    const auto found = m_tokens.constFind(foldedToken);
    if (found == m_tokens.cend())
        return std::nullopt;
    return found.value();
}

bool IrcCapabilityNegotiation::anyToken(
    IrcCapability capability, std::initializer_list<TokenState> states) const
{
    for (const Wanted& wanted : wantedTable) {
        if (wanted.capability != capability)
            continue;
        const std::optional<TokenState> state = stateOf(foldedToken(wanted));
        if (!state)
            continue;
        for (const TokenState wantedState : states) {
            if (*state == wantedState)
                return true;
        }
    }
    return false;
}

void IrcCapabilityNegotiation::advertise(const QStringList& tokens)
{
    for (const QString& token : tokens) {
        const Wanted *wanted = wantedFor(tokenName(token));
        if (!wanted || !wanted->acceptsValue(tokenValue(token)))
            continue;
        const QString name = foldedName(token);
        const std::optional<TokenState> state = stateOf(name);
        if (state == TokenState::Enabled || state == TokenState::Requested)
            continue;
        m_tokens.insert(name, TokenState::Advertised);
    }
}

void IrcCapabilityNegotiation::withdraw(const QStringList& tokens)
{
    for (const QString& token : tokens) {
        if (!wantedFor(tokenName(token)))
            continue;
        m_tokens.remove(foldedName(token));
    }
}

bool IrcCapabilityNegotiation::isRequestable(const Wanted& wanted) const
{
    if (stateOf(foldedToken(wanted)) != TokenState::Advertised)
        return false;
    if (anyToken(wanted.capability, {TokenState::Enabled, TokenState::Requested}))
        return false;
    if (wanted.needsCredentials && !m_saslCredentialsAvailable)
        return false;
    if (wanted.dependency) {
        return anyToken(*wanted.dependency,
                        {TokenState::Advertised, TokenState::Requested,
                         TokenState::Enabled});
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
        if (wanted.ownRequestLine)
            request.lines.append(wanted.token);
        else
            presence.append(wanted.token);
        if (wanted.capability == IrcCapability::Sasl)
            request.requestsSasl = true;
        m_tokens.insert(foldedToken(wanted), TokenState::Requested);
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
        const QString name = foldedName(token);
        const std::optional<TokenState> state = stateOf(name);
        if (token.startsWith(QLatin1Char('-'))) {
            if (state == TokenState::Enabled)
                m_tokens.insert(name, TokenState::Refused);
            continue;
        }
        // Only a token we are still waiting on can be granted. An ACK that
        // arrives after the capability timeout gave up must not re-enable it.
        if (state != TokenState::Requested)
            continue;
        const bool wasEnabled =
            anyToken(wanted->capability, {TokenState::Enabled});
        m_tokens.insert(name, TokenState::Enabled);
        if (!wasEnabled)
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
        const QString name = foldedName(token);
        if (stateOf(name) != TokenState::Requested)
            continue;
        m_tokens.insert(name, TokenState::Refused);
        refused.insert(wanted->capability);
    }
    return refused;
}

IrcCapabilitySet IrcCapabilityNegotiation::abandonOutstanding()
{
    IrcCapabilitySet abandoned;
    for (const Wanted& wanted : wantedTable) {
        const QString name = foldedToken(wanted);
        if (stateOf(name) != TokenState::Requested)
            continue;
        m_tokens.insert(name, TokenState::Refused);
        abandoned.insert(wanted.capability);
    }
    return abandoned;
}

bool IrcCapabilityNegotiation::settled() const noexcept
{
    for (auto it = m_tokens.cbegin(); it != m_tokens.cend(); ++it) {
        if (it.value() == TokenState::Requested)
            return false;
    }
    return true;
}

IrcCapabilitySet IrcCapabilityNegotiation::enabled() const noexcept
{
    IrcCapabilitySet set;
    for (const Wanted& wanted : wantedTable) {
        if (stateOf(foldedToken(wanted)) == TokenState::Enabled)
            set.insert(wanted.capability);
    }
    return set;
}
