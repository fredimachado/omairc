#pragma once

#include <QMetaType>

#include <cstdint>

enum class IrcCapability {
    Sasl,
    AwayNotify,
    Batch,
    MemberMetadata,
    MessageTags,
};

class IrcCapabilitySet
{
public:
    constexpr IrcCapabilitySet() noexcept = default;

    bool contains(IrcCapability capability) const noexcept;
    void insert(IrcCapability capability) noexcept;
    void remove(IrcCapability capability) noexcept;
    bool isEmpty() const noexcept;

    friend bool operator==(IrcCapabilitySet left, IrcCapabilitySet right) noexcept
    {
        return left.m_bits == right.m_bits;
    }

    friend bool operator!=(IrcCapabilitySet left, IrcCapabilitySet right) noexcept
    {
        return !(left == right);
    }

private:
    static std::uint32_t bit(IrcCapability capability) noexcept;

    std::uint32_t m_bits = 0;
};

Q_DECLARE_METATYPE(IrcCapabilitySet)
