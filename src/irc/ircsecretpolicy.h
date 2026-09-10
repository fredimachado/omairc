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
std::optional<QString> redactWireLine(QStringView line);
std::optional<IrcMaskedCommand> redactMessage(const IrcMessage& message);
}
