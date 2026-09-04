#pragma once

#include "ircevent.h"
#include "ircmessage.h"
#include "ircserverfeatures.h"

#include <QString>

#include <vector>

class IrcEventTranslator
{
public:
    static std::vector<IrcEvent> translate(const QString& networkId,
                                           const QString& currentNick,
                                           const IrcServerFeatures& features,
                                           const IrcMessage& message);
};
