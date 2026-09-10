/*
 * Copyright (C) 2011 Fredi Machado <https://github.com/fredimachado>
 *
 * Omairc adapted the protocol behavior from IRCClient under the GNU Lesser
 * General Public License, version 3 or later.
 */

#ifndef IRCMESSAGE_H
#define IRCMESSAGE_H

#include <cstddef>
#include <optional>
#include <string>
#include <utility>
#include <vector>

namespace IrcProtocol
{
inline constexpr std::size_t maxClassicFrameBytes = 512;
// Servers prepend prefixes and emit long 005 lines. Keep a receive ceiling.
inline constexpr std::size_t maxInboundClassicFrameBytes = 4096;
// IRCv3 message-tags: 8191 bytes of tag payload, excluding '@' and the trailing space.
inline constexpr std::size_t maxTagSectionBytes = 8191;
}

enum class IrcError
{
    None,
    EmptyInput,
    InvalidCharacter,
    InvalidTags,
    InvalidPrefix,
    InvalidCommand,
    TooManyBytes,
    InvalidField
};

inline const char *ircErrorName(IrcError error) noexcept
{
    switch (error) {
    case IrcError::None:
        return "none";
    case IrcError::EmptyInput:
        return "empty input";
    case IrcError::InvalidCharacter:
        return "invalid character";
    case IrcError::InvalidTags:
        return "invalid tags";
    case IrcError::InvalidPrefix:
        return "invalid prefix";
    case IrcError::InvalidCommand:
        return "invalid command";
    case IrcError::TooManyBytes:
        return "too many bytes";
    case IrcError::InvalidField:
        return "invalid field";
    }
    return "unknown";
}

template<typename T>
struct IrcResult
{
    std::optional<T> value;
    IrcError error = IrcError::None;

    explicit operator bool() const noexcept
    {
        return value.has_value();
    }

    static IrcResult success(T result)
    {
        return {std::move(result), IrcError::None};
    }

    static IrcResult failure(IrcError reason)
    {
        return {std::nullopt, reason};
    }
};

struct IrcTag
{
    std::string name;
    std::optional<std::string> value;
};

struct IrcPrefix
{
    std::string raw;
    std::string nick;
    std::string user;
    std::string host;
};

struct IrcMessage
{
    std::vector<IrcTag> tags;
    std::optional<IrcPrefix> prefix;
    std::string command;
    std::vector<std::string> parameters;
};

using IrcParseResult = IrcResult<IrcMessage>;
using IrcBuildResult = IrcResult<std::string>;

#endif
