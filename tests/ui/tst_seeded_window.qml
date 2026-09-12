import QtQuick
import QtTest
import Omairc.Test 1.0
import "../../src" as Omairc

TestCase {
    id: testCase

    name: "SeededOmaircWindow"
    when: windowShown

    property var appWindow
    readonly property string artifactDirectory: {
        var url = Qt.resolvedUrl("../../test-artifacts/").toString();
        return decodeURIComponent(url.substring("file://".length));
    }

    SeededIrcFixture {
        id: world
    }

    Component {
        id: seededWindowComponent

        Omairc.OmaircWindow {
            backend: world.backend
            irc: world.irc
            connection: world.connection
            slashCommands: world.slash
        }
    }

    function field(model, row, name) {
        return model.field(row, name);
    }

    function seedBodyVisible(model, body) {
        var count = model.rowCount();
        for (var row = 0; row < count; ++row) {
            if (field(model, row, "body") === body)
                return true;
        }
        return false;
    }

    function cleanup() {
        if (appWindow)
            appWindow.close();
        appWindow = null;
    }

    function test_seededWindowShowsOmarchyWorld() {
        verify(world.open(), world.lastError);
        verify(world.backend);
        verify(world.irc);
        verify(world.connection);
        verify(world.slash);

        appWindow = createTemporaryObject(seededWindowComponent, testCase);
        verify(appWindow !== null, "production OmaircWindow should load with the seeded controller");
        appWindow.suppressDesktopNotification = true;
        tryCompare(appWindow, "visible", true);
        waitForRendering(appWindow.contentItem);

        tryVerify(function() {
            return appWindow.currentConversation === "#omarchy"
                && !appWindow.consoleVisible;
        });

        compare(appWindow.currentTopic,
                "A cozy corner for Omarchy users and builders.");
        compare(appWindow.currentPeopleCount, 12);
        compare(appWindow.sidebarConversationRows().length > 0, true);
        verify(findChild(appWindow, "liveNetworkRepeater").count >= 2);

        var list = findChild(appWindow, "messageList");
        verify(list);
        verify(list.model);
        verify(typeof list.model.field === "function");
        verify(seedBodyVisible(list.model,
                              "Keep the member list optional and I am sold."));

        var image = grabImage(appWindow.contentItem);
        image.save(artifactDirectory + "seeded-qml-omarchy.png");
    }
}
