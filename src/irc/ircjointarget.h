#pragma once

#include <QString>
#include <QVector>

#include <optional>

class IrcJoinTarget
{
public:
    static std::optional<IrcJoinTarget> make(const QString& channel,
                                             std::optional<QString> key = std::nullopt);

    const QString& channel() const noexcept { return m_channel; }
    const std::optional<QString>& key() const noexcept { return m_key; }
    bool hasKey() const noexcept { return m_key.has_value(); }

    friend bool operator==(const IrcJoinTarget&, const IrcJoinTarget&) noexcept;
    friend bool operator!=(const IrcJoinTarget&, const IrcJoinTarget&) noexcept;

private:
    IrcJoinTarget(QString channel, std::optional<QString> key);

    QString m_channel;
    std::optional<QString> m_key;
};

std::optional<QVector<IrcJoinTarget>> ircParseJoinTargets(const QString& argument);
