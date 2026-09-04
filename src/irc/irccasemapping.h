#pragma once

#include <optional>
#include <string>
#include <string_view>

class IrcCaseMapping
{
public:
    enum class Kind {
        Rfc1459,
        Ascii,
        Rfc1459Strict,
    };

    explicit IrcCaseMapping(Kind kind = Kind::Rfc1459);

    static std::optional<IrcCaseMapping> fromName(std::string_view name);

    Kind kind() const noexcept;
    std::string normalize(std::string_view identifier) const;
    bool equals(std::string_view left, std::string_view right) const;

private:
    Kind m_kind;
};
