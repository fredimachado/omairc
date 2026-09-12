import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Window
import QtTest
import Omairc.Test 1.0
import "../../src" as Omairc

TestCase {
    id: testCase

    name: "OmaircUi"
    when: windowShown

    property var appWindow
    property var seed
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
            iconColor: 1
        }
    }

    ListModel {
        id: namedNetworks
        ListElement {
            networkId: "libera"
            displayName: "irc.libera.chat"
            stored: true
            selected: true
            iconColor: 1
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
        property string account: ""
        property string bouncerNetwork: ""
        property string autojoin: "#omarchy"
        property bool passwordSet: false
        property bool nickServSet: false
        property string credentialError: ""
        property string credentialStatus: "secure storage unavailable"
        property bool canForgetPassword: false
        property bool canForgetNickServ: false
        property string problem: "Nick is required"
        property bool dirty: true
        property string displayName: "irc.libera.chat"
        property bool setupRequired: true
        property bool focusPassword: false
        property bool focusNickServ: false
        property var networks: setupNetworks
        property string selectedNetworkId: "setup-id"
        property bool canAdd: false
        property bool canRemove: false

        property int setPasswordCalls: 0
        property string lastSetPassword: ""
        property int forgetPasswordCalls: 0
        property int removeStoredPasswordCalls: 0
        property int setNickServPasswordCalls: 0
        property string lastSetNickServPassword: ""
        property int forgetNickServCalls: 0
        property int removeStoredNickServCalls: 0

        function setPassword(password) {
            setPasswordCalls += 1;
            lastSetPassword = password;
        }

        function setNickServPassword(password) {
            setNickServPasswordCalls += 1;
            lastSetNickServPassword = password;
        }

        function forgetPassword() {
            forgetPasswordCalls += 1;
        }

        function forgetNickServ() {
            forgetNickServCalls += 1;
        }

        function removeStoredPassword() {
            removeStoredPasswordCalls += 1;
        }

        function removeStoredNickServ() {
            removeStoredNickServCalls += 1;
        }

        property int applyCalls: 0

        function apply() {
            applyCalls += 1;
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
            irc: liveIrc
        }
    }

    Component {
        id: seedComponent

        SeededIrcFixture {
        }
    }

    Component {
        id: seededWindowComponent

        Omairc.OmaircWindow {
        }
    }

    Component {
        id: setupWindowComponent

        Omairc.OmaircWindow {
            backend: fakeBackend
            irc: liveIrc
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
            return text.length > 0;
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
        property string account: ""
        property string bouncerNetwork: ""
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
        property int applyCalls: 0
        property bool applySucceeds: false
        property int discardCalls: 0
        property var addedFromSnapshot: null

        signal selectedNetworkChanged()

        function setPassword(password) {
            passwordSetCalls += 1;
            lastPassword = password;
        }

        function apply() {
            applyCalls += 1;
            return applySucceeds && problem.length === 0;
        }

        function discard() {
            discardCalls += 1;
            if (!addedFromSnapshot)
                return;
            var row = namedNetworks.count - 1;
            for (; row >= 0; --row) {
                if (namedNetworks.get(row).networkId === "new-id") {
                    namedNetworks.remove(row);
                    break;
                }
            }
            var snap = addedFromSnapshot;
            addedFromSnapshot = null;
            selectedNetworkId = snap.networkId;
            host = snap.host;
            nick = snap.nick;
            displayName = snap.displayName;
            dirty = false;
            problem = "";
            canAdd = true;
            canRemove = true;
            for (row = 0; row < namedNetworks.count; ++row)
                namedNetworks.setProperty(row, "selected",
                    namedNetworks.get(row).networkId === snap.networkId);
            selectedNetworkChanged();
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
            addedFromSnapshot = {
                networkId: selectedNetworkId,
                host: host,
                nick: nick,
                displayName: displayName
            };
            for (var row = 0; row < namedNetworks.count; ++row)
                namedNetworks.setProperty(row, "selected", false);
            namedNetworks.append({
                networkId: "new-id",
                displayName: "New network",
                stored: false,
                selected: true,
                iconColor: 1
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
            irc: liveIrc
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

    function destroyAppWindowAndSeed() {
        // The window's onDestruction calls backend.saveWindowGeometry.
        // Destroy the window while SeededIrcFixture still owns Backend.
        if (appWindow) {
            appWindow.close();
            appWindow.destroy();
            appWindow = null;
            wait(0);
        }
        if (seed) {
            seed.destroy();
            seed = null;
        }
    }

    function cleanup() {
        destroyAppWindowAndSeed();
        slashFake.reset();
        liveConsole.open = false;
        liveConsole.networkId = "libera";
    }

    function field(model, row, name) {
        return model.field(row, name);
    }

    function liveConversation(name) {
        return "conversation-" + seed.omarchyNetworkId + "-" + name;
    }

    function liveOftcConversation(name) {
        return "conversation-" + seed.oftcNetworkId + "-" + name;
    }

    function liveHeader(networkId) {
        return namedItem("networkHeader-" + networkId);
    }

    function liveHeaderButton(networkId) {
        return namedItem("networkHeaderButton-" + networkId);
    }

    function statusTitle(networkId) {
        return networkDisplayName(networkId) + " Status";
    }

    function findNamed(objectName) {
        // TestCase.findChild does not see Repeater conversation rows with live names.
        var visible = null;
        var any = null;
        function walk(node) {
            if (!node)
                return;
            if (node.objectName === objectName) {
                if (!any)
                    any = node;
                if (!visible && node.visible && node.width > 0 && node.height > 0)
                    visible = node;
            }
            var kids = node.children;
            if (kids) {
                var index = 0;
                for (; index < kids.length; ++index)
                    walk(kids[index]);
            }
            if (node.contentItem)
                walk(node.contentItem);
        }
        walk(appWindow.contentItem);
        return visible ? visible : any;
    }

    function namedItem(objectName) {
        var result = findNamed(objectName);
        verify(result !== null, "Could not find " + objectName);
        return result;
    }

    function visibleDirects(networkId) {
        var names = [];
        var seen = [];
        function walk(node) {
            if (!node || seen.indexOf(node) !== -1)
                return;
            seen.push(node);
            if (node.direct === true && node.visible && node.height > 0
                    && node.conversationName
                    && node.networkId === networkId)
                names.push(node.conversationName);
            var kids = node.children;
            if (kids) {
                var index = 0;
                for (; index < kids.length; ++index)
                    walk(kids[index]);
            }
            if (node.contentItem)
                walk(node.contentItem);
        }
        walk(appWindow.contentItem);
        return names;
    }

    function memberIndex(nick) {
        var members = item("membersList");
        var row = 0;
        for (; row < members.count; ++row) {
            members.positionViewAtIndex(row, ListView.Contain);
            waitForRendering(appWindow.contentItem);
            var delegate = members.itemAtIndex(row);
            if (delegate && delegate.nick === nick)
                return row;
        }
        fail("Could not find member " + nick);
        return -1;
    }

    function openSeededAppWindow() {
        destroyAppWindowAndSeed();
        seed = createTemporaryObject(seedComponent, testCase);
        verify(seed !== null, "SeededIrcFixture should construct");
        verify(seed.open(), seed.lastError);
        verify(seed.connection, "seeded window needs a real IrcConnection");
        appWindow = createTemporaryObject(seededWindowComponent, testCase, {
            backend: seed.backend,
            irc: seed.irc,
            slashCommands: seed.slash,
            connection: seed.connection
        });
        verify(appWindow !== null, "The seeded Omairc window should load");
        tryCompare(appWindow, "visible", true);
        waitForRendering(appWindow.contentItem);
        appWindow.suppressExternalUrlOpen = true;
        appWindow.lastOpenedUrl = "";
        appWindow.suppressDesktopNotification = true;
        appWindow.lastNotification = null;
        tryVerify(function() {
            return appWindow.currentConversation === "#omarchy"
                && !appWindow.consoleVisible
                && !appWindow.connectionOverlayVisible;
        });
    }

    function clickMember(nick) {
        var members = item("membersList");
        var row = 0;
        for (; row < members.count; ++row) {
            members.positionViewAtIndex(row, ListView.Contain);
            waitForRendering(appWindow.contentItem);
            var delegate = members.itemAtIndex(row);
            if (delegate && delegate.objectName === "member-" + nick) {
                mouseClick(delegate);
                return;
            }
        }
        fail("Could not find member-" + nick);
    }

    function restoreNamedConnection() {
        namedNetworks.clear();
        namedNetworks.append({
            networkId: "libera",
            displayName: "irc.libera.chat",
            stored: true,
            selected: true,
            iconColor: 1
        });
        namedConnection.selectedNetworkId = "libera";
        namedConnection.displayName = "irc.libera.chat";
        namedConnection.host = "irc.libera.chat";
        namedConnection.nick = "sheet-nick";
        namedConnection.passwordSetCalls = 0;
        namedConnection.lastPassword = "";
        namedConnection.applyCalls = 0;
        namedConnection.applySucceeds = false;
        namedConnection.discardCalls = 0;
        namedConnection.addedFromSnapshot = null;
        namedConnection.dirty = false;
        namedConnection.problem = "";
        namedConnection.setupRequired = false;
        namedConnection.canAdd = true;
        namedConnection.canRemove = true;
        namedConnection.account = "";
        namedConnection.bouncerNetwork = "";
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

    function focusObjectName(window) {
        var item = window.activeFocusItem;
        while (item) {
            if (item.objectName && item.objectName.length > 0)
                return item.objectName;
            item = item.parent;
        }
        return "";
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

    function openJumpSheet() {
        keyClick(Qt.Key_K, Qt.ControlModifier);
        var sheet = item("jumpSheet");
        tryCompare(sheet, "opened", true);
        tryCompare(item("jumpFilter"), "activeFocus", true);
        return sheet;
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
        openSeededAppWindow();
        mouseClick(namedItem(liveConversation("#desktop")));

        tryCompare(appWindow, "currentConversation", "#desktop");
        compare(appWindow.currentTopic,
                "Desktops should feel personal, fast, and calm.");
        compare(appWindow.currentPeopleCount, 8);
        compare(item("messageList").Accessible.name, "Messages in #desktop");
        saveScreenshot("switch-channel");
    }

    function test_sendMessageWithKeyboard() {
        openSeededAppWindow();
        var composer = item("messageComposer");
        var messages = item("messageList");
        var previousCount = messages.model.rowCount();

        mouseClick(composer);
        verify(composer.activeFocus);
        typeText("Hello from the UI test");
        compare(composer.text, "Hello from the UI test");
        keyClick(Qt.Key_Return);
        verify(seed.echoLastOmarchyPrivmsg());

        tryVerify(function() {
            return messages.model.rowCount() === previousCount + 1;
        });
        compare(field(messages.model, previousCount, "author"), "fred");
        compare(field(messages.model, previousCount, "body"), "Hello from the UI test");
        compare(composer.text, "");
        saveScreenshot("send-message");
    }

    function test_toggleStatusWithShortcut() {
        openSeededAppWindow();
        compare(appWindow.consoleVisible, false);

        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);

        tryCompare(appWindow, "consoleVisible", true);
        compare(appWindow.title, statusTitle(seed.omarchyNetworkId));
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
        openSeededAppWindow();
        compare(appWindow.currentConversation, "#omarchy");

        keyClick(Qt.Key_Down, Qt.AltModifier);

        tryCompare(appWindow, "currentConversation", "#ricing");
        compare(appWindow.currentTopic,
                "Themes, type, wallpapers, and the tiny details.");
        compare(appWindow.currentPeopleCount, 10);
        compare(item("messageList").Accessible.name, "Messages in #ricing");
        tryCompare(item("messageComposer"), "activeFocus", true);

        keyClick(Qt.Key_Down, Qt.AltModifier);

        tryCompare(appWindow, "currentConversation", "anna");
        compare(appWindow.currentTopic, "Direct message with anna");
        compare(item("messageList").Accessible.name, "Messages in anna");
    }

    function test_walkConversationsWrapsToLast() {
        openSeededAppWindow();
        mouseClick(namedItem(liveConversation("#desktop")));
        tryCompare(appWindow, "currentConversation", "#desktop");

        keyClick(Qt.Key_Up, Qt.AltModifier);

        tryCompare(appWindow, "currentConversation", "rio");
        compare(appWindow.currentTopic, "Direct message with rio");
        compare(item("messageList").Accessible.name, "Messages in rio");
        compare(namedItem(liveOftcConversation("rio")).current, true);
    }

    function test_walkConversationsFromChannelToDirect() {
        openSeededAppWindow();
        mouseClick(namedItem(liveConversation("#ricing")));
        tryCompare(appWindow, "currentConversation", "#ricing");

        keyClick(Qt.Key_Down, Qt.AltModifier);

        tryCompare(appWindow, "currentConversation", "anna");
        compare(appWindow.currentTopic, "Direct message with anna");
        compare(item("messageList").Accessible.name, "Messages in anna");
    }

    function test_walkConversationsClosesStatus() {
        openSeededAppWindow();
        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);

        keyClick(Qt.Key_Down, Qt.AltModifier);

        tryCompare(appWindow, "consoleVisible", false);
        compare(appWindow.currentConversation, "#ricing");
        compare(item("messageList").Accessible.name, "Messages in #ricing");
    }

    function test_jumpToNextUnreadPrefersMention() {
        openSeededAppWindow();
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
        openSeededAppWindow();
        mouseClick(namedItem(liveConversation("#desktop")));
        tryCompare(appWindow, "currentConversation", "#desktop");

        keyClick(Qt.Key_A, Qt.AltModifier);

        tryCompare(appWindow, "currentConversation", "#ricing");
    }

    function test_jumpToNextUnreadFallsBackToUnread() {
        openSeededAppWindow();
        mouseClick(namedItem(liveConversation("#ricing")));
        tryCompare(appWindow, "currentConversation", "#ricing");

        mouseClick(namedItem(liveConversation("anna")));
        tryCompare(appWindow, "currentConversation", "anna");

        mouseClick(namedItem(liveConversation("#omarchy")));
        tryCompare(appWindow, "currentConversation", "#omarchy");

        keyClick(Qt.Key_A, Qt.AltModifier);

        tryCompare(appWindow, "currentConversation", "#build");
        compare(namedItem(liveOftcConversation("#build")).current, true);
    }

    function test_tabCompletesChannelNick() {
        openSeededAppWindow();
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
        openSeededAppWindow();
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
        openSeededAppWindow();
        var composer = item("messageComposer");
        mouseClick(composer);
        verify(composer.activeFocus);

        typeText("alpha");
        keyClick(Qt.Key_Return);
        verify(seed.echoLastOmarchyPrivmsg());
        compare(composer.text, "");

        typeText("beta");
        keyClick(Qt.Key_Return);
        verify(seed.echoLastOmarchyPrivmsg());
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
        openSeededAppWindow();
        var composer = item("messageComposer");
        mouseClick(composer);
        verify(composer.activeFocus);
        compare(appWindow.currentConversation, "#omarchy");

        typeText("omarchy draft");
        compare(composer.text, "omarchy draft");

        mouseClick(namedItem(liveConversation("#desktop")));
        tryCompare(appWindow, "currentConversation", "#desktop");
        compare(composer.text, "");

        typeText("desktop draft");
        compare(composer.text, "desktop draft");

        mouseClick(namedItem(liveConversation("#omarchy")));
        tryCompare(appWindow, "currentConversation", "#omarchy");
        compare(composer.text, "omarchy draft");

        mouseClick(namedItem(liveConversation("#desktop")));
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

        mouseClick(namedItem(liveConversation("#omarchy")));
        tryCompare(appWindow, "currentConversation", "#omarchy");
        compare(composer.text, "omarchy draft");

        var messages = item("messageList");
        var previousCount = messages.model.rowCount();
        keyClick(Qt.Key_Return);
        verify(seed.echoLastOmarchyPrivmsg());
        compare(composer.text, "");
        tryVerify(function() {
            return messages.model.rowCount() === previousCount + 1;
        });
        compare(field(messages.model, previousCount, "author"), "fred");
        compare(field(messages.model, previousCount, "body"), "omarchy draft");

        keyClick(Qt.Key_Up);
        compare(composer.text, "omarchy draft");
        keyClick(Qt.Key_Down);
        compare(composer.text, "");

        mouseClick(namedItem(liveConversation("#desktop")));
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
            var hay = (field(list.model, index, "body")
                || field(list.model, index, "text")
                || "").toLowerCase();
            if (hay.indexOf(lower) >= 0)
                return index;
        }
        return -1;
    }

    function networkDisplayName(networkId) {
        var model = seed.connection.networks;
        var row = 0;
        for (; row < model.rowCount(); ++row) {
            var idx = model.index(row, 0);
            if (model.data(idx, Qt.UserRole + 1) === networkId)
                return model.data(idx, Qt.UserRole + 2) || "";
        }
        return "";
    }

    function injectOmarchyChat(nick, target, body, time) {
        var prefix = time ? "@time=2026-09-12T" + time + ":00.000Z " : "";
        seed.injectOmarchy(prefix + ":" + nick + "!u@h PRIVMSG " + target
            + " :" + body + "\r\n");
    }

    function waitForRowCount(list, expected) {
        tryVerify(function() {
            return list.model.rowCount() === expected;
        });
    }

    function findMatchAt(list, index) {
        var row = list.itemAtIndex(index);
        if (!row)
            return null;
        return findChild(row, "findMatch");
    }

    function test_ctrlFFindsTextInConversation() {
        openSeededAppWindow();
        var composer = item("messageComposer");
        var list = item("messageList");
        injectOmarchyChat("anna", "#omarchy", "unique-find one");
        injectOmarchyChat("dax", "#omarchy", "unique-find two");
        injectOmarchyChat("mira", "#omarchy", "unique-find three");
        fillTranscriptUntilScrollable(list);
        appendLiveMessages(list, 24, "find filler");
        waitForRendering(appWindow.contentItem);
        list.pinToEnd();
        waitForRendering(appWindow.contentItem);
        wait(0);
        var pinnedY = list.contentY;
        verify(pinnedY > 0);

        compare(composer.text, "");
        keyClick(Qt.Key_F, Qt.ControlModifier);
        tryCompare(appWindow, "findActive", true);
        compare(composer.placeholderText, "Find");
        compare(appWindow.findIndex, -1);
        keyClick(Qt.Key_Escape);
        tryCompare(appWindow, "findActive", false);
        compare(composer.text, "");

        typeText("keep me");
        compare(composer.text, "keep me");
        keyClick(Qt.Key_F, Qt.ControlModifier);
        tryCompare(appWindow, "findActive", true);
        compare(composer.text, "keep me");
        verify(composer.activeFocus);

        typeText("unique-find");
        compare(composer.text, "unique-find");
        tryVerify(function() {
            return list.contentY < pinnedY && appWindow.findIndex >= 0;
        }, 1000, "Ctrl+F should jump the list to the match");
        var first = appWindow.findIndex;
        verify(first >= 0, "The first unique-find row should be current");
        verify(field(list.model, first, "body").toLowerCase().indexOf("unique-find") >= 0);
        verify(visibleMatchIndex(list, "unique-find") >= 0,
               "The first unique-find row should be in view");
        var firstMark = findMatchAt(list, first);
        verify(firstMark && firstMark.visible, "The current match row should highlight");

        var countBefore = list.model.rowCount();
        keyClick(Qt.Key_Return);
        compare(list.model.rowCount(), countBefore);
        waitForRendering(appWindow.contentItem);
        wait(0);
        var second = appWindow.findIndex;
        verify(second !== first, "Enter in find should go to the next match");
        verify(second > first, "Enter in find should go to a later match first");
        compare(composer.text, "unique-find");

        var current = second;
        var hops = 0;
        while (current !== first) {
            keyClick(Qt.Key_F, Qt.ControlModifier);
            waitForRendering(appWindow.contentItem);
            wait(0);
            current = appWindow.findIndex;
            hops += 1;
            verify(hops < list.count, "Find should wrap back to the first match");
        }
        verify(hops >= 1);
        compare(list.model.rowCount(), countBefore);

        composer.selectAll();
        typeText("kai");
        tryVerify(function() {
            var index = appWindow.findIndex;
            return index >= 0 && field(list.model, index, "author") === "kai";
        }, 1000, "Find should match an author nick that is not in the body");
        verify(field(list.model, appWindow.findIndex, "body").toLowerCase().indexOf("kai") < 0);
        var authorMark = findMatchAt(list, appWindow.findIndex);
        verify(authorMark && authorMark.visible);

        keyClick(Qt.Key_Escape);
        tryCompare(appWindow, "findActive", false);
        compare(composer.text, "keep me");
        verify(composer.activeFocus);

        keyClick(Qt.Key_Return);
        verify(seed.echoLastOmarchyPrivmsg());
        compare(composer.text, "");
        waitForRowCount(list, countBefore + 1);
        compare(field(list.model, countBefore, "body"), "keep me");
    }

    function test_ctrlFFindsTextInStatus() {
        openSeededAppWindow();
        var composer = item("messageComposer");
        var list = item("consoleList");
        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);
        fillConsoleUntilScrollable(list);
        var pinnedY = list.contentY;
        verify(pinnedY > 0);

        mouseClick(composer);
        verify(composer.activeFocus);
        compare(composer.text, "");
        keyClick(Qt.Key_F, Qt.ControlModifier);
        tryCompare(appWindow, "findActive", true);
        compare(composer.placeholderText, "Find");
        compare(appWindow.findIndex, -1);
        keyClick(Qt.Key_Escape);
        tryCompare(appWindow, "findActive", false);

        typeText("hostname");
        keyClick(Qt.Key_F, Qt.ControlModifier);
        tryCompare(appWindow, "findActive", true);
        waitForRendering(appWindow.contentItem);
        wait(0);

        verify(list.contentY < pinnedY, "Ctrl+F should jump Status to the match");
        var match = visibleMatchIndex(list, "hostname");
        verify(match >= 0, "The hostname Status line should be in view");
        verify(field(list.model, match, "text").toLowerCase().indexOf("hostname") >= 0);
        compare(composer.text, "hostname");
        compare(appWindow.findIndex, match);
        var statusMark = findMatchAt(list, match);
        verify(statusMark && statusMark.visible);

        composer.selectAll();
        typeText("NOTICE");
        var noticeRow = -1;
        var row = 0;
        for (; row < list.model.rowCount(); ++row) {
            if (field(list.model, row, "label") === "NOTICE") {
                noticeRow = row;
                break;
            }
        }
        verify(noticeRow >= 0, "Status should keep a NOTICE line");
        tryCompare(appWindow, "findIndex", noticeRow);
        compare(field(list.model, noticeRow, "label"), "NOTICE");
        verify(field(list.model, noticeRow, "text").indexOf("NOTICE") < 0);
        var labelMark = findMatchAt(list, noticeRow);
        verify(labelMark && labelMark.visible);

        keyClick(Qt.Key_Escape);
        tryCompare(appWindow, "findActive", false);
        compare(composer.text, "hostname");
        compare(appWindow.consoleVisible, true);
    }

    function verticalScrollBar(list) {
        var bar = list.Controls.ScrollBar.vertical;
        verify(bar !== null && bar !== undefined,
               "The list should attach a vertical scrollbar");
        return bar;
    }

    function waitForScrollbarThumb(list, shown) {
        var bar = verticalScrollBar(list);
        var thumb = bar.contentItem;
        verify(thumb !== null, "The scrollbar should have a thumb");
        if (shown) {
            verify(list.contentHeight > list.height);
            tryVerify(function() {
                return thumb.opacity > 0.5 && bar.size < 1;
            }, 1000, "An overflowing transcript should show a scrollbar thumb");
        } else {
            tryVerify(function() {
                return thumb.opacity < 0.1 || bar.size >= 1;
            }, 1000, "A short transcript should not show a scrollbar thumb");
        }
        return bar;
    }

    function pageTranscriptToEnd(list) {
        var hops = 0;
        while (!transcriptPinned(list)) {
            keyClick(Qt.Key_PageDown);
            waitForRendering(appWindow.contentItem);
            wait(0);
            hops += 1;
            verify(hops < 40, "Page Down should reach the end of the transcript");
        }
    }

    function test_pageUpScrollsTranscript() {
        openSeededAppWindow();
        var composer = item("messageComposer");
        var list = item("messageList");
        mouseClick(composer);
        verify(composer.activeFocus);
        fillTranscriptUntilScrollable(list);

        verify(list.contentHeight > list.height);
        var before = list.contentY;
        verify(before > 0);
        var bar = waitForScrollbarThumb(list, true);
        var endPosition = bar.position;

        keyClick(Qt.Key_PageUp);

        verify(list.contentY < before, "Page Up should scroll toward older lines");
        verify(composer.activeFocus);
        waitForScrollbarThumb(list, true);
        verify(bar.position < endPosition, "Page Up should move the thumb up");

        var afterUp = list.contentY;
        keyClick(Qt.Key_PageDown);
        waitForRendering(appWindow.contentItem);
        wait(0);
        verify(list.contentY > afterUp, "Page Down should scroll toward newer lines");
        verify(composer.activeFocus);

        pageTranscriptToEnd(list);
        waitForScrollbarThumb(list, true);
        tryVerify(function() {
            return Math.abs((bar.position + bar.size) - 1) < 0.05;
        }, 1000, "Page Down should reach the end of the transcript");
        verify(composer.activeFocus);
    }

    function test_pageUpShowsStatusScrollbar() {
        openSeededAppWindow();
        var composer = item("messageComposer");
        var list = item("consoleList");
        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);
        fillConsoleUntilScrollable(list);
        mouseClick(composer);
        verify(composer.activeFocus);

        var before = list.contentY;
        var bar = waitForScrollbarThumb(list, true);
        var endPosition = bar.position;

        keyClick(Qt.Key_PageUp);
        verify(list.contentY < before, "Page Up should scroll Status toward older lines");
        verify(composer.activeFocus);
        waitForScrollbarThumb(list, true);
        verify(bar.position < endPosition, "Page Up should move the Status thumb up");

        pageTranscriptToEnd(list);
        waitForScrollbarThumb(list, true);
        fuzzyCompare(bar.position + bar.size, 1, 0.05);
        verify(composer.activeFocus);
        compare(appWindow.consoleVisible, true);
    }

    function test_shortTranscriptHidesScrollbar() {
        openSeededAppWindow();
        mouseClick(namedItem(liveConversation("#help")));
        tryCompare(appWindow, "currentConversation", "#help");
        var list = item("messageList");
        waitForRendering(appWindow.contentItem);
        verify(list.contentHeight <= list.height);
        waitForScrollbarThumb(list, false);
        verify(item("messageComposer").activeFocus);
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

    function fillTranscriptUntilScrollable(list) {
        var composer = item("messageComposer");
        mouseClick(composer);
        verify(composer.activeFocus);
        var start = list.model.rowCount();
        var target = appWindow.currentConversation;
        var index = 0;
        for (index = 0; index < 24; ++index) {
            var minute = index < 10 ? "0" + index : "" + index;
            injectOmarchyChat("anna", target, "scroll line " + index, "10:" + minute);
        }
        waitForRowCount(list, start + 24);
        waitForRendering(appWindow.contentItem);
        list.pinToEnd();
        waitForRendering(appWindow.contentItem);
        wait(0);
        verify(list.contentHeight > list.height);
        verify(transcriptPinned(list));
    }

    function appendLiveMessages(list, count, bodyPrefix) {
        var start = list.model.rowCount();
        var target = appWindow.currentConversation;
        var index = 0;
        for (index = 0; index < count; ++index)
            injectOmarchyChat("anna", target, bodyPrefix + " " + index);
        waitForRowCount(list, start + count);
    }

    function appendLiveConsoleLines(list, count, textPrefix) {
        var start = list.model.rowCount();
        var index = 0;
        for (index = 0; index < count; ++index) {
            seed.injectOmarchy(":server 404 fred #omarchy :" + textPrefix
                + " " + index + "\r\n");
        }
        waitForRowCount(list, start + count);
    }

    function unseenIsInView(list, unseen) {
        if (firstVisibleIndex(list) === unseen)
            return true;
        var row = list.itemAtIndex(unseen);
        if (!row)
            return false;
        return row.y + row.height > list.contentY && row.y < list.contentY + list.height;
    }

    function fillConsoleUntilScrollable(list) {
        appendLiveConsoleLines(list, 40, "console fill");
        waitForRendering(appWindow.contentItem);
        list.pinToEnd();
        waitForRendering(appWindow.contentItem);
        wait(0);
        verify(list.contentHeight > list.height);
        verify(transcriptPinned(list));
    }

    function test_followingAppendAndSendKeepTranscriptPinned() {
        openSeededAppWindow();
        var list = item("messageList");
        fillTranscriptUntilScrollable(list);

        var previousCount = list.model.rowCount();
        var previousY = list.contentY;
        injectOmarchyChat("anna", "#omarchy", "incoming while following");
        waitForRowCount(list, previousCount + 1);
        waitForRendering(appWindow.contentItem);
        wait(0);
        verify(transcriptPinned(list), "Incoming rows should keep a following list at the end");
        verify(list.contentY >= previousY);
        compare(item("messageUnseenJump").visible, false);

        previousCount = list.model.rowCount();
        previousY = list.contentY;
        typeText("sent while following");
        keyClick(Qt.Key_Return);
        verify(seed.echoLastOmarchyPrivmsg());
        waitForRowCount(list, previousCount + 1);
        waitForRendering(appWindow.contentItem);
        wait(0);
        verify(transcriptPinned(list), "Sending should keep the list pinned to the end");
        verify(list.contentY >= previousY);
        compare(item("messageUnseenJump").visible, false);
    }

    function test_followingAppendStaysPinnedThroughLayout() {
        openSeededAppWindow();
        var list = item("messageList");
        fillTranscriptUntilScrollable(list);

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

        var previousCount = list.model.rowCount();
        injectOmarchyChat("anna", "#omarchy", "incoming layout pin");
        waitForRowCount(list, previousCount + 1);
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
        openSeededAppWindow();
        var list = item("messageList");
        fillTranscriptUntilScrollable(list);

        keyClick(Qt.Key_PageUp);
        waitForRendering(appWindow.contentItem);
        tryCompare(list, "stick", 1);
        verify(!transcriptPinned(list));

        var frozenY = list.contentY;
        var previousCount = list.model.rowCount();
        appendLiveMessages(list, 1, "first unseen");
        waitForRendering(appWindow.contentItem);
        wait(0);

        fuzzyCompare(list.contentY, frozenY, 2);
        compare(list.firstUnseenIndex, previousCount);
        var jump = item("messageUnseenJump");
        tryCompare(jump, "visible", true);

        appendLiveMessages(list, 24, "later unseen");
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
        openSeededAppWindow();
        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);

        var list = item("consoleList");
        verify(list.visible);
        fillConsoleUntilScrollable(list);

        keyClick(Qt.Key_PageUp);
        waitForRendering(appWindow.contentItem);
        tryCompare(list, "stick", 1);
        verify(!transcriptPinned(list));

        var frozenY = list.contentY;
        var previousCount = list.model.rowCount();
        appendLiveConsoleLines(list, 1, "first unseen console");
        waitForRendering(appWindow.contentItem);
        wait(0);

        fuzzyCompare(list.contentY, frozenY, 2);
        compare(list.firstUnseenIndex, previousCount);
        var jump = item("consoleUnseenJump");
        tryCompare(jump, "visible", true);

        appendLiveConsoleLines(list, 40, "later unseen console");
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
        openSeededAppWindow();
        var list = item("messageList");
        var start = list.model.rowCount();
        injectOmarchyChat("anna", "#omarchy", "group lead", "11:11");
        injectOmarchyChat("anna", "#omarchy", "group continuation", "11:11");
        injectOmarchyChat("anna", "#omarchy", "changed minute", "11:12");
        injectOmarchyChat("dax", "#omarchy", "changed sender", "11:12");
        seed.injectOmarchy(":rio!u@h PART #omarchy\r\n");
        injectOmarchyChat("dax", "#omarchy", "after event", "11:12");
        waitForRowCount(list, start + 6);

        compare(field(list.model, start + 1, "author"), "anna");
        compare(field(list.model, start + 1, "time"), field(list.model, start, "time"));
        compare(field(list.model, start + 1, "body"), "group continuation");
        compare(field(list.model, start + 1, "kind"), "message");

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
        compare(eventText.text, "rio left");
        compare(findChild(eventRow, "messageAvatar").visible, false);
        compare(findChild(eventRow, "messageHeader").visible, false);
        compare(findChild(eventRow, "messageBody").visible, false);

        assertMessageChrome(afterEvent, true, "after event");
        saveScreenshot("grouped-messages");

        mouseClick(namedItem(liveConversation("anna")));
        tryCompare(appWindow, "currentConversation", "anna");
        waitForRendering(appWindow.contentItem);

        var dmList = item("messageList");
        compare(dmList.Accessible.name, "Messages in anna");
        var dmStart = dmList.model.rowCount();
        compare(field(dmList.model, dmStart - 1, "author"), "anna");
        compare(field(dmList.model, dmStart - 1, "body"),
                "fred: The prototype already feels at home. Nice work.");
        injectOmarchyChat("anna", "fred", "dm continuation", "10:12");
        injectOmarchyChat("anna", "fred", "dm new minute", "10:13");
        waitForRowCount(dmList, dmStart + 2);

        var dmExisting = renderedMessageRow(dmList, dmStart - 1);
        var dmGrouped = renderedMessageRow(dmList, dmStart);
        var dmLead = renderedMessageRow(dmList, dmStart + 1);
        assertMessageChrome(dmExisting, true, "fred: The prototype already feels at home. Nice work.");
        assertMessageChrome(dmGrouped, false, "dm continuation");
        assertMessageChrome(dmLead, true, "dm new minute");
        verify(dmGrouped.height < dmExisting.height);
        compare(field(dmList.model, dmStart, "author"), "anna");
        compare(field(dmList.model, dmStart, "time"),
                field(dmList.model, dmStart - 1, "time"));
        compare(field(dmList.model, dmStart, "body"), "dm continuation");
    }

    function test_replayAndLiveSameAuthorMinuteDoNotGroup() {
        openSeededAppWindow();
        mouseClick(namedItem(liveConversation("anna")));
        tryCompare(appWindow, "currentConversation", "anna");
        waitForRendering(appWindow.contentItem);
        var list = item("messageList");
        var start = list.model.rowCount();
        seed.injectOmarchy(
            ":znc.in BATCH +hx znc.in/playback anna\r\n"
            + "@batch=hx;time=2011-10-19T16:40:51.620Z;msgid=old :anna!u@h PRIVMSG fred :replayed line\r\n"
            + ":znc.in BATCH -hx\r\n"
            + "@time=2011-10-19T16:40:51.620Z :anna!u@h PRIVMSG fred :live line\r\n");
        waitForRowCount(list, start + 2);

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

    function prependLiveReplay(list, count) {
        var start = list.model.rowCount();
        var target = appWindow.currentConversation;
        var bytes = ":znc.in BATCH +pb znc.in/playback " + target + "\r\n";
        var index = 0;
        for (index = 0; index < count; ++index) {
            bytes += "@batch=pb;time=2026-09-11T09:00:00.000Z;msgid=replay-"
                + index + " :anna!u@h PRIVMSG " + target + " :replayed "
                + index + "\r\n";
        }
        bytes += ":znc.in BATCH -pb\r\n";
        seed.injectOmarchy(bytes);
        waitForRowCount(list, start + count);
    }

    function test_historySpliceKeepsTheReaderOnTheSameMessage() {
        openSeededAppWindow();
        var list = item("messageList");
        fillTranscriptUntilScrollable(list);

        keyClick(Qt.Key_PageUp);
        waitForRendering(appWindow.contentItem);
        tryCompare(list, "stick", 1);
        verify(!transcriptPinned(list));

        var visible = firstVisibleIndex(list);
        var anchorBody = field(list.model, visible, "body");
        prependLiveReplay(list, 9);
        waitForRendering(appWindow.contentItem);
        wait(0);

        compare(field(list.model, firstVisibleIndex(list), "body"), anchorBody);
        saveScreenshot("history-splice-anchor");
    }

    function test_historySpliceMovesTheUnseenMarkerWithItsRow() {
        openSeededAppWindow();
        var list = item("messageList");
        fillTranscriptUntilScrollable(list);

        keyClick(Qt.Key_PageUp);
        waitForRendering(appWindow.contentItem);
        tryCompare(list, "stick", 1);

        var previousCount = list.model.rowCount();
        appendLiveMessages(list, 1, "first unseen");
        waitForRendering(appWindow.contentItem);
        wait(0);
        compare(list.firstUnseenIndex, previousCount);
        var unseenBody = field(list.model, list.firstUnseenIndex, "body");

        prependLiveReplay(list, 9);
        waitForRendering(appWindow.contentItem);
        wait(0);
        compare(field(list.model, list.firstUnseenIndex, "body"), unseenBody);

        list.firstUnseenIndex = -1;
        list.noteSplice(list.count, list.count + 9);
        compare(list.firstUnseenIndex, -1);
    }

    function test_messageBodyIsSelectable() {
        openSeededAppWindow();
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
        clickMember("anna");
        tryCompare(appWindow, "currentConversation", "anna");
        waitForRendering(appWindow.contentItem);

        var dmBody = visibleListChild("messageList", "messageBody");
        dmBody.selectAll();
        verify(dmBody.selectedText.length > 0);
        verify(!containsMirc(dmBody.selectedText));
        verify(composer.activeFocus);
    }

    function test_consoleBodyIsSelectable() {
        openSeededAppWindow();
        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);

        var body = visibleListChild("consoleList", "consoleText");
        body.selectAll();
        verify(body.selectedText.length > 0);
        verify(!containsMirc(body.selectedText));
        verify(item("messageComposer").activeFocus);
    }

    function test_consoleBodyStripsMircFormatting() {
        openSeededAppWindow();
        var list = item("consoleList");
        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);

        var previousCount = list.model.rowCount();
        seed.injectOmarchy(":server 404 fred #omarchy :" + formattedIrcBody() + "\r\n");
        waitForRowCount(list, previousCount + 1);
        verify(field(list.model, previousCount, "text").indexOf(formattedIrcBody()) >= 0);

        list.positionViewAtIndex(previousCount, ListView.Contain);
        waitForRendering(appWindow.contentItem);

        var row = list.itemAtIndex(previousCount);
        verify(row !== null, "The formatted console line should be rendered");
        var body = findChild(row, "consoleText");
        verify(body !== null, "Could not find formatted consoleText");
        compare(body.text.indexOf("bold / red") >= 0, true);
        verify(!containsMirc(body.text));
        body.selectAll();
        compare(body.selectedText.indexOf("bold / red") >= 0, true);
        verify(!containsMirc(body.selectedText));
    }

    function test_messageBodyStripsMircFormatting() {
        openSeededAppWindow();
        var list = item("messageList");
        var previousCount = list.model.rowCount();
        injectOmarchyChat("anna", "#omarchy", formattedIrcBody());
        waitForRowCount(list, previousCount + 1);
        compare(appWindow.plainIrcText(formattedIrcBody()), "bold / red");
        compare(field(list.model, previousCount, "body"), formattedIrcBody());

        list.positionViewAtIndex(previousCount, ListView.Contain);
        waitForRendering(appWindow.contentItem);

        var row = list.itemAtIndex(previousCount);
        verify(row !== null, "The formatted seeded message should be rendered");
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
        openSeededAppWindow();
        var list = item("messageList");
        var previousCount = list.model.rowCount();
        injectOmarchyChat("anna", "#omarchy", "read https://example.com thanks");
        waitForRowCount(list, previousCount + 1);
        list.positionViewAtIndex(previousCount, ListView.Contain);
        waitForRendering(appWindow.contentItem);

        var row = list.itemAtIndex(previousCount);
        verify(row !== null, "The linked seeded message should be rendered");
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
        openSeededAppWindow();
        var list = item("consoleList");
        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);

        var previousCount = list.model.rowCount();
        seed.injectOmarchy(":server 404 fred #omarchy :motd http://example.com end\r\n");
        waitForRowCount(list, previousCount + 1);
        list.positionViewAtIndex(previousCount, ListView.Contain);
        waitForRendering(appWindow.contentItem);

        var row = list.itemAtIndex(previousCount);
        verify(row !== null, "The linked console line should be rendered");
        var body = findChild(row, "consoleText");
        verify(body !== null, "Could not find linked consoleText");
        compare(body.textFormat, TextEdit.PlainText);
        verify(body.text.indexOf("http://example.com") >= 0);
        body.selectAll();
        verify(body.selectedText.indexOf("http://example.com") >= 0);
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
        openSeededAppWindow();
        seed.injectOmarchy(":anna!u@h TOPIC #omarchy :" + formattedIrcBody() + "\r\n");
        tryCompare(appWindow, "currentTopic", formattedIrcBody());
        var topic = item("conversationTopic");
        compare(topic.text, "bold / red");
        compare(topic.textFormat, Text.PlainText);
        verify(!containsMirc(topic.text));

        var list = item("messageList");
        var previousCount = list.model.rowCount();
        seed.injectOmarchy(":rio!u@h PART #omarchy\r\n");
        waitForRowCount(list, previousCount + 1);
        compare(field(list.model, previousCount, "kind"), "event");
        list.positionViewAtIndex(previousCount, ListView.Contain);
        waitForRendering(appWindow.contentItem);

        var row = list.itemAtIndex(previousCount);
        verify(row !== null, "The event row should be rendered");
        var eventText = findChild(row, "messageEvent");
        verify(eventText !== null && eventText.visible, "Could not find messageEvent");
        compare(eventText.text, "rio left");
        compare(eventText.textFormat, Text.PlainText);
        verify(!containsMirc(eventText.text));
    }

    function test_whoisRowWrapsLongBody() {
        openSeededAppWindow();
        var list = item("messageList");
        var previousCount = list.model.rowCount();
        var chunk = "#omarchy #help #omairc ";
        var channels = chunk + chunk + chunk + chunk + chunk + chunk + chunk + chunk;
        var composer = item("messageComposer");
        mouseClick(composer);
        typeText("/whois lena");
        if (item("slashCompleteList").visible)
            keyClick(Qt.Key_Escape);
        keyClick(Qt.Key_Return);
        seed.injectOmarchy(":server 319 fred lena :" + channels + "\r\n");
        waitForRowCount(list, previousCount + 1);
        compare(field(list.model, previousCount, "kind"), "whois");
        var body = field(list.model, previousCount, "body");
        verify(body.indexOf("lena is on") === 0);
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
        openSeededAppWindow();
        var panel = item("membersPanel");
        verify(panel.visible);

        keyClick(Qt.Key_M, Qt.ControlModifier | Qt.ShiftModifier);

        tryCompare(panel, "visible", false);
        compare(item("peopleButton").Accessible.name, "Show members");
        saveScreenshot("toggle-members");
    }

    function test_focusMembersWithShortcut() {
        openSeededAppWindow();
        var panel = item("membersPanel");
        var members = item("membersList");
        verify(panel.visible);

        keyClick(Qt.Key_P, Qt.ControlModifier | Qt.ShiftModifier);

        tryCompare(panel, "visible", true);
        tryCompare(members, "activeFocus", true);
    }

    function test_memberHighlightOnlyWhileListFocused() {
        openSeededAppWindow();
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
        openSeededAppWindow();
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
        openSeededAppWindow();
        var members = item("membersList");

        keyClick(Qt.Key_P, Qt.ControlModifier | Qt.ShiftModifier);
        tryCompare(members, "activeFocus", true);
        tryCompare(members, "currentIndex", 0);

        var mira = memberIndex("mira");
        var step = 0;
        for (; step < mira; ++step)
            keyClick(Qt.Key_Down);
        tryCompare(members, "currentIndex", mira);

        keyClick(Qt.Key_Return);
        tryCompare(appWindow, "currentConversation", "mira");
    }

    function test_memberEnterAfterSwitchingToSmallerChannel() {
        openSeededAppWindow();
        var members = item("membersList");

        keyClick(Qt.Key_P, Qt.ControlModifier | Qt.ShiftModifier);
        tryCompare(members, "activeFocus", true);

        var step = 0;
        for (step = 0; step < 9; ++step)
            keyClick(Qt.Key_Down);
        tryCompare(members, "currentIndex", 9);

        mouseClick(namedItem(liveConversation("#help")));
        tryCompare(appWindow, "currentConversation", "#help");
        tryCompare(members, "currentIndex", 0);

        keyClick(Qt.Key_P, Qt.ControlModifier | Qt.ShiftModifier);
        tryCompare(members, "activeFocus", true);
        keyClick(Qt.Key_Return);
        tryCompare(appWindow, "currentConversation", "anna");
    }

    function test_focusMembersShortcutIgnoredOnDirectMessage() {
        openSeededAppWindow();
        mouseClick(namedItem(liveConversation("anna")));
        tryCompare(appWindow, "currentConversation", "anna");
        verify(!item("membersPanel").visible);

        keyClick(Qt.Key_P, Qt.ControlModifier | Qt.ShiftModifier);

        verify(!item("membersPanel").visible);
        compare(appWindow.currentConversation, "anna");
    }

    function test_shortcutsSheetTogglesAndEscapeKeepsConversation() {
        openSeededAppWindow();
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
        openSeededAppWindow();
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
        openSeededAppWindow();
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

    function test_ctrlKJumpsToFilteredConversation() {
        openSeededAppWindow();
        compare(appWindow.currentConversation, "#omarchy");
        var composer = item("messageComposer");
        var sheet = openJumpSheet();

        keyClick(Qt.Key_Down, Qt.AltModifier);
        compare(appWindow.currentConversation, "#omarchy");
        verify(sheet.opened);

        typeText("ric");
        tryCompare(item("jumpFilter"), "text", "ric");
        var model = item("jumpModel");
        compare(model.count, 1);
        compare(model.get(0).name, "#ricing");
        compare(appWindow.jumpSelectedIndex, 0);

        keyClick(Qt.Key_Return);
        tryCompare(sheet, "opened", false);
        tryCompare(appWindow, "currentConversation", "#ricing");
        compare(appWindow.title, "#ricing - Omairc");
        compare(appWindow.consoleVisible, false);
        tryCompare(composer, "activeFocus", true);
    }

    function test_ctrlKJumpsFromStatus() {
        openSeededAppWindow();
        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);
        compare(appWindow.title, networkDisplayName(seed.omarchyNetworkId) + " Status");

        var sheet = openJumpSheet();
        typeText("ric");
        tryCompare(item("jumpFilter"), "text", "ric");
        keyClick(Qt.Key_Return);

        tryCompare(sheet, "opened", false);
        tryCompare(appWindow, "consoleVisible", false);
        compare(appWindow.currentConversation, "#ricing");
        compare(appWindow.title, "#ricing - Omairc");
        tryCompare(item("messageComposer"), "activeFocus", true);
    }

    function test_ctrlKEscapeKeepsConversationAndDraft() {
        openSeededAppWindow();
        var composer = item("messageComposer");
        mouseClick(composer);
        verify(composer.activeFocus);
        typeText("keep this draft");
        compare(composer.text, "keep this draft");
        compare(appWindow.currentConversation, "#omarchy");

        var sheet = openJumpSheet();
        keyClick(Qt.Key_Escape);
        tryCompare(sheet, "opened", false);
        compare(appWindow.currentConversation, "#omarchy");
        compare(composer.text, "keep this draft");
        tryCompare(composer, "activeFocus", true);

        sheet = openJumpSheet();
        typeText("ri");
        tryCompare(item("jumpFilter"), "text", "ri");
        keyClick(Qt.Key_Escape);
        tryCompare(sheet, "opened", false);
        compare(appWindow.currentConversation, "#omarchy");
        compare(appWindow.title,
                "#omarchy · " + networkDisplayName(seed.omarchyNetworkId) + " - Omairc");
        compare(composer.text, "keep this draft");
        tryCompare(composer, "activeFocus", true);
    }

    function test_ctrlKDisambiguatesDuplicateChannels() {
        openSeededAppWindow();
        var sheet = openJumpSheet();
        typeText("#omarchy");
        tryCompare(item("jumpFilter"), "text", "#omarchy");
        var model = item("jumpModel");
        compare(model.count, 2);
        compare(model.get(0).name, "#omarchy");
        compare(model.get(0).networkId, seed.omarchyNetworkId);
        compare(model.get(0).label,
                "#omarchy · " + networkDisplayName(seed.omarchyNetworkId));
        compare(model.get(1).name, "#omarchy");
        compare(model.get(1).networkId, seed.oftcNetworkId);
        compare(model.get(1).label,
                "#omarchy · " + networkDisplayName(seed.oftcNetworkId));
        compare(appWindow.jumpSelectedIndex, 0);

        keyClick(Qt.Key_Down);
        compare(appWindow.jumpSelectedIndex, 1);

        keyClick(Qt.Key_Return);
        tryCompare(sheet, "opened", false);
        tryCompare(appWindow, "currentConversationId",
                   seed.oftcNetworkId + "\n#omarchy");
        compare(appWindow.currentConversation, "#omarchy");
        compare(appWindow.title,
                "#omarchy · " + networkDisplayName(seed.oftcNetworkId) + " - Omairc");
        tryCompare(item("messageComposer"), "activeFocus", true);
    }

    function test_ctrlKIsNoOpWhenConnectIsVisible() {
        var window = createTemporaryObject(setupWindowComponent, null);
        verify(window !== null, "The setup window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        var connectSheet = findChild(window, "connectionSheet");
        verify(connectSheet !== null, "Could not find connectionSheet");
        verify(connectSheet.visible);
        var jump = findChild(window, "jumpSheet");
        verify(jump !== null, "Could not find jumpSheet");
        compare(jump.opened, false);

        keyClick(Qt.Key_K, Qt.ControlModifier);

        compare(jump.opened, false);
        verify(connectSheet.visible);
        window.close();
    }

    function test_openDirectMessageFromMember() {
        openSeededAppWindow();
        var previousCount = appWindow.irc.conversations.rowCount();
        clickMember("mira");

        tryCompare(appWindow, "currentConversation", "mira");
        compare(appWindow.currentTopic, "Direct message with mira");
        tryVerify(function() {
            return appWindow.irc.conversations.rowCount() === previousCount + 1;
        });
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
        compare(findChild(window, "connectionAccount").text, "");
        compare(findChild(window, "connectionBouncerNetwork").text, "");
        compare(findChild(window, "connectionAutojoin").text, "#omarchy");
        compare(findChild(window, "connectionConnectOnStartup").checked, false);
        compare(findChild(window, "connectionProblem").text, "Nick is required");
        compare(findChild(window, "connectionCredentialStatus").text,
                "secure storage unavailable");
        var forgetPassword = findChild(window, "connectionForgetPassword");
        verify(forgetPassword !== null, "Could not find connectionForgetPassword");
        compare(forgetPassword.visible, false);
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

    function test_forgetPasswordShowsKeyboardFocus() {
        fakeConnection.canForgetPassword = true;
        var window = createTemporaryObject(setupWindowComponent, null);
        verify(window !== null, "The setup window should load");
        tryCompare(window, "visible", true);

        var forgetPassword = findChild(window, "connectionForgetPassword");
        verify(forgetPassword !== null, "Could not find connectionForgetPassword");
        forgetPassword.forceActiveFocus();
        tryCompare(forgetPassword, "activeFocus", true);
        verify(forgetPassword.font.underline);
        window.close();
        fakeConnection.canForgetPassword = false;
    }

    function test_forgetPasswordRemovesStoredCredential() {
        fakeConnection.canForgetPassword = true;
        fakeConnection.forgetPasswordCalls = 0;
        fakeConnection.removeStoredPasswordCalls = 0;
        var window = createTemporaryObject(setupWindowComponent, null);
        verify(window !== null, "The setup window should load");
        tryCompare(window, "visible", true);

        var forgetPassword = findChild(window, "connectionForgetPassword");
        verify(forgetPassword !== null, "Could not find connectionForgetPassword");
        mouseClick(forgetPassword);
        compare(fakeConnection.forgetPasswordCalls, 1);
        compare(fakeConnection.removeStoredPasswordCalls, 1);
        window.close();
        fakeConnection.canForgetPassword = false;
    }

    function test_nickServFieldAndForgetAreIndependent() {
        fakeConnection.canForgetNickServ = true;
        fakeConnection.forgetNickServCalls = 0;
        fakeConnection.removeStoredNickServCalls = 0;
        fakeConnection.forgetPasswordCalls = 0;
        var window = createTemporaryObject(setupWindowComponent, null);
        verify(window !== null, "The setup window should load");
        tryCompare(window, "visible", true);

        var nickServ = findChild(window, "connectionNickServ");
        verify(nickServ !== null, "Could not find connectionNickServ");
        var forgetNickServ = findChild(window, "connectionForgetNickServ");
        verify(forgetNickServ !== null, "Could not find connectionForgetNickServ");
        compare(forgetNickServ.visible, true);
        mouseClick(forgetNickServ);
        compare(fakeConnection.forgetNickServCalls, 1);
        compare(fakeConnection.removeStoredNickServCalls, 1);
        compare(fakeConnection.forgetPasswordCalls, 0);
        window.close();
        fakeConnection.canForgetNickServ = false;
    }

    function test_focusNickServOpensSheetAndFocusesField() {
        fakeConnection.focusNickServ = false;
        var window = createTemporaryObject(setupWindowComponent, null);
        verify(window !== null, "The setup window should load");
        tryCompare(window, "visible", true);
        window.connectionSheetOpen = false;

        fakeConnection.focusNickServ = true;
        tryCompare(window, "connectionSheetOpen", true);
        var nickServ = findChild(window, "connectionNickServ");
        verify(nickServ !== null, "Could not find connectionNickServ");
        tryCompare(nickServ, "activeFocus", true);
        window.close();
        fakeConnection.focusNickServ = false;
    }

    function test_incompleteProfileEnterDoesNotCommitPassword() {
        fakeConnection.setPasswordCalls = 0;
        fakeConnection.lastSetPassword = "";
        var window = createTemporaryObject(setupWindowComponent, null);
        verify(window !== null, "The setup window should load");
        tryCompare(window, "visible", true);

        var password = findChild(window, "connectionPassword");
        verify(password !== null, "Could not find connectionPassword");
        password.forceActiveFocus();
        password.text = "typed-secret";
        keyClick(Qt.Key_Return);
        compare(fakeConnection.setPasswordCalls, 0);
        compare(fakeConnection.lastSetPassword, "");
        window.close();
    }

    function test_connectionSheetTabOrderReachesApply() {
        fakeConnection.applyCalls = 0;
        var window = createTemporaryObject(setupWindowComponent, null);
        verify(window !== null, "The setup window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        var network = repeaterItemByName(findChild(window, "networkChoiceRepeater"),
                                         "networkChoice-setup-id");
        verify(network !== null, "Could not find networkChoice-setup-id");
        network.forceActiveFocus();
        tryCompare(network, "activeFocus", true);

        var expected = [
            "networkChoice-setup-id",
            "connectionHost",
            "connectionPort",
            "connectionTls",
            "connectionNick",
            "connectionUsername",
            "connectionAccount",
            "connectionBouncerNetwork",
            "connectionRealname",
            "connectionAutojoin",
            "connectionConnectOnStartup",
            "connectionPassword",
            "connectionNickServ",
            "connectionDiscard",
            "connectionApply"
        ];
        var names = [focusObjectName(window)];
        var step = 0;
        for (step = 1; step < expected.length; ++step) {
            keyClick(Qt.Key_Tab);
            wait(0);
            names.push(focusObjectName(window));
        }
        compare(names, expected);

        keyClick(Qt.Key_Tab);
        wait(0);
        compare(focusObjectName(window), "networkChoice-setup-id");

        keyClick(Qt.Key_Tab, Qt.ShiftModifier);
        wait(0);
        compare(focusObjectName(window), "connectionApply");
        keyClick(Qt.Key_Tab, Qt.ShiftModifier);
        wait(0);
        compare(focusObjectName(window), "connectionDiscard");
        window.close();
    }

    function test_connectionSheetOpenedFromComposerKeepsTabInsideSheet() {
        restoreNamedConnection();
        failOnWarning("QQuickItem: Cannot set activeFocusOnTab to false once item is the active focus item.");
        var window = createTemporaryObject(fallbackWindowComponent, null);
        verify(window !== null, "The composer-to-Connect window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        var composer = findChild(window, "messageComposer");
        verify(composer !== null, "Could not find messageComposer");
        composer.forceActiveFocus();
        tryCompare(composer, "activeFocus", true);

        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        tryCompare(findChild(window, "connectionSheet"), "visible", true);
        tryCompare(composer, "activeFocus", false);
        tryCompare(findChild(window, "connectionHost"), "activeFocus", true);

        var step = 0;
        var name = "";
        for (step = 0; step < 24; ++step) {
            keyClick(Qt.Key_Tab);
            wait(0);
            name = focusObjectName(window);
            verify(name !== "messageComposer",
                   "Tab landed on messageComposer at step " + step);
        }

        var applyButton = findChild(window, "connectionApply");
        applyButton.forceActiveFocus();
        tryCompare(applyButton, "activeFocus", true);
        for (step = 0; step < 24; ++step) {
            keyClick(Qt.Key_Tab, Qt.ShiftModifier);
            wait(0);
            name = focusObjectName(window);
            verify(name !== "messageComposer",
                   "Shift+Tab landed on messageComposer at step " + step);
        }
        window.close();
        restoreNamedConnection();
    }

    function test_connectionSheetEnterFromHostKeepsNickProblem() {
        fakeConnection.applyCalls = 0;
        var window = createTemporaryObject(setupWindowComponent, null);
        verify(window !== null, "The setup window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        var host = findChild(window, "connectionHost");
        verify(host !== null, "Could not find connectionHost");
        host.forceActiveFocus();
        tryCompare(host, "activeFocus", true);
        keyClick(Qt.Key_Return);

        compare(fakeConnection.applyCalls, 0);
        compare(findChild(window, "connectionProblem").text, "Nick is required");
        verify(findChild(window, "connectionProblem").visible);
        verify(findChild(window, "connectionSheet").visible);
        verify(host.activeFocus);
        window.close();
    }

    function test_connectionSheetEnterFromNickAppliesAndCloses() {
        restoreNamedConnection();
        namedConnection.applySucceeds = true;
        namedConnection.applyCalls = 0;
        var window = createTemporaryObject(fallbackWindowComponent, null);
        verify(window !== null, "The apply window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        var sheet = findChild(window, "connectionSheet");
        tryCompare(sheet, "visible", true);

        var nick = findChild(window, "connectionNick");
        verify(nick !== null, "Could not find connectionNick");
        nick.forceActiveFocus();
        tryCompare(nick, "activeFocus", true);
        keyClick(Qt.Key_Return);

        compare(namedConnection.applyCalls, 1);
        compare(sheet.visible, false);
        window.close();
        restoreNamedConnection();
    }

    function test_connectionSheetEnterFromSwitchesAppliesAndCloses() {
        restoreNamedConnection();
        namedConnection.applySucceeds = true;
        namedConnection.applyCalls = 0;
        var window = createTemporaryObject(fallbackWindowComponent, null);
        verify(window !== null, "The switch-enter window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        var sheet = findChild(window, "connectionSheet");
        tryCompare(sheet, "visible", true);

        var tls = findChild(window, "connectionTls");
        verify(tls !== null, "Could not find connectionTls");
        tls.forceActiveFocus();
        tryCompare(tls, "activeFocus", true);
        keyClick(Qt.Key_Return);
        compare(namedConnection.applyCalls, 1);
        compare(sheet.visible, false);

        namedConnection.applyCalls = 0;
        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        tryCompare(sheet, "visible", true);
        var startup = findChild(window, "connectionConnectOnStartup");
        verify(startup !== null, "Could not find connectionConnectOnStartup");
        startup.forceActiveFocus();
        tryCompare(startup, "activeFocus", true);
        keyClick(Qt.Key_Return);
        compare(namedConnection.applyCalls, 1);
        compare(sheet.visible, false);
        window.close();
        restoreNamedConnection();
    }

    function test_connectionSheetApplyDiscardReachableByTab() {
        restoreNamedConnection();
        var window = createTemporaryObject(fallbackWindowComponent, null);
        verify(window !== null, "The action-tab window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        tryCompare(findChild(window, "connectionSheet"), "visible", true);

        var addButton = findChild(window, "connectionAddNetwork");
        addButton.forceActiveFocus();
        tryCompare(addButton, "activeFocus", true);

        var sawDiscard = false;
        var sawApply = false;
        var step = 0;
        for (step = 0; step < 20; ++step) {
            keyClick(Qt.Key_Tab);
            wait(0);
            var name = focusObjectName(window);
            if (name === "connectionDiscard")
                sawDiscard = true;
            if (name === "connectionApply")
                sawApply = true;
        }
        compare(sawDiscard, true);
        compare(sawApply, true);

        var discardButton = findChild(window, "connectionDiscard");
        discardButton.forceActiveFocus();
        tryCompare(discardButton, "activeFocus", true);
        keyClick(Qt.Key_Return);
        compare(namedConnection.discardCalls, 1);
        window.close();
        restoreNamedConnection();
    }

    function test_networkRowSecondEnterApplies() {
        restoreNamedConnection();
        namedConnection.applySucceeds = true;
        namedConnection.applyCalls = 0;
        var window = createTemporaryObject(fallbackWindowComponent, null);
        verify(window !== null, "The network-enter window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        var sheet = findChild(window, "connectionSheet");
        tryCompare(sheet, "visible", true);

        var row = repeaterItemByName(findChild(window, "networkChoiceRepeater"),
                                     "networkChoice-libera");
        verify(row !== null, "Could not find networkChoice-libera");
        row.forceActiveFocus();
        tryCompare(row, "activeFocus", true);
        keyClick(Qt.Key_Return);
        compare(namedConnection.applyCalls, 0);
        verify(sheet.visible);
        verify(row.activeFocus);

        keyClick(Qt.Key_Return);
        compare(namedConnection.applyCalls, 1);
        compare(sheet.visible, false);
        window.close();
        restoreNamedConnection();
    }

    function test_networkRowCtrlReturnApplies() {
        restoreNamedConnection();
        namedConnection.applySucceeds = true;
        namedConnection.applyCalls = 0;
        var window = createTemporaryObject(fallbackWindowComponent, null);
        verify(window !== null, "The network-ctrl-return window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        var sheet = findChild(window, "connectionSheet");
        tryCompare(sheet, "visible", true);

        var row = repeaterItemByName(findChild(window, "networkChoiceRepeater"),
                                     "networkChoice-libera");
        verify(row !== null, "Could not find networkChoice-libera");
        row.forceActiveFocus();
        tryCompare(row, "activeFocus", true);
        keyClick(Qt.Key_Return, Qt.ControlModifier);
        compare(namedConnection.applyCalls, 1);
        compare(sheet.visible, false);
        window.close();
        restoreNamedConnection();
    }

    function test_connectionSheetFirstOpenFocusesNickWhenEmpty() {
        fakeConnection.applyCalls = 0;
        var window = createTemporaryObject(setupWindowComponent, null);
        verify(window !== null, "The first-open setup window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        var nick = findChild(window, "connectionNick");
        verify(nick !== null, "Could not find connectionNick");
        tryCompare(nick, "activeFocus", true);
        compare(focusObjectName(window), "connectionNick");

        keyClick(Qt.Key_Return);
        compare(fakeConnection.applyCalls, 0);
        compare(findChild(window, "connectionProblem").text, "Nick is required");
        verify(findChild(window, "connectionProblem").visible);
        verify(findChild(window, "connectionSheet").visible);
        compare(fakeConnection.selectedNetworkId, "setup-id");
        verify(nick.activeFocus);
        window.close();
    }

    function test_connectionSheetFirstOpenEnterAppliesWithoutChangingNetwork() {
        restoreNamedConnection();
        namedNetworks.append({
            networkId: "oftc",
            displayName: "irc.oftc.net",
            stored: true,
            selected: false,
            iconColor: 1
        });
        namedConnection.applySucceeds = true;
        namedConnection.applyCalls = 0;
        var window = createTemporaryObject(fallbackWindowComponent, null);
        verify(window !== null, "The first-open apply window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        var sheet = findChild(window, "connectionSheet");
        tryCompare(sheet, "visible", true);
        waitForRendering(window.contentItem);

        var host = findChild(window, "connectionHost");
        verify(host !== null, "Could not find connectionHost");
        tryCompare(host, "activeFocus", true);
        compare(focusObjectName(window), "connectionHost");
        compare(namedConnection.selectedNetworkId, "libera");

        keyClick(Qt.Key_Return);
        compare(namedConnection.applyCalls, 1);
        compare(namedConnection.selectedNetworkId, "libera");
        compare(sheet.visible, false);
        window.close();
        restoreNamedConnection();
    }

    function test_connectionSheetAddThenDiscardRestoresPriorNetwork() {
        restoreNamedConnection();
        namedNetworks.append({
            networkId: "oftc",
            displayName: "irc.oftc.net",
            stored: true,
            selected: false,
            iconColor: 1
        });
        var window = createTemporaryObject(fallbackWindowComponent, null);
        verify(window !== null, "The add-then-discard window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        var sheet = findChild(window, "connectionSheet");
        tryCompare(sheet, "visible", true);

        var addButton = findChild(window, "connectionAddNetwork");
        var discardButton = findChild(window, "connectionDiscard");
        verify(addButton !== null, "Could not find connectionAddNetwork");
        verify(discardButton !== null, "Could not find connectionDiscard");

        mouseClick(addButton);
        compare(namedNetworks.count, 3);
        compare(namedConnection.selectedNetworkId, "new-id");
        compare(findChild(window, "connectionHost").text, "");

        mouseClick(discardButton);
        compare(namedConnection.discardCalls, 1);
        compare(namedNetworks.count, 2);
        compare(namedConnection.selectedNetworkId, "libera");
        compare(namedConnection.host, "irc.libera.chat");
        compare(findChild(window, "connectionHost").text, "irc.libera.chat");
        compare(namedConnection.nick, "sheet-nick");

        mouseClick(addButton);
        compare(namedConnection.selectedNetworkId, "new-id");
        discardButton.forceActiveFocus();
        tryCompare(discardButton, "activeFocus", true);
        keyClick(Qt.Key_Return);
        compare(namedConnection.discardCalls, 2);
        compare(namedNetworks.count, 2);
        compare(namedConnection.selectedNetworkId, "libera");
        compare(findChild(window, "connectionHost").text, "irc.libera.chat");
        verify(sheet.visible);
        window.close();
        restoreNamedConnection();
    }

    function test_sheetFlickShowsScrollbarWhenFormOverflows() {
        var window = createTemporaryObject(setupWindowComponent, null);
        verify(window !== null, "The overflow window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        window.minimumHeight = 200;
        window.height = 320;
        waitForRendering(window.contentItem);
        wait(0);

        var flick = findChild(window, "sheetFlick");
        verify(flick !== null, "Could not find sheetFlick");
        verify(flick.contentHeight > flick.height);
        var bar = findChild(window, "sheetFlickScrollBar");
        verify(bar !== null, "Could not find sheetFlickScrollBar");
        compare(bar.policy, Controls.ScrollBar.AlwaysOn);
        compare(bar.visible, true);
        verify(bar.size < 1.0);
        window.close();
    }

    function test_networkListShowsScrollbarWhenItOverflows() {
        restoreNamedConnection();
        var extra = 0;
        for (extra = 0; extra < 16; ++extra) {
            namedNetworks.append({
                networkId: "extra-" + extra,
                displayName: "irc.extra" + extra + ".example",
                stored: true,
                selected: false,
                iconColor: 1
            });
        }
        var window = createTemporaryObject(fallbackWindowComponent, null);
        verify(window !== null, "The rail-overflow window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        tryCompare(findChild(window, "connectionSheet"), "visible", true);
        waitForRendering(window.contentItem);

        var rail = findChild(window, "networkChoiceScroll");
        verify(rail !== null, "Could not find networkChoiceScroll");
        verify(rail.contentHeight > rail.height);
        var bar = findChild(window, "networkChoiceScrollBar");
        verify(bar !== null, "Could not find networkChoiceScrollBar");
        compare(bar.policy, Controls.ScrollBar.AlwaysOn);
        compare(bar.visible, true);
        window.close();
        restoreNamedConnection();
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
        openSeededAppWindow();
        var members = item("membersList");
        verify(item("membersPanel").visible);
        var anna = members.itemAtIndex(memberIndex("anna"));
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
        openSeededAppWindow();
        compare(appWindow.currentConversation, "#omarchy");
        var anna = namedItem(liveConversation("anna"));
        compare(anna.conversationName, "anna");
        compare(anna.direct, true);
        compare(anna.current, false);
        compare(anna.typing, true);
        var dots = findChild(anna, "conversation-typing-" + seed.omarchyNetworkId + "-anna");
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

        var header = findChild(window, "networkHeaderButton-libera");
        verify(header !== null, "Could not find networkHeaderButton-libera");
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
        openSeededAppWindow();
        mouseClick(liveHeaderButton(seed.omarchyNetworkId));
        tryCompare(appWindow, "consoleVisible", true);
        compare(appWindow.title, statusTitle(seed.omarchyNetworkId));
        var list = item("consoleList");
        verify(list.visible);
        verify(list.model.rowCount() > 0);
        verify(!item("peopleButton").visible);
    }

    function test_mockIdentityFooterShowsFredAndDropsNotice() {
        openSeededAppWindow();
        compare(item("selfNickLabel").text, "fred");
        compare(findChild(appWindow, "mockNotice"), null);
    }

    function test_mockIdentityFooterShowsAvailable() {
        openSeededAppWindow();
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
        openSeededAppWindow();
        var anna = namedItem(liveConversation("anna"));
        compare(anna.unread, 1);
        compare(anna.mention, true);

        mouseClick(anna);

        tryCompare(appWindow, "currentConversation", "anna");
        compare(anna.unread, 0);
        compare(anna.mention, false);
    }

    function test_channelCtrlWIsNoOp() {
        openSeededAppWindow();
        compare(appWindow.currentConversation, "#omarchy");
        compare(visibleDirects(seed.omarchyNetworkId).length, 2);

        keyClick(Qt.Key_W, Qt.ControlModifier);

        compare(appWindow.currentConversation, "#omarchy");
        compare(appWindow.consoleVisible, false);
        compare(visibleDirects(seed.omarchyNetworkId).length, 2);

        appWindow.closeDirectMessage();
        compare(appWindow.currentConversation, "#omarchy");
        compare(visibleDirects(seed.omarchyNetworkId).length, 2);
        verify(findNamed(liveConversation("anna")) !== null);
        verify(findNamed(liveConversation("dax")) !== null);
    }

    function test_closeDirectMessageSelectsNext() {
        openSeededAppWindow();
        mouseClick(namedItem(liveConversation("anna")));
        tryCompare(appWindow, "currentConversation", "anna");

        keyClick(Qt.Key_W, Qt.ControlModifier);

        tryCompare(appWindow, "currentConversation", "dax");
        compare(appWindow.consoleVisible, false);
        compare(visibleDirects(seed.omarchyNetworkId).length, 1);
        compare(visibleDirects(seed.omarchyNetworkId)[0], "dax");
        compare(findNamed(liveConversation("anna")), null);
        compare(item("messageList").Accessible.name, "Messages in dax");
    }

    function test_closeDirectMessageSelectsPreviousWithoutWrapping() {
        openSeededAppWindow();
        mouseClick(namedItem(liveConversation("dax")));
        tryCompare(appWindow, "currentConversation", "dax");

        keyClick(Qt.Key_W, Qt.ControlModifier);

        tryCompare(appWindow, "currentConversation", "anna");
        compare(appWindow.consoleVisible, false);
        compare(visibleDirects(seed.omarchyNetworkId).length, 1);
        compare(visibleDirects(seed.omarchyNetworkId)[0], "anna");

        keyClick(Qt.Key_W, Qt.ControlModifier);

        tryCompare(appWindow, "currentConversation", "#ricing");
        compare(appWindow.consoleVisible, false);
        compare(visibleDirects(seed.omarchyNetworkId).length, 0);
        compare(findNamed(liveConversation("anna")), null);
        compare(findNamed(liveConversation("dax")), null);
    }

    function test_statusShortcutsUntouchedByClose() {
        openSeededAppWindow();
        mouseClick(namedItem(liveConversation("anna")));
        tryCompare(appWindow, "currentConversation", "anna");

        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);

        keyClick(Qt.Key_W, Qt.ControlModifier);
        compare(appWindow.consoleVisible, true);
        compare(appWindow.currentConversation, "anna");
        compare(visibleDirects(seed.omarchyNetworkId).length, 2);

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
        openSeededAppWindow();
        var composer = item("messageComposer");
        var messages = item("messageList");
        var previousCount = messages.model.rowCount();
        mouseClick(composer);
        typeText("/close");
        compare(composer.text, "/close");
        if (item("slashCompleteList").visible)
            keyClick(Qt.Key_Escape);
        keyClick(Qt.Key_Return);

        compare(messages.model.rowCount(), previousCount);
        compare(composer.text, "/close");
        compare(appWindow.currentConversation, "#omarchy");
        compare(visibleDirects(seed.omarchyNetworkId).length, 2);
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
        openSeededAppWindow();
        verify(liveHeader(seed.omarchyNetworkId).visible);
        verify(liveHeader(seed.oftcNetworkId).visible);
        verify(liveHeaderButton(seed.omarchyNetworkId).visible);
        verify(liveHeaderButton(seed.oftcNetworkId).visible);
        compare(namedItem(liveConversation("#omarchy")).conversationName, "#omarchy");
        compare(namedItem(liveOftcConversation("#omarchy")).conversationName, "#omarchy");
        compare(namedItem(liveOftcConversation("#lab")).conversationName, "#lab");
        compare(namedItem(liveOftcConversation("rio")).conversationName, "rio");
        compare(namedItem(liveConversation("#omarchy")).current, true);
        compare(namedItem(liveOftcConversation("#omarchy")).current, false);
        saveScreenshot("two-networks");
    }

    function test_mockNetworkIconsUseDistinctPaletteColors() {
        openSeededAppWindow();
        var omarchy = namedItem("networkIcon-" + seed.omarchyNetworkId);
        var oftc = namedItem("networkIcon-" + seed.oftcNetworkId);
        compare(omarchy.color, Qt.color(appWindow.paletteColor(1)));
        compare(oftc.color, Qt.color(appWindow.paletteColor(2)));
        verify(omarchy.color.toString() !== oftc.color.toString());
        saveScreenshot("network-icon-colors");
    }

    function test_duplicateOmarchyHighlightsIndependently() {
        openSeededAppWindow();
        mouseClick(namedItem(liveOftcConversation("#omarchy")));
        tryCompare(appWindow, "currentConversation", "#omarchy");
        compare(appWindow.currentConversationId, seed.oftcNetworkId + "\n#omarchy");
        compare(appWindow.currentTopic, "A different #omarchy, hosted on OFTC.");
        compare(appWindow.currentPeopleCount, 4);
        compare(item("selfNickLabel").text, "oak");
        compare(namedItem(liveOftcConversation("#omarchy")).current, true);
        compare(namedItem(liveConversation("#omarchy")).current, false);
        compare(appWindow.title,
                "#omarchy · " + networkDisplayName(seed.oftcNetworkId) + " - Omairc");

        mouseClick(namedItem(liveConversation("#omarchy")));
        tryCompare(appWindow, "currentConversationId",
                   seed.omarchyNetworkId + "\n#omarchy");
        compare(appWindow.currentTopic,
                "A cozy corner for Omarchy users and builders.");
        compare(appWindow.currentPeopleCount, 12);
        compare(item("selfNickLabel").text, "fred");
        compare(namedItem(liveConversation("#omarchy")).current, true);
        compare(namedItem(liveOftcConversation("#omarchy")).current, false);
        compare(appWindow.title,
                "#omarchy · " + networkDisplayName(seed.omarchyNetworkId) + " - Omairc");
    }

    function test_altWalkSkipsNetworkHeaders() {
        openSeededAppWindow();
        mouseClick(namedItem(liveConversation("#help")));
        tryCompare(appWindow, "currentConversation", "#help");
        keyClick(Qt.Key_Down, Qt.AltModifier);
        tryCompare(appWindow, "currentConversation", "#omarchy");
        compare(appWindow.currentConversationId, seed.omarchyNetworkId + "\n#omarchy");
        keyClick(Qt.Key_Down, Qt.AltModifier);
        tryCompare(appWindow, "currentConversation", "#ricing");
        keyClick(Qt.Key_Down, Qt.AltModifier);
        tryCompare(appWindow, "currentConversation", "anna");
        mouseClick(namedItem(liveConversation("dax")));
        tryCompare(appWindow, "currentConversation", "dax");
        keyClick(Qt.Key_Down, Qt.AltModifier);
        tryCompare(appWindow, "currentConversation", "#build");
        compare(appWindow.currentConversationId, seed.oftcNetworkId + "\n#build");
    }

    function test_walkNetworksWithShortcut() {
        openSeededAppWindow();
        compare(appWindow.currentConversation, "#omarchy");
        compare(appWindow.sidebarNetworkFocusId, "");

        keyClick(Qt.Key_Right, Qt.AltModifier);

        compare(appWindow.sidebarNetworkFocusId, seed.oftcNetworkId);
        compare(appWindow.currentConversation, "#omarchy");
        compare(appWindow.consoleVisible, false);
        compare(liveHeader(seed.oftcNetworkId).parent.headerFocused, true);
        compare(liveHeader(seed.omarchyNetworkId).parent.headerFocused, false);
    }

    function test_walkNetworksWraps() {
        openSeededAppWindow();
        compare(appWindow.currentConversation, "#omarchy");

        keyClick(Qt.Key_Left, Qt.AltModifier);

        compare(appWindow.sidebarNetworkFocusId, seed.oftcNetworkId);

        keyClick(Qt.Key_Left, Qt.AltModifier);

        compare(appWindow.sidebarNetworkFocusId, seed.omarchyNetworkId);
        compare(liveHeader(seed.omarchyNetworkId).parent.headerFocused, true);
    }

    function test_enterOnNetworkHeaderOpensStatus() {
        openSeededAppWindow();
        keyClick(Qt.Key_Right, Qt.AltModifier);
        compare(appWindow.sidebarNetworkFocusId, seed.oftcNetworkId);

        keyClick(Qt.Key_Return);

        tryCompare(appWindow, "consoleVisible", true);
        compare(appWindow.sidebarNetworkFocusId, "");
        compare(appWindow.title, statusTitle(seed.oftcNetworkId));
    }

    function test_altDownStaysConversationOnlyAfterNetworkWalk() {
        openSeededAppWindow();
        keyClick(Qt.Key_Right, Qt.AltModifier);
        compare(appWindow.sidebarNetworkFocusId, seed.oftcNetworkId);

        keyClick(Qt.Key_Down, Qt.AltModifier);

        tryCompare(appWindow, "currentConversation", "#ricing");
        compare(appWindow.currentConversationId, seed.omarchyNetworkId + "\n#ricing");
        compare(appWindow.sidebarNetworkFocusId, "");
        compare(appWindow.consoleVisible, false);
    }

    function test_shortcutsSheetListsNetworkWalk() {
        openSeededAppWindow();
        var sheet = item("shortcutsSheet");
        keyClick(Qt.Key_Slash, Qt.ControlModifier);
        tryCompare(sheet, "opened", true);
        var texts = shortcutSheetTexts();
        verify(texts.indexOf("Alt+Left / Alt+Right") !== -1,
               "shortcut sheet should list Alt+Left / Alt+Right");
        verify(texts.indexOf("walk networks") !== -1,
               "shortcut sheet should name walk networks");
        verify(texts.indexOf("Ctrl+K") !== -1,
               "shortcut sheet should list Ctrl+K");
        verify(texts.indexOf("jump to conversation") !== -1,
               "shortcut sheet should name jump to conversation");
        keyClick(Qt.Key_Escape);
        tryCompare(sheet, "opened", false);
    }

    function test_emptyNetworkHeaderIsAKeyboardStop() {
        restoreNamedConnection();
        namedNetworks.append({
            networkId: "oftc",
            displayName: "irc.oftc.net",
            stored: true,
            selected: false,
            iconColor: 1
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
            compare(window.sidebarNetworkSections().length, 2);

            window.stepNetwork(1);
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

            window.networkConsole.open = false;
            compare(window.consoleVisible, false);

            window.stepConversation(1);
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

            window.stepConversation(1);
            tryCompare(window, "currentConversation", "#lab");
            waitForRendering(window.contentItem);

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

    function test_accountAndBouncerNetworkAreEditable() {
        restoreNamedConnection();
        var window = createTemporaryObject(fallbackWindowComponent, null);
        verify(window !== null, "The bouncer-login window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        var account = findChild(window, "connectionAccount");
        verify(account !== null, "Could not find connectionAccount");
        mouseClick(account);
        keyClick(Qt.Key_J);
        keyClick(Qt.Key_O);
        keyClick(Qt.Key_E);
        compare(namedConnection.account, "joe");

        var bouncerNetwork = findChild(window, "connectionBouncerNetwork");
        verify(bouncerNetwork !== null, "Could not find connectionBouncerNetwork");
        mouseClick(bouncerNetwork);
        keyClick(Qt.Key_L);
        keyClick(Qt.Key_I);
        keyClick(Qt.Key_B);
        compare(namedConnection.bouncerNetwork, "lib");
        compare(account.text, "joe");

        window.close();
        restoreNamedConnection();
    }

    function test_mockStatusTitleIdentifiesNetwork() {
        openSeededAppWindow();
        mouseClick(liveHeaderButton(seed.oftcNetworkId));
        tryCompare(appWindow, "consoleVisible", true);
        compare(appWindow.title, statusTitle(seed.oftcNetworkId));
        compare(item("selfNickLabel").text, "oak");
    }

    function test_rejectedNetworkSelectKeepsPassword() {
        restoreNamedConnection();
        namedNetworks.append({
            networkId: "oftc",
            displayName: "irc.oftc.net",
            stored: true,
            selected: false,
            iconColor: 1
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
        openSeededAppWindow();
        mouseClick(namedItem(liveOftcConversation("#lab")));
        tryCompare(appWindow, "currentConversation", "#lab");
        clickMember("rio");
        tryCompare(appWindow, "currentConversation", "rio");
        compare(visibleDirects(seed.omarchyNetworkId).indexOf("rio"), -1);
        compare(visibleDirects(seed.oftcNetworkId).filter(function(name) {
            return name === "rio";
        }).length, 1);
        compare(namedItem(liveOftcConversation("rio")).conversationName, "rio");
        mouseClick(namedItem(liveConversation("#omarchy")));
        tryCompare(appWindow, "currentConversationId",
                   seed.omarchyNetworkId + "\n#omarchy");
    }

    function test_mockDirectStaysOnItsNetwork() {
        openSeededAppWindow();
        mouseClick(namedItem(liveOftcConversation("#lab")));
        tryCompare(appWindow, "currentConversationId", seed.oftcNetworkId + "\n#lab");
        appWindow.openDirectMessage("kai");
        tryCompare(appWindow, "currentConversation", "kai");
        compare(appWindow.currentConversationId, seed.oftcNetworkId + "\nkai");
        compare(visibleDirects(seed.omarchyNetworkId).indexOf("kai"), -1);
        compare(visibleDirects(seed.oftcNetworkId).indexOf("kai") >= 0, true);
        keyClick(Qt.Key_W, Qt.ControlModifier);
        mouseClick(namedItem(liveConversation("#omarchy")));
        tryCompare(appWindow, "currentConversationId",
                   seed.omarchyNetworkId + "\n#omarchy");
    }

    function test_liveUnreadFollowsConversationEpoch() {
        liveIrc.unreadSink = [0, false];
        liveIrc.conversationEpoch = 0;
        var window = createTemporaryObject(liveWindowComponent, null);
        verify(window !== null, "The live unread window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        var mark = findChild(window, "networkUnreadMark-libera");
        verify(mark !== null, "Could not find networkUnreadMark-libera");
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
        openSeededAppWindow();
        var composer = item("messageComposer");
        mouseClick(liveHeaderButton(seed.omarchyNetworkId));
        tryCompare(appWindow, "consoleVisible", true);
        mouseClick(composer);
        typeText("omarchy status");
        compare(composer.text, "omarchy status");

        mouseClick(liveHeaderButton(seed.oftcNetworkId));
        tryCompare(appWindow, "consoleVisible", true);
        compare(appWindow.currentNetworkId, seed.oftcNetworkId);
        compare(composer.text, "");

        typeText("oftc status");
        compare(composer.text, "oftc status");

        mouseClick(liveHeaderButton(seed.omarchyNetworkId));
        tryCompare(appWindow, "currentNetworkId", seed.omarchyNetworkId);
        compare(composer.text, "omarchy status");

        mouseClick(liveHeaderButton(seed.oftcNetworkId));
        compare(composer.text, "oftc status");
        keyClick(Qt.Key_Escape);
        tryCompare(appWindow, "consoleVisible", false);
        mouseClick(namedItem(liveConversation("#omarchy")));
    }

    function test_mockSendUsesSelfNick() {
        openSeededAppWindow();
        mouseClick(namedItem(liveOftcConversation("#lab")));
        tryCompare(appWindow, "currentConversationId", seed.oftcNetworkId + "\n#lab");
        var messages = item("messageList");
        var previousCount = messages.model.rowCount();
        var composer = item("messageComposer");
        mouseClick(composer);
        typeText("hello oak");
        keyClick(Qt.Key_Return);
        verify(seed.echoLastOftcPrivmsg());
        tryVerify(function() {
            return messages.model.rowCount() === previousCount + 1;
        });
        compare(field(messages.model, previousCount, "author"), "oak");
        compare(field(messages.model, previousCount, "body"), "hello oak");
        mouseClick(namedItem(liveConversation("#omarchy")));
    }

    function test_oftcDirectHasOwnHistory() {
        openSeededAppWindow();
        mouseClick(namedItem(liveOftcConversation("#lab")));
        tryCompare(appWindow, "currentConversationId", seed.oftcNetworkId + "\n#lab");
        appWindow.openDirectMessage("ness");
        tryCompare(appWindow, "currentConversation", "ness");
        tryCompare(appWindow, "currentConversationId", seed.oftcNetworkId + "\nness");
        waitForRendering(appWindow.contentItem);
        var messages = item("messageList");
        var previousCount = messages.model.rowCount();
        mouseClick(item("messageComposer"));
        typeText("secret to ness");
        keyClick(Qt.Key_Return);
        verify(seed.echoLastOftcPrivmsg());
        tryVerify(function() {
            return messages.model.rowCount() === previousCount + 1;
        });

        mouseClick(namedItem(liveOftcConversation("#omarchy")));
        tryCompare(appWindow, "currentConversationId", seed.oftcNetworkId + "\n#omarchy");
        var bodies = [];
        var row = 0;
        for (; row < item("messageList").model.rowCount(); ++row)
            bodies.push(field(item("messageList").model, row, "body"));
        compare(bodies.indexOf("secret to ness"), -1);

        mouseClick(namedItem(liveOftcConversation("ness")));
        tryCompare(appWindow, "currentConversation", "ness");
        compare(field(item("messageList").model, item("messageList").model.rowCount() - 1, "body"),
                "secret to ness");
        keyClick(Qt.Key_W, Qt.ControlModifier);
        mouseClick(namedItem(liveConversation("#omarchy")));
    }

    function test_keyboardNavigationRevealsSidebarRow() {
        openSeededAppWindow();
        mouseClick(namedItem(liveConversation("#omarchy")));
        tryCompare(appWindow, "currentConversationId",
                   seed.omarchyNetworkId + "\n#omarchy");
        var extra = 0;
        for (; extra < 16; ++extra)
            appWindow.openDirectMessage("bulk" + extra);
        appWindow.selectConversation("#omarchy", seed.omarchyNetworkId);
        tryCompare(appWindow, "currentConversation", "#omarchy");
        waitForRendering(appWindow.contentItem);

        var row = namedItem(liveConversation("bulk15"));
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
        mouseClick(namedItem(liveConversation("#omarchy")));
    }

    function test_networkChoiceIsKeyboardAccessible() {
        restoreNamedConnection();
        namedNetworks.append({
            networkId: "oftc",
            displayName: "irc.oftc.net",
            stored: true,
            selected: false,
            iconColor: 1
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
