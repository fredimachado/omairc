#include <QCoreApplication>
#include <QFile>
#include <QSettings>
#include <QTemporaryDir>
#include <QTest>

#include <memory>

#include "irccontroller.h"
#include "ircdemoserver.h"
#include "irccasemapping.h"
#include "ircscrollplace.h"
#include "messagelistmodel.h"
#include "testsettings.h"

class ScrollPlaceTest : public QObject
{
    Q_OBJECT

private slots:
    void init();
    void jsonBytesMatchTheSharedContract();
    void ephemeralRememberSkipsDisk();
    void demoPlaceSurvivesRestart();

private:
    std::unique_ptr<QTemporaryDir> m_dir;
};

void ScrollPlaceTest::init()
{
    m_dir = std::make_unique<QTemporaryDir>();
    QVERIFY(m_dir->isValid());
    TestSettings::isolate(m_dir->path());
    QCoreApplication::setOrganizationName(QStringLiteral("omairc"));
    QCoreApplication::setApplicationName(QStringLiteral("omairc"));
}

void ScrollPlaceTest::jsonBytesMatchTheSharedContract()
{
    const IrcCaseMapping mapping;
    IrcScrollPlaceStore store;
    const auto places = [] {
        QSettings settings;
        settings.sync();
        settings.beginGroup(QStringLiteral("scrollPlaces"));
        settings.beginGroup(QStringLiteral("net-1"));
        return settings.value(QStringLiteral("places")).toString();
    };

    IrcScrollPlace follow;
    follow.followEnd = true;
    store.remember(QStringLiteral("net-1"), QStringLiteral("#omarchy"), follow, mapping);
    QCOMPARE(places(),
             QStringLiteral("[{\"follow\":true,\"target\":\"#omarchy\"}]"));

    IrcScrollPlace msgid;
    msgid.followEnd = false;
    msgid.msgid = QStringLiteral("abc");
    store.remember(QStringLiteral("net-1"), QStringLiteral("#omarchy"), msgid, mapping);
    QCOMPARE(places(),
             QStringLiteral("[{\"follow\":false,\"msgid\":\"abc\",\"target\":\"#omarchy\"}]"));

    IrcScrollPlace content;
    content.followEnd = false;
    content.author = QStringLiteral("anna");
    content.body = QStringLiteral("hi <&>");
    content.kind = QStringLiteral("message");
    content.epochMs = 1700000000123;
    content.hasTime = true;
    store.remember(QStringLiteral("net-1"), QStringLiteral("#omarchy"), content, mapping);
    const QString wantContent =
        QStringLiteral("[{\"author\":\"anna\",\"body\":\"hi <&>\",\"follow\":false,"
                       "\"kind\":\"message\",\"ms\":\"1700000000123\",\"target\":\"#omarchy\"}]");
    QCOMPARE(places(), wantContent);

    IrcScrollPlace event;
    event.followEnd = false;
    event.kind = QStringLiteral("event");
    event.body = QStringLiteral("x");
    store.remember(QStringLiteral("net-1"), QStringLiteral("#lab"), event, mapping);
    QCOMPARE(places(),
             wantContent.left(wantContent.size() - 1)
                 + QStringLiteral(",{\"author\":\"\",\"body\":\"x\",\"follow\":false,"
                                  "\"kind\":\"event\",\"target\":\"#lab\"}]"));

    IrcScrollPlaceStore reloaded;
    const auto place = reloaded.place(QStringLiteral("net-1"), QStringLiteral("#OMARCHY"), mapping);
    QVERIFY(place);
    QVERIFY(!place->followEnd);
    QCOMPARE(place->body, QStringLiteral("hi <&>"));
    QVERIFY(place->hasTime);
    QCOMPARE(place->epochMs, qint64(1700000000123));

    QVERIFY(store.rekey(QStringLiteral("net-1"), QStringLiteral("#lab"),
                        QStringLiteral("#Lab"), mapping));
    QVERIFY(store.rekey(QStringLiteral("net-1"), QStringLiteral("#Lab"),
                        QStringLiteral("#omarchy"), mapping));
    const auto replaced = store.place(QStringLiteral("net-1"), QStringLiteral("#omarchy"), mapping);
    QVERIFY(replaced);
    QCOMPARE(replaced->body, QStringLiteral("x"));
    QVERIFY(!store.place(QStringLiteral("net-1"), QStringLiteral("#lab"), mapping));

    store.forget(QStringLiteral("net-1"));
    QVERIFY(!IrcScrollPlaceStore().place(QStringLiteral("net-1"), QStringLiteral("#omarchy"), mapping));
}

void ScrollPlaceTest::ephemeralRememberSkipsDisk()
{
    IrcController controller;
    controller.setEphemeral(true);
    IrcDemoServer demo;
    QVERIFY(demo.attach(controller, true));
    controller.rememberScrollPlace(true, -1);

    QSettings settings;
    QFile file(settings.fileName());
    if (file.exists()) {
        QVERIFY(file.open(QIODevice::ReadOnly | QIODevice::Text));
        QVERIFY(!QString::fromUtf8(file.readAll()).contains(QStringLiteral("scrollPlaces")));
    }
    QCOMPARE(controller.currentScrollPlace().value(QStringLiteral("known")).toBool(), true);
    QCOMPARE(controller.currentScrollPlace().value(QStringLiteral("follow")).toBool(), true);
}

void ScrollPlaceTest::demoPlaceSurvivesRestart()
{
    QString body;
    {
        IrcController controller;
        controller.setEphemeral(false);
        IrcDemoServer demo;
        QVERIFY(demo.attach(controller, true));
        auto *model = qobject_cast<MessageListModel *>(controller.messages());
        QVERIFY(model);
        int anchor = -1;
        int stores = 0;
        for (int row = 0; row < model->rowCount(); ++row) {
            if (model->storeIndexAt(row) < 0)
                continue;
            ++stores;
            if (stores == 4) {
                anchor = row;
                break;
            }
        }
        QVERIFY(anchor >= 0);
        body = model->field(anchor, QStringLiteral("body"));
        QVERIFY(!body.isEmpty());
        controller.rememberScrollPlace(false, anchor);
        const QVariantMap place = controller.currentScrollPlace();
        QCOMPARE(place.value(QStringLiteral("known")).toBool(), true);
        QCOMPARE(place.value(QStringLiteral("follow")).toBool(), false);
        QCOMPARE(place.value(QStringLiteral("row")).toInt(), anchor);
    }

    IrcController again;
    again.setEphemeral(false);
    IrcDemoServer demo;
    QVERIFY(demo.attach(again, true));
    const QVariantMap restored = again.currentScrollPlace();
    QCOMPARE(restored.value(QStringLiteral("known")).toBool(), true);
    QCOMPARE(restored.value(QStringLiteral("follow")).toBool(), false);
    auto *model = qobject_cast<MessageListModel *>(again.messages());
    QVERIFY(model);
    const int row = restored.value(QStringLiteral("row")).toInt();
    QVERIFY(row >= 0);
    QCOMPARE(model->field(row, QStringLiteral("body")), body);
}

int runScrollPlaceTests(int argc, char **argv)
{
    ScrollPlaceTest test;
    return QTest::qExec(&test, argc, argv);
}

#include "tst_scrollplace.moc"
