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
    bool isRequestable(const Wanted& wanted) const;

    IrcCapabilitySet m_advertised;
    IrcCapabilitySet m_enabled;
    IrcCapabilitySet m_outstanding;
    bool m_saslCredentialsAvailable = false;
};
