#pragma once

#include "irccapability.h"

#include <QString>
#include <QStringList>

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

    /// CAP LS or CAP NEW tokens, still in their `name=value` wire form.
    /// Repeated calls accumulate, so a 302 continuation needs no special case.
    void advertise(const QStringList& tokens);
    void withdraw(const QStringList& tokens);

    /// Wanted, advertised, not yet enabled and not already asked for. Marks the
    /// result outstanding, so a second call without a new advertisement is empty.
    Request beginRequest();

    IrcCapabilitySet acknowledge(const QStringList& tokens);
    IrcCapabilitySet reject(const QStringList& tokens);
    IrcCapabilitySet abandonOutstanding();

    bool settled() const noexcept;
    IrcCapabilitySet enabled() const noexcept;

    /// Wire names, dependencies and preconditions for everything we may ask for.
    struct Wanted;

private:
    bool isRequestable(const Wanted& wanted) const;

    IrcCapabilitySet m_advertised;
    IrcCapabilitySet m_enabled;
    IrcCapabilitySet m_outstanding;
    bool m_saslCredentialsAvailable = false;
};
