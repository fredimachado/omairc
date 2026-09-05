#pragma once

#include "ircserverfeatures.h"

#include <QString>
#include <QStringList>

#include <optional>
#include <utility>
#include <variant>

class IrcChannelModeRequest
{
public:
    struct Query {
        QString channel;
    };

    struct Change {
        QString channel;
        QString modes;
        QStringList parameters;
    };

    static std::optional<IrcChannelModeRequest> parse(
        const QString& argument,
        const IrcServerFeatures& features);

    const QString& channel() const;

    template<typename Visitor>
    decltype(auto) visit(Visitor&& visitor) const
    {
        return std::visit(std::forward<Visitor>(visitor), m_payload);
    }

private:
    using Payload = std::variant<Query, Change>;
    explicit IrcChannelModeRequest(Payload payload)
        : m_payload(std::move(payload))
    {
    }

    Payload m_payload;
};
