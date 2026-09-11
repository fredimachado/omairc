#include <QFontDatabase>
#include <QGuiApplication>
#include <QQuickStyle>

int runLiveUiTests(int argc, char **argv);

int main(int argc, char **argv)
{
    qputenv("OMAIRC_ALLOW_MULTI", "1");
    QGuiApplication app(argc, argv);
    app.setApplicationName(QStringLiteral("omairc"));
    app.setOrganizationName(QStringLiteral("omairc"));
    QFontDatabase::addApplicationFont(QStringLiteral(":/fonts/iAWriterMonoS-Regular.ttf"));
    QFontDatabase::addApplicationFont(QStringLiteral(":/fonts/iAWriterMonoS-Bold.ttf"));
    QQuickStyle::setStyle(QStringLiteral("Material"));
    return runLiveUiTests(argc, argv);
}
