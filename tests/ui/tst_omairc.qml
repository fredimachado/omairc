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
    }

    ListModel {
        id: liveConsoleLines
        ListElement {
            time: "12:00:01"
            label: "NOTICE"
            text: "*** Looking up your hostname..."
            source: "server"
            severity: "info"
        }
        ListElement {
            time: "12:00:02"
            label: "001"
            text: "Welcome to Libera"
            source: "server"
            severity: "info"
        }
    }

    QtObject {
        id: liveConsole

        property var lines: liveConsoleLines
        property bool open: false
        property int alerts: 0
        property string networkId: "libera"

        function submit(input) {
            return input.length > 0;
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

        property string currentNick: "live-nick"
        property string selectedTarget: "#omarchy"
        property string selectedNetworkId: "libera"
        property string topic: "A cozy corner for Omarchy users and builders."
        property bool isChannel: true
        property int peopleCount: 1
        property string connectionStatus: "Connected"
        property string lastError: ""
        property bool hasAwayPresence: true
        property bool hasMemberStatus: true
        property var conversations: liveConversations
        property var messages: liveMessages
        property var members: liveMembers
        property var statusConsole: liveConsole

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

    QtObject {
        id: emptyNickIrc

        property string selectedTarget: "#omarchy"
        property string selectedNetworkId: "libera"
        property string topic: ""
        property bool isChannel: true
        property int peopleCount: 1
        property string connectionStatus: "Connected"
        property string lastError: ""
        property string currentNick: ""
        property bool hasAwayPresence: true
        property bool hasMemberStatus: true
        property var conversations: liveConversations
        property var messages: liveMessages
        property var members: liveMembers
        property var statusConsole: liveConsole

        function selectConversation() {
        }

        function openDirectMessage() {
        }

        function sendMessage() {
            return false;
        }
    }

    ListModel {
        id: gatedMembers

        ListElement { nick: "anna"; label: "anna"; status: "writing docs"; away: true }
    }

    QtObject {
        id: gatedIrc

        property string currentNick: "live-nick"
        property string selectedTarget: "#omarchy"
        property string selectedNetworkId: "libera"
        property string topic: "A cozy corner for Omarchy users and builders."
        property bool isChannel: true
        property int peopleCount: 1
        property string connectionStatus: "Connected"
        property string lastError: ""
        property bool hasAwayPresence: false
        property bool hasMemberStatus: false
        property var conversations: liveConversations
        property var messages: liveMessages
        property var members: gatedMembers
        property var statusConsole: liveConsole

        function selectConversation() {
        }

        function openDirectMessage() {
        }

        function sendMessage() {
            return false;
        }
    }

    QtObject {
        id: namedConnection

        property string host: "irc.libera.chat"
        property int port: 6697
        property bool tlsEnabled: true
        property string nick: "sheet-nick"
        property string username: ""
        property string realname: ""
        property string autojoin: "#omarchy"
        property bool passwordSet: false
        property string problem: ""
        property bool dirty: false
        property string displayName: "irc.libera.chat"
        property bool setupRequired: false
        property bool focusPassword: false

        function setPassword() {
        }

        function apply() {
            return false;
        }

        function discard() {
        }
    }

    Component {
        id: liveWindowComponent

        Omairc.OmaircWindow {
            backend: fakeBackend
            irc: liveIrc
        }
    }

    Component {
        id: gatedWindowComponent

        Omairc.OmaircWindow {
            backend: fakeBackend
            irc: gatedIrc
        }
    }

    Component {
        id: fallbackWindowComponent

        Omairc.OmaircWindow {
            backend: fakeBackend
            irc: emptyNickIrc
            connection: namedConnection
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

    function test_toggleStatusWithShortcut() {
        compare(appWindow.consoleVisible, false);

        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);

        tryCompare(appWindow, "consoleVisible", true);
        compare(appWindow.title, "Status");
        var list = item("consoleList");
        verify(list.visible);
        verify(!item("peopleButton").visible);

        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);

        tryCompare(appWindow, "consoleVisible", false);
    }

    function test_toggleStatusWithShortcutOnLiveWindow() {
        liveConsole.open = false;
        if (appWindow) {
            appWindow.close();
            appWindow = null;
        }
        var window = createTemporaryObject(liveWindowComponent, null);
        verify(window !== null, "The live window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        compare(liveConsole.open, false);

        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);

        tryCompare(liveConsole, "open", true);

        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);

        tryCompare(liveConsole, "open", false);
        window.close();
        liveConsole.open = false;
    }

    function test_openConnectSheetWithShortcut() {
        var window = createTemporaryObject(fallbackWindowComponent, null);
        verify(window !== null, "The fallback window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        var sheet = findChild(window, "connectionSheet");
        verify(sheet !== null, "Could not find connectionSheet");
        compare(sheet.visible, false);

        keyClick(Qt.Key_Comma, Qt.ControlModifier);

        tryCompare(sheet, "visible", true);
        window.close();
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

    function liveDirectNames(window) {
        var dms = findChild(window, "directConversationRepeater");
        verify(dms !== null, "Live direct-message repeater should be named");
        var names = [];
        var index = 0;
        for (index = 0; index < dms.count; ++index) {
            var directRow = dms.itemAt(index);
            if (directRow && directRow.visible)
                names.push(directRow.conversationName);
        }
        return names;
    }

    function test_liveSidebarClickSwitchesChannel() {
        var window = createTemporaryObject(liveWindowComponent, null);
        verify(window !== null, "The live window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        compare(window.currentConversation, "#omarchy");
        var channels = findChild(window, "channelConversationRepeater");
        verify(channels !== null, "Live channel repeater should be named");

        var channel = null;
        var index = 0;
        for (index = 0; index < channels.count; ++index) {
            var channelRow = channels.itemAt(index);
            if (channelRow && channelRow.visible
                    && channelRow.conversationName === "#omarchy")
                channel = channelRow;
        }

        verify(channel !== null, "Live #omarchy row should render under Channels");
        compare(channel.networkId, "libera");
        compare(channel.direct, false);
        verify(liveDirectNames(window).indexOf("#omarchy") === -1,
               "#omarchy must stay out of Direct Messages");
        verify(liveDirectNames(window).indexOf("AUTH") === -1,
               "AUTH must not appear under Direct Messages");

        mouseClick(channel);

        tryCompare(window, "currentConversation", "#omarchy");
        compare(liveIrc.selectedNetworkId, "libera");
        compare(window.currentConversationIsChannel, true);
        window.close();
    }

    function test_memberPresenceChromeFollowsCapabilities() {
        liveConsole.open = false;
        gatedIrc.hasAwayPresence = false;
        gatedIrc.hasMemberStatus = false;
        var window = createTemporaryObject(gatedWindowComponent, null);
        verify(window !== null, "The gated window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        verify(findChild(window, "membersPanel").visible);
        var members = findChild(window, "membersList");
        verify(members !== null, "Could not find membersList");
        var member = members.itemAtIndex(0);
        verify(member !== null, "The anna member delegate should be rendered");
        var dot = findChild(member, "presence-dot-anna");
        var subtitle = findChild(member, "member-status-anna");
        verify(dot !== null, "The presence dot should be rendered");
        verify(subtitle !== null, "The status subtitle should be rendered");

        compare(dot.visible, false);
        compare(subtitle.visible, false);
        compare(member.away, false);
        compare(member.Accessible.description, "");

        gatedIrc.hasAwayPresence = true;
        gatedIrc.hasMemberStatus = true;

        tryCompare(dot, "visible", true);
        tryCompare(subtitle, "visible", true);
        compare(subtitle.text, "writing docs");
        compare(member.away, true);
        compare(member.Accessible.description, "writing docs");
        window.close();
    }

    function test_openStatusFromNetworkHeaderKeepsAuthOutOfDirects() {
        liveConsole.open = false;
        var window = createTemporaryObject(liveWindowComponent, null);
        verify(window !== null, "The live window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        compare(window.consoleVisible, false);
        verify(liveDirectNames(window).indexOf("AUTH") === -1,
               "AUTH must not appear under Direct Messages");

        var header = findChild(window, "networkHeaderButton");
        verify(header !== null, "Could not find networkHeaderButton");
        mouseClick(header);

        tryCompare(window, "consoleVisible", true);
        compare(window.title, "Status");
        var list = findChild(window, "consoleList");
        verify(list !== null, "Could not find consoleList");
        verify(list.visible);
        compare(list.model.count, 2);
        compare(list.model.get(0).text, "*** Looking up your hostname...");
        verify(liveDirectNames(window).indexOf("AUTH") === -1,
               "Opening Status must not create an AUTH direct message");
        verify(!findChild(window, "peopleButton").visible);
        try {
            grabImage(window.contentItem).save(artifactDirectory + "status-console.png");
        } catch (error) {
            fail("Failed to save screenshot 'status-console': " + error);
        }
        window.close();
        liveConsole.open = false;
    }

    function test_mockStatusOpensFromNetworkHeader() {
        mouseClick(item("networkHeaderButton"));
        tryCompare(appWindow, "consoleVisible", true);
        compare(appWindow.title, "Status");
        var list = item("consoleList");
        verify(list.visible);
        compare(list.model.get(0).text, "*** Looking up your hostname...");
        verify(!item("peopleButton").visible);
    }

    function test_mockIdentityFooterShowsFredAndDropsNotice() {
        compare(item("selfNickLabel").text, "fred");
        compare(findChild(appWindow, "mockNotice"), null);
    }

    function test_liveIdentityFooterShowsCurrentNick() {
        var window = createTemporaryObject(liveWindowComponent, null);
        verify(window !== null, "The live window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        compare(findChild(window, "selfNickLabel").text, "live-nick");
        window.close();
    }

    function test_identityFooterFallsBackToConnectionNick() {
        var window = createTemporaryObject(fallbackWindowComponent, null);
        verify(window !== null, "The fallback window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        compare(findChild(window, "selfNickLabel").text, "sheet-nick");
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
