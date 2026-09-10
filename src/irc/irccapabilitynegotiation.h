#pragma once

#include "irccapability.h"

#include <QHash>
#include <QString>
#include <QStringList>

#include <initializer_list>
#include <optional>

class IrcCapabilityNegotiation
{
public:
    struct Request
    {
        QStringList lines;
        bool requestsSasl = false;
    };

    explicit IrcCapabilityNegotiation(bool saslCredentialsAvailable = false);

    void reset(bool saslCredentialsAvailable);

    void advertise(const QStringList& tokens);
    void withdraw(const QStringList& tokens);

    Request takeRequest();

    IrcCapabilitySet acknowledge(const QStringList& tokens);
    IrcCapabilitySet reject(const QStringList& tokens);
    IrcCapabilitySet abandonOutstanding();

    bool settled() const noexcept;
    IrcCapabilitySet enabled() const noexcept;

    struct Wanted;

private:
    // A token the server offered and what we have done with it since. One
    // capability can have several spellings, so the token is what the wire
    // names and the capability is derived from it.
    enum class TokenState {
        Advertised,
        Requested,
        Enabled,
        Refused,
    };

    bool isRequestable(const Wanted& wanted) const;
    bool anyToken(IrcCapability capability,
                  std::initializer_list<TokenState> states) const;
    std::optional<TokenState> stateOf(const QString& foldedToken) const;

    QHash<QString, TokenState> m_tokens;
    bool m_saslCredentialsAvailable = false;
};
