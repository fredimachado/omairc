#include "seededircfixture.h"

#include <QFontDatabase>
#include <QQmlEngine>
#include <QQuickStyle>
#include <QtQml>
#include <QtQuickTest>

class SeededQmlSetup : public QObject
{
    Q_OBJECT

public:
    SeededQmlSetup()
    {
        qputenv("OMAIRC_ALLOW_MULTI", "1");
    }

public slots:
    void applicationAvailable()
    {
        QFontDatabase::addApplicationFont(QStringLiteral(":/fonts/iAWriterMonoS-Regular.ttf"));
        QFontDatabase::addApplicationFont(QStringLiteral(":/fonts/iAWriterMonoS-Bold.ttf"));
        QQuickStyle::setStyle(QStringLiteral("Material"));
    }

    void qmlEngineAvailable(QQmlEngine *)
    {
        qmlRegisterType<SeededIrcFixture>("Omairc.Test", 1, 0, "SeededIrcFixture");
    }
};

QUICK_TEST_MAIN_WITH_SETUP(seeded_qml, SeededQmlSetup)

#include "tst_main.moc"
