#pragma once

#include <QStringView>

bool ircIsServiceIdentity(QStringView nick,
                          QStringView host,
                          QStringView channelTypes = {});

/// Whether Omairc could put this nick in a PRIVMSG and reach a person, so that
/// a conversation opened for it is one the user can reply in. Deliberately not
/// a nick-grammar test: some networks allow a leading digit, so `0day` stays
/// routable while a bouncer pseudo-client such as `*status` does not.
bool ircNickIsRoutable(QStringView nick) noexcept;
