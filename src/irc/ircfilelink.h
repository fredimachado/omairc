#pragma once

#include <optional>
#include <string>
#include <string_view>

// absoluteFileLink resolves a successful upload's Location against the
// upload endpoint. The result is an absolute http or https URL with no
// userinfo. Anything else is refused.
std::optional<std::string> absoluteFileLink(std::string_view endpoint,
                                            std::string_view location);

// fileHostSendsBasicAuth reports whether the IRC password may be sent to
// this upload endpoint. The scheme must be http or https, the URL must have
// no userinfo, and a cleartext upload is refused when the IRC connection is
// encrypted. serverHost is unused; a different https host still authenticates.
bool fileHostSendsBasicAuth(std::string_view serverHost,
                            std::string_view uploadUrl,
                            bool serverEncrypted);

// basicAuthorizationValue is the Authorization header value, or empty when
// the pair cannot be sent. Callers must not log it.
std::string basicAuthorizationValue(std::string_view user,
                                    std::string_view secret);
