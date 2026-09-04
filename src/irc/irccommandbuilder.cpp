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

#include <string>

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
}

IrcBuildResult IrcCommandBuilder::line(std::string_view command)
{
    if (command.empty())
        return IrcBuildResult::failure(IrcError::EmptyInput);
    if (containsForbidden(command))
        return IrcBuildResult::failure(IrcError::InvalidCharacter);
    if (command.size() + 2 > kMaxFrameBytes)
        return IrcBuildResult::failure(IrcError::TooManyBytes);

    std::string result(command);
    result += "\r\n";
    return IrcBuildResult::success(std::move(result));
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
