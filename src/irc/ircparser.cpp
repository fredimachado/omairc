/*
 * Copyright (C) 2011 Fredi Machado <https://github.com/fredimachado>
 *
 * IRCClient is free software; you can redistribute it and/or modify it under
 * the terms of the GNU Lesser General Public License as published by the Free
 * Software Foundation; either version 3.0 of the License, or any later version.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT
 * ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or
 * FITNESS FOR A PARTICULAR PURPOSE. See the GNU Lesser General Public License
 * for more details.
 *
 * https://www.gnu.org/licenses/lgpl.html
 *
 * Omairc adapted the protocol behavior from IRCClient.
 */

#include "ircparser.h"

#include <algorithm>
#include <cctype>

namespace
{
bool containsForbidden(std::string_view text)
{
    return text.find('\r') != std::string_view::npos
        || text.find('\n') != std::string_view::npos
        || text.find('\0') != std::string_view::npos;
}

bool isValidCommand(std::string_view command)
{
    if (command.size() == 3
        && std::all_of(command.begin(), command.end(), [](unsigned char ch) {
               return std::isdigit(ch) != 0;
           })) {
        return true;
    }

    return !command.empty()
        && std::all_of(command.begin(), command.end(), [](unsigned char ch) {
               return ch < 0x80 && std::isalpha(ch) != 0;
           });
}

std::string uppercaseAscii(std::string_view text)
{
    std::string result(text);
    std::transform(result.begin(), result.end(), result.begin(), [](unsigned char ch) {
        return static_cast<char>(std::toupper(ch));
    });
    return result;
}

std::string unescapeTagValue(std::string_view value)
{
    std::string result;
    result.reserve(value.size());
    for (std::size_t index = 0; index < value.size(); ++index) {
        if (value[index] != '\\') {
            result.push_back(value[index]);
            continue;
        }
        if (++index == value.size())
            break;

        switch (value[index]) {
        case ':':
            result.push_back(';');
            break;
        case 's':
            result.push_back(' ');
            break;
        case '\\':
            result.push_back('\\');
            break;
        case 'r':
            result.push_back('\r');
            break;
        case 'n':
            result.push_back('\n');
            break;
        default:
            result.push_back(value[index]);
            break;
        }
    }
    return result;
}

bool isValidTagName(std::string_view name)
{
    if (!name.empty() && name.front() == '+')
        name.remove_prefix(1);
    if (name.empty())
        return false;

    const std::size_t slash = name.find('/');
    const std::string_view vendor = slash == std::string_view::npos
        ? std::string_view{}
        : name.substr(0, slash);
    const std::string_view key = slash == std::string_view::npos
        ? name
        : name.substr(slash + 1);

    const auto validKeyCharacter = [](unsigned char ch) {
        return ch < 0x80 && (std::isalnum(ch) != 0 || ch == '-');
    };
    const auto validVendorCharacter = [](unsigned char ch) {
        return ch < 0x80 && (std::isalnum(ch) != 0 || ch == '-' || ch == '.');
    };

    return !key.empty()
        && name.find('/', slash == std::string_view::npos ? 0 : slash + 1)
            == std::string_view::npos
        && std::all_of(key.begin(), key.end(), validKeyCharacter)
        && (slash == std::string_view::npos
            || (!vendor.empty()
                && std::all_of(vendor.begin(), vendor.end(), validVendorCharacter)));
}

bool parseTags(std::string_view section, std::vector<IrcTag>& tags)
{
    std::size_t start = 0;
    while (start <= section.size()) {
        const std::size_t end = section.find(';', start);
        const std::string_view tag = section.substr(
            start, end == std::string_view::npos ? section.size() - start : end - start);
        const std::size_t equals = tag.find('=');
        const std::string_view name = tag.substr(0, equals);
        if (!isValidTagName(name))
            return false;

        IrcTag parsed{std::string(name), std::nullopt};
        if (equals != std::string_view::npos)
            parsed.value = unescapeTagValue(tag.substr(equals + 1));
        tags.push_back(std::move(parsed));

        if (end == std::string_view::npos)
            return true;
        start = end + 1;
    }
    return false;
}

std::optional<IrcPrefix> parsePrefix(std::string_view raw)
{
    if (raw.empty() || containsForbidden(raw))
        return std::nullopt;

    IrcPrefix prefix;
    prefix.raw = std::string(raw);

    const std::size_t at = raw.find('@');
    std::string_view nickPart = raw;
    if (at != std::string_view::npos) {
        if (at == 0 || at + 1 == raw.size() || raw.find('@', at + 1) != std::string_view::npos)
            return std::nullopt;
        nickPart = raw.substr(0, at);
        prefix.host = std::string(raw.substr(at + 1));
    }

    const std::size_t bang = nickPart.find('!');
    if (bang != std::string_view::npos) {
        if (bang == 0 || bang + 1 == nickPart.size()
            || nickPart.find('!', bang + 1) != std::string_view::npos) {
            return std::nullopt;
        }
        prefix.nick = std::string(nickPart.substr(0, bang));
        prefix.user = std::string(nickPart.substr(bang + 1));
    } else if (at != std::string_view::npos) {
        prefix.nick = std::string(nickPart);
    }

    return prefix;
}

std::vector<std::string> parseParameters(std::string_view input)
{
    std::vector<std::string> parameters;
    std::size_t position = 0;

    while (position < input.size()) {
        while (position < input.size() && input[position] == ' ')
            ++position;
        if (position == input.size())
            break;

        if (parameters.size() == 14) {
            if (input[position] == ':')
                ++position;
            parameters.emplace_back(input.substr(position));
            break;
        }

        if (input[position] == ':') {
            parameters.emplace_back(input.substr(position + 1));
            break;
        }

        const std::size_t space = input.find(' ', position);
        if (space == std::string_view::npos) {
            parameters.emplace_back(input.substr(position));
            break;
        }
        parameters.emplace_back(input.substr(position, space - position));
        position = space + 1;
    }

    return parameters;
}
}

