#include <QCommandLineParser>
#include <QCoreApplication>
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
#include <QWindow>

#include <stdio.h>
#include <string.h>

#include "backend.h"
#include "irc/ircconnection.h"
#include "irc/irccontroller.h"
#include "irc/ircslashcomplete.h"
#include "omairccli.h"
#include "omaircipchandler.h"
#include "singleinstance.h"
#include "systemtheme.h"

#ifndef OMAIRC_VERSION
#error "Build with omairc.pro so OMAIRC_VERSION is defined"
#endif

static void raiseOmaircWindow(QQmlApplicationEngine &engine)
{
    const auto roots = engine.rootObjects();
    if (roots.isEmpty())
        return;
    if (auto *window = qobject_cast<QWindow *>(roots.constFirst())) {
        window->show();
        window->raise();
        window->requestActivate();
    }
}

int main(int argc, char *argv[]) {
    for (int i = 1; i < argc; ++i) {
        if (strcmp(argv[i], "--version") == 0) {
            fputs("omairc " OMAIRC_VERSION "\n", stdout);
            return 0;
        }
    }

    if (OmaircCli::looksLikeCommand(argc, argv)) {
        QCoreApplication app(argc, argv);
        app.setApplicationName(QStringLiteral("omairc"));
        app.setApplicationVersion(QStringLiteral(OMAIRC_VERSION));
        return OmaircCli::run(app);
    }

    QGuiApplication app(argc, argv);
    app.setApplicationName(QStringLiteral("omairc"));
    app.setApplicationVersion(QStringLiteral(OMAIRC_VERSION));
    app.setDesktopFileName(QStringLiteral("omairc"));
    app.setWindowIcon(QIcon::fromTheme(
        QStringLiteral("omairc"),
        QIcon(QStringLiteral(":/icons/omairc.svg"))));
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

    SingleInstance instance;
    const bool guardProcess =
        !mockMode && qEnvironmentVariableIsEmpty("OMAIRC_ALLOW_MULTI");
    if (guardProcess && !instance.acquireOrNotify())
        return 0;

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

    bool pendingRaise = false;
    OmaircIpcHandler ipcHandler(ircController);
    if (instance.isPrimary()) {
        const auto raiseWindow = [&engine, &pendingRaise]() {
            if (engine.rootObjects().isEmpty()) {
                pendingRaise = true;
                return;
            }
            raiseOmaircWindow(engine);
        };
        ipcHandler.setRaiseFn(raiseWindow);
        instance.setRequestHandler(
            [&ipcHandler](const QByteArray &line) {
                return ipcHandler.handleLine(line);
            });
        QObject::connect(&instance, &SingleInstance::activationRequested, &app,
                         [raiseWindow]() { raiseWindow(); });
    }

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
    if (pendingRaise)
        raiseOmaircWindow(engine);
    if (ircConnection)
        ircConnection->activate();

    return app.exec();
}
