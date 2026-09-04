#pragma once

#include <QAbstractListModel>
#include <QHash>
#include <QString>

class IrcNetworkLog;

class NetworkLogModel : public QAbstractListModel
{
    Q_OBJECT
public:
    enum Role {
        TimeRole = Qt::UserRole + 1,
        LabelRole,
        TextRole,
        SourceRole,
        SeverityRole,
    };

    explicit NetworkLogModel(IrcNetworkLog& log, QObject *parent = nullptr);

    int rowCount(const QModelIndex& parent = {}) const override;
    QVariant data(const QModelIndex& index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;
    void show(const QString& networkId);

private:
    void onAppended(const QString& networkId, int row);
    void onTrimmed(const QString& networkId, int removedFromFront);
    void onCleared(const QString& networkId);

    IrcNetworkLog& m_log;
    QString m_networkId;
};
