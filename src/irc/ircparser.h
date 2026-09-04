/*
 * Copyright (C) 2011 Fredi Machado <https://github.com/fredimachado>
 *
 * Omairc adapted the protocol behavior from IRCClient under the GNU Lesser
 * General Public License, version 3 or later.
 */

#ifndef IRCPARSER_H
#define IRCPARSER_H

#include "ircmessage.h"

#include <string_view>

class IrcParser
{
public:
    static IrcParseResult parse(std::string_view line);
};

#endif
