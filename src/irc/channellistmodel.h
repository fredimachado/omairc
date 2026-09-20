#pragma once

#include <QAbstractListModel>
#include <QByteArray>
#include <QHash>
#include <QString>
#include <QVariant>
#include <QVariantMap>
#include <QVector>

struct IrcChannelListRow
{
    QString channel;
    int users = 0;
    QString topic;
};

class ChannelListModel : public QAbstractListModel
{
    Q_OBJECT
    Q_PROPERTY(QString filter READ filter WRITE setFilter NOTIFY filterChanged)
    Q_PROPERTY(bool loading READ loading NOTIFY stateChanged)
    Q_PROPERTY(bool complete READ complete NOTIFY stateChanged)
    Q_PROPERTY(bool cached READ cached NOTIFY stateChanged)
    Q_PROPERTY(QString networkId READ networkId NOTIFY stateChanged)
    Q_PROPERTY(QString mask READ mask NOTIFY stateChanged)
    Q_PROPERTY(int sourceCount READ sourceCount NOTIFY stateChanged)

public:
    enum Role {
        ChannelRole = Qt::UserRole + 1,
        UsersRole,
        TopicRole,
        LabelRole,
    };

    explicit ChannelListModel(QObject *parent = nullptr);

    static QHash<int, QByteArray> staticRoleNames();

    int rowCount(const QModelIndex& parent = QModelIndex()) const override;
    QVariant data(const QModelIndex& index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;
    Q_INVOKABLE QVariantMap get(int row) const;
    Q_INVOKABLE QVariant field(int row, const QString& name) const;

    QString filter() const;
    void setFilter(const QString& filter);
    bool loading() const;
    bool complete() const;
    bool cached() const;
    QString networkId() const;
    QString mask() const;
    int sourceCount() const;
    const QVector<IrcChannelListRow>& sourceRows() const;

    void show(const QString& networkId,
              const QString& mask,
              QVector<IrcChannelListRow> rows,
              bool complete,
              bool loading,
              bool cached);
    void beginLoad(const QString& networkId, const QString& mask);
    void appendRow(IrcChannelListRow row);
    void finish(bool complete);
    void clear();

signals:
    void filterChanged();
    void stateChanged();

private:
    bool matches(const IrcChannelListRow& row) const;
    void rebuildVisible(bool resetAlways);
    QString rowLabel(const IrcChannelListRow& row) const;

    QVector<IrcChannelListRow> m_source;
    QVector<int> m_visible;
    QString m_filter;
    QString m_networkId;
    QString m_mask;
    bool m_loading = false;
    bool m_complete = false;
    bool m_cached = false;
};
