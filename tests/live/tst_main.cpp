#include <QCoreApplication>

int runLiveIrcdTests(int argc, char **argv);
int runLiveMembersTests(int argc, char **argv);
int runLiveReconnectTests(int argc, char **argv);

int main(int argc, char **argv)
{
    QCoreApplication app(argc, argv);
    const int ircd = runLiveIrcdTests(argc, argv);
    const int members = runLiveMembersTests(argc, argv);
    const int reconnect = runLiveReconnectTests(argc, argv);
    return (ircd == 0 && members == 0 && reconnect == 0) ? 0 : 1;
}
