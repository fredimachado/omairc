#include <QFontDatabase>
#include <QGuiApplication>
#include <QQuickStyle>

#include "omaircupdatecheck.h"
#include "omaircversion.h"

int runLiveUiTests(int argc, char **argv);

int main(int argc, char **argv)
{
    qputenv("OMAIRC_ALLOW_MULTI", "1");
    QGuiApplication app(argc, argv);
    omaircRegisterUpdateCheck();
    app.setApplicationName(QStringLiteral("omairc"));
    app.setApplicationVersion(QStringLiteral(OMAIRC_VERSION));
    app.setOrganizationName(QStringLiteral("omairc"));
    QFontDatabase::addApplicationFont(QStringLiteral(":/fonts/iAWriterMonoS-Regular.ttf"));
    QFontDatabase::addApplicationFont(QStringLiteral(":/fonts/iAWriterMonoS-Bold.ttf"));
    QQuickStyle::setStyle(QStringLiteral("Material"));
    return runLiveUiTests(argc, argv);
}
