#pragma once

#include <QObject>

#ifdef Q_OS_UNIX
#include <functional>

class QDBusVariant;
#endif

class SystemTheme : public QObject {
    Q_OBJECT

public:
    explicit SystemTheme(QObject *parent = nullptr);

    bool darkMode() const { return m_darkMode; }
    qreal textScale() const { return m_textScale; }

signals:
    void darkModeChanged(bool darkMode);
    void textScaleChanged(qreal textScale);

public slots:
    void refresh();

#ifdef Q_OS_UNIX
private slots:
    void handlePortalSettingChanged(const QString &nameSpace, const QString &key,
                                    const QDBusVariant &value);
#endif

private:
#ifdef Q_OS_UNIX
    void requestPortalSetting(const QString &nameSpace, const QString &key,
                              std::function<void(const QVariant &)> handler);
    void requestPortalDarkMode();
    void requestPortalTextScale();
#endif
    bool qtDarkMode(bool *known) const;
    void setDarkMode(bool darkMode);
    void setTextScale(qreal textScale);

    bool m_darkMode = true;
    qreal m_textScale = 1.0;
};
