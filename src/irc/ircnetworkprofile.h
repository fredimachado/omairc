#pragma once

#include <QList>
#include <QString>
#include <QStringList>

struct IrcNetworkProfile
{
    enum class Problem {
        None,
        MissingHost,
        MissingNick,
        InvalidPort,
        UnsendableIdentity,
        UnsendableAccount,
        UnsendableBouncerNetwork,
        UnsendableChannel,
    };

    QString networkId;
    QString host;
    quint16 port = 6697;
    bool tlsEnabled = true;
    bool connectOnStartup = false;
    bool secretSaved = false;
    bool nickServSaved = false;
    QString nick;
    QString username;
    QString realname;
    QString account;
    QString bouncerNetwork;
    QStringList autojoinChannels;
    static constexpr int iconColorCount = 5;
    static constexpr int noIconColor = -1;
    int iconColor = noIconColor;

    static IrcNetworkProfile create();
    static IrcNetworkProfile suggested();
    static QStringList parseAutojoin(const QString &channels);
    static int pickIconColor(const QList<int> &used);
    bool ensureIconColor(const QList<int> &used);
    IrcNetworkProfile normalized() const;
    // Bouncers select the upstream network from the account name, after a slash.
    QString saslAccount() const;
    Problem validate() const;
    bool isComplete() const;
    static QString problemText(Problem problem);

    friend bool operator==(const IrcNetworkProfile &, const IrcNetworkProfile &);
    friend bool operator!=(const IrcNetworkProfile &, const IrcNetworkProfile &);
};
