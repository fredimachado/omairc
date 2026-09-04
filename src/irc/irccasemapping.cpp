#include "irccasemapping.h"

#include <cctype>

namespace
{
char normalizedCharacter(char character, IrcCaseMapping::Kind kind)
{
    const auto byte = static_cast<unsigned char>(character);
    if (byte >= 'A' && byte <= 'Z')
        return static_cast<char>(byte + ('a' - 'A'));

    if (kind == IrcCaseMapping::Kind::Ascii)
        return character;

    switch (character) {
    case '[':
        return '{';
    case ']':
        return '}';
    case '\\':
        return '|';
    case '^':
        return kind == IrcCaseMapping::Kind::Rfc1459 ? '~' : '^';
    default:
        return character;
    }
}

std::string asciiLower(std::string_view value)
{
    std::string lowered;
    lowered.reserve(value.size());
    for (const char character : value)
        lowered.push_back(static_cast<char>(
            std::tolower(static_cast<unsigned char>(character))));
    return lowered;
}
}

IrcCaseMapping::IrcCaseMapping(Kind kind)
    : m_kind(kind)
{
}

std::optional<IrcCaseMapping> IrcCaseMapping::fromName(std::string_view name)
{
    const std::string normalized = asciiLower(name);
    if (normalized == "rfc1459")
        return IrcCaseMapping(Kind::Rfc1459);
    if (normalized == "ascii")
        return IrcCaseMapping(Kind::Ascii);
    if (normalized == "rfc1459-strict" || normalized == "strict-rfc1459")
        return IrcCaseMapping(Kind::Rfc1459Strict);
    return std::nullopt;
}

IrcCaseMapping::Kind IrcCaseMapping::kind() const noexcept
{
    return m_kind;
}

std::string IrcCaseMapping::normalize(std::string_view identifier) const
{
    std::string normalized;
    normalized.reserve(identifier.size());
    for (const char character : identifier)
        normalized.push_back(normalizedCharacter(character, m_kind));
    return normalized;
}

bool IrcCaseMapping::equals(std::string_view left, std::string_view right) const
{
    return normalize(left) == normalize(right);
}
