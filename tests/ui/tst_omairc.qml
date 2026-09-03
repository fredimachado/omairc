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

    Component {
        id: windowComponent

        Omairc.OmaircWindow {
            backend: fakeBackend
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
