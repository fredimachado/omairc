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

#include "irccommandbuilder.h"

#include <QString>

#include <string>
#include <vector>

namespace
{
bool containsForbidden(std::string_view text)
{
    return text.find('\r') != std::string_view::npos
        || text.find('\n') != std::string_view::npos
        || text.find('\0') != std::string_view::npos;
}

bool isSingleField(std::string_view field)
{
    return !field.empty() && !containsForbidden(field)
        && field.find(' ') == std::string_view::npos;
}

IrcBuildResult invalidField()
{
    return IrcBuildResult::failure(IrcError::InvalidField);
}

bool isJoinField(std::string_view field)
{
    if (field.empty() || field.front() == ':')
        return false;
    const QString text = QString::fromUtf8(field.data(), qsizetype(field.size()));
    if (text.isEmpty())
        return false;
    for (const QChar ch : text) {
        if (ch.isSpace() || ch == QLatin1Char(',') || ch == QChar(u'\0'))
            return false;
    }
    return true;
}
}

IrcBuildResult IrcCommandBuilder::line(std::string_view command, std::size_t frameBytes)
{
    if (command.empty())
        return IrcBuildResult::failure(IrcError::EmptyInput);
    if (containsForbidden(command))
        return IrcBuildResult::failure(IrcError::InvalidCharacter);
    if (frameBytes < 2 || command.size() + 2 > frameBytes)
        return IrcBuildResult::failure(IrcError::TooManyBytes);

    std::string result(command);
    result += "\r\n";
    return IrcBuildResult::success(std::move(result));
}

int IrcCommandBuilder::composerByteBudget(std::string_view target, std::size_t frameBytes)
{
    if (frameBytes < 2)
        return 0;
    if (target.empty())
        return int(frameBytes) - 2;

    // Same residual as splitTrailingParam for prefix "PRIVMSG <target> :"
    // and an empty suffix: one chunk, including CRLF, stays within the frame.
    const std::string prefix = std::string("PRIVMSG ") + std::string(target) + " :";
    if (prefix.size() + 3 > frameBytes)
        return 0;
    return int(frameBytes - prefix.size() - 2);
}

int IrcCommandBuilder::actionComposerByteBudget(std::string_view target,
                                                std::size_t frameBytes)
{
    // `\x01ACTION ` is 8 bytes and the closing `\x01` is 1. `/me ` is 4.
    constexpr int kActionWrapper = 9;
    constexpr int kMePrefix = 4;
    const int privmsg = composerByteBudget(target, frameBytes);
    if (privmsg < kActionWrapper)
        return 0;
    return privmsg - kActionWrapper + kMePrefix;
}

namespace
{
bool draftIsAction(std::string_view draft)
{
    std::size_t begin = 0;
    while (begin < draft.size()
           && (draft[begin] == ' ' || draft[begin] == '\t')) {
        ++begin;
    }
    if (begin >= draft.size() || draft[begin] != '/')
        return false;
    const std::size_t space = draft.find(' ', begin);
    if (space == std::string_view::npos)
        return false;
    const std::string_view token = draft.substr(begin, space - begin);
    if (token.size() != 3)
        return false;
    const char m = token[1];
    const char e = token[2];
    return (m == 'm' || m == 'M') && (e == 'e' || e == 'E');
}
}

int IrcCommandBuilder::composerByteBudgetForDraft(std::string_view target,
                                                  std::string_view draft,
                                                  std::size_t frameBytes)
{
    if (target.empty() || !draftIsAction(draft))
        return composerByteBudget(target, frameBytes);
    return actionComposerByteBudget(target, frameBytes);
}

