#pragma once

#include "irccasemapping.h"

#include <cstddef>
#include <cstdint>
#include <optional>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

class IrcPrefixSet
{
public:
    IrcPrefixSet() noexcept = default;

    bool empty() const noexcept { return m_bits == 0; }

    friend bool operator==(const IrcPrefixSet& left,
                           const IrcPrefixSet& right) noexcept
    {
        return left.m_bits == right.m_bits;
    }

    friend bool operator!=(const IrcPrefixSet& left,
                           const IrcPrefixSet& right) noexcept
    {
        return left.m_bits != right.m_bits;
    }

private:
    friend class IrcServerFeatures;
    explicit IrcPrefixSet(std::uint32_t bits) noexcept
        : m_bits(bits)
    {
    }

    std::uint32_t m_bits = 0;
};

class IrcPrefixChange
{
public:
    const std::string& nick() const noexcept { return m_nick; }

private:
    friend class IrcServerFeatures;
    IrcPrefixChange(std::string nick, char letter, bool grant)
        : m_nick(std::move(nick))
        , m_letter(letter)
        , m_grant(grant)
    {
    }

    std::string m_nick;
    char m_letter = '\0';
    bool m_grant = true;
};

struct IrcParsedName
{
    std::string nick;
    IrcPrefixSet ranks;
};

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

    std::optional<IrcParsedName> parseNamesToken(std::string_view token) const;
    std::vector<IrcPrefixChange> prefixChanges(
        std::string_view mode,
        const std::vector<std::string>& arguments) const;
    IrcPrefixSet apply(IrcPrefixSet ranks, const IrcPrefixChange& change) const;
    std::string memberLabel(const IrcPrefixSet& ranks,
                            std::string_view nick) const;

private:
    char letterForSymbol(char symbol) const;
    char symbolForLetter(char mode) const;
    void rebuildPrefixDumps();

    IrcCaseMapping m_caseMapping;
    std::string m_channelTypes;
    std::vector<std::pair<char, char>> m_prefixPairs;
    std::string m_prefixModes;
    std::string m_prefixSymbols;
    std::optional<std::size_t> m_nickLength;
};
