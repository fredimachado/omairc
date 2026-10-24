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

// One attribute from a BOUNCER NETWORK / ADDNETWORK / CHANGENETWORK blob.
// removed is set when the key is present with an empty or missing value.
struct IrcBouncerAttribute
{
    std::string key;
    bool removed = false;
    std::string value;
};

enum class IrcBouncerSubcommand
{
    Bind,
    ListNetworks,
    AddNetwork,
    ChangeNetwork,
    DelNetwork,
    Network,
};

// A parsed BOUNCER command. Subcommands are matched case-insensitively.
// NETWORK <netid> * sets removed. Attribute blobs use message-tag encoding.
struct IrcParsedBouncer
{
    IrcBouncerSubcommand subcommand = IrcBouncerSubcommand::Bind;
    std::string netId;
    bool removed = false;
    std::vector<IrcBouncerAttribute> attributes;
};

std::optional<IrcParsedBouncer> parseBouncerCommand(const IrcMessage& message);

#endif
