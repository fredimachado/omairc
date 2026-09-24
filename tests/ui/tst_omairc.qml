import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Window
import QtTest
import Omairc.Test 1.0
import "../../src" as Omairc
import "../../src/qml" as OmaircQml

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
            iconUrl: ""
            collapsed: false
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
            iconUrl: ""
            collapsed: false
        }
    }

    QtObject {
        id: fakeConnection

        property string host: "irc.libera.chat"
        property string name: "irc.libera.chat"
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
        property bool canDisconnect: false

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
        property int disconnectSelectedCalls: 0

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

        function disconnectSelected() {
            disconnectSelectedCalls += 1;
            return false;
        }

        function isNetworkCollapsed(networkId) {
            return false;
        }

        function setNetworkCollapsed(networkId, collapsed) {
        }

        function setAllNetworksCollapsed(collapsed) {
        }

        function moveNetwork(networkId, delta) {
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
        id: nullIrcWindowComponent

        Omairc.OmaircWindow {
            backend: fakeBackend
            irc: null
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
        id: shortTranscriptComponent

        OmaircQml.TranscriptList {
            width: 400
            height: 400
            property alias rows: shortRows
            model: ListModel {
                id: shortRows
                ListElement { body: "first" }
            }
            delegate: Rectangle {
                required property string body
                width: ListView.view.width
                height: 20
            }
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
            presence: ""
            avatar: ""
            bot: false
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
            presence: "online"
            avatar: ""
            bot: false
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
        property bool reopenDirectMessages: true
        property bool openConversationsAtUnread: false
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
        property bool reopenDirectMessages: true
        property bool openConversationsAtUnread: false
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

        ListElement { nick: "anna"; label: "anna"; status: "writing docs"; away: true; avatar: ""; bot: false }
    }

    ListModel {
        id: selfAwayMembers

        ListElement { nick: "live-nick"; label: "~live-nick"; status: ""; away: true; avatar: ""; bot: false }
        ListElement { nick: "anna"; label: "&anna"; status: ""; away: true; avatar: ""; bot: false }
    }

    ListModel {
        id: prefixedMembers

        ListElement { nick: "mira"; label: "@mira"; status: ""; away: false; avatar: ""; bot: false }
        ListElement { nick: "sol"; label: "+sol"; status: ""; away: false; avatar: ""; bot: false }
        ListElement { nick: "anna"; label: "anna"; status: ""; away: false; avatar: ""; bot: false }
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
        property bool reopenDirectMessages: true
        property bool openConversationsAtUnread: false
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
        property bool reopenDirectMessages: true
        property bool openConversationsAtUnread: false
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
        property string name: "irc.libera.chat"
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
        property bool canDisconnect: false
        property int passwordSetCalls: 0
        property int disconnectSelectedCalls: 0
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
            name = snap.name;
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
            for (var row = 0; row < namedNetworks.count; ++row) {
                var item = namedNetworks.get(row);
                namedNetworks.setProperty(row, "selected",
                    item.networkId === networkId);
                if (item.networkId !== networkId)
                    continue;
                displayName = item.displayName;
                var rowHost = item.host;
                var rowName = item.name;
                host = (rowHost !== undefined && String(rowHost).length > 0)
                    ? String(rowHost) : item.displayName;
                name = (rowName !== undefined && String(rowName).length > 0)
                    ? String(rowName) : item.displayName;
                if (item.nick !== undefined && String(item.nick).length > 0)
                    nick = String(item.nick);
            }
            selectedNetworkChanged();
        }

        function add() {
            addedFromSnapshot = {
                networkId: selectedNetworkId,
                host: host,
                name: name,
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
                iconColor: 1,
                iconUrl: "",
                collapsed: false
            });
            selectedNetworkId = "new-id";
            displayName = "New network";
            selectedNetworkChanged();
            host = "";
            name = "";
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

        function disconnectSelected() {
            disconnectSelectedCalls += 1;
            return true;
        }

        function isNetworkCollapsed(networkId) {
            for (var row = 0; row < namedNetworks.count; ++row) {
                if (namedNetworks.get(row).networkId === networkId)
                    return namedNetworks.get(row).collapsed === true;
            }
            return false;
        }

        function setNetworkCollapsed(networkId, collapsed) {
            for (var row = 0; row < namedNetworks.count; ++row) {
                if (namedNetworks.get(row).networkId === networkId) {
                    namedNetworks.setProperty(row, "collapsed", collapsed);
                    return;
                }
            }
        }

        function setAllNetworksCollapsed(collapsed) {
            for (var row = 0; row < namedNetworks.count; ++row)
                namedNetworks.setProperty(row, "collapsed", collapsed);
        }

        function moveNetwork(networkId, delta) {
            return false;
        }
    }

    Component {
        id: liveWindowComponent

        Omairc.OmaircWindow {
            backend: fakeBackend
            irc: liveIrc
            connection: namedConnection
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

    function resetGatedIrc() {
        gatedIrc.currentNick = "live-nick";
        gatedIrc.selfAway = false;
        gatedIrc.selectedTarget = "#omarchy";
        gatedIrc.selectedNetworkId = "libera";
        gatedIrc.topic = "A cozy corner for Omarchy users and builders.";
        gatedIrc.isChannel = true;
        gatedIrc.peopleCount = 1;
        gatedIrc.connectionStatus = "Connected";
        gatedIrc.lastError = "";
        gatedIrc.conversationEpoch = 0;
        gatedIrc.hasAwayPresence = false;
        gatedIrc.hasMemberStatus = false;
        gatedIrc.hasTyping = false;
        gatedIrc.reopenDirectMessages = true;
        gatedIrc.openConversationsAtUnread = false;
        gatedIrc.typingNicks = ["anna"];
        gatedIrc.conversations = liveConversations;
        gatedIrc.messages = liveMessages;
        gatedIrc.members = gatedMembers;
        gatedIrc.statusConsole = liveConsole;
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
        liveConsole.open = false;
        liveConsole.networkId = "libera";
        resetGatedIrc();
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

    function liveCollapseButton(networkId) {
        return namedItem("networkCollapseButton-" + networkId);
    }

    function liveChannelsHeading(networkId) {
        return namedItem("channelsHeading-" + networkId);
    }

    function liveDirectsHeading(networkId) {
        return namedItem("directsHeading-" + networkId);
    }

    function sidebarRowShown(objectName) {
        var row = namedItem(objectName);
        return row.visible && row.height > 0;
    }

    function statusTitle(networkId) {
        return networkDisplayName(networkId) + " Status";
    }

    function findNamedIn(root, objectName) {
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
        walk(root.contentItem ? root.contentItem : root);
        return visible ? visible : any;
    }

    function findNamed(objectName) {
        return findNamedIn(appWindow, objectName);
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

    function renderedMembers() {
        var members = item("membersList");
        var rows = [];
        var row = 0;
        for (; row < members.count; ++row) {
            members.positionViewAtIndex(row, ListView.Contain);
            waitForRendering(appWindow.contentItem);
            var delegate = members.itemAtIndex(row);
            if (delegate && delegate.nick.length > 0)
                rows.push(delegate.nick + "=" + delegate.label);
        }
        return rows;
    }

    function openSeededAppWindow() {
        destroyAppWindowAndSeed();
        seed = createTemporaryObject(seedComponent, testCase);
        verify(seed !== null, "SeededIrcFixture should construct");
        verify(seed.open(), seed.lastError);
        verify(seed.connection, "seeded window needs a real IrcConnection");
        // Product default is on. Seeded tests pin to the end unless they opt in.
        seed.irc.openConversationsAtUnread = false;
        appWindow = createTemporaryObject(seededWindowComponent, testCase, {
            backend: seed.backend,
            irc: seed.irc,
            slashCommands: seed.slash,
            connection: seed.connection,
            avatarStore: appAvatarStore
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
            iconColor: 1,
            iconUrl: "",
            collapsed: false
        });
        namedConnection.selectedNetworkId = "libera";
        namedConnection.displayName = "irc.libera.chat";
        namedConnection.host = "irc.libera.chat";
        namedConnection.name = "irc.libera.chat";
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
        namedConnection.canDisconnect = false;
        namedConnection.disconnectSelectedCalls = 0;
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

    function clickAboutUpdateStatus() {
        // The line's mouse area does not take clicks on the frame it becomes
        // enabled. The card background swallows that click.
        var status = item("aboutUpdateStatus");
        tryVerify(function() { return status.visible && status.height > 0; });
        wait(0);
        mouseClick(status);
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

    function openNickSheet() {
        keyClick(Qt.Key_K, Qt.ControlModifier | Qt.ShiftModifier);
        var sheet = item("nickSheet");
        tryCompare(sheet, "opened", true);
        tryCompare(item("nickFilter"), "activeFocus", true);
        return sheet;
    }

    function openInboxSheet() {
        keyClick(Qt.Key_A, Qt.ControlModifier | Qt.ShiftModifier);
        var sheet = item("inboxSheet");
        tryCompare(sheet, "opened", true);
        return sheet;
    }

    function inboxBadgeText() {
        var mark = namedItem("inboxMark");
        var badge = findChild(mark, "inboxBadgeText");
        return badge ? badge.text : "";
    }

    function nickModelRows() {
        var model = item("nickModel");
        var rows = [];
        var index = 0;
        for (; index < model.count; ++index)
            rows.push(model.get(index).name + "=" + model.get(index).label);
        return rows;
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

    function shortcutCtrlLabel() {
        return Qt.platform.os === "osx" || Qt.platform.os === "macos" ? "Cmd" : "Ctrl";
    }

    function shortcutAltLabel() {
        return Qt.platform.os === "osx" || Qt.platform.os === "macos" ? "Option" : "Alt";
    }

    function shortcutCommandModifier() {
        return Qt.platform.os === "osx" || Qt.platform.os === "macos"
            ? Qt.MetaModifier : Qt.ControlModifier;
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

    function messageIndexWithBody(body) {
        var list = item("messageList");
        var row = 0;
        for (; row < list.model.rowCount(); ++row) {
            if (field(list.model, row, "body") === body)
                return row;
        }
        fail("Could not find message body " + body);
        return -1;
    }

    function renderedRowWithBody(body) {
        var list = item("messageList");
        var index = messageIndexWithBody(body);
        list.positionViewAtIndex(index, ListView.Contain);
        waitForRendering(appWindow.contentItem);
        tryVerify(function() {
            list.positionViewAtIndex(index, ListView.Contain);
            return list.itemAtIndex(index) !== null;
        }, 1000, "Message row " + index + " (" + body + ") should be rendered");
        return list.itemAtIndex(index);
    }

    function clickNamedInRow(row, childName) {
        var target = findChild(row, childName);
        verify(target !== null, "Could not find " + childName);
        verify(target.visible, childName + " should be visible");
        mouseClick(target);
    }

    function selectOmarchy() {
        appWindow.selectConversation("#omarchy", seed.omarchyNetworkId);
        tryCompare(appWindow, "currentConversation", "#omarchy");
        tryVerify(function() {
            return item("messageList").Accessible.name === "Messages in #omarchy";
        });
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

    function unreadMarkLabelText(mark) {
        if (!mark)
            return "";
        var kids = mark.children;
        var index = 0;
        for (; index < kids.length; ++index) {
            if (kids[index].text !== undefined && String(kids[index].text).length > 0)
                return String(kids[index].text);
        }
        return "";
    }

    function containsMirc(text) {
        return /[\u0002\u0003\u0004\u000f\u0011\u0016\u001d\u001e\u001f]/.test(text);
    }

    TextEdit {
        id: clipboardProbe
        visible: false
        textFormat: TextEdit.PlainText
        width: 1
        height: 1
    }

    function writeClipboard(text) {
        clipboardProbe.clear();
        clipboardProbe.text = text;
        clipboardProbe.selectAll();
        clipboardProbe.copy();
    }

    function readClipboard() {
        clipboardProbe.clear();
        clipboardProbe.paste();
        return clipboardProbe.text;
    }

    function clipboardMatches(expected) {
        var got = readClipboard();
        if (got === expected || got === expected + "\n")
            return true;
        compare(got, expected);
        return false;
    }

    function clickCopy() {
        keyClick(Qt.Key_C, shortcutCommandModifier());
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

        waitForNewMessage(messages, previousCount, "Hello from the UI test");
        compare(field(messages.model, messages.model.rowCount() - 1, "author"), "fred");
        compare(field(messages.model, messages.model.rowCount() - 1, "body"), "Hello from the UI test");
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

    function test_jumpToNextUnreadSkipsMutedMention() {
        openSeededAppWindow();
        compare(appWindow.currentConversation, "#omarchy");
        verify(appWindow.irc.sendMessage("/mute #ricing"));
        var ricing = namedItem(liveConversation("#ricing"));
        tryCompare(ricing, "muted", true);
        compare(ricing.mention, false);

        keyClick(Qt.Key_A, Qt.AltModifier);

        tryCompare(appWindow, "currentConversation", "anna");
        compare(namedItem(liveConversation("#ricing")).muted, true);
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
        waitForNewMessage(messages, previousCount, "omarchy draft");
        compare(field(messages.model, messages.model.rowCount() - 1, "author"), "fred");
        compare(field(messages.model, messages.model.rowCount() - 1, "body"), "omarchy draft");

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
        if (typeof model.get === "function") {
            for (; row < model.rowCount(); ++row) {
                var item = model.get(row);
                if (item.networkId === networkId)
                    return item.displayName || "";
            }
            return "";
        }
        for (; row < model.rowCount(); ++row) {
            var idx = model.index(row, 0);
            if (model.data(idx, Qt.UserRole + 1) === networkId)
                return model.data(idx, Qt.UserRole + 2) || "";
        }
        return "";
    }

    function injectOmarchyChat(nick, target, body, time) {
        var hhmm = time ? time : "12:00";
        var prefix = "@time=2026-09-12T" + hhmm + ":00.000Z ";
        seed.injectOmarchy(prefix + ":" + nick + "!u@h PRIVMSG " + target
            + " :" + body + "\r\n");
    }

    function waitForRowCount(list, expected) {
        tryVerify(function() {
            return list.model.rowCount() === expected;
        });
    }

    function rowForBody(model, body) {
        var row = 0;
        for (; row < model.rowCount(); ++row) {
            if (field(model, row, "body") === body)
                return row;
        }
        return -1;
    }

    function waitForBody(list, body) {
        tryVerify(function() {
            return rowForBody(list.model, body) >= 0;
        });
    }

    function waitForNewMessage(list, previousCount, body) {
        tryVerify(function() {
            var count = list.model.rowCount();
            if (count === previousCount + 1)
                return field(list.model, previousCount, "body") === body;
            if (count === previousCount + 2)
                return field(list.model, previousCount, "kind") === "event"
                    && field(list.model, previousCount + 1, "body") === body;
            return false;
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
        waitForNewMessage(list, countBefore, "keep me");
        compare(field(list.model, list.model.rowCount() - 1, "body"), "keep me");
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

    function test_shortRevealAdoptsViewportAfterGrowth() {
        var list = createTemporaryObject(shortTranscriptComponent, testCase);
        verify(list !== null);
        verify(list.contentHeight <= list.height);

        list.revealRow(0);
        var shortGeneration = list.pinGeneration;
        list.rows.append({ body: "appended" });
        verify(list.contentHeight <= list.height);
        compare(list.pinGeneration, shortGeneration,
                "Appending should preserve deferred reveal cleanup");
        wait(0);
        compare(list.stick, list.stickFollowing);
        tryCompare(list, "pinning", false);
        verify(transcriptPinned(list));

        list.pinToUnread(0);
        var unreadGeneration = list.pinGeneration;
        list.rows.append({ body: "after unread pin" });
        compare(list.pinGeneration, unreadGeneration,
                "Appending should preserve deferred unread-pin cleanup");
        wait(0);
        compare(list.stick, list.stickFollowing);
        tryCompare(list, "pinning", false);

    }

    function test_revealRowPreservesGrowthAndCancelsOnReset() {
        openSeededAppWindow();
        var list = item("messageList");
        fillTranscriptUntilScrollable(list);

        list.revealRow(0);
        var generation = list.pinGeneration;
        compare(list.stick, list.stickDetached);
        compare(list.pinning, true);
        appendLiveMessages(list, 1, "reveal growth");
        compare(list.pinGeneration, generation,
                "A new row should not cancel deferred reveal cleanup");
        wait(0);
        compare(list.pinning, false);
        compare(list.stick, list.stickDetached);
        verify(!transcriptPinned(list));

        list.firstUnseenIndex = 0;
        list.jumpToUnseen();
        var jumpGeneration = list.pinGeneration;
        appendLiveMessages(list, 1, "unseen jump growth");
        compare(list.pinGeneration, jumpGeneration,
                "Appending should preserve deferred unseen-jump cleanup");
        wait(0);
        compare(list.pinning, false);
        compare(list.stick, list.stickDetached);

        list.revealRow(0);
        var resetGeneration = list.pinGeneration;
        prependLiveReplay(list, 1);
        verify(list.pinGeneration > resetGeneration,
               "A history-splice reset should cancel deferred reveal cleanup");
        wait(0);
        tryCompare(list, "pinning", false);

        list.revealRow(0);
        var switchGeneration = list.pinGeneration;
        compare(list.pinning, true);
        appWindow.selectConversation("#ricing", seed.omarchyNetworkId);
        tryCompare(appWindow, "currentConversation", "#ricing");
        verify(list.pinGeneration > switchGeneration,
               "A conversation switch should cancel deferred reveal cleanup");
        tryCompare(list, "pinning", false);
        compare(list.stick, list.stickFollowing);
    }

    function test_statusRevealRowAdoptsViewportAfterGrowth() {
        openSeededAppWindow();
        var list = item("consoleList");
        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);
        fillConsoleUntilScrollable(list);

        list.revealRow(0);
        var generation = list.pinGeneration;
        compare(list.stick, list.stickDetached);
        compare(list.pinning, true);
        appendLiveConsoleLines(list, 1, "reveal growth");
        compare(list.pinGeneration, generation,
                "A new Status row should preserve deferred reveal cleanup");
        wait(0);
        compare(list.pinning, false);
        compare(list.stick, list.stickDetached);
        verify(!transcriptPinned(list));
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

    function warmTranscriptRows(list) {
        // Instantiate rows so indexAt sees real heights before comparing hop size.
        keyClick(Qt.Key_PageUp);
        waitForRendering(appWindow.contentItem);
        wait(0);
        pageTranscriptToEnd(list);
        verify(transcriptPinned(list));
    }

    function assertShiftPageLeavesContentY(list) {
        var before = list.contentY;
        keyClick(Qt.Key_PageUp, Qt.ShiftModifier);
        waitForRendering(appWindow.contentItem);
        wait(0);
        compare(list.contentY, before,
                "Shift+Page Up should not scroll under an overlay");
        keyClick(Qt.Key_PageDown, Qt.ShiftModifier);
        waitForRendering(appWindow.contentItem);
        wait(0);
        compare(list.contentY, before,
                "Shift+Page Down should not scroll under an overlay");
    }

    function clickCtrlHome() {
        keyClick(Qt.Key_Home, shortcutCommandModifier());
    }

    function clickCtrlEnd() {
        keyClick(Qt.Key_End, shortcutCommandModifier());
    }

    function assertCtrlHomeEndLeavesContentY(list) {
        var before = list.contentY;
        clickCtrlHome();
        waitForRendering(appWindow.contentItem);
        wait(0);
        compare(list.contentY, before,
                "Ctrl+Home should not jump under an overlay");
        clickCtrlEnd();
        waitForRendering(appWindow.contentItem);
        wait(0);
        compare(list.contentY, before,
                "Ctrl+End should not jump under an overlay");
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

    function test_shiftPageUpShowsStatusScrollbar() {
        openSeededAppWindow();
        var composer = item("messageComposer");
        var list = item("consoleList");
        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);
        fillConsoleUntilScrollable(list);
        appendLiveConsoleLines(list, 40, "half-page console");
        waitForRendering(appWindow.contentItem);
        list.pinToEnd();
        waitForRendering(appWindow.contentItem);
        wait(0);
        mouseClick(composer);
        verify(composer.activeFocus);
        warmTranscriptRows(list);
        verify(list.contentHeight > list.height);
        var startY = list.contentY;
        verify(startY > 0);
        var startFirst = firstVisibleIndex(list);
        var bar = waitForScrollbarThumb(list, true);
        var endPosition = bar.position;

        keyClick(Qt.Key_PageUp, Qt.ShiftModifier);
        waitForRendering(appWindow.contentItem);
        wait(0);
        verify(list.contentY < startY, "Shift+Page Up should scroll Status toward older lines");
        verify(composer.activeFocus);
        waitForScrollbarThumb(list, true);
        verify(bar.position < endPosition, "Shift+Page Up should move the Status thumb up");
        var halfUpDelta = startY - list.contentY;
        verify(halfUpDelta > 0);
        verify(firstVisibleIndex(list) < startFirst);

        pageTranscriptToEnd(list);
        verify(transcriptPinned(list));
        var fullStartY = list.contentY;
        keyClick(Qt.Key_PageUp);
        waitForRendering(appWindow.contentItem);
        wait(0);
        var fullUpDelta = fullStartY - list.contentY;
        verify(fullUpDelta > halfUpDelta,
               "Shift+Page Up should hop less than Page Up on Status");
        verify(composer.activeFocus);
        compare(appWindow.consoleVisible, true);
    }

    function test_shiftPageIgnoredWhenConnectOrShortcutsOpen() {
        openSeededAppWindow();
        var composer = item("messageComposer");
        var chat = item("messageList");
        mouseClick(composer);
        verify(composer.activeFocus);
        fillTranscriptUntilScrollable(chat);
        keyClick(Qt.Key_PageUp);
        waitForRendering(appWindow.contentItem);
        wait(0);
        verify(chat.contentY > 0);
        verify(!transcriptPinned(chat));

        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        tryCompare(appWindow, "connectionOverlayVisible", true);
        tryCompare(item("connectionSheet"), "visible", true);
        assertShiftPageLeavesContentY(chat);
        compare(appWindow.connectionOverlayVisible, true);
        verify(item("connectionSheet").visible);

        keyClick(Qt.Key_Escape);
        tryCompare(appWindow, "connectionOverlayVisible", false);
        tryCompare(composer, "activeFocus", true);

        keyClick(Qt.Key_Slash, Qt.ControlModifier);
        tryCompare(item("shortcutsSheet"), "opened", true);
        assertShiftPageLeavesContentY(chat);
        compare(item("shortcutsSheet").opened, true);

        keyClick(Qt.Key_Escape);
        tryCompare(item("shortcutsSheet"), "opened", false);
        tryCompare(composer, "activeFocus", true);

        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);
        var status = item("consoleList");
        fillConsoleUntilScrollable(status);
        mouseClick(composer);
        verify(composer.activeFocus);
        keyClick(Qt.Key_PageUp);
        waitForRendering(appWindow.contentItem);
        wait(0);
        verify(status.contentY > 0);
        verify(!transcriptPinned(status));

        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        tryCompare(appWindow, "connectionOverlayVisible", true);
        tryCompare(item("connectionSheet"), "visible", true);
        assertShiftPageLeavesContentY(status);
        compare(appWindow.connectionOverlayVisible, true);

        keyClick(Qt.Key_Escape);
        tryCompare(appWindow, "connectionOverlayVisible", false);

        keyClick(Qt.Key_Slash, Qt.ControlModifier);
        tryCompare(item("shortcutsSheet"), "opened", true);
        assertShiftPageLeavesContentY(status);
        compare(item("shortcutsSheet").opened, true);
        compare(appWindow.consoleVisible, true);
    }

    function test_ctrlHomeEndJumpsTranscript() {
        openSeededAppWindow();
        var composer = item("messageComposer");
        var list = item("messageList");
        mouseClick(composer);
        verify(composer.activeFocus);
        fillTranscriptUntilScrollable(list);
        warmTranscriptRows(list);
        verify(list.contentHeight > list.height);
        var startY = list.contentY;
        verify(startY > 0);
        verify(transcriptPinned(list));

        clickCtrlHome();
        waitForRendering(appWindow.contentItem);
        wait(0);
        tryVerify(function() {
            return list.contentY < startY && list.contentY <= list.originY + 8;
        }, 1000, "Ctrl+Home should jump to the oldest lines");
        verify(list.atYBeginning || firstVisibleIndex(list) === 0,
               "Ctrl+Home should show the oldest rows");
        tryCompare(list, "stick", list.stickDetached);
        verify(!transcriptPinned(list));
        verify(composer.activeFocus);

        var frozenY = list.contentY;
        appendLiveMessages(list, 1, "ctrl-home unseen");
        waitForRendering(appWindow.contentItem);
        wait(0);
        fuzzyCompare(list.contentY, frozenY, 2);
        verify(!transcriptPinned(list));
        verify(composer.activeFocus);

        clickCtrlEnd();
        waitForRendering(appWindow.contentItem);
        wait(0);
        tryVerify(function() {
            return transcriptPinned(list);
        }, 1000, "Ctrl+End should pin the transcript to the bottom");
        verify(composer.activeFocus);

        var pinnedY = list.contentY;
        appendLiveMessages(list, 1, "ctrl-end follow");
        waitForRendering(appWindow.contentItem);
        wait(0);
        tryVerify(function() {
            return transcriptPinned(list);
        }, 1000, "Incoming rows should keep a following list at the end");
        verify(list.contentY >= pinnedY);
        verify(composer.activeFocus);
    }

    function test_ctrlHomeEndJumpsStatus() {
        openSeededAppWindow();
        var composer = item("messageComposer");
        var list = item("consoleList");
        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);
        fillConsoleUntilScrollable(list);
        mouseClick(composer);
        verify(composer.activeFocus);
        warmTranscriptRows(list);
        verify(list.contentHeight > list.height);
        var startY = list.contentY;
        verify(startY > 0);
        verify(transcriptPinned(list));
        var bar = waitForScrollbarThumb(list, true);
        var endPosition = bar.position;

        clickCtrlHome();
        waitForRendering(appWindow.contentItem);
        wait(0);
        tryVerify(function() {
            return list.contentY < startY && list.contentY <= list.originY + 8;
        }, 1000, "Ctrl+Home should jump Status to the oldest lines");
        tryCompare(list, "stick", list.stickDetached);
        verify(!transcriptPinned(list));
        verify(composer.activeFocus);
        waitForScrollbarThumb(list, true);
        verify(bar.position < endPosition, "Ctrl+Home should move the Status thumb up");

        clickCtrlEnd();
        waitForRendering(appWindow.contentItem);
        wait(0);
        tryVerify(function() {
            return transcriptPinned(list);
        }, 1000, "Ctrl+End should pin Status to the bottom");
        verify(composer.activeFocus);
        compare(appWindow.consoleVisible, true);
    }

    function test_homeEndMoveComposerCaret() {
        openSeededAppWindow();
        var composer = item("messageComposer");
        var list = item("messageList");
        mouseClick(composer);
        verify(composer.activeFocus);
        fillTranscriptUntilScrollable(list);
        verify(transcriptPinned(list));
        typeText("hello world");
        compare(composer.text, "hello world");
        composer.cursorPosition = 5;
        compare(composer.cursorPosition, 5);
        var before = list.contentY;

        keyClick(Qt.Key_Home);
        compare(composer.cursorPosition, 0);
        compare(list.contentY, before,
                "Home should not jump the transcript");
        verify(composer.activeFocus);

        keyClick(Qt.Key_End);
        compare(composer.cursorPosition, composer.text.length);
        compare(list.contentY, before,
                "End should not jump the transcript");
        verify(composer.activeFocus);
        verify(transcriptPinned(list));
    }

    function test_ctrlHomeEndIgnoredWhenConnectOrShortcutsOpen() {
        openSeededAppWindow();
        var composer = item("messageComposer");
        var chat = item("messageList");
        mouseClick(composer);
        verify(composer.activeFocus);
        fillTranscriptUntilScrollable(chat);
        keyClick(Qt.Key_PageUp);
        waitForRendering(appWindow.contentItem);
        wait(0);
        verify(chat.contentY > 0);
        verify(!transcriptPinned(chat));

        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        tryCompare(appWindow, "connectionOverlayVisible", true);
        tryCompare(item("connectionSheet"), "visible", true);
        assertCtrlHomeEndLeavesContentY(chat);
        compare(appWindow.connectionOverlayVisible, true);
        verify(item("connectionSheet").visible);

        keyClick(Qt.Key_Escape);
        tryCompare(appWindow, "connectionOverlayVisible", false);
        tryCompare(composer, "activeFocus", true);

        keyClick(Qt.Key_Slash, Qt.ControlModifier);
        tryCompare(item("shortcutsSheet"), "opened", true);
        assertCtrlHomeEndLeavesContentY(chat);
        compare(item("shortcutsSheet").opened, true);

        keyClick(Qt.Key_Escape);
        tryCompare(item("shortcutsSheet"), "opened", false);
        tryCompare(composer, "activeFocus", true);

        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);
        var status = item("consoleList");
        fillConsoleUntilScrollable(status);
        mouseClick(composer);
        verify(composer.activeFocus);
        keyClick(Qt.Key_PageUp);
        waitForRendering(appWindow.contentItem);
        wait(0);
        verify(status.contentY > 0);
        verify(!transcriptPinned(status));

        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        tryCompare(appWindow, "connectionOverlayVisible", true);
        tryCompare(item("connectionSheet"), "visible", true);
        assertCtrlHomeEndLeavesContentY(status);
        compare(appWindow.connectionOverlayVisible, true);

        keyClick(Qt.Key_Escape);
        tryCompare(appWindow, "connectionOverlayVisible", false);

        keyClick(Qt.Key_Slash, Qt.ControlModifier);
        tryCompare(item("shortcutsSheet"), "opened", true);
        assertCtrlHomeEndLeavesContentY(status);
        compare(item("shortcutsSheet").opened, true);
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
        waitForNewMessage(list, previousCount, "sent while following");
        waitForRendering(appWindow.contentItem);
        wait(0);
        tryVerify(function() {
            return transcriptPinned(list);
        }, 1000, "Sending should keep the list pinned to the end");
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
        compare(list.hasUnreadMark, false);
        var jump = item("messageUnseenJump");
        tryCompare(jump, "visible", true);
        var burst = findChild(jump, "bounceBurst");
        verify(burst !== null, "Could not find bounceBurst on the jump chip");
        var dot = findChild(jump, "unseenJumpDot");
        verify(dot !== null, "Could not find unseenJumpDot on the jump chip");
        compare(dot.visible, true);
        compare(burst.loops, 5);
        tryCompare(burst, "running", true);

        appendLiveMessages(list, 24, "later unseen");
        waitForRendering(appWindow.contentItem);
        wait(0);
        fuzzyCompare(list.contentY, frozenY, 2);
        compare(list.firstUnseenIndex, previousCount);
        compare(burst.loops, 5);
        tryCompare(burst, "running", true);

        var unseen = list.firstUnseenIndex;
        mouseClick(jump);
        waitForRendering(appWindow.contentItem);
        wait(0);
        tryCompare(jump, "visible", false);
        compare(dot.visible, false);
        compare(burst.loops, 5);
        compare(burst.running, false);
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
        waitForBody(list, "after event");

        var leadRow = rowForBody(list.model, "group lead");
        var groupedRow = rowForBody(list.model, "group continuation");
        var newMinuteRow = rowForBody(list.model, "changed minute");
        var newSenderRow = rowForBody(list.model, "changed sender");
        var eventRowIndex = rowForBody(list.model, "rio left");
        var afterEventRow = rowForBody(list.model, "after event");
        compare(leadRow, start);
        compare(groupedRow, leadRow + 1);
        compare(field(list.model, groupedRow, "author"), "anna");
        compare(field(list.model, groupedRow, "time"), field(list.model, leadRow, "time"));
        compare(field(list.model, groupedRow, "kind"), "message");

        var lead = renderedMessageRow(list, leadRow);
        var grouped = renderedMessageRow(list, groupedRow);
        var newMinute = renderedMessageRow(list, newMinuteRow);
        var newSender = renderedMessageRow(list, newSenderRow);
        var eventRow = renderedMessageRow(list, eventRowIndex);
        var afterEvent = renderedMessageRow(list, afterEventRow);

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

    function test_unreadMarkSitsBeforeFirstUnseenLine() {
        openSeededAppWindow();
        mouseClick(namedItem(liveConversation("#desktop")));
        tryCompare(appWindow, "currentConversation", "#desktop");

        injectOmarchyChat("anna", "#omarchy", "unread-mark-first-zx9");
        injectOmarchyChat("dax", "#omarchy", "unread-mark-second-zx9");

        mouseClick(namedItem(liveConversation("#omarchy")));
        tryCompare(appWindow, "currentConversation", "#omarchy");
        var list = item("messageList");
        waitForBody(list, "unread-mark-first-zx9");
        waitForBody(list, "unread-mark-second-zx9");

        var first = rowForBody(list.model, "unread-mark-first-zx9");
        verify(first > 0, "The first unseen line should not lead the buffer");
        var markRow = list.model.unreadMarkRow();
        compare(markRow, first - 1);
        compare(field(list.model, markRow, "kind"), "unread");
        compare(field(list.model, markRow, "body"), "");
        compare(field(list.model, markRow, "author"), "");
        compare(field(list.model, markRow, "time"), "");
        compare(rowForBody(list.model, "unread-mark-second-zx9"), first + 1);

        var row = renderedMessageRow(list, markRow);
        var mark = findChild(row, "unreadMark");
        verify(mark !== null, "Could not find unreadMark on the message row");
        verify(mark.visible);
        compare(unreadMarkLabelText(mark), "New messages");
        compare(findNamedIn(list, "unreadMark"), mark);
    }

    function test_unreadMarkBreaksSameAuthorMinuteGrouping() {
        openSeededAppWindow();
        var list = item("messageList");
        injectOmarchyChat("anna", "#omarchy", "group-before-mark", "13:37");
        waitForBody(list, "group-before-mark");

        mouseClick(namedItem(liveConversation("#desktop")));
        tryCompare(appWindow, "currentConversation", "#desktop");
        injectOmarchyChat("anna", "#omarchy", "group-after-mark", "13:37");

        mouseClick(namedItem(liveConversation("#omarchy")));
        tryCompare(appWindow, "currentConversation", "#omarchy");
        waitForBody(list, "group-after-mark");

        var beforeRow = rowForBody(list.model, "group-before-mark");
        var afterRow = rowForBody(list.model, "group-after-mark");
        var markRow = list.model.unreadMarkRow();
        compare(markRow, beforeRow + 1);
        compare(afterRow, markRow + 1);
        compare(field(list.model, afterRow, "author"), "anna");
        compare(field(list.model, afterRow, "time"),
                field(list.model, beforeRow, "time"));
        compare(appWindow.continuesMessageGroup(
                    list.model, afterRow, "anna",
                    field(list.model, afterRow, "time"), "message", "live"),
                false);

        var after = renderedMessageRow(list, afterRow);
        assertMessageChrome(after, true, "group-after-mark");
    }

    function test_ctrlFSkipsUnreadMarkLabel() {
        openSeededAppWindow();
        mouseClick(namedItem(liveConversation("#desktop")));
        tryCompare(appWindow, "currentConversation", "#desktop");
        injectOmarchyChat("anna", "#omarchy", "zx9-surrounding-unique");
        mouseClick(namedItem(liveConversation("#omarchy")));
        tryCompare(appWindow, "currentConversation", "#omarchy");
        var list = item("messageList");
        waitForBody(list, "zx9-surrounding-unique");

        var markRow = list.model.unreadMarkRow();
        verify(markRow >= 0, "The unread mark should be in the transcript");
        compare(field(list.model, markRow, "body"), "");

        var composer = item("messageComposer");
        mouseClick(composer);
        keyClick(Qt.Key_F, Qt.ControlModifier);
        tryCompare(appWindow, "findActive", true);
        typeText("new");
        waitForRendering(appWindow.contentItem);
        wait(0);
        verify(appWindow.findIndex !== markRow,
               "Ctrl+F for new must not land on the unread mark");
        verify(appWindow.findNextMatch(true) !== markRow);
        if (appWindow.findIndex >= 0)
            verify(field(list.model, appWindow.findIndex, "kind") !== "unread");

        composer.selectAll();
        typeText("new messages");
        waitForRendering(appWindow.contentItem);
        wait(0);
        compare(appWindow.findNextMatch(true), -1);
        verify(appWindow.findIndex !== markRow);

        keyClick(Qt.Key_Escape);
        tryCompare(appWindow, "findActive", false);
        compare(appWindow.findIndex, -1);
        keyClick(Qt.Key_F, Qt.ControlModifier);
        tryCompare(appWindow, "findActive", true);
        composer.text = "qqq-no-such-find-needle";
        waitForRendering(appWindow.contentItem);
        wait(0);
        compare(appWindow.findNextMatch(true), -1);
        compare(appWindow.findIndex, -1);
    }

    function injectUnreadWhileAway(target, firstBody, secondBody, extra) {
        injectOmarchyChat("anna", target, firstBody);
        injectOmarchyChat("dax", target, secondBody);
        var count = extra === undefined ? 0 : extra;
        var index = 0;
        for (index = 0; index < count; ++index) {
            var minute = index < 10 ? "0" + index : "" + index;
            injectOmarchyChat("mira", target, "open-at-unread filler " + index,
                              "11:" + minute);
        }
    }

    function waitForOpenAtUnreadViewport(list, markRow) {
        waitForRendering(appWindow.contentItem);
        wait(0);
        tryVerify(function() {
            return firstVisibleIndex(list) === markRow;
        }, 1000, "Switching should land the unread mark at the start of the view");
        verify(unseenIsInView(list, markRow));
        verify(!transcriptPinned(list),
               "A long backlog should keep the unread mark off the last page");
        compare(list.stick, list.stickDetached);
    }

    function test_openAtUnreadOffSwitchWithUnreadPinsToEnd() {
        openSeededAppWindow();
        seed.irc.openConversationsAtUnread = false;
        compare(seed.irc.openConversationsAtUnread, false);
        var list = item("messageList");
        fillTranscriptUntilScrollable(list);

        mouseClick(namedItem(liveConversation("#desktop")));
        tryCompare(appWindow, "currentConversation", "#desktop");
        injectUnreadWhileAway("#omarchy", "open-at-unread-off-first-zx9",
                              "open-at-unread-off-second-zx9", 24);

        mouseClick(namedItem(liveConversation("#omarchy")));
        tryCompare(appWindow, "currentConversation", "#omarchy");
        waitForBody(list, "open-at-unread-off-first-zx9");
        waitForBody(list, "open-at-unread-off-second-zx9");
        waitForRendering(appWindow.contentItem);
        wait(0);

        var first = rowForBody(list.model, "open-at-unread-off-first-zx9");
        var markRow = list.model.unreadMarkRow();
        verify(markRow >= 0, "Open-at-unread off still keeps the unread mark");
        compare(markRow, first - 1);
        compare(field(list.model, markRow, "kind"), "unread");
        tryVerify(function() {
            return transcriptPinned(list);
        }, 1000, "Open-at-unread off should still pin a switched transcript to the end");
        verify(firstVisibleIndex(list) !== markRow);
    }

    function test_unfocusedArrivalPinsToNewMessagesMarkOnReturn() {
        openSeededAppWindow();
        var list = item("messageList");
        fillTranscriptUntilScrollable(list);

        appWindow.windowFocusLost();
        injectOmarchyChat("anna", "#omarchy", "unfocused-first-zx9");
        injectOmarchyChat("dax", "#omarchy", "unfocused-second-zx9");
        var index = 0;
        for (index = 0; index < 24; ++index) {
            var minute = index < 10 ? "0" + index : "" + index;
            injectOmarchyChat("mira", "#omarchy", "unfocused filler " + index,
                              "11:" + minute);
        }
        waitForBody(list, "unfocused-first-zx9");
        waitForBody(list, "unfocused-second-zx9");
        waitForRendering(appWindow.contentItem);
        wait(0);

        var first = rowForBody(list.model, "unfocused-first-zx9");
        verify(first > 0, "The first unfocused line should not lead the buffer");
        var markRow = list.model.unreadMarkRow();
        verify(markRow >= 0, "Unfocused arrivals should plant the New messages mark");
        compare(markRow, first - 1);
        compare(field(list.model, markRow, "kind"), "unread");
        compare(rowForBody(list.model, "unfocused-second-zx9"), first + 1);

        appWindow.windowFocusGained();
        waitForRendering(appWindow.contentItem);
        wait(0);
        tryVerify(function() {
            return firstVisibleIndex(list) === markRow;
        }, 1000, "Focus return should land the New messages mark at the start of the view");
        verify(!transcriptPinned(list),
               "A long unfocused backlog should keep the mark off the last page");
        compare(list.stick, list.stickDetached);

        var row = list.itemAtIndex(markRow);
        verify(row !== null, "The New messages mark row should be in view");
        var mark = findChild(row, "unreadMark");
        verify(mark !== null && mark.visible);
        compare(unreadMarkLabelText(mark), "New messages");
    }

    function test_unreadMarkShowsScrollDownUntilBottom() {
        openSeededAppWindow();
        var list = item("messageList");
        fillTranscriptUntilScrollable(list);

        appWindow.windowFocusLost();
        injectOmarchyChat("anna", "#omarchy", "scroll-down-first-zx9");
        injectOmarchyChat("dax", "#omarchy", "scroll-down-second-zx9");
        var index = 0;
        for (index = 0; index < 24; ++index) {
            var minute = index < 10 ? "0" + index : "" + index;
            injectOmarchyChat("mira", "#omarchy", "scroll-down filler " + index,
                              "11:" + minute);
        }
        waitForBody(list, "scroll-down-first-zx9");
        waitForRendering(appWindow.contentItem);
        wait(0);

        var markRow = list.model.unreadMarkRow();
        verify(markRow >= 0, "Unfocused arrivals should plant the New messages mark");

        tryVerify(function() {
            return transcriptPinned(list);
        }, 1000, "Unfocused backlog should stay pinned while the window is inactive");
        var jump = item("messageUnseenJump");
        var burst = findChild(jump, "bounceBurst");
        verify(burst !== null, "Could not find bounceBurst on the jump chip");
        var dot = findChild(jump, "unseenJumpDot");
        verify(dot !== null, "Could not find unseenJumpDot on the jump chip");
        compare(jump.visible, false);
        compare(burst.loops, 5);
        compare(burst.running, false);
        compare(dot.visible, false);

        appWindow.windowFocusGained();
        waitForRendering(appWindow.contentItem);
        wait(0);
        tryVerify(function() {
            return firstVisibleIndex(list) === markRow;
        }, 1000, "Focus return should land on the unread mark");
        verify(!transcriptPinned(list),
               "Unread backlog below the mark should leave room to scroll down");

        tryCompare(jump, "visible", true);
        compare(burst.loops, 5);
        compare(burst.running, false);
        compare(dot.visible, true);

        list.pinToEnd();
        waitForRendering(appWindow.contentItem);
        wait(0);
        tryVerify(function() {
            return transcriptPinned(list);
        }, 1000, "Pinning to end should reach the bottom");
        tryCompare(jump, "visible", false);
        compare(burst.loops, 5);
        tryCompare(burst, "running", false);
        compare(dot.visible, false);

        list.pinToUnread(markRow);
        waitForRendering(appWindow.contentItem);
        wait(0);
        verify(!transcriptPinned(list));
        tryCompare(jump, "visible", true);
        compare(burst.loops, 5);
        compare(burst.running, false);
        compare(dot.visible, true);

        appendLiveMessages(list, 1, "detached-arrival");
        waitForRendering(appWindow.contentItem);
        wait(0);
        verify(!transcriptPinned(list));
        tryCompare(jump, "visible", true);
        compare(dot.visible, true);
        compare(burst.loops, 5);
        tryCompare(burst, "running", true);

        mouseClick(jump);
        waitForRendering(appWindow.contentItem);
        wait(0);
        tryCompare(jump, "visible", false);
        compare(dot.visible, false);
        compare(burst.loops, 5);
        tryCompare(burst, "running", false);
        tryVerify(function() {
            return transcriptPinned(list);
        }, 1000, "Jump should scroll to the latest messages");
    }

    function test_openAtUnreadLandsOnMarkWhenSwitchingAwayAndBack() {
        openSeededAppWindow();
        var list = item("messageList");
        fillTranscriptUntilScrollable(list);

        mouseClick(namedItem(liveConversation("#desktop")));
        tryCompare(appWindow, "currentConversation", "#desktop");
        injectUnreadWhileAway("#omarchy", "open-at-unread-on-first-zx9",
                              "open-at-unread-on-second-zx9", 24);
        seed.irc.openConversationsAtUnread = true;

        mouseClick(namedItem(liveConversation("#omarchy")));
        tryCompare(appWindow, "currentConversation", "#omarchy");
        waitForBody(list, "open-at-unread-on-first-zx9");
        waitForBody(list, "open-at-unread-on-second-zx9");

        var first = rowForBody(list.model, "open-at-unread-on-first-zx9");
        verify(first > 0, "The first unseen line should not lead the buffer");
        var markRow = list.model.unreadMarkRow();
        compare(markRow, first - 1);
        compare(field(list.model, markRow, "kind"), "unread");
        compare(rowForBody(list.model, "open-at-unread-on-second-zx9"), first + 1);
        waitForOpenAtUnreadViewport(list, markRow);

        var row = list.itemAtIndex(markRow);
        verify(row !== null, "The unread mark row should be in view");
        var mark = findChild(row, "unreadMark");
        verify(mark !== null && mark.visible);
        compare(unreadMarkLabelText(mark), "New messages");
        compare(field(list.model, first, "body"), "open-at-unread-on-first-zx9");
    }

    function test_openAtUnreadLandsOnMarkWhenQuerySwitchesConversation() {
        openSeededAppWindow();
        mouseClick(namedItem(liveConversation("anna")));
        tryCompare(appWindow, "currentConversation", "anna");
        var list = item("messageList");
        var start = list.model.rowCount();
        var index = 0;
        for (index = 0; index < 24; ++index) {
            var minute = index < 10 ? "0" + index : "" + index;
            injectOmarchyChat("anna", "fred", "scroll line " + index, "10:" + minute);
        }
        waitForRowCount(list, start + 24);
        waitForRendering(appWindow.contentItem);
        list.pinToEnd();
        waitForRendering(appWindow.contentItem);
        wait(0);
        verify(list.contentHeight > list.height);
        verify(transcriptPinned(list));

        mouseClick(namedItem(liveConversation("#omarchy")));
        tryCompare(appWindow, "currentConversation", "#omarchy");
        injectOmarchyChat("anna", "fred", "open-at-unread-query-first-zx9");
        injectOmarchyChat("anna", "fred", "open-at-unread-query-second-zx9");
        for (index = 0; index < 24; ++index) {
            var unreadMinute = index < 10 ? "0" + index : "" + index;
            injectOmarchyChat("anna", "fred", "open-at-unread filler " + index,
                              "11:" + unreadMinute);
        }
        seed.irc.openConversationsAtUnread = true;

        var composer = item("messageComposer");
        mouseClick(composer);
        typeText("/query anna hello");
        compare(composer.text, "/query anna hello");
        if (item("slashCompleteList").visible)
            keyClick(Qt.Key_Escape);
        keyClick(Qt.Key_Return);

        tryCompare(appWindow, "currentConversation", "anna");
        waitForBody(list, "open-at-unread-query-first-zx9");
        waitForBody(list, "open-at-unread-query-second-zx9");
        verify(seed.echoLastOmarchyPrivmsg());
        waitForBody(list, "hello");

        var first = rowForBody(list.model, "open-at-unread-query-first-zx9");
        verify(first > 0, "The first unseen line should not lead the buffer");
        var markRow = list.model.unreadMarkRow();
        compare(markRow, first - 1);
        compare(field(list.model, markRow, "kind"), "unread");
        compare(rowForBody(list.model, "open-at-unread-query-second-zx9"), first + 1);
        waitForOpenAtUnreadViewport(list, markRow);
        verify(rowForBody(list.model, "hello") > first);
    }

    function test_openAtUnreadWithoutMarkPinsToEnd() {
        openSeededAppWindow();
        seed.irc.openConversationsAtUnread = true;
        mouseClick(namedItem(liveConversation("#desktop")));
        tryCompare(appWindow, "currentConversation", "#desktop");
        var list = item("messageList");
        fillTranscriptUntilScrollable(list);

        mouseClick(namedItem(liveConversation("#omarchy")));
        tryCompare(appWindow, "currentConversation", "#omarchy");
        mouseClick(namedItem(liveConversation("#desktop")));
        tryCompare(appWindow, "currentConversation", "#desktop");
        waitForRendering(appWindow.contentItem);
        wait(0);

        compare(list.model.unreadMarkRow(), -1);
        tryVerify(function() {
            return transcriptPinned(list);
        }, 1000, "A caught-up conversation should still pin to the end");
        compare(list.stick, list.stickFollowing);
    }

    function test_openAtUnreadLeavesAlreadyViewingViewport() {
        openSeededAppWindow();
        seed.irc.openConversationsAtUnread = true;
        var list = item("messageList");
        fillTranscriptUntilScrollable(list);

        keyClick(Qt.Key_PageUp);
        waitForRendering(appWindow.contentItem);
        tryCompare(list, "stick", list.stickDetached);
        verify(!transcriptPinned(list));
        var frozenY = list.contentY;
        var frozenStick = list.stick;

        appWindow.selectConversation("#omarchy", seed.omarchyNetworkId);
        tryCompare(appWindow, "currentConversation", "#omarchy");
        waitForRendering(appWindow.contentItem);
        wait(0);
        waitForRendering(appWindow.contentItem);

        fuzzyCompare(list.contentY, frozenY, 2);
        compare(list.stick, frozenStick);
        verify(!transcriptPinned(list));
    }

    function test_replayAndLiveSameAuthorMinuteDoNotGroup() {
        openSeededAppWindow();
        mouseClick(namedItem(liveConversation("anna")));
        tryCompare(appWindow, "currentConversation", "anna");
        waitForRendering(appWindow.contentItem);
        var list = item("messageList");
        seed.injectOmarchy(
            ":znc.in BATCH +hx znc.in/playback anna\r\n"
            + "@batch=hx;time=2011-10-19T16:40:51.620Z;msgid=old :anna!u@h PRIVMSG fred :replayed line\r\n"
            + ":znc.in BATCH -hx\r\n"
            + "@time=2011-10-19T16:40:51.620Z :anna!u@h PRIVMSG fred :live line\r\n");
        waitForBody(list, "live line");

        var replayRow = rowForBody(list.model, "replayed line");
        var liveRow = rowForBody(list.model, "live line");
        compare(liveRow, replayRow + 1);
        var replay = renderedMessageRow(list, replayRow);
        var live = renderedMessageRow(list, liveRow);
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
        waitForRowCount(list, start + count + 1);
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

    function test_copyTranscriptSelection() {
        openSeededAppWindow();
        var composer = item("messageComposer");
        mouseClick(composer);
        verify(composer.activeFocus);

        var body = visibleListChild("messageList", "messageBody");
        body.select(0, Math.min(4, body.length));
        verify(body.selectedText.length > 0);
        var expected = body.selectedText;
        compare(appWindow.transcriptSelection, body);
        verify(composer.activeFocus);
        verify(composer.selectedText.length === 0);

        clickCopy();
        verify(clipboardMatches(expected));
        compare(body.selectedText, expected);
        verify(composer.activeFocus);
        verify(!containsMirc(readClipboard()));
    }

    function test_copyRichTranscriptSelectionIsPlain() {
        openSeededAppWindow();
        var composer = item("messageComposer");
        mouseClick(composer);
        verify(composer.activeFocus);

        var list = item("messageList");
        var previousCount = list.model.rowCount();
        var boldBody = "hello \x02world\x02";
        injectOmarchyChat("anna", "#omarchy", boldBody);
        waitForRowCount(list, previousCount + 1);
        list.positionViewAtIndex(previousCount, ListView.Contain);
        waitForRendering(appWindow.contentItem);

        var body = findChild(list.itemAtIndex(previousCount), "messageBody");
        verify(body !== null && body.visible, "Could not find bold messageBody");
        compare(body.textFormat, TextEdit.RichText);
        body.selectAll();
        compare(body.selectedText, "hello world");
        verify(composer.activeFocus);

        clickCopy();
        verify(clipboardMatches("hello world"));
        var copied = readClipboard();
        verify(!containsMirc(copied));
        verify(copied.indexOf("<") < 0);
    }

    function test_composerSelectionWinsOverTranscript() {
        openSeededAppWindow();
        var composer = item("messageComposer");
        mouseClick(composer);
        typeText("keep this draft");
        var body = visibleListChild("messageList", "messageBody");
        body.selectAll();
        var transcript = body.selectedText;
        verify(transcript.length > 0);
        composer.selectAll();
        compare(composer.selectedText, "keep this draft");

        clickCopy();
        verify(clipboardMatches("keep this draft"));
        compare(body.selectedText, transcript);
    }

    function test_copyComposerWhenTranscriptHasNoSelection() {
        openSeededAppWindow();
        var composer = item("messageComposer");
        mouseClick(composer);
        typeText("only the draft");
        composer.selectAll();
        compare(appWindow.transcriptSelection, null);

        clickCopy();
        verify(clipboardMatches("only the draft"));
    }

    function test_secondTranscriptSelectionReplacesTheFirst() {
        openSeededAppWindow();
        var composer = item("messageComposer");
        mouseClick(composer);
        verify(composer.activeFocus);

        var list = item("messageList");
        var firstIndex = -1;
        var secondIndex = -1;
        var index = 0;
        for (; index < list.count; ++index) {
            var row = list.itemAtIndex(index);
            if (!row)
                continue;
            var child = findChild(row, "messageBody");
            if (!child || !child.visible)
                continue;
            if (firstIndex < 0)
                firstIndex = index;
            else {
                secondIndex = index;
                break;
            }
        }
        verify(firstIndex >= 0 && secondIndex >= 0,
               "two visible message bodies are required");
        var first = findChild(list.itemAtIndex(firstIndex), "messageBody");
        var second = findChild(list.itemAtIndex(secondIndex), "messageBody");
        first.selectAll();
        var firstText = first.selectedText;
        second.selectAll();
        compare(first.selectedText, "");
        compare(appWindow.transcriptSelection, second);
        verify(second.selectedText.length > 0);
        verify(firstText.length > 0);
        verify(composer.activeFocus);

        clickCopy();
        verify(clipboardMatches(second.selectedText));
    }

    function test_switchingConversationClearsTranscriptSelection() {
        openSeededAppWindow();
        writeClipboard("sentinel");
        var body = visibleListChild("messageList", "messageBody");
        body.selectAll();
        verify(appWindow.transcriptSelection !== null);

        mouseClick(namedItem(liveConversation("anna")));
        tryCompare(appWindow, "currentConversation", "anna");
        compare(appWindow.transcriptSelection, null);

        clickCopy();
        verify(clipboardMatches("sentinel"));
    }

    function test_copyConsoleSelection() {
        openSeededAppWindow();
        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);
        var composer = item("messageComposer");
        verify(composer.activeFocus);

        var body = visibleListChild("consoleList", "consoleText");
        body.selectAll();
        var expected = body.selectedText;
        verify(expected.length > 0);
        compare(appWindow.transcriptSelection, body);

        clickCopy();
        verify(clipboardMatches(expected));
        verify(!containsMirc(readClipboard()));
        verify(composer.activeFocus);
    }

    function test_copyWhoisSelection() {
        openSeededAppWindow();
        var list = item("messageList");
        var composer = item("messageComposer");
        mouseClick(composer);
        typeText("/whois lena");
        if (item("slashCompleteList").visible)
            keyClick(Qt.Key_Escape);
        keyClick(Qt.Key_Return);
        var label = seed.lastOmarchyRequestLabel();
        verify(label.length > 0, "WHOIS should carry a labeled-response label");
        seed.injectOmarchy("@label=" + label + " :server 319 fred lena :#omarchy\r\n");
        tryVerify(function() {
            var last = list.model.rowCount() - 1;
            return last >= 0 && field(list.model, last, "kind") === "whois";
        });
        var whoisAt = list.model.rowCount() - 1;
        list.positionViewAtIndex(whoisAt, ListView.Contain);
        waitForRendering(appWindow.contentItem);
        var whoisText = findChild(list.itemAtIndex(whoisAt), "messageWhois");
        verify(whoisText !== null && whoisText.visible, "Could not find messageWhois");
        whoisText.selectAll();
        var expected = whoisText.selectedText;
        verify(expected.length > 0);
        compare(appWindow.transcriptSelection, whoisText);

        clickCopy();
        verify(clipboardMatches(expected));
        verify(composer.activeFocus);
    }

    function test_copyTranscriptSelectionWhileMembersFocused() {
        openSeededAppWindow();
        var body = visibleListChild("messageList", "messageBody");
        body.selectAll();
        var expected = body.selectedText;
        verify(expected.length > 0);

        var members = item("membersList");
        keyClick(Qt.Key_P, Qt.ControlModifier | Qt.ShiftModifier);
        tryCompare(members, "activeFocus", true);
        verify(!item("messageComposer").activeFocus);

        keyClick(Qt.Key_C, Qt.ControlModifier);
        verify(clipboardMatches(expected));
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

    function test_emphasizedIrcText() {
        var bold = "hello \x02world\x02";
        compare(appWindow.plainIrcText(bold), "hello world");
        verify(appWindow.emphasizedIrcText(bold).indexOf("<b>world</b>") >= 0);

        verify(appWindow.emphasizedIrcText("\x1ditalic\x1d").indexOf("<i>italic</i>") >= 0);
        verify(appWindow.emphasizedIrcText("\x1funder\x1f").indexOf("<u>under</u>") >= 0);

        var nested = appWindow.emphasizedIrcText("\x02bold\x1ditalic\x1d\x02");
        verify(nested.indexOf("<b>") >= 0);
        verify(nested.indexOf("<i>") >= 0);
        verify(nested.indexOf("</b></i>") < 0);
        verify(!/<b>[^<]*<i>[\s\S]*<\/b>\s*<\/i>/.test(nested));

        var colorOnly = appWindow.emphasizedIrcText("\x0304red\x03");
        compare(appWindow.plainIrcText("\x0304red\x03"), "red");
        verify(colorOnly.indexOf("red") >= 0);
        verify(colorOnly.indexOf("<b") < 0);
        verify(colorOnly.indexOf("<i") < 0);
        verify(colorOnly.indexOf("<u") < 0);

        var literal = appWindow.emphasizedIrcText("<b>not html</b>");
        verify(literal.indexOf("&lt;b&gt;") >= 0);
        verify(literal.indexOf("&lt;/b&gt;") >= 0);
        verify(literal.indexOf("<b>") < 0);
        verify(literal.indexOf("</b>") < 0);

        var reverse = appWindow.emphasizedIrcText("\x02a\x16b\x02");
        verify(reverse.indexOf("<b>ab</b>") >= 0);
        verify(reverse.indexOf("\x16") < 0);
        verify(reverse.indexOf("</b><b>") < 0);

        compare(appWindow.plainIrcText("\x04FF0000red"), "red");
        var hexColor = appWindow.emphasizedIrcText("\x04FF0000red");
        verify(hexColor.indexOf("red") >= 0);
        verify(hexColor.indexOf("FF0000") < 0);

        var extraCodes = appWindow.emphasizedIrcText("\x02a\x11b\x1ec\x02");
        verify(extraCodes.indexOf("<b>abc</b>") >= 0);

        var spaced = appWindow.emphasizedIrcText("a  \x02b\x02");
        verify(spaced.indexOf("a  <b>b</b>") >= 0);
    }

    function test_emphasizedIrcTextFailClosed() {
        var samples = [
            "hello \x02world\x02",
            "\x1ditalic\x1d",
            "\x1funder\x1f",
            "\x02bold\x1ditalic\x1d\x02",
            "\x0304red\x03",
            "<b>not html</b>",
            "<a href=\"https://evil.example\">x</a>\x02y\x02",
            "\x16flip\x16",
            "\x02\x1d\x1f\x0f",
            "\x02a\x16b\x02",
            "\x04FF0000red",
            "\x02a\x11b\x1ec\x02",
            "a  \x02b\x02",
            formattedIrcBody()
        ];
        var wrapper = "<span style=\"white-space: pre-wrap;\">";
        var index = 0;
        for (; index < samples.length; ++index) {
            var html = appWindow.emphasizedIrcText(samples[index]);
            verify(html.indexOf("<a") < 0);
            verify(html.indexOf(wrapper) === 0);
            verify(html.lastIndexOf("</span>") === html.length - 7);
            verify(html.indexOf("<span") === 0);
            verify(html.indexOf("<span", 1) < 0);
            var re = /<\/?([A-Za-z][A-Za-z0-9]*)\b([^>]*)>/g;
            var match;
            while ((match = re.exec(html)) !== null) {
                var tag = match[1].toLowerCase();
                verify(tag === "b" || tag === "i" || tag === "u" || tag === "span",
                       "unexpected tag <" + tag + "> in " + html);
                if (tag === "span" && match[0].charAt(1) !== "/")
                    compare(match[0], wrapper);
                else
                    compare(match[2], "");
            }
        }
    }

    function test_messageBodyRendersIrcEmphasis() {
        openSeededAppWindow();
        var list = item("messageList");
        var previousCount = list.model.rowCount();

        var boldBody = "hello \x02world\x02";
        injectOmarchyChat("anna", "#omarchy", boldBody);
        waitForRowCount(list, previousCount + 1);
        compare(field(list.model, previousCount, "body"), boldBody);

        list.positionViewAtIndex(previousCount, ListView.Contain);
        waitForRendering(appWindow.contentItem);
        var row = list.itemAtIndex(previousCount);
        verify(row !== null, "The bold seeded message should be rendered");
        var body = findChild(row, "messageBody");
        verify(body !== null && body.visible, "Could not find bold messageBody");
        compare(body.textFormat, TextEdit.RichText);
        compare(body.getText(0, body.length), "hello world");
        body.selectAll();
        compare(body.selectedText, "hello world");
        verify(!containsMirc(body.selectedText));
        verify(appWindow.emphasizedIrcText(boldBody).indexOf("<b>world</b>") >= 0);
        verify(body.text.indexOf("<b>world</b>") >= 0
               || /font-weight\s*:\s*(bold|[6-9]00)/.test(body.text));

        injectOmarchyChat("dax", "#omarchy", formattedIrcBody());
        waitForRowCount(list, previousCount + 2);
        compare(field(list.model, previousCount + 1, "body"), formattedIrcBody());
        list.positionViewAtIndex(previousCount + 1, ListView.Contain);
        waitForRendering(appWindow.contentItem);
        row = list.itemAtIndex(previousCount + 1);
        verify(row !== null, "The formatted seeded message should be rendered");
        body = findChild(row, "messageBody");
        verify(body !== null && body.visible, "Could not find formatted messageBody");
        compare(body.textFormat, TextEdit.RichText);
        compare(body.getText(0, body.length), "bold / red");
        body.selectAll();
        compare(body.selectedText, "bold / red");
        verify(!containsMirc(body.selectedText));
        verify(appWindow.emphasizedIrcText(formattedIrcBody()).indexOf("<b>bold</b>") >= 0);
        verify(body.text.indexOf("<b>bold</b>") >= 0
               || /font-weight\s*:\s*(bold|[6-9]00)/.test(body.text));

        var colorBody = "\x0304red\x03";
        injectOmarchyChat("mira", "#omarchy", colorBody);
        waitForRowCount(list, previousCount + 3);
        compare(field(list.model, previousCount + 2, "body"), colorBody);
        list.positionViewAtIndex(previousCount + 2, ListView.Contain);
        waitForRendering(appWindow.contentItem);
        row = list.itemAtIndex(previousCount + 2);
        verify(row !== null, "The color-only seeded message should be rendered");
        body = findChild(row, "messageBody");
        verify(body !== null && body.visible, "Could not find color-only messageBody");
        compare(body.textFormat, TextEdit.PlainText);
        compare(body.text, "red");
        verify(body.text.indexOf("span") < 0);
        verify(body.text.indexOf("color") < 0);

        var literal = "<b>not html</b>";
        injectOmarchyChat("kai", "#omarchy", literal);
        waitForRowCount(list, previousCount + 4);
        compare(field(list.model, previousCount + 3, "body"), literal);
        list.positionViewAtIndex(previousCount + 3, ListView.Contain);
        waitForRendering(appWindow.contentItem);
        row = list.itemAtIndex(previousCount + 3);
        verify(row !== null, "The literal HTML seeded message should be rendered");
        body = findChild(row, "messageBody");
        verify(body !== null && body.visible, "Could not find literal HTML messageBody");
        compare(body.textFormat, TextEdit.PlainText);
        compare(body.text, "<b>not html</b>");

        var spacedBody = "a  \x02b\x02";
        injectOmarchyChat("rio", "#omarchy", spacedBody);
        waitForRowCount(list, previousCount + 5);
        compare(field(list.model, previousCount + 4, "body"), spacedBody);
        list.positionViewAtIndex(previousCount + 4, ListView.Contain);
        waitForRendering(appWindow.contentItem);
        row = list.itemAtIndex(previousCount + 4);
        verify(row !== null, "The spaced emphasized message should be rendered");
        body = findChild(row, "messageBody");
        verify(body !== null && body.visible, "Could not find spaced messageBody");
        compare(body.textFormat, TextEdit.RichText);
        compare(body.getText(0, body.length), "a  b");
    }

    function test_liveMessageBodyRendersIrcEmphasis() {
        liveMessages.clear();
        liveMessages.append({
            author: "anna",
            time: "10:00",
            body: "hello \x02world\x02",
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
        verify(row !== null, "The bold live message should be rendered");
        body = findChild(row, "messageBody");
        verify(body !== null && body.visible, "Could not find live messageBody");
        compare(body.textFormat, TextEdit.RichText);
        compare(body.getText(0, body.length), "hello world");
        body.selectAll();
        compare(body.selectedText, "hello world");
        verify(!containsMirc(body.selectedText));
        verify(window.emphasizedIrcText("hello \x02world\x02").indexOf("<b>world</b>") >= 0);
        verify(body.text.indexOf("<b>world</b>") >= 0
               || /font-weight\s*:\s*(bold|[6-9]00)/.test(body.text));

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

    function test_notificationActivateOpensLiveChannelMention() {
        openSeededAppWindow();
        appWindow.selectConversation("#ricing", seed.omarchyNetworkId);
        tryCompare(appWindow, "currentConversation", "#ricing");

        appWindow.lastNotification = null;
        appWindow.notifyMentionIfUnfocused(true, "anna", "fred: ping",
                                           seed.omarchyNetworkId, "#omarchy",
                                           "mention-1");
        compare(appWindow.lastNotification, null);

        seed.injectOmarchy("@msgid=mention-1 :anna!u@h PRIVMSG #omarchy :fred: ping\r\n");
        appWindow.notifyMentionIfUnfocused(false, "anna", "fred: ping",
                                           seed.omarchyNetworkId, "#omarchy",
                                           "mention-1");
        compare(appWindow.lastNotification.author, "anna");
        compare(appWindow.lastNotification.body, "fred: ping");
        compare(appWindow.lastNotification.networkId, seed.omarchyNetworkId);
        compare(appWindow.lastNotification.target, "#omarchy");
        compare(appWindow.lastNotification.msgid, "mention-1");

        appWindow.activateNotifiedConversation(seed.omarchyNetworkId, "#omarchy",
                                               "mention-1");
        tryCompare(appWindow, "currentConversation", "#omarchy");
        compare(appWindow.currentNetworkId, seed.omarchyNetworkId);
        waitForRendering(appWindow.contentItem);
        var mentionRow = appWindow.msgidRow("mention-1");
        verify(mentionRow >= 0);
        compare(field(item("messageList").model, mentionRow, "msgid"), "mention-1");
        compare(field(item("messageList").model, mentionRow, "body"), "fred: ping");
    }

    function test_notificationActivateOpensDirectMessage() {
        openSeededAppWindow();
        appWindow.selectConversation("#ricing", seed.omarchyNetworkId);
        tryCompare(appWindow, "currentConversation", "#ricing");

        seed.injectOmarchy("@msgid=dm-7 :anna!u@h PRIVMSG fred :secret\r\n");
        appWindow.notifyMentionIfUnfocused(false, "anna", "secret",
                                           seed.omarchyNetworkId, "anna", "dm-7");
        compare(appWindow.lastNotification.author, "anna");
        compare(appWindow.lastNotification.body, "secret");
        compare(appWindow.lastNotification.networkId, seed.omarchyNetworkId);
        compare(appWindow.lastNotification.target, "anna");
        compare(appWindow.lastNotification.msgid, "dm-7");

        appWindow.activateNotifiedConversation(seed.omarchyNetworkId, "anna", "dm-7");
        tryCompare(appWindow, "currentConversation", "anna");
        compare(appWindow.currentNetworkId, seed.omarchyNetworkId);
        waitForRendering(appWindow.contentItem);
        var dmRow = appWindow.msgidRow("dm-7");
        verify(dmRow >= 0);
        compare(field(item("messageList").model, dmRow, "body"), "secret");
    }

    function test_openAtUnreadNotificationActivateLandsOnMark() {
        openSeededAppWindow();
        var list = item("messageList");
        fillTranscriptUntilScrollable(list);

        mouseClick(namedItem(liveConversation("#desktop")));
        tryCompare(appWindow, "currentConversation", "#desktop");
        injectUnreadWhileAway("#omarchy", "open-at-unread-notify-first-zx9",
                              "open-at-unread-notify-second-zx9", 24);
        seed.injectOmarchy(
            "@msgid=mention-notify-unread-zx9 :anna!u@h PRIVMSG #omarchy :fred: ping notify-unread-zx9\r\n");
        seed.irc.openConversationsAtUnread = true;

        appWindow.activateNotifiedConversation(seed.omarchyNetworkId, "#omarchy",
                                               "mention-notify-unread-zx9");
        tryCompare(appWindow, "currentConversation", "#omarchy");
        compare(appWindow.currentNetworkId, seed.omarchyNetworkId);
        waitForBody(list, "open-at-unread-notify-first-zx9");
        waitForBody(list, "open-at-unread-notify-second-zx9");
        waitForBody(list, "fred: ping notify-unread-zx9");

        var first = rowForBody(list.model, "open-at-unread-notify-first-zx9");
        verify(first > 0, "The first unseen line should not lead the buffer");
        var markRow = list.model.unreadMarkRow();
        compare(markRow, first - 1);
        compare(field(list.model, markRow, "kind"), "unread");
        var msgidRow = appWindow.msgidRow("mention-notify-unread-zx9");
        verify(msgidRow > markRow, "The notified mention should sit below the unread mark");
        waitForOpenAtUnreadViewport(list, markRow);
        compare(firstVisibleIndex(list), markRow);
        verify(firstVisibleIndex(list) !== msgidRow);
    }

    function test_openAtUnreadNotificationActivateLeavesAlreadyViewingViewport() {
        openSeededAppWindow();
        seed.irc.openConversationsAtUnread = true;
        var list = item("messageList");
        fillTranscriptUntilScrollable(list);
        seed.injectOmarchy(
            "@msgid=mention-notify-same-zx9 :anna!u@h PRIVMSG #omarchy :fred: ping notify-same-zx9\r\n");
        waitForBody(list, "fred: ping notify-same-zx9");
        waitForRendering(appWindow.contentItem);
        list.pinToEnd();
        waitForRendering(appWindow.contentItem);
        wait(0);

        keyClick(Qt.Key_PageUp);
        waitForRendering(appWindow.contentItem);
        tryCompare(list, "stick", list.stickDetached);
        verify(!transcriptPinned(list));
        var frozenY = list.contentY;
        var frozenStick = list.stick;

        appWindow.activateNotifiedConversation(seed.omarchyNetworkId, "#omarchy",
                                               "mention-notify-same-zx9");
        tryCompare(appWindow, "currentConversation", "#omarchy");
        waitForRendering(appWindow.contentItem);
        wait(0);
        waitForRendering(appWindow.contentItem);

        fuzzyCompare(list.contentY, frozenY, 2);
        compare(list.stick, frozenStick);
        verify(!transcriptPinned(list));
        verify(item("messageComposer").activeFocus);
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

    function test_whoisRowClickOpensHttpsUrl() {
        openSeededAppWindow();
        seed.injectOmarchy(
            ":server 761 fred fred avatar * :https://example.com/a.png\r\n");
        var list = item("messageList");
        verify(appWindow.irc.sendMessage("/avatar"));
        waitForBody(list, "Standing avatar: https://example.com/a.png");
        var whoisAt = rowForBody(list.model, "Standing avatar: https://example.com/a.png");
        list.positionViewAtIndex(whoisAt, ListView.Contain);
        waitForRendering(appWindow.contentItem);
        var row = list.itemAtIndex(whoisAt);
        verify(row !== null, "The avatar inspect row should be rendered");
        var body = findChild(row, "messageWhois");
        verify(body !== null && body.visible, "Could not find messageWhois");
        compare(body.textFormat, TextEdit.PlainText);
        compare(body.text, "Standing avatar: https://example.com/a.png");

        appWindow.lastOpenedUrl = "";
        var start = body.text.indexOf("https://example.com/a.png");
        var rect = body.positionToRectangle(start + 4);
        var hit = findChild(body, "urlHit");
        verify(hit !== null, "Could not find whois urlHit");
        mouseClick(hit, rect.x + Math.max(1, rect.width / 2), rect.y + rect.height / 2);
        compare(appWindow.lastOpenedUrl, "https://example.com/a.png");
    }

    function test_ctrlFFindsEmphasizedVisibleText() {
        openSeededAppWindow();
        var list = item("messageList");
        var body = "find-emph \x02world\x02";
        injectOmarchyChat("anna", "#omarchy", body);
        waitForBody(list, body);

        keyClick(Qt.Key_F, Qt.ControlModifier);
        tryCompare(appWindow, "findActive", true);
        typeText("find-emph");
        tryVerify(function() {
            var index = appWindow.findIndex;
            return index >= 0 && field(list.model, index, "body") === body;
        });
        verify(containsMirc(field(list.model, appWindow.findIndex, "body")));
        compare(appWindow.plainIrcText(field(list.model, appWindow.findIndex, "body")),
                "find-emph world");
    }

    function test_actionRowRendersItalicAndEmphasis() {
        openSeededAppWindow();
        var list = item("messageList");
        seed.injectOmarchy(":anna!u@h PRIVMSG #omarchy :\x01ACTION waves\x01\r\n");
        waitForBody(list, "waves");
        var actionAt = rowForBody(list.model, "waves");
        compare(field(list.model, actionAt, "kind"), "action");

        list.positionViewAtIndex(actionAt, ListView.Contain);
        waitForRendering(appWindow.contentItem);
        var row = list.itemAtIndex(actionAt);
        verify(row !== null, "The action row should be rendered");
        var body = findChild(row, "messageBody");
        verify(body !== null && body.visible, "Could not find action messageBody");
        compare(body.font.italic, true);
        compare(body.textFormat, TextEdit.PlainText);
        compare(body.text, "waves");

        var boldAction = "waves \x02hello\x02";
        seed.injectOmarchy(":dax!u@h PRIVMSG #omarchy :\x01ACTION waves \x02hello\x02\x01\r\n");
        waitForBody(list, boldAction);
        var boldAt = rowForBody(list.model, boldAction);
        compare(field(list.model, boldAt, "kind"), "action");

        list.positionViewAtIndex(boldAt, ListView.Contain);
        waitForRendering(appWindow.contentItem);
        row = list.itemAtIndex(boldAt);
        verify(row !== null, "The bold action row should be rendered");
        body = findChild(row, "messageBody");
        verify(body !== null && body.visible, "Could not find bold action messageBody");
        compare(body.font.italic, true);
        compare(body.textFormat, TextEdit.RichText);
        compare(body.getText(0, body.length), "waves hello");
        verify(appWindow.emphasizedIrcText(boldAction).indexOf("<b>hello</b>") >= 0);
        verify(body.text.indexOf("<b>") >= 0
               || /font-weight\s*:\s*(bold|[6-9]00)/.test(body.text));
    }

    function test_messageBodyClickOpensHttpsUrlInsideEmphasis() {
        openSeededAppWindow();
        var list = item("messageList");
        var previousCount = list.model.rowCount();
        injectOmarchyChat("anna", "#omarchy", "see \x02https://example.com\x02");
        waitForRowCount(list, previousCount + 1);
        list.positionViewAtIndex(previousCount, ListView.Contain);
        waitForRendering(appWindow.contentItem);

        var row = list.itemAtIndex(previousCount);
        verify(row !== null, "The emphasized URL message should be rendered");
        var body = findChild(row, "messageBody");
        verify(body !== null && body.visible, "Could not find emphasized URL messageBody");
        compare(body.textFormat, TextEdit.RichText);
        compare(body.getText(0, body.length), "see https://example.com");
        compare(appWindow.editVisibleText(body), "see https://example.com");
        body.selectAll();
        compare(body.selectedText, "see https://example.com");
        verify(!containsMirc(body.selectedText));
        body.deselect();

        appWindow.lastOpenedUrl = "";
        var start = appWindow.editVisibleText(body).indexOf("https://example.com");
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

    function test_consoleInviteChannelClickJoins() {
        openSeededAppWindow();
        var list = item("consoleList");
        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);

        var previousCount = list.model.rowCount();
        seed.injectOmarchy(":alice!u@h INVITE fred :#invited\r\n");
        waitForRowCount(list, previousCount + 1);
        list.positionViewAtIndex(previousCount, ListView.Contain);
        waitForRendering(appWindow.contentItem);

        var row = list.itemAtIndex(previousCount);
        verify(row !== null, "The invite console line should be rendered");
        var body = findChild(row, "consoleText");
        verify(body !== null, "Could not find invite consoleText");
        compare(body.textFormat, TextEdit.PlainText);
        compare(field(list.model, previousCount, "label"), "INVITE");
        compare(body.text, "alice invited you to #invited");
        body.selectAll();
        compare(body.selectedText, "alice invited you to #invited");
        body.deselect();

        var framesBefore = seed.omarchyFrameCount();
        var nickRect = body.positionToRectangle(1);
        var nickHit = findChild(body, "urlHit");
        verify(nickHit !== null, "Could not find console urlHit");
        mouseClick(nickHit, nickRect.x + Math.max(1, nickRect.width / 2),
                   nickRect.y + nickRect.height / 2);
        verify(!seed.omarchyWroteFrom(framesBefore, "JOIN #invited"));
        compare(appWindow.currentConversation, "#omarchy");
        tryCompare(appWindow, "consoleVisible", true);

        var start = body.text.indexOf("#invited");
        var rect = body.positionToRectangle(start + 1);
        mouseClick(nickHit, rect.x + Math.max(1, rect.width / 2),
                   rect.y + rect.height / 2);
        verify(seed.omarchyWroteFrom(framesBefore, "JOIN #invited"));
        seed.injectOmarchy(":fred!u@h JOIN :#invited\r\n");
        tryCompare(appWindow, "currentConversation", "#invited");
        tryCompare(appWindow, "consoleVisible", false);
    }

    function test_inboxSheetTogglesWithShortcut() {
        openSeededAppWindow();
        var sheet = item("inboxSheet");
        var composer = item("messageComposer");
        verify(!sheet.opened);

        keyClick(Qt.Key_A, Qt.ControlModifier | Qt.ShiftModifier);
        tryCompare(sheet, "opened", true);

        keyClick(Qt.Key_A, Qt.ControlModifier | Qt.ShiftModifier);
        tryCompare(sheet, "opened", false);
        tryCompare(composer, "activeFocus", true);

        openInboxSheet();
        keyClick(Qt.Key_Escape);
        tryCompare(sheet, "opened", false);
        tryCompare(composer, "activeFocus", true);

        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        tryCompare(appWindow, "connectionOverlayVisible", true);
        keyClick(Qt.Key_A, Qt.ControlModifier | Qt.ShiftModifier);
        compare(sheet.opened, false);
        compare(appWindow.connectionOverlayVisible, true);
    }

    function test_inboxMarkShowsCountAndOpensSheet() {
        openSeededAppWindow();
        var inboxBaseline = seed.irc.inboxCount;
        compare(inboxBadgeText(), String(inboxBaseline));

        appWindow.selectConversation("#ricing", seed.omarchyNetworkId);
        tryCompare(appWindow, "currentConversation", "#ricing");
        var inboxBeforeInject = seed.irc.inboxCount;
        seed.injectOmarchy("@msgid=inbox-mark-1 :anna!u@h PRIVMSG #omarchy :fred: inbox ping\r\n");
        tryVerify(function() { return seed.irc.inboxCount === inboxBeforeInject + 1; });
        compare(inboxBadgeText(), String(inboxBeforeInject + 1));

        mouseClick(namedItem("inboxMark"));
        tryCompare(item("inboxSheet"), "opened", true);
        var list = item("inboxList");
        compare(list.count, inboxBeforeInject + 1);
        compare(field(seed.irc.inbox, 0, "kind"), "mention");
        compare(field(seed.irc.inbox, 0, "target"), "#omarchy");
    }

    function test_inboxMentionEnterJumpsToMsgid() {
        openSeededAppWindow();
        appWindow.selectConversation("#ricing", seed.omarchyNetworkId);
        tryCompare(appWindow, "currentConversation", "#ricing");
        var inboxBeforeInject = seed.irc.inboxCount;

        seed.injectOmarchy("@msgid=inbox-mention-1 :anna!u@h PRIVMSG #omarchy :fred: inbox scroll\r\n");
        tryVerify(function() { return seed.irc.inboxCount === inboxBeforeInject + 1; });

        openInboxSheet();
        keyClick(Qt.Key_Return);
        tryCompare(appWindow, "currentConversation", "#omarchy");
        compare(appWindow.currentNetworkId, seed.omarchyNetworkId);
        waitForRendering(appWindow.contentItem);
        var mentionRow = appWindow.msgidRow("inbox-mention-1");
        verify(mentionRow >= 0);
        compare(field(item("messageList").model, mentionRow, "msgid"), "inbox-mention-1");
        compare(field(item("messageList").model, mentionRow, "body"), "fred: inbox scroll");
        tryCompare(item("messageComposer"), "activeFocus", true);
    }

    function test_inboxInviteEnterJoinsChannel() {
        openSeededAppWindow();
        var inboxBaseline = seed.irc.inboxCount;
        var framesBefore = seed.omarchyFrameCount();
        seed.injectOmarchy(":alice!u@h INVITE fred :#invited\r\n");
        tryVerify(function() { return seed.irc.inboxCount === inboxBaseline + 1; });
        compare(field(seed.irc.inbox, 0, "kind"), "invite");
        compare(field(seed.irc.inbox, 0, "target"), "#invited");

        openInboxSheet();
        keyClick(Qt.Key_Return);
        verify(seed.omarchyWroteFrom(framesBefore, "JOIN #invited"));
        seed.injectOmarchy(":fred!u@h JOIN :#invited\r\n");
        tryCompare(appWindow, "currentConversation", "#invited");
        compare(seed.irc.inboxCount, inboxBaseline);
    }

    function test_inboxSelectingConversationClearsMatchingRows() {
        openSeededAppWindow();
        appWindow.selectConversation("#ricing", seed.omarchyNetworkId);
        tryCompare(appWindow, "currentConversation", "#ricing");
        var inboxBaseline = seed.irc.inboxCount;
        seed.injectOmarchy("@msgid=inbox-clear-1 :anna!u@h PRIVMSG #omarchy :fred: waiting\r\n");
        tryVerify(function() { return seed.irc.inboxCount === inboxBaseline + 1; });
        compare(inboxBadgeText(), String(inboxBaseline + 1));

        mouseClick(namedItem(liveConversation("#omarchy")));
        tryCompare(appWindow, "currentConversation", "#omarchy");
        compare(seed.irc.inboxCount, inboxBaseline);
        compare(inboxBadgeText(), String(inboxBaseline));
        compare(item("inboxSheet").opened, false);
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
        waitForBody(list, "rio left");
        var eventAt = rowForBody(list.model, "rio left");
        verify(eventAt >= previousCount);
        compare(field(list.model, eventAt, "kind"), "event");
        list.positionViewAtIndex(eventAt, ListView.Contain);
        waitForRendering(appWindow.contentItem);

        var row = list.itemAtIndex(eventAt);
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
        var label = seed.lastOmarchyRequestLabel();
        verify(label.length > 0, "WHOIS should carry a labeled-response label");
        seed.injectOmarchy("@label=" + label + " :server 319 fred lena :" + channels + "\r\n");
        tryVerify(function() {
            var last = list.model.rowCount() - 1;
            return last >= previousCount
                && field(list.model, last, "kind") === "whois";
        });
        var whoisAt = list.model.rowCount() - 1;
        compare(field(list.model, whoisAt, "kind"), "whois");
        var body = field(list.model, whoisAt, "body");
        verify(body.indexOf("lena is on") === 0);
        list.positionViewAtIndex(whoisAt, ListView.Contain);
        waitForRendering(appWindow.contentItem);

        var row = list.itemAtIndex(whoisAt);
        verify(row !== null, "The long whois row should be rendered");
        var whoisText = findChild(row, "messageWhois");
        verify(whoisText !== null && whoisText.visible, "Could not find messageWhois");
        compare(whoisText.wrapMode, TextEdit.Wrap);
        compare(whoisText.textFormat, TextEdit.PlainText);
        compare(whoisText.selectionColor, appWindow.selectionColor);
        compare(whoisText.selectedTextColor, "#ffffff");
        compare(whoisText.text, body);
        tryVerify(function () { return whoisText.lineCount > 1; },
                  1000, "The WHOIS row should wrap");
        tryVerify(function () {
            return row.height > appWindow.scaledSize(22);
        }, 1000, "A wrapped whois row should grow past the single-line height");
        compare(row.height, whoisText.implicitHeight + appWindow.scaledSize(8));
        compare(findChild(row, "messageEvent").visible, false);
        compare(findChild(row, "messageAvatar").visible, false);
        compare(findChild(row, "messageHeader").visible, false);
        compare(findChild(row, "messageBody").visible, false);
        saveScreenshot("wrapped-whois-row");
    }

    function test_ctcpReplyCopiesIntoAskingTranscript() {
        destroyAppWindowAndSeed();
        seed = createTemporaryObject(seedComponent, testCase);
        verify(seed !== null, "SeededIrcFixture should construct");
        verify(seed.openWithAutoEcho(), seed.lastError);
        verify(seed.connection, "seeded window needs a real IrcConnection");
        appWindow = createTemporaryObject(seededWindowComponent, testCase, {
            backend: seed.backend,
            irc: seed.irc,
            slashCommands: seed.slash,
            connection: seed.connection,
            avatarStore: appAvatarStore
        });
        verify(appWindow !== null, "The seeded Omairc window should load");
        tryCompare(appWindow, "visible", true);
        waitForRendering(appWindow.contentItem);
        tryVerify(function() {
            return appWindow.currentConversation === "#omarchy"
                && !appWindow.consoleVisible
                && !appWindow.connectionOverlayVisible;
        });

        var list = item("messageList");
        var previousCount = list.model.rowCount();
        var composer = item("messageComposer");
        mouseClick(composer);
        typeText("/version anna");
        if (item("slashCompleteList").visible)
            keyClick(Qt.Key_Escape);
        keyClick(Qt.Key_Return);
        tryVerify(function() {
            var last = list.model.rowCount() - 1;
            return last >= previousCount
                && field(list.model, last, "kind") === "whois"
                && field(list.model, last, "body").indexOf(
                    "VERSION reply from anna: https://omairc.app") === 0;
        });
        var replyAt = list.model.rowCount() - 1;
        list.positionViewAtIndex(replyAt, ListView.Contain);
        waitForRendering(appWindow.contentItem);
        var row = list.itemAtIndex(replyAt);
        verify(row !== null, "The CTCP reply row should be rendered");
        var whoisText = findChild(row, "messageWhois");
        verify(whoisText !== null && whoisText.visible, "Could not find messageWhois");
        compare(whoisText.textFormat, Text.PlainText);
        compare(whoisText.text, field(list.model, replyAt, "body"));
        compare(findChild(row, "messageEvent").visible, false);
        compare(findChild(row, "messageAvatar").visible, false);
        compare(findChild(row, "messageHeader").visible, false);
        compare(findChild(row, "messageBody").visible, false);
        saveScreenshot("ctcp-version-transcript");
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

    function test_toggleServerListWithShortcut() {
        openSeededAppWindow();
        var sidebar = item("serverList");
        var expandedWidth = sidebar.width;
        verify(expandedWidth > 0, "The server list should occupy width while shown");

        keyClick(Qt.Key_S, Qt.ControlModifier | Qt.ShiftModifier);

        tryCompare(sidebar, "width", 0);
        saveScreenshot("server-list-collapsed");

        keyClick(Qt.Key_S, Qt.ControlModifier | Qt.ShiftModifier);

        tryCompare(sidebar, "width", expandedWidth);
        saveScreenshot("server-list-restored");
    }

    function test_collapsedServerListKeepsWalking() {
        openSeededAppWindow();
        var sidebar = item("serverList");
        var expandedWidth = sidebar.width;

        keyClick(Qt.Key_S, Qt.ControlModifier | Qt.ShiftModifier);
        tryCompare(sidebar, "width", 0);

        // Collapsing is not hiding: the rail's rows stay live, so walking
        // conversations still reaches them while the column is out of view.
        keyClick(Qt.Key_Down, Qt.AltModifier);
        tryCompare(appWindow, "currentConversation", "#ricing");

        // Network walking highlights a rail row, so it brings the rail back
        // rather than arming a selection nobody can see.
        keyClick(Qt.Key_Right, Qt.AltModifier);
        compare(appWindow.sidebarNetworkFocusId, seed.oftcNetworkId);
        tryCompare(sidebar, "width", expandedWidth);
    }

    function test_collapsingServerListDropsNetworkSelection() {
        openSeededAppWindow();
        var sidebar = item("serverList");

        keyClick(Qt.Key_Right, Qt.AltModifier);
        compare(appWindow.sidebarNetworkFocusId, seed.oftcNetworkId);

        keyClick(Qt.Key_S, Qt.ControlModifier | Qt.ShiftModifier);
        tryCompare(sidebar, "width", 0);
        compare(appWindow.sidebarNetworkFocusId, "",
                "collapsing the server list should drop its keyboard selection");
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
        var top = members.itemAtIndex(0);
        verify(top !== null, "The first member delegate should be rendered");
        compare(top.nick, "fred");
        compare(top.label, "~fred");
        var highlight = findChild(top, "memberHighlight");
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

    function test_memberRowsFollowRankOrder() {
        openSeededAppWindow();
        compare(appWindow.currentConversation, "#omarchy");
        verify(item("membersPanel").visible);

        // The demo network advertises PREFIX=(qaohv)~&@%+, and the seeded
        // #omarchy ranks fred as founder, anna as admin, dax and mira as ops,
        // kai as halfop, and teo as voice. Mira holds two ranks at once, and
        // teo is away, because rank and presence are independent.
        compare(renderedMembers().join(" "),
                "fred=~fred anna=&anna dax=@dax mira=@mira kai=%kai teo=+teo "
                + "ivy=ivy lena=lena max=max nora=nora sam=sam sol=sol");
        compare(appWindow.currentPeopleCount, 12);
        saveScreenshot("member-rank-order");

        // A 353 replaces the member set, so the seeded ranks give way.
        seed.injectOmarchy(
            ":server 353 fred = #omarchy :teo @anna @dax +mira fred kai sol\r\n"
            + ":server 366 fred #omarchy :End of NAMES\r\n");
        tryVerify(function() {
            var rows = renderedMembers();
            return rows.length === 7 && rows[0] === "anna=@anna";
        });
        compare(renderedMembers().join(" "),
                "anna=@anna dax=@dax mira=+mira "
                + "fred=fred kai=kai sol=sol teo=teo");
        compare(appWindow.currentPeopleCount, 7);

        seed.injectOmarchy(":op!u@h MODE #omarchy -o anna\r\n"
                           + ":op!u@h MODE #omarchy +o teo\r\n");
        tryVerify(function() {
            return renderedMembers()[0] === "dax=@dax";
        });
        compare(renderedMembers().join(" "),
                "dax=@dax teo=@teo mira=+mira "
                + "anna=anna fred=fred kai=kai sol=sol");
        saveScreenshot("member-rank-order-demoted");
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

    function memberListVisibleRows(list) {
        var first = list.indexAt(list.width / 2, list.contentY + 1);
        var last = list.indexAt(list.width / 2, list.contentY + Math.max(1, list.height - 1));
        if (first < 0)
            first = 0;
        if (last < 0)
            last = list.count - 1;
        if (last < first)
            last = first;
        return last - first + 1;
    }

    function memberListLastVisible(list) {
        var last = list.indexAt(list.width / 2, list.contentY + Math.max(1, list.height - 1));
        if (last < 0)
            last = list.count - 1;
        return last;
    }

    function memberListPageSize(list, fraction) {
        if (fraction === undefined)
            fraction = 0.8;
        return Math.max(1, Math.round(memberListVisibleRows(list) * fraction));
    }

    function memberListPageDownIndex(list, fraction) {
        return Math.min(list.count - 1, memberListLastVisible(list) + memberListPageSize(list, fraction));
    }

    function focusOverflowingMemberList(memberCount) {
        if (memberCount === undefined)
            memberCount = 30;
        var members = item("membersList");
        var names = [];
        var index = 0;
        for (; index < memberCount; ++index)
            names.push("bulk" + index);
        seed.injectOmarchy(
            ":server 353 fred = #omarchy :" + names.join(" ") + "\r\n"
            + ":server 366 fred #omarchy :End of NAMES\r\n");
        tryVerify(function() { return members.count === memberCount; });
        waitForRendering(appWindow.contentItem);
        verify(members.contentHeight > members.height);
        keyClick(Qt.Key_P, Qt.ControlModifier | Qt.ShiftModifier);
        tryCompare(members, "activeFocus", true);
        return members;
    }

    function fillDirectMessageUntilScrollable(list) {
        var composer = item("messageComposer");
        mouseClick(composer);
        verify(composer.activeFocus);
        var start = list.model.rowCount();
        var index = 0;
        for (index = 0; index < 24; ++index) {
            var minute = index < 10 ? "0" + index : "" + index;
            injectOmarchyChat("anna", "fred", "scroll line " + index, "10:" + minute);
        }
        waitForRowCount(list, start + 24);
        waitForRendering(appWindow.contentItem);
        list.pinToEnd();
        waitForRendering(appWindow.contentItem);
        wait(0);
        verify(list.contentHeight > list.height);
        verify(transcriptPinned(list));
    }

    function test_memberListScrollbarAppearsWhenOverflowing() {
        openSeededAppWindow();
        verify(item("membersPanel").visible);
        var members = item("membersList");
        var bar = verticalScrollBar(members);
        compare(bar.policy, Controls.ScrollBar.AsNeeded);

        var names = [];
        var index = 0;
        for (; index < 30; ++index)
            names.push("bulk" + index);
        seed.injectOmarchy(
            ":server 353 fred = #omarchy :" + names.join(" ") + "\r\n"
            + ":server 366 fred #omarchy :End of NAMES\r\n");
        tryVerify(function() { return members.count === 30; });
        waitForRendering(appWindow.contentItem);
        verify(members.contentHeight > members.height);
        verify(bar.size < 1);

        keyClick(Qt.Key_P, Qt.ControlModifier | Qt.ShiftModifier);
        tryCompare(members, "activeFocus", true);
        keyClick(Qt.Key_Down);
        tryCompare(members, "currentIndex", 1);
    }

    function test_memberListPageDownMovesByVisiblePage() {
        openSeededAppWindow();
        var members = focusOverflowingMemberList();
        var transcript = item("messageList");
        compare(firstVisibleIndex(members), 0);

        var transcriptY = transcript.contentY;
        var expected = memberListPageDownIndex(members);
        keyClick(Qt.Key_PageDown);
        waitForRendering(appWindow.contentItem);
        wait(0);
        tryCompare(members, "currentIndex", expected);
        verify(firstVisibleIndex(members) > 0);
        compare(transcript.contentY, transcriptY,
                "Page Down with member list focused should not scroll the transcript");

        expected = memberListPageDownIndex(members);
        keyClick(Qt.Key_PageDown);
        waitForRendering(appWindow.contentItem);
        wait(0);
        tryCompare(members, "currentIndex", expected);
        compare(transcript.contentY, transcriptY,
                "another Page Down should still leave the transcript alone");
    }

    function test_memberListPageUpRestoresPriorViewport() {
        openSeededAppWindow();
        var members = focusOverflowingMemberList(60);

        keyClick(Qt.Key_PageDown);
        waitForRendering(appWindow.contentItem);
        wait(0);
        var firstAfterOneDown = firstVisibleIndex(members);

        keyClick(Qt.Key_PageDown);
        waitForRendering(appWindow.contentItem);
        wait(0);
        verify(firstVisibleIndex(members) > firstAfterOneDown,
               "a second Page Down should move the viewport farther down");

        keyClick(Qt.Key_PageUp);
        waitForRendering(appWindow.contentItem);
        wait(0);
        compare(firstVisibleIndex(members), firstAfterOneDown,
                "Page Up should undo one Page Down without overshooting");
    }

    function test_memberListPageDownUsesViewportNotSelection() {
        openSeededAppWindow();
        var members = focusOverflowingMemberList();
        var step = 0;
        for (step = 0; step < 5; ++step)
            keyClick(Qt.Key_Down);
        tryCompare(members, "currentIndex", 5);
        compare(firstVisibleIndex(members), 0,
                "arrow keys should not scroll the member viewport yet");

        var expected = memberListPageDownIndex(members);
        keyClick(Qt.Key_PageDown);
        waitForRendering(appWindow.contentItem);
        wait(0);
        compare(members.currentIndex, expected,
                "Page Down should page from the viewport edge, not the selection");
    }

    function test_memberListShiftPageMovesHalfViewport() {
        openSeededAppWindow();
        var members = focusOverflowingMemberList();
        var halfPage = memberListPageSize(members, 0.35);
        compare(firstVisibleIndex(members), 0);

        var expected = memberListPageDownIndex(members, 0.35);
        keyClick(Qt.Key_PageDown, Qt.ShiftModifier);
        waitForRendering(appWindow.contentItem);
        wait(0);
        tryCompare(members, "currentIndex", expected);

        var mid = firstVisibleIndex(members);
        keyClick(Qt.Key_PageDown);
        waitForRendering(appWindow.contentItem);
        wait(0);
        var fullPage = memberListPageSize(members);
        verify(firstVisibleIndex(members) - mid > halfPage,
               "Page Down should hop farther than Shift+Page Down from the same start");
        verify(firstVisibleIndex(members) - mid <= fullPage + 1,
               "Page Down should move by about one visible page");
    }

    function test_memberListHomeEndJumpToEnds() {
        openSeededAppWindow();
        var members = focusOverflowingMemberList();

        keyClick(Qt.Key_End);
        waitForRendering(appWindow.contentItem);
        wait(0);
        tryCompare(members, "currentIndex", members.count - 1);

        keyClick(Qt.Key_Home);
        waitForRendering(appWindow.contentItem);
        wait(0);
        tryCompare(members, "currentIndex", 0);
    }

    function test_pageDownWithComposerStillScrollsTranscript() {
        openSeededAppWindow();
        var composer = item("messageComposer");
        var list = item("messageList");
        mouseClick(composer);
        tryCompare(composer, "activeFocus", true);
        fillTranscriptUntilScrollable(list);
        pageTranscriptToEnd(list);
        verify(transcriptPinned(list));

        var before = list.contentY;
        keyClick(Qt.Key_PageUp);
        waitForRendering(appWindow.contentItem);
        wait(0);
        verify(list.contentY < before,
               "Page Up with composer focused should still scroll the transcript");
        tryCompare(composer, "activeFocus", true);
        verify(!item("membersList").activeFocus,
               "composer-focused paging should not move member list focus");
    }

    function test_ctrlHomeEndWithMembersFocusedJumpsTranscript() {
        openSeededAppWindow();
        var members = focusOverflowingMemberList();
        var list = item("messageList");
        fillTranscriptUntilScrollable(list);
        pageTranscriptToEnd(list);
        verify(transcriptPinned(list));

        keyClick(Qt.Key_P, Qt.ControlModifier | Qt.ShiftModifier);
        tryCompare(members, "activeFocus", true);
        var memberIndex = members.currentIndex;

        keyClick(Qt.Key_Home, Qt.ControlModifier);
        waitForRendering(appWindow.contentItem);
        wait(0);
        compare(firstVisibleIndex(list), 0,
               "Ctrl+Home with members focused should jump transcript to top");
        compare(members.currentIndex, memberIndex,
               "Ctrl+Home should not move member list selection");

        mouseClick(item("messageComposer"));
        tryCompare(item("messageComposer"), "activeFocus", true);
        pageTranscriptToEnd(list);
        keyClick(Qt.Key_P, Qt.ControlModifier | Qt.ShiftModifier);
        tryCompare(members, "activeFocus", true);
        memberIndex = members.currentIndex;

        keyClick(Qt.Key_End, Qt.ControlModifier);
        waitForRendering(appWindow.contentItem);
        wait(0);
        verify(transcriptPinned(list),
               "Ctrl+End with members focused should jump transcript to bottom");
        compare(members.currentIndex, memberIndex,
               "Ctrl+End should not move member list selection");
    }

    function test_pageKeysWithHiddenMembersPanelStayHidden() {
        openSeededAppWindow();
        var members = focusOverflowingMemberList();
        var list = item("messageList");
        fillTranscriptUntilScrollable(list);
        pageTranscriptToEnd(list);

        keyClick(Qt.Key_P, Qt.ControlModifier | Qt.ShiftModifier);
        tryCompare(members, "activeFocus", true);
        keyClick(Qt.Key_PageDown);
        waitForRendering(appWindow.contentItem);
        wait(0);
        var memberIndex = members.currentIndex;
        var memberContentY = members.contentY;

        keyClick(Qt.Key_M, Qt.ControlModifier | Qt.ShiftModifier);
        tryCompare(item("membersPanel"), "visible", false);
        compare(appWindow.membersVisible, false);
        tryCompare(item("messageComposer"), "activeFocus", true);

        var before = list.contentY;
        keyClick(Qt.Key_PageUp);
        waitForRendering(appWindow.contentItem);
        wait(0);
        verify(list.contentY < before,
               "Page Up with the members panel hidden should scroll the transcript");
        compare(members.currentIndex, memberIndex,
               "Page Up with hidden panel should not move member list selection");
        compare(members.contentY, memberContentY,
               "Page Up with hidden panel should not scroll the member list");
        compare(appWindow.membersVisible, false);
        tryCompare(item("membersPanel"), "visible", false);

        keyClick(Qt.Key_PageDown);
        waitForRendering(appWindow.contentItem);
        wait(0);
        compare(appWindow.membersVisible, false,
                "Page Down should not reopen the members panel");
        tryCompare(item("membersPanel"), "visible", false);
        compare(members.currentIndex, memberIndex,
               "Page Down with hidden panel should not move member list selection");
        compare(members.contentY, memberContentY,
               "Page Down with hidden panel should not scroll the member list");
    }

    function test_pageKeysOnDirectMessageScrollTranscript() {
        openSeededAppWindow();
        mouseClick(namedItem(liveConversation("anna")));
        tryCompare(appWindow, "currentConversation", "anna");
        verify(!item("membersPanel").visible);

        var list = item("messageList");
        fillDirectMessageUntilScrollable(list);
        pageTranscriptToEnd(list);
        verify(transcriptPinned(list));

        var before = list.contentY;
        keyClick(Qt.Key_PageUp);
        waitForRendering(appWindow.contentItem);
        wait(0);
        verify(list.contentY < before,
               "Page Up on a direct message should scroll the transcript");
        var afterUp = list.contentY;

        keyClick(Qt.Key_PageDown);
        waitForRendering(appWindow.contentItem);
        wait(0);
        verify(list.contentY > afterUp,
               "Page Down on a direct message should scroll the transcript");
    }

    function test_pageKeysOnStatusScrollTranscript() {
        openSeededAppWindow();
        var list = item("consoleList");
        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);
        fillConsoleUntilScrollable(list);
        pageTranscriptToEnd(list);

        var before = list.contentY;
        keyClick(Qt.Key_PageUp);
        waitForRendering(appWindow.contentItem);
        wait(0);
        verify(list.contentY < before,
               "Page Up on Status should scroll the console transcript");
        var afterUp = list.contentY;

        keyClick(Qt.Key_PageDown);
        waitForRendering(appWindow.contentItem);
        wait(0);
        verify(list.contentY > afterUp,
               "Page Down on Status should scroll the console transcript");
        compare(appWindow.consoleVisible, true);
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

    function test_ctrlKJumpFilterPlaceholder() {
        openSeededAppWindow();
        openJumpSheet();
        compare(item("jumpFilter").text, "");
        tryCompare(item("jumpFilter"), "activeFocus", true);
        compare(item("jumpFilterPlaceholder").visible, true);
        typeText("ric");
        tryCompare(item("jumpFilter"), "text", "ric");
        compare(item("jumpFilterPlaceholder").visible, false);
    }

    function test_ctrlShiftKOpensFilteredNickAndCreatesDm() {
        openSeededAppWindow();
        compare(appWindow.currentConversation, "#omarchy");
        var previousCount = appWindow.irc.conversations.rowCount();
        var composer = item("messageComposer");
        var sheet = openNickSheet();
        compare(item("nickFilterPlaceholder").visible, true);
        compare(item("nickFilterPlaceholder").text, "Jump to nick…");
        compare(nickModelRows().join(" "),
                "fred=~fred anna=&anna dax=@dax mira=@mira kai=%kai teo=+teo "
                + "ivy=ivy lena=lena max=max nora=nora sam=sam sol=sol");
        compare(appWindow.nickSelectedIndex, 1);
        compare(item("nickModel").get(1).name, "anna");
        saveScreenshot("jump-to-nick");

        typeText("mi");
        tryCompare(item("nickFilter"), "text", "mi");
        compare(item("nickFilterPlaceholder").visible, false);
        var model = item("nickModel");
        compare(model.count, 1);
        compare(model.get(0).name, "mira");
        compare(model.get(0).label, "@mira");
        compare(model.get(0).memberStatus, "making tea");
        compare(appWindow.nickSelectedIndex, 0);

        keyClick(Qt.Key_Return);
        tryCompare(sheet, "opened", false);
        tryCompare(appWindow, "currentConversation", "mira");
        compare(appWindow.currentTopic, "Direct message with mira");
        tryVerify(function() {
            return appWindow.irc.conversations.rowCount() === previousCount + 1;
        });
        verify(!item("membersPanel").visible);
        tryCompare(composer, "activeFocus", true);
    }

    function test_ctrlShiftKSelectsExistingDirectMessage() {
        openSeededAppWindow();
        var previousCount = appWindow.irc.conversations.rowCount();
        var sheet = openNickSheet();
        typeText("anna");
        tryCompare(item("nickFilter"), "text", "anna");
        compare(item("nickModel").count, 1);
        compare(item("nickModel").get(0).name, "anna");
        keyClick(Qt.Key_Return);
        tryCompare(sheet, "opened", false);
        tryCompare(appWindow, "currentConversation", "anna");
        compare(appWindow.irc.conversations.rowCount(), previousCount);
        compare(appWindow.currentTopic, "Direct message with anna");
    }

    function test_ctrlShiftKEscapeKeepsConversationAndDraft() {
        openSeededAppWindow();
        var composer = item("messageComposer");
        mouseClick(composer);
        verify(composer.activeFocus);
        typeText("keep this draft");
        compare(composer.text, "keep this draft");

        var sheet = openNickSheet();
        typeText("teo");
        tryCompare(item("nickFilter"), "text", "teo");
        keyClick(Qt.Key_Escape);
        tryCompare(sheet, "opened", false);
        compare(appWindow.currentConversation, "#omarchy");
        compare(composer.text, "keep this draft");
        tryCompare(composer, "activeFocus", true);
    }

    function test_ctrlShiftKOrderMatchesMemberPanel() {
        openSeededAppWindow();
        verify(item("membersPanel").visible);
        var panel = renderedMembers();
        openNickSheet();
        compare(nickModelRows().join(" "), panel.join(" "));
    }

    function test_ctrlShiftKWorksWithMembersHidden() {
        openSeededAppWindow();
        keyClick(Qt.Key_M, Qt.ControlModifier | Qt.ShiftModifier);
        tryCompare(item("membersPanel"), "visible", false);
        var previousCount = appWindow.irc.conversations.rowCount();
        var sheet = openNickSheet();
        verify(!item("membersPanel").visible);
        typeText("kai");
        tryCompare(item("nickFilter"), "text", "kai");
        compare(item("nickModel").count, 1);
        compare(item("nickModel").get(0).name, "kai");
        keyClick(Qt.Key_Return);
        tryCompare(sheet, "opened", false);
        tryCompare(appWindow, "currentConversation", "kai");
        tryVerify(function() {
            return appWindow.irc.conversations.rowCount() === previousCount + 1;
        });
    }

    function test_ctrlShiftKShowsAwayState() {
        openSeededAppWindow();
        openNickSheet();
        var model = item("nickModel");
        var teo = -1;
        var index = 0;
        for (; index < model.count; ++index) {
            if (model.get(index).name === "teo") {
                teo = index;
                break;
            }
        }
        verify(teo >= 0, "teo should be in the nick sheet");
        compare(model.get(teo).awayFlag, 1);
        compare(model.get(teo).label, "+teo");
        var list = item("nickList");
        list.positionViewAtIndex(teo, ListView.Contain);
        tryVerify(function() {
            return list.itemAtIndex(teo) !== null;
        });
        var row = list.itemAtIndex(teo);
        verify(row !== null, "teo nick row should render");
        compare(row.nick, "teo");
        compare(row.showAway, true);
        var dot = findChild(row, "nickPick-presence-teo");
        verify(dot !== null, "nick sheet should show teo's presence dot");
        compare(dot.visible, true);
        verify(Qt.colorEqual(dot.color, "#d6a552"));
    }

    function test_ctrlShiftKIgnoredOnDirectMessage() {
        openSeededAppWindow();
        mouseClick(namedItem(liveConversation("anna")));
        tryCompare(appWindow, "currentConversation", "anna");
        keyClick(Qt.Key_K, Qt.ControlModifier | Qt.ShiftModifier);
        compare(item("nickSheet").opened, false);
        compare(appWindow.currentConversation, "anna");
    }

    function test_ctrlShiftKIgnoredOnStatus() {
        openSeededAppWindow();
        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);
        keyClick(Qt.Key_K, Qt.ControlModifier | Qt.ShiftModifier);
        compare(item("nickSheet").opened, false);
        compare(appWindow.consoleVisible, true);
    }

    function test_ctrlShiftKIgnoredWhenConnectIsVisible() {
        openSeededAppWindow();
        compare(appWindow.currentConversation, "#omarchy");
        verify(item("membersPanel").visible);
        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        tryCompare(appWindow, "connectionOverlayVisible", true);
        tryCompare(item("connectionSheet"), "visible", true);

        keyClick(Qt.Key_K, Qt.ControlModifier | Qt.ShiftModifier);
        compare(item("nickSheet").opened, false);
        compare(appWindow.connectionOverlayVisible, true);

        keyClick(Qt.Key_M, Qt.ControlModifier | Qt.ShiftModifier);
        compare(appWindow.membersVisible, true);
        compare(item("membersPanel").visible, true);
        compare(appWindow.connectionOverlayVisible, true);
    }

    function test_connectionSheetBlocksSidebarAndMembers() {
        openSeededAppWindow();
        compare(appWindow.currentConversation, "#omarchy");
        verify(item("membersPanel").visible);
        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        tryCompare(appWindow, "connectionOverlayVisible", true);
        var sheet = item("connectionSheet");
        tryCompare(sheet, "visible", true);
        compare(sheet.width, appWindow.width);
        compare(sheet.height, appWindow.height);

        mouseClick(namedItem(liveConversation("#desktop")));
        wait(0);
        compare(appWindow.currentConversation, "#omarchy");

        if (!appWindow.connectionOverlayVisible)
            keyClick(Qt.Key_Comma, Qt.ControlModifier);
        tryCompare(appWindow, "connectionOverlayVisible", true);
        mouseClick(appWindow.contentItem, 40, 124);
        wait(0);
        compare(appWindow.currentConversation, "#omarchy");

        if (!appWindow.connectionOverlayVisible)
            keyClick(Qt.Key_Comma, Qt.ControlModifier);
        tryCompare(appWindow, "connectionOverlayVisible", true);
        mouseClick(appWindow.contentItem, 1070, 200);
        wait(0);
        compare(appWindow.currentConversation, "#omarchy");

        if (!appWindow.connectionOverlayVisible)
            keyClick(Qt.Key_Comma, Qt.ControlModifier);
        tryCompare(item("connectionSheet"), "visible", true);
        keyClick(Qt.Key_Down, Qt.AltModifier);
        compare(appWindow.currentConversation, "#omarchy");
        verify(item("connectionSheet").visible);

        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        compare(appWindow.consoleVisible, false);
        verify(item("connectionSheet").visible);

        mouseClick(liveHeaderButton(seed.omarchyNetworkId));
        compare(appWindow.consoleVisible, false);
        compare(appWindow.currentConversation, "#omarchy");
    }

    function test_connectionSheetFirstRunIgnoresSidebarClicks() {
        liveIrc.selectedTarget = "#omarchy";
        liveIrc.selectedNetworkId = "libera";
        liveIrc.selectedConversationId = "libera\n#omarchy";
        liveIrc.isChannel = true;
        liveConsole.open = false;
        var window = createTemporaryObject(setupWindowComponent, null);
        verify(window !== null, "The setup window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        var sheet = findChild(window, "connectionSheet");
        verify(sheet !== null, "Could not find connectionSheet");
        verify(sheet.visible);
        compare(sheet.width, window.width);
        compare(sheet.height, window.height);

        // Left of the centered card, over the sidebar. First-run must ignore
        // that click instead of switching conversation or dismissing.
        mouseClick(window.contentItem, 40, 140);
        compare(liveIrc.selectedTarget, "#omarchy");
        verify(sheet.visible);

        var header = findNamedIn(window, "networkHeaderButton-setup-id");
        verify(header !== null, "Could not find networkHeaderButton-setup-id");
        mouseClick(header);
        compare(liveConsole.open, false);
        verify(sheet.visible);
        window.close();
        liveIrc.selectedTarget = "#omarchy";
        liveIrc.selectedConversationId = "libera\n#omarchy";
        liveIrc.isChannel = true;
        liveConsole.open = false;
    }

    function test_membersHeadingOpensNickSheet() {
        openSeededAppWindow();
        verify(item("membersPanel").visible);
        mouseClick(item("membersHeadingButton"));
        var sheet = item("nickSheet");
        tryCompare(sheet, "opened", true);
        tryCompare(item("nickFilter"), "activeFocus", true);
        compare(item("nickModel").get(0).name, "fred");
        keyClick(Qt.Key_Escape);
        tryCompare(sheet, "opened", false);
        tryCompare(item("messageComposer"), "activeFocus", true);
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

    function test_openDirectMessageFromTranscriptNick() {
        openSeededAppWindow();
        var list = item("messageList");
        var composer = item("messageComposer");
        var start = list.model.rowCount();
        injectOmarchyChat("mira", "#omarchy", "transcript-dm-mira");
        injectOmarchyChat("fred", "#omarchy", "transcript-dm-self");
        injectOmarchyChat("anna", "#omarchy", "transcript-dm-anna");
        injectOmarchyChat("mira", "#omarchy", "transcript-dm-group-lead", "12:02");
        injectOmarchyChat("mira", "#omarchy", "transcript-dm-group-follow", "12:02");
        seed.injectOmarchy(":rio!u@h PART #omarchy\r\n");
        waitForBody(list, "rio left");

        mouseClick(composer);
        typeText("omarchy draft stays");
        compare(composer.text, "omarchy draft stays");

        var previousCount = appWindow.irc.conversations.rowCount();
        clickNamedInRow(renderedRowWithBody("transcript-dm-mira"), "messageAvatar");
        tryCompare(appWindow, "currentConversation", "mira");
        compare(appWindow.currentTopic, "Direct message with mira");
        tryVerify(function() {
            return appWindow.irc.conversations.rowCount() === previousCount + 1;
        });
        verify(visibleDirects(seed.omarchyNetworkId).indexOf("mira") !== -1);
        verify(!item("membersPanel").visible);
        saveScreenshot("open-direct-message-transcript");

        selectOmarchy();
        compare(composer.text, "omarchy draft stays");
        clickNamedInRow(renderedRowWithBody("transcript-dm-mira"), "messageAuthor");
        tryCompare(appWindow, "currentConversation", "mira");
        compare(appWindow.irc.conversations.rowCount(), previousCount + 1);

        selectOmarchy();
        var annaCount = appWindow.irc.conversations.rowCount();
        clickNamedInRow(renderedRowWithBody("transcript-dm-anna"), "messageAuthor");
        tryCompare(appWindow, "currentConversation", "anna");
        compare(appWindow.irc.conversations.rowCount(), annaCount);
        compare(field(item("messageList").model, item("messageList").model.rowCount() - 1,
                      "body"),
                "fred: The prototype already feels at home. Nice work.");

        selectOmarchy();
        clickNamedInRow(renderedRowWithBody("transcript-dm-self"), "messageAvatar");
        compare(appWindow.currentConversation, "#omarchy");
        clickNamedInRow(renderedRowWithBody("transcript-dm-self"), "messageAuthor");
        compare(appWindow.currentConversation, "#omarchy");

        var eventRow = renderedRowWithBody("rio left");
        compare(findChild(eventRow, "messageAvatar").visible, false);
        compare(findChild(eventRow, "messageHeader").visible, false);
        mouseClick(findChild(eventRow, "messageEvent"));
        compare(appWindow.currentConversation, "#omarchy");

        var grouped = renderedRowWithBody("transcript-dm-group-follow");
        compare(findChild(grouped, "messageAvatar").visible, false);
        compare(findChild(grouped, "messageHeader").visible, false);
        mouseClick(findChild(grouped, "messageBody"));
        compare(appWindow.currentConversation, "#omarchy");

        clickMember("mira");
        tryCompare(appWindow, "currentConversation", "mira");
        compare(appWindow.irc.conversations.rowCount(), previousCount + 1);

        selectOmarchy();
        seed.injectOmarchy(":NickServ!NickServ@services PRIVMSG fred "
                           + ":This nickname is registered.\r\n");
        compare(findNamed(liveConversation("NickServ")), null);
        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);
        var consoleList = item("consoleList");
        tryVerify(function() {
            var row = 0;
            for (; row < consoleList.model.rowCount(); ++row) {
                if (field(consoleList.model, row, "text").indexOf(
                        "This nickname is registered.") >= 0)
                    return true;
            }
            return false;
        });
        compare(findNamed(liveConversation("NickServ")), null);
    }

    function test_connectionSheetCtrlEnterAppliesFromAnyFocus() {
        restoreNamedConnection();
        namedConnection.applySucceeds = true;
        var window = createTemporaryObject(fallbackWindowComponent, null);
        verify(window !== null, "The ctrl-enter window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        var sheet = findChild(window, "connectionSheet");
        var rail = findChild(window, "networkChoiceRepeater");

        // Ctrl+Enter used to be wired per control, so it did nothing wherever
        // focus sat somewhere without its own handler. It is now one
        // window-level action that works from every stop in the sheet.
        var targets = [
            "networkChoice-libera",
            "connectionShortcutsHint",
            "connectionSheetTab-connection",
            "connectionSheetTab-preferences",
            "connectionTls",
            "connectionConnectOnStartup",
            "connectionPassword",
            "connectionNickServ",
            "connectionApply"
        ];
        var i = 0;
        for (i = 0; i < targets.length; ++i) {
            keyClick(Qt.Key_Comma, Qt.ControlModifier);
            tryCompare(sheet, "visible", true);
            waitForRendering(window.contentItem);

            var target = findChild(window, targets[i]);
            if (target === null)
                target = repeaterItemByName(rail, targets[i]);
            verify(target !== null, "Could not find " + targets[i]);
            target.forceActiveFocus();
            tryCompare(target, "activeFocus", true);

            namedConnection.applyCalls = 0;
            keyClick(Qt.Key_Return, Qt.ControlModifier);
            compare(namedConnection.applyCalls, 1);
            compare(sheet.visible, false);
        }

        // Reaching the row by Tab, the way a user does, works the same.
        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        tryCompare(sheet, "visible", true);
        waitForRendering(window.contentItem);

        var reached = false;
        var step = 0;
        for (step = 0; step < 40 && !reached; ++step) {
            keyClick(Qt.Key_Tab);
            wait(0);
            reached = focusObjectName(window).indexOf("networkChoice-") === 0;
        }
        verify(reached, "Tab should reach a network row");
        namedConnection.applyCalls = 0;
        keyClick(Qt.Key_Return, Qt.ControlModifier);
        compare(namedConnection.applyCalls, 1);
        compare(sheet.visible, false);

        // Apply commits the selected network from the whole sheet,
        // including Preferences.
        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        tryCompare(sheet, "visible", true);
        waitForRendering(window.contentItem);
        window.connectionSheetTab = "preferences";
        waitForRendering(window.contentItem);
        var preferencesTab = findChild(window, "connectionSheetTab-preferences");
        verify(preferencesTab !== null, "Could not find connectionSheetTab-preferences");
        preferencesTab.forceActiveFocus();
        tryCompare(preferencesTab, "activeFocus", true);
        namedConnection.applyCalls = 0;
        keyClick(Qt.Key_Return, Qt.ControlModifier);
        compare(namedConnection.applyCalls, 1);
        compare(sheet.visible, false);

        window.close();
        restoreNamedConnection();
    }

    function test_connectionSheetCtrlEnterDoesNotApplyWhileShortcutsOpen() {
        restoreNamedConnection();
        namedConnection.applySucceeds = true;
        var window = createTemporaryObject(fallbackWindowComponent, null);
        verify(window !== null, "The ctrl-enter-over-shortcuts window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        tryCompare(window, "connectionOverlayVisible", true);
        waitForRendering(window.contentItem);

        keyClick(Qt.Key_Slash, Qt.ControlModifier);
        if (!window.shortcutOverlayOpen) {
            var hint = findChild(window, "connectionShortcutsHint");
            verify(hint !== null, "Could not find connectionShortcutsHint");
            mouseClick(hint);
        }
        tryCompare(window, "shortcutOverlayOpen", true);

        window.connectionSheetTab = "preferences";
        waitForRendering(window.contentItem);
        compare(window.connectionOverlayVisible, true);

        namedConnection.applyCalls = 0;
        keyClick(Qt.Key_Return, Qt.ControlModifier);
        compare(namedConnection.applyCalls, 0);
        compare(window.connectionOverlayVisible, true);
        compare(window.shortcutOverlayOpen, true);

        keyClick(Qt.Key_Enter, Qt.ControlModifier);
        compare(namedConnection.applyCalls, 0);
        compare(window.connectionOverlayVisible, true);
        compare(window.shortcutOverlayOpen, true);

        keyClick(Qt.Key_Escape);
        tryCompare(window, "shortcutOverlayOpen", false);
        compare(window.connectionOverlayVisible, true);

        keyClick(Qt.Key_Return, Qt.ControlModifier);
        compare(namedConnection.applyCalls, 1);
        compare(window.connectionOverlayVisible, false);

        window.close();
        restoreNamedConnection();
    }

    function test_connectionSheetCtrlTabSwitchesTabsFromAnywhere() {
        restoreNamedConnection();
        var window = createTemporaryObject(fallbackWindowComponent, null);
        verify(window !== null, "The ctrl-tab window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        var sheet = findChild(window, "connectionSheet");
        tryCompare(sheet, "visible", true);
        waitForRendering(window.contentItem);

        var connectionTab = findChild(window, "connectionSheetTab-connection");
        var preferencesTab = findChild(window, "connectionSheetTab-preferences");
        var host = findChild(window, "connectionHost");
        verify(connectionTab !== null, "Could not find connectionSheetTab-connection");
        verify(preferencesTab !== null, "Could not find connectionSheetTab-preferences");
        verify(host !== null, "Could not find connectionHost");
        compare(window.connectionSheetTab, "connection");

        host.forceActiveFocus();
        tryCompare(host, "activeFocus", true);
        keyClick(Qt.Key_Tab, Qt.ControlModifier);
        wait(0);
        compare(window.connectionSheetTab, "preferences");
        tryCompare(preferencesTab, "activeFocus", true);
        compare(focusObjectName(window), "connectionSheetTab-preferences");
        compare(host.activeFocus, false);
        verify(findChild(window, "connectionPreferencesPanel").visible);
        compare(findChild(window, "connectionFormArea").visible, false);

        keyClick(Qt.Key_Tab);
        wait(0);
        compare(focusObjectName(window), "connectionReopenDirects");

        keyClick(Qt.Key_Tab, Qt.ControlModifier);
        wait(0);
        compare(window.connectionSheetTab, "connection");
        tryCompare(connectionTab, "activeFocus", true);
        compare(focusObjectName(window), "connectionSheetTab-connection");
        verify(findChild(window, "connectionFormArea").visible);

        host.forceActiveFocus();
        tryCompare(host, "activeFocus", true);
        keyClick(Qt.Key_Tab, Qt.ControlModifier | Qt.ShiftModifier);
        wait(0);
        compare(window.connectionSheetTab, "connection");
        keyClick(Qt.Key_Backtab, Qt.ControlModifier);
        wait(0);
        compare(window.connectionSheetTab, "connection");

        window.close();
        restoreNamedConnection();
    }

    function test_connectionSheetAltArrowsWalkNetworksFromHost() {
        restoreNamedConnection();
        namedNetworks.append({
            networkId: "oftc",
            displayName: "irc.oftc.net",
            stored: true,
            selected: false,
            iconColor: 1,
            iconUrl: "",
            collapsed: false
        });
        var extra = 0;
        for (extra = 0; extra < 16; ++extra) {
            namedNetworks.append({
                networkId: "probe-" + extra,
                displayName: "irc.probe" + extra + ".example",
                stored: true,
                selected: false,
                iconColor: 1,
                iconUrl: "",
                collapsed: false
            });
        }
        var window = createTemporaryObject(fallbackWindowComponent, null);
        verify(window !== null, "The host-alt-arrow window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        tryCompare(findChild(window, "connectionSheet"), "visible", true);
        waitForRendering(window.contentItem);

        var host = findChild(window, "connectionHost");
        var rail = findChild(window, "networkChoiceRepeater");
        var scroll = findChild(window, "networkChoiceScroll");
        verify(host !== null, "Could not find connectionHost");
        host.forceActiveFocus();
        tryCompare(host, "activeFocus", true);
        compare(host.text, "irc.libera.chat");
        compare(namedConnection.selectedNetworkId, "libera");
        compare(namedConnection.nick, "sheet-nick");

        function rowIsVisible(row) {
            return row.y >= scroll.contentY
                && row.y + row.height <= scroll.contentY + scroll.height;
        }

        keyClick(Qt.Key_Right, Qt.AltModifier);
        wait(0);
        compare(namedConnection.selectedNetworkId, "oftc");
        compare(host.text, "irc.oftc.net");
        compare(namedConnection.nick, "sheet-nick");
        tryCompare(host, "activeFocus", true);
        compare(focusObjectName(window), "connectionHost");

        keyClick(Qt.Key_Left, Qt.AltModifier);
        wait(0);
        compare(namedConnection.selectedNetworkId, "libera");
        compare(host.text, "irc.libera.chat");
        tryCompare(host, "activeFocus", true);
        compare(focusObjectName(window), "connectionHost");

        var step = 0;
        for (step = 0; step < 17; ++step)
            keyClick(Qt.Key_Right, Qt.AltModifier);
        wait(0);
        compare(namedConnection.selectedNetworkId, "probe-15");
        compare(host.text, "irc.probe15.example");
        tryCompare(host, "activeFocus", true);
        compare(focusObjectName(window), "connectionHost");
        var last = repeaterItemByName(rail, "networkChoice-probe-15");
        verify(last !== null, "Could not find networkChoice-probe-15");
        verify(rowIsVisible(last));
        verify(scroll.contentY > 0);

        window.close();
        restoreNamedConnection();
    }

    function test_connectionSheetAltArrowsMoveRailFocus() {
        restoreNamedConnection();
        namedNetworks.append({
            networkId: "oftc",
            displayName: "irc.oftc.net",
            stored: true,
            selected: false,
            iconColor: 1,
            iconUrl: "",
            collapsed: false
        });
        var window = createTemporaryObject(fallbackWindowComponent, null);
        verify(window !== null, "The rail-alt-arrow window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        tryCompare(findChild(window, "connectionSheet"), "visible", true);
        waitForRendering(window.contentItem);

        var reached = false;
        var step = 0;
        for (step = 0; step < 40 && !reached; ++step) {
            keyClick(Qt.Key_Tab);
            wait(0);
            reached = focusObjectName(window).indexOf("networkChoice-") === 0;
        }
        verify(reached, "Tab should reach a network row");
        compare(focusObjectName(window), "networkChoice-libera");
        compare(namedConnection.selectedNetworkId, "libera");

        keyClick(Qt.Key_Right, Qt.AltModifier);
        wait(0);
        compare(namedConnection.selectedNetworkId, "oftc");
        compare(focusObjectName(window), "networkChoice-oftc");
        compare(findChild(window, "connectionHost").text, "irc.oftc.net");

        var addButton = findChild(window, "connectionAddNetwork");
        verify(addButton !== null, "Could not find connectionAddNetwork");
        addButton.forceActiveFocus();
        tryCompare(addButton, "activeFocus", true);
        keyClick(Qt.Key_Left, Qt.AltModifier);
        wait(0);
        compare(namedConnection.selectedNetworkId, "libera");
        compare(focusObjectName(window), "networkChoice-libera");
        compare(addButton.activeFocus, false);

        window.close();
        restoreNamedConnection();
    }

    function test_connectionSheetCtrlNAddsThenDiscardRestores() {
        restoreNamedConnection();
        namedNetworks.append({
            networkId: "oftc",
            displayName: "irc.oftc.net",
            stored: true,
            selected: false,
            iconColor: 1,
            iconUrl: "",
            collapsed: false
        });
        var window = createTemporaryObject(fallbackWindowComponent, null);
        verify(window !== null, "The ctrl-n window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        var sheet = findChild(window, "connectionSheet");
        tryCompare(sheet, "visible", true);
        waitForRendering(window.contentItem);

        keyClick(Qt.Key_N, Qt.ControlModifier);
        wait(0);
        compare(namedNetworks.count, 3);
        compare(namedConnection.selectedNetworkId, "new-id");
        compare(findChild(window, "connectionHost").text, "");
        compare(findChild(window, "connectionName").text, "");
        tryCompare(findChild(window, "connectionName"), "activeFocus", true);
        compare(focusObjectName(window), "connectionName");

        var reachedDiscard = false;
        var step = 0;
        for (step = 0; step < 40 && !reachedDiscard; ++step) {
            keyClick(Qt.Key_Tab);
            wait(0);
            reachedDiscard = focusObjectName(window) === "connectionDiscard";
        }
        verify(reachedDiscard, "Tab should reach Discard after Ctrl+N");
        keyClick(Qt.Key_Return);
        compare(namedConnection.discardCalls, 1);
        compare(namedNetworks.count, 2);
        compare(namedConnection.selectedNetworkId, "libera");
        compare(findChild(window, "connectionHost").text, "irc.libera.chat");
        compare(findChild(window, "connectionName").text, "irc.libera.chat");
        compare(namedConnection.nick, "sheet-nick");
        verify(sheet.visible);

        window.close();
        restoreNamedConnection();
    }

    function test_connectionSheetCtrlShiftDeleteRemovesOrLeavesPresent() {
        restoreNamedConnection();
        namedNetworks.append({
            networkId: "oftc",
            displayName: "irc.oftc.net",
            stored: true,
            selected: false,
            iconColor: 1,
            iconUrl: "",
            collapsed: false
        });
        var window = createTemporaryObject(fallbackWindowComponent, null);
        verify(window !== null, "The ctrl-shift-delete window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        var sheet = findChild(window, "connectionSheet");
        tryCompare(sheet, "visible", true);
        waitForRendering(window.contentItem);
        compare(namedNetworks.count, 2);
        compare(namedConnection.selectedNetworkId, "libera");
        namedConnection.applyCalls = 0;

        keyClick(Qt.Key_Delete, Qt.ControlModifier | Qt.ShiftModifier);
        wait(0);
        compare(window.connectionRemoveArmed, true);
        compare(namedNetworks.count, 2);
        compare(namedConnection.selectedNetworkId, "libera");
        compare(namedConnection.applyCalls, 0);
        verify(sheet.visible);

        keyClick(Qt.Key_Escape);
        tryCompare(sheet, "visible", false);
        compare(namedNetworks.count, 2);
        compare(namedConnection.selectedNetworkId, "libera");
        compare(namedConnection.applyCalls, 0);

        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        tryCompare(sheet, "visible", true);
        waitForRendering(window.contentItem);
        compare(namedNetworks.count, 2);

        keyClick(Qt.Key_Delete, Qt.ControlModifier | Qt.ShiftModifier);
        wait(0);
        compare(window.connectionRemoveArmed, true);
        keyClick(Qt.Key_Delete, Qt.ControlModifier | Qt.ShiftModifier);
        wait(0);
        compare(namedNetworks.count, 1);
        compare(namedConnection.selectedNetworkId, "oftc");
        compare(namedConnection.applyCalls, 0);
        verify(sheet.visible);

        window.close();
        restoreNamedConnection();
    }

    function test_connectionSheetLeftRightFromHostStayInField() {
        restoreNamedConnection();
        namedNetworks.append({
            networkId: "oftc",
            displayName: "irc.oftc.net",
            stored: true,
            selected: false,
            iconColor: 1,
            iconUrl: "",
            collapsed: false
        });
        var window = createTemporaryObject(fallbackWindowComponent, null);
        verify(window !== null, "The host-arrow window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        tryCompare(findChild(window, "connectionSheet"), "visible", true);
        waitForRendering(window.contentItem);

        var host = findChild(window, "connectionHost");
        verify(host !== null, "Could not find connectionHost");
        host.forceActiveFocus();
        tryCompare(host, "activeFocus", true);
        compare(window.connectionSheetTab, "connection");
        compare(namedConnection.selectedNetworkId, "libera");

        keyClick(Qt.Key_Right);
        wait(0);
        tryCompare(host, "activeFocus", true);
        compare(focusObjectName(window), "connectionHost");
        compare(window.connectionSheetTab, "connection");
        compare(namedConnection.selectedNetworkId, "libera");

        keyClick(Qt.Key_Left);
        wait(0);
        tryCompare(host, "activeFocus", true);
        compare(focusObjectName(window), "connectionHost");
        compare(window.connectionSheetTab, "connection");
        compare(namedConnection.selectedNetworkId, "libera");
        compare(host.text, "irc.libera.chat");

        window.close();
        restoreNamedConnection();
    }

    function test_connectionSheetCtrlNAndRemoveNoopOnFirstRun() {
        var window = createTemporaryObject(setupWindowComponent, null);
        verify(window !== null, "The first-run chord window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        var nick = findChild(window, "connectionNick");
        verify(nick !== null, "Could not find connectionNick");
        tryCompare(nick, "activeFocus", true);
        compare(fakeConnection.canAdd, false);
        compare(fakeConnection.canRemove, false);
        compare(setupNetworks.count, 1);
        compare(fakeConnection.selectedNetworkId, "setup-id");
        compare(findChild(window, "connectionAddNetwork").visible, false);
        compare(findChild(window, "connectionRemove").visible, false);

        keyClick(Qt.Key_N, Qt.ControlModifier);
        wait(0);
        compare(setupNetworks.count, 1);
        compare(fakeConnection.selectedNetworkId, "setup-id");
        tryCompare(nick, "activeFocus", true);
        compare(window.connectionRemoveArmed, false);

        keyClick(Qt.Key_Delete, Qt.ControlModifier | Qt.ShiftModifier);
        wait(0);
        compare(setupNetworks.count, 1);
        compare(fakeConnection.selectedNetworkId, "setup-id");
        compare(window.connectionRemoveArmed, false);
        verify(findChild(window, "connectionSheet").visible);

        window.close();
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
        compare(findChild(window, "connectionName").text, "irc.libera.chat");
        compare(findChild(window, "connectionNick").text, "");
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
        var hiddenDisconnect = findChild(window, "connectionDisconnect");
        verify(hiddenDisconnect !== null, "Could not find connectionDisconnect");
        compare(hiddenDisconnect.visible, false);
        var nameField = findChild(window, "connectionName");
        var hostField = findChild(window, "connectionHost");
        verify(nameField.mapToItem(sheet, 0, 0).y < hostField.mapToItem(sheet, 0, 0).y,
               "Name field should sit above Host");
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

        // Not even the explicit commit may store a password while the profile
        // is still invalid.
        password.forceActiveFocus();
        tryCompare(password, "activeFocus", true);
        keyClick(Qt.Key_Return, Qt.ControlModifier);
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
            "connectionShortcutsHint",
            "connectionName",
            "connectionHost",
            "connectionPort",
            "connectionTls",
            "connectionNick",
            "connectionUsername",
            "connectionRealname",
            "connectionAutojoin",
            "connectionConnectOnStartup",
            "connectionPasswordHelp",
            "connectionPassword",
            "connectionNickServHelp",
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
        compare(focusObjectName(window), "connectionSheetTab-connection");
        keyClick(Qt.Key_Tab);
        wait(0);
        compare(focusObjectName(window), "connectionSheetTab-preferences");
        keyClick(Qt.Key_Tab);
        wait(0);
        compare(focusObjectName(window), "networkChoice-setup-id");

        keyClick(Qt.Key_Tab, Qt.ShiftModifier);
        wait(0);
        compare(focusObjectName(window), "connectionSheetTab-preferences");
        keyClick(Qt.Key_Tab, Qt.ShiftModifier);
        wait(0);
        compare(focusObjectName(window), "connectionSheetTab-connection");
        keyClick(Qt.Key_Tab, Qt.ShiftModifier);
        wait(0);
        compare(focusObjectName(window), "connectionApply");
        keyClick(Qt.Key_Tab, Qt.ShiftModifier);
        wait(0);
        compare(focusObjectName(window), "connectionDiscard");
        window.close();
    }

    function test_connectionSheetTabsSwitchWithArrowKeys() {
        var window = createTemporaryObject(setupWindowComponent, null);
        verify(window !== null, "The tab-window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        var connectionTab = findChild(window, "connectionSheetTab-connection");
        var preferencesTab = findChild(window, "connectionSheetTab-preferences");
        verify(connectionTab !== null, "Could not find connectionSheetTab-connection");
        verify(preferencesTab !== null, "Could not find connectionSheetTab-preferences");
        compare(window.connectionSheetTab, "connection");

        connectionTab.forceActiveFocus();
        tryCompare(connectionTab, "activeFocus", true);
        keyClick(Qt.Key_Right);
        wait(0);
        compare(window.connectionSheetTab, "preferences");
        tryCompare(preferencesTab, "activeFocus", true);

        var preferences = findChild(window, "connectionPreferencesPanel");
        verify(preferences !== null, "Could not find connectionPreferencesPanel");
        verify(preferences.visible);
        compare(findChild(window, "connectionFormArea").visible, false);

        keyClick(Qt.Key_Left);
        wait(0);
        compare(window.connectionSheetTab, "connection");
        tryCompare(connectionTab, "activeFocus", true);
        verify(findChild(window, "connectionFormArea").visible);

        // Stepping uses the direction, so it wraps both ways rather than
        // always toggling: Left from the first tab lands on the last, and
        // Right from the last lands back on the first.
        keyClick(Qt.Key_Left);
        wait(0);
        compare(window.connectionSheetTab, "preferences");
        tryCompare(preferencesTab, "activeFocus", true);

        keyClick(Qt.Key_Right);
        wait(0);
        compare(window.connectionSheetTab, "connection");
        tryCompare(connectionTab, "activeFocus", true);
        window.close();
    }

    function test_connectionSheetHelpShowsOnHoverAndFocus() {
        var window = createTemporaryObject(setupWindowComponent, null);
        verify(window !== null, "The help-hover window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);
        wait(200);

        var help = findChild(window, "connectionPasswordHelp");
        var host = findChild(window, "connectionHost");
        var tip = findChild(window, "connectionPasswordHelpTip");
        verify(tip !== null, "Could not find connectionPasswordHelpTip");
        var away = { x: 5, y: window.height - 5 };

        // Hover alone.
        mouseMove(window.contentItem, away.x, away.y);
        wait(500);
        compare(tip.visible, false);
        mouseMove(help, help.width / 2, help.height / 2);
        wait(800);
        compare(tip.visible, true);

        // Parking the pointer hides it again.
        mouseMove(window.contentItem, away.x, away.y);
        wait(700);
        compare(tip.visible, false);

        // Keyboard focus now drives the same declarative binding. The popup
        // used to be opened imperatively, which wrote `visible` underneath the
        // binding; hover must keep working after a focus/blur cycle.
        help.forceActiveFocus();
        tryCompare(help, "activeFocus", true);
        tryCompare(tip, "visible", true);

        host.forceActiveFocus();
        tryCompare(host, "activeFocus", true);
        tryCompare(tip, "visible", false);

        mouseMove(help, help.width / 2, help.height / 2);
        wait(900);
        compare(tip.visible, true);

        window.close();
    }

    function test_connectionSheetTabsRespondToClicks() {
        var window = createTemporaryObject(setupWindowComponent, null);
        verify(window !== null, "The tab-click window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        var connectionTab = findChild(window, "connectionSheetTab-connection");
        var preferencesTab = findChild(window, "connectionSheetTab-preferences");
        verify(connectionTab !== null, "Could not find connectionSheetTab-connection");
        verify(preferencesTab !== null, "Could not find connectionSheetTab-preferences");

        mouseClick(preferencesTab);
        compare(window.connectionSheetTab, "preferences");
        verify(findChild(window, "connectionPreferencesPanel").visible);
        compare(findChild(window, "connectionFormArea").visible, false);

        mouseClick(connectionTab);
        compare(window.connectionSheetTab, "connection");
        verify(findChild(window, "connectionFormArea").visible);
        window.close();
    }

    function test_connectionPreferencesReopenDirectsToggle() {
        liveIrc.reopenDirectMessages = true;
        var window = createTemporaryObject(setupWindowComponent, null);
        verify(window !== null, "The preferences-toggle window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        var preferencesTab = findChild(window, "connectionSheetTab-preferences");
        verify(preferencesTab !== null, "Could not find connectionSheetTab-preferences");
        mouseClick(preferencesTab);
        compare(window.connectionSheetTab, "preferences");

        var reopen = findChild(window, "connectionReopenDirects");
        verify(reopen !== null, "Could not find connectionReopenDirects");
        verify(reopen.visible);
        compare(reopen.checked, true);
        compare(liveIrc.reopenDirectMessages, true);

        mouseClick(reopen);
        compare(reopen.checked, false);
        compare(liveIrc.reopenDirectMessages, false);

        mouseClick(reopen);
        compare(reopen.checked, true);
        compare(liveIrc.reopenDirectMessages, true);

        preferencesTab.forceActiveFocus();
        tryCompare(preferencesTab, "activeFocus", true);
        keyClick(Qt.Key_Tab);
        wait(0);
        compare(focusObjectName(window), "connectionReopenDirects");

        var checkedBefore = reopen.checked;
        keyClick(Qt.Key_Return);
        compare(reopen.checked, checkedBefore);
        compare(liveIrc.reopenDirectMessages, checkedBefore);
        verify(findChild(window, "connectionSheet").visible);
        verify(focusObjectName(window) !== "connectionReopenDirects");

        window.close();
        liveIrc.reopenDirectMessages = true;
    }

    function test_connectionPreferencesOpenAtUnreadToggle() {
        liveIrc.openConversationsAtUnread = false;
        var window = createTemporaryObject(setupWindowComponent, null);
        verify(window !== null, "The open-at-unread window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        var preferencesTab = findChild(window, "connectionSheetTab-preferences");
        verify(preferencesTab !== null, "Could not find connectionSheetTab-preferences");
        mouseClick(preferencesTab);
        compare(window.connectionSheetTab, "preferences");

        var toggle = findChild(window, "connectionOpenAtUnread");
        verify(toggle !== null, "Could not find connectionOpenAtUnread");
        verify(toggle.visible);
        compare(toggle.checked, false);
        compare(liveIrc.openConversationsAtUnread, false);

        mouseClick(toggle);
        compare(toggle.checked, true);
        compare(liveIrc.openConversationsAtUnread, true);

        mouseClick(toggle);
        compare(toggle.checked, false);
        compare(liveIrc.openConversationsAtUnread, false);

        preferencesTab.forceActiveFocus();
        tryCompare(preferencesTab, "activeFocus", true);
        keyClick(Qt.Key_Tab);
        wait(0);
        compare(focusObjectName(window), "connectionReopenDirects");
        keyClick(Qt.Key_Tab);
        wait(0);
        compare(focusObjectName(window), "connectionReopenDirectsHelp");
        keyClick(Qt.Key_Tab);
        wait(0);
        compare(focusObjectName(window), "connectionLoadPeerAvatars");
        keyClick(Qt.Key_Tab);
        wait(0);
        compare(focusObjectName(window), "connectionLoadPeerAvatarsHelp");
        keyClick(Qt.Key_Tab);
        wait(0);
        compare(focusObjectName(window), "connectionOpenAtUnread");

        var checkedBefore = toggle.checked;
        keyClick(Qt.Key_Return);
        compare(toggle.checked, checkedBefore);
        compare(liveIrc.openConversationsAtUnread, checkedBefore);
        verify(findChild(window, "connectionSheet").visible);
        verify(focusObjectName(window) !== "connectionOpenAtUnread");

        window.close();
        liveIrc.openConversationsAtUnread = false;
    }

    function test_connectionSheetShortcutsHintOpensShortcuts() {
        var window = createTemporaryObject(setupWindowComponent, null);
        verify(window !== null, "The shortcuts-hint window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        var hint = findChild(window, "connectionShortcutsHint");
        verify(hint !== null, "Could not find connectionShortcutsHint");
        verify(hint.visible);
        var hintKeys = findChild(window, "connectionShortcutsHintKeys");
        verify(hintKeys !== null, "Could not find connectionShortcutsHintKeys");
        compare(hintKeys.text, shortcutCtrlLabel() + " + /");

        var shortcuts = findChild(window, "shortcutsSheet");
        verify(shortcuts !== null, "Could not find shortcutsSheet");
        mouseClick(hint);
        tryCompare(shortcuts, "opened", true);
        window.close();
    }

    function test_connectionSheetShortcutsHintIsKeyboardReachable() {
        var window = createTemporaryObject(setupWindowComponent, null);
        verify(window !== null, "The shortcuts-hint window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        var hint = findChild(window, "connectionShortcutsHint");
        verify(hint !== null, "Could not find connectionShortcutsHint");
        compare(hint.activeFocusOnTab, true);

        var shortcuts = findChild(window, "shortcutsSheet");
        hint.forceActiveFocus();
        tryCompare(hint, "activeFocus", true);

        keyClick(Qt.Key_Return);
        tryCompare(shortcuts, "opened", true);
        shortcuts.close();
        tryCompare(shortcuts, "opened", false);

        hint.forceActiveFocus();
        tryCompare(hint, "activeFocus", true);
        keyClick(Qt.Key_Space);
        tryCompare(shortcuts, "opened", true);
        shortcuts.close();
        window.close();
    }

    function test_connectionSheetDistinguishesTheTwoSecrets() {
        fakeConnection.canForgetPassword = true;
        fakeConnection.canForgetNickServ = true;
        var window = createTemporaryObject(setupWindowComponent, null);
        verify(window !== null, "The label window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        // #176: a newcomer must be able to tell the server password from the
        // NickServ password from the sheet alone, and the labels, forget links
        // and accessibility names must agree.
        var visibleLabels = [];
        var seen = [];
        function walk(node) {
            if (!node || seen.indexOf(node) !== -1)
                return;
            seen.push(node);
            if (node.text !== undefined && node.visible && node.font !== undefined)
                visibleLabels.push(String(node.text));
            var kids = node.children;
            if (kids) {
                var index = 0;
                for (; index < kids.length; ++index)
                    walk(kids[index]);
            }
            if (node.contentItem)
                walk(node.contentItem);
        }
        walk(window.contentItem);

        verify(visibleLabels.indexOf("Server password") !== -1,
               "the sheet should label the server password field");
        verify(visibleLabels.indexOf("NickServ password") !== -1,
               "the sheet should label the NickServ password field");
        verify(visibleLabels.indexOf("PASS") !== -1,
               "the server password field should carry the PASS badge");
        // The old ambiguous labels are gone.
        compare(visibleLabels.indexOf("Password"), -1);
        compare(visibleLabels.indexOf("NickServ"), -1);

        verify(visibleLabels.indexOf("forget saved server password") !== -1,
               "the server password forget link should name the server password");
        verify(visibleLabels.indexOf("forget saved NickServ password") !== -1,
               "the NickServ forget link should name the NickServ password");

        var forgetPassword = findChild(window, "connectionForgetPassword");
        var forgetNickServ = findChild(window, "connectionForgetNickServ");
        compare(forgetPassword.Accessible.name, "Forget saved server password");
        compare(forgetNickServ.Accessible.name, "Forget saved NickServ password");

        // Both secrets stay hidden, and the help matches the real protocol
        // behaviour. A server password is NOT also sent as PASS when SASL is
        // negotiated: sendRegistration() suppresses PASS whenever SASL was
        // requested and no separate NickServ password exists, so the password
        // is used as the SASL secret *instead*. See sendsPassWhenSaslIsUnavailable,
        // negotiatesSaslPlain and bothSecretsSaslSendsPassAndPlainFromNickServ.
        compare(findChild(window, "connectionPassword").echoMode, TextInput.Password);
        compare(findChild(window, "connectionNickServ").echoMode, TextInput.Password);
        compare(findChild(window, "connectionPassword").Accessible.description,
                "Used as the SASL secret when no NickServ password is set; otherwise sent as PASS while connecting.");
        compare(findChild(window, "connectionNickServ").Accessible.description,
                "Preferred SASL secret. Sent as NickServ IDENTIFY when SASL did not succeed.");

        window.close();
        fakeConnection.canForgetPassword = false;
        fakeConnection.canForgetNickServ = false;
    }

    function test_connectionPreferencesHelpIsKeyboardReachable() {
        var window = createTemporaryObject(setupWindowComponent, null);
        verify(window !== null, "The preferences-help window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        var preferencesTab = findChild(window, "connectionSheetTab-preferences");
        verify(preferencesTab !== null, "Could not find connectionSheetTab-preferences");
        mouseClick(preferencesTab);
        compare(window.connectionSheetTab, "preferences");

        var help = findChild(window, "connectionLoadPeerAvatarsHelp");
        verify(help !== null, "Could not find connectionLoadPeerAvatarsHelp");
        compare(help.activeFocusOnTab, true);
        compare(help.Accessible.name, "Show peer avatars help");

        var toggle = findChild(window, "connectionLoadPeerAvatars");
        var tip = findChild(window, "connectionLoadPeerAvatarsHelpTip");
        verify(tip !== null, "Could not find connectionLoadPeerAvatarsHelpTip");
        compare(toggle.Accessible.description,
                "Fetches IRCv3 avatar images from peer metadata. Nick initials still show when this is off or no image loads. Turn off to keep avatar hosts from seeing your IP on busy channels.");
        compare(tip.visible, false);

        help.forceActiveFocus();
        tryCompare(help, "activeFocus", true);
        tryCompare(tip, "visible", true);

        toggle.forceActiveFocus();
        tryCompare(toggle, "activeFocus", true);
        tryCompare(tip, "visible", false);

        compare(findChild(window, "connectionReopenDirectsHelp").visible, true);
        compare(findChild(window, "connectionOpenAtUnreadHelp").visible, true);
        window.close();
    }

    function test_connectionSheetHelpIsKeyboardReachable() {
        var window = createTemporaryObject(setupWindowComponent, null);
        verify(window !== null, "The help window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        var help = findChild(window, "connectionPasswordHelp");
        verify(help !== null, "Could not find connectionPasswordHelp");
        compare(help.activeFocusOnTab, true);
        compare(help.Accessible.name, "Server password help");

        // The help text is exposed to assistive tech and rendered on focus.
        compare(findChild(window, "connectionPassword").Accessible.description,
                "Used as the SASL secret when no NickServ password is set; otherwise sent as PASS while connecting.");
        var tip = findChild(window, "connectionPasswordHelpTip");
        verify(tip !== null, "Could not find connectionPasswordHelpTip");
        compare(tip.visible, false);

        help.forceActiveFocus();
        tryCompare(help, "activeFocus", true);
        tryCompare(tip, "visible", true);

        // Leaving the marker hides it again.
        var host = findChild(window, "connectionHost");
        host.forceActiveFocus();
        tryCompare(host, "activeFocus", true);
        tryCompare(tip, "visible", false);

        // Only the fields that carry help get a marker.
        compare(findChild(window, "connectionHostHelp").visible, false);
        compare(findChild(window, "connectionNickServHelp").visible, true);
        window.close();
    }

    function test_connectionSheetRailArrowNavigation() {
        restoreNamedConnection();
        namedNetworks.append({
            networkId: "oftc",
            displayName: "irc.oftc.net",
            stored: true,
            selected: false,
            iconColor: 1,
            iconUrl: "",
            collapsed: false
        });
        var window = createTemporaryObject(fallbackWindowComponent, null);
        verify(window !== null, "The rail-arrow window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        tryCompare(findChild(window, "connectionSheet"), "visible", true);
        waitForRendering(window.contentItem);

        var rail = findChild(window, "networkChoiceRepeater");
        var libera = repeaterItemByName(rail, "networkChoice-libera");
        verify(libera !== null, "Could not find networkChoice-libera");
        libera.forceActiveFocus();
        tryCompare(libera, "activeFocus", true);

        keyClick(Qt.Key_Down);
        wait(0);
        compare(focusObjectName(window), "networkChoice-oftc");

        keyClick(Qt.Key_Down);
        wait(0);
        compare(focusObjectName(window), "connectionAddNetwork");

        keyClick(Qt.Key_Up);
        wait(0);
        compare(focusObjectName(window), "networkChoice-oftc");

        keyClick(Qt.Key_Home);
        wait(0);
        compare(focusObjectName(window), "networkChoice-libera");

        keyClick(Qt.Key_End);
        wait(0);
        compare(focusObjectName(window), "connectionAddNetwork");

        // Down on the last stop stays put rather than jumping columns.
        keyClick(Qt.Key_Down);
        wait(0);
        compare(focusObjectName(window), "connectionAddNetwork");

        // Selecting with Enter still works from the keyboard.
        namedConnection.applyCalls = 0;
        libera.forceActiveFocus();
        keyClick(Qt.Key_Return);
        compare(namedConnection.applyCalls, 0);
        compare(namedConnection.selectedNetworkId, "libera");
        window.close();
        restoreNamedConnection();
    }

    function test_connectionSheetRailKeepsFocusedRowVisible() {
        restoreNamedConnection();
        var extra = 0;
        for (extra = 0; extra < 16; ++extra) {
            namedNetworks.append({
                networkId: "probe-" + extra,
                displayName: "irc.probe" + extra + ".example",
                stored: true,
                selected: false,
                iconColor: 1,
                iconUrl: "",
                collapsed: false
            });
        }
        var window = createTemporaryObject(fallbackWindowComponent, null);
        verify(window !== null, "The rail-scroll window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        tryCompare(findChild(window, "connectionSheet"), "visible", true);
        waitForRendering(window.contentItem);

        var rail = findChild(window, "networkChoiceRepeater");
        var scroll = findChild(window, "networkChoiceScroll");
        var first = repeaterItemByName(rail, "networkChoice-libera");
        var last = repeaterItemByName(rail, "networkChoice-probe-15");

        function rowIsVisible(row) {
            return row.y >= scroll.contentY
                && row.y + row.height <= scroll.contentY + scroll.height;
        }

        first.forceActiveFocus();
        tryCompare(first, "activeFocus", true);
        compare(scroll.contentY, 0);
        verify(rowIsVisible(first));

        // Focus used to land on rows that sat entirely outside the viewport.
        verify(last.y >= scroll.contentY + scroll.height);
        last.forceActiveFocus();
        tryCompare(last, "activeFocus", true);
        verify(rowIsVisible(last));
        verify(scroll.contentY > 0);

        first.forceActiveFocus();
        tryCompare(first, "activeFocus", true);
        verify(rowIsVisible(first));

        // Walking the rail with Down keeps every focused row on screen.
        first.forceActiveFocus();
        var step = 0;
        for (step = 0; step < 17; ++step) {
            keyClick(Qt.Key_Down);
            wait(0);
            var focused = window.activeFocusItem;
            if (focused && focused.objectName
                    && focused.objectName.indexOf("networkChoice-") === 0)
                verify(rowIsVisible(focused));
        }
        window.close();
        restoreNamedConnection();
    }

    function test_connectionSheetCloseRestoresComposerFocus() {
        restoreNamedConnection();
        var window = createTemporaryObject(fallbackWindowComponent, null);
        verify(window !== null, "The focus-restore window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        var sheet = findChild(window, "connectionSheet");
        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        tryCompare(sheet, "visible", true);
        waitForRendering(window.contentItem);

        var host = findChild(window, "connectionHost");
        host.forceActiveFocus();
        tryCompare(host, "activeFocus", true);
        keyClick(Qt.Key_Escape);
        tryCompare(sheet, "visible", false);

        // Focus used to be left on the window itself, so typing did nothing.
        tryCompare(findChild(window, "messageComposer"), "activeFocus", true);
        window.close();
        restoreNamedConnection();
    }

    function test_connectionDisconnectSitsBesideApply() {
        fakeConnection.applyCalls = 0;
        fakeConnection.disconnectSelectedCalls = 0;
        fakeConnection.canDisconnect = false;
        var window = createTemporaryObject(setupWindowComponent, null);
        verify(window !== null, "The setup window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        var disconnectButton = findChild(window, "connectionDisconnect");
        var applyButton = findChild(window, "connectionApply");
        verify(disconnectButton !== null, "Could not find connectionDisconnect");
        verify(applyButton !== null, "Could not find connectionApply");
        compare(disconnectButton.visible, false);
        compare(disconnectButton.parent, applyButton.parent);

        fakeConnection.canDisconnect = true;
        tryCompare(disconnectButton, "visible", true);
        waitForRendering(window.contentItem);
        compare(disconnectButton.width > 0, true);
        verify(disconnectButton.x < applyButton.x);
        verify(applyButton.x - (disconnectButton.x + disconnectButton.width)
               <= disconnectButton.parent.spacing + 1);

        mouseClick(disconnectButton);
        compare(fakeConnection.disconnectSelectedCalls, 1);
        compare(fakeConnection.applyCalls, 0);
        compare(findChild(window, "connectionSheet").visible, true);

        applyButton.forceActiveFocus();
        tryCompare(applyButton, "activeFocus", true);
        fakeConnection.canDisconnect = false;
        tryCompare(disconnectButton, "visible", false);
        window.close();
        fakeConnection.disconnectSelectedCalls = 0;
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
        tryCompare(findChild(window, "connectionName"), "activeFocus", true);

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

    function test_connectionSheetEnterFromHostAdvancesWithoutApplying() {
        fakeConnection.applyCalls = 0;
        var window = createTemporaryObject(setupWindowComponent, null);
        verify(window !== null, "The setup window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        var nameField = findChild(window, "connectionName");
        verify(nameField !== null, "Could not find connectionName");
        nameField.forceActiveFocus();
        tryCompare(nameField, "activeFocus", true);
        keyClick(Qt.Key_Return);
        compare(fakeConnection.applyCalls, 0);
        tryCompare(findChild(window, "connectionHost"), "activeFocus", true);

        var host = findChild(window, "connectionHost");
        verify(host !== null, "Could not find connectionHost");
        host.forceActiveFocus();
        tryCompare(host, "activeFocus", true);
        keyClick(Qt.Key_Return);

        // Plain Enter walks the form instead of committing it.
        compare(fakeConnection.applyCalls, 0);
        tryCompare(findChild(window, "connectionPort"), "activeFocus", true);
        verify(host.activeFocus === false);
        verify(findChild(window, "connectionSheet").visible);

        // Ctrl+Enter is the commit, and an invalid profile still refuses it.
        host.forceActiveFocus();
        tryCompare(host, "activeFocus", true);
        keyClick(Qt.Key_Return, Qt.ControlModifier);
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
        keyClick(Qt.Key_Return, Qt.ControlModifier);

        compare(namedConnection.applyCalls, 1);
        compare(sheet.visible, false);
        window.close();
        restoreNamedConnection();
    }

    function test_connectionSheetEnterFromSwitchesAdvancesWithoutApplying() {
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

        // Enter advances from a switch without flipping or committing it.
        var tls = findChild(window, "connectionTls");
        verify(tls !== null, "Could not find connectionTls");
        var tlsBefore = tls.checked;
        tls.forceActiveFocus();
        tryCompare(tls, "activeFocus", true);
        keyClick(Qt.Key_Return);
        compare(namedConnection.applyCalls, 0);
        compare(tls.checked, tlsBefore);
        tryCompare(findChild(window, "connectionNick"), "activeFocus", true);
        verify(sheet.visible);

        // Ctrl+Enter commits from a switch.
        tls.forceActiveFocus();
        tryCompare(tls, "activeFocus", true);
        keyClick(Qt.Key_Return, Qt.ControlModifier);
        compare(namedConnection.applyCalls, 1);
        compare(sheet.visible, false);

        namedConnection.applyCalls = 0;
        keyClick(Qt.Key_Comma, Qt.ControlModifier);
        tryCompare(sheet, "visible", true);
        var startup = findChild(window, "connectionConnectOnStartup");
        verify(startup !== null, "Could not find connectionConnectOnStartup");
        startup.forceActiveFocus();
        tryCompare(startup, "activeFocus", true);
        keyClick(Qt.Key_Return, Qt.ControlModifier);
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

        // Plain Enter steps out of the empty nick field without committing.
        keyClick(Qt.Key_Return);
        compare(fakeConnection.applyCalls, 0);
        compare(fakeConnection.selectedNetworkId, "setup-id");
        verify(findChild(window, "connectionSheet").visible);
        tryCompare(findChild(window, "connectionUsername"), "activeFocus", true);

        // Ctrl+Enter still reports the missing nick and keeps the sheet open.
        nick.forceActiveFocus();
        tryCompare(nick, "activeFocus", true);
        keyClick(Qt.Key_Return, Qt.ControlModifier);
        compare(fakeConnection.applyCalls, 0);
        compare(findChild(window, "connectionProblem").text, "Nick is required");
        verify(findChild(window, "connectionProblem").visible);
        verify(findChild(window, "connectionSheet").visible);
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
            iconColor: 1,
            iconUrl: "",
            collapsed: false
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

        var name = findChild(window, "connectionName");
        verify(name !== null, "Could not find connectionName");
        tryCompare(name, "activeFocus", true);
        compare(focusObjectName(window), "connectionName");
        compare(namedConnection.selectedNetworkId, "libera");

        keyClick(Qt.Key_Return, Qt.ControlModifier);
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
            iconColor: 1,
            iconUrl: "",
            collapsed: false
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
        compare(findChild(window, "connectionName").text, "");

        mouseClick(discardButton);
        compare(namedConnection.discardCalls, 1);
        compare(namedNetworks.count, 2);
        compare(namedConnection.selectedNetworkId, "libera");
        compare(namedConnection.host, "irc.libera.chat");
        compare(findChild(window, "connectionHost").text, "irc.libera.chat");
        compare(findChild(window, "connectionName").text, "irc.libera.chat");
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

        var card = findChild(window, "connectionSheetCard");
        verify(card !== null, "Could not find connectionSheetCard");
        var widthBefore = card.width;

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
        compare(card.width, widthBefore);

        var sheet = findChild(window, "connectionSheet");
        var host = findChild(window, "connectionHost");
        verify(host !== null, "Could not find connectionHost");
        verify(host.mapToItem(sheet, host.width, 0).x <= bar.mapToItem(sheet, 0, 0).x);
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
                iconColor: 1,
                iconUrl: "",
                collapsed: false
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
        var names = [];
        var seen = [];
        function walk(node) {
            if (!node || seen.indexOf(node) !== -1)
                return;
            seen.push(node);
            if (node.direct === true && node.visible && node.height > 0
                    && node.conversationName)
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
        walk(window.contentItem);
        return names;
    }

    function test_liveIrcWithoutConnectionHasNoSidebarConversations() {
        var window = createTemporaryObject(windowComponent, null);
        verify(window !== null, "A live window without connection should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        compare(window.sidebarConversationRows().length, 0);
        compare(findChild(window, "liveNetworkRepeater").count, 0);
        compare(findChild(window, "conversation-#omarchy"), null);
        compare(findChild(window, "conversation-libera-#omarchy"), null);
        window.close();
    }

    function test_nullIrcHasNoSidebarConversations() {
        var window = createTemporaryObject(nullIrcWindowComponent, null);
        verify(window !== null, "A window with irc null should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        compare(window.sidebarConversationRows().length, 0);
        compare(findChild(window, "liveNetworkRepeater").count, 0);
        compare(findChild(window, "conversation-#omarchy"), null);
        compare(findChild(window, "conversation-libera-#omarchy"), null);
        window.close();
    }

    function test_sidebarRowsComeFromTheConversationModel() {
        restoreNamedConnection();
        var window = createTemporaryObject(liveWindowComponent, null);
        verify(window !== null, "The live window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        var rows = window.sidebarConversationRows();
        compare(rows.length, 2);
        compare(typeof rows[0].activate, "undefined");
        compare(rows[0].conversationName, "#omarchy");
        compare(rows[0].direct, false);
        compare(rows[1].conversationName, "anna");
        compare(rows[1].direct, true);
        compare(window.sectionHasDirects("libera"), true);
        compare(window.sectionHasDirects("oftc"), false);

        var sections = window.sidebarNetworkSections();
        compare(sections.length, 1);
        compare(typeof sections[0].headerItem, "undefined");
        compare(sections[0].networkId, "libera");
        window.close();
    }

    function test_liveSidebarClickSwitchesChannel() {
        var window = createTemporaryObject(liveWindowComponent, null);
        verify(window !== null, "The live window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        compare(window.currentConversation, "#omarchy");
        var channel = findNamedIn(window, "conversation-libera-#omarchy");
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

    function test_liveDmTypingIndicatorUsesProductionDelegate() {
        var window = createTemporaryObject(liveWindowComponent, null);
        verify(window !== null, "The live window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        var networks = findChild(window, "liveNetworkRepeater");
        verify(networks !== null, "The live network repeater should exist");
        var network = networks.itemAt(0);
        verify(network !== null, "The live network section should be rendered");
        var dms = findChild(network, "directConversationRepeater");
        verify(dms !== null, "The live direct-message repeater should exist");
        compare(dms.count, 2);
        var row = dms.itemAt(1);
        verify(row !== null, "The live DM row should be rendered");
        compare(row.conversationName, "anna");
        tryCompare(row, "visible", true);
        var dots = findChild(row, "conversation-typing-libera-anna");
        verify(dots !== null, "The live DM typing indicator should exist");
        tryCompare(dots, "visible", true);
        window.close();
    }

    function test_liveDmPresenceFollowsAwayFacts() {
        liveConversations.setProperty(1, "presence", "online");
        var window = createTemporaryObject(liveWindowComponent, null);
        verify(window !== null, "The live window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        var networks = findChild(window, "liveNetworkRepeater");
        verify(networks !== null, "The live network repeater should exist");
        var network = networks.itemAt(0);
        var dms = findChild(network, "directConversationRepeater");
        verify(dms !== null, "The live direct-message repeater should exist");
        var row = dms.itemAt(1);
        verify(row !== null, "The live DM row should be rendered");
        compare(row.conversationName, "anna");
        var dot = findChild(row, "conversation-presence-libera-anna");
        verify(dot !== null, "The DM presence dot should be rendered");
        compare(dot.visible, true);
        verify(Qt.colorEqual(dot.color, "#69b978"));

        liveConversations.setProperty(1, "presence", "away");
        waitForRendering(window.contentItem);
        verify(Qt.colorEqual(dot.color, "#d6a552"));

        // A peer we cannot place in a shared channel reads offline, not online.
        liveConversations.setProperty(1, "presence", "offline");
        waitForRendering(window.contentItem);
        verify(Qt.colorEqual(dot.color, window.mutedColor));
        compare(dot.visible, true);

        // Other people's away state needs away-notify, like member rows.
        liveIrc.hasAwayPresence = false;
        waitForRendering(window.contentItem);
        compare(dot.visible, false);
        liveIrc.hasAwayPresence = true;

        liveConversations.setProperty(1, "presence", "online");
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

    function test_memberBotMarkShowsForDemoDaxNotAnna() {
        openSeededAppWindow();
        var members = item("membersList");
        verify(item("membersPanel").visible);
        var dax = members.itemAtIndex(memberIndex("dax"));
        verify(dax !== null, "The dax member delegate should be rendered");
        compare(dax.bot, true);
        var daxBot = findChild(dax, "member-bot-dax");
        verify(daxBot !== null, "dax should have a bot mark");
        compare(daxBot.visible, true);
        compare(daxBot.shown, true);
        var daxLabel = null;
        var kids = dax.children;
        // Prefer the nick label that sits in the same Row as the bot mark.
        function findLabelNearBot(node) {
            if (!node || !node.children)
                return null;
            for (var i = 0; i < node.children.length; ++i) {
                var child = node.children[i];
                if (child === daxBot)
                    continue;
                if (child.text === dax.label)
                    return child;
                var nested = findLabelNearBot(child);
                if (nested)
                    return nested;
            }
            return null;
        }
        daxLabel = findLabelNearBot(dax);
        verify(daxLabel !== null, "dax nick label should exist");
        // Bot mark sits beside the nick, not at the far right of the row.
        compare(daxBot.x < daxLabel.x + daxLabel.width + appWindow.scaledSize(12), true);
        compare(daxBot.x > daxLabel.x + daxLabel.width - 2, true);

        var anna = members.itemAtIndex(memberIndex("anna"));
        verify(anna !== null, "The anna member delegate should be rendered");
        compare(anna.bot, false);
        compare(anna.away, false);
        var annaBot = findChild(anna, "member-bot-anna");
        verify(annaBot !== null, "anna should still instantiate a hidden bot mark");
        compare(annaBot.visible, false);
        var annaStatus = findChild(anna, "member-status-anna");
        verify(annaStatus !== null, "anna status line should exist");
        compare(annaStatus.visible, true);
        compare(annaStatus.text, "writing docs");
        compare(anna.Accessible.description, "writing docs");
    }

    function test_demoMiraAvatarReachesImageReady() {
        openSeededAppWindow();
        function findPhoto(node) {
            if (!node)
                return null;
            if (node.objectName === "nickGlyphPhoto")
                return node;
            var children = node.children || [];
            for (var i = 0; i < children.length; ++i) {
                var found = findPhoto(children[i]);
                if (found)
                    return found;
            }
            return null;
        }
        function assertDemoAvatarReady(nick, url) {
            var members = item("membersList");
            var row = members.itemAtIndex(memberIndex(nick));
            verify(row !== null, nick + " member row should render");
            compare(row.avatar, url);
            var photo = findPhoto(row);
            verify(photo !== null, nick + " glyph photo should exist");
            tryCompare(photo, "status", Image.Ready);
            tryCompare(photo, "visible", true);
        }
        assertDemoAvatarReady("mira", "qrc:/demo/mira-avatar.png");
        assertDemoAvatarReady("anna", "qrc:/demo/anna-avatar.png");
        assertDemoAvatarReady("kai", "qrc:/demo/kai-avatar.png");
    }

    function test_demoOmarchyNetworkIconReachesImageReady() {
        openSeededAppWindow();
        var omarchyPhoto = namedItem("networkIconPhoto-" + seed.omarchyNetworkId);
        verify(omarchyPhoto !== null, "omarchy network icon photo should exist");
        tryCompare(omarchyPhoto, "status", Image.Ready);
        tryCompare(omarchyPhoto, "visible", true);
        compare(String(omarchyPhoto.source).indexOf("image://omairc-avatar/square/"), 0);
        var omarchyInitial = namedItem("networkIconInitial-" + seed.omarchyNetworkId);
        verify(omarchyInitial !== null, "omarchy network icon initial should exist");
        compare(omarchyInitial.visible, false);

        var oftcPhoto = namedItem("networkIconPhoto-" + seed.oftcNetworkId);
        verify(oftcPhoto !== null, "OFTC network icon photo should exist");
        compare(oftcPhoto.status === Image.Ready, false);
        compare(oftcPhoto.visible, false);
        var oftcInitial = namedItem("networkIconInitial-" + seed.oftcNetworkId);
        verify(oftcInitial !== null, "OFTC network icon initial should exist");
        compare(oftcInitial.visible, true);
    }

    function test_mockNetworkIconWithoutTokenKeepsInitial() {
        restoreNamedConnection();
        var window = createTemporaryObject(liveNamedWindowComponent, null);
        verify(window !== null, "The mock network window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        var photo = findNamedIn(window, "networkIconPhoto-libera");
        verify(photo !== null, "mock network icon photo should exist");
        compare(photo.status === Image.Ready, false);
        compare(photo.visible, false);
        var initial = findNamedIn(window, "networkIconInitial-libera");
        verify(initial !== null, "mock network icon initial should exist");
        compare(initial.visible, true);
        window.close();
        restoreNamedConnection();
    }

    function test_loadPeerAvatarsPreferenceGatesStoreSource() {
        openSeededAppWindow();
        compare(seed.irc.loadPeerAvatars, true);
        var mira = item("membersList").itemAtIndex(memberIndex("mira"));
        var glyph = null;
        function findGlyph(node) {
            if (!node)
                return null;
            if (node.avatarUrl === "qrc:/demo/mira-avatar.png")
                return node;
            var children = node.children || [];
            for (var i = 0; i < children.length; ++i) {
                var found = findGlyph(children[i]);
                if (found)
                    return found;
            }
            return null;
        }
        glyph = findGlyph(mira);
        verify(glyph !== null, "mira NickGlyph should bind the bundled avatar");
        tryVerify(function() {
            return glyph.storeSource.indexOf("image://omairc-avatar/") === 0;
        });
        seed.irc.loadPeerAvatars = false;
        tryCompare(glyph, "storeSource", "");
        seed.irc.loadPeerAvatars = true;
        tryVerify(function() {
            return glyph.storeSource.indexOf("image://omairc-avatar/") === 0;
        });
    }

    function test_memberBotMarkAppearsAfterLateMetadata() {
        openSeededAppWindow();
        var members = item("membersList");
        var anna = members.itemAtIndex(memberIndex("anna"));
        verify(anna !== null, "The anna member delegate should be rendered");
        compare(anna.bot, false);
        var annaBot = findChild(anna, "member-bot-anna");
        compare(annaBot.visible, false);

        seed.injectOmarchy(":server 761 fred anna bot * :PacketBot\r\n");
        tryCompare(anna, "bot", true);
        tryCompare(annaBot, "shown", true);
        tryCompare(annaBot, "visible", true);
    }

    function test_identityFooterBotAndAvatarRefreshAfterSelfMetadata() {
        openSeededAppWindow();
        var selfBot = item("selfBotMark");
        compare(selfBot.shown, false);
        compare(selfBot.visible, false);
        compare(appWindow.peerBot(appWindow.selfNick), false);
        compare(appWindow.peerAvatar(appWindow.selfNick), "");

        seed.injectOmarchy(":server 761 fred fred bot * :ReviewBot\r\n");
        tryCompare(selfBot, "shown", true);
        tryCompare(selfBot, "visible", true);
        tryVerify(function() {
            return appWindow.peerBot(appWindow.selfNick) === true;
        });

        seed.injectOmarchy(
            ":server 761 fred fred avatar * :https://example.com/self.png\r\n");
        tryVerify(function() {
            return appWindow.peerAvatar(appWindow.selfNick)
                === "https://example.com/self.png";
        });
        var selfGlyph = item("selfNickGlyph");
        tryCompare(selfGlyph, "avatarUrl", "https://example.com/self.png");
    }

    function test_transcriptBotAndAvatarRefreshAfterLateMetadata() {
        openSeededAppWindow();
        // nora has no seeded avatar/bot — unlike anna/mira/kai.
        injectOmarchyChat("nora", "#omarchy", "late-meta-chrome", "12:30");
        var row = renderedRowWithBody("late-meta-chrome");
        compare(row.author, "nora");
        compare(row.authorBot, false);
        compare(row.authorAvatar, "");
        var bot = findChild(row, "message-bot-nora");
        verify(bot !== null, "Transcript bot mark should exist for nora");
        compare(bot.shown, false);

        seed.injectOmarchy(
            ":server 761 fred nora bot * :PacketBot\r\n"
            + ":server 761 fred nora avatar * :https://example.com/nora.png\r\n");
        tryCompare(row, "authorBot", true);
        tryCompare(row, "authorAvatar", "https://example.com/nora.png");
        tryCompare(bot, "shown", true);
        tryCompare(bot, "visible", true);
    }

    function mentionWashAt(row) {
        var wash = findChild(row, "mentionWash");
        verify(wash !== null, "Could not find mentionWash");
        return wash;
    }

    function visualChildIndex(row, objectName) {
        var children = row.children;
        var index = 0;
        for (; index < children.length; ++index) {
            if (children[index].objectName === objectName)
                return index;
        }
        return -1;
    }

    function test_transcriptMentionWashForNickAndHighlightWords() {
        openSeededAppWindow();
        injectOmarchyChat("anna", "#omarchy", "mention-wash-plain-zx9");
        var plain = renderedRowWithBody("mention-wash-plain-zx9");
        compare(plain.mentioned, false);
        compare(mentionWashAt(plain).visible, false);

        injectOmarchyChat("anna", "#omarchy", "mention-wash-nick-zx9 fred please");
        var nickHit = renderedRowWithBody("mention-wash-nick-zx9 fred please");
        compare(nickHit.mentioned, true);
        compare(mentionWashAt(nickHit).visible, true);

        injectOmarchyChat("fred", "#omarchy", "mention-wash-self-zx9 fred");
        var selfHit = renderedRowWithBody("mention-wash-self-zx9 fred");
        compare(selfHit.mentioned, false);
        compare(mentionWashAt(selfHit).visible, false);

        injectOmarchyChat("anna", "#omarchy", "mention-wash-deploy-zx9 please review deploy");
        var deploy = renderedRowWithBody("mention-wash-deploy-zx9 please review deploy");
        compare(deploy.mentioned, false);
        compare(mentionWashAt(deploy).visible, false);

        verify(appWindow.irc.sendMessage("/highlight deploy"));
        deploy = renderedRowWithBody("mention-wash-deploy-zx9 please review deploy");
        tryCompare(deploy, "mentioned", true);
        tryCompare(mentionWashAt(deploy), "visible", true);
        plain = renderedRowWithBody("mention-wash-plain-zx9");
        compare(plain.mentioned, false);
        compare(mentionWashAt(plain).visible, false);

        injectOmarchyChat("dax", "#omarchy", "mention-wash-after-zx9 still plain");
        var after = renderedRowWithBody("mention-wash-after-zx9 still plain");
        compare(after.mentioned, false);
        compare(mentionWashAt(after).visible, false);

        verify(appWindow.irc.sendMessage("/unhighlight deploy"));
        deploy = renderedRowWithBody("mention-wash-deploy-zx9 please review deploy");
        tryCompare(deploy, "mentioned", false);
        tryCompare(mentionWashAt(deploy), "visible", false);

        verify(appWindow.irc.sendMessage("/mute #omarchy"));
        tryCompare(namedItem(liveConversation("#omarchy")), "muted", true);
        injectOmarchyChat("anna", "#omarchy", "mention-wash-muted-zx9 fred please");
        var mutedHit = renderedRowWithBody("mention-wash-muted-zx9 fred please");
        compare(mutedHit.mentioned, true);
        compare(mentionWashAt(mutedHit).visible, true);

        keyClick(Qt.Key_F, Qt.ControlModifier);
        tryCompare(appWindow, "findActive", true);
        typeText("mention-wash-muted-zx9");
        tryVerify(function() {
            return appWindow.findIndex >= 0
                && field(item("messageList").model, appWindow.findIndex, "body")
                    === "mention-wash-muted-zx9 fred please";
        }, 1000, "Find should land on the muted mention row");
        var findRow = renderedRowWithBody("mention-wash-muted-zx9 fred please");
        var findMark = findChild(findRow, "findMatch");
        verify(findMark !== null && findMark.visible,
               "findMatch should be visible on the mentioned row");
        compare(mentionWashAt(findRow).visible, true);
        var washIndex = visualChildIndex(findRow, "mentionWash");
        var matchIndex = visualChildIndex(findRow, "findMatch");
        verify(washIndex >= 0, "mentionWash should be a visual child of the row");
        verify(matchIndex >= 0, "findMatch should be a visual child of the row");
        verify(matchIndex > washIndex,
               "findMatch should paint on top of mentionWash");
        keyClick(Qt.Key_Escape);
        tryCompare(appWindow, "findActive", false);

        mouseClick(namedItem(liveConversation("anna")));
        tryCompare(appWindow, "currentConversation", "anna");
        injectOmarchyChat("anna", "fred", "mention-wash-dm-plain-zx9");
        var dmPlain = renderedRowWithBody("mention-wash-dm-plain-zx9");
        compare(dmPlain.mentioned, false);
        compare(mentionWashAt(dmPlain).visible, false);

        injectOmarchyChat("anna", "fred", "mention-wash-dm-nick-zx9 fred hi");
        var dmNick = renderedRowWithBody("mention-wash-dm-nick-zx9 fred hi");
        compare(dmNick.mentioned, true);
        compare(mentionWashAt(dmNick).visible, true);

        selectOmarchy();
    }

    function test_typingTranscriptBotRefreshesAfterLateMetadata() {
        openSeededAppWindow();
        mouseClick(namedItem(liveConversation("anna")));
        tryCompare(appWindow, "currentConversation", "anna");

        var footer = item("typingTranscript");
        tryCompare(footer, "visible", true);
        tryCompare(footer, "grouped", false);
        var header = findChild(footer, "typingTranscriptHeader");
        verify(header !== null, "The typing header should exist");
        compare(header.bot, false);
        var bot = findChild(header, "message-bot-anna");
        verify(bot !== null, "Typing header should host a bot mark");
        compare(bot.shown, false);
        compare(appWindow.peerBot("anna"), false);

        seed.injectOmarchy(":server 761 fred anna bot * :PacketBot\r\n");
        tryVerify(function() {
            return appWindow.peerBot("anna") === true;
        });
        tryCompare(header, "bot", true);
        tryCompare(bot, "shown", true);
        tryCompare(bot, "visible", true);
    }

    function test_dmSidebarBotMarkAppearsAfterLateMetadata() {
        openSeededAppWindow();
        var annaRow = namedItem(liveConversation("anna"));
        verify(annaRow !== null, "Seeded anna DM row should exist");
        compare(annaRow.bot, false);
        var bot = findChild(annaRow, "conversation-bot-anna");
        verify(bot !== null, "DM sidebar bot mark should exist");
        compare(bot.shown, false);

        seed.injectOmarchy(":server 761 fred anna bot * :PacketBot\r\n");
        tryCompare(annaRow, "bot", true);
        tryCompare(bot, "shown", true);
        tryCompare(bot, "visible", true);
    }

    function test_memberPresenceShowsOurOwnAwayWithoutAwayNotify() {
        gatedIrc.members = selfAwayMembers;
        gatedIrc.peopleCount = selfAwayMembers.count;
        gatedIrc.hasAwayPresence = false;
        var window = createTemporaryObject(gatedWindowComponent, null);
        verify(window !== null, "The self-away window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        var members = findChild(window, "membersList");
        verify(members !== null, "Could not find membersList");
        var selfRow = members.itemAtIndex(0);
        verify(selfRow !== null, "The self member delegate should be rendered");
        compare(selfRow.nick, "live-nick");
        compare(selfRow.away, true);
        var selfDot = findChild(selfRow, "presence-dot-live-nick");
        verify(selfDot !== null, "The self presence dot should be rendered");
        compare(selfDot.visible, true);
        verify(Qt.colorEqual(selfDot.color, "#d6a552"));

        // away-notify is off, so another member's away state stays unpainted.
        var otherRow = members.itemAtIndex(1);
        verify(otherRow !== null, "The other member delegate should be rendered");
        compare(otherRow.nick, "anna");
        compare(otherRow.away, false);
        var otherDot = findChild(otherRow, "presence-dot-anna");
        verify(otherDot !== null, "The other presence dot should be rendered");
        compare(otherDot.visible, false);

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
        var overlay = findChild(window, "typingTranscript");
        verify(overlay !== null, "The transcript typing footer should exist");
        tryCompare(overlay, "visible", true);
        gatedIrc.hasTyping = false;
        tryCompare(overlay, "visible", false);
        window.close();
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
        var overlay = item("typingTranscript");
        tryCompare(overlay, "visible", true);
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
        compare(dots.pixelSize, appWindow.scaledSize(12));
        saveScreenshot("typing-sidebar-dm");
        var pulse = dots.pulse;
        wait(320);
        tryCompare(dots, "pulse", (pulse + 1) % 3);
    }

    function test_typingIndicatorRidesTheTranscriptEnd() {
        openSeededAppWindow();
        mouseClick(namedItem(liveConversation("anna")));
        tryCompare(appWindow, "currentConversation", "anna");

        var footer = item("typingTranscript");
        tryCompare(footer, "visible", true);
        var list = item("messageList");
        var grouped = appWindow.continuesMessageGroup(
            list.model, list.model.rowCount(), "anna",
            appWindow.currentTranscriptMinute(), "message", "live");
        compare(footer.grouped, grouped);
        var avatar = findChild(footer, "typingTranscriptAvatar");
        verify(avatar !== null, "The typing avatar should exist");
        compare(avatar.visible, !grouped);
        var header = findChild(footer, "typingTranscriptHeader");
        verify(header !== null, "The typing header should exist");
        compare(header.visible, !grouped);
        var dots = findChild(footer, "typingTranscriptDots");
        verify(dots !== null, "The typing dots should exist");
        compare(dots.anchors.topMargin,
                appWindow.bodyTextTopMargin(grouped)
                    + Math.round((appWindow.messageLineHeight - dots.implicitHeight) / 2));
        compare(dots.pixelSize, appWindow.scaledSize(16));
        compare(footer.height, appWindow.transcriptRowHeight(
                    false, grouped, appWindow.messageLineHeight));
        compare(dots.Accessible.role, Accessible.StaticText);
        compare(dots.Accessible.name, "anna is typing");
        compare(dots.Accessible.ignored, false);
        compare(dots.children[0].color.toString(), appWindow.mutedColor.toString());
    }

    function test_typingIndicatorMovesBelowMyOwnMessage() {
        openSeededAppWindow();
        mouseClick(namedItem(liveConversation("anna")));
        tryCompare(appWindow, "currentConversation", "anna");

        var footer = item("typingTranscript");
        tryCompare(footer, "visible", true);

        var composer = item("messageComposer");
        var messages = item("messageList");
        compare(footer.grouped, appWindow.continuesMessageGroup(
                    messages.model, messages.model.rowCount(), "anna",
                    appWindow.currentTranscriptMinute(), "message", "live"));
        var previousCount = messages.model.rowCount();
        mouseClick(composer);
        typeText("hello anna");
        keyClick(Qt.Key_Return);
        verify(seed.echoLastOmarchyPrivmsg());
        waitForNewMessage(messages, previousCount, "hello anna");

        tryCompare(footer, "grouped", false);
        var avatar = findChild(footer, "typingTranscriptAvatar");
        var header = findChild(footer, "typingTranscriptHeader");
        compare(avatar.visible, true);
        compare(header.visible, true);
        var avatarHit = findChild(avatar, "transcriptNickHit");
        var headerHit = findChild(header, "transcriptNickHit");
        verify(avatarHit !== null, "Could not find avatar transcriptNickHit");
        verify(headerHit !== null, "Could not find header transcriptNickHit");
        compare(avatarHit.enabled, false);
        compare(headerHit.enabled, false);
        compare(avatarHit.Accessible.ignored, true);
        compare(headerHit.Accessible.ignored, true);
        var dots = findChild(footer, "typingTranscriptDots");
        compare(dots.anchors.topMargin,
                appWindow.bodyTextTopMargin(false)
                    + Math.round((appWindow.messageLineHeight - dots.implicitHeight) / 2));
        compare(findChild(footer, "messageAuthor").text, "anna");
        tryCompare(footer, "visible", true);
    }

    function test_typingIndicatorKeepsScrollWhileReadingHistory() {
        openSeededAppWindow();
        mouseClick(namedItem(liveConversation("dax")));
        tryCompare(appWindow, "currentConversation", "dax");

        var list = item("messageList");
        var composer = item("messageComposer");
        mouseClick(composer);
        verify(composer.activeFocus);
        var start = list.model.rowCount();
        var index = 0;
        for (index = 0; index < 40; ++index) {
            var minute = index < 10 ? "0" + index : "" + index;
            injectOmarchyChat("dax", "fred", "scroll line " + index, "11:" + minute);
        }
        tryVerify(function() {
            return list.model.rowCount() > start;
        });
        waitForRendering(appWindow.contentItem);
        list.pinToEnd();
        waitForRendering(appWindow.contentItem);
        wait(0);
        verify(list.contentHeight > list.height);

        keyClick(Qt.Key_PageUp);
        waitForRendering(appWindow.contentItem);
        tryCompare(list, "stick", 1);
        verify(!transcriptPinned(list));

        var frozenY = list.contentY;
        seed.injectOmarchy("@+typing=active :dax!u@h TAGMSG fred\r\n");
        var footer = item("typingTranscript");
        tryCompare(footer, "visible", true);
        wait(0);

        fuzzyCompare(list.contentY, frozenY, 2);
        compare(list.stick, list.stickDetached);
    }

    function test_typingIndicatorFollowsTheEndWhileFollowing() {
        openSeededAppWindow();
        mouseClick(namedItem(liveConversation("anna")));
        tryCompare(appWindow, "currentConversation", "anna");

        var list = item("messageList");
        var footer = item("typingTranscript");
        tryCompare(footer, "visible", true);

        var start = list.model.rowCount();
        var index = 0;
        for (index = 0; index < 40; ++index)
            injectOmarchyChat("anna", "fred", "follow filler " + index);
        tryVerify(function() {
            return list.model.rowCount() > start + 30;
        });
        seed.injectOmarchy("@+typing=done :anna!u@h TAGMSG fred\r\n");
        tryCompare(footer, "visible", false);

        list.pinToEnd();
        waitForRendering(appWindow.contentItem);
        wait(0);
        verify(list.contentHeight > list.height);

        seed.injectOmarchy("@+typing=active :anna!u@h TAGMSG fred\r\n");
        tryCompare(footer, "visible", true);
        waitForRendering(appWindow.contentItem);

        tryCompare(list, "atYEnd", true);
        fuzzyCompare(list.contentY, list.endContentY(), 2);
    }

    function test_typingIndicatorSlotMatchesTheRowThatReplacesIt() {
        liveConsole.open = false;
        liveMessages.clear();
        liveIrc.selectedTarget = "anna";
        liveIrc.isChannel = false;
        liveIrc.hasTyping = true;
        liveIrc.typingNicks = ["anna"];
        var window = createTemporaryObject(liveWindowComponent, null);
        verify(window !== null, "The live window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        var list = findChild(window, "messageList");
        verify(list !== null, "Could not find messageList");
        var footer = findChild(window, "typingTranscript");
        verify(footer !== null, "Could not find typingTranscript");

        var nowMinute = window.currentTranscriptMinute();
        var otherMinute = nowMinute === "10:00" ? "09:00" : "10:00";

        liveMessages.append({
            author: "anna",
            time: nowMinute,
            body: "first",
            kind: "message",
            origin: "live"
        });
        tryCompare(footer, "visible", true);

        compare(footer.grouped, true);
        var groupedFooterHeight = footer.height;
        liveMessages.append({
            author: "anna",
            time: nowMinute,
            body: "second",
            kind: "message",
            origin: "live"
        });
        tryVerify(function() {
            return list.itemAtIndex(1) !== null;
        });
        waitForRendering(window.contentItem);
        var groupedRow = list.itemAtIndex(1);
        verify(groupedRow !== null, "The arriving grouped row should be rendered");
        compare(groupedRow.height, groupedFooterHeight);

        liveMessages.append({
            author: "live-nick",
            time: "10:01",
            body: "mine",
            kind: "message",
            origin: "live"
        });
        tryCompare(footer, "grouped", false);
        var ungroupedAfterSelfHeight = footer.height;
        liveMessages.append({
            author: "anna",
            time: "10:02",
            body: "third",
            kind: "message",
            origin: "live"
        });
        tryVerify(function() {
            return list.itemAtIndex(3) !== null;
        });
        waitForRendering(window.contentItem);
        var avatarRow = list.itemAtIndex(3);
        verify(avatarRow !== null, "The arriving avatar row should be rendered");
        compare(avatarRow.height, ungroupedAfterSelfHeight);

        liveMessages.clear();
        liveMessages.append({
            author: "anna",
            time: otherMinute,
            body: "older minute",
            kind: "message",
            origin: "live"
        });
        tryCompare(footer, "grouped", false);
        var reservedHeight = footer.height;
        liveMessages.append({
            author: "anna",
            time: nowMinute,
            body: "later minute",
            kind: "message",
            origin: "live"
        });
        tryVerify(function() {
            return list.itemAtIndex(1) !== null;
        });
        waitForRendering(window.contentItem);
        var laterRow = list.itemAtIndex(1);
        verify(laterRow !== null, "The arriving ungrouped row should be rendered");
        compare(laterRow.grouped, false);
        compare(laterRow.height, reservedHeight);

        window.close();
        liveIrc.hasTyping = false;
        liveIrc.typingNicks = [];
        liveIrc.isChannel = true;
        liveIrc.selectedTarget = "#omarchy";
        liveMessages.clear();
        liveConsole.open = false;
    }

    function test_typingIndicatorGoesUngroupedForANonChatLastRow() {
        liveConsole.open = false;
        liveMessages.clear();
        liveIrc.selectedTarget = "anna";
        liveIrc.isChannel = false;
        liveIrc.hasTyping = true;
        liveIrc.typingNicks = ["anna"];
        var window = createTemporaryObject(liveWindowComponent, null);
        verify(window !== null, "The live window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        var footer = findChild(window, "typingTranscript");
        verify(footer !== null, "Could not find typingTranscript");
        var avatar = findChild(footer, "typingTranscriptAvatar");
        verify(avatar !== null, "The typing avatar should exist");
        var list = findChild(window, "messageList");
        verify(list !== null, "Could not find messageList");
        tryCompare(footer, "visible", true);

        liveMessages.append({
            author: "",
            time: "",
            body: "2026-09-12",
            kind: "event",
            origin: "live"
        });
        tryCompare(footer, "grouped", false);
        compare(avatar.visible, true);

        liveMessages.clear();
        liveMessages.append({
            author: "",
            time: "",
            body: "anna is on #omarchy",
            kind: "whois",
            origin: "live"
        });
        tryCompare(footer, "grouped", false);
        compare(avatar.visible, true);

        liveMessages.clear();
        liveMessages.append({
            author: "anna",
            time: window.currentTranscriptMinute(),
            body: "live tail",
            kind: "message",
            origin: "live"
        });
        tryCompare(footer, "grouped", true);
        var revision = list.rowRevision;
        liveMessages.setProperty(0, "origin", "replay");
        compare(list.rowRevision, revision + 1);
        tryCompare(footer, "grouped", false);
        compare(avatar.visible, true);

        window.close();
        liveIrc.hasTyping = false;
        liveIrc.typingNicks = [];
        liveIrc.isChannel = true;
        liveIrc.selectedTarget = "#omarchy";
        liveMessages.clear();
        liveConsole.open = false;
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

        var header = findNamedIn(window, "networkHeaderButton-libera");
        verify(header !== null, "Could not find networkHeaderButton-libera");
        mouseClick(header);

        tryCompare(window, "consoleVisible", true);
        compare(window.title, "irc.libera.chat Status");
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

    function test_identityFooterShowsAppVersion() {
        openSeededAppWindow();
        verify(appWindow.appVersion.length > 0);
        var version = item("selfVersionLabel");
        compare(version.text, appWindow.appVersion);
        verify(version.visible);
        var nick = item("selfNickLabel");
        var nickRight = nick.mapToItem(appWindow.contentItem, nick.width, 0).x;
        var versionLeft = version.mapToItem(appWindow.contentItem, 0, 0).x;
        verify(versionLeft >= nickRight);
    }

    function test_aboutSheetOpensFromVersionAndEscapeKeepsConversation() {
        openSeededAppWindow();
        var sheet = item("aboutSheet");
        verify(!sheet.opened);
        verify(!sheet.visible);

        mouseClick(item("selfVersionHit"));
        tryCompare(sheet, "opened", true);
        compare(item("aboutTitle").text, "About Omairc");
        compare(item("aboutName").text, "Omairc");
        compare(item("aboutVersion").text, appWindow.appVersion);
        verify(String(item("aboutLogo").source).indexOf("icons/omairc.svg") >= 0);
        tryCompare(item("aboutLogo"), "status", Image.Ready);
        verify(item("aboutDescription").text.indexOf("open-source") === -1);
        compare(item("aboutOpenSource").text, "This project is open-source.");
        compare(item("aboutGithubLink").text, "View the source on GitHub");
        compare(item("aboutCopyright").text, "Copyright © 2026 Fredi Machado");
        verify(item("aboutCheckUpdates").visible);
        verify(item("aboutOk").visible);
        verify(!item("aboutUpdateStatus").visible);
        compare(sheet.width, appWindow.width);
        compare(sheet.height, appWindow.height);
        fuzzyCompare(sheet.color.a, 0.5, 0.01);
        verify(item("aboutSheetCard").visible);
        saveScreenshot("about-sheet");

        keyClick(Qt.Key_Escape);
        tryCompare(sheet, "opened", false);
        compare(appWindow.currentConversation, "#omarchy");
    }

    function test_aboutSheetBlocksWindowShortcuts() {
        openSeededAppWindow();
        var sheet = item("aboutSheet");
        mouseClick(item("selfVersionHit"));
        tryCompare(sheet, "opened", true);

        keyClick(Qt.Key_Down, Qt.AltModifier);
        compare(appWindow.currentConversation, "#omarchy");
        verify(sheet.opened);

        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        compare(appWindow.consoleVisible, false);
        verify(sheet.opened);

        keyClick(Qt.Key_Slash, Qt.ControlModifier);
        verify(sheet.opened);
        verify(!item("shortcutsSheet").opened);

        compare(appWindow.serverListVisible, true);
        keyClick(Qt.Key_S, Qt.ControlModifier | Qt.ShiftModifier);
        compare(appWindow.serverListVisible, true);
        verify(sheet.opened);
    }

    function test_aboutSheetBlocksCtrlQ() {
        openSeededAppWindow();
        var sheet = item("aboutSheet");
        mouseClick(item("selfVersionHit"));
        tryCompare(sheet, "opened", true);
        verify(appWindow.visible);

        keyClick(Qt.Key_Q, Qt.ControlModifier);
        wait(0);
        verify(appWindow.visible);
        verify(sheet.opened);
    }

    function test_aboutSheetOkCloses() {
        openSeededAppWindow();
        var sheet = item("aboutSheet");
        mouseClick(item("selfVersionHit"));
        tryCompare(sheet, "opened", true);

        mouseClick(item("aboutOk"));
        tryCompare(sheet, "opened", false);
        compare(appWindow.currentConversation, "#omarchy");
    }

    function test_aboutSheetGithubLinkOpensRepo() {
        openSeededAppWindow();
        mouseClick(item("selfVersionHit"));
        tryCompare(item("aboutSheet"), "opened", true);
        compare(item("aboutUpdateCheck").repoUrl,
                "https://github.com/fredimachado/omairc");

        appWindow.lastOpenedUrl = "";
        mouseClick(item("aboutGithubLink"));
        compare(appWindow.lastOpenedUrl, "https://github.com/fredimachado/omairc");
        verify(item("aboutSheet").opened);
    }

    function test_aboutSheetCheckForUpdatesShowsLatest() {
        openSeededAppWindow();
        mouseClick(item("selfVersionHit"));
        tryCompare(item("aboutSheet"), "opened", true);

        var checker = item("aboutUpdateCheck");
        checker.currentVersion = "0.1.0";
        verify(item("aboutCheckUpdates").visible);
        verify(!item("aboutUpdateStatus").visible);

        checker.applyGithubPayload(
            '{"tag_name":"v9.9.9","html_url":"https://github.com/fredimachado/omairc/releases/tag/v9.9.9"}',
            200);
        compare(checker.status, "updateAvailable");
        compare(item("aboutUpdateStatus").text, "Version 9.9.9 is available.");
        verify(item("aboutUpdateStatus").visible);

        appWindow.lastOpenedUrl = "";
        clickAboutUpdateStatus();
        compare(appWindow.lastOpenedUrl,
                "https://github.com/fredimachado/omairc/releases/tag/v9.9.9");

        checker.applyGithubPayload(
            '{"tag_name":"v0.1.0","html_url":"https://github.com/fredimachado/omairc/releases/tag/v0.1.0"}',
            200);
        compare(item("aboutUpdateStatus").text, "Omairc is up to date.");
    }

    function test_aboutSheetInstalledCopyDownloadsSetup() {
        openSeededAppWindow();
        mouseClick(item("selfVersionHit"));
        tryCompare(item("aboutSheet"), "opened", true);

        var checker = item("aboutUpdateCheck");
        checker.currentVersion = "0.1.0";
        checker.installedCopy = false;
        var payload = '{"tag_name":"v9.9.9","html_url":"https://github.com/fredimachado/omairc/releases/tag/v9.9.9","assets":[{"name":"omairc-9.9.9-windows-x64-setup.exe","browser_download_url":"https://github.com/fredimachado/omairc/releases/download/v9.9.9/omairc-9.9.9-windows-x64-setup.exe","size":12,"digest":"sha256:d323de13bbb0973b891849578f74c69cb4ef85fc37ffeb05b31d835f708c0ae9"}]}';
        checker.applyGithubPayload(payload, 200);
        compare(checker.installerUpdateAvailable, false);
        compare(item("aboutUpdateStatus").text, "Version 9.9.9 is available.");
        appWindow.lastOpenedUrl = "";
        clickAboutUpdateStatus();
        compare(appWindow.lastOpenedUrl,
                "https://github.com/fredimachado/omairc/releases/tag/v9.9.9");

        checker.installedCopy = true;
        checker.suppressInstallerLaunch();
        checker.stageInstallerDownload("omairc-setup");
        checker.applyGithubPayload(payload, 200);
        compare(checker.installerUpdateAvailable, true);
        appWindow.lastOpenedUrl = "";
        clickAboutUpdateStatus();
        compare(appWindow.lastOpenedUrl, "");
        compare(checker.status, "readyToRestart");
        compare(item("aboutUpdateStatus").text, "Restart to update");
        compare(item("selfVersionLabel").text, "Restart to update");

        mouseClick(item("aboutOk"));
        tryCompare(item("aboutSheet"), "opened", false);
        compare(checker.status, "readyToRestart");
        compare(item("selfVersionLabel").text, "Restart to update");
        compare(checker.launchAttempts, 0);
        mouseClick(item("selfVersionHit"));
        compare(checker.launchAttempts, 1);
        verify(!item("aboutSheet").opened);
        appWindow.close();
        compare(checker.launchAttempts, 1);
    }

    function test_aboutSheetQuitStartsReadyInstaller() {
        openSeededAppWindow();
        var checker = item("aboutUpdateCheck");
        checker.currentVersion = "0.1.0";
        checker.installedCopy = true;
        checker.suppressInstallerLaunch();
        checker.stageInstallerDownload("omairc-setup");
        checker.applyGithubPayload(
            '{"tag_name":"v9.9.9","html_url":"https://github.com/fredimachado/omairc/releases/tag/v9.9.9","assets":[{"name":"omairc-9.9.9-windows-x64-setup.exe","browser_download_url":"https://github.com/fredimachado/omairc/releases/download/v9.9.9/omairc-9.9.9-windows-x64-setup.exe","size":12,"digest":"sha256:d323de13bbb0973b891849578f74c69cb4ef85fc37ffeb05b31d835f708c0ae9"}]}',
            200);
        compare(checker.status, "updateAvailable");
        appWindow.close();
        compare(checker.launchAttempts, 0);

        openSeededAppWindow();
        checker = item("aboutUpdateCheck");
        checker.currentVersion = "0.1.0";
        checker.installedCopy = true;
        checker.suppressInstallerLaunch();
        checker.stageInstallerDownload("omairc-setup");
        checker.applyGithubPayload(
            '{"tag_name":"v9.9.9","html_url":"https://github.com/fredimachado/omairc/releases/tag/v9.9.9","assets":[{"name":"omairc-9.9.9-windows-x64-setup.exe","browser_download_url":"https://github.com/fredimachado/omairc/releases/download/v9.9.9/omairc-9.9.9-windows-x64-setup.exe","size":12,"digest":"sha256:d323de13bbb0973b891849578f74c69cb4ef85fc37ffeb05b31d835f708c0ae9"}]}',
            200);
        mouseClick(item("selfVersionHit"));
        tryCompare(item("aboutSheet"), "opened", true);
        clickAboutUpdateStatus();
        compare(checker.status, "readyToRestart");
        compare(checker.launchAttempts, 0);
        appWindow.close();
        compare(checker.launchAttempts, 1);
    }

    function test_aboutSheetOpensFromFirstRunVersion() {
        var window = createTemporaryObject(setupWindowComponent, null);
        verify(window !== null, "The setup window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.suppressExternalUrlOpen = true;

        var sheet = findChild(window, "aboutSheet");
        verify(sheet !== null, "Could not find aboutSheet");
        verify(!sheet.opened);
        mouseClick(findChild(window, "selfVersionHit"));
        tryCompare(sheet, "opened", true);
        compare(findChild(window, "aboutVersion").text, window.appVersion);
        compare(findChild(window, "aboutOpenSource").text, "This project is open-source.");
        verify(window.connectionOverlayVisible);
        fuzzyCompare(sheet.color.a, 0.5, 0.01);

        mouseClick(findChild(window, "aboutSheetDimmer"), 10, 10);
        tryCompare(sheet, "opened", false);
        verify(window.connectionOverlayVisible);

        mouseClick(findChild(window, "selfVersionHit"));
        tryCompare(sheet, "opened", true);
        keyClick(Qt.Key_Escape);
        tryCompare(sheet, "opened", false);
        verify(window.connectionOverlayVisible);
        window.close();
    }

    function test_aboutSheetLeftClickDimmerDismisses() {
        openSeededAppWindow();
        var sheet = item("aboutSheet");
        mouseClick(item("selfVersionHit"));
        tryCompare(sheet, "opened", true);

        mouseClick(item("aboutSheetDimmer"), 10, 10);
        tryCompare(sheet, "opened", false);
        compare(appWindow.currentConversation, "#omarchy");
    }

    function test_aboutSheetIgnoresRightClickAndCardClick() {
        openSeededAppWindow();
        var sheet = item("aboutSheet");
        mouseClick(item("selfVersionHit"));
        tryCompare(sheet, "opened", true);

        mouseClick(item("aboutSheetDimmer"), 10, 10, Qt.RightButton);
        wait(0);
        verify(sheet.opened);

        mouseClick(item("aboutName"));
        wait(0);
        verify(sheet.opened);
        compare(appWindow.currentConversation, "#omarchy");
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
    }

    function test_liveIdentityFooterShowsOfflineWhenDisconnected() {
        gatedIrc.selfAway = false;
        gatedIrc.connectionStatus = "Offline";
        var window = createTemporaryObject(gatedWindowComponent, null);
        verify(window !== null, "The gated offline window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        compare(findChild(window, "selfPresenceLabel").text, "offline");
        verify(Qt.colorEqual(findChild(window, "selfPresenceDot").color,
                             window.mutedColor));
        gatedIrc.selfAway = true;
        compare(findChild(window, "selfPresenceLabel").text, "offline");
        gatedIrc.connectionStatus = "Connected";
        compare(findChild(window, "selfPresenceLabel").text, "away");
        verify(Qt.colorEqual(findChild(window, "selfPresenceDot").color, "#d6a552"));
        window.close();
    }

    function test_identityFooterFallsBackToConnectionNick() {
        var window = createTemporaryObject(fallbackWindowComponent, null);
        verify(window !== null, "The fallback window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);

        compare(findChild(window, "selfNickLabel").text, "sheet-nick");
        compare(findChild(window, "selfVersionLabel").text, window.appVersion);
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

    function test_typedQueryOpensDirectAndClearsComposer() {
        openSeededAppWindow();
        var composer = item("messageComposer");
        mouseClick(composer);
        verify(composer.activeFocus);
        typeText("/query mira");
        compare(composer.text, "/query mira");
        if (item("slashCompleteList").visible)
            keyClick(Qt.Key_Escape);
        keyClick(Qt.Key_Return);

        compare(appWindow.currentConversation, "mira");
        compare(appWindow.consoleVisible, false);
        compare(appWindow.currentTopic, "Direct message with mira");
        compare(composer.text, "");
        tryCompare(composer, "activeFocus", true);
        verify(visibleDirects(seed.omarchyNetworkId).indexOf("mira") !== -1);

        mouseClick(namedItem(liveConversation("#omarchy")));
        tryCompare(appWindow, "currentConversation", "#omarchy");
        compare(composer.text, "");
    }

    function test_typedQueryFromStatusOpensDirectAndClearsComposer() {
        openSeededAppWindow();
        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);
        var composer = item("messageComposer");
        mouseClick(composer);
        verify(composer.activeFocus);
        typeText("/query mira");
        compare(composer.text, "/query mira");
        if (item("slashCompleteList").visible)
            keyClick(Qt.Key_Escape);
        keyClick(Qt.Key_Return);

        compare(appWindow.consoleVisible, false);
        compare(appWindow.currentConversation, "mira");
        compare(composer.text, "");
        tryCompare(composer, "activeFocus", true);
    }

    function test_typedQueryRestoresDestinationDraft() {
        openSeededAppWindow();
        var composer = item("messageComposer");
        mouseClick(namedItem(liveConversation("anna")));
        tryCompare(appWindow, "currentConversation", "anna");
        mouseClick(composer);
        typeText("anna draft");
        compare(composer.text, "anna draft");

        mouseClick(namedItem(liveConversation("#omarchy")));
        tryCompare(appWindow, "currentConversation", "#omarchy");
        compare(composer.text, "");

        mouseClick(composer);
        typeText("/query anna");
        compare(composer.text, "/query anna");
        if (item("slashCompleteList").visible)
            keyClick(Qt.Key_Escape);
        keyClick(Qt.Key_Return);

        compare(appWindow.currentConversation, "anna");
        compare(composer.text, "anna draft");
        tryCompare(composer, "activeFocus", true);
    }

    function test_refusedQueryKeepsComposer() {
        openSeededAppWindow();
        var composer = item("messageComposer");
        mouseClick(composer);
        verify(composer.activeFocus);
        typeText("/query");
        compare(composer.text, "/query");
        if (item("slashCompleteList").visible)
            keyClick(Qt.Key_Escape);
        keyClick(Qt.Key_Return);

        compare(appWindow.currentConversation, "#omarchy");
        compare(appWindow.consoleVisible, false);
        compare(composer.text, "/query");
        tryCompare(composer, "activeFocus", true);
    }

    function test_refusedQueryFromStatusKeepsComposer() {
        openSeededAppWindow();
        keyClick(Qt.Key_QuoteLeft, Qt.ControlModifier);
        tryCompare(appWindow, "consoleVisible", true);
        var composer = item("messageComposer");
        var list = item("consoleList");
        mouseClick(composer);
        verify(composer.activeFocus);
        typeText("/query");
        compare(composer.text, "/query");
        if (item("slashCompleteList").visible)
            keyClick(Qt.Key_Escape);
        keyClick(Qt.Key_Return);

        compare(appWindow.consoleVisible, true);
        compare(composer.text, "/query");
        tryCompare(composer, "activeFocus", true);
        var found = false;
        var row = 0;
        for (; row < list.model.rowCount(); ++row) {
            if (field(list.model, row, "text").indexOf("Command was refused") >= 0) {
                found = true;
                break;
            }
        }
        verify(found, "Status should log the refused /query");
    }

    function test_typedQueryHistoryStaysOnSource() {
        openSeededAppWindow();
        var composer = item("messageComposer");
        mouseClick(composer);
        verify(composer.activeFocus);
        typeText("/query mira");
        compare(composer.text, "/query mira");
        if (item("slashCompleteList").visible)
            keyClick(Qt.Key_Escape);
        keyClick(Qt.Key_Return);

        compare(appWindow.currentConversation, "mira");
        compare(composer.text, "");
        keyClick(Qt.Key_Up);
        compare(composer.text, "");

        mouseClick(namedItem(liveConversation("#omarchy")));
        tryCompare(appWindow, "currentConversation", "#omarchy");
        compare(composer.text, "");
        keyClick(Qt.Key_Up);
        compare(composer.text, "/query mira");
    }

    function typeJoinCommand(composer, command) {
        mouseClick(composer);
        verify(composer.activeFocus);
        typeText(command);
        compare(composer.text, command);
        if (item("slashCompleteList").visible)
            keyClick(Qt.Key_Escape);
        keyClick(Qt.Key_Return);
    }

    function test_joinOpensChannelAndConsumesComposer() {
        openSeededAppWindow();
        var composer = item("messageComposer");
        mouseClick(composer);
        verify(composer.activeFocus);
        compare(appWindow.currentConversation, "#omarchy");

        mouseClick(namedItem(liveConversation("#help")));
        tryCompare(appWindow, "currentConversation", "#help");
        typeText("help draft");
        compare(composer.text, "help draft");

        mouseClick(namedItem(liveConversation("#omarchy")));
        tryCompare(appWindow, "currentConversation", "#omarchy");
        compare(composer.text, "");

        typeJoinCommand(composer, "/join #desktop,#help");
        tryCompare(appWindow, "currentConversation", "#help");
        compare(composer.text, "help draft");

        mouseClick(namedItem(liveConversation("#omarchy")));
        tryCompare(appWindow, "currentConversation", "#omarchy");
        compare(composer.text, "");

        typeJoinCommand(composer, "/join #lab");
        tryCompare(appWindow, "currentConversation", "#lab");
        compare(composer.text, "");
        verify(namedItem(liveConversation("#lab")) !== null);
        compare(appWindow.currentConversationIsChannel, true);

        mouseClick(namedItem(liveConversation("#omarchy")));
        tryCompare(appWindow, "currentConversation", "#omarchy");
        compare(composer.text, "");
    }

    function openSeededListWindow() {
        destroyAppWindowAndSeed();
        seed = createTemporaryObject(seedComponent, testCase);
        verify(seed !== null, "SeededIrcFixture should construct");
        verify(seed.openWithAutoEcho(), seed.lastError);
        verify(seed.connection, "seeded window needs a real IrcConnection");
        appWindow = createTemporaryObject(seededWindowComponent, testCase, {
            backend: seed.backend,
            irc: seed.irc,
            slashCommands: seed.slash,
            connection: seed.connection,
            avatarStore: appAvatarStore
        });
        verify(appWindow !== null, "The seeded Omairc window should load");
        tryCompare(appWindow, "visible", true);
        waitForRendering(appWindow.contentItem);
        appWindow.suppressExternalUrlOpen = true;
        appWindow.suppressDesktopNotification = true;
        tryVerify(function() {
            return appWindow.currentConversation === "#omarchy"
                && !appWindow.consoleVisible
                && !appWindow.connectionOverlayVisible;
        });
    }

    function typeListCommand(composer, command) {
        if (!composer.activeFocus) {
            mouseClick(composer);
            tryCompare(composer, "activeFocus", true);
        }
        typeText(command);
        compare(composer.text, command);
        if (item("slashCompleteList").visible)
            keyClick(Qt.Key_Escape);
        keyClick(Qt.Key_Return);
    }

    function test_listOpensOverlayAndJoins() {
        openSeededListWindow();
        var composer = item("messageComposer");
        typeListCommand(composer, "/list");
        var sheet = item("channelListSheet");
        tryCompare(sheet, "opened", true);
        tryCompare(item("channelListFilter"), "activeFocus", true);
        var model = appWindow.irc.channelList;
        tryCompare(model, "complete", true);
        compare(model.rowCount(), 6);
        compare(model.get(0).channel, "#linux");
        compare(model.get(0).users, 42);
        compare(item("channelListStatus").text, "6 channels");
        verify(containsMirc(model.get(0).topic));

        var list = item("channelListList");
        list.positionViewAtIndex(0, ListView.Contain);
        waitForRendering(appWindow.contentItem);
        tryVerify(function() { return list.itemAtIndex(0) !== null; });
        var linuxRow = list.itemAtIndex(0);
        verify(linuxRow !== null, "The #linux list row should be rendered");
        compare(linuxRow.objectName, "channelListRow-#linux");
        var topic = findChild(linuxRow, "channelListTopic");
        verify(topic !== null && topic.visible, "Could not find channelListTopic");
        compare(topic.textFormat, Text.RichText);
        compare(topic.elide, Text.ElideNone);
        compare(topic.clip, true);
        compare(topic.wrapMode, Text.NoWrap);
        compare(linuxRow.clip, true);
        verify(topic.height <= linuxRow.height);
        verify(topic.x + topic.width <= linuxRow.width + 0.5);
        verify(!containsMirc(topic.text));
        verify(topic.text.indexOf("Kernel") >= 0);
        verify(topic.text.indexOf("distro") >= 0);
        verify(topic.text.indexOf("help") >= 0);
        verify(topic.text.indexOf("04") < 0);
        verify(topic.text.indexOf("<b>Kernel</b>") >= 0
               || /font-weight\s*:\s*(bold|[6-9]00)/.test(topic.text));

        var randomRow = findChild(list, "channelListRow-#random");
        verify(randomRow !== null, "The #random list row should be rendered");
        var randomTopic = findChild(randomRow, "channelListTopic");
        verify(randomTopic !== null && randomTopic.visible);
        compare(randomTopic.textFormat, Text.PlainText);
        compare(randomTopic.elide, Text.ElideRight);
        saveScreenshot("channel-list-overlay");

        typeText("distro");
        tryCompare(item("channelListFilter"), "text", "distro");
        tryVerify(function() { return model.rowCount() === 1; });
        compare(model.get(0).channel, "#linux");
        compare(item("channelListStatus").text, "1 of 6 channels");

        item("channelListFilter").text = "";
        tryCompare(item("channelListFilter"), "text", "");
        tryVerify(function() { return model.rowCount() === 6; });

        typeText("04");
        tryCompare(item("channelListFilter"), "text", "04");
        tryVerify(function() { return model.rowCount() === 0; });
        compare(item("channelListStatus").text, "No matches");

        item("channelListFilter").text = "";
        tryCompare(item("channelListFilter"), "text", "");
        tryVerify(function() { return model.rowCount() === 6; });

        typeText("lin");
        tryCompare(item("channelListFilter"), "text", "lin");
        tryVerify(function() { return model.rowCount() === 1; });
        compare(model.get(0).channel, "#linux");
        compare(item("channelListStatus").text, "1 of 6 channels");

        keyClick(Qt.Key_Return);
        tryCompare(sheet, "opened", false);
        tryCompare(appWindow, "currentConversation", "#linux");
        tryCompare(composer, "activeFocus", true);
        compare(composer.text, "");
    }

    function test_listUsesPerNetworkCache() {
        openSeededListWindow();
        var composer = item("messageComposer");
        var start = seed.omarchyFrameCount();
        typeListCommand(composer, "/list");
        var sheet = item("channelListSheet");
        tryCompare(sheet, "opened", true);
        var model = appWindow.irc.channelList;
        tryCompare(model, "complete", true);
        verify(seed.omarchyWroteFrom(start, "LIST"));
        keyClick(Qt.Key_Escape);
        tryCompare(sheet, "opened", false);
        tryCompare(composer, "activeFocus", true);

        var after = seed.omarchyFrameCount();
        typeListCommand(composer, "/list");
        tryCompare(sheet, "opened", true);
        tryCompare(model, "cached", true);
        compare(model.rowCount(), 6);
        compare(model.get(0).channel, "#linux");
        compare(seed.omarchyFrameCount(), after);
        verify(!seed.omarchyWroteFrom(after, "LIST"));
        keyClick(Qt.Key_Escape);
        tryCompare(sheet, "opened", false);
        tryCompare(composer, "activeFocus", true);

        mouseClick(namedItem(liveOftcConversation("#omarchy")));
        tryCompare(appWindow, "currentNetworkId", seed.oftcNetworkId);
        tryCompare(appWindow, "currentConversation", "#omarchy");
        typeListCommand(composer, "/list");
        tryCompare(sheet, "opened", true);
        tryCompare(model, "complete", true);
        compare(model.get(0).channel, "#debian");
        compare(model.get(0).users, 28);
        compare(model.networkId, seed.oftcNetworkId);
    }

    function openSlashWindow() {
        openSeededAppWindow();
        appWindow.requestActivate();
        tryCompare(appWindow, "active", true);
        return appWindow;
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
        compare(seed.slash.selectedIndex, 0);
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
    }

    function test_slashCompleteUpDownMoveSelection() {
        var window = openSlashWindow();
        var composer = findChild(window, "messageComposer");
        verify(composer !== null, "Could not find messageComposer");
        mouseClick(composer);
        verify(composer.activeFocus);
        typeText("/m");
        tryCompare(findChild(window, "slashCompleteList"), "visible", true);
        compare(seed.slash.selectedIndex, 0);

        keyClick(Qt.Key_Down);
        compare(seed.slash.selectedIndex, 1);
        keyClick(Qt.Key_Up);
        compare(seed.slash.selectedIndex, 0);
        keyClick(Qt.Key_Up);
        compare(seed.slash.selectedIndex, seed.slash.matches.length - 1);
        compare(composer.text, "/m");
        verify(findChild(window, "slashCompleteList").visible);
    }

    function test_slashCompleteHistoryUpWalksPastCommand() {
        var window = openSlashWindow();
        var composer = findChild(window, "messageComposer");
        verify(composer !== null, "Could not find messageComposer");
        mouseClick(composer);
        verify(composer.activeFocus);

        typeText("hello");
        keyClick(Qt.Key_Return);
        compare(composer.text, "");

        typeText("/a");
        tryCompare(findChild(window, "slashCompleteList"), "visible", true);
        keyClick(Qt.Key_Tab);
        compare(composer.text, "/away ");
        mouseClick(findChild(window, "sendButton"));
        compare(composer.text, "");
        tryCompare(findChild(window, "slashCompleteList"), "visible", false);

        keyClick(Qt.Key_Up);
        compare(composer.text, "/away");
        tryCompare(findChild(window, "slashCompleteList"), "visible", false);
        compare(seed.slash.selectedIndex, -1);

        keyClick(Qt.Key_Up);
        compare(composer.text, "hello");
        compare(findChild(window, "slashCompleteList").visible, false);
        window.close();
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
        var ctrl = shortcutCtrlLabel();
        var alt = shortcutAltLabel();
        verify(texts.indexOf("MOVE") !== -1, "shortcut sheet should head MOVE");
        verify(texts.indexOf("JUMP") !== -1, "shortcut sheet should head JUMP");
        verify(texts.indexOf("WRITE") !== -1, "shortcut sheet should head WRITE");
        verify(texts.indexOf("WINDOW") !== -1, "shortcut sheet should head WINDOW");
        verify(texts.indexOf("CONNECT") !== -1, "shortcut sheet should head CONNECT");
        verify(texts.indexOf(alt + "+Down / " + alt + "+Up") !== -1,
               "shortcut sheet should list " + alt + "+Down / " + alt + "+Up");
        verify(texts.indexOf("walk conversations") !== -1,
               "shortcut sheet should name walk conversations");
        verify(texts.indexOf(alt + "+Left / " + alt + "+Right") !== -1,
               "shortcut sheet should list " + alt + "+Left / " + alt + "+Right");
        verify(texts.indexOf("walk networks") !== -1,
               "shortcut sheet should name walk networks");
        verify(texts.indexOf(alt + "+Shift+Left / Right") !== -1,
               "shortcut sheet should list " + alt + "+Shift+Left / Right");
        verify(texts.indexOf("collapse / expand network") !== -1,
               "shortcut sheet should name collapse / expand network");
        verify(texts.indexOf(ctrl + "+" + alt + "+Shift+Left / Right") !== -1,
               "shortcut sheet should list " + ctrl + "+" + alt + "+Shift+Left / Right");
        verify(texts.indexOf("collapse / expand all") !== -1,
               "shortcut sheet should name collapse / expand all");
        verify(texts.indexOf(alt + "+Shift+Up / Down") !== -1,
               "shortcut sheet should list " + alt + "+Shift+Up / Down");
        verify(texts.indexOf("move network") !== -1,
               "shortcut sheet should name move network");
        verify(texts.indexOf("collapse network") === -1,
               "shortcut sheet should not keep a separate collapse network row");
        verify(texts.indexOf("expand network") === -1,
               "shortcut sheet should not keep a separate expand network row");
        verify(texts.indexOf("collapse all networks") === -1,
               "shortcut sheet should not keep a separate collapse all networks row");
        verify(texts.indexOf("expand all networks") === -1,
               "shortcut sheet should not keep a separate expand all networks row");
        verify(texts.indexOf("move network up") === -1,
               "shortcut sheet should not keep a separate move network up row");
        verify(texts.indexOf("move network down") === -1,
               "shortcut sheet should not keep a separate move network down row");
        verify(texts.indexOf(ctrl + "+K") !== -1,
               "shortcut sheet should list " + ctrl + "+K");
        verify(texts.indexOf("jump to conversation") !== -1,
               "shortcut sheet should name jump to conversation");
        verify(texts.indexOf(ctrl + "+Shift+K") !== -1,
               "shortcut sheet should list " + ctrl + "+Shift+K");
        verify(texts.indexOf("jump to nick") !== -1,
               "shortcut sheet should name jump to nick");
        verify(texts.indexOf(ctrl + "+Shift+S") !== -1,
               "shortcut sheet should list " + ctrl + "+Shift+S");
        verify(texts.indexOf("server list") !== -1,
               "shortcut sheet should name server list");
        verify(texts.indexOf("/disconnect") === -1,
               "shortcut sheet should not list /disconnect");
        verify(texts.indexOf(ctrl + "+/") !== -1,
               "shortcut sheet should list " + ctrl + "+/");
        verify(texts.indexOf(ctrl + "+Tab") !== -1,
               "shortcut sheet should list " + ctrl + "+Tab");
        verify(texts.indexOf(ctrl + "+Shift+Tab") === -1,
               "shortcut sheet should not list " + ctrl + "+Shift+Tab");
        verify(texts.indexOf("Connect tabs") !== -1,
               "shortcut sheet should name Connect tabs");
        verify(texts.indexOf(ctrl + "+N") !== -1,
               "shortcut sheet should list " + ctrl + "+N");
        verify(texts.indexOf("add network") !== -1,
               "shortcut sheet should name add network");
        verify(texts.indexOf(ctrl + "+Shift+Delete") !== -1,
               "shortcut sheet should list " + ctrl + "+Shift+Delete");
        verify(texts.indexOf("remove network") !== -1,
               "shortcut sheet should name remove network");
        verify(texts.indexOf(ctrl + "+Enter") !== -1,
               "shortcut sheet should list " + ctrl + "+Enter");
        verify(texts.indexOf("apply selected network") !== -1,
               "shortcut sheet should name apply selected network");
        verify(texts.indexOf("apply connection") === -1,
               "shortcut sheet should not keep the old apply connection label");
        verify(texts.indexOf("Page Up / Page Down") !== -1,
               "shortcut sheet should list Page Up / Page Down");
        verify(texts.indexOf("scroll transcript") !== -1,
               "shortcut sheet should name scroll transcript");
        verify(texts.indexOf("Shift+Page Up / Shift+Page Down") !== -1,
               "shortcut sheet should list Shift+Page Up / Shift+Page Down");
        verify(texts.indexOf("scroll transcript half page") !== -1,
               "shortcut sheet should name scroll transcript half page");
        verify(texts.indexOf("page focused members") !== -1,
               "shortcut sheet should name page focused members");
        verify(texts.indexOf("page focused members half page") !== -1,
               "shortcut sheet should name page focused members half page");
        verify(texts.indexOf("first / last focused nick") !== -1,
               "shortcut sheet should name first / last focused nick");
        verify(texts.indexOf(ctrl + "+Home / " + ctrl + "+End") !== -1,
               "shortcut sheet should list " + ctrl + "+Home / " + ctrl + "+End");
        verify(texts.indexOf("top / bottom") !== -1,
               "shortcut sheet should name top / bottom");
        verify(texts.indexOf(ctrl + "+C") !== -1,
               "shortcut sheet should list " + ctrl + "+C");
        verify(texts.indexOf("copy selection") !== -1,
               "shortcut sheet should name copy selection");
        saveScreenshot("shortcuts-sheet");
        keyClick(Qt.Key_Escape);
        tryCompare(sheet, "opened", false);
    }

    function test_walkNetworksFromComposerLeavesCaret() {
        openSeededAppWindow();
        var composer = item("messageComposer");
        mouseClick(composer);
        tryCompare(composer, "activeFocus", true);
        typeText("hello world");
        compare(composer.text, "hello world");
        var caret = composer.cursorPosition;

        keyClick(Qt.Key_Right, Qt.AltModifier);

        compare(appWindow.sidebarNetworkFocusId, seed.oftcNetworkId);
        compare(appWindow.currentConversation, "#omarchy");
        compare(composer.text, "hello world");
        compare(composer.cursorPosition, caret);
        tryCompare(composer, "activeFocus", true);

        keyClick(Qt.Key_Left, Qt.AltModifier | Qt.ShiftModifier);

        tryVerify(function() {
            return appWindow.connection.isNetworkCollapsed(seed.oftcNetworkId);
        });
        compare(composer.text, "hello world");
        compare(composer.cursorPosition, caret);
        compare(appWindow.sidebarNetworkFocusId, seed.oftcNetworkId);
    }

    function test_collapseFocusedNetworkWithShortcut() {
        openSeededAppWindow();
        keyClick(Qt.Key_Right, Qt.AltModifier);
        compare(appWindow.sidebarNetworkFocusId, seed.oftcNetworkId);
        verify(sidebarRowShown(liveOftcConversation("#build")));
        verify(sidebarRowShown(liveOftcConversation("rio")));
        verify(liveChannelsHeading(seed.oftcNetworkId).visible);
        verify(liveDirectsHeading(seed.oftcNetworkId).visible);
        verify(liveHeader(seed.oftcNetworkId).visible);
        verify(liveHeader(seed.oftcNetworkId).height > 0);

        keyClick(Qt.Key_Left, Qt.AltModifier | Qt.ShiftModifier);

        tryVerify(function() {
            return appWindow.connection.isNetworkCollapsed(seed.oftcNetworkId);
        });
        waitForRendering(appWindow.contentItem);
        compare(appWindow.sidebarNetworkFocusId, seed.oftcNetworkId);
        verify(liveHeader(seed.oftcNetworkId).visible);
        verify(liveHeader(seed.oftcNetworkId).height > 0);
        compare(sidebarRowShown(liveOftcConversation("#build")), false);
        compare(namedItem(liveOftcConversation("#build")).height, 0);
        compare(sidebarRowShown(liveOftcConversation("rio")), false);
        compare(liveChannelsHeading(seed.oftcNetworkId).visible, false);
        compare(liveChannelsHeading(seed.oftcNetworkId).height, 0);
        compare(liveDirectsHeading(seed.oftcNetworkId).visible, false);
        compare(liveDirectsHeading(seed.oftcNetworkId).height, 0);
        verify(sidebarRowShown(liveConversation("#omarchy")));
        verify(sidebarRowShown(liveConversation("anna")));
        saveScreenshot("network-section-collapsed");

        keyClick(Qt.Key_Right, Qt.AltModifier | Qt.ShiftModifier);

        tryVerify(function() {
            return !appWindow.connection.isNetworkCollapsed(seed.oftcNetworkId);
        });
        waitForRendering(appWindow.contentItem);
        verify(sidebarRowShown(liveOftcConversation("#build")));
        verify(sidebarRowShown(liveOftcConversation("rio")));
        verify(liveChannelsHeading(seed.oftcNetworkId).visible);
        verify(liveDirectsHeading(seed.oftcNetworkId).visible);
        compare(appWindow.sidebarNetworkFocusId, seed.oftcNetworkId);
    }

    function test_networkCollapseChordNeedsHeaderFocus() {
        openSeededAppWindow();
        mouseClick(item("messageComposer"));
        tryCompare(item("messageComposer"), "activeFocus", true);
        compare(appWindow.sidebarNetworkFocusId, "");
        verify(sidebarRowShown(liveOftcConversation("#build")));
        var order = appWindow.sidebarNetworkSections();
        compare(order[0].networkId, seed.omarchyNetworkId);
        compare(order[1].networkId, seed.oftcNetworkId);

        keyClick(Qt.Key_Left, Qt.AltModifier | Qt.ShiftModifier);
        keyClick(Qt.Key_Right, Qt.AltModifier | Qt.ShiftModifier);
        keyClick(Qt.Key_Up, Qt.AltModifier | Qt.ShiftModifier);
        keyClick(Qt.Key_Down, Qt.AltModifier | Qt.ShiftModifier);

        compare(appWindow.sidebarNetworkFocusId, "");
        compare(appWindow.connection.isNetworkCollapsed(seed.oftcNetworkId), false);
        compare(appWindow.connection.isNetworkCollapsed(seed.omarchyNetworkId), false);
        verify(sidebarRowShown(liveOftcConversation("#build")));
        verify(sidebarRowShown(liveConversation("#omarchy")));
        compare(appWindow.sidebarNetworkSections()[0].networkId, seed.omarchyNetworkId);
        compare(appWindow.sidebarNetworkSections()[1].networkId, seed.oftcNetworkId);
        tryCompare(item("messageComposer"), "activeFocus", true);
    }

    function test_collapseAllNetworksWithoutHeaderFocus() {
        openSeededAppWindow();
        mouseClick(item("messageComposer"));
        tryCompare(item("messageComposer"), "activeFocus", true);
        compare(appWindow.sidebarNetworkFocusId, "");

        keyClick(Qt.Key_Left, shortcutCommandModifier() | Qt.AltModifier | Qt.ShiftModifier);

        compare(appWindow.sidebarNetworkFocusId, "");
        tryVerify(function() {
            return appWindow.connection.isNetworkCollapsed(seed.omarchyNetworkId)
                && appWindow.connection.isNetworkCollapsed(seed.oftcNetworkId);
        });
        waitForRendering(appWindow.contentItem);
        compare(sidebarRowShown(liveConversation("#omarchy")), false);
        compare(sidebarRowShown(liveConversation("anna")), false);
        compare(sidebarRowShown(liveOftcConversation("#build")), false);
        compare(sidebarRowShown(liveOftcConversation("rio")), false);
        compare(liveChannelsHeading(seed.omarchyNetworkId).visible, false);
        compare(liveChannelsHeading(seed.oftcNetworkId).visible, false);
        verify(liveHeader(seed.omarchyNetworkId).visible);
        verify(liveHeader(seed.oftcNetworkId).visible);
        verify(namedItem("networkUnreadMark-" + seed.omarchyNetworkId).visible,
               "collapsed headers keep the unread/mention mark");
        saveScreenshot("networks-collapsed-all");

        keyClick(Qt.Key_Right, shortcutCommandModifier() | Qt.AltModifier | Qt.ShiftModifier);

        compare(appWindow.sidebarNetworkFocusId, "");
        tryVerify(function() {
            return !appWindow.connection.isNetworkCollapsed(seed.omarchyNetworkId)
                && !appWindow.connection.isNetworkCollapsed(seed.oftcNetworkId);
        });
        waitForRendering(appWindow.contentItem);
        verify(sidebarRowShown(liveConversation("#omarchy")));
        verify(sidebarRowShown(liveOftcConversation("#build")));
        verify(liveChannelsHeading(seed.omarchyNetworkId).visible);
        verify(liveChannelsHeading(seed.oftcNetworkId).visible);
    }

    function test_collapseAllNetworksFromComposerLeavesCaret() {
        openSeededAppWindow();
        var composer = item("messageComposer");
        mouseClick(composer);
        tryCompare(composer, "activeFocus", true);
        typeText("hello world");
        compare(composer.text, "hello world");
        var caret = composer.cursorPosition;
        compare(appWindow.sidebarNetworkFocusId, "");

        keyClick(Qt.Key_Left, shortcutCommandModifier() | Qt.AltModifier | Qt.ShiftModifier);

        tryVerify(function() {
            return appWindow.connection.isNetworkCollapsed(seed.omarchyNetworkId)
                && appWindow.connection.isNetworkCollapsed(seed.oftcNetworkId);
        });
        compare(composer.text, "hello world");
        compare(composer.cursorPosition, caret);
        compare(appWindow.sidebarNetworkFocusId, "");
        tryCompare(composer, "activeFocus", true);

        keyClick(Qt.Key_Right, shortcutCommandModifier() | Qt.AltModifier | Qt.ShiftModifier);

        tryVerify(function() {
            return !appWindow.connection.isNetworkCollapsed(seed.omarchyNetworkId)
                && !appWindow.connection.isNetworkCollapsed(seed.oftcNetworkId);
        });
        compare(composer.text, "hello world");
        compare(composer.cursorPosition, caret);
        compare(appWindow.sidebarNetworkFocusId, "");
        tryCompare(composer, "activeFocus", true);
    }

    function test_altWalkSkipsCollapsedNetworkRows() {
        openSeededAppWindow();
        appWindow.connection.setNetworkCollapsed(seed.oftcNetworkId, true);
        waitForRendering(appWindow.contentItem);
        compare(sidebarRowShown(liveOftcConversation("#build")), false);
        mouseClick(namedItem(liveConversation("dax")));
        tryCompare(appWindow, "currentConversation", "dax");

        keyClick(Qt.Key_Down, Qt.AltModifier);

        tryCompare(appWindow, "currentConversation", "#desktop");
        compare(appWindow.currentConversationId, seed.omarchyNetworkId + "\n#desktop");
        compare(appWindow.consoleVisible, false);

        mouseClick(namedItem(liveConversation("#desktop")));
        tryCompare(appWindow, "currentConversation", "#desktop");
        keyClick(Qt.Key_Up, Qt.AltModifier);
        tryCompare(appWindow, "currentConversation", "dax");
        compare(appWindow.currentConversationId, seed.omarchyNetworkId + "\ndax");
    }

    function test_altWalkFromCollapsedCurrentNetworkKeepsDirection() {
        openSeededAppWindow();
        compare(appWindow.currentConversation, "#omarchy");
        compare(appWindow.currentConversationId, seed.omarchyNetworkId + "\n#omarchy");

        var rows = appWindow.sidebarConversationRows();
        var lastOftc = null;
        var index;
        for (index = 0; index < rows.length; ++index) {
            if (rows[index].networkId === seed.oftcNetworkId)
                lastOftc = rows[index];
        }
        verify(lastOftc !== null, "seeded oftc should have a conversation");

        appWindow.connection.setNetworkCollapsed(seed.omarchyNetworkId, true);
        waitForRendering(appWindow.contentItem);
        compare(sidebarRowShown(liveConversation("#omarchy")), false);
        compare(appWindow.currentConversation, "#omarchy");

        keyClick(Qt.Key_Down, Qt.AltModifier);

        tryCompare(appWindow, "currentConversation", "#build");
        compare(appWindow.currentConversationId, seed.oftcNetworkId + "\n#build");
        compare(appWindow.connection.isNetworkCollapsed(seed.omarchyNetworkId), true);

        appWindow.connection.setNetworkCollapsed(seed.omarchyNetworkId, false);
        waitForRendering(appWindow.contentItem);
        mouseClick(namedItem(liveConversation("#omarchy")));
        tryCompare(appWindow, "currentConversation", "#omarchy");
        appWindow.connection.setNetworkCollapsed(seed.omarchyNetworkId, true);
        waitForRendering(appWindow.contentItem);
        compare(sidebarRowShown(liveConversation("#omarchy")), false);
        compare(appWindow.currentConversation, "#omarchy");

        keyClick(Qt.Key_Up, Qt.AltModifier);

        tryCompare(appWindow, "currentConversation", lastOftc.conversationName);
        compare(appWindow.currentConversationId, lastOftc.conversationId);
        compare(appWindow.connection.isNetworkCollapsed(seed.omarchyNetworkId), true);
    }

    function test_altWalkFromCollapsedMiddleNetworkKeepsDirection() {
        restoreNamedConnection();
        namedNetworks.append({
            networkId: "oftc",
            displayName: "irc.oftc.net",
            stored: true,
            selected: false,
            iconColor: 1,
            iconUrl: "",
            collapsed: false
        });
        namedNetworks.append({
            networkId: "tilde",
            displayName: "tilde.chat",
            stored: true,
            selected: false,
            iconColor: 1,
            iconUrl: "",
            collapsed: false
        });
        liveConversations.append({
            conversation: "#build",
            unread: 0,
            mention: false,
            direct: false,
            networkId: "oftc",
            conversationId: "oftc\n#build",
            conversationName: "#build",
            typing: false
        });
        liveConversations.append({
            conversation: "#town",
            unread: 0,
            mention: false,
            direct: false,
            networkId: "tilde",
            conversationId: "tilde\n#town",
            conversationName: "#town",
            typing: false
        });
        liveIrc.conversationEpoch += 1;

        var window = createTemporaryObject(liveNamedWindowComponent, null);
        verify(window !== null, "The three-network window should load");
        tryCompare(window, "visible", true);
        waitForRendering(window.contentItem);
        window.requestActivate();
        tryCompare(window, "active", true);

        try {
            compare(window.sidebarNetworkSections().length, 3);
            var rows = window.sidebarConversationRows();
            compare(rows.length, 4);
            compare(rows[0].conversationId, "libera\n#omarchy");
            compare(rows[1].conversationId, "libera\nanna");
            compare(rows[2].conversationId, "oftc\n#build");
            compare(rows[3].conversationId, "tilde\n#town");

            liveIrc.selectConversation("oftc", "#build");
            tryCompare(window, "currentConversation", "#build");
            compare(window.currentConversationId, "oftc\n#build");

            window.connection.setNetworkCollapsed("oftc", true);
            waitForRendering(window.contentItem);
            compare(window.connection.isNetworkCollapsed("oftc"), true);
            compare(window.visibleSidebarConversationRows().length, 3);
            compare(window.currentConversation, "#build");

            window.stepConversation(1);

            tryCompare(window, "currentConversation", "#town");
            compare(window.currentConversationId, "tilde\n#town");
            compare(window.connection.isNetworkCollapsed("oftc"), true);

            liveIrc.selectConversation("oftc", "#build");
            tryCompare(window, "currentConversation", "#build");
            compare(window.connection.isNetworkCollapsed("oftc"), true);

            window.stepConversation(-1);

            tryCompare(window, "currentConversation", "anna");
            compare(window.currentConversationId, "libera\nanna");
            compare(window.connection.isNetworkCollapsed("oftc"), true);

            window.close();
        } finally {
            while (liveConversations.count > 2)
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

    function test_jumpActivatesCollapsedNetworkConversation() {
        openSeededAppWindow();
        appWindow.connection.setNetworkCollapsed(seed.oftcNetworkId, true);
        waitForRendering(appWindow.contentItem);
        compare(appWindow.connection.isNetworkCollapsed(seed.oftcNetworkId), true);
        compare(sidebarRowShown(liveOftcConversation("#build")), false);

        var sheet = openJumpSheet();
        typeText("build");
        tryCompare(item("jumpFilter"), "text", "build");
        var model = item("jumpModel");
        compare(model.count, 1);
        compare(model.get(0).name, "#build");
        compare(model.get(0).networkId, seed.oftcNetworkId);

        keyClick(Qt.Key_Return);

        tryCompare(sheet, "opened", false);
        tryCompare(appWindow, "currentConversation", "#build");
        compare(appWindow.currentConversationId, seed.oftcNetworkId + "\n#build");
        compare(appWindow.connection.isNetworkCollapsed(seed.oftcNetworkId), false);
        verify(sidebarRowShown(liveOftcConversation("#build")));
        verify(liveChannelsHeading(seed.oftcNetworkId).visible);
    }

    function test_unreadJumpExpandsCollapsedNetwork() {
        openSeededAppWindow();
        appWindow.connection.setNetworkCollapsed(seed.omarchyNetworkId, true);
        waitForRendering(appWindow.contentItem);
        mouseClick(namedItem(liveOftcConversation("#build")));
        tryCompare(appWindow, "currentConversation", "#build");
        compare(appWindow.connection.isNetworkCollapsed(seed.omarchyNetworkId), true);

        keyClick(Qt.Key_A, Qt.AltModifier);

        tryCompare(appWindow, "currentConversation", "#ricing");
        compare(appWindow.currentConversationId, seed.omarchyNetworkId + "\n#ricing");
        compare(appWindow.connection.isNetworkCollapsed(seed.omarchyNetworkId), false);
        verify(sidebarRowShown(liveConversation("#ricing")));
    }

    function test_moveFocusedNetworkWithShortcut() {
        openSeededAppWindow();
        keyClick(Qt.Key_Right, Qt.AltModifier);
        compare(appWindow.sidebarNetworkFocusId, seed.oftcNetworkId);
        compare(appWindow.sidebarNetworkSections()[0].networkId, seed.omarchyNetworkId);
        compare(appWindow.sidebarNetworkSections()[1].networkId, seed.oftcNetworkId);

        keyClick(Qt.Key_Up, Qt.AltModifier | Qt.ShiftModifier);

        tryVerify(function() {
            var sections = appWindow.sidebarNetworkSections();
            return sections.length === 2
                && sections[0].networkId === seed.oftcNetworkId
                && sections[1].networkId === seed.omarchyNetworkId
                && appWindow.sidebarNetworkFocusId === seed.oftcNetworkId;
        });
        compare(liveHeader(seed.oftcNetworkId).parent.headerFocused, true);
        saveScreenshot("network-moved-up");

        keyClick(Qt.Key_Down, Qt.AltModifier | Qt.ShiftModifier);

        tryVerify(function() {
            var sections = appWindow.sidebarNetworkSections();
            return sections.length === 2
                && sections[0].networkId === seed.omarchyNetworkId
                && sections[1].networkId === seed.oftcNetworkId
                && appWindow.sidebarNetworkFocusId === seed.oftcNetworkId;
        });
        compare(liveHeader(seed.oftcNetworkId).parent.headerFocused, true);
    }

    function test_collapsedSectionKeepsHeaderWalkAndServerListToggle() {
        openSeededAppWindow();
        appWindow.connection.setNetworkCollapsed(seed.oftcNetworkId, true);
        waitForRendering(appWindow.contentItem);
        compare(appWindow.connection.isNetworkCollapsed(seed.oftcNetworkId), true);

        keyClick(Qt.Key_Right, Qt.AltModifier);
        compare(appWindow.sidebarNetworkFocusId, seed.oftcNetworkId);
        verify(liveHeader(seed.oftcNetworkId).visible);
        compare(liveHeader(seed.oftcNetworkId).parent.headerFocused, true);

        keyClick(Qt.Key_Right, Qt.AltModifier);
        compare(appWindow.sidebarNetworkFocusId, seed.omarchyNetworkId);
        compare(liveHeader(seed.omarchyNetworkId).parent.headerFocused, true);

        var sidebar = item("serverList");
        var expandedWidth = sidebar.width;
        keyClick(Qt.Key_S, Qt.ControlModifier | Qt.ShiftModifier);
        tryCompare(sidebar, "width", 0);
        compare(appWindow.sidebarNetworkFocusId, "");
        keyClick(Qt.Key_S, Qt.ControlModifier | Qt.ShiftModifier);
        tryCompare(sidebar, "width", expandedWidth);
        compare(appWindow.connection.isNetworkCollapsed(seed.oftcNetworkId), true);
    }

    function test_emptyNetworkHeaderIsAKeyboardStop() {
        restoreNamedConnection();
        namedNetworks.append({
            networkId: "oftc",
            displayName: "irc.oftc.net",
            stored: true,
            selected: false,
            iconColor: 1,
            iconUrl: "",
            collapsed: false
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
            compare(findChild(window, "connectionName").text, "");
            namedNetworks.setProperty(1, "displayName", "irc.oftc.net");
            namedConnection.displayName = "irc.oftc.net";
            namedConnection.host = "irc.oftc.net";
            namedConnection.name = "irc.oftc.net";
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
            iconColor: 1,
            iconUrl: "",
            collapsed: false
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

        var mark = findNamedIn(window, "networkUnreadMark-libera");
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
        waitForNewMessage(messages, previousCount, "hello oak");
        compare(field(messages.model, messages.model.rowCount() - 1, "author"), "oak");
        compare(field(messages.model, messages.model.rowCount() - 1, "body"), "hello oak");
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
        waitForNewMessage(messages, previousCount, "secret to ness");

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
            iconColor: 1,
            iconUrl: "",
            collapsed: false
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
