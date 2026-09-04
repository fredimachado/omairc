#include <QCoreApplication>

int runLiveIrcdTests(int argc, char **argv);

int main(int argc, char **argv)
{
    QCoreApplication app(argc, argv);
    return runLiveIrcdTests(argc, argv);
}
