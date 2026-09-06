#include "ircserverfeatures.h"

#include <array>
#include <charconv>

namespace
{
char asciiLower(char character)
{
    if (character >= 'A' && character <= 'Z')
        return static_cast<char>(character - 'A' + 'a');
    return character;
}

std::uint32_t bitFor(char letter)
{
    if (letter < 'a' || letter > 'z')
        return 0;
    return std::uint32_t{1} << (letter - 'a');
}

bool splitChanModes(std::string_view value, std::array<std::string_view, 4>& parts)
{
    std::size_t count = 0;
    std::size_t start = 0;
    for (std::size_t index = 0; index <= value.size(); ++index) {
        if (index != value.size() && value[index] != ',')
            continue;
        if (count == 4)
            return false;
        parts[count++] = value.substr(start, index - start);
        start = index + 1;
    }
    return count == 4;
}
}

IrcServerFeatures::IrcServerFeatures()
    : m_channelTypes("#&")
    , m_prefixPairs{{'q', '~'}, {'a', '&'}, {'o', '@'}, {'h', '%'}, {'v', '+'}}
    , m_chanModesA("b")
    , m_chanModesB("k")
    , m_chanModesC("l")
    , m_chanModesD("imnpst")
{
    rebuildPrefixDumps();
}

void IrcServerFeatures::rebuildPrefixDumps()
{
    m_prefixModes.clear();
    m_prefixSymbols.clear();
    m_prefixModes.reserve(m_prefixPairs.size());
    m_prefixSymbols.reserve(m_prefixPairs.size());
    for (const auto& pair : m_prefixPairs) {
        m_prefixModes.push_back(pair.first);
        m_prefixSymbols.push_back(pair.second);
    }
    rebuildModeRules();
}

void IrcServerFeatures::rebuildModeRules()
{
    m_modeRules.fill(ModeParamRule::Never);
    const auto assign = [this](std::string_view letters, ModeParamRule rule) {
        for (char letter : letters)
            m_modeRules[static_cast<unsigned char>(letter)] = rule;
    };
    assign(m_chanModesA, ModeParamRule::Always);
    assign(m_chanModesB, ModeParamRule::Always);
    assign(m_chanModesC, ModeParamRule::SetOnly);
    assign(m_chanModesD, ModeParamRule::Never);
    for (const auto& pair : m_prefixPairs) {
        m_modeRules[static_cast<unsigned char>(pair.first)] =
            ModeParamRule::Prefix;
    }
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
        m_prefixPairs.clear();
        m_prefixPairs.reserve(modes.size());
        for (std::size_t index = 0; index < modes.size(); ++index)
            m_prefixPairs.emplace_back(asciiLower(modes[index]), symbols[index]);
        rebuildPrefixDumps();
        return;
    }

    if (name == "CHANMODES") {
        std::array<std::string_view, 4> parts{};
        if (!splitChanModes(value, parts))
            return;
        m_chanModesA.assign(parts[0]);
        m_chanModesB.assign(parts[1]);
        m_chanModesC.assign(parts[2]);
        m_chanModesD.assign(parts[3]);
        rebuildModeRules();
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

std::string_view IrcServerFeatures::chanModesA() const noexcept
{
    return m_chanModesA;
}

std::string_view IrcServerFeatures::chanModesB() const noexcept
{
    return m_chanModesB;
}

std::string_view IrcServerFeatures::chanModesC() const noexcept
{
    return m_chanModesC;
}

std::string_view IrcServerFeatures::chanModesD() const noexcept
{
    return m_chanModesD;
}

char IrcServerFeatures::letterForSymbol(char symbol) const
{
    for (const auto& pair : m_prefixPairs) {
        if (pair.second == symbol)
            return pair.first;
    }
    return '\0';
}

IrcServerFeatures::ModeParamRule IrcServerFeatures::ruleFor(char raw) const
{
    const ModeParamRule lowered =
        m_modeRules[static_cast<unsigned char>(asciiLower(raw))];
    if (lowered == ModeParamRule::Prefix)
        return ModeParamRule::Prefix;
    return m_modeRules[static_cast<unsigned char>(raw)];
}

std::optional<IrcParsedName> IrcServerFeatures::parseNamesToken(
    std::string_view token) const
{
    std::uint32_t bits = 0;
    std::size_t index = 0;
    while (index < token.size()) {
        const char letter = letterForSymbol(token[index]);
        if (letter == '\0')
            break;
        bits |= bitFor(letter);
        ++index;
    }
    const std::string_view nick = token.substr(index);
    if (nick.empty())
        return std::nullopt;
    return IrcParsedName{std::string(nick), IrcPrefixSet(bits)};
}

std::vector<IrcPrefixChange> IrcServerFeatures::prefixChanges(
    std::string_view mode, const std::vector<std::string>& arguments) const
{
    std::vector<IrcPrefixChange> changes;
    bool grant = true;
    std::size_t argument = 0;
    for (char raw : mode) {
        if (raw == '+') {
            grant = true;
            continue;
        }
        if (raw == '-') {
            grant = false;
            continue;
        }
        const char letter = asciiLower(raw);
        const ModeParamRule rule = ruleFor(raw);
        const bool consumes = rule == ModeParamRule::Always
            || rule == ModeParamRule::Prefix
            || (rule == ModeParamRule::SetOnly && grant);
        if (rule == ModeParamRule::Prefix) {
            if (argument >= arguments.size())
                continue;
            changes.emplace_back(
                IrcPrefixChange(arguments[argument++], letter, grant));
            continue;
        }
        if (!consumes)
            continue;
        if (argument >= arguments.size())
            continue;
        ++argument;
    }
    return changes;
}

IrcPrefixSet IrcServerFeatures::apply(IrcPrefixSet ranks,
                                      const IrcPrefixChange& change) const
{
    const std::uint32_t bit = bitFor(change.m_letter);
    if (bit == 0)
        return ranks;
    if (change.m_grant)
        return IrcPrefixSet(ranks.m_bits | bit);
    return IrcPrefixSet(ranks.m_bits & ~bit);
}

std::string IrcServerFeatures::memberLabel(const IrcPrefixSet& ranks,
                                           std::string_view nick) const
{
    for (const auto& pair : m_prefixPairs) {
        if ((ranks.m_bits & bitFor(pair.first)) != 0) {
            std::string label;
            label.reserve(nick.size() + 1);
            label.push_back(pair.second);
            label.append(nick);
            return label;
        }
    }
    return std::string(nick);
}
