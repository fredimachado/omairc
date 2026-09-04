#include <QCoreApplication>

int runProtocolTests(int argc, char **argv);
int runCaseMappingTests(int argc, char **argv);
int runTransportTests(int argc, char **argv);
int runCapabilityTests(int argc, char **argv);
int runSessionTests(int argc, char **argv);
int runControllerTests(int argc, char **argv);
int runTypingTests(int argc, char **argv);
int runProfileTests(int argc, char **argv);
int runConnectionTests(int argc, char **argv);
int runReducerTests(int argc, char **argv);
int runModelTests(int argc, char **argv);
int runQtIrcTransportIntegrationTests(int argc, char **argv);

int main(int argc, char **argv)
{
    QCoreApplication app(argc, argv);
    const int protocolStatus = runProtocolTests(argc, argv);
    const int caseMappingStatus = runCaseMappingTests(argc, argv);
    const int transportStatus = runTransportTests(argc, argv);
    const int capabilityStatus = runCapabilityTests(argc, argv);
    const int sessionStatus = runSessionTests(argc, argv);
    const int controllerStatus = runControllerTests(argc, argv);
    const int typingStatus = runTypingTests(argc, argv);
    const int profileStatus = runProfileTests(argc, argv);
    const int connectionStatus = runConnectionTests(argc, argv);
    const int reducerStatus = runReducerTests(argc, argv);
    const int modelStatus = runModelTests(argc, argv);
    const int integrationStatus = runQtIrcTransportIntegrationTests(argc, argv);
    const int statuses[] = {
        protocolStatus,
        caseMappingStatus,
        transportStatus,
        capabilityStatus,
        sessionStatus,
        controllerStatus,
        typingStatus,
        profileStatus,
        connectionStatus,
        reducerStatus,
        modelStatus,
        integrationStatus,
    };
    for (const int status : statuses) {
        if (status != 0)
            return status;
    }
    return 0;
}
