#pragma once

#include <QObject>
#include <QString>

class MacOsNotifications : public QObject {
    Q_OBJECT

public:
    explicit MacOsNotifications(QObject *parent = nullptr);
    ~MacOsNotifications() override;

    void notify(const QString &summary, const QString &body,
                const QString &networkId, const QString &target,
                const QString &msgid);

signals:
    void activated(const QString &networkId, const QString &target,
                   const QString &msgid);

private:
    bool m_enabled = false;
};
