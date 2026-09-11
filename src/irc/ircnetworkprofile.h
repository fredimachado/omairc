#pragma once

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
        UnsendableChannel,
    };

    QString networkId;
    QString host;
    quint16 port = 6697;
    bool tlsEnabled = true;
    bool connectOnStartup = false;
    bool secretSaved = false;
    QString nick;
    QString username;
    QString realname;
    QStringList autojoinChannels;

    static IrcNetworkProfile create();
    static IrcNetworkProfile suggested();
    static QStringList parseAutojoin(const QString &channels);
    IrcNetworkProfile normalized() const;
    Problem validate() const;
    bool isComplete() const;
    static QString problemText(Problem problem);

    friend bool operator==(const IrcNetworkProfile &, const IrcNetworkProfile &);
    friend bool operator!=(const IrcNetworkProfile &, const IrcNetworkProfile &);
};
