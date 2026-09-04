import QtQuick
import QtQuick.Window
import QtTest
import "../../src" as Omairc

TestCase {
    id: testCase

    name: "OmaircUi"
    when: windowShown

    property var appWindow
    readonly property string artifactDirectory: {
        var url = Qt.resolvedUrl("../../test-artifacts/").toString();
        return decodeURIComponent(url.substring("file://".length));
    }

    QtObject {
        id: fakeBackend

        property bool darkMode: true
        property real textScale: 1.0
        property string themeBackground: "#101010"
        property string themeForeground: "#eeeeee"
        property string themeAccent: "#5584aa"
        property string themeSelection: "#186a9a"

        function windowGeometry() {
            return { valid: false };
        }

        function saveWindowGeometry(x, y, width, height, maximized) {
        }
    }

    QtObject {
        id: fakeConnection

        property string host: "irc.libera.chat"
        property int port: 6697
        property bool tlsEnabled: true
        property string nick: ""
        property string username: ""
        property string realname: ""
        property string autojoin: "#omarchy"
        property bool passwordSet: false
        property string problem: "Nick is required"
        property bool dirty: true
        property string displayName: "irc.libera.chat"
        property bool setupRequired: true
        property bool focusPassword: false

        function setPassword(password) {
        }

        function apply() {
            return false;
        }

        function discard() {
        }
    }

    Component {
        id: windowComponent

        Omairc.OmaircWindow {
            backend: fakeBackend
        }
    }

    Component {
        id: setupWindowComponent

        Omairc.OmaircWindow {
            backend: fakeBackend
            connection: fakeConnection
        }
    }

    ListModel {
        id: liveConversations
        ListElement {
            conversation: "#omarchy"
            unread: 0
            mention: false
            direct: false
            networkId: "libera"
        }
        ListElement {
            conversation: "AUTH"
            unread: 1
            mention: false
            direct: true
            networkId: "libera"
        }
    }

    ListModel {
        id: liveMessages
    }

    ListModel {
        id: liveMembers
    }

    QtObject {
        id: liveIrc

        property string selectedTarget: "AUTH"
        property string selectedNetworkId: "libera"
        property string topic: "Ident notices"
        property bool isChannel: false
        property int peopleCount: 0
        property string connectionStatus: "Connected"
        property string lastError: ""
        property var conversations: liveConversations
        property var messages: liveMessages
        property var members: liveMembers

        function selectConversation(networkId, name) {
            if (!networkId || !name)
                return;
            selectedNetworkId = networkId;
            selectedTarget = name;
            isChannel = name.charAt(0) === "#";
            topic = isChannel ? "" : "Direct message with " + name;
            peopleCount = isChannel ? 1 : 0;
        }

        function openDirectMessage(nick) {
            selectConversation(selectedNetworkId, nick);
        }

        function sendMessage(text) {
            return false;
        }
    }

    Component {
        id: liveWindowComponent

        Omairc.OmaircWindow {
            backend: fakeBackend
            irc: liveIrc
        }
    }

    function init() {
        appWindow = createTemporaryObject(windowComponent, null);
        verify(appWindow !== null, "The production Omairc window should load");
        tryCompare(appWindow, "visible", true);
        waitForRendering(appWindow.contentItem);
    }

    function cleanup() {
        if (appWindow)
            appWindow.close();
        appWindow = null;
    }

    function item(objectName) {
        var result = findChild(appWindow, objectName);
        verify(result !== null, "Could not find " + objectName);
        return result;
    }

    function saveScreenshot(name) {
        var image = grabImage(appWindow.contentItem);
        try {
            image.save(artifactDirectory + name + ".png");
        } catch (error) {
            fail("Failed to save screenshot '" + name + "': " + error);
        }
    }

    function typeText(text) {
        for (var index = 0; index < text.length; ++index) {
            if (text.charAt(index) === " ")
                keyClick(Qt.Key_Space);
            else
                keyClick(text.charAt(index));
        }
    }

    function test_switchChannel() {
        mouseClick(item("conversation-#desktop"));

        tryCompare(appWindow, "currentConversation", "#desktop");
        compare(appWindow.currentTopic,
                "Desktops should feel personal, fast, and calm.");
        compare(appWindow.currentPeopleCount, 8);
        compare(item("messageList").Accessible.name, "Messages in #desktop");
        saveScreenshot("switch-channel");
    }

    function test_sendMessageWithKeyboard() {
        var composer = item("messageComposer");
        var messages = item("messageList");
        var previousCount = messages.model.count;

        mouseClick(composer);
        verify(composer.activeFocus);
        typeText("Hello from the UI test");
        compare(composer.text, "Hello from the UI test");
        keyClick(Qt.Key_Return);

        tryCompare(messages.model, "count", previousCount + 1);
        compare(messages.model.get(previousCount).author, "fred");
        compare(messages.model.get(previousCount).body, "Hello from the UI test");
        compare(composer.text, "");
        saveScreenshot("send-message");
    }

    function test_toggleMembersWithShortcut() {
        var panel = item("membersPanel");
        verify(panel.visible);

        keyClick(Qt.Key_M, Qt.ControlModifier | Qt.ShiftModifier);

        tryCompare(panel, "visible", false);
        compare(item("peopleButton").Accessible.name, "Show members");
        saveScreenshot("toggle-members");
    }

    function test_openDirectMessageFromMember() {
        var members = item("membersList");
        var directConversations = item("directConversationRepeater");
        var previousCount = directConversations.count;
        members.positionViewAtIndex(2, ListView.Contain);
        wait(0);
        var mira = members.itemAtIndex(2);
        verify(mira !== null, "The mira member delegate should be rendered");
        mouseClick(mira);

        tryCompare(appWindow, "currentConversation", "mira");
        compare(appWindow.currentTopic, "Direct message with mira");
        compare(directConversations.count, previousCount + 1);
        verify(!item("membersPanel").visible);
        saveScreenshot("open-direct-message");
    }

    function test_connectionSheetOpensWhenSetupRequired() {
        var window = createTemporaryObject(setupWindowComponent, null);
        verify(window !== null, "The setup window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        var sheet = findChild(window, "connectionSheet");
        verify(sheet !== null, "Could not find connectionSheet");
        verify(sheet.visible);
        compare(findChild(window, "connectionHost").text, "irc.libera.chat");
        compare(findChild(window, "connectionNick").text, "");
        compare(findChild(window, "connectionAutojoin").text, "#omarchy");
        compare(findChild(window, "connectionProblem").text, "Nick is required");
        try {
            grabImage(window.contentItem).save(artifactDirectory + "connection-sheet.png");
        } catch (error) {
            fail("Failed to save screenshot 'connection-sheet': " + error);
        }
        window.close();
    }

    function test_liveSidebarClickSwitchesFromAuthToChannel() {
        var window = createTemporaryObject(liveWindowComponent, null);
        verify(window !== null, "The live window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        compare(window.currentConversation, "AUTH");
        var channels = findChild(window, "channelConversationRepeater");
        verify(channels !== null, "Live channel repeater should be named");
        var dms = findChild(window, "directConversationRepeater");
        verify(dms !== null, "Live direct-message repeater should be named");

        var channel = null;
        var channelInDirects = false;
        var authInDirects = false;
        var index = 0;
        for (index = 0; index < channels.count; ++index) {
            var channelRow = channels.itemAt(index);
            if (channelRow && channelRow.visible
                    && channelRow.conversationName === "#omarchy")
                channel = channelRow;
        }
        for (index = 0; index < dms.count; ++index) {
            var directRow = dms.itemAt(index);
            if (!directRow || !directRow.visible)
                continue;
            if (directRow.conversationName === "#omarchy")
                channelInDirects = true;
            if (directRow.conversationName === "AUTH")
                authInDirects = true;
        }

        verify(channel !== null, "Live #omarchy row should render under Channels");
        compare(channel.networkId, "libera");
        compare(channel.direct, false);
        verify(!channelInDirects, "#omarchy must stay out of Direct Messages");
        verify(authInDirects, "AUTH belongs under Direct Messages");

        mouseClick(channel);

        tryCompare(window, "currentConversation", "#omarchy");
        compare(liveIrc.selectedNetworkId, "libera");
        compare(window.currentConversationIsChannel, true);
        window.close();
    }

    function test_openDirectMessageClearsModelUnreadState() {
        var directConversations = item("directConversationRepeater");
        var anna = directConversations.itemAt(0);
        verify(anna !== null, "The anna direct-message delegate should be rendered");
        compare(directConversations.model.get(0).directUnread, 1);
        compare(directConversations.model.get(0).directMention, true);

        mouseClick(anna);

        tryCompare(appWindow, "currentConversation", "anna");
        compare(directConversations.model.get(0).directUnread, 0);
        compare(directConversations.model.get(0).directMention, false);
        compare(anna.unread, 0);
        compare(anna.mention, false);
    }
}
