#include <QCoreApplication>

#include "singleinstance.h"

int runProtocolTests(int argc, char **argv);
int runCaseMappingTests(int argc, char **argv);
int runCorpusTests(int argc, char **argv);
int runTransportTests(int argc, char **argv);
int runCapabilityTests(int argc, char **argv);
int runStsTests(int argc, char **argv);
int runSessionTests(int argc, char **argv);
int runControllerTests(int argc, char **argv);
int runCommandTests(int argc, char **argv);
int runIgnoreTests(int argc, char **argv);
int runMuteTests(int argc, char **argv);
int runOpenDirectTests(int argc, char **argv);
int runHighlightTests(int argc, char **argv);
int runTypingTests(int argc, char **argv);
int runProfileTests(int argc, char **argv);
int runConnectionTests(int argc, char **argv);
int runReducerTests(int argc, char **argv);
int runAvatarUrlTests(int argc, char **argv);
int runAvatarStoreTests(int argc, char **argv);
int runModelTests(int argc, char **argv);
int runQtIrcTransportIntegrationTests(int argc, char **argv);
int runSingleInstanceTests(int argc, char **argv);
int runOmaircIpcTests(int argc, char **argv);
int runOmaircCliTests(int argc, char **argv);
int runOmaircFileLogTests(int argc, char **argv);
int runIrcTextFormatterTests(int argc, char **argv);
int runOmaircUpdateCheckTests(int argc, char **argv);
int runSecretServiceTests(int argc, char **argv);
int runBackendTests(int argc, char **argv);

int main(int argc, char **argv)
{
#if defined(Q_OS_MACOS)
    // SecureTransport otherwise imports the loopback TLS key into the login
    // keychain and can block protocol_tests on a permission dialog.
    qputenv("QT_SSL_USE_TEMPORARY_KEYCHAIN", "1");
#endif
    QCoreApplication app(argc, argv);
    if (qEnvironmentVariableIsSet("OMAIRC_TEST_SI_SECONDARY")) {
        SingleInstance secondary;
        return secondary.acquireOrNotify() ? 1 : 0;
    }

    const int protocolStatus = runProtocolTests(argc, argv);
    const int caseMappingStatus = runCaseMappingTests(argc, argv);
    const int corpusStatus = runCorpusTests(argc, argv);
    const int transportStatus = runTransportTests(argc, argv);
    const int capabilityStatus = runCapabilityTests(argc, argv);
    const int stsStatus = runStsTests(argc, argv);
    const int sessionStatus = runSessionTests(argc, argv);
    const int controllerStatus = runControllerTests(argc, argv);
    const int commandStatus = runCommandTests(argc, argv);
    const int ignoreStatus = runIgnoreTests(argc, argv);
    const int muteStatus = runMuteTests(argc, argv);
    const int openDirectStatus = runOpenDirectTests(argc, argv);
    const int highlightStatus = runHighlightTests(argc, argv);
    const int typingStatus = runTypingTests(argc, argv);
    const int profileStatus = runProfileTests(argc, argv);
    const int connectionStatus = runConnectionTests(argc, argv);
    const int reducerStatus = runReducerTests(argc, argv);
    const int avatarUrlStatus = runAvatarUrlTests(argc, argv);
    const int avatarStoreStatus = runAvatarStoreTests(argc, argv);
    const int modelStatus = runModelTests(argc, argv);
    const int integrationStatus = runQtIrcTransportIntegrationTests(argc, argv);
    const int singleInstanceStatus = runSingleInstanceTests(argc, argv);
    const int ipcStatus = runOmaircIpcTests(argc, argv);
    const int cliStatus = runOmaircCliTests(argc, argv);
    const int fileLogStatus = runOmaircFileLogTests(argc, argv);
    const int ircTextFormatterStatus = runIrcTextFormatterTests(argc, argv);
    const int updateCheckStatus = runOmaircUpdateCheckTests(argc, argv);
    const int secretServiceStatus = runSecretServiceTests(argc, argv);
    const int backendStatus = runBackendTests(argc, argv);
    const int statuses[] = {
        protocolStatus,
        caseMappingStatus,
        corpusStatus,
        transportStatus,
        capabilityStatus,
        stsStatus,
        sessionStatus,
        controllerStatus,
        commandStatus,
        ignoreStatus,
        muteStatus,
        openDirectStatus,
        highlightStatus,
        typingStatus,
        profileStatus,
        connectionStatus,
        reducerStatus,
        avatarUrlStatus,
        avatarStoreStatus,
        modelStatus,
        integrationStatus,
        singleInstanceStatus,
        ipcStatus,
        cliStatus,
        fileLogStatus,
        ircTextFormatterStatus,
        updateCheckStatus,
        secretServiceStatus,
        backendStatus,
    };
    for (const int status : statuses) {
        if (status != 0)
            return status;
    }
    return 0;
}
