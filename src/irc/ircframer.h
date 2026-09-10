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

struct IrcFrameFault
{
    IrcError error = IrcError::None;
    std::string preview;
    std::size_t byteCount = 0;
};

struct IrcFrameResult
{
    std::vector<std::string> frames;
    std::vector<IrcFrameFault> faults;
};

class IrcFramer
{
public:
    static constexpr std::size_t kMaxInboundClassicFrameBytes =
        IrcProtocol::maxInboundClassicFrameBytes;
    static constexpr std::size_t kMaxTagSectionBytes = IrcProtocol::maxTagSectionBytes;
    static constexpr std::size_t kFaultPreviewBytes = 160;

    IrcFrameResult feed(std::string_view bytes);

private:
    bool pendingFrameIsOverlong() const;
    bool completeFrameIsValid(std::size_t delimiter) const;
    void recordFault(IrcFrameResult& result, IrcError error, std::string_view bytes);
    void beginDiscard(IrcFrameResult& result, IrcError error);

    std::string buffer_;
    bool discarding_ = false;
};

#endif
