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

        function notifyDesktop(summary, body) {
        }
    }

    ListModel {
        id: setupNetworks
        ListElement {
            networkId: "setup-id"
            displayName: "irc.libera.chat"
            stored: false
            selected: true
        }
    }

    ListModel {
        id: namedNetworks
        ListElement {
            networkId: "libera"
            displayName: "irc.libera.chat"
            stored: true
            selected: true
        }
    }

    QtObject {
        id: fakeConnection

        property string host: "irc.libera.chat"
        property int port: 6697
        property bool tlsEnabled: true
        property bool connectOnStartup: false
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
        property var networks: setupNetworks
        property string selectedNetworkId: "setup-id"
        property bool canAdd: false
        property bool canRemove: false

        function setPassword(password) {
        }

        function apply() {
            return false;
        }

        function discard() {
        }

        function select(networkId) {
            selectedNetworkId = networkId;
        }

        function add() {
            return false;
        }

        function removeSelected() {
            return false;
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
            conversationId: "libera\n#omarchy"
            conversationName: "#omarchy"
            typing: false
        }
        ListElement {
            conversation: "anna"
            unread: 0
            mention: false
            direct: true
            networkId: "libera"
            conversationId: "libera\nanna"
            conversationName: "anna"
            typing: true
        }
    }

    ListModel {
        id: liveConsoleLines
        ListElement {
            time: "12:00:01"
            label: "NOTICE"
            text: "-AUTH- *** Looking up your hostname..."
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

        function alertsFor(networkId) {
            return alerts;
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
        property bool selfAway: false
        property string selectedTarget: "#omarchy"
        property string selectedNetworkId: "libera"
        property string topic: "A cozy corner for Omarchy users and builders."
        property bool isChannel: true
        property int peopleCount: 1
        property string connectionStatus: "Connected"
        property string lastError: ""
        property int conversationEpoch: 0
        property var unreadSink: [0, false]
        property bool hasAwayPresence: true
        property bool hasMemberStatus: true
        property bool hasTyping: false
        property var typingNicks: []
        property var conversations: liveConversations
        property var messages: liveMessages
        property var members: liveMembers
        property var statusConsole: liveConsole
        property string selectedConversationId: "libera\n#omarchy"
        property string focusedNetworkId: liveConsole.open ? liveConsole.networkId : selectedNetworkId

        function connectionStatusFor(networkId) {
            return connectionStatus;
        }

        function lastErrorFor(networkId) {
            return lastError;
        }

        function unreadCountFor(networkId) {
            return unreadSink[0];
        }

        function mentionFor(networkId) {
            return unreadSink[1];
        }

        function openStatus(networkId) {
            liveConsole.networkId = networkId;
            liveConsole.open = true;
        }

        function selectConversationById(conversationId) {
            var sep = conversationId.indexOf("\n");
            if (sep <= 0)
                return;
            selectConversation(conversationId.substring(0, sep),
                               conversationId.substring(sep + 1));
        }

        function selectConversation(networkId, name) {
            if (!networkId || !name)
                return;
            selectedNetworkId = networkId;
            selectedTarget = name;
            selectedConversationId = networkId + "\n" + name;
            isChannel = name.charAt(0) === "#";
            topic = isChannel ? "" : "Direct message with " + name;
            peopleCount = isChannel ? 1 : 0;
            liveConsole.open = false;
        }

        function openDirectMessage(nick) {
            selectConversation(selectedNetworkId, nick);
        }

        function closeDirectMessage() {
        }

        function sendMessage(text) {
            return false;
        }

        function nickIsTyping(nick) {
            return false;
        }

        function notifyComposerText(text) {
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
        property int conversationEpoch: 0
        property string currentNick: ""
        property bool selfAway: false
        property bool hasAwayPresence: true
        property bool hasMemberStatus: true
        property bool hasTyping: false
        property var typingNicks: []
        property var conversations: liveConversations
        property var messages: liveMessages
        property var members: liveMembers
        property var statusConsole: liveConsole
        property string selectedConversationId: selectedNetworkId + "\n" + selectedTarget
        property string focusedNetworkId: selectedNetworkId

        function connectionStatusFor(networkId) {
            return connectionStatus;
        }

        function lastErrorFor(networkId) {
            return lastError;
        }

        function unreadCountFor(networkId) {
            return 0;
        }

        function mentionFor(networkId) {
            return false;
        }

        function openStatus(networkId) {
            liveConsole.networkId = networkId;
            liveConsole.open = true;
        }

        function selectConversationById(conversationId) {
        }

        function selectConversation() {
        }

        function openDirectMessage() {
        }

        function closeDirectMessage() {
        }

        function sendMessage() {
            return false;
        }

        function nickIsTyping() {
            return false;
        }

        function notifyComposerText() {
        }
    }

    ListModel {
        id: gatedMembers

        ListElement { nick: "anna"; label: "anna"; status: "writing docs"; away: true }
    }

    ListModel {
        id: prefixedMembers

        ListElement { nick: "mira"; label: "@mira"; status: ""; away: false }
        ListElement { nick: "sol"; label: "+sol"; status: ""; away: false }
        ListElement { nick: "anna"; label: "anna"; status: ""; away: false }
    }

    QtObject {
        id: gatedIrc

        property string currentNick: "live-nick"
        property bool selfAway: false
        property string selectedTarget: "#omarchy"
        property string selectedNetworkId: "libera"
        property string topic: "A cozy corner for Omarchy users and builders."
        property bool isChannel: true
        property int peopleCount: 1
        property string connectionStatus: "Connected"
        property string lastError: ""
        property int conversationEpoch: 0
        property bool hasAwayPresence: false
        property bool hasMemberStatus: false
        property bool hasTyping: false
        property var typingNicks: ["anna"]
        property var conversations: liveConversations
        property var messages: liveMessages
        property var members: gatedMembers
        property var statusConsole: liveConsole
        property string selectedConversationId: selectedNetworkId + "\n" + selectedTarget
        property string focusedNetworkId: liveConsole.open ? liveConsole.networkId : selectedNetworkId

        function connectionStatusFor(networkId) {
            return connectionStatus;
        }

        function lastErrorFor(networkId) {
            return lastError;
        }

        function unreadCountFor(networkId) {
            return 0;
        }

        function mentionFor(networkId) {
            return false;
        }

        function openStatus(networkId) {
            liveConsole.networkId = networkId;
            liveConsole.open = true;
        }

        function selectConversationById(conversationId) {
        }

        function selectConversation() {
        }

        function openDirectMessage() {
        }

        function closeDirectMessage() {
        }

        function sendMessage() {
            return false;
        }

        function nickIsTyping(nick) {
            return hasTyping && typingNicks.indexOf(nick) !== -1;
        }

        function notifyComposerText() {
        }
    }

    QtObject {
        id: prefixedIrc

        property string currentNick: "live-nick"
        property bool selfAway: false
        property string selectedTarget: "#omarchy"
        property string selectedNetworkId: "libera"
        property string topic: "A cozy corner for Omarchy users and builders."
        property bool isChannel: true
        property int peopleCount: 3
        property string connectionStatus: "Connected"
        property string lastError: ""
        property int conversationEpoch: 0
        property bool hasAwayPresence: true
        property bool hasMemberStatus: true
        property bool hasTyping: false
        property var typingNicks: []
        property var conversations: liveConversations
        property var messages: liveMessages
        property var members: prefixedMembers
        property var statusConsole: liveConsole
        property string selectedConversationId: selectedNetworkId + "\n" + selectedTarget
        property string focusedNetworkId: liveConsole.open ? liveConsole.networkId : selectedNetworkId

        function connectionStatusFor(networkId) {
            return connectionStatus;
        }

        function lastErrorFor(networkId) {
            return lastError;
        }

        function unreadCountFor(networkId) {
            return 0;
        }

        function mentionFor(networkId) {
            return false;
        }

        function openStatus(networkId) {
            liveConsole.networkId = networkId;
            liveConsole.open = true;
        }

        function selectConversationById(conversationId) {
        }

        function selectConversation() {
        }

        function openDirectMessage() {
        }

        function closeDirectMessage() {
        }

        function sendMessage() {
            return false;
        }

        function nickIsTyping(nick) {
            return false;
        }

        function notifyComposerText() {
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
        property var networks: namedNetworks
        property string selectedNetworkId: "libera"
        property bool canAdd: true
        property bool canRemove: true
        property int passwordSetCalls: 0
        property string lastPassword: ""

        signal selectedNetworkChanged()

        function setPassword(password) {
            passwordSetCalls += 1;
            lastPassword = password;
        }

        function apply() {
            return false;
        }

        function discard() {
        }

        function select(networkId) {
            if (dirty && networkId !== selectedNetworkId)
                return;
            if (selectedNetworkId === networkId) {
                for (var same = 0; same < namedNetworks.count; ++same)
                    namedNetworks.setProperty(same, "selected",
                        namedNetworks.get(same).networkId === networkId);
                return;
            }
            selectedNetworkId = networkId;
            for (var row = 0; row < namedNetworks.count; ++row)
                namedNetworks.setProperty(row, "selected",
                    namedNetworks.get(row).networkId === networkId);
            selectedNetworkChanged();
        }

        function add() {
            for (var row = 0; row < namedNetworks.count; ++row)
                namedNetworks.setProperty(row, "selected", false);
            namedNetworks.append({
                networkId: "new-id",
                displayName: "New network",
                stored: false,
                selected: true
            });
            selectedNetworkId = "new-id";
            displayName = "New network";
            selectedNetworkChanged();
            host = "";
            nick = "";
            return true;
        }

        function removeSelected() {
            for (var row = 0; row < namedNetworks.count; ++row) {
                if (namedNetworks.get(row).networkId === selectedNetworkId) {
                    namedNetworks.remove(row);
                    break;
                }
            }
            if (namedNetworks.count > 0) {
                selectedNetworkId = namedNetworks.get(0).networkId;
                namedNetworks.setProperty(0, "selected", true);
                displayName = namedNetworks.get(0).displayName;
            }
            return true;
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
        id: prefixedWindowComponent

        Omairc.OmaircWindow {
            backend: fakeBackend
            irc: prefixedIrc
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

    Component {
        id: liveNamedWindowComponent

        Omairc.OmaircWindow {
            backend: fakeBackend
            irc: liveIrc
            connection: namedConnection
        }
    }

    QtObject {
        id: slashFake

        property bool open: false
        property var matches: []
        property int selectedIndex: 0
        property string syncedText: ""

        function reset() {
            open = false;
            matches = [];
            selectedIndex = 0;
            syncedText = "";
        }

        function sync(text, statusConsole) {
            syncedText = text;
            var start = 0;
            while (start < text.length && " \t".indexOf(text.charAt(start)) !== -1)
                start += 1;
            var rest = text.substring(start);
            if (rest.length < 2 || rest.charAt(0) !== "/" || rest.charAt(1) === "/"
                    || rest.indexOf(" ") !== -1) {
                open = false;
                matches = [];
                selectedIndex = 0;
                return;
            }
            var needle = rest.substring(1).toLowerCase();
            if (needle.charAt(0) !== "j") {
                open = false;
                matches = [];
                selectedIndex = 0;
                return;
            }
            var keepSelection = open;
            open = true;
            matches = [
                { label: "/join", usage: "/join <channel> [key][, ...]" },
                { label: "/nick", usage: "/nick <nickname>" }
            ];
            if (!keepSelection)
                selectedIndex = 0;
            else if (selectedIndex < 0 || selectedIndex >= matches.length)
                selectedIndex = 0;
        }

        function routeKey(key, modifiers) {
            if (modifiers !== Qt.NoModifier && modifiers !== Qt.KeypadModifier)
                return { accepted: false, insertion: "" };
            if (!open)
                return { accepted: false, insertion: "" };
            if (key === Qt.Key_Escape) {
                dismiss();
                return { accepted: true, insertion: "" };
            }
            if (key === Qt.Key_Down) {
                selectedIndex = (selectedIndex + 1) % matches.length;
                return { accepted: true, insertion: "" };
            }
            if (key === Qt.Key_Up) {
                selectedIndex = (selectedIndex + matches.length - 1) % matches.length;
                return { accepted: true, insertion: "" };
            }
            if (key === Qt.Key_Tab || key === Qt.Key_Return || key === Qt.Key_Enter) {
                return { accepted: true, insertion: activate(selectedIndex) };
            }
            return { accepted: false, insertion: "" };
        }

        function activate(index) {
            if (!open || index < 0 || index >= matches.length)
                return "";
            selectedIndex = index;
            var start = 0;
            while (start < syncedText.length && " \t".indexOf(syncedText.charAt(start)) !== -1)
                start += 1;
            return syncedText.substring(0, start) + matches[index].label + " ";
        }

        function dismiss() {
            open = false;
            selectedIndex = 0;
        }
    }

    Component {
        id: slashWindowComponent

        Omairc.OmaircWindow {
            backend: fakeBackend
            slashCommands: slashFake
        }
    }

    function init() {
        appWindow = createTemporaryObject(windowComponent, null);
        verify(appWindow !== null, "The production Omairc window should load");
        tryCompare(appWindow, "visible", true);
        waitForRendering(appWindow.contentItem);
        appWindow.suppressExternalUrlOpen = true;
        appWindow.lastOpenedUrl = "";
        appWindow.suppressDesktopNotification = true;
        appWindow.lastNotification = null;
    }

    function cleanup() {
        if (appWindow)
            appWindow.close();
        appWindow = null;
        slashFake.reset();
        liveConsole.open = false;
        liveConsole.networkId = "libera";
    }

    function restoreNamedConnection() {
        namedNetworks.clear();
        namedNetworks.append({
            networkId: "libera",
            displayName: "irc.libera.chat",
            stored: true,
            selected: true
        });
        namedConnection.selectedNetworkId = "libera";
        namedConnection.displayName = "irc.libera.chat";
        namedConnection.host = "irc.libera.chat";
        namedConnection.nick = "sheet-nick";
        namedConnection.passwordSetCalls = 0;
        namedConnection.lastPassword = "";
        namedConnection.dirty = false;
    }

    function repeaterItemByName(repeater, objectName) {
        var index = 0;
        for (index = 0; index < repeater.count; ++index) {
            var row = repeater.itemAt(index);
            if (row && row.objectName === objectName)
                return row;
        }
        return null;
    }

    function item(objectName) {
        var result = findChild(appWindow, objectName);
        verify(result !== null, "Could not find " + objectName);
        return result;
    }

    function shortcutSheetTexts(window) {
        var sheet = window ? findChild(window, "shortcutsSheet") : item("shortcutsSheet");
        var texts = [];
        function walk(node) {
            if (!node)
                return;
            if (node.text !== undefined && String(node.text).length > 0)
                texts.push(String(node.text));
            var kids = node.children;
            if (kids) {
                for (var index = 0; index < kids.length; ++index)
                    walk(kids[index]);
            }
            if (node.contentItem)
                walk(node.contentItem);
        }
        walk(sheet);
        return texts;
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

    function visibleListChild(listName, childName) {
        var list = item(listName);
        var index = 0;
        for (index = 0; index < list.count; ++index) {
            var row = list.itemAtIndex(index);
            if (!row)
                continue;
            var child = findChild(row, childName);
            if (child && child.visible)
                return child;
        }
        fail("Could not find visible " + childName + " in " + listName);
        return null;
    }

    function renderedMessageRow(list, index) {
        list.positionViewAtIndex(index, ListView.Contain);
        waitForRendering(appWindow.contentItem);
        var row = list.itemAtIndex(index);
        verify(row !== null, "Message row " + index + " should be rendered");
        return row;
    }

    function assertMessageChrome(row, headerVisible, bodyText) {
        var avatar = findChild(row, "messageAvatar");
        var header = findChild(row, "messageHeader");
        var body = findChild(row, "messageBody");
        verify(avatar !== null, "Could not find messageAvatar");
        verify(header !== null, "Could not find messageHeader");
        verify(body !== null, "Could not find messageBody");
        compare(avatar.visible, headerVisible);
        compare(header.visible, headerVisible);
        compare(body.visible, true);
        compare(body.text, bodyText);
        return body;
    }

    function containsMirc(text) {
        return /[\u0002\u0003\u000f\u0016\u001d\u001f]/.test(text);
    }

    function formattedIrcBody() {
        return "\u0002bold\u000f / \u000304red";
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
        compare(appWindow.title, "Omarchy IRC Status");
        var list = item("consoleList");
        verify(list.visible);
        verify(!item("peopleButton").visible);

        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);

        tryCompare(appWindow, "consoleVisible", false);
    }

    function test_toggleStatusWithShortcutOnLiveWindow() {
        liveConsole.open = false;
        if (appWindow) {
            // Destroy rather than hide: a closed-but-alive window keeps its
            // Ctrl+` Qt.ApplicationShortcut registered, which would be
            // ambiguous with the live window's shortcut and fire neither.
            appWindow.destroy();
            appWindow = null;
            wait(0);
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

    function test_walkConversationsWithShortcut() {
        compare(appWindow.currentConversation, "#omarchy");

        keyClick(Qt.Key_Down, Qt.AltModifier);

        tryCompare(appWindow, "currentConversation", "#desktop");
        compare(appWindow.currentTopic,
                "Desktops should feel personal, fast, and calm.");
        compare(appWindow.currentPeopleCount, 8);
        compare(item("messageList").Accessible.name, "Messages in #desktop");
        tryCompare(item("messageComposer"), "activeFocus", true);

        keyClick(Qt.Key_Down, Qt.AltModifier);

        tryCompare(appWindow, "currentConversation", "#ricing");
        compare(appWindow.currentTopic,
                "Themes, type, wallpapers, and the tiny details.");
        compare(appWindow.currentPeopleCount, 10);
        compare(item("messageList").Accessible.name, "Messages in #ricing");
    }

    function test_walkConversationsWrapsToLast() {
        compare(appWindow.currentConversation, "#omarchy");

        keyClick(Qt.Key_Up, Qt.AltModifier);

        tryCompare(appWindow, "currentConversation", "rio");
        compare(appWindow.currentTopic, "Direct message with rio");
        compare(item("messageList").Accessible.name, "Messages in rio");
    }

    function test_walkConversationsFromChannelToDirect() {
        mouseClick(item("conversation-#help"));
        tryCompare(appWindow, "currentConversation", "#help");

        keyClick(Qt.Key_Down, Qt.AltModifier);

        tryCompare(appWindow, "currentConversation", "anna");
        compare(appWindow.currentTopic, "Direct message with anna");
        compare(item("messageList").Accessible.name, "Messages in anna");
    }

    function test_walkConversationsClosesStatus() {
        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);

        keyClick(Qt.Key_Down, Qt.AltModifier);

        tryCompare(appWindow, "consoleVisible", false);
        compare(appWindow.currentConversation, "#desktop");
        compare(item("messageList").Accessible.name, "Messages in #desktop");
    }

    function test_jumpToNextUnreadPrefersMention() {
        compare(appWindow.currentConversation, "#omarchy");

        keyClick(Qt.Key_A, Qt.AltModifier);

        tryCompare(appWindow, "currentConversation", "#ricing");
        compare(appWindow.currentTopic,
                "Themes, type, wallpapers, and the tiny details.");
        tryCompare(item("messageComposer"), "activeFocus", true);

        keyClick(Qt.Key_A, Qt.AltModifier);

        tryCompare(appWindow, "currentConversation", "anna");
        compare(appWindow.currentTopic, "Direct message with anna");
    }

    function test_jumpToNextUnreadFromNonMentionPrefersMention() {
        mouseClick(item("conversation-#desktop"));
        tryCompare(appWindow, "currentConversation", "#desktop");

        keyClick(Qt.Key_A, Qt.AltModifier);

        tryCompare(appWindow, "currentConversation", "#ricing");
    }

    function test_jumpToNextUnreadFallsBackToUnread() {
        mouseClick(item("conversation-#ricing"));
        tryCompare(appWindow, "currentConversation", "#ricing");

        var anna = item("directConversationRepeater").itemAt(0);
        verify(anna !== null, "The anna direct-message delegate should be rendered");
        mouseClick(anna);
        tryCompare(appWindow, "currentConversation", "anna");

        mouseClick(item("conversation-#omarchy"));
        tryCompare(appWindow, "currentConversation", "#omarchy");

        keyClick(Qt.Key_A, Qt.AltModifier);

        tryCompare(appWindow, "currentConversation", "#desktop");
    }

    function test_tabCompletesChannelNick() {
        var composer = item("messageComposer");
        mouseClick(composer);
        verify(composer.activeFocus);
        typeText("mi");
        keyClick(Qt.Key_Tab);

        compare(composer.text, "mira: ");
        verify(composer.activeFocus);
    }

    function test_tabCompletesLiveNickIgnoringPrefixLabel() {
        if (appWindow) {
            appWindow.close();
            appWindow = null;
        }
        var window = createTemporaryObject(prefixedWindowComponent, null);
        verify(window !== null, "The prefixed-member window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        var composer = findChild(window, "messageComposer");
        verify(composer !== null, "Could not find messageComposer");
        mouseClick(composer);
        verify(composer.activeFocus);
        typeText("mi");
        keyClick(Qt.Key_Tab);

        compare(composer.text, "mira: ");
        verify(composer.text.indexOf("@") === -1);
        verify(composer.activeFocus);
        window.close();
    }

    function test_tabCompletesNickAfterText() {
        var composer = item("messageComposer");
        mouseClick(composer);
        verify(composer.activeFocus);
        typeText("hello s");
        keyClick(Qt.Key_Tab);

        verify(composer.text !== "hello s");
        verify(composer.text.indexOf("hello ") === 0);
        verify(composer.text.charAt(composer.text.length - 1) === " ");
        var nick = composer.text.substring(6, composer.text.length - 1);
        verify(nick.length > 0);
        compare(nick.charAt(0).toLowerCase(), "s");
        verify(composer.activeFocus);
    }

    function test_composerHistoryRecallsSentLines() {
        var composer = item("messageComposer");
        mouseClick(composer);
        verify(composer.activeFocus);

        typeText("alpha");
        keyClick(Qt.Key_Return);
        compare(composer.text, "");

        typeText("beta");
        keyClick(Qt.Key_Return);
        compare(composer.text, "");

        keyClick(Qt.Key_Up);
        compare(composer.text, "beta");
        keyClick(Qt.Key_Up);
        compare(composer.text, "alpha");
        keyClick(Qt.Key_Down);
        compare(composer.text, "beta");
        keyClick(Qt.Key_Down);
        compare(composer.text, "");

        typeText("draft");
        compare(composer.text, "draft");
        keyClick(Qt.Key_Up);
        compare(composer.text, "beta");
        keyClick(Qt.Key_Down);
        compare(composer.text, "draft");
    }

    function test_composerDraftsStayWithConversation() {
        var composer = item("messageComposer");
        mouseClick(composer);
        verify(composer.activeFocus);
        compare(appWindow.currentConversation, "#omarchy");

        typeText("omarchy draft");
        compare(composer.text, "omarchy draft");

        mouseClick(item("conversation-#desktop"));
        tryCompare(appWindow, "currentConversation", "#desktop");
        compare(composer.text, "");

        typeText("desktop draft");
        compare(composer.text, "desktop draft");

        mouseClick(item("conversation-#omarchy"));
        tryCompare(appWindow, "currentConversation", "#omarchy");
        compare(composer.text, "omarchy draft");

        mouseClick(item("conversation-#desktop"));
        tryCompare(appWindow, "currentConversation", "#desktop");
        compare(composer.text, "desktop draft");

        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);
        compare(composer.text, "");

        typeText("status draft");
        compare(composer.text, "status draft");

        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", false);
        compare(appWindow.currentConversation, "#desktop");
        compare(composer.text, "desktop draft");

        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);
        compare(composer.text, "status draft");

        keyClick(Qt.Key_Escape);
        tryCompare(appWindow, "consoleVisible", false);
        compare(composer.text, "desktop draft");

        mouseClick(item("conversation-#omarchy"));
        tryCompare(appWindow, "currentConversation", "#omarchy");
        compare(composer.text, "omarchy draft");

        var messages = item("messageList");
        var previousCount = messages.model.count;
        keyClick(Qt.Key_Return);
        compare(composer.text, "");
        tryCompare(messages.model, "count", previousCount + 1);
        compare(messages.model.get(previousCount).author, "fred");
        compare(messages.model.get(previousCount).body, "omarchy draft");

        keyClick(Qt.Key_Up);
        compare(composer.text, "omarchy draft");
        keyClick(Qt.Key_Down);
        compare(composer.text, "");

        mouseClick(item("conversation-#desktop"));
        tryCompare(appWindow, "currentConversation", "#desktop");
        compare(composer.text, "desktop draft");
    }

    function visibleMatchIndex(list, needle) {
        var lower = needle.toLowerCase();
        var first = firstVisibleIndex(list);
        if (first < 0)
            return -1;
        var last = list.indexAt(Math.max(1, list.width / 2),
            list.contentY + Math.max(1, list.height - 1));
        if (last < 0)
            last = list.count - 1;
        if (last < first)
            last = first;
        var index = 0;
        for (index = first; index <= last; ++index) {
            var row = list.model.get(index);
            var hay = ((row && (row.body || row.text)) || "").toLowerCase();
            if (hay.indexOf(lower) >= 0)
                return index;
        }
        return -1;
    }

    function test_ctrlFFindsTextInConversation() {
        var composer = item("messageComposer");
        var list = item("messageList");
        fillMockMessagesUntilScrollable(list);
        appendMockMessages(list, 24, "find filler");
        waitForRendering(appWindow.contentItem);
        list.pinToEnd();
        waitForRendering(appWindow.contentItem);
        wait(0);
        var pinnedY = list.contentY;
        verify(pinnedY > 0);

        typeText("keep me");
        compare(composer.text, "keep me");
        keyClick(Qt.Key_F, Qt.ControlModifier);
        tryCompare(appWindow, "findActive", true);
        compare(composer.text, "keep me");
        verify(composer.activeFocus);

        typeText("omarchy");
        compare(composer.text, "omarchy");
        tryVerify(function() {
            return list.contentY < pinnedY;
        }, 1000, "Ctrl+F should jump the list to the match");
        var first = visibleMatchIndex(list, "omarchy");
        verify(first >= 0, "The first omarchy row should be in view");
        verify(list.model.get(first).body.toLowerCase().indexOf("omarchy") >= 0);

        var countBefore = list.model.count;
        keyClick(Qt.Key_Return);
        compare(list.model.count, countBefore);
        waitForRendering(appWindow.contentItem);
        wait(0);
        var second = visibleMatchIndex(list, "omarchy");
        verify(second > first, "Enter in find should go to the next match");
        compare(composer.text, "omarchy");

        var current = second;
        var hops = 0;
        while (current !== first) {
            keyClick(Qt.Key_F, Qt.ControlModifier);
            waitForRendering(appWindow.contentItem);
            wait(0);
            current = visibleMatchIndex(list, "omarchy");
            hops += 1;
            verify(hops < list.count, "Find should wrap back to the first match");
        }
        verify(hops >= 1);
        compare(list.model.count, countBefore);

        keyClick(Qt.Key_Escape);
        tryCompare(appWindow, "findActive", false);
        compare(composer.text, "keep me");
        verify(composer.activeFocus);

        keyClick(Qt.Key_Return);
        compare(composer.text, "");
        tryCompare(list.model, "count", countBefore + 1);
        compare(list.model.get(countBefore).body, "keep me");
    }

    function test_ctrlFFindsTextInStatus() {
        var composer = item("messageComposer");
        var list = item("consoleList");
        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);
        fillMockConsoleUntilScrollable(list);
        var pinnedY = list.contentY;
        verify(pinnedY > 0);

        mouseClick(composer);
        verify(composer.activeFocus);
        typeText("hostname");
        keyClick(Qt.Key_F, Qt.ControlModifier);
        tryCompare(appWindow, "findActive", true);
        waitForRendering(appWindow.contentItem);
        wait(0);

        verify(list.contentY < pinnedY, "Ctrl+F should jump Status to the match");
        var match = visibleMatchIndex(list, "hostname");
        verify(match >= 0, "The hostname Status line should be in view");
        verify(list.model.get(match).text.toLowerCase().indexOf("hostname") >= 0);
        compare(composer.text, "hostname");

        keyClick(Qt.Key_Escape);
        tryCompare(appWindow, "findActive", false);
        compare(composer.text, "hostname");
        compare(appWindow.consoleVisible, true);
    }

    function test_pageUpScrollsTranscript() {
        var composer = item("messageComposer");
        var list = item("messageList");
        mouseClick(composer);
        verify(composer.activeFocus);

        var index = 0;
        for (index = 0; index < 12; ++index) {
            typeText("scroll line " + index);
            keyClick(Qt.Key_Return);
        }
        waitForRendering(appWindow.contentItem);
        list.positionViewAtEnd();
        waitForRendering(appWindow.contentItem);

        verify(list.contentHeight > list.height);
        var before = list.contentY;
        verify(before > 0);

        keyClick(Qt.Key_PageUp);

        verify(list.contentY < before, "Page Up should scroll toward older lines");
        verify(composer.activeFocus);

        var afterUp = list.contentY;
        keyClick(Qt.Key_PageDown);
        verify(list.contentY > afterUp, "Page Down should scroll toward newer lines");
        verify(composer.activeFocus);
    }

    function transcriptPinned(list) {
        return list.count === 0
            || list.contentHeight <= list.height
            || list.atYEnd
            || list.contentY >= list.contentHeight - list.height - 2;
    }

    function firstVisibleIndex(list) {
        var x = Math.max(1, list.width / 2);
        var index = list.indexAt(x, list.contentY + 1);
        if (index >= 0)
            return index;
        return list.indexAt(x, list.contentY + 8);
    }

    function fillMockMessagesUntilScrollable(list) {
        var composer = item("messageComposer");
        mouseClick(composer);
        verify(composer.activeFocus);
        var index = 0;
        for (index = 0; index < 12; ++index) {
            typeText("scroll line " + index);
            keyClick(Qt.Key_Return);
        }
        waitForRendering(appWindow.contentItem);
        list.pinToEnd();
        waitForRendering(appWindow.contentItem);
        wait(0);
        verify(list.contentHeight > list.height);
        verify(transcriptPinned(list));
    }

    function appendMockMessages(list, count, bodyPrefix) {
        var start = list.model.count;
        var index = 0;
        for (index = 0; index < count; ++index) {
            list.model.append({
                author: "anna",
                time: "10:00",
                body: bodyPrefix + " " + index,
                kind: "message"
            });
        }
        tryCompare(list.model, "count", start + count);
    }

    function appendMockConsoleLines(list, count, textPrefix) {
        var start = list.model.count;
        var index = 0;
        for (index = 0; index < count; ++index) {
            list.model.append({
                time: "12:00:03",
                label: "PRIVMSG",
                text: textPrefix + " " + index,
                source: "server",
                severity: "info"
            });
        }
        tryCompare(list.model, "count", start + count);
    }

    function unseenIsInView(list, unseen) {
        if (firstVisibleIndex(list) === unseen)
            return true;
        var row = list.itemAtIndex(unseen);
        if (!row)
            return false;
        return row.y + row.height > list.contentY && row.y < list.contentY + list.height;
    }

    function fillMockConsoleUntilScrollable(list) {
        appendMockConsoleLines(list, 40, "console fill");
        waitForRendering(appWindow.contentItem);
        list.pinToEnd();
        waitForRendering(appWindow.contentItem);
        wait(0);
        verify(list.contentHeight > list.height);
        verify(transcriptPinned(list));
    }

    function test_followingAppendAndSendKeepTranscriptPinned() {
        var list = item("messageList");
        fillMockMessagesUntilScrollable(list);

        var previousCount = list.model.count;
        var previousY = list.contentY;
        list.model.append({
            author: "anna",
            time: "10:00",
            body: "incoming while following",
            kind: "message"
        });
        tryCompare(list.model, "count", previousCount + 1);
        waitForRendering(appWindow.contentItem);
        wait(0);
        verify(transcriptPinned(list), "Incoming rows should keep a following list at the end");
        verify(list.contentY >= previousY);
        compare(item("messageUnseenJump").visible, false);

        previousCount = list.model.count;
        previousY = list.contentY;
        typeText("sent while following");
        keyClick(Qt.Key_Return);
        tryCompare(list.model, "count", previousCount + 1);
        waitForRendering(appWindow.contentItem);
        wait(0);
        verify(transcriptPinned(list), "Sending should keep the list pinned to the end");
        verify(list.contentY >= previousY);
        compare(item("messageUnseenJump").visible, false);
    }

    function test_followingAppendStaysPinnedThroughLayout() {
        var list = item("messageList");
        fillMockMessagesUntilScrollable(list);

        var startY = list.contentY;
        var unpinned = 0;
        var rewind = 0;
        function sample() {
            if (!transcriptPinned(list))
                unpinned += 1;
            if (list.contentY + 2 < startY)
                rewind += 1;
        }
        list.contentYChanged.connect(sample);
        list.contentHeightChanged.connect(sample);
        list.originYChanged.connect(sample);

        var previousCount = list.model.count;
        list.model.append({
            author: "anna",
            time: "10:00",
            body: "incoming layout pin",
            kind: "message"
        });
        tryCompare(list.model, "count", previousCount + 1);
        waitForRendering(appWindow.contentItem);
        wait(0);
        waitForRendering(appWindow.contentItem);

        compare(unpinned, 0, "Following growth must stay pinned while the new row lays out");
        compare(rewind, 0, "Following growth must not jump toward older lines");
        verify(transcriptPinned(list));
        verify(list.contentY >= startY);
        compare(item("messageUnseenJump").visible, false);
    }

    function test_detachedArrivalKeepsViewportAndJumpsToFirstUnseen() {
        var list = item("messageList");
        fillMockMessagesUntilScrollable(list);

        keyClick(Qt.Key_PageUp);
        waitForRendering(appWindow.contentItem);
        tryCompare(list, "stick", 1);
        verify(!transcriptPinned(list));

        var frozenY = list.contentY;
        var previousCount = list.model.count;
        appendMockMessages(list, 1, "first unseen");
        waitForRendering(appWindow.contentItem);
        wait(0);

        fuzzyCompare(list.contentY, frozenY, 2);
        compare(list.firstUnseenIndex, previousCount);
        var jump = item("messageUnseenJump");
        tryCompare(jump, "visible", true);

        appendMockMessages(list, 24, "later unseen");
        waitForRendering(appWindow.contentItem);
        wait(0);
        fuzzyCompare(list.contentY, frozenY, 2);
        compare(list.firstUnseenIndex, previousCount);

        var unseen = list.firstUnseenIndex;
        mouseClick(jump);
        waitForRendering(appWindow.contentItem);
        wait(0);
        tryCompare(jump, "visible", false);
        compare(list.firstUnseenIndex, -1);
        tryVerify(function() {
            return unseenIsInView(list, unseen);
        });
        compare(firstVisibleIndex(list), unseen);
    }

    function test_consoleDetachedArrivalKeepsViewportAndJumpsToFirstUnseen() {
        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);

        var list = item("consoleList");
        verify(list.visible);
        fillMockConsoleUntilScrollable(list);

        keyClick(Qt.Key_PageUp);
        waitForRendering(appWindow.contentItem);
        tryCompare(list, "stick", 1);
        verify(!transcriptPinned(list));

        var frozenY = list.contentY;
        var previousCount = list.model.count;
        appendMockConsoleLines(list, 1, "first unseen console");
        waitForRendering(appWindow.contentItem);
        wait(0);

        fuzzyCompare(list.contentY, frozenY, 2);
        compare(list.firstUnseenIndex, previousCount);
        var jump = item("consoleUnseenJump");
        tryCompare(jump, "visible", true);

        appendMockConsoleLines(list, 40, "later unseen console");
        waitForRendering(appWindow.contentItem);
        wait(0);
        fuzzyCompare(list.contentY, frozenY, 2);
        compare(list.firstUnseenIndex, previousCount);

        var unseen = list.firstUnseenIndex;
        mouseClick(jump);
        waitForRendering(appWindow.contentItem);
        wait(0);
        tryCompare(jump, "visible", false);
        compare(list.firstUnseenIndex, -1);
        tryVerify(function() {
            return unseenIsInView(list, unseen);
        });
        compare(firstVisibleIndex(list), unseen);
    }

    function test_consecutiveSameAuthorMinuteGroupsTranscriptRows() {
        var list = item("messageList");
        var start = list.model.count;
        var fixture = [
            { author: "anna", time: "11:11", body: "group lead", kind: "message" },
            { author: "anna", time: "11:11", body: "group continuation", kind: "message" },
            { author: "anna", time: "11:12", body: "changed minute", kind: "message" },
            { author: "dax", time: "11:12", body: "changed sender", kind: "message" },
            { author: "", time: "", body: "event break", kind: "event" },
            { author: "dax", time: "11:12", body: "after event", kind: "message" }
        ];
        var index = 0;
        for (index = 0; index < fixture.length; ++index)
            list.model.append(fixture[index]);
        tryCompare(list.model, "count", start + fixture.length);

        compare(list.model.get(start + 1).author, "anna");
        compare(list.model.get(start + 1).time, "11:11");
        compare(list.model.get(start + 1).body, "group continuation");
        compare(list.model.get(start + 1).kind, "message");

        var lead = renderedMessageRow(list, start);
        var grouped = renderedMessageRow(list, start + 1);
        var newMinute = renderedMessageRow(list, start + 2);
        var newSender = renderedMessageRow(list, start + 3);
        var eventRow = renderedMessageRow(list, start + 4);
        var afterEvent = renderedMessageRow(list, start + 5);

        assertMessageChrome(lead, true, "group lead");
        assertMessageChrome(grouped, false, "group continuation");
        verify(grouped.height < lead.height, "Grouped rows should use tighter vertical spacing");
        verify(grouped.height < appWindow.scaledSize(58));
        verify(lead.height >= appWindow.scaledSize(58));
        var groupedBody = findChild(grouped, "messageBody");
        compare(groupedBody.anchors.topMargin, appWindow.scaledSize(4));
        compare(findChild(lead, "messageBody").anchors.topMargin, appWindow.scaledSize(29));

        assertMessageChrome(newMinute, true, "changed minute");
        assertMessageChrome(newSender, true, "changed sender");

        var eventText = findChild(eventRow, "messageEvent");
        verify(eventText !== null && eventText.visible, "Events should keep their own row");
        compare(eventText.text, "event break");
        compare(findChild(eventRow, "messageAvatar").visible, false);
        compare(findChild(eventRow, "messageHeader").visible, false);
        compare(findChild(eventRow, "messageBody").visible, false);

        assertMessageChrome(afterEvent, true, "after event");
        saveScreenshot("grouped-messages");

        var dms = item("directConversationRepeater");
        var anna = dms.itemAt(0);
        verify(anna !== null, "The anna direct-message delegate should be rendered");
        mouseClick(anna);
        tryCompare(appWindow, "currentConversation", "anna");
        waitForRendering(appWindow.contentItem);

        var dmList = item("messageList");
        compare(dmList.Accessible.name, "Messages in anna");
        var dmStart = dmList.model.count;
        compare(dmList.model.get(dmStart - 1).author, "anna");
        compare(dmList.model.get(dmStart - 1).time, "10:12");
        dmList.model.append({
            author: "anna",
            time: "10:12",
            body: "dm continuation",
            kind: "message"
        });
        dmList.model.append({
            author: "anna",
            time: "10:13",
            body: "dm new minute",
            kind: "message"
        });
        tryCompare(dmList.model, "count", dmStart + 2);

        var dmExisting = renderedMessageRow(dmList, dmStart - 1);
        var dmGrouped = renderedMessageRow(dmList, dmStart);
        var dmLead = renderedMessageRow(dmList, dmStart + 1);
        assertMessageChrome(dmExisting, true, "The prototype already feels at home. Nice work.");
        assertMessageChrome(dmGrouped, false, "dm continuation");
        assertMessageChrome(dmLead, true, "dm new minute");
        verify(dmGrouped.height < dmExisting.height);
        compare(dmList.model.get(dmStart).author, "anna");
        compare(dmList.model.get(dmStart).time, "10:12");
        compare(dmList.model.get(dmStart).body, "dm continuation");
    }

    function test_replayAndLiveSameAuthorMinuteDoNotGroup() {
        var list = item("messageList");
        var start = list.model.count;
        list.model.append({
            author: "anna",
            time: "16:40",
            body: "replayed line",
            kind: "message",
            origin: "replay"
        });
        list.model.append({
            author: "anna",
            time: "16:40",
            body: "live line",
            kind: "message",
            origin: "live"
        });
        tryCompare(list.model, "count", start + 2);

        var replay = renderedMessageRow(list, start);
        var live = renderedMessageRow(list, start + 1);
        assertMessageChrome(replay, true, "replayed line");
        assertMessageChrome(live, true, "live line");
        var replayBody = findChild(replay, "messageBody");
        compare(replayBody.color, appWindow.mutedColor);

        var replayInitial = findChild(replay, "messageAvatarInitial");
        var liveInitial = findChild(live, "messageAvatarInitial");
        verify(replayInitial !== null, "Could not find messageAvatarInitial");
        verify(liveInitial !== null, "Could not find messageAvatarInitial");
        compare(replayInitial.color.toString(), appWindow.mutedColor.toString());
        compare(liveInitial.color.toString(),
                Qt.color(appWindow.nickColor("anna")).toString());
        verify(liveInitial.color.toString() !== appWindow.mutedColor.toString());
        saveScreenshot("replay-live-ungrouped");
    }

    function prependMockReplay(list, count) {
        var index = 0;
        for (index = 0; index < count; ++index) {
            list.model.insert(0, {
                author: "anna",
                time: "09:00",
                body: "replayed " + index,
                kind: "message",
                origin: "replay"
            });
        }
        tryCompare(list.model, "count", list.count);
    }

    function test_historySpliceKeepsTheReaderOnTheSameMessage() {
        var list = item("messageList");
        fillMockMessagesUntilScrollable(list);

        keyClick(Qt.Key_PageUp);
        waitForRendering(appWindow.contentItem);
        tryCompare(list, "stick", 1);
        verify(!transcriptPinned(list));

        var anchorBody = list.model.get(firstVisibleIndex(list)).body;
        list.snapshotAnchor();
        prependMockReplay(list, 9);
        waitForRendering(appWindow.contentItem);
        list.restoreAnchor();
        waitForRendering(appWindow.contentItem);
        wait(0);

        compare(list.model.get(firstVisibleIndex(list)).body, anchorBody);
        saveScreenshot("history-splice-anchor");
    }

    function test_historySpliceMovesTheUnseenMarkerWithItsRow() {
        var list = item("messageList");
        fillMockMessagesUntilScrollable(list);

        keyClick(Qt.Key_PageUp);
        waitForRendering(appWindow.contentItem);
        tryCompare(list, "stick", 1);

        var previousCount = list.model.count;
        appendMockMessages(list, 1, "first unseen");
        waitForRendering(appWindow.contentItem);
        wait(0);
        compare(list.firstUnseenIndex, previousCount);
        var unseenBody = list.model.get(list.firstUnseenIndex).body;

        var beforeSplice = list.model.count;
        prependMockReplay(list, 9);
        list.noteSplice(beforeSplice, list.count);
        compare(list.model.get(list.firstUnseenIndex).body, unseenBody);

        list.firstUnseenIndex = -1;
        list.noteSplice(list.count, list.count + 9);
        compare(list.firstUnseenIndex, -1);
    }

    function test_messageBodyIsSelectable() {
        var composer = item("messageComposer");
        mouseClick(composer);
        verify(composer.activeFocus);

        var body = visibleListChild("messageList", "messageBody");
        mouseClick(body);
        verify(composer.activeFocus);

        body.selectAll();
        verify(body.selectedText.length > 0);
        verify(!containsMirc(body.selectedText));

        var members = item("membersList");
        members.positionViewAtIndex(0, ListView.Contain);
        wait(0);
        var anna = members.itemAtIndex(0);
        verify(anna !== null, "The first member delegate should be rendered");
        mouseClick(anna);
        tryCompare(appWindow, "currentConversation", "anna");
        waitForRendering(appWindow.contentItem);

        var dmBody = visibleListChild("messageList", "messageBody");
        dmBody.selectAll();
        verify(dmBody.selectedText.length > 0);
        verify(!containsMirc(dmBody.selectedText));
        verify(composer.activeFocus);
    }

    function test_consoleBodyIsSelectable() {
        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);

        var body = visibleListChild("consoleList", "consoleText");
        body.selectAll();
        verify(body.selectedText.length > 0);
        verify(!containsMirc(body.selectedText));
        verify(item("messageComposer").activeFocus);
    }

    function test_consoleBodyStripsMircFormatting() {
        var list = item("consoleList");
        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);

        var previousCount = list.model.count;
        list.model.append({
            time: "12:00:03",
            label: "PRIVMSG",
            text: formattedIrcBody(),
            source: "server",
            severity: "info"
        });
        tryCompare(list.model, "count", previousCount + 1);
        compare(list.model.get(previousCount).text, formattedIrcBody());

        list.positionViewAtIndex(previousCount, ListView.Contain);
        waitForRendering(appWindow.contentItem);

        var row = list.itemAtIndex(previousCount);
        verify(row !== null, "The formatted console line should be rendered");
        var body = findChild(row, "consoleText");
        verify(body !== null, "Could not find formatted consoleText");
        compare(body.text, "bold / red");
        body.selectAll();
        compare(body.selectedText, "bold / red");
        verify(!containsMirc(body.selectedText));
    }

    function test_messageBodyStripsMircFormatting() {
        var list = item("messageList");
        var previousCount = list.model.count;
        list.model.append({
            author: "anna",
            time: "10:00",
            body: formattedIrcBody(),
            kind: "message"
        });
        tryCompare(list.model, "count", previousCount + 1);
        compare(appWindow.plainIrcText(formattedIrcBody()), "bold / red");
        compare(list.model.get(previousCount).body, formattedIrcBody());

        list.positionViewAtIndex(previousCount, ListView.Contain);
        waitForRendering(appWindow.contentItem);

        var row = list.itemAtIndex(previousCount);
        verify(row !== null, "The formatted mock message should be rendered");
        var body = findChild(row, "messageBody");
        verify(body !== null && body.visible, "Could not find formatted messageBody");
        compare(body.text, "bold / red");
        body.selectAll();
        compare(body.selectedText, "bold / red");
        verify(!containsMirc(body.selectedText));
    }

    function test_liveMessageBodyStripsMircFormatting() {
        liveMessages.clear();
        liveMessages.append({
            author: "anna",
            time: "10:00",
            body: formattedIrcBody(),
            kind: "message"
        });
        liveConsole.open = false;
        if (appWindow) {
            appWindow.destroy();
            appWindow = null;
            wait(0);
        }
        var window = createTemporaryObject(liveWindowComponent, null);
        verify(window !== null, "The live window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        var list = findChild(window, "messageList");
        verify(list !== null, "Could not find messageList");
        list.positionViewAtIndex(0, ListView.Contain);
        waitForRendering(window.contentItem);

        var body = null;
        var row = list.itemAtIndex(0);
        verify(row !== null, "The formatted live message should be rendered");
        body = findChild(row, "messageBody");
        verify(body !== null && body.visible, "Could not find live messageBody");
        compare(body.text, "bold / red");
        body.selectAll();
        compare(body.selectedText, "bold / red");
        verify(!containsMirc(body.selectedText));

        window.close();
        liveMessages.clear();
        liveConsole.open = false;
    }

    function test_httpUrlAllowlist() {
        compare(appWindow.httpUrlAt("see https://example.com now", 6), "https://example.com");
        compare(appWindow.httpUrlAt("see http://example.com now", 6), "http://example.com");
        compare(appWindow.httpUrlAt("see javascript:alert(1) now", 6), "");
        compare(appWindow.httpUrlAt("see file:///etc/passwd now", 6), "");
        compare(appWindow.httpUrlAt("see https://example.com now", 0), "");
        compare(appWindow.lastOpenedUrl, "");
        verify(!appWindow.openAllowedUrl("javascript:alert(1)"));
        compare(appWindow.lastOpenedUrl, "");
        verify(!appWindow.openAllowedUrl("file:///tmp/x"));
        compare(appWindow.lastOpenedUrl, "");
        verify(appWindow.openAllowedUrl("https://example.com"));
        compare(appWindow.lastOpenedUrl, "https://example.com");
        verify(appWindow.openAllowedUrl("http://example.com"));
        compare(appWindow.lastOpenedUrl, "http://example.com");
    }

    function test_unfocusedMentionNotifiesOnce() {
        appWindow.lastNotification = null;
        appWindow.notifyMentionIfUnfocused(false, "alice", "hey \x02fred");
        compare(appWindow.lastNotification.author, "alice");
        compare(appWindow.lastNotification.body, "hey fred");

        appWindow.lastNotification = null;
        appWindow.notifyMentionIfUnfocused(true, "alice", "hey fred");
        compare(appWindow.lastNotification, null);

        appWindow.notifyMentionIfUnfocused(false, "alice", "hello");
        compare(appWindow.lastNotification.author, "alice");
        compare(appWindow.lastNotification.body, "hello");
    }

    function test_messageBodyClickOpensHttpsUrl() {
        var list = item("messageList");
        var previousCount = list.model.count;
        list.model.append({
            author: "anna",
            time: "10:00",
            body: "read https://example.com thanks",
            kind: "message"
        });
        tryCompare(list.model, "count", previousCount + 1);
        list.positionViewAtIndex(previousCount, ListView.Contain);
        waitForRendering(appWindow.contentItem);

        var row = list.itemAtIndex(previousCount);
        verify(row !== null, "The linked mock message should be rendered");
        var body = findChild(row, "messageBody");
        verify(body !== null && body.visible, "Could not find linked messageBody");
        compare(body.textFormat, TextEdit.PlainText);
        compare(body.text, "read https://example.com thanks");
        body.selectAll();
        compare(body.selectedText, "read https://example.com thanks");
        body.deselect();

        appWindow.lastOpenedUrl = "";
        var start = body.text.indexOf("https://example.com");
        var rect = body.positionToRectangle(start + 4);
        var hit = findChild(body, "urlHit");
        verify(hit !== null, "Could not find message urlHit");
        mouseClick(hit, rect.x + Math.max(1, rect.width / 2), rect.y + rect.height / 2);
        compare(appWindow.lastOpenedUrl, "https://example.com");
    }

    function test_consoleBodyClickOpensHttpUrl() {
        var list = item("consoleList");
        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);

        var previousCount = list.model.count;
        list.model.append({
            time: "12:00:03",
            label: "PRIVMSG",
            text: "motd http://example.com end",
            source: "server",
            severity: "info"
        });
        tryCompare(list.model, "count", previousCount + 1);
        list.positionViewAtIndex(previousCount, ListView.Contain);
        waitForRendering(appWindow.contentItem);

        var row = list.itemAtIndex(previousCount);
        verify(row !== null, "The linked console line should be rendered");
        var body = findChild(row, "consoleText");
        verify(body !== null, "Could not find linked consoleText");
        compare(body.textFormat, TextEdit.PlainText);
        compare(body.text, "motd http://example.com end");
        body.selectAll();
        compare(body.selectedText, "motd http://example.com end");
        body.deselect();

        appWindow.lastOpenedUrl = "";
        var start = body.text.indexOf("http://example.com");
        var rect = body.positionToRectangle(start + 4);
        var hit = findChild(body, "urlHit");
        verify(hit !== null, "Could not find console urlHit");
        mouseClick(hit, rect.x + Math.max(1, rect.width / 2), rect.y + rect.height / 2);
        compare(appWindow.lastOpenedUrl, "http://example.com");
    }

    function test_eventRowAndTopicStripMirc() {
        appWindow.mockCurrentTopic = formattedIrcBody();
        compare(appWindow.currentTopic, formattedIrcBody());
        var topic = item("conversationTopic");
        compare(topic.text, "bold / red");
        compare(topic.textFormat, Text.PlainText);
        verify(!containsMirc(topic.text));

        var list = item("messageList");
        var previousCount = list.model.count;
        list.model.append({
            author: "",
            time: "",
            body: formattedIrcBody(),
            kind: "event"
        });
        tryCompare(list.model, "count", previousCount + 1);
        list.positionViewAtIndex(previousCount, ListView.Contain);
        waitForRendering(appWindow.contentItem);

        var row = list.itemAtIndex(previousCount);
        verify(row !== null, "The formatted event row should be rendered");
        var eventText = findChild(row, "messageEvent");
        verify(eventText !== null && eventText.visible, "Could not find messageEvent");
        compare(eventText.text, "bold / red");
        compare(eventText.textFormat, Text.PlainText);
        verify(!containsMirc(eventText.text));
    }

    function test_whoisRowWrapsLongBody() {
        var list = item("messageList");
        var previousCount = list.model.count;
        var chunk = "lena is ~lena@user/host (Lena) is on #omarchy #help #omairc ";
        var body = chunk + chunk + chunk + chunk + chunk + chunk +
                   chunk + chunk + "End of WHOIS for lena";
        list.model.append({
            author: "",
            time: "",
            body: body,
            kind: "whois"
        });
        tryCompare(list.model, "count", previousCount + 1);
        list.positionViewAtIndex(previousCount, ListView.Contain);
        waitForRendering(appWindow.contentItem);

        var row = list.itemAtIndex(previousCount);
        verify(row !== null, "The long whois row should be rendered");
        var whoisText = findChild(row, "messageWhois");
        verify(whoisText !== null && whoisText.visible, "Could not find messageWhois");
        compare(whoisText.wrapMode, Text.Wrap);
        compare(whoisText.textFormat, Text.PlainText);
        compare(whoisText.text, body);
        tryVerify(function () { return whoisText.lineCount > 1; },
                  1000, "The WHOIS row should wrap");
        tryVerify(function () {
            return row.height > appWindow.scaledSize(22);
        }, 1000, "A wrapped whois row should grow past the single-line height");
        compare(findChild(row, "messageEvent").visible, false);
        compare(findChild(row, "messageAvatar").visible, false);
        compare(findChild(row, "messageHeader").visible, false);
        compare(findChild(row, "messageBody").visible, false);
        saveScreenshot("wrapped-whois-row");
    }

    function test_toggleMembersWithShortcut() {
        var panel = item("membersPanel");
        verify(panel.visible);

        keyClick(Qt.Key_M, Qt.ControlModifier | Qt.ShiftModifier);

        tryCompare(panel, "visible", false);
        compare(item("peopleButton").Accessible.name, "Show members");
        saveScreenshot("toggle-members");
    }

    function test_focusMembersWithShortcut() {
        var panel = item("membersPanel");
        var members = item("membersList");
        verify(panel.visible);

        keyClick(Qt.Key_P, Qt.ControlModifier | Qt.ShiftModifier);

        tryCompare(panel, "visible", true);
        tryCompare(members, "activeFocus", true);
    }

    function test_memberHighlightOnlyWhileListFocused() {
        var members = item("membersList");
        members.positionViewAtIndex(0, ListView.Contain);
        wait(0);
        var anna = members.itemAtIndex(0);
        verify(anna !== null, "The first member delegate should be rendered");
        compare(anna.nick, "anna");
        var highlight = findChild(anna, "memberHighlight");
        verify(highlight !== null, "Could not find memberHighlight");
        verify(!members.activeFocus);
        compare(members.currentIndex, 0);
        verify(Qt.colorEqual(highlight.color, "transparent"));

        keyClick(Qt.Key_P, Qt.ControlModifier | Qt.ShiftModifier);
        tryCompare(members, "activeFocus", true);
        verify(Qt.colorEqual(highlight.color, appWindow.raisedColor),
               "focused current member should use raisedColor");

        keyClick(Qt.Key_L, Qt.ControlModifier);
        tryCompare(item("messageComposer"), "activeFocus", true);
        tryCompare(members, "activeFocus", false);
        verify(Qt.colorEqual(highlight.color, "transparent"),
               "unfocused member list should not keep a selection fill");
    }

    function test_focusMembersReopensHiddenPanel() {
        var panel = item("membersPanel");
        var members = item("membersList");
        verify(panel.visible);

        keyClick(Qt.Key_M, Qt.ControlModifier | Qt.ShiftModifier);
        tryCompare(panel, "visible", false);

        keyClick(Qt.Key_P, Qt.ControlModifier | Qt.ShiftModifier);

        tryCompare(panel, "visible", true);
        tryCompare(members, "activeFocus", true);
    }

    function test_memberListEnterOpensDirectMessage() {
        var members = item("membersList");

        keyClick(Qt.Key_P, Qt.ControlModifier | Qt.ShiftModifier);
        tryCompare(members, "activeFocus", true);
        tryCompare(members, "currentIndex", 0);

        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        tryCompare(members, "currentIndex", 2);

        keyClick(Qt.Key_Return);
        tryCompare(appWindow, "currentConversation", "mira");
    }

    function test_memberEnterAfterSwitchingToSmallerChannel() {
        var members = item("membersList");

        keyClick(Qt.Key_P, Qt.ControlModifier | Qt.ShiftModifier);
        tryCompare(members, "activeFocus", true);

        var step = 0;
        for (step = 0; step < 9; ++step)
            keyClick(Qt.Key_Down);
        tryCompare(members, "currentIndex", 9);

        mouseClick(item("conversation-#help"));
        tryCompare(appWindow, "currentConversation", "#help");
        tryCompare(members, "currentIndex", 0);

        keyClick(Qt.Key_P, Qt.ControlModifier | Qt.ShiftModifier);
        tryCompare(members, "activeFocus", true);
        keyClick(Qt.Key_Return);
        tryCompare(appWindow, "currentConversation", "anna");
    }

    function test_focusMembersShortcutIgnoredOnDirectMessage() {
        var anna = item("directConversationRepeater").itemAt(0);
        verify(anna !== null, "The anna direct-message delegate should be rendered");
        mouseClick(anna);
        tryCompare(appWindow, "currentConversation", "anna");
        verify(!item("membersPanel").visible);

        keyClick(Qt.Key_P, Qt.ControlModifier | Qt.ShiftModifier);

        verify(!item("membersPanel").visible);
        compare(appWindow.currentConversation, "anna");
    }

    function test_shortcutsSheetTogglesAndEscapeKeepsConversation() {
        var sheet = item("shortcutsSheet");
        verify(!sheet.opened);
        verify(!sheet.visible);

        keyClick(Qt.Key_Slash, Qt.ControlModifier);
        tryCompare(sheet, "opened", true);

        keyClick(Qt.Key_Escape);
        tryCompare(sheet, "opened", false);
        compare(appWindow.currentConversation, "#omarchy");
    }

    function test_shortcutsSheetBlocksWindowShortcuts() {
        var sheet = item("shortcutsSheet");

        keyClick(Qt.Key_Slash, Qt.ControlModifier);
        tryCompare(sheet, "opened", true);
        compare(appWindow.currentConversation, "#omarchy");
        compare(appWindow.consoleVisible, false);

        keyClick(Qt.Key_Down, Qt.AltModifier);
        compare(appWindow.currentConversation, "#omarchy");
        verify(sheet.opened);

        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        compare(appWindow.consoleVisible, false);
        verify(sheet.opened);

        keyClick(Qt.Key_A, Qt.AltModifier);
        compare(appWindow.currentConversation, "#omarchy");
        verify(sheet.opened);

        keyClick(Qt.Key_Right, Qt.AltModifier);
        compare(appWindow.sidebarNetworkFocusId, "");
        verify(sheet.opened);
    }

    function test_shortcutsSheetEscapeDoesNotLeaveStatus() {
        var sheet = item("shortcutsSheet");

        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);

        keyClick(Qt.Key_Slash, Qt.ControlModifier);
        tryCompare(sheet, "opened", true);

        keyClick(Qt.Key_Escape);
        tryCompare(sheet, "opened", false);
        compare(appWindow.consoleVisible, true);

        keyClick(Qt.Key_Escape);
        tryCompare(appWindow, "consoleVisible", false);
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
        compare(findChild(window, "connectionConnectOnStartup").checked, false);
        compare(findChild(window, "connectionProblem").text, "Nick is required");
        compare(findChild(window, "networkChoiceList") !== null, true);
        compare(findChild(window, "connectionAddNetwork").visible, false);
        compare(findChild(window, "connectionRemove").visible, false);
        var password = findChild(window, "connectionPassword");
        var formViewport = findChild(window, "sheetFlick");
        verify(password.mapToItem(sheet, 0, password.height).y
               <= formViewport.mapToItem(sheet, 0, formViewport.height).y,
               "Password field should be visible without scrolling");
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

    function test_liveDmTypingIndicatorUsesBothProductionDelegates() {
        var fallback = createTemporaryObject(liveWindowComponent, null);
        verify(fallback !== null, "The fallback live window should load");
        tryCompare(fallback, "visible", true);
        waitForRendering(fallback.contentItem);

        var fallbackDms = findChild(fallback, "directConversationRepeater");
        verify(fallbackDms !== null, "The fallback direct-message repeater should exist");
        compare(fallbackDms.count, 2);
        var fallbackRow = fallbackDms.itemAt(1);
        verify(fallbackRow !== null, "The fallback DM row should be rendered");
        compare(fallbackRow.conversationName, "anna");
        tryCompare(fallbackRow, "visible", true);
        var fallbackDots = findChild(fallbackRow, "conversation-typing-anna");
        verify(fallbackDots !== null, "The fallback DM typing indicator should exist");
        tryCompare(fallbackDots, "visible", true);
        fallback.close();

        var connected = createTemporaryObject(fallbackWindowComponent, null);
        verify(connected !== null, "The connected live window should load");
        tryCompare(connected, "visible", true);
        waitForRendering(connected.contentItem);

        var networks = findChild(connected, "liveNetworkRepeater");
        verify(networks !== null, "The live network repeater should exist");
        var network = networks.itemAt(0);
        verify(network !== null, "The connected network section should be rendered");
        var connectedDms = findChild(network, "directConversationRepeater");
        verify(connectedDms !== null, "The connected direct-message repeater should exist");
        compare(connectedDms.count, 2);
        var connectedRow = connectedDms.itemAt(1);
        verify(connectedRow !== null, "The connected DM row should be rendered");
        compare(connectedRow.conversationName, "anna");
        tryCompare(connectedRow, "visible", true);
        var connectedDots = findChild(connectedRow, "conversation-typing-libera-anna");
        verify(connectedDots !== null, "The connected DM typing indicator should exist");
        tryCompare(connectedDots, "visible", true);
        connected.close();
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

    function test_typingChromeFollowsCapabilities() {
        liveConsole.open = false;
        gatedIrc.hasTyping = false;
        gatedIrc.isChannel = true;
        gatedIrc.selectedTarget = "#omarchy";
        var window = createTemporaryObject(gatedWindowComponent, null);
        verify(window !== null, "The gated window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        verify(findChild(window, "membersPanel").visible);
        var members = findChild(window, "membersList");
        verify(members !== null, "Could not find membersList");
        var member = members.itemAtIndex(0);
        verify(member !== null, "The anna member delegate should be rendered");
        var glyph = findChild(member, "member-typing-anna");
        verify(glyph !== null, "The member typing glyph should be rendered");
        compare(glyph.visible, false);

        gatedIrc.hasTyping = true;
        tryCompare(glyph, "visible", true);

        gatedIrc.isChannel = false;
        gatedIrc.selectedTarget = "anna";
        waitForRendering(window.contentItem);
        var overlay = findChild(window, "composer-typing");
        verify(overlay !== null, "The composer typing overlay should exist");
        tryCompare(overlay, "visible", true);
        window.close();
        gatedIrc.hasTyping = false;
        gatedIrc.isChannel = true;
        gatedIrc.selectedTarget = "#omarchy";
    }

    function test_mockTypingShowsMemberGlyphAndDmOverlay() {
        var members = item("membersList");
        verify(item("membersPanel").visible);
        var anna = members.itemAtIndex(0);
        verify(anna !== null, "The anna member delegate should be rendered");
        var glyph = findChild(anna, "member-typing-anna");
        verify(glyph !== null, "The member typing glyph should be rendered");
        tryCompare(glyph, "visible", true);
        saveScreenshot("typing-member-glyph");

        mouseClick(anna);
        tryCompare(appWindow, "currentConversation", "anna");
        verify(!item("membersPanel").visible);
        var overlay = item("composer-typing");
        tryCompare(overlay, "visible", true);
        compare(overlay.height, appWindow.scaledSize(12));
        saveScreenshot("typing-dm-overlay");
    }

    function test_mockBackgroundDmTypingIndicatorPulses() {
        compare(appWindow.currentConversation, "#omarchy");
        var dms = item("directConversationRepeater");
        var anna = dms.itemAt(0);
        verify(anna !== null, "The anna direct-message delegate should be rendered");
        compare(anna.conversationName, "anna");
        compare(anna.direct, true);
        compare(anna.current, false);
        compare(anna.typing, true);
        var dots = findChild(anna, "conversation-typing-anna");
        verify(dots !== null, "The background DM typing indicator should be rendered");
        tryCompare(dots, "visible", true);
        saveScreenshot("typing-sidebar-dm");
        var pulse = dots.pulse;
        wait(320);
        tryCompare(dots, "pulse", (pulse + 1) % 3);
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
        compare(list.model.get(0).text, "-AUTH- *** Looking up your hostname...");
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
        compare(appWindow.title, "Omarchy IRC Status");
        var list = item("consoleList");
        verify(list.visible);
        compare(list.model.get(0).text, "-AUTH- *** Looking up your hostname...");
        verify(!item("peopleButton").visible);
    }

    function test_mockIdentityFooterShowsFredAndDropsNotice() {
        compare(item("selfNickLabel").text, "fred");
        compare(findChild(appWindow, "mockNotice"), null);
    }

    function test_mockIdentityFooterShowsAvailable() {
        compare(item("selfPresenceLabel").text, "available");
        verify(Qt.colorEqual(item("selfPresenceDot").color, "#69b978"));
    }

    function test_liveIdentityFooterShowsCurrentNick() {
        var window = createTemporaryObject(liveWindowComponent, null);
        verify(window !== null, "The live window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        compare(findChild(window, "selfNickLabel").text, "live-nick");
        window.close();
    }

    function test_liveIdentityFooterShowsAwayWithoutMemberPresence() {
        gatedIrc.selfAway = true;
        gatedIrc.hasAwayPresence = false;
        var window = createTemporaryObject(gatedWindowComponent, null);
        verify(window !== null, "The gated away window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        compare(findChild(window, "selfPresenceLabel").text, "away");
        verify(Qt.colorEqual(findChild(window, "selfPresenceDot").color, "#d6a552"));
        window.close();
        gatedIrc.selfAway = false;
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

    function test_channelCtrlWIsNoOp() {
        compare(appWindow.currentConversation, "#omarchy");
        var dms = item("directConversationRepeater");
        compare(dms.count, 2);

        keyClick(Qt.Key_W, Qt.ControlModifier);

        compare(appWindow.currentConversation, "#omarchy");
        compare(appWindow.consoleVisible, false);
        compare(dms.count, 2);

        appWindow.closeDirectMessage();
        compare(appWindow.currentConversation, "#omarchy");
        compare(dms.count, 2);
        compare(dms.itemAt(0).conversationName, "anna");
        compare(dms.itemAt(1).conversationName, "dax");
    }

    function test_closeDirectMessageSelectsNext() {
        var dms = item("directConversationRepeater");
        var anna = dms.itemAt(0);
        verify(anna !== null, "The anna direct-message delegate should be rendered");
        mouseClick(anna);
        tryCompare(appWindow, "currentConversation", "anna");

        keyClick(Qt.Key_W, Qt.ControlModifier);

        tryCompare(appWindow, "currentConversation", "dax");
        compare(appWindow.consoleVisible, false);
        compare(dms.count, 1);
        compare(dms.itemAt(0).conversationName, "dax");
        compare(findChild(appWindow, "conversation-anna"), null);
        compare(item("messageList").Accessible.name, "Messages in dax");
    }

    function test_closeDirectMessageSelectsPreviousWithoutWrapping() {
        var dms = item("directConversationRepeater");
        var dax = dms.itemAt(1);
        verify(dax !== null, "The dax direct-message delegate should be rendered");
        mouseClick(dax);
        tryCompare(appWindow, "currentConversation", "dax");

        keyClick(Qt.Key_W, Qt.ControlModifier);

        tryCompare(appWindow, "currentConversation", "anna");
        compare(appWindow.consoleVisible, false);
        compare(dms.count, 1);
        compare(dms.itemAt(0).conversationName, "anna");

        keyClick(Qt.Key_W, Qt.ControlModifier);

        tryCompare(appWindow, "currentConversation", "#help");
        compare(appWindow.consoleVisible, false);
        compare(dms.count, 0);
        compare(findChild(appWindow, "conversation-anna"), null);
        compare(findChild(appWindow, "conversation-dax"), null);
    }

    function test_statusShortcutsUntouchedByClose() {
        var dms = item("directConversationRepeater");
        var anna = dms.itemAt(0);
        verify(anna !== null, "The anna direct-message delegate should be rendered");
        mouseClick(anna);
        tryCompare(appWindow, "currentConversation", "anna");

        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);

        keyClick(Qt.Key_W, Qt.ControlModifier);
        compare(appWindow.consoleVisible, true);
        compare(appWindow.currentConversation, "anna");
        compare(item("directConversationRepeater").count, 2);

        keyClick(Qt.Key_Escape);
        tryCompare(appWindow, "consoleVisible", false);
        compare(appWindow.currentConversation, "anna");

        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);
        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", false);
        compare(appWindow.currentConversation, "anna");
    }

    function test_typedCloseStaysChatLine() {
        var composer = item("messageComposer");
        var messages = item("messageList");
        var previousCount = messages.model.count;
        mouseClick(composer);
        typeText("/close");
        compare(composer.text, "/close");
        keyClick(Qt.Key_Return);

        tryCompare(messages.model, "count", previousCount + 1);
        compare(messages.model.get(previousCount).body, "/close");
        compare(composer.text, "");
        compare(appWindow.currentConversation, "#omarchy");
        compare(item("directConversationRepeater").count, 2);
    }

    function openSlashWindow() {
        if (appWindow) {
            appWindow.close();
            appWindow = null;
        }
        slashFake.reset();
        var window = createTemporaryObject(slashWindowComponent, null);
        verify(window !== null, "The slash-complete window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);
        return window;
    }

    function test_slashCompleteListAppearsForSlashJ() {
        var window = openSlashWindow();
        var composer = findChild(window, "messageComposer");
        verify(composer !== null, "Could not find messageComposer");
        mouseClick(composer);
        verify(composer.activeFocus);
        typeText("/j");

        var list = findChild(window, "slashCompleteList");
        verify(list !== null, "Could not find slashCompleteList");
        tryCompare(list, "visible", true);
        waitForRendering(window.contentItem);
        var joinHit = findChild(list, "slashHit-join");
        verify(joinHit !== null, "Could not find slashHit-join");
        verify(joinHit.visible);
        compare(slashFake.selectedIndex, 0);
        window.close();
        slashFake.reset();
    }

    function test_slashCompleteTabInsertsCanonicalVerb() {
        var window = openSlashWindow();
        var composer = findChild(window, "messageComposer");
        verify(composer !== null, "Could not find messageComposer");
        mouseClick(composer);
        verify(composer.activeFocus);
        typeText("/j");
        tryCompare(findChild(window, "slashCompleteList"), "visible", true);
        keyClick(Qt.Key_Tab);

        compare(composer.text, "/join ");
        tryCompare(findChild(window, "slashCompleteList"), "visible", false);
        verify(composer.activeFocus);
        window.close();
        slashFake.reset();
    }

    function test_slashCompleteEscapeDismisses() {
        var window = openSlashWindow();
        var composer = findChild(window, "messageComposer");
        verify(composer !== null, "Could not find messageComposer");
        mouseClick(composer);
        verify(composer.activeFocus);
        typeText("/j");
        tryCompare(findChild(window, "slashCompleteList"), "visible", true);
        keyClick(Qt.Key_Escape);

        tryCompare(findChild(window, "slashCompleteList"), "visible", false);
        compare(composer.text, "/j");
        compare(window.consoleVisible, false);
        verify(composer.activeFocus);
        window.close();
        slashFake.reset();
    }

    function test_slashCompleteUpDownMoveSelection() {
        var window = openSlashWindow();
        var composer = findChild(window, "messageComposer");
        verify(composer !== null, "Could not find messageComposer");
        mouseClick(composer);
        verify(composer.activeFocus);
        typeText("/j");
        tryCompare(findChild(window, "slashCompleteList"), "visible", true);
        compare(slashFake.selectedIndex, 0);

        keyClick(Qt.Key_Down);
        compare(slashFake.selectedIndex, 1);
        keyClick(Qt.Key_Up);
        compare(slashFake.selectedIndex, 0);
        keyClick(Qt.Key_Up);
        compare(slashFake.selectedIndex, 1);
        compare(composer.text, "/j");
        verify(findChild(window, "slashCompleteList").visible);
        window.close();
        slashFake.reset();
    }

    function test_slashCompleteHistoryUpWalksPastBareCommand() {
        var window = openSlashWindow();
        var composer = findChild(window, "messageComposer");
        verify(composer !== null, "Could not find messageComposer");
        mouseClick(composer);
        verify(composer.activeFocus);

        typeText("hello");
        keyClick(Qt.Key_Return);
        compare(composer.text, "");

        typeText("/j");
        tryCompare(findChild(window, "slashCompleteList"), "visible", true);
        mouseClick(findChild(window, "sendButton"));
        compare(composer.text, "");
        tryCompare(findChild(window, "slashCompleteList"), "visible", false);

        keyClick(Qt.Key_Up);
        compare(composer.text, "/j");
        tryCompare(findChild(window, "slashCompleteList"), "visible", false);
        compare(slashFake.selectedIndex, 0);

        keyClick(Qt.Key_Up);
        compare(composer.text, "hello");
        compare(findChild(window, "slashCompleteList").visible, false);
        window.close();
        slashFake.reset();
    }

    function test_mockTwoNetworkSectionsStaySeparated() {
        var omarchyHeader = item("networkHeader");
        var oftcHeader = item("networkHeader-mock-oftc");
        verify(omarchyHeader.visible);
        verify(oftcHeader.visible);
        verify(item("networkHeaderButton").visible);
        verify(item("networkHeaderButton-mock-oftc").visible);
        compare(item("conversation-#omarchy").conversationName, "#omarchy");
        compare(item("conversation-oftc-#omarchy").conversationName, "#omarchy");
        compare(item("conversation-oftc-#lab").conversationName, "#lab");
        compare(item("conversation-oftc-rio").conversationName, "rio");
        compare(item("conversation-#omarchy").current, true);
        compare(item("conversation-oftc-#omarchy").current, false);
        saveScreenshot("two-networks");
    }

    function test_duplicateOmarchyHighlightsIndependently() {
        mouseClick(item("conversation-oftc-#omarchy"));
        tryCompare(appWindow, "currentConversation", "#omarchy");
        compare(appWindow.currentConversationId, "mock-oftc\n#omarchy");
        compare(appWindow.currentTopic, "A different #omarchy, hosted on OFTC.");
        compare(appWindow.currentPeopleCount, 4);
        compare(item("selfNickLabel").text, "oak");
        compare(item("conversation-oftc-#omarchy").current, true);
        compare(item("conversation-#omarchy").current, false);
        compare(appWindow.title, "#omarchy · irc.oftc.net - Omairc");

        mouseClick(item("conversation-#omarchy"));
        tryCompare(appWindow, "currentConversationId", "mock-omarchy\n#omarchy");
        compare(appWindow.currentTopic,
                "A cozy corner for Omarchy users and builders.");
        compare(appWindow.currentPeopleCount, 12);
        compare(item("selfNickLabel").text, "fred");
        compare(item("conversation-#omarchy").current, true);
        compare(item("conversation-oftc-#omarchy").current, false);
        compare(appWindow.title, "#omarchy · Omarchy IRC - Omairc");
    }

    function test_altWalkSkipsNetworkHeaders() {
        mouseClick(item("conversation-#help"));
        tryCompare(appWindow, "currentConversation", "#help");
        keyClick(Qt.Key_Down, Qt.AltModifier);
        tryCompare(appWindow, "currentConversation", "anna");
        keyClick(Qt.Key_Down, Qt.AltModifier);
        tryCompare(appWindow, "currentConversation", "dax");
        keyClick(Qt.Key_Down, Qt.AltModifier);
        tryCompare(appWindow, "currentConversation", "#omarchy");
        compare(appWindow.currentConversationId, "mock-oftc\n#omarchy");
        compare(appWindow.currentTopic, "A different #omarchy, hosted on OFTC.");
    }

    function test_walkNetworksWithShortcut() {
        compare(appWindow.currentConversation, "#omarchy");
        compare(appWindow.sidebarNetworkFocusId, "");

        keyClick(Qt.Key_Right, Qt.AltModifier);

        compare(appWindow.sidebarNetworkFocusId, appWindow.mockOftcId);
        compare(appWindow.currentConversation, "#omarchy");
        compare(appWindow.consoleVisible, false);
        compare(item("networkHeader-mock-oftc").parent.headerFocused, true);
        compare(item("networkHeader").parent.headerFocused, false);
    }

    function test_walkNetworksWraps() {
        compare(appWindow.currentConversation, "#omarchy");

        keyClick(Qt.Key_Left, Qt.AltModifier);

        compare(appWindow.sidebarNetworkFocusId, appWindow.mockOftcId);

        keyClick(Qt.Key_Left, Qt.AltModifier);

        compare(appWindow.sidebarNetworkFocusId, appWindow.mockOmarchyId);
        compare(item("networkHeader").parent.headerFocused, true);
    }

    function test_enterOnNetworkHeaderOpensStatus() {
        keyClick(Qt.Key_Right, Qt.AltModifier);
        compare(appWindow.sidebarNetworkFocusId, appWindow.mockOftcId);

        keyClick(Qt.Key_Return);

        tryCompare(appWindow, "consoleVisible", true);
        compare(appWindow.mockStatusNetworkId, appWindow.mockOftcId);
        compare(appWindow.sidebarNetworkFocusId, "");
        compare(appWindow.title, "irc.oftc.net Status");
    }

    function test_altDownStaysConversationOnlyAfterNetworkWalk() {
        keyClick(Qt.Key_Right, Qt.AltModifier);
        compare(appWindow.sidebarNetworkFocusId, appWindow.mockOftcId);

        keyClick(Qt.Key_Down, Qt.AltModifier);

        tryCompare(appWindow, "currentConversation", "#desktop");
        compare(appWindow.sidebarNetworkFocusId, "");
        compare(appWindow.consoleVisible, false);
    }

    function test_shortcutsSheetListsNetworkWalk() {
        var sheet = item("shortcutsSheet");
        keyClick(Qt.Key_Slash, Qt.ControlModifier);
        tryCompare(sheet, "opened", true);
        var texts = shortcutSheetTexts();
        verify(texts.indexOf("Alt+Left / Alt+Right") !== -1,
               "shortcut sheet should list Alt+Left / Alt+Right");
        verify(texts.indexOf("walk networks") !== -1,
               "shortcut sheet should name walk networks");
        keyClick(Qt.Key_Escape);
        tryCompare(sheet, "opened", false);
    }

    function test_emptyNetworkHeaderIsAKeyboardStop() {
        if (appWindow)
            appWindow.close();
        appWindow = null;
        restoreNamedConnection();
        namedNetworks.append({
            networkId: "oftc",
            displayName: "irc.oftc.net",
            stored: true,
            selected: false
        });
        var window = createTemporaryObject(liveNamedWindowComponent, null);
        verify(window !== null, "The two-network window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        try {
            compare(window.currentConversation, "#omarchy");
            compare(namedNetworks.count, 2);
            var networks = findChild(window, "liveNetworkRepeater");
            verify(networks !== null, "The live network repeater should exist");
            tryCompare(networks, "count", 2);
            var oftcSection = networks.itemAt(1);
            verify(oftcSection !== null, "empty OFTC section should stay in the sidebar");
            var oftcHeader = findChild(oftcSection, "networkHeader-oftc");
            verify(oftcHeader !== null, "empty OFTC header should stay in the sidebar");

            keyClick(Qt.Key_Right, Qt.AltModifier);
            compare(window.sidebarNetworkFocusId, "oftc");
            compare(window.currentConversation, "#omarchy");
            compare(window.consoleVisible, false);

            keyClick(Qt.Key_Comma, Qt.ControlModifier);
            var sheet = findChild(window, "connectionSheet");
            tryCompare(sheet, "visible", true);
            compare(namedConnection.selectedNetworkId, "oftc");

            keyClick(Qt.Key_Escape);
            tryCompare(sheet, "visible", false);
            compare(window.sidebarNetworkFocusId, "oftc");

            keyClick(Qt.Key_Return);
            tryCompare(window, "consoleVisible", true);
            compare(liveConsole.networkId, "oftc");
            compare(window.sidebarNetworkFocusId, "");
            compare(window.irc.focusedNetworkId, "oftc");

            keyClick(Qt.Key_Escape);
            tryCompare(window, "consoleVisible", false);

            keyClick(Qt.Key_Down, Qt.AltModifier);
            tryCompare(window, "currentConversation", "anna");
            compare(window.sidebarNetworkFocusId, "");

            liveConversations.append({
                conversation: "#lab",
                unread: 0,
                mention: false,
                direct: false,
                networkId: "oftc",
                conversationId: "oftc\n#lab",
                conversationName: "#lab",
                typing: false
            });
            liveIrc.conversationEpoch += 1;
            waitForRendering(window.contentItem);

            keyClick(Qt.Key_Down, Qt.AltModifier);
            tryCompare(window, "currentConversation", "#lab");

            window.close();
        } finally {
            if (liveConversations.count > 2)
                liveConversations.remove(liveConversations.count - 1);
            liveIrc.selectedNetworkId = "libera";
            liveIrc.selectedTarget = "#omarchy";
            liveIrc.selectedConversationId = "libera\n#omarchy";
            liveIrc.isChannel = true;
            liveIrc.topic = "A cozy corner for Omarchy users and builders.";
            liveIrc.peopleCount = 1;
            liveConsole.open = false;
            liveConsole.networkId = "libera";
            restoreNamedConnection();
        }
    }

    function test_connectionSheetAddSelectRemoveSurface() {
        var window = createTemporaryObject(fallbackWindowComponent, null);
        verify(window !== null, "The fallback window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        try {
            var sheet = findChild(window, "connectionSheet");
            verify(sheet !== null, "Could not find connectionSheet");
            compare(sheet.visible, false);
            keyClick(Qt.Key_Comma, Qt.ControlModifier);
            tryCompare(sheet, "visible", true);

            var addButton = findChild(window, "connectionAddNetwork");
            verify(addButton.visible);
            var removeButton = findChild(window, "connectionRemove");
            verify(removeButton.visible);
            compare(removeButton.width > 0, true);
            var discardButton = findChild(window, "connectionDiscard");
            verify(discardButton !== null);
            compare(findChild(window, "connectionApply") !== null, true);
            compare(namedNetworks.count, 1);

            mouseClick(addButton);
            compare(namedNetworks.count, 2);
            compare(namedConnection.selectedNetworkId, "new-id");
            compare(findChild(window, "connectionHost").text, "");
            namedNetworks.setProperty(1, "displayName", "irc.oftc.net");
            namedConnection.displayName = "irc.oftc.net";
            namedConnection.host = "irc.oftc.net";
            namedConnection.nick = "oak";
            waitForRendering(window.contentItem);

            var choices = findChild(window, "networkChoiceRepeater");
            verify(choices !== null, "Could not find networkChoiceRepeater");
            tryCompare(choices, "count", 2);
            try {
                grabImage(window.contentItem).save(artifactDirectory + "connection-sheet-rail.png");
            } catch (error) {
                fail("Failed to save screenshot 'connection-sheet-rail': " + error);
            }
            var liberaChoice = repeaterItemByName(choices, "networkChoice-libera");
            verify(liberaChoice !== null, "Could not find networkChoice-libera");
            compare(liberaChoice.displayName, "irc.libera.chat");
            mouseClick(liberaChoice);
            compare(namedConnection.selectedNetworkId, "libera");

            mouseClick(removeButton);
            compare(window.connectionRemoveArmed, true);
            mouseClick(removeButton);
            compare(namedNetworks.count, 1);
            window.close();
        } finally {
            restoreNamedConnection();
        }
    }

    function test_untouchedPasswordIsNotClearedOnApply() {
        restoreNamedConnection();
        var window = createTemporaryObject(fallbackWindowComponent, null);
        verify(window !== null, "The password-preservation window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        var applyButton = findChild(window, "connectionApply");
        verify(applyButton !== null, "Could not find connectionApply");
        mouseClick(applyButton);
        compare(namedConnection.passwordSetCalls, 0);

        var password = findChild(window, "connectionPassword");
        verify(password !== null, "Could not find connectionPassword");
        mouseClick(password);
        keyClick(Qt.Key_S);
        keyClick(Qt.Key_E);
        keyClick(Qt.Key_C);
        keyClick(Qt.Key_R);
        keyClick(Qt.Key_E);
        keyClick(Qt.Key_T);
        mouseClick(applyButton);
        compare(namedConnection.passwordSetCalls, 1);
        compare(namedConnection.lastPassword, "secret");
        window.close();
        restoreNamedConnection();
    }

    function test_mockStatusTitleIdentifiesNetwork() {
        mouseClick(item("networkHeaderButton-mock-oftc"));
        tryCompare(appWindow, "consoleVisible", true);
        compare(appWindow.title, "irc.oftc.net Status");
        compare(item("selfNickLabel").text, "oak");
    }

    function test_rejectedNetworkSelectKeepsPassword() {
        restoreNamedConnection();
        namedNetworks.append({
            networkId: "oftc",
            displayName: "irc.oftc.net",
            stored: true,
            selected: false
        });
        namedConnection.dirty = true;
        var window = createTemporaryObject(fallbackWindowComponent, null);
        verify(window !== null, "The rejected-select window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        var password = findChild(window, "connectionPassword");
        verify(password !== null, "Could not find connectionPassword");
        mouseClick(password);
        keyClick(Qt.Key_S);
        keyClick(Qt.Key_E);
        keyClick(Qt.Key_C);
        compare(window.connectionPasswordEdited, true);
        compare(password.text, "sec");

        var oftcChoice = repeaterItemByName(findChild(window, "networkChoiceRepeater"),
                                            "networkChoice-oftc");
        verify(oftcChoice !== null, "Could not find networkChoice-oftc");
        mouseClick(oftcChoice);
        compare(namedConnection.selectedNetworkId, "libera");
        compare(password.text, "sec");
        compare(window.connectionPasswordEdited, true);

        mouseClick(findChild(window, "connectionApply"));
        compare(namedConnection.passwordSetCalls, 1);
        compare(namedConnection.lastPassword, "sec");
        window.close();
        restoreNamedConnection();
    }

    function test_networkSwitchClearsPendingPassword() {
        restoreNamedConnection();
        var window = createTemporaryObject(fallbackWindowComponent, null);
        verify(window !== null, "The password-clear window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        var password = findChild(window, "connectionPassword");
        mouseClick(password);
        keyClick(Qt.Key_S);
        keyClick(Qt.Key_E);
        keyClick(Qt.Key_C);
        compare(window.connectionPasswordEdited, true);

        namedConnection.add();
        waitForRendering(window.contentItem);
        compare(namedConnection.selectedNetworkId, "new-id");
        compare(password.text, "");
        compare(window.connectionPasswordEdited, false);

        mouseClick(findChild(window, "connectionApply"));
        compare(namedConnection.passwordSetCalls, 0);
        window.close();
        restoreNamedConnection();
    }

    function test_openingRioDoesNotDuplicate() {
        mouseClick(item("conversation-oftc-#lab"));
        tryCompare(appWindow, "currentConversation", "#lab");
        var members = item("membersList");
        members.positionViewAtIndex(0, ListView.Contain);
        wait(0);
        var rio = members.itemAtIndex(0);
        verify(rio !== null, "The rio member delegate should be rendered");
        mouseClick(rio);
        tryCompare(appWindow, "currentConversation", "rio");
        compare(item("directConversationRepeater").count, 2);
        compare(findChild(appWindow, "conversation-oftc-rio").conversationName, "rio");
        var extras = item("directConversationRepeater-mock-oftc");
        var extraRio = 0;
        var extraIndex = 0;
        for (extraIndex = 0; extraIndex < extras.count; ++extraIndex) {
            if (extras.itemAt(extraIndex).visible
                    && extras.itemAt(extraIndex).conversationName === "rio")
                extraRio += 1;
        }
        compare(extraRio, 0);
        mouseClick(item("conversation-#omarchy"));
        tryCompare(appWindow, "currentConversationId", "mock-omarchy\n#omarchy");
    }

    function test_mockDirectStaysOnItsNetwork() {
        mouseClick(item("conversation-oftc-#lab"));
        tryCompare(appWindow, "currentConversationId", "mock-oftc\n#lab");
        appWindow.openDirectMessage("kai");
        tryCompare(appWindow, "currentConversation", "kai");
        compare(appWindow.currentConversationId, "mock-oftc\nkai");
        var omarchy = item("directConversationRepeater");
        var shown = 0;
        var index = 0;
        for (index = 0; index < omarchy.count; ++index) {
            if (omarchy.itemAt(index).visible
                    && omarchy.itemAt(index).conversationName === "kai")
                shown += 1;
        }
        compare(shown, 0);
        var oftc = item("directConversationRepeater-mock-oftc");
        var kai = 0;
        for (index = 0; index < oftc.count; ++index) {
            if (oftc.itemAt(index).visible
                    && oftc.itemAt(index).conversationName === "kai")
                kai += 1;
        }
        compare(kai, 1);
        var model = omarchy.model;
        var remove = model.count - 1;
        for (; remove >= 0; --remove) {
            if (model.get(remove).conversation === "kai")
                model.remove(remove);
        }
        mouseClick(item("conversation-#omarchy"));
        tryCompare(appWindow, "currentConversationId", "mock-omarchy\n#omarchy");
    }

    function test_liveUnreadFollowsConversationEpoch() {
        liveIrc.unreadSink = [0, false];
        liveIrc.conversationEpoch = 0;
        var window = createTemporaryObject(liveWindowComponent, null);
        verify(window !== null, "The live unread window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        var mark = findChild(window, "networkUnreadMark");
        verify(mark !== null, "Could not find networkUnreadMark");
        compare(mark.visible, false);

        liveIrc.unreadSink[0] = 4;
        liveIrc.unreadSink[1] = true;
        waitForRendering(window.contentItem);
        compare(mark.visible, false);

        liveIrc.conversationEpoch = 1;
        tryCompare(mark, "visible", true);
        window.close();
        liveIrc.unreadSink = [0, false];
        liveIrc.conversationEpoch = 0;
    }

    function test_statusDraftsStayWithNetwork() {
        var composer = item("messageComposer");
        mouseClick(item("networkHeaderButton"));
        tryCompare(appWindow, "consoleVisible", true);
        mouseClick(composer);
        typeText("omarchy status");
        compare(composer.text, "omarchy status");

        mouseClick(item("networkHeaderButton-mock-oftc"));
        tryCompare(appWindow, "consoleVisible", true);
        compare(appWindow.currentNetworkId, appWindow.mockOftcId);
        compare(composer.text, "");

        typeText("oftc status");
        compare(composer.text, "oftc status");

        mouseClick(item("networkHeaderButton"));
        tryCompare(appWindow, "currentNetworkId", appWindow.mockOmarchyId);
        compare(composer.text, "omarchy status");

        mouseClick(item("networkHeaderButton-mock-oftc"));
        compare(composer.text, "oftc status");
        keyClick(Qt.Key_Escape);
        tryCompare(appWindow, "consoleVisible", false);
        mouseClick(item("conversation-#omarchy"));
    }

    function test_mockSendUsesSelfNick() {
        mouseClick(item("conversation-oftc-#lab"));
        tryCompare(appWindow, "currentConversationId", "mock-oftc\n#lab");
        var messages = item("messageList");
        var previousCount = messages.model.count;
        var composer = item("messageComposer");
        mouseClick(composer);
        typeText("hello oak");
        keyClick(Qt.Key_Return);
        tryCompare(messages.model, "count", previousCount + 1);
        compare(messages.model.get(previousCount).author, "oak");
        compare(messages.model.get(previousCount).body, "hello oak");
        mouseClick(item("conversation-#omarchy"));
    }

    function test_oftcDirectHasOwnHistory() {
        mouseClick(item("conversation-oftc-#lab"));
        tryCompare(appWindow, "currentConversationId", "mock-oftc\n#lab");
        appWindow.openDirectMessage("ness");
        tryCompare(appWindow, "currentConversation", "ness");
        tryCompare(appWindow, "currentConversationId", "mock-oftc\nness");
        waitForRendering(appWindow.contentItem);
        var messages = item("messageList");
        var previousCount = messages.model.count;
        mouseClick(item("messageComposer"));
        typeText("secret to ness");
        keyClick(Qt.Key_Return);
        tryCompare(messages.model, "count", previousCount + 1);

        mouseClick(item("conversation-oftc-#omarchy"));
        tryCompare(appWindow, "currentConversationId", "mock-oftc\n#omarchy");
        var bodies = [];
        var row = 0;
        for (; row < item("messageList").model.count; ++row)
            bodies.push(item("messageList").model.get(row).body);
        compare(bodies.indexOf("secret to ness"), -1);

        var oftcDirects = item("directConversationRepeater-mock-oftc");
        var nessRow = null;
        for (row = 0; row < oftcDirects.count; ++row) {
            var candidate = oftcDirects.itemAt(row);
            if (candidate && candidate.visible && candidate.conversationName === "ness")
                nessRow = candidate;
        }
        verify(nessRow !== null, "ness sidebar row should exist on OFTC");
        mouseClick(nessRow);
        tryCompare(appWindow, "currentConversation", "ness");
        compare(item("messageList").model.get(item("messageList").model.count - 1).body,
                "secret to ness");
        keyClick(Qt.Key_W, Qt.ControlModifier);
        mouseClick(item("conversation-#omarchy"));
    }

    function test_keyboardNavigationRevealsSidebarRow() {
        mouseClick(item("conversation-#omarchy"));
        tryCompare(appWindow, "currentConversationId", "mock-omarchy\n#omarchy");
        var extra = 0;
        for (; extra < 16; ++extra)
            appWindow.openDirectMessage("bulk" + extra);
        mouseClick(item("conversation-#omarchy"));
        tryCompare(appWindow, "currentConversation", "#omarchy");
        waitForRendering(appWindow.contentItem);

        var repeater = item("directConversationRepeater");
        var row = null;
        var index = 0;
        for (; index < repeater.count; ++index) {
            var candidate = repeater.itemAt(index);
            if (candidate && candidate.visible && candidate.conversationName === "bulk15")
                row = candidate;
        }
        verify(row !== null, "bulk15 sidebar row should exist");

        var list = item("sidebarList");
        list.contentY = 0;
        waitForRendering(appWindow.contentItem);

        var hops = 0;
        for (; hops < 40; ++hops) {
            keyClick(Qt.Key_Down, Qt.AltModifier);
            if (appWindow.currentConversation === "bulk15")
                break;
        }
        compare(appWindow.currentConversation, "bulk15");
        waitForRendering(appWindow.contentItem);
        var mapped = row.mapToItem(list, 0, 0);
        verify(mapped.y >= -1, "selected row should not sit above the sidebar");
        verify(mapped.y + row.height <= list.height + 2,
               "selected row should sit inside the sidebar");

        var model = item("directConversationRepeater").model;
        var remove = model.count - 1;
        for (; remove >= 0; --remove) {
            if (String(model.get(remove).conversation).indexOf("bulk") === 0)
                model.remove(remove);
        }
        mouseClick(item("conversation-#omarchy"));
    }

    function test_networkChoiceIsKeyboardAccessible() {
        restoreNamedConnection();
        namedNetworks.append({
            networkId: "oftc",
            displayName: "irc.oftc.net",
            stored: true,
            selected: false
        });
        var window = createTemporaryObject(fallbackWindowComponent, null);
        verify(window !== null, "The accessible rail window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        var libera = repeaterItemByName(findChild(window, "networkChoiceRepeater"),
                                        "networkChoice-libera");
        var oftc = repeaterItemByName(findChild(window, "networkChoiceRepeater"),
                                      "networkChoice-oftc");
        verify(libera !== null, "Could not find networkChoice-libera");
        verify(oftc !== null, "Could not find networkChoice-oftc");
        compare(libera.Accessible.role, Accessible.Button);
        compare(oftc.Accessible.role, Accessible.Button);
        oftc.forceActiveFocus();
        tryCompare(oftc, "activeFocus", true);
        keyClick(Qt.Key_Return);
        compare(namedConnection.selectedNetworkId, "oftc");
        window.close();
        restoreNamedConnection();
    }
}
