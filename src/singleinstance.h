#pragma once

#include <QObject>
#include <QString>

class QLockFile;
class QLocalServer;

class SingleInstance final : public QObject {
    Q_OBJECT

public:
    explicit SingleInstance(QObject *parent = nullptr);
    ~SingleInstance() override;

    bool acquireOrNotify();
    bool isPrimary() const;

signals:
    void activationRequested();

private:
    QString lockFilePath() const;
    QString serverName() const;
    QString runtimeDir() const;
    bool becomePrimary();
    bool notifyPrimary();
    void listenForActivation();

    QLockFile *m_lock = nullptr;
    QLocalServer *m_server = nullptr;
    bool m_primary = false;
};
