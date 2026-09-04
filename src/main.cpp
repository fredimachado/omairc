#include <QCommandLineParser>
#include <QFile>
#include <QFont>
#include <QFontDatabase>
#include <QGuiApplication>
#include <QIcon>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQmlError>
#include <QQuickStyle>
#include <QUrl>

#include "backend.h"
#include "irc/ircconnection.h"
#include "irc/irccontroller.h"
#include "irc/ircslashcomplete.h"
#include "systemtheme.h"

int main(int argc, char *argv[]) {
    QGuiApplication app(argc, argv);
    app.setApplicationName(QStringLiteral("omairc"));
    app.setDesktopFileName(QStringLiteral("omairc"));
    app.setWindowIcon(QIcon::fromTheme(QStringLiteral("omairc")));
    app.setOrganizationName(QStringLiteral("omairc"));

    QCommandLineParser parser;
    parser.setApplicationDescription(
        QStringLiteral("A dead-simple IRC client for Omarchy."));
    parser.addHelpOption();
    const QCommandLineOption mockOption(
        QStringLiteral("mock"),
        QStringLiteral("Open the local prototype UI without connecting."));
    parser.addOption(mockOption);
    parser.process(app);
    const bool mockMode = parser.isSet(mockOption);

    QFontDatabase::addApplicationFont(QStringLiteral(":/fonts/iAWriterMonoS-Regular.ttf"));
    QFontDatabase::addApplicationFont(QStringLiteral(":/fonts/iAWriterMonoS-Bold.ttf"));

    QQuickStyle::setStyle(QStringLiteral("Material"));

    Backend backend(&app);
    IrcSlashSession slashSession(&app);
    IrcController *ircController = nullptr;
    IrcConnection *ircConnection = nullptr;
    if (!mockMode) {
        ircController = new IrcController(&app);
        ircConnection = new IrcConnection(*ircController, &app);
    }
    SystemTheme systemTheme(&app);
    backend.setDarkMode(systemTheme.darkMode());

    QObject::connect(&systemTheme, &SystemTheme::darkModeChanged, &backend,
                     &Backend::setDarkMode);

    const QFont interfaceFont(QStringLiteral("iA Writer Mono S"));
    const qreal basePointSize = interfaceFont.pointSizeF() > 0
        ? interfaceFont.pointSizeF()
        : app.font().pointSizeF();
    const auto applyInterfaceFont = [&app, interfaceFont, basePointSize](qreal textScale) {
        QFont scaled = interfaceFont;
        scaled.setPointSizeF(basePointSize * textScale);
        app.setFont(scaled);
    };
    applyInterfaceFont(systemTheme.textScale());
    backend.setTextScale(systemTheme.textScale());

    QObject::connect(&systemTheme, &SystemTheme::textScaleChanged, &backend,
                     [&backend, applyInterfaceFont](qreal textScale) {
        applyInterfaceFont(textScale);
        backend.setTextScale(textScale);
    });

    QQmlApplicationEngine engine;
    QObject::connect(&engine, &QQmlApplicationEngine::warnings, &app,
                     [](const QList<QQmlError> &warnings) {
        for (const QQmlError &warning : warnings)
            qWarning().noquote() << warning.toString();
    });
    engine.rootContext()->setContextProperty(QStringLiteral("appBackend"), &backend);
    engine.rootContext()->setContextProperty(
        QStringLiteral("ircController"), ircController);
    engine.rootContext()->setContextProperty(
        QStringLiteral("ircConnection"), ircConnection);
    engine.rootContext()->setContextProperty(
        QStringLiteral("slashSession"), &slashSession);
    engine.load(QUrl(QStringLiteral("qrc:/Main.qml")));
    if (engine.rootObjects().isEmpty()) {
        qCritical() << "Could not load the Omairc interface; resource available:"
                    << QFile::exists(QStringLiteral(":/Main.qml"));
        return -1;
    }
    if (ircConnection)
        ircConnection->activate();

    return app.exec();
}
