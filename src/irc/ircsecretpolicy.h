#pragma once

#include "ircmessage.h"

#include <QString>
#include <QStringList>
#include <QStringView>

#include <optional>

struct IrcMaskedCommand
{
    QString verb;
    QStringList parameters;
};

namespace IrcSecretPolicy
{
std::optional<QString> redactWireLine(QStringView line, QStringView channelTypes = {});
std::optional<QString> redactPreviewLine(QStringView line, QStringView channelTypes = {});
std::optional<IrcMaskedCommand> redactMessage(const IrcMessage& message,
                                              QStringView channelTypes = {});
}