std::string IrcCommandBuilder::clampUtf8Prefix(std::string_view text, int maxBytes)
{
    if (maxBytes <= 0 || text.empty())
        return {};
    std::size_t end = 0;
    while (end < text.size()) {
        const auto lead = static_cast<unsigned char>(text[end]);
        std::size_t size = 1;
        if (lead >= 0x80) {
            if ((lead & 0xE0) == 0xC0)
                size = 2;
            else if ((lead & 0xF0) == 0xE0)
                size = 3;
            else if ((lead & 0xF8) == 0xF0)
                size = 4;
            if (end + size > text.size())
                size = 1;
            else {
                for (std::size_t index = 1; index < size; ++index) {
                    if ((static_cast<unsigned char>(text[end + index]) & 0xC0) != 0x80) {
                        size = 1;
                        break;
                    }
                }
            }
        }
        if (end + size > static_cast<std::size_t>(maxBytes))
            break;
        end += size;
    }
    return std::string(text.substr(0, end));
}

std::vector<std::string> IrcCommandBuilder::splitTrailingParam(std::string_view prefix,
                                                              std::string_view body,
                                                              std::string_view suffix,
                                                              std::size_t frameBytes)
{
    if (body.empty() || containsForbidden(prefix) || containsForbidden(body)
        || containsForbidden(suffix)) {
        return {};
    }
    if (frameBytes < 3 || prefix.size() + suffix.size() + 3 > frameBytes)
        return {};
    const std::size_t maxBody = frameBytes - prefix.size() - suffix.size() - 2;

    std::vector<std::string> chunks;
    std::size_t offset = 0;
    while (offset < body.size()) {
        const std::size_t remaining = body.size() - offset;
        if (remaining <= maxBody) {
            chunks.emplace_back(body.substr(offset));
            break;
        }

        const std::string_view window = body.substr(offset, maxBody);
        const std::size_t lastSpace = window.rfind(' ');
        if (lastSpace != std::string_view::npos && lastSpace > 0) {
            chunks.emplace_back(window.substr(0, lastSpace));
            offset += lastSpace + 1;
            continue;
        }

        std::size_t take = maxBody;
        while (take > 0
               && (static_cast<unsigned char>(body[offset + take]) & 0xC0) == 0x80) {
            --take;
        }
        if (take == 0)
            return {};
        chunks.emplace_back(body.substr(offset, take));
        offset += take;
    }
    return chunks;
}

IrcBuildResult IrcCommandBuilder::nick(std::string_view nickname)
{
    if (!isSingleField(nickname))
        return invalidField();
    return line("NICK " + std::string(nickname));
}

IrcBuildResult IrcCommandBuilder::user(std::string_view username, std::string_view realname)
{
    if (!isSingleField(username) || realname.empty() || containsForbidden(realname))
        return invalidField();
    return line("USER " + std::string(username) + " 8 * :" + std::string(realname));
}

IrcBuildResult IrcCommandBuilder::pass(std::string_view password)
{
    if (!isSingleField(password))
        return invalidField();
    return line("PASS " + std::string(password));
}

IrcBuildResult IrcCommandBuilder::join(std::string_view channel, std::string_view key)
{
    if (!isJoinField(channel))
        return invalidField();
    if (key.empty())
        return line("JOIN " + std::string(channel));
    if (!isJoinField(key))
        return invalidField();
    return line("JOIN " + std::string(channel) + " " + std::string(key));
}

IrcBuildResult IrcCommandBuilder::registration(std::string_view nickname,
                                               std::string_view username,
                                               std::string_view realname,
                                               std::string_view password)
{
    const IrcBuildResult nickCommand = nick(nickname);
    if (!nickCommand)
        return IrcBuildResult::failure(nickCommand.error);

    const IrcBuildResult userCommand = user(username, realname);
    if (!userCommand)
        return IrcBuildResult::failure(userCommand.error);

    std::string result;
    if (!password.empty()) {
        const IrcBuildResult passCommand = pass(password);
        if (!passCommand)
            return IrcBuildResult::failure(passCommand.error);
        result += *passCommand.value;
    }
    result += *nickCommand.value;
    result += *userCommand.value;
    return IrcBuildResult::success(std::move(result));
}
