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

#include "ircframer.h"

bool IrcFramer::completeFrameIsValid(std::size_t delimiter) const
{
    if (buffer_.empty() || buffer_.front() != '@')
        return delimiter + 2 <= kMaxInboundClassicFrameBytes;

    const std::size_t space = buffer_.find(' ');
    if (space == std::string::npos || space >= delimiter)
        return delimiter + 2 <= kMaxInboundClassicFrameBytes;

    const std::size_t tagPayloadBytes = space - 1;
    const std::size_t classicFrameBytes = delimiter - space - 1 + 2;
    return tagPayloadBytes <= kMaxTagSectionBytes
        && classicFrameBytes <= kMaxInboundClassicFrameBytes;
}

bool IrcFramer::pendingFrameIsOverlong() const
{
    if (buffer_.empty() || buffer_.front() != '@')
        return buffer_.size() >= kMaxInboundClassicFrameBytes;

    const std::size_t space = buffer_.find(' ');
    if (space == std::string::npos)
        return buffer_.size() > 1 + kMaxTagSectionBytes;
    if (space > 0 && space - 1 > kMaxTagSectionBytes)
        return true;

    return buffer_.size() - space - 1 >= kMaxInboundClassicFrameBytes;
}

void IrcFramer::recordFault(IrcFrameResult& result, IrcError error, std::string_view bytes)
{
    IrcFrameFault fault;
    fault.error = error;
    fault.byteCount = bytes.size();
    const std::size_t take = bytes.size() < kFaultPreviewBytes
        ? bytes.size() : kFaultPreviewBytes;
    fault.preview.assign(bytes.data(), take);
    result.faults.push_back(std::move(fault));
}

void IrcFramer::beginDiscard(IrcFrameResult& result, IrcError error)
{
    recordFault(result, error, buffer_);
    discarding_ = true;
    const bool keepCarriageReturn = !buffer_.empty() && buffer_.back() == '\r';
    buffer_.clear();
    if (keepCarriageReturn)
        buffer_.push_back('\r');
}

IrcFrameResult IrcFramer::feed(std::string_view bytes)
{
    IrcFrameResult result;
    buffer_.append(bytes.data(), bytes.size());

    for (;;) {
        if (discarding_) {
            const std::size_t delimiter = buffer_.find("\r\n");
            if (delimiter == std::string::npos) {
                const bool keepCarriageReturn = !buffer_.empty() && buffer_.back() == '\r';
                buffer_.clear();
                if (keepCarriageReturn)
                    buffer_.push_back('\r');
                return result;
            }
            buffer_.erase(0, delimiter + 2);
            discarding_ = false;
            continue;
        }

        const std::size_t delimiter = buffer_.find("\r\n");
        if (delimiter != std::string::npos) {
            const std::size_t nul = buffer_.find('\0');
            if (nul < delimiter) {
                recordFault(result, IrcError::InvalidCharacter,
                            std::string_view(buffer_.data(), delimiter));
                buffer_.erase(0, delimiter + 2);
                continue;
            }
            if (!completeFrameIsValid(delimiter)) {
                recordFault(result, IrcError::TooManyBytes,
                            std::string_view(buffer_.data(), delimiter));
                buffer_.erase(0, delimiter + 2);
                continue;
            }

            result.frames.push_back(buffer_.substr(0, delimiter));
            buffer_.erase(0, delimiter + 2);
            continue;
        }

        if (buffer_.find('\0') != std::string::npos) {
            beginDiscard(result, IrcError::InvalidCharacter);
            continue;
        }
        if (pendingFrameIsOverlong()) {
            beginDiscard(result, IrcError::TooManyBytes);
            continue;
        }
        return result;
    }
}
