import QtQuick
import QtTest
import Omairc.Test 1.0
import "../../src" as Omairc

TestCase {
    id: testCase
    name: "SeededProfiles"
    when: windowShown

    property var world
    property var appWindow

    Component { id: worldComponent; SeededIrcFixture {} }
    Component {
        id: windowComponent
        Omairc.OmaircWindow {}
    }

    function init() {
        world = createTemporaryObject(worldComponent, testCase);
        verify(world !== null);
        verify(world.open(), world.lastError);
        appWindow = createTemporaryObject(windowComponent, testCase, {
            backend: world.backend,
            irc: world.irc,
            connection: world.connection,
            slashCommands: world.slash,
            avatarStore: appAvatarStore
        });
        verify(appWindow !== null);
        tryCompare(appWindow, "visible", true);
        waitForRendering(appWindow.contentItem);
        appWindow.requestActivate();
        tryCompare(appWindow, "active", true);
    }

    function cleanup() {
        if (appWindow) {
            appWindow.close();
            appWindow.destroy();
            wait(0);
        }
        appWindow = null;
        if (world)
            world.destroy();
        world = null;
    }

    function networkRow(networkId) {
        return findChild(appWindow, "networkChoice-" + networkId);
    }

    function test_selectProfileKeepsConversationNavigationProductionBacked() {
        compare(appWindow.currentConversation, "#omarchy");
        compare(appWindow.currentNetworkId, world.omarchyNetworkId);

        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        tryCompare(findChild(appWindow, "connectionSheet"), "visible", true);
        world.connection.select(world.oftcNetworkId);
        compare(world.connection.selectedNetworkId, world.oftcNetworkId);
        compare(findChild(appWindow, "connectionHost").text, "irc.example");

        keyClick(Qt.Key_Escape);
        tryCompare(findChild(appWindow, "connectionSheet"), "visible", false);
        keyClick(Qt.Key_Down, Qt.AltModifier);
        tryCompare(appWindow, "currentConversation", "#ricing");
        compare(appWindow.currentNetworkId, world.omarchyNetworkId);
    }

    function test_addAndDiscardRestoresSelectedProfile() {
        var connection = world.connection;
        var originalId = connection.selectedNetworkId;
        var originalCount = connection.networks.rowCount();
        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        tryCompare(findChild(appWindow, "connectionSheet"), "visible", true);
        mouseClick(findChild(appWindow, "connectionAddNetwork"));
        compare(connection.networks.rowCount(), originalCount + 1);
        verify(connection.selectedNetworkId !== originalId);
        compare(findChild(appWindow, "connectionHost").text, "");
        mouseClick(findChild(appWindow, "connectionDiscard"));
        compare(connection.networks.rowCount(), originalCount);
        compare(connection.selectedNetworkId, originalId);
        compare(findChild(appWindow, "connectionHost").text, "irc.example");
    }

    function test_removeProfileUsesProductionRoster() {
        var connection = world.connection;
        var originalCount = connection.networks.rowCount();
        var removedId = connection.selectedNetworkId;
        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        tryCompare(findChild(appWindow, "connectionSheet"), "visible", true);
        verify(connection.removeSelected());
        tryVerify(function() {
            return connection.networks.rowCount() === originalCount - 1;
        });
        verify(connection.selectedNetworkId !== removedId);
        compare(findChild(appWindow, "connectionHost").text, "irc.example");
    }
}
