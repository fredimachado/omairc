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
        property bool hasAwayPresence: true
        property bool hasMemberStatus: true
        property bool hasTyping: false
        property var typingNicks: []
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
        property bool hasAwayPresence: false
        property bool hasMemberStatus: false
        property bool hasTyping: false
        property var typingNicks: ["anna"]
        property var conversations: liveConversations
        property var messages: liveMessages
        property var members: gatedMembers
        property var statusConsole: liveConsole

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
        property bool hasAwayPresence: true
        property bool hasMemberStatus: true
        property bool hasTyping: false
        property var typingNicks: []
        property var conversations: liveConversations
        property var messages: liveMessages
        property var members: prefixedMembers
        property var statusConsole: liveConsole

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
                { label: "/join", usage: "/join <channel>" },
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
    }

    function cleanup() {
        if (appWindow)
            appWindow.close();
        appWindow = null;
        slashFake.reset();
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

        tryCompare(appWindow, "currentConversation", "dax");
        compare(appWindow.currentTopic, "Direct message with dax");
        compare(item("messageList").Accessible.name, "Messages in dax");
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
        compare(appWindow.title, "Status");
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
}
