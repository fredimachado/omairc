/*
 * Copyright (C) 2011 Fredi Machado <https://github.com/fredimachado>
 *
 * Omairc adapted the protocol behavior from IRCClient under the GNU Lesser
 * General Public License, version 3 or later.
 */

#ifndef IRCCOMMANDBUILDER_H
#define IRCCOMMANDBUILDER_H

#include "ircmessage.h"

#include <cstddef>
#include <string>
#include <string_view>
#include <vector>

class IrcCommandBuilder
{
public:
    static constexpr std::size_t kMaxFrameBytes = IrcProtocol::maxClassicFrameBytes;

    static IrcBuildResult line(std::string_view command);
    static std::vector<std::string> splitTrailingParam(std::string_view prefix,
                                                       std::string_view body,
                                                       std::string_view suffix = {});
    static IrcBuildResult nick(std::string_view nickname);
    static IrcBuildResult user(std::string_view username, std::string_view realname);
    static IrcBuildResult pass(std::string_view password);
    static IrcBuildResult join(std::string_view channel, std::string_view key = {});
    static IrcBuildResult registration(std::string_view nickname,
                                       std::string_view username,
                                       std::string_view realname,
                                       std::string_view password = {});
};

#endif
