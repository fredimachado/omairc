#include "ircavatarstore.h"
#include "omaircupdatecheck.h"
#include "seededircfixture.h"

#include <QCoreApplication>
#include <QFontDatabase>
#include <QQmlContext>
#include <QQmlEngine>
#include <QQuickStyle>
#include <QtQml>
#include <QtQuickTest>

#include "omaircversion.h"

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
        QCoreApplication::setApplicationVersion(QStringLiteral(OMAIRC_VERSION));
        QFontDatabase::addApplicationFont(QStringLiteral(":/fonts/iAWriterMonoS-Regular.ttf"));
        QFontDatabase::addApplicationFont(QStringLiteral(":/fonts/iAWriterMonoS-Bold.ttf"));
        QQuickStyle::setStyle(QStringLiteral("Material"));
    }

    void qmlEngineAvailable(QQmlEngine *engine)
    {
        qmlRegisterType<SeededIrcFixture>("Omairc.Test", 1, 0, "SeededIrcFixture");
        omaircRegisterUpdateCheck();
        IrcAvatarStore *store = ircInstallAvatarStore(engine);
        engine->rootContext()->setContextProperty(QStringLiteral("appAvatarStore"),
                                                  store);
    }
};

QUICK_TEST_MAIN_WITH_SETUP(seeded_qml, SeededQmlSetup)

#include "tst_main.moc"
