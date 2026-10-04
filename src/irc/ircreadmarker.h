#pragma once

#include <QDateTime>
#include <QString>

#include <optional>

struct IrcMessage;

// Parsed server read-marker payload: absent means the server sent `*`.
std::optional<std::optional<QDateTime>> parseIrcReadMarkerParameter(
    const QString& parameter);

QString formatIrcReadMarkerTimestamp(const QDateTime& when);

// Incoming `:prefix COMMAND target ...` when the read-marker cap is enabled.
// Returns the target and marker, or nullopt when the line is not a read marker.
std::optional<std::pair<QString, std::optional<QDateTime>>> parseIrcReadMarkerLine(
    const IrcMessage& message,
    const QString& command);
