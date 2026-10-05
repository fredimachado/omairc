#pragma once

#include <QHash>
#include <QObject>
#include <QSslConfiguration>
#include <QSslServer>
#include <QString>

class QSslSocket;

// FileUploadCatcher is a test HTTPS upload endpoint. It trusts the bundled
// localhost CA for this process only, and restores the previous default when
// it is destroyed. The product never loads that CA.
class FileUploadCatcher : public QObject
{
    Q_OBJECT
    Q_PROPERTY(int uploads READ uploads NOTIFY uploadsChanged)
    Q_PROPERTY(QString endpoint READ endpoint NOTIFY endpointChanged)
    Q_PROPERTY(QString lastError READ lastError NOTIFY lastErrorChanged)
    Q_PROPERTY(bool refuse READ refuse WRITE setRefuse NOTIFY refuseChanged)

public:
    explicit FileUploadCatcher(QObject *parent = nullptr);
    ~FileUploadCatcher() override;

    Q_INVOKABLE bool listen();
    int uploads() const;
    QString endpoint() const;
    QString lastError() const;
    bool refuse() const;
    void setRefuse(bool refuse);

signals:
    void uploadsChanged();
    void endpointChanged();
    void lastErrorChanged();
    void refuseChanged();

private:
    void takeConnection();
    void readSocket(QSslSocket *socket);
    void setLastError(const QString &error);

    QSslServer m_server;
    QHash<QSslSocket *, QByteArray> m_buffers;
    QSslConfiguration m_previousSsl;
    bool m_replacedSsl = false;
    int m_uploads = 0;
    QString m_lastError;
    bool m_refuse = false;
};
