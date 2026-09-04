#include "ircserverfeatures.h"

#include <charconv>

IrcServerFeatures::IrcServerFeatures()
    : m_channelTypes("#&")
    , m_prefixModes("ov")
    , m_prefixSymbols("@+")
{
}

void IrcServerFeatures::applyToken(std::string_view token)
{
    if (token.empty() || token.front() == '-')
        return;

    const std::size_t separator = token.find('=');
    const std::string_view name = token.substr(0, separator);
    const std::string_view value = separator == std::string_view::npos
        ? std::string_view()
        : token.substr(separator + 1);

    if (name == "CASEMAPPING") {
        if (const auto mapping = IrcCaseMapping::fromName(value))
            m_caseMapping = *mapping;
        return;
    }

    if (name == "CHANTYPES") {
        m_channelTypes.assign(value);
        return;
    }

    if (name == "PREFIX") {
        if (value.size() < 4 || value.front() != '(')
            return;
        const std::size_t close = value.find(')');
        if (close == std::string_view::npos || close < 2)
            return;
        const std::string_view modes = value.substr(1, close - 1);
        const std::string_view symbols = value.substr(close + 1);
        if (modes.size() != symbols.size())
            return;
        m_prefixModes.assign(modes);
        m_prefixSymbols.assign(symbols);
        return;
    }

    if (name == "NICKLEN") {
        std::size_t length = 0;
        const char *begin = value.data();
        const char *end = begin + value.size();
        const auto result = std::from_chars(begin, end, length);
        if (result.ec == std::errc() && result.ptr == end && length > 0)
            m_nickLength = length;
    }
}

void IrcServerFeatures::applyTokens(const std::vector<std::string>& tokens)
{
    for (const std::string& token : tokens)
        applyToken(token);
}

const IrcCaseMapping& IrcServerFeatures::caseMapping() const noexcept
{
    return m_caseMapping;
}

std::string_view IrcServerFeatures::channelTypes() const noexcept
{
    return m_channelTypes;
}

bool IrcServerFeatures::isChannel(std::string_view target) const noexcept
{
    return !target.empty() && m_channelTypes.find(target.front()) != std::string::npos;
}

std::optional<std::size_t> IrcServerFeatures::nickLength() const noexcept
{
    return m_nickLength;
}

std::string_view IrcServerFeatures::prefixModes() const noexcept
{
    return m_prefixModes;
}

std::string_view IrcServerFeatures::prefixSymbols() const noexcept
{
    return m_prefixSymbols;
}

std::string IrcServerFeatures::statusForPrefix(char prefix) const
{
    const std::size_t index = m_prefixSymbols.find(prefix);
    if (index == std::string::npos)
        return {};
    return std::string(1, m_prefixModes[index]);
}
