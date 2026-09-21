#pragma once

#include <QAbstractListModel>
#include <QByteArray>
#include <QHash>
#include <QString>
#include <QTimer>
#include <QVariant>
#include <QVariantMap>
#include <QVector>

#include "irctextformatter.h"

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
    Q_PROPERTY(QString error READ error NOTIFY stateChanged)
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

    static constexpr int kMaxRows = 8000;

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
    QString error() const;
    QString networkId() const;
    QString mask() const;
    int sourceCount() const;

    void show(const QString& networkId,
              const QString& mask,
              QVector<IrcChannelListRow> rows,
              bool complete,
              bool loading,
              bool cached,
              const QString& error = {});
    void beginLoad(const QString& networkId, const QString& mask);
    void appendRow(IrcChannelListRow row);
    void fail(const QString& error);
    void clear();

signals:
    void filterChanged();
    void stateChanged();

private:
    QString plainTopic(const QString& topic) const;
    bool matches(const IrcChannelListRow& row) const;
    void rebuildVisible();
    void emitStateSoon();
    void emitStateNow();
    QString rowLabel(const IrcChannelListRow& row) const;

    QVector<IrcChannelListRow> m_source;
    QVector<int> m_visible;
    QString m_filter;
    QString m_filterQuery;
    QString m_networkId;
    QString m_mask;
    QString m_error;
    QTimer m_stateFlush;
    IrcTextFormatter m_textFormatter;
    bool m_loading = false;
    bool m_complete = false;
    bool m_cached = false;
};
