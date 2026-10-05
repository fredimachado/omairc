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

    // frameBytes includes CRLF. Callers pass the network's LINELEN, or omit
    // it to keep the classic 512-byte frame used before ISUPPORT.
    static IrcBuildResult line(std::string_view command,
                               std::size_t frameBytes = kMaxFrameBytes);

    // Bytes the composer may hold so one send stays inside frameBytes.
    // An empty target is Status: a raw line, excluding CRLF. Any other
    // target is the PRIVMSG trailing body that fits with that target
    // (`PRIVMSG <target> :<body>\r\n`). Tags are not subtracted.
    static int composerByteBudget(std::string_view target,
                                  std::size_t frameBytes = kMaxFrameBytes);

    // A `/me` draft is sent as CTCP ACTION. The wrapper is 9 bytes beyond a
    // PRIVMSG (`\x01ACTION ` and `\x01`), and the 4-byte `/me ` counts here.
    static int actionComposerByteBudget(std::string_view target,
                                        std::size_t frameBytes = kMaxFrameBytes);
    static int composerByteBudgetForDraft(std::string_view target,
                                          std::string_view draft,
                                          std::size_t frameBytes = kMaxFrameBytes);

    // Longest UTF-8 prefix of text whose encoding fits in maxBytes. A 1, 2,
    // 3, or 4-byte scalar is kept whole or dropped.
    static std::string clampUtf8Prefix(std::string_view text, int maxBytes);

    static std::vector<std::string> splitTrailingParam(
        std::string_view prefix,
        std::string_view body,
        std::string_view suffix = {},
        std::size_t frameBytes = kMaxFrameBytes);
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
