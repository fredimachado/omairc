#pragma once

#include <QStringView>

bool ircIsServiceIdentity(QStringView nick,
                          QStringView host,
                          QStringView channelTypes = {});