IrcParseResult IrcParser::parse(std::string_view line)
{
    if (line.empty())
        return IrcParseResult::failure(IrcError::EmptyInput);
    if (containsForbidden(line))
        return IrcParseResult::failure(IrcError::InvalidCharacter);

    IrcMessage message;
    std::size_t position = 0;

    if (line.front() == '@') {
        const std::size_t space = line.find(' ');
        if (space == std::string_view::npos || space == 1
            || space - 1 > IrcProtocol::maxTagSectionBytes) {
            return IrcParseResult::failure(IrcError::InvalidTags);
        }
        if (!parseTags(line.substr(1, space - 1), message.tags))
            return IrcParseResult::failure(IrcError::InvalidTags);
        position = space + 1;
        while (position < line.size() && line[position] == ' ')
            ++position;
    }

    if (position < line.size() && line[position] == ':') {
        const std::size_t space = line.find(' ', position);
        if (space == std::string_view::npos || space == position + 1)
            return IrcParseResult::failure(IrcError::InvalidPrefix);
        message.prefix = parsePrefix(line.substr(position + 1, space - position - 1));
        if (!message.prefix)
            return IrcParseResult::failure(IrcError::InvalidPrefix);
        position = space + 1;
        while (position < line.size() && line[position] == ' ')
            ++position;
    }

    if (position == line.size())
        return IrcParseResult::failure(IrcError::InvalidCommand);

    const std::size_t commandEnd = line.find(' ', position);
    const std::string_view command = line.substr(
        position, commandEnd == std::string_view::npos ? line.size() - position
                                                       : commandEnd - position);
    if (!isValidCommand(command))
        return IrcParseResult::failure(IrcError::InvalidCommand);
    message.command = uppercaseAscii(command);

    if (commandEnd != std::string_view::npos)
        message.parameters = parseParameters(line.substr(commandEnd + 1));

    return IrcParseResult::success(std::move(message));
}
