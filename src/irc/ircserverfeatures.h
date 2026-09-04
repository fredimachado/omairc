#pragma once

#include "irccasemapping.h"

#include <cstddef>
#include <optional>
#include <string>
#include <string_view>
#include <vector>

class IrcServerFeatures
{
public:
    IrcServerFeatures();

    void applyToken(std::string_view token);
    void applyTokens(const std::vector<std::string>& tokens);

    const IrcCaseMapping& caseMapping() const noexcept;
    std::string_view channelTypes() const noexcept;
    bool isChannel(std::string_view target) const noexcept;
    std::optional<std::size_t> nickLength() const noexcept;
    std::string_view prefixModes() const noexcept;
    std::string_view prefixSymbols() const noexcept;
    std::string statusForPrefix(char prefix) const;

private:
    IrcCaseMapping m_caseMapping;
    std::string m_channelTypes;
    std::string m_prefixModes;
    std::string m_prefixSymbols;
    std::optional<std::size_t> m_nickLength;
};
