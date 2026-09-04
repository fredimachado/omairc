#include "irccapability.h"

std::uint32_t IrcCapabilitySet::bit(IrcCapability capability) noexcept
{
    return std::uint32_t(1) << static_cast<unsigned>(capability);
}

bool IrcCapabilitySet::contains(IrcCapability capability) const noexcept
{
    return (m_bits & bit(capability)) != 0;
}

void IrcCapabilitySet::insert(IrcCapability capability) noexcept
{
    m_bits |= bit(capability);
}

void IrcCapabilitySet::remove(IrcCapability capability) noexcept
{
    m_bits &= ~bit(capability);
}

bool IrcCapabilitySet::isEmpty() const noexcept
{
    return m_bits == 0;
}
