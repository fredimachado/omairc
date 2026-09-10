#include <QCoreApplication>

#include "singleinstance.h"

int runProtocolTests(int argc, char **argv);
int runCaseMappingTests(int argc, char **argv);
int runCorpusTests(int argc, char **argv);
int runTransportTests(int argc, char **argv);
int runCapabilityTests(int argc, char **argv);
int runSessionTests(int argc, char **argv);
int runControllerTests(int argc, char **argv);
int runCommandTests(int argc, char **argv);
int runTypingTests(int argc, char **argv);
int runProfileTests(int argc, char **argv);
int runConnectionTests(int argc, char **argv);
int runReducerTests(int argc, char **argv);
int runModelTests(int argc, char **argv);
int runQtIrcTransportIntegrationTests(int argc, char **argv);
int runSingleInstanceTests(int argc, char **argv);
int runOmaircIpcTests(int argc, char **argv);
int runOmaircCliTests(int argc, char **argv);

int main(int argc, char **argv)
{
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
    const int sessionStatus = runSessionTests(argc, argv);
    const int controllerStatus = runControllerTests(argc, argv);
    const int commandStatus = runCommandTests(argc, argv);
    const int typingStatus = runTypingTests(argc, argv);
    const int profileStatus = runProfileTests(argc, argv);
    const int connectionStatus = runConnectionTests(argc, argv);
    const int reducerStatus = runReducerTests(argc, argv);
    const int modelStatus = runModelTests(argc, argv);
    const int integrationStatus = runQtIrcTransportIntegrationTests(argc, argv);
    const int singleInstanceStatus = runSingleInstanceTests(argc, argv);
    const int ipcStatus = runOmaircIpcTests(argc, argv);
    const int cliStatus = runOmaircCliTests(argc, argv);
    const int statuses[] = {
        protocolStatus,
        caseMappingStatus,
        corpusStatus,
        transportStatus,
        capabilityStatus,
        sessionStatus,
        controllerStatus,
        commandStatus,
        typingStatus,
        profileStatus,
        connectionStatus,
        reducerStatus,
        modelStatus,
        integrationStatus,
        singleInstanceStatus,
        ipcStatus,
        cliStatus,
    };
    for (const int status : statuses) {
        if (status != 0)
            return status;
    }
    return 0;
}
