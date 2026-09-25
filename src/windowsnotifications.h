#pragma once

#include <QObject>
#include <QString>

struct WindowsNotificationsImpl;

// Native Windows toast notifications for mentions and direct messages.
// Mirrors MacOsNotifications: Backend owns one instance and forwards
// activation through the activated() signal. The WinRT and COM work runs on a
// dedicated thread so the Qt main thread keeps its own COM apartment.
class WindowsNotifications : public QObject {
    Q_OBJECT

public:
    explicit WindowsNotifications(QObject *parent = nullptr);
    ~WindowsNotifications() override;

    void notify(const QString &summary, const QString &body,
                const QString &networkId, const QString &target,
                const QString &msgid);

signals:
    void activated(const QString &networkId, const QString &target,
                   const QString &msgid);

private:
    WindowsNotificationsImpl *m_impl = nullptr;
    bool m_enabled = false;
};
