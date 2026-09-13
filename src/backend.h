#pragma once

#include <QFileSystemWatcher>
#include <QHash>
#include <QObject>
#include <QString>
#include <QVariantMap>

class Backend : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool darkMode READ darkMode WRITE setDarkMode NOTIFY darkModeChanged)
    Q_PROPERTY(qreal textScale READ textScale WRITE setTextScale NOTIFY textScaleChanged)
    Q_PROPERTY(QString themeBackground READ themeBackground NOTIFY themeColorsChanged)
    Q_PROPERTY(QString themeForeground READ themeForeground NOTIFY themeColorsChanged)
    Q_PROPERTY(QString themeAccent READ themeAccent NOTIFY themeColorsChanged)
    Q_PROPERTY(QString themeSelection READ themeSelection NOTIFY themeColorsChanged)

public:
    explicit Backend(QObject *parent = nullptr);

    bool darkMode() const { return m_darkMode; }
    void setDarkMode(bool darkMode);
    qreal textScale() const { return m_textScale; }
    void setTextScale(qreal textScale);
    QString themeBackground() const { return m_themeBackground; }
    QString themeForeground() const { return m_themeForeground; }
    QString themeAccent() const { return m_themeAccent; }
    QString themeSelection() const { return m_themeSelection; }

    Q_INVOKABLE QVariantMap windowGeometry() const;
    Q_INVOKABLE void saveWindowGeometry(int x, int y, int width, int height, bool maximized);
    Q_INVOKABLE void notifyDesktop(const QString &summary, const QString &body,
                                   const QString &networkId = {},
                                   const QString &target = {},
                                   const QString &msgid = {});

signals:
    void darkModeChanged();
    void textScaleChanged();
    void themeColorsChanged();
    void notificationActivated(const QString &networkId, const QString &target,
                               const QString &msgid);

private slots:
    void handleActionInvoked(uint id, const QString &actionKey);

private:
    struct NotifyConversation {
        QString networkId;
        QString target;
        QString msgid;
    };

    void loadOmarchyTheme();
    void watchOmarchyTheme();
    void rememberNotifyId(const QString &networkId, const QString &target,
                          const QString &msgid, uint id);

    bool m_darkMode = true;
    qreal m_textScale = 1.0;
    QString m_themeBackground;
    QString m_themeForeground;
    QString m_themeAccent;
    QString m_themeSelection;
    QFileSystemWatcher m_themeWatcher;
    QHash<QString, uint> m_conversationNotifyIds;
    QHash<uint, NotifyConversation> m_notifyById;
};
