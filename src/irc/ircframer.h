/*
 * Copyright (C) 2011 Fredi Machado <https://github.com/fredimachado>
 *
 * Omairc adapted the protocol behavior from IRCClient under the GNU Lesser
 * General Public License, version 3 or later.
 */

#ifndef IRCFRAMER_H
#define IRCFRAMER_H

#include "ircmessage.h"

#include <cstddef>
#include <string>
#include <string_view>
#include <vector>

struct IrcFrameResult
{
    std::vector<std::string> frames;
    std::vector<IrcError> errors;
};

class IrcFramer
{
public:
    static constexpr std::size_t kMaxClassicFrameBytes = IrcProtocol::maxClassicFrameBytes;
    static constexpr std::size_t kMaxTagSectionBytes = IrcProtocol::maxTagSectionBytes;

    IrcFrameResult feed(std::string_view bytes);

private:
    bool pendingFrameIsOverlong() const;
    bool completeFrameIsValid(std::size_t delimiter) const;
    void beginDiscard(IrcFrameResult& result, IrcError error);

    std::string buffer_;
    bool discarding_ = false;
};

#endif
