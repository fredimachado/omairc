import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Layouts
import QtQuick.Window

ApplicationWindow {
    id: win

    required property var backend
    property var irc: null
    property var connection: null
    property var slashCommands: null
    property bool connectionSheetOpen: false
    property bool connectionRemoveArmed: false
    property bool connectionPasswordEdited: false

    objectName: "omaircWindow"
    width: 1180
    height: 760
    minimumWidth: 760
    minimumHeight: 540
    visible: true
    title: consoleVisible ? statusTitleText() : conversationTitleText()

    readonly property string mockOmarchyId: "mock-omarchy"
    readonly property string mockOftcId: "mock-oftc"
    readonly property bool darkMode: backend.darkMode
    readonly property real textScale: backend.textScale
    readonly property color pageColor: backend.themeBackground
    readonly property color inkColor: backend.themeForeground
    readonly property color accentColor: backend.themeAccent
    readonly property color selectionColor: backend.themeSelection
    readonly property color panelColor: mixColors(pageColor, inkColor, darkMode ? 0.035 : 0.025)
    readonly property color raisedColor: mixColors(pageColor, inkColor, darkMode ? 0.075 : 0.055)
    readonly property color hoverColor: mixColors(pageColor, inkColor, darkMode ? 0.10 : 0.075)
    readonly property color dividerColor: mixColors(pageColor, inkColor, darkMode ? 0.13 : 0.11)
    readonly property color mutedColor: mixColors(pageColor, inkColor, darkMode ? 0.52 : 0.47)

    property string mockCurrentConversation: "#omarchy"
    property string mockSelectedNetworkId: mockOmarchyId
    property string mockCurrentTopic: "A cozy corner for Omarchy users and builders."
    property var mockActiveMessages: omarchyMessages
    property bool membersVisible: true
    property bool mockStatusOpen: false
    property string mockStatusNetworkId: mockOmarchyId
    property bool shortcutsSheetEscapeGuard: false
    readonly property bool shortcutOverlayOpen: shortcutsSheet.opened
    readonly property var networkConsole: irc
        ? (irc.statusConsole ? irc.statusConsole : irc.console)
        : null
    readonly property bool consoleVisible: networkConsole
        ? (networkConsole.open || irc.selectedTarget.length === 0)
        : mockStatusOpen
    onConsoleVisibleChanged: {
        resetNickComplete();
        if (!abandonFind())
            stashComposerDraft();
        restoreComposerDraft();
        resetComposerHistoryBrowse();
        if (win.slashCommands)
            win.slashCommands.sync(composer.text, consoleVisible);
        if (!consoleVisible)
            return;
        Qt.callLater(function() {
            consoleList.pinToEnd();
            composer.forceActiveFocus();
        });
    }
    readonly property string currentConversation: irc ? irc.selectedTarget : mockCurrentConversation
    readonly property string currentNetworkId: irc
        ? irc.focusedNetworkId
        : (consoleVisible ? mockStatusNetworkId : mockSelectedNetworkId)
    readonly property string currentConversationId: irc
        ? irc.selectedConversationId
        : (mockCurrentConversation.length > 0
            ? mockSelectedNetworkId + "\n" + mockCurrentConversation
            : "")
    onCurrentConversationIdChanged: {
        resetNickComplete();
        if (!abandonFind())
            stashComposerDraft();
        restoreComposerDraft();
        resetComposerHistoryBrowse();
    }
    onCurrentNetworkIdChanged: {
        if (!consoleVisible)
            return;
        resetNickComplete();
        if (!abandonFind())
            stashComposerDraft();
        restoreComposerDraft();
        resetComposerHistoryBrowse();
        if (win.slashCommands)
            win.slashCommands.sync(composer.text, consoleVisible);
    }
    onCurrentConversationChanged: {
        Qt.callLater(function() {
            if (membersList)
                membersList.currentIndex = 0;
        });
    }
    readonly property string currentTopic: irc
        ? (irc.selectedTarget.length > 0
            ? irc.topic
            : (irc.lastError.length > 0 ? irc.lastError : irc.connectionStatus))
        : mockCurrentTopic
    readonly property var activeMessages: irc ? irc.messages : mockActiveMessages
    readonly property bool currentConversationIsChannel: irc
        ? irc.isChannel : currentConversation.charAt(0) === "#"
    readonly property int currentPeopleCount: irc
        ? irc.peopleCount : peopleCountFor(currentConversation, mockSelectedNetworkId)
    readonly property bool memberStatusVisible: !irc || irc.hasMemberStatus
    readonly property bool awayPresenceVisible: !irc || irc.hasAwayPresence
    readonly property bool selfAway: irc ? irc.selfAway : false
    readonly property bool typingVisible: !irc || irc.hasTyping
    readonly property var typingNicks: irc ? irc.typingNicks : mockTypingNicks()
    property int typingPulse: 0
    readonly property string selfNick: {
        if (irc) {
            var live = irc.currentNick
            if (live && live.length > 0)
                return live
            if (connection && connection.nick && connection.nick.length > 0)
                return connection.nick
            return ""
        }
        if (currentNetworkId === mockOftcId)
            return "oak"
        return "fred"
    }
    readonly property bool connectionOverlayVisible: connection
        && (connection.setupRequired || connectionSheetOpen)

    property var composerHistories: ({})
    property var composerDrafts: ({})
    property string composerDraftKey: ""
    property var mockDirectMessageModels: ({})
    property bool findActive: false
    property int findIndex: -1
    property int composerHistoryIndex: -1
    property string composerHistoryDraft: ""
    property string nickCompletePrefix: ""
    property var nickCompleteMatches: []
    property int nickCompleteIndex: -1
    property int nickCompleteOrigin: -1
    property string lastOpenedUrl: ""
    property bool suppressExternalUrlOpen: false
    property var lastNotification: null
    property bool suppressDesktopNotification: false
    property var allowedUrlSchemes: ({ "http": true, "https": true })

    Material.theme: darkMode ? Material.Dark : Material.Light
    Material.accent: accentColor
    color: pageColor

    Timer {
        interval: 320
        repeat: true
        running: win.typingVisible && win.typingNicks && win.typingNicks.length > 0
        onTriggered: win.typingPulse = (win.typingPulse + 1) % 3
    }

    component PlainUrlHit: MouseArea {
        required property Item edit

        objectName: "urlHit"
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton
        cursorShape: win.httpUrlAt(edit.text, edit.positionAt(mouseX, mouseY)).length > 0
            ? Qt.PointingHandCursor
            : Qt.IBeamCursor
        onPressed: function(mouse) {
            var url = win.httpUrlAt(edit.text, edit.positionAt(mouse.x, mouse.y));
            if (url.length === 0)
                mouse.accepted = false;
        }
        onClicked: function(mouse) {
            win.openAllowedUrl(win.httpUrlAt(edit.text, edit.positionAt(mouse.x, mouse.y)));
        }
    }

    component TypingDots: Row {
        id: dots

        property color ink: win.mutedColor
        property int pixelSize: win.scaledSize(12)

        Accessible.ignored: true
        spacing: 0

        Repeater {
            model: 3
            Text {
                text: "."
                color: dots.ink
                opacity: win.typingPulse === index ? 1 : 0.28
                font.family: "iA Writer Mono S"
                font.pixelSize: dots.pixelSize
            }
        }
    }

    function scaledSize(pixels) {
        return Math.max(1, Math.round(pixels * textScale));
    }

    function mixColors(base, tint, amount) {
        return Qt.rgba(
            base.r + (tint.r - base.r) * amount,
            base.g + (tint.g - base.g) * amount,
            base.b + (tint.b - base.b) * amount,
            1);
    }

    function focusedNetworkDisplayName() {
        if (!irc) {
            if (currentNetworkId === mockOftcId)
                return "irc.oftc.net";
            return "Omarchy IRC";
        }
        if (connection && connection.networks) {
            var model = connection.networks;
            var id = currentNetworkId;
            for (var row = 0; row < model.rowCount(); ++row) {
                var idx = model.index(row, 0);
                if (model.data(idx, Qt.UserRole + 1) === id)
                    return model.data(idx, Qt.UserRole + 2) || "";
            }
        }
        if (connection && connection.displayName
                && connection.selectedNetworkId === currentNetworkId)
            return connection.displayName;
        return "";
    }

    function duplicateTargetName(name) {
        if (name.length === 0)
            return false;
        var rows = sidebarConversationRows();
        var seen = 0;
        for (var index = 0; index < rows.length; ++index) {
            if (rows[index].conversationName === name)
                seen += 1;
            if (seen > 1)
                return true;
        }
        return false;
    }

    function conversationTitleText() {
        if (currentConversation.length === 0)
            return "Omairc";
        var networkName = focusedNetworkDisplayName();
        if (duplicateTargetName(currentConversation) && networkName.length > 0)
            return currentConversation + " · " + networkName + " - Omairc";
        return currentConversation + " - Omairc";
    }

    function statusTitleText() {
        var networkName = focusedNetworkDisplayName();
        if (networkName.length > 0)
            return networkName + " Status";
        if (connection && connection.displayName.length > 0)
            return connection.displayName + " Status";
        return "Status";
    }

    function nickColor(nick) {
        var palette = [
            accentColor,
            darkMode ? "#c099ff" : "#7950b8",
            darkMode ? "#7fc8a9" : "#237a58",
            darkMode ? "#efb366" : "#a45f14",
            darkMode ? "#ed8f9d" : "#b44355"
        ];
        var hash = 0;
        for (var index = 0; index < nick.length; ++index)
            hash = (hash + nick.charCodeAt(index)) % palette.length;
        return palette[hash];
    }

    function initials(nick) {
        return nick.length > 0 ? nick.charAt(0).toUpperCase() : "?";
    }

    function plainIrcText(text) {
        return text.replace(/\x03(?:\d{1,2}(?:,\d{1,2})?)?/g, "")
            .replace(/[\x02\x0f\x16\x1d\x1f]/g, "");
    }

    function transcriptRowCount(model) {
        if (!model)
            return 0;
        if (typeof model.count === "number")
            return model.count;
        if (typeof model.rowCount === "function")
            return model.rowCount();
        return 0;
    }

    function transcriptField(model, row, name) {
        if (!model || row < 0 || row >= transcriptRowCount(model))
            return "";
        if (typeof model.get === "function") {
            var item = model.get(row);
            if (!item)
                return "";
            var value = item[name];
            return value == null ? "" : String(value);
        }
        if (typeof model.data !== "function" || typeof model.index !== "function")
            return "";
        // MessageListModel roles are UserRole+1..+4: author, time, body, kind.
        var offset = name === "author" ? 1
            : name === "time" ? 2
            : name === "body" ? 3
            : name === "kind" ? 4
            : -1;
        if (offset < 0)
            return "";
        var value = model.data(model.index(row, 0), Qt.UserRole + offset);
        return value == null ? "" : String(value);
    }

    function continuesMessageGroup(model, row, author, time, kind) {
        if (kind === "event" || row <= 0 || author.length === 0 || time.length === 0)
            return false;
        var previousKind = transcriptField(model, row - 1, "kind");
        if (previousKind === "event" || previousKind.length === 0)
            return false;
        return transcriptField(model, row - 1, "author") === author
            && transcriptField(model, row - 1, "time") === time;
    }

    function isAllowedHttpUrl(url) {
        if (!url)
            return false;
        var colon = url.indexOf(":");
        if (colon <= 0)
            return false;
        var scheme = url.substring(0, colon).toLowerCase();
        if (!allowedUrlSchemes[scheme])
            return false;
        if (url.substring(colon, colon + 3) !== "://")
            return false;
        var rest = url.substring(colon + 3);
        if (rest.length === 0)
            return false;
        if (url.indexOf("\n") >= 0 || url.indexOf("\r") >= 0 || url.indexOf(" ") >= 0)
            return false;
        return true;
    }

    function httpUrlAt(text, index) {
        if (!text || index < 0 || index >= text.length)
            return "";
        var re = /https?:\/\/[^\s<>"']+/gi;
        var match;
        while ((match = re.exec(text)) !== null) {
            var start = match.index;
            var raw = match[0].replace(/[.,;:!?)\]>]+$/, "");
            var end = start + raw.length;
            if (index >= start && index < end && isAllowedHttpUrl(raw))
                return raw;
        }
        return "";
    }

    function openAllowedUrl(url) {
        if (!isAllowedHttpUrl(url))
            return false;
        lastOpenedUrl = url;
        if (!suppressExternalUrlOpen)
            Qt.openUrlExternally(url);
        return true;
    }

    function openHttpUrlAt(text, index) {
        return openAllowedUrl(httpUrlAt(text, index));
    }

    function notifyMentionIfUnfocused(windowActive, author, body) {
        if (windowActive)
            return;
        var text = plainIrcText(body);
        lastNotification = { author: author, body: text };
        if (suppressDesktopNotification)
            return;
        backend.notifyDesktop(author, text);
    }

    function topicFor(name, networkId) {
        var id = networkId || mockSelectedNetworkId;
        if (id === mockOftcId) {
            if (name === "#omarchy")
                return "A different #omarchy, hosted on OFTC.";
            if (name === "#lab")
                return "Build lab for packaging and CI.";
            if (name === "#build")
                return "Nightly builds and failing tests.";
            if (name.charAt(0) !== "#")
                return "Direct message with " + name;
            return "A local mock conversation.";
        }
        if (name === "#omarchy")
            return "A cozy corner for Omarchy users and builders.";
        if (name === "#desktop")
            return "Desktops should feel personal, fast, and calm.";
        if (name === "#ricing")
            return "Themes, type, wallpapers, and the tiny details.";
        if (name === "#help")
            return "Ask a clear question. Share what you already tried.";
        if (name.charAt(0) !== "#")
            return "Direct message with " + name;
        return "A local mock conversation.";
    }

    function peopleCountFor(name, networkId) {
        var id = networkId || mockSelectedNetworkId;
        if (id === mockOftcId) {
            if (name === "#omarchy")
                return 4;
            if (name === "#lab")
                return 6;
            if (name === "#build")
                return 3;
            return 2;
        }
        if (name === "#omarchy")
            return 12;
        if (name === "#desktop")
            return 8;
        if (name === "#ricing")
            return 10;
        if (name === "#help")
            return 5;
        return 2;
    }

    function memberDataFor(index) {
        if (mockSelectedNetworkId === mockOftcId)
            return oftcMembersModel.get(index);
        if (currentConversation === "anna")
            return membersModel.get(index === 0 ? 0 : 4);
        if (currentConversation === "dax")
            return membersModel.get(index === 0 ? 1 : 4);
        return membersModel.get(index);
    }

    function mockTypingNicks() {
        if (mockSelectedNetworkId === mockOftcId)
            return [];
        if (currentConversation === "#omarchy" || currentConversation === "anna")
            return ["anna"];
        return [];
    }

    function messagesFor(name, networkId) {
        var id = networkId || mockSelectedNetworkId;
        if (id === mockOftcId) {
            if (name === "#lab")
                return labMessages;
            if (name === "#build")
                return buildMessages;
            if (name === "rio")
                return rioMessages;
            if (name.charAt(0) === "#")
                return oftcOmarchyMessages;
            return mockDirectMessages(name, id);
        }
        if (name === "#desktop")
            return desktopMessages;
        if (name === "#ricing")
            return ricingMessages;
        if (name === "#help")
            return helpMessages;
        if (name === "anna")
            return annaMessages;
        if (name === "dax")
            return daxMessages;
        if (name === "mira")
            return miraMessages;
        if (name === "sol")
            return solMessages;
        if (name === "kai")
            return kaiMessages;
        if (name === "nora")
            return noraMessages;
        if (name === "teo")
            return teoMessages;
        if (name === "lena")
            return lenaMessages;
        if (name === "sam")
            return samMessages;
        if (name === "ivy")
            return ivyMessages;
        if (name === "max")
            return maxMessages;
        return omarchyMessages;
    }

    function openDirectMessage(nick) {
        if (irc) {
            irc.openDirectMessage(nick);
            Qt.callLater(function() {
                messageList.pinToEnd();
                composer.forceActiveFocus();
            });
            return;
        }
        if (mockDirectExists(nick, mockSelectedNetworkId)) {
            selectConversation(nick, mockSelectedNetworkId);
            return;
        }

        directConversations.append({
            conversation: nick,
            networkId: mockSelectedNetworkId,
            directUnread: 0,
            directMention: false
        });
        selectConversation(nick, mockSelectedNetworkId);
    }

    function closeDirectMessage() {
        if (irc) {
            irc.closeDirectMessage();
            Qt.callLater(function() {
                messageList.pinToEnd();
                composer.forceActiveFocus();
            });
            return;
        }
        if (currentConversation.length === 0 || currentConversation.charAt(0) === "#")
            return;

        var rows = sidebarConversationRows();
        var current = -1;
        for (var index = 0; index < rows.length; ++index) {
            if (rows[index].conversationId === currentConversationId) {
                current = index;
                break;
            }
        }

        var nextRow = neighborAfterDrop(rows, current);

        var closing = currentConversation;
        var closingNetwork = mockSelectedNetworkId;
        for (var removeIndex = 0; removeIndex < directConversations.count; ++removeIndex) {
            if (directConversations.get(removeIndex).conversation === closing
                    && directConversations.get(removeIndex).networkId === closingNetwork) {
                directConversations.remove(removeIndex);
                break;
            }
        }

        if (nextRow) {
            selectConversation(nextRow.conversationName, rowNetworkId(nextRow));
            return;
        }
        mockStatusOpen = true;
        mockStatusNetworkId = closingNetwork;
        mockCurrentConversation = "";
    }

    function focusMembersList() {
        membersVisible = true;
        Qt.callLater(function() {
            var last = memberCount() - 1;
            if (membersList.currentIndex < 0 || membersList.currentIndex > last)
                membersList.currentIndex = last < 0 ? -1 : 0;
            membersList.forceActiveFocus();
        });
    }

    function activateFocusedMember() {
        var nick = memberNickAt(membersList.currentIndex);
        if (nick.length === 0 || nick === win.selfNick)
            return;
        win.openDirectMessage(nick);
    }

    function markDirectConversationRead(name, networkId) {
        var id = networkId || mockSelectedNetworkId;
        for (var index = 0; index < directConversations.count; ++index) {
            if (directConversations.get(index).conversation === name
                    && directConversations.get(index).networkId === id) {
                directConversations.setProperty(index, "directUnread", 0);
                directConversations.setProperty(index, "directMention", false);
                return;
            }
        }
    }

    function selectConversation(name, networkId) {
        var id = networkId && networkId.length
            ? networkId
            : (irc ? irc.selectedNetworkId : mockSelectedNetworkId);
        if (irc) {
            irc.selectConversation(id, name);
            Qt.callLater(function() {
                messageList.pinToEnd();
                composer.forceActiveFocus();
            });
            return;
        }
        mockStatusOpen = false;
        mockSelectedNetworkId = id;
        if (name.charAt(0) !== "#")
            markDirectConversationRead(name, id);
        mockCurrentConversation = name;
        mockCurrentTopic = topicFor(name, id);
        mockActiveMessages = messagesFor(name, id);
        Qt.callLater(function() {
            messageList.pinToEnd();
            composer.forceActiveFocus();
        });
    }

    function openNetworkStatus(networkId) {
        if (irc) {
            irc.openStatus(networkId);
            Qt.callLater(function() {
                consoleList.pinToEnd();
                composer.forceActiveFocus();
            });
            return;
        }
        mockStatusNetworkId = networkId;
        mockStatusOpen = true;
        Qt.callLater(function() {
            consoleList.pinToEnd();
            composer.forceActiveFocus();
        });
    }

    function sidebarConversationRows() {
        var rows = [];

        function appendSection(section) {
            if (!section || section.visible === false)
                return;
            var kids = section.children;
            for (var index = 0; index < kids.length; ++index) {
                var child = kids[index];
                if (child && child.conversationName !== undefined && child.activate
                        && child.visible && child.height > 0)
                    rows.push(child);
            }
        }

        if (irc && connection) {
            for (var liveIndex = 0; liveIndex < liveNetworkRepeater.count; ++liveIndex)
                appendSection(liveNetworkRepeater.itemAt(liveIndex));
        } else if (irc) {
            appendSection(liveFallbackSection);
        } else {
            appendSection(mockOmarchySection);
            appendSection(mockOftcSection);
        }
        return rows;
    }

    function mockDirectExists(nick, networkId) {
        if (networkId === mockOftcId && nick === "rio")
            return true;
        for (var index = 0; index < directConversations.count; ++index) {
            if (directConversations.get(index).conversation === nick
                    && directConversations.get(index).networkId === networkId)
                return true;
        }
        return false;
    }

    function sectionHasDirects(networkId) {
        if (!irc) {
            if (networkId === mockOftcId)
                return true;
            for (var mockIndex = 0; mockIndex < directConversations.count; ++mockIndex) {
                if (directConversations.get(mockIndex).networkId === networkId)
                    return true;
            }
            return false;
        }
        var model = irc.conversations;
        if (!model)
            return false;
        for (var row = 0; row < model.rowCount(); ++row) {
            var idx = model.index(row, 0);
            if (model.data(idx, Qt.UserRole + 4) === true
                    && model.data(idx, Qt.UserRole + 5) === networkId)
                return true;
        }
        return false;
    }

    function stepConversation(delta) {
        var rows = sidebarConversationRows();
        if (rows.length === 0)
            return;

        var current = -1;
        for (var index = 0; index < rows.length; ++index) {
            if (rows[index].conversationId === currentConversationId) {
                current = index;
                break;
            }
        }

        var nextIndex = current < 0
            ? (delta > 0 ? 0 : rows.length - 1)
            : (current + delta + rows.length) % rows.length;
        rows[nextIndex].activate();
    }

    function revealSidebarRow(row) {
        if (!row || !sidebarScroll)
            return;
        var mapped = row.mapToItem(sidebarScroll.contentItem, 0, 0);
        var top = mapped.y;
        var bottom = top + row.height;
        if (top < sidebarScroll.contentY)
            sidebarScroll.contentY = Math.max(0, top);
        else if (bottom > sidebarScroll.contentY + sidebarScroll.height)
            sidebarScroll.contentY = Math.max(0, bottom - sidebarScroll.height);
    }

    function mockDirectMessages(nick, networkId) {
        var key = networkId + "\n" + nick;
        if (mockDirectMessageModels[key])
            return mockDirectMessageModels[key];
        var model = Qt.createQmlObject("import QtQuick; ListModel {}", win);
        model.append({
            author: "",
            time: "",
            body: "This is the beginning of your conversation with " + nick + ".",
            kind: "event"
        });
        mockDirectMessageModels[key] = model;
        return model;
    }

    function jumpToNextUnread() {
        var rows = sidebarConversationRows();
        if (rows.length === 0)
            return;

        var current = -1;
        for (var index = 0; index < rows.length; ++index) {
            if (rows[index].conversationId === currentConversationId) {
                current = index;
                break;
            }
        }

        var start = current < 0 ? 0 : (current + 1) % rows.length;
        var mentionRow = null;
        var unreadRow = null;
        for (var step = 0; step < rows.length; ++step) {
            var rowIndex = (start + step) % rows.length;
            if (rowIndex === current)
                continue;

            var row = rows[rowIndex];
            if (mentionRow === null && row.mention === true)
                mentionRow = row;
            if (unreadRow === null && row.unread > 0)
                unreadRow = row;
            if (mentionRow)
                break;
        }

        var target = mentionRow ? mentionRow : unreadRow;
        if (target)
            target.activate();
    }

    function composerHistoryKey() {
        if (consoleVisible) {
            var statusNetwork = irc ? irc.focusedNetworkId : mockStatusNetworkId;
            return "status\n" + statusNetwork;
        }
        return currentConversationId;
    }

    function unsentComposerText() {
        if (composerHistoryIndex >= 0)
            return composerHistoryDraft;
        return composer.text;
    }

    function rowNetworkId(row) {
        if (!row)
            return "";
        var id = row.conversationId || "";
        var sep = id.indexOf("\n");
        if (sep > 0)
            return id.substring(0, sep);
        return row.networkId || "";
    }

    function neighborAfterDrop(rows, current) {
        if (current < 0)
            return rows.length > 0 ? rows[rows.length - 1] : null;
        var network = rowNetworkId(rows[current]);
        var index;
        for (index = current + 1; index < rows.length; ++index) {
            if (rowNetworkId(rows[index]) === network)
                return rows[index];
        }
        for (index = current - 1; index >= 0; --index) {
            if (rowNetworkId(rows[index]) === network)
                return rows[index];
        }
        if (current + 1 < rows.length)
            return rows[current + 1];
        if (current > 0)
            return rows[current - 1];
        return null;
    }

    function stashComposerDraft() {
        if (!composer)
            return;
        if (composerDraftKey.length === 0)
            return;
        composerDrafts[composerDraftKey] = unsentComposerText();
    }

    function restoreComposerDraft() {
        if (!composer)
            return;
        var key = composerHistoryKey();
        composerDraftKey = key;
        composer.text = composerDrafts[key] || "";
        composer.cursorPosition = composer.text.length;
    }

    function abandonFind() {
        if (!findActive)
            return false;
        findActive = false;
        findIndex = -1;
        return true;
    }

    function leaveFind() {
        if (!abandonFind())
            return;
        restoreComposerDraft();
        composer.forceActiveFocus();
    }

    function transcriptRowText(model, row) {
        if (!model || row < 0)
            return "";
        if (typeof model.get === "function") {
            var rowData = model.get(row);
            if (!rowData)
                return "";
            if (consoleVisible)
                return rowData.text || "";
            return rowData.body || "";
        }
        return model.data(model.index(row, 0), Qt.UserRole + 3) || "";
    }

    function findNextMatch(fromStart) {
        var query = composer.text;
        if (query.length === 0)
            return -1;
        var list = consoleVisible ? consoleList : messageList;
        if (!list || list.count <= 0)
            return -1;
        var needle = query.toLowerCase();
        var start = fromStart ? -1 : findIndex;
        var count = list.count;
        for (var step = 1; step <= count; ++step) {
            var index = (start + step) % count;
            var hay = plainIrcText(transcriptRowText(list.model, index)).toLowerCase();
            if (hay.indexOf(needle) >= 0)
                return index;
        }
        return -1;
    }

    function revealFindMatch(index) {
        var list = consoleVisible ? consoleList : messageList;
        if (!list)
            return;
        list.stick = list.stickDetached;
        list.pinning = true;
        var generation = ++list.pinGeneration;
        list.positionViewAtIndex(index, ListView.Beginning);
        Qt.callLater(function() {
            if (generation !== list.pinGeneration)
                return;
            list.pinning = false;
            list.adoptViewport();
        });
    }

    function advanceFind(fromStart) {
        if (composer.text.length === 0) {
            findIndex = -1;
            return;
        }
        var index = findNextMatch(fromStart);
        if (index < 0)
            return;
        findIndex = index;
        revealFindMatch(index);
    }

    function beginOrAdvanceFind() {
        composer.forceActiveFocus();
        if (!findActive) {
            stashComposerDraft();
            findActive = true;
            findIndex = -1;
            composer.selectAll();
            advanceFind(true);
            return;
        }
        advanceFind(false);
    }

    function resetComposerHistoryBrowse() {
        composerHistoryIndex = -1;
        composerHistoryDraft = "";
    }

    function rememberSentComposerLine(text) {
        resetNickComplete();
        resetComposerHistoryBrowse();
        var key = composerHistoryKey();
        var lines = (composerHistories[key] || []).slice();
        lines.push(text);
        if (lines.length > 50)
            lines.shift();
        composerHistories[key] = lines;
    }

    function recallComposerHistory(delta) {
        var lines = composerHistories[composerHistoryKey()] || [];
        if (composer.text.length === 0 && lines.length === 0)
            return false;

        if (composerHistoryIndex < 0) {
            if (delta > 0 || lines.length === 0)
                return false;
            composerHistoryDraft = composer.text;
            composerHistoryIndex = lines.length;
        }

        var next = composerHistoryIndex + delta;
        if (next < 0)
            next = 0;
        if (next >= lines.length) {
            var draft = composerHistoryDraft;
            resetComposerHistoryBrowse();
            composer.text = draft;
            composer.cursorPosition = composer.text.length;
            return true;
        }

        composerHistoryIndex = next;
        composer.text = lines[next];
        composer.cursorPosition = composer.text.length;
        return true;
    }

    function resetNickComplete() {
        nickCompletePrefix = "";
        nickCompleteMatches = [];
        nickCompleteIndex = -1;
        nickCompleteOrigin = -1;
    }

    function liveMemberNick(model, row) {
        if (!model)
            return "";
        if (typeof model.get === "function") {
            var rowData = model.get(row);
            return rowData && rowData.nick ? rowData.nick : "";
        }
        return model.data(model.index(row, 0), Qt.UserRole + 1) || "";
    }

    function liveMemberCount(model) {
        if (!model)
            return 0;
        if (model.count !== undefined)
            return model.count;
        return model.rowCount();
    }

    function memberCount() {
        if (irc)
            return liveMemberCount(irc.members);
        return currentPeopleCount;
    }

    function memberNickAt(index) {
        if (index < 0 || index >= memberCount())
            return "";
        if (irc)
            return liveMemberNick(irc.members, index);
        var mock = memberDataFor(index);
        return mock && mock.nick ? mock.nick : "";
    }

    function nickCompleteCandidates() {
        if (consoleVisible)
            return [];
        if (!currentConversationIsChannel)
            return currentConversation.length > 0 ? [currentConversation] : [];

        var nicks = [];
        if (irc) {
            var model = irc.members;
            var count = liveMemberCount(model);
            for (var row = 0; row < count; ++row) {
                var liveNick = liveMemberNick(model, row);
                if (liveNick.length > 0)
                    nicks.push(liveNick);
            }
            return nicks;
        }

        for (var index = 0; index < currentPeopleCount; ++index) {
            var mock = memberDataFor(index);
            var mockNick = mock && mock.nick ? mock.nick : "";
            if (mockNick.length > 0)
                nicks.push(mockNick);
        }
        return nicks;
    }

    function nickMatchesForPrefix(prefix) {
        var lower = prefix.toLowerCase();
        var decorated = [];
        var candidates = nickCompleteCandidates();
        for (var index = 0; index < candidates.length; ++index) {
            var nick = candidates[index];
            if (nick.toLowerCase().indexOf(lower) !== 0)
                continue;
            decorated.push({ nick: nick, order: index });
        }
        decorated.sort(function(left, right) {
            if (left.nick < right.nick)
                return -1;
            if (left.nick > right.nick)
                return 1;
            return left.order - right.order;
        });
        var matches = [];
        for (var match = 0; match < decorated.length; ++match)
            matches.push(decorated[match].nick);
        return matches;
    }

    function applyNickComplete() {
        var nick = nickCompleteMatches[nickCompleteIndex];
        var insertion = nick + (nickCompleteOrigin === 0 ? ": " : " ");
        var after = composer.text.substring(composer.cursorPosition);
        composer.text = composer.text.substring(0, nickCompleteOrigin) + insertion + after;
        composer.cursorPosition = nickCompleteOrigin + insertion.length;
    }

    function completeNick() {
        if (nickCompleteMatches.length > 0 && nickCompleteIndex >= 0) {
            nickCompleteIndex = (nickCompleteIndex + 1) % nickCompleteMatches.length;
            applyNickComplete();
            return;
        }

        var cursor = composer.cursorPosition;
        var origin = composer.text.substring(0, cursor).lastIndexOf(" ") + 1;
        var token = composer.text.substring(origin, cursor);
        if (token.length === 0)
            return;

        var matches = nickMatchesForPrefix(token);
        if (matches.length === 0)
            return;

        nickCompletePrefix = token;
        nickCompleteMatches = matches;
        nickCompleteIndex = 0;
        nickCompleteOrigin = origin;
        applyNickComplete();
    }

    function composerHasPlainModifier(event) {
        return event.modifiers === Qt.NoModifier
            || event.modifiers === Qt.KeypadModifier;
    }

    function transcriptIndexAt(list, y) {
        var x = Math.max(1, list.width / 2);
        var index = list.indexAt(x, y);
        if (index >= 0)
            return index;
        return list.indexAt(x, y + 8);
    }

    function scrollTranscript(direction) {
        var list = consoleVisible ? consoleList : messageList;
        if (!list || list.count <= 0)
            return;

        var first = transcriptIndexAt(list, list.contentY + 1);
        var last = transcriptIndexAt(list, list.contentY + Math.max(1, list.height - 1));
        if (first < 0)
            first = 0;
        if (last < 0)
            last = list.count - 1;
        if (last < first)
            last = first;

        var page = Math.max(1, Math.round((last - first + 1) * 0.8));
        if (direction < 0)
            list.positionViewAtIndex(Math.max(0, first - page), ListView.Beginning);
        else
            list.positionViewAtIndex(Math.min(list.count - 1, last + page), ListView.End);
        Qt.callLater(function() { list.adoptViewport(); });
    }

    function sendMessage() {
        if (findActive) {
            advanceFind(false);
            return;
        }

        var original = composer.text.trim();
        if (original.length === 0)
            return;

        if (consoleVisible) {
            if (irc) {
                if (networkConsole.submit(original)) {
                    rememberSentComposerLine(original);
                    composer.clear();
                }
                consoleList.pinToEnd();
                return;
            }
            var statusLog = mockStatusNetworkId === mockOftcId
                ? oftcStatusMessages : mockStatusMessages;
            statusLog.append({
                time: Qt.formatTime(new Date(), "hh:mm:ss"),
                label: "command",
                text: original,
                source: "local",
                severity: "info"
            });
            rememberSentComposerLine(original);
            composer.clear();
            consoleList.pinToEnd();
            return;
        }

        if (irc) {
            if (irc.sendMessage(original)) {
                rememberSentComposerLine(original);
                composer.clear();
            }
            messageList.pinToEnd();
            return;
        }

        var kind = "message";
        var body = original;
        if (body.indexOf("/me ") === 0) {
            body = win.selfNick + " " + body.substring(4);
            kind = "action";
        }

        activeMessages.append({
            author: win.selfNick,
            time: Qt.formatTime(new Date(), "hh:mm"),
            body: body,
            kind: kind
        });
        rememberSentComposerLine(original);
        composer.clear();
        messageList.pinToEnd();
    }

    Shortcut {
        sequence: "Ctrl+Q"
        context: Qt.ApplicationShortcut
        onActivated: win.close()
    }

    Shortcut {
        sequence: "Ctrl+L"
        context: Qt.ApplicationShortcut
        enabled: !win.shortcutOverlayOpen
        onActivated: composer.forceActiveFocus()
    }

    Shortcut {
        sequence: "Ctrl+F"
        context: Qt.ApplicationShortcut
        enabled: !win.connectionOverlayVisible && !win.shortcutOverlayOpen
        onActivated: win.beginOrAdvanceFind()
    }

    Shortcut {
        sequence: "Ctrl+Shift+M"
        context: Qt.ApplicationShortcut
        enabled: currentConversationIsChannel && !consoleVisible && !win.shortcutOverlayOpen
        onActivated: membersVisible = !membersVisible
    }

    Shortcut {
        sequence: "Ctrl+Shift+P"
        context: Qt.ApplicationShortcut
        enabled: currentConversationIsChannel && !consoleVisible && !win.shortcutOverlayOpen
        onActivated: focusMembersList()
    }

    Shortcut {
        sequence: "Ctrl+W"
        context: Qt.ApplicationShortcut
        enabled: !currentConversationIsChannel && !consoleVisible && !win.shortcutOverlayOpen
        onActivated: win.closeDirectMessage()
    }

    Shortcut {
        sequence: "Ctrl+/"
        context: Qt.ApplicationShortcut
        onActivated: {
            if (shortcutsSheet.opened)
                shortcutsSheet.close();
            else
                shortcutsSheet.open();
        }
    }

    Shortcut {
        sequence: "Ctrl+`"
        context: Qt.ApplicationShortcut
        enabled: !win.shortcutOverlayOpen
        onActivated: {
            if (win.consoleVisible) {
                if (win.irc)
                    win.networkConsole.open = false;
                else
                    win.mockStatusOpen = false;
                return;
            }
            var id = win.irc
                ? (win.irc.focusedNetworkId || win.irc.selectedNetworkId)
                : win.mockSelectedNetworkId;
            if (id && id.length > 0)
                win.openNetworkStatus(id);
        }
    }

    Shortcut {
        sequence: "Ctrl+,"
        context: Qt.ApplicationShortcut
        enabled: win.connection !== null && !win.shortcutOverlayOpen
        onActivated: {
            var id = win.irc ? win.irc.focusedNetworkId : "";
            if (id.length > 0)
                win.connection.select(id);
            win.connectionSheetOpen = true;
        }
    }

    Shortcut {
        sequence: "Alt+Down"
        context: Qt.ApplicationShortcut
        enabled: !win.shortcutOverlayOpen
        onActivated: stepConversation(1)
    }

    Shortcut {
        sequence: "Alt+Up"
        context: Qt.ApplicationShortcut
        enabled: !win.shortcutOverlayOpen
        onActivated: stepConversation(-1)
    }

    Shortcut {
        sequence: "Alt+A"
        context: Qt.ApplicationShortcut
        enabled: !win.shortcutOverlayOpen
        onActivated: jumpToNextUnread()
    }

    Shortcut {
        sequence: "PgUp"
        context: Qt.ApplicationShortcut
        enabled: !win.connectionOverlayVisible && !win.shortcutOverlayOpen
        onActivated: scrollTranscript(-1)
    }

    Shortcut {
        sequence: "PgDown"
        context: Qt.ApplicationShortcut
        enabled: !win.connectionOverlayVisible && !win.shortcutOverlayOpen
        onActivated: scrollTranscript(1)
    }

    Shortcut {
        sequence: "Escape"
        context: Qt.ApplicationShortcut
        enabled: {
            if (win.slashCommands && win.slashCommands.open)
                return true;
            if (shortcutsSheet.opened || shortcutsSheetEscapeGuard)
                return true;
            if (win.connection && win.connection.setupRequired)
                return false;
            if (win.connection && win.connectionSheetOpen)
                return true;
            if (win.findActive)
                return true;
            if (!win.consoleVisible)
                return false;
            if (win.irc)
                return win.irc.selectedTarget.length > 0;
            return win.mockCurrentConversation.length > 0;
        }
        onActivated: {
            if (win.slashCommands && win.slashCommands.open) {
                win.slashCommands.dismiss();
                return;
            }
            if (shortcutsSheet.opened || shortcutsSheetEscapeGuard) {
                shortcutsSheet.close();
                shortcutsSheetEscapeGuard = false;
                return;
            }
            if (win.connection && win.connectionSheetOpen) {
                win.connectionSheetOpen = false;
                return;
            }
            if (win.findActive) {
                win.leaveFind();
                return;
            }
            if (win.irc)
                win.networkConsole.open = false;
            else
                win.mockStatusOpen = false;
        }
    }

    component ConnectionField: Column {
        id: field

        property string label
        property alias fieldObjectName: input.objectName
        property alias text: input.text
        property bool secret: false

        signal textEdited(string text)

        function focusInput() {
            input.forceActiveFocus();
        }

        width: parent ? parent.width : 0
        spacing: win.scaledSize(4)

        Text {
            text: field.label
            color: win.mutedColor
            font.family: "iA Writer Mono S"
            font.pixelSize: win.scaledSize(10)
        }

        Rectangle {
            width: parent.width
            height: win.scaledSize(36)
            radius: win.scaledSize(7)
            color: win.panelColor
            border.width: 1
            border.color: input.activeFocus ? win.accentColor : win.dividerColor

            TextField {
                id: input
                anchors.fill: parent
                echoMode: field.secret ? TextInput.Password : TextInput.Normal
                color: win.inkColor
                selectionColor: win.selectionColor
                selectedTextColor: "#ffffff"
                font.family: "iA Writer Mono S"
                font.pixelSize: win.scaledSize(12)
                leftPadding: win.scaledSize(10)
                rightPadding: win.scaledSize(10)
                verticalAlignment: TextInput.AlignVCenter
                background: Item {}
                onTextEdited: field.textEdited(text)
                Keys.onPressed: function(event) {
                    if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                        win.submitConnection();
                        event.accepted = true;
                    }
                }
            }
        }
    }

    function clearConnectionPassword() {
        connectionPassword.text = "";
        connectionPasswordEdited = false;
    }

    function submitConnection() {
        if (!connection)
            return;
        if (connectionPasswordEdited)
            connection.setPassword(connectionPassword.text);
        if (connection.apply() && !connection.setupRequired) {
            connectionSheetOpen = false;
            connectionRemoveArmed = false;
            clearConnectionPassword();
        }
    }

    function selectSheetNetwork(networkId) {
        if (!connection)
            return;
        connection.select(networkId);
        connectionRemoveArmed = false;
    }

    Connections {
        target: win.connection
        ignoreUnknownSignals: true
        function onSelectedNetworkChanged() {
            win.clearConnectionPassword();
        }
        function onFocusPasswordChanged() {
            if (win.connection && win.connection.focusPassword) {
                win.connectionSheetOpen = true;
                connectionPassword.focusInput();
            }
        }
    }

    Connections {
        target: win.irc
        ignoreUnknownSignals: true
        function onMentionArrived(author, body) {
            win.notifyMentionIfUnfocused(win.active, author, body);
        }
    }

    component NetworkSection: Column {
        id: section

        property string networkId
        property string displayName
        property string statusText: "mock connected"
        property int alerts: 0
        property int unread: 0
        property bool mention: false
        property bool preserveLegacyNames: false
        property bool showEdit: win.connection !== null

        width: parent ? parent.width : 0
        spacing: 0

        readonly property color statusPulse: win.irc ? win.mixColors(win.pageColor, win.accentColor, 0) : "transparent"
        readonly property string liveStatus: {
            var pulse = win.irc ? win.irc.connectionStatus : "";
            if (!win.irc)
                return section.statusText;
            var error = win.irc.lastErrorFor ? win.irc.lastErrorFor(section.networkId) : "";
            if (error && error.length > 0)
                return error;
            return win.irc.connectionStatusFor
                ? win.irc.connectionStatusFor(section.networkId)
                : pulse;
        }
        readonly property int liveUnread: {
            var epoch = win.irc ? win.irc.conversationEpoch : 0;
            var pulse = win.irc ? win.irc.connectionStatus : "";
            if (!win.irc)
                return section.unread;
            return win.irc.unreadCountFor ? win.irc.unreadCountFor(section.networkId) : 0;
        }
        readonly property bool liveMention: {
            var epoch = win.irc ? win.irc.conversationEpoch : 0;
            var pulse = win.irc ? win.irc.connectionStatus : "";
            if (!win.irc)
                return section.mention;
            return win.irc.mentionFor ? win.irc.mentionFor(section.networkId) : false;
        }
        readonly property int liveAlerts: {
            var pulse = win.networkConsole ? win.networkConsole.alerts : 0;
            if (!win.networkConsole)
                return section.alerts;
            return win.networkConsole.alertsFor
                ? win.networkConsole.alertsFor(section.networkId)
                : pulse;
        }

        Item {
            id: networkHeader
            objectName: section.preserveLegacyNames ? "networkHeader"
                                                    : "networkHeader-" + section.networkId
            width: parent.width
            height: win.scaledSize(64)

            MouseArea {
                id: networkHeaderButton
                objectName: section.preserveLegacyNames ? "networkHeaderButton"
                                                        : "networkHeaderButton-" + section.networkId
                z: 1
                anchors.left: parent.left
                anchors.right: networkEditButton.left
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: win.openNetworkStatus(section.networkId)
            }

            Rectangle {
                anchors.left: parent.left
                anchors.leftMargin: win.scaledSize(18)
                anchors.verticalCenter: parent.verticalCenter
                width: win.scaledSize(28)
                height: width
                radius: win.scaledSize(8)
                color: win.accentColor

                Text {
                    anchors.centerIn: parent
                    text: section.displayName.length > 0
                        ? section.displayName.charAt(0).toUpperCase()
                        : "?"
                    color: "#ffffff"
                    font.family: "iA Writer Mono S"
                    font.bold: true
                    font.pixelSize: win.scaledSize(14)
                }
            }

            Item {
                id: networkEditButton
                objectName: section.preserveLegacyNames ? "networkEditButton"
                                                        : "networkEditButton-" + section.networkId
                z: 2
                visible: section.showEdit
                anchors.right: parent.right
                anchors.rightMargin: win.scaledSize(10)
                anchors.verticalCenter: parent.verticalCenter
                width: visible ? win.scaledSize(28) : 0
                height: win.scaledSize(28)
                Accessible.name: "Edit connection"
                Accessible.role: Accessible.Button
                Accessible.onPressAction: {
                    if (win.connection)
                        win.selectSheetNetwork(section.networkId);
                    win.connectionSheetOpen = true;
                }

                Text {
                    anchors.centerIn: parent
                    text: "edit"
                    color: editMouse.containsMouse ? win.inkColor : win.mutedColor
                    font.family: "iA Writer Mono S"
                    font.pixelSize: win.scaledSize(9)
                }

                MouseArea {
                    id: editMouse
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        if (win.connection)
                            win.selectSheetNetwork(section.networkId);
                        win.connectionSheetOpen = true;
                    }
                }
            }

            Column {
                anchors.left: parent.left
                anchors.leftMargin: win.scaledSize(56)
                anchors.right: networkEditButton.left
                anchors.rightMargin: win.scaledSize(8)
                anchors.verticalCenter: parent.verticalCenter
                spacing: win.scaledSize(2)

                Row {
                    width: parent.width
                    spacing: win.scaledSize(6)

                    Text {
                        width: Math.max(0, parent.width - (unreadMark.visible ? unreadMark.width + parent.spacing : 0))
                        text: section.displayName
                        color: win.inkColor
                        elide: Text.ElideRight
                        font.family: "iA Writer Mono S"
                        font.bold: true
                        font.pixelSize: win.scaledSize(13)
                    }

                    Rectangle {
                        id: unreadMark
                        objectName: section.preserveLegacyNames ? "networkUnreadMark"
                                                                : "networkUnreadMark-" + section.networkId
                        visible: section.liveUnread > 0 || section.liveMention
                        anchors.verticalCenter: parent.verticalCenter
                        width: win.scaledSize(7)
                        height: width
                        radius: width / 2
                        color: section.liveMention ? win.accentColor : win.inkColor
                    }
                }

                Row {
                    width: parent.width
                    spacing: win.scaledSize(6)

                    Rectangle {
                        anchors.verticalCenter: parent.verticalCenter
                        width: win.scaledSize(7)
                        height: width
                        radius: width / 2
                        color: section.liveAlerts > 0
                            ? win.accentColor
                            : (section.liveStatus === "Connected" || section.liveStatus === "mock connected"
                                ? "#69b978" : win.mutedColor)
                    }

                    Text {
                        width: Math.max(0, parent.width - win.scaledSize(13))
                        text: section.liveStatus
                        color: win.mutedColor
                        elide: Text.ElideRight
                        font.family: "iA Writer Mono S"
                        font.pixelSize: win.scaledSize(10)
                    }
                }
            }
        }
    }

    component ConversationRow: Item {
        id: conversationRow

        property string conversationName
        property int unread: 0
        property bool mention: false
        property bool direct: false
        property string networkId: win.mockOmarchyId
        property string conversationId: networkId + "\n" + conversationName

        objectName: networkId === win.mockOftcId
            ? "conversation-oftc-" + conversationName
            : (win.irc && networkId.length > 0 && win.connection
                ? "conversation-" + networkId + "-" + conversationName
                : "conversation-" + conversationName)
        Accessible.name: conversationName
        Accessible.role: Accessible.Button
        Accessible.onPressAction: activate()
        width: parent ? parent.width : 0
        height: win.scaledSize(36)

        readonly property bool current: conversationId === win.currentConversationId
            && !win.consoleVisible

        function activate() {
            if (!win.irc && !conversationRow.direct) {
                conversationRow.unread = 0;
                conversationRow.mention = false;
            }
            win.selectConversation(conversationRow.conversationName,
                                   conversationRow.networkId);
            Qt.callLater(function() {
                win.revealSidebarRow(conversationRow);
            });
        }

        Rectangle {
            anchors.fill: parent
            anchors.leftMargin: win.scaledSize(8)
            anchors.rightMargin: win.scaledSize(8)
            radius: win.scaledSize(7)
            color: conversationRow.current
                ? win.raisedColor
                : rowMouse.containsMouse ? win.hoverColor : "transparent"
        }

        Rectangle {
            visible: conversationRow.current
            anchors.left: parent.left
            anchors.leftMargin: win.scaledSize(8)
            anchors.verticalCenter: parent.verticalCenter
            width: win.scaledSize(3)
            height: win.scaledSize(18)
            radius: width
            color: win.accentColor
        }

        Rectangle {
            visible: conversationRow.direct
            anchors.left: parent.left
            anchors.leftMargin: win.scaledSize(18)
            anchors.verticalCenter: parent.verticalCenter
            width: win.scaledSize(22)
            height: width
            radius: width / 2
            color: win.mixColors(win.pageColor, win.nickColor(conversationRow.conversationName), 0.24)

            Text {
                anchors.centerIn: parent
                text: win.initials(conversationRow.conversationName)
                color: win.nickColor(conversationRow.conversationName)
                font.family: "iA Writer Mono S"
                font.bold: true
                font.pixelSize: win.scaledSize(11)
            }

            Rectangle {
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                width: win.scaledSize(7)
                height: width
                radius: width / 2
                color: "#69b978"
                border.width: win.scaledSize(2)
                border.color: win.panelColor
            }
        }

        Text {
            anchors.left: parent.left
            anchors.leftMargin: conversationRow.direct ? win.scaledSize(49) : win.scaledSize(20)
            anchors.right: unreadBadge.left
            anchors.rightMargin: win.scaledSize(8)
            anchors.verticalCenter: parent.verticalCenter
            text: (conversationRow.direct ? "" : "#  ") + conversationRow.conversationName.replace("#", "")
            color: conversationRow.current
                ? win.inkColor
                : conversationRow.unread > 0 ? win.inkColor : win.mutedColor
            elide: Text.ElideRight
            font.family: "iA Writer Mono S"
            font.bold: conversationRow.current
                       || conversationRow.unread > 0
            font.pixelSize: win.scaledSize(13)
        }

        Rectangle {
            id: unreadBadge
            visible: conversationRow.unread > 0
            anchors.right: parent.right
            anchors.rightMargin: win.scaledSize(17)
            anchors.verticalCenter: parent.verticalCenter
            width: Math.max(win.scaledSize(19), badgeText.implicitWidth + win.scaledSize(10))
            height: win.scaledSize(19)
            radius: height / 2
            color: conversationRow.mention ? win.accentColor : win.raisedColor

            Text {
                id: badgeText
                anchors.centerIn: parent
                text: conversationRow.unread
                color: conversationRow.mention ? "#ffffff" : win.inkColor
                font.family: "iA Writer Mono S"
                font.bold: true
                font.pixelSize: win.scaledSize(10)
            }
        }

        MouseArea {
            id: rowMouse
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: conversationRow.activate()
        }
    }

    component TranscriptList: ListView {
        id: list

        readonly property int stickFollowing: 0
        readonly property int stickDetached: 1
        property int stick: 0
        property int firstUnseenIndex: -1
        readonly property bool jumpArmed: stick === stickDetached
            && firstUnseenIndex >= 0
            && firstUnseenIndex < count

        property bool pinning: false
        property int trackedCount: 0
        property int restoreIndex: -1
        property int pinGeneration: 0
        property bool resetPending: false
        property int resetSavedCount: 0
        property real previousContentHeight: 0

        boundsBehavior: Flickable.StopAtBounds
        clip: true
        spacing: 0
        highlightFollowsCurrentItem: false

        function endContentY() {
            return originY + Math.max(0, contentHeight - height);
        }

        function viewportPinned() {
            if (count === 0 || contentHeight <= height || atYEnd)
                return true;
            return contentY >= endContentY() - 2;
        }

        function stickToEnd() {
            contentY = endContentY();
        }

        function pinToEnd() {
            stick = stickFollowing;
            firstUnseenIndex = -1;
            pinning = true;
            trackedCount = count;
            stickToEnd();
            positionViewAtEnd();
            var generation = ++pinGeneration;
            Qt.callLater(function() {
                if (generation !== pinGeneration)
                    return;
                stickToEnd();
                positionViewAtEnd();
                pinning = false;
                trackedCount = count;
            });
        }

        function adoptViewport() {
            if (pinning)
                return;
            if (viewportPinned())
                pinToEnd();
            else
                stick = stickDetached;
        }

        function noteGrowth(previousCount, newCount) {
            if (newCount < previousCount) {
                if (newCount <= 0) {
                    pinToEnd();
                    return;
                }
                if (firstUnseenIndex >= newCount)
                    firstUnseenIndex = -1;
                trackedCount = newCount;
                return;
            }
            if (newCount <= previousCount) {
                trackedCount = newCount;
                return;
            }
            if (stick === stickFollowing) {
                trackedCount = newCount;
                stickToEnd();
                return;
            }
            if (firstUnseenIndex < 0)
                firstUnseenIndex = previousCount;
            trackedCount = newCount;
        }

        function snapshotAnchor() {
            var index = indexAt(Math.max(1, width / 2), contentY + 1);
            if (index < 0)
                index = indexAt(Math.max(1, width / 2), contentY + 8);
            restoreIndex = index >= 0 ? index : 0;
        }

        function restoreAnchor() {
            if (stick === stickFollowing) {
                restoreIndex = -1;
                pinToEnd();
                return;
            }
            var target = restoreIndex;
            restoreIndex = -1;
            pinning = true;
            var generation = ++pinGeneration;
            if (target >= 0 && target < count)
                positionViewAtIndex(target, ListView.Beginning);
            Qt.callLater(function() {
                if (generation !== pinGeneration)
                    return;
                pinning = false;
            });
        }

        function jumpToUnseen() {
            if (!jumpArmed)
                return;
            var target = firstUnseenIndex;
            firstUnseenIndex = -1;
            pinning = true;
            var generation = ++pinGeneration;
            positionViewAtIndex(target, ListView.Beginning);
            Qt.callLater(function() {
                if (generation !== pinGeneration)
                    return;
                pinning = false;
                adoptViewport();
            });
        }

        onCountChanged: {
            if (resetPending)
                return;
            noteGrowth(trackedCount, count);
        }

        onModelChanged: pinToEnd()

        onMovementEnded: adoptViewport()
        onFlickEnded: adoptViewport()
        onContentHeightChanged: {
            var wasAtEnd = previousContentHeight <= height
                || contentY + height >= originY + previousContentHeight - 2;
            if (stick === stickFollowing && wasAtEnd)
                stickToEnd();
            previousContentHeight = contentHeight;
        }
        onHeightChanged: {
            if (stick === stickFollowing)
                stickToEnd();
            else
                adoptViewport();
        }

        Connections {
            target: list.model
            ignoreUnknownSignals: true
            function onModelAboutToBeReset() {
                list.resetPending = true;
                list.resetSavedCount = list.count;
                if (list.stick === list.stickDetached)
                    list.snapshotAnchor();
            }
            function onModelReset() {
                var previous = list.resetSavedCount;
                list.resetPending = false;
                list.noteGrowth(previous, list.count);
                list.restoreAnchor();
            }
            function onRowsInserted(parent, first, last) {
                list.noteGrowth(list.trackedCount, list.count);
            }
            function onRowsRemoved(parent, first, last) {
                if (first === 0
                        && list.stick === list.stickDetached
                        && list.firstUnseenIndex >= 0) {
                    list.firstUnseenIndex -= (last - first + 1);
                    if (list.firstUnseenIndex < 0)
                        list.firstUnseenIndex = -1;
                }
            }
        }

        Component.onCompleted: pinToEnd()
    }

    component UnseenJumpButton: Rectangle {
        required property TranscriptList list

        visible: list.visible && list.jumpArmed
        width: win.scaledSize(36)
        height: width
        radius: width / 2
        z: 2
        anchors.right: list.right
        anchors.bottom: list.bottom
        anchors.rightMargin: win.scaledSize(16)
        anchors.bottomMargin: win.scaledSize(16)
        color: jumpMouse.containsMouse
            ? win.mixColors(win.raisedColor, win.accentColor, win.darkMode ? 0.35 : 0.22)
            : win.raisedColor
        border.width: 1
        border.color: win.dividerColor

        Accessible.role: Accessible.Button
        Accessible.name: "Jump to first new message"
        Accessible.onPressAction: list.jumpToUnseen()

        Text {
            anchors.centerIn: parent
            text: "\u2193"
            color: win.inkColor
            font.family: "iA Writer Mono S"
            font.pixelSize: win.scaledSize(16)
        }

        MouseArea {
            id: jumpMouse
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: list.jumpToUnseen()
        }
    }

    ListModel {
        id: omarchyMessages
        ListElement {
            author: ""
            time: ""
            body: "Today"
            kind: "event"
        }
        ListElement {
            author: "anna"
            time: "09:41"
            body: "Morning! Has anyone tried the new minimal install flow yet?"
            kind: "message"
        }
        ListElement {
            author: "dax"
            time: "09:43"
            body: "Yes. Fresh install on my Framework took about twelve minutes. The defaults feel really considered."
            kind: "message"
        }
        ListElement {
            author: "mira"
            time: "09:46"
            body: "The way the theme carries across the terminal and native apps is my favorite detail."
            kind: "message"
        }
        ListElement {
            author: ""
            time: ""
            body: "sol joined #omarchy"
            kind: "event"
        }
        ListElement {
            author: "sol"
            time: "09:52"
            body: "Hey all. Just landed here from Arch. This feels surprisingly calm."
            kind: "message"
        }
        ListElement {
            author: "anna"
            time: "09:53"
            body: "Welcome, sol. Calm is the whole idea."
            kind: "message"
        }
        ListElement {
            author: "dax"
            time: "09:55"
            body: "If you have not already, try the keyboard-first app launcher. It becomes muscle memory fast."
            kind: "message"
        }
        ListElement {
            author: "sol"
            time: "09:56"
            body: "I found it. The shortcuts sheet is a nice touch too."
            kind: "message"
        }
        ListElement {
            author: "mira"
            time: "09:57"
            body: "Most of the system makes sense once you learn three or four core bindings."
            kind: "message"
        }
        ListElement {
            author: "anna"
            time: "09:58"
            body: "And everything important is still plain text when you want to look underneath."
            kind: "message"
        }
        ListElement {
            author: "kai"
            time: "09:59"
            body: "That balance is hard to get right: friendly defaults without hiding the actual system."
            kind: "message"
        }
        ListElement {
            author: "dax"
            time: "10:00"
            body: "Exactly. Start simple, then make it yours one deliberate change at a time."
            kind: "message"
        }
        ListElement {
            author: ""
            time: ""
            body: "nora joined #omarchy"
            kind: "event"
        }
        ListElement {
            author: "nora"
            time: "10:01"
            body: "Good timing. I was just looking for a quiet place to ask about native Omarchy apps."
            kind: "message"
        }
        ListElement {
            author: "fred"
            time: "10:02"
            body: "I am sketching a tiny IRC client that belongs here. No browser chrome, no clutter."
            kind: "message"
        }
        ListElement {
            author: "mira"
            time: "10:04"
            body: "Keep the member list optional and I am sold."
            kind: "message"
        }
    }

    ListModel {
        id: desktopMessages
        ListElement { author: ""; time: ""; body: "Today"; kind: "event" }
        ListElement {
            author: "dax"
            time: "08:22"
            body: "I finally moved every workspace rule into a small, readable file."
            kind: "message"
        }
        ListElement {
            author: "mira"
            time: "08:24"
            body: "That is the dream. Configuration you can understand in one sitting."
            kind: "message"
        }
        ListElement {
            author: "sol"
            time: "10:08"
            body: "Does anyone use a vertical monitor alongside the main display?"
            kind: "message"
        }
    }

    ListModel {
        id: ricingMessages
        ListElement { author: ""; time: ""; body: "Yesterday"; kind: "event" }
        ListElement {
            author: "anna"
            time: "18:10"
            body: "Muted colors, one strong accent, and enough breathing room."
            kind: "message"
        }
        ListElement {
            author: "mira"
            time: "18:13"
            body: "Typography does more work than decoration ever will."
            kind: "message"
        }
        ListElement {
            author: "dax"
            time: "18:20"
            body: "Dropped a new warm theme in the usual place. It looks great after sunset."
            kind: "message"
        }
    }

    ListModel {
        id: helpMessages
        ListElement { author: ""; time: ""; body: "Today"; kind: "event" }
        ListElement {
            author: "mira"
            time: "09:11"
            body: "Tip: include the command output and the exact behavior you expected."
            kind: "message"
        }
        ListElement {
            author: "sol"
            time: "09:15"
            body: "That made my monitor issue much easier to diagnose. Thanks."
            kind: "message"
        }
    }

    ListModel {
        id: annaMessages
        ListElement {
            author: ""
            time: ""
            body: "This is the beginning of your conversation with anna."
            kind: "event"
        }
        ListElement {
            author: "anna"
            time: "10:12"
            body: "The prototype already feels at home. Nice work."
            kind: "message"
        }
    }

    ListModel {
        id: daxMessages
        ListElement {
            author: ""
            time: ""
            body: "This is the beginning of your conversation with dax."
            kind: "event"
        }
        ListElement {
            author: "dax"
            time: "Yesterday"
            body: "Send me the build when the mock is ready."
            kind: "message"
        }
    }

    ListModel {
        id: miraMessages
        ListElement { author: ""; time: ""; body: "This is the beginning of your conversation with mira."; kind: "event" }
    }

    ListModel {
        id: solMessages
        ListElement { author: ""; time: ""; body: "This is the beginning of your conversation with sol."; kind: "event" }
    }

    ListModel {
        id: kaiMessages
        ListElement { author: ""; time: ""; body: "This is the beginning of your conversation with kai."; kind: "event" }
    }

    ListModel {
        id: noraMessages
        ListElement { author: ""; time: ""; body: "This is the beginning of your conversation with nora."; kind: "event" }
    }

    ListModel {
        id: teoMessages
        ListElement { author: ""; time: ""; body: "This is the beginning of your conversation with teo."; kind: "event" }
    }

    ListModel {
        id: lenaMessages
        ListElement { author: ""; time: ""; body: "This is the beginning of your conversation with lena."; kind: "event" }
    }

    ListModel {
        id: samMessages
        ListElement { author: ""; time: ""; body: "This is the beginning of your conversation with sam."; kind: "event" }
    }

    ListModel {
        id: ivyMessages
        ListElement { author: ""; time: ""; body: "This is the beginning of your conversation with ivy."; kind: "event" }
    }

    ListModel {
        id: maxMessages
        ListElement { author: ""; time: ""; body: "This is the beginning of your conversation with max."; kind: "event" }
    }

    ListModel {
        id: directConversations
        ListElement {
            conversation: "anna"
            networkId: "mock-omarchy"
            directUnread: 1
            directMention: true
        }
        ListElement {
            conversation: "dax"
            networkId: "mock-omarchy"
            directUnread: 0
            directMention: false
        }
    }

    ListModel {
        id: mockStatusMessages
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
            text: "Welcome to the mock network"
            source: "server"
            severity: "info"
        }
    }

    ListModel {
        id: oftcOmarchyMessages
        ListElement { author: ""; time: ""; body: "Today"; kind: "event" }
        ListElement {
            author: "rio"
            time: "11:02"
            body: "This #omarchy is the OFTC one. Different people, same name."
            kind: "message"
        }
        ListElement {
            author: "oak"
            time: "11:04"
            body: "Good. If the sidebar mixed them, we would already be lost."
            kind: "message"
        }
    }

    ListModel {
        id: labMessages
        ListElement { author: ""; time: ""; body: "Today"; kind: "event" }
        ListElement {
            author: "ness"
            time: "10:18"
            body: "Package build is green on the new runner."
            kind: "message"
        }
        ListElement {
            author: "rio"
            time: "10:21"
            body: "Leave the log in #build if it fails after sunset."
            kind: "message"
        }
    }

    ListModel {
        id: buildMessages
        ListElement { author: ""; time: ""; body: "Today"; kind: "event" }
        ListElement {
            author: "ness"
            time: "09:05"
            body: "Nightly failed on missing qt6keychain. Looking."
            kind: "message"
        }
        ListElement {
            author: "oak"
            time: "09:12"
            body: "Patched. Waiting on the next image."
            kind: "message"
        }
    }

    ListModel {
        id: rioMessages
        ListElement {
            author: ""
            time: ""
            body: "This is the beginning of your conversation with rio."
            kind: "event"
        }
        ListElement {
            author: "rio"
            time: "11:40"
            body: "Ping me on OFTC, not Libera."
            kind: "message"
        }
    }

    ListModel {
        id: oftcStatusMessages
        ListElement {
            time: "12:10:01"
            label: "NOTICE"
            text: "-AUTH- *** Looking up your hostname..."
            source: "server"
            severity: "info"
        }
        ListElement {
            time: "12:10:02"
            label: "001"
            text: "Welcome to the mock OFTC network"
            source: "server"
            severity: "info"
        }
    }

    ListModel {
        id: oftcMembersModel
        ListElement { nick: "rio"; label: "rio"; status: "on #lab"; away: false }
        ListElement { nick: "ness"; label: "ness"; status: "watching CI"; away: false }
        ListElement { nick: "oak"; label: "oak"; status: "building Omairc"; away: false }
        ListElement { nick: "pip"; label: "pip"; status: ""; away: true }
        ListElement { nick: "jules"; label: "jules"; status: ""; away: false }
        ListElement { nick: "remy"; label: "remy"; status: ""; away: false }
    }

    ListModel {
        id: membersModel
        ListElement { nick: "anna"; label: "anna"; status: "writing docs"; away: false }
        ListElement { nick: "dax"; label: "dax"; status: "on #desktop"; away: false }
        ListElement { nick: "mira"; label: "mira"; status: "making tea"; away: false }
        ListElement { nick: "sol"; label: "sol"; status: "new here"; away: false }
        ListElement { nick: "fred"; label: "fred"; status: "building Omairc"; away: false }
        ListElement { nick: "kai"; label: "kai"; status: ""; away: false }
        ListElement { nick: "nora"; label: "nora"; status: ""; away: false }
        ListElement { nick: "teo"; label: "teo"; status: ""; away: true }
        ListElement { nick: "lena"; label: "lena"; status: ""; away: true }
        ListElement { nick: "sam"; label: "sam"; status: ""; away: true }
        ListElement { nick: "ivy"; label: "ivy"; status: ""; away: true }
        ListElement { nick: "max"; label: "max"; status: ""; away: true }
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        Rectangle {
            id: sidebar
            Layout.preferredWidth: win.scaledSize(244)
            Layout.minimumWidth: win.scaledSize(214)
            Layout.fillHeight: true
            color: win.panelColor

            Rectangle {
                anchors.right: parent.right
                width: 1
                height: parent.height
                color: win.dividerColor
            }

            Flickable {
                id: sidebarScroll
                objectName: "sidebarList"
                anchors.top: parent.top
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: identityFooter.top
                clip: true
                contentWidth: width
                contentHeight: sidebarColumn.implicitHeight
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

                Column {
                    id: sidebarColumn
                    width: sidebarScroll.width
                    spacing: 0

                    Repeater {
                        id: liveNetworkRepeater
                        model: win.irc && win.connection ? win.connection.networks : null

                        Column {
                            id: liveNet
                            required property int index
                            required property string networkId
                            required property string displayName
                            width: parent ? parent.width : 0
                            spacing: 0

                            NetworkSection {
                                networkId: liveNet.networkId
                                displayName: liveNet.displayName
                                preserveLegacyNames: liveNet.index === 0
                                unread: 0
                                mention: false
                            }

                            Item {
                                width: parent.width
                                height: win.scaledSize(28)
                                Text {
                                    anchors.left: parent.left
                                    anchors.leftMargin: win.scaledSize(19)
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: "CHANNELS"
                                    color: win.mutedColor
                                    font.family: "iA Writer Mono S"
                                    font.bold: true
                                    font.letterSpacing: win.scaledSize(0.8)
                                    font.pixelSize: win.scaledSize(9)
                                }
                            }

                            Repeater {
                                objectName: liveNet.index === 0
                                    ? "channelConversationRepeater"
                                    : "channelConversationRepeater-" + liveNet.networkId
                                model: win.irc ? win.irc.conversations : null
                                delegate: ConversationRow {
                                    required property var model
                                    conversationName: model.conversation
                                    conversationId: model.conversationId
                                    unread: model.unread
                                    mention: model.mention
                                    direct: model.direct
                                    networkId: model.networkId
                                    visible: !model.direct
                                        && model.networkId === liveNet.networkId
                                    width: sidebar.width
                                    height: visible ? win.scaledSize(36) : 0
                                }
                            }

                            Item {
                                width: parent.width
                                height: {
                                    var epoch = win.irc ? win.irc.conversationEpoch : 0;
                                    return win.sectionHasDirects(liveNet.networkId)
                                        ? win.scaledSize(36) : 0;
                                }
                                visible: height > 0
                                Text {
                                    anchors.left: parent.left
                                    anchors.leftMargin: win.scaledSize(19)
                                    anchors.bottom: parent.bottom
                                    anchors.bottomMargin: win.scaledSize(8)
                                    text: "DIRECT MESSAGES"
                                    color: win.mutedColor
                                    font.family: "iA Writer Mono S"
                                    font.bold: true
                                    font.letterSpacing: win.scaledSize(0.8)
                                    font.pixelSize: win.scaledSize(9)
                                }
                            }

                            Repeater {
                                objectName: liveNet.index === 0
                                    ? "directConversationRepeater"
                                    : "directConversationRepeater-" + liveNet.networkId
                                model: win.irc ? win.irc.conversations : null
                                delegate: ConversationRow {
                                    required property var model
                                    conversationName: model.conversation
                                    conversationId: model.conversationId
                                    unread: model.unread
                                    mention: model.mention
                                    direct: model.direct
                                    networkId: model.networkId
                                    visible: model.direct
                                        && model.networkId === liveNet.networkId
                                    width: sidebar.width
                                    height: visible ? win.scaledSize(36) : 0
                                }
                            }
                        }
                    }

                    NetworkSection {
                        id: liveFallbackSection
                        visible: win.irc && !win.connection
                        height: visible ? implicitHeight : 0
                        networkId: win.irc ? win.irc.selectedNetworkId : ""
                        displayName: win.irc && win.irc.selectedNetworkId.length > 0
                            ? win.irc.selectedNetworkId : "Network"
                        preserveLegacyNames: visible
                        showEdit: false

                        Item {
                            width: parent.width
                            height: win.scaledSize(28)
                            Text {
                                anchors.left: parent.left
                                anchors.leftMargin: win.scaledSize(19)
                                anchors.verticalCenter: parent.verticalCenter
                                text: "CHANNELS"
                                color: win.mutedColor
                                font.family: "iA Writer Mono S"
                                font.bold: true
                                font.letterSpacing: win.scaledSize(0.8)
                                font.pixelSize: win.scaledSize(9)
                            }
                        }

                        Repeater {
                            id: channelConversationRepeater
                            objectName: win.irc && !win.connection ? "channelConversationRepeater" : ""
                            model: win.irc && !win.connection ? win.irc.conversations : null
                            delegate: ConversationRow {
                                required property var model
                                conversationName: model.conversation
                                conversationId: model.conversationId
                                unread: model.unread
                                mention: model.mention
                                direct: model.direct
                                networkId: model.networkId
                                visible: !model.direct
                                width: sidebar.width
                                height: visible ? win.scaledSize(36) : 0
                            }
                        }

                        Item {
                            width: parent.width
                            height: win.scaledSize(36)
                            Text {
                                anchors.left: parent.left
                                anchors.leftMargin: win.scaledSize(19)
                                anchors.bottom: parent.bottom
                                anchors.bottomMargin: win.scaledSize(8)
                                text: "DIRECT MESSAGES"
                                color: win.mutedColor
                                font.family: "iA Writer Mono S"
                                font.bold: true
                                font.letterSpacing: win.scaledSize(0.8)
                                font.pixelSize: win.scaledSize(9)
                            }
                        }

                        Repeater {
                            id: liveDirectConversationRepeater
                            objectName: win.irc && !win.connection ? "directConversationRepeater" : ""
                            model: win.irc && !win.connection ? win.irc.conversations : null
                            delegate: ConversationRow {
                                required property var model
                                conversationName: model.conversation
                                conversationId: model.conversationId
                                unread: model.unread
                                mention: model.mention
                                direct: model.direct
                                networkId: model.networkId
                                visible: model.direct
                                width: sidebar.width
                                height: visible ? win.scaledSize(36) : 0
                            }
                        }
                    }

                    NetworkSection {
                        id: mockOmarchySection
                        visible: !win.irc
                        height: visible ? implicitHeight : 0
                        networkId: win.mockOmarchyId
                        displayName: "Omarchy IRC"
                        statusText: "mock connected"
                        unread: 16
                        mention: true
                        preserveLegacyNames: true
                        showEdit: false

                        Item {
                            width: parent.width
                            height: win.scaledSize(28)
                            Text {
                                anchors.left: parent.left
                                anchors.leftMargin: win.scaledSize(19)
                                anchors.verticalCenter: parent.verticalCenter
                                text: "CHANNELS"
                                color: win.mutedColor
                                font.family: "iA Writer Mono S"
                                font.bold: true
                                font.letterSpacing: win.scaledSize(0.8)
                                font.pixelSize: win.scaledSize(9)
                            }
                        }

                        ConversationRow {
                            id: mockOmarchyRow
                            conversationName: "#omarchy"
                            networkId: win.mockOmarchyId
                            unread: 0
                            mention: false
                        }

                        ConversationRow {
                            id: mockDesktopRow
                            conversationName: "#desktop"
                            networkId: win.mockOmarchyId
                            unread: 3
                            mention: false
                        }

                        ConversationRow {
                            id: mockRicingRow
                            conversationName: "#ricing"
                            networkId: win.mockOmarchyId
                            unread: 12
                            mention: true
                        }

                        ConversationRow {
                            id: mockHelpRow
                            conversationName: "#help"
                            networkId: win.mockOmarchyId
                            unread: 0
                            mention: false
                        }

                        Item {
                            width: parent.width
                            height: win.scaledSize(36)
                            Text {
                                anchors.left: parent.left
                                anchors.leftMargin: win.scaledSize(19)
                                anchors.bottom: parent.bottom
                                anchors.bottomMargin: win.scaledSize(8)
                                text: "DIRECT MESSAGES"
                                color: win.mutedColor
                                font.family: "iA Writer Mono S"
                                font.bold: true
                                font.letterSpacing: win.scaledSize(0.8)
                                font.pixelSize: win.scaledSize(9)
                            }
                        }

                        Repeater {
                            id: directConversationRepeater
                            objectName: win.irc ? "" : "directConversationRepeater"
                            model: win.irc ? null : directConversations
                            delegate: ConversationRow {
                                required property string conversation
                                required property int index
                                required property int directUnread
                                required property bool directMention
                                width: sidebar.width
                                conversationName: conversation
                                networkId: {
                                    var row = directConversations.get(index);
                                    return row ? row.networkId : win.mockOmarchyId;
                                }
                                unread: directUnread
                                mention: directMention
                                direct: true
                                visible: networkId === win.mockOmarchyId
                                objectName: visible ? "conversation-" + conversation : ""
                                height: visible ? win.scaledSize(36) : 0
                            }
                        }
                    }

                    NetworkSection {
                        id: mockOftcSection
                        visible: !win.irc
                        height: visible ? implicitHeight : 0
                        networkId: win.mockOftcId
                        displayName: "irc.oftc.net"
                        statusText: "mock connected"
                        unread: 2
                        mention: false
                        preserveLegacyNames: false
                        showEdit: false

                        Item {
                            width: parent.width
                            height: win.scaledSize(28)
                            Text {
                                anchors.left: parent.left
                                anchors.leftMargin: win.scaledSize(19)
                                anchors.verticalCenter: parent.verticalCenter
                                text: "CHANNELS"
                                color: win.mutedColor
                                font.family: "iA Writer Mono S"
                                font.bold: true
                                font.letterSpacing: win.scaledSize(0.8)
                                font.pixelSize: win.scaledSize(9)
                            }
                        }

                        ConversationRow {
                            id: mockOftcOmarchyRow
                            conversationName: "#omarchy"
                            networkId: win.mockOftcId
                            unread: 0
                            mention: false
                        }

                        ConversationRow {
                            id: mockLabRow
                            conversationName: "#lab"
                            networkId: win.mockOftcId
                            unread: 0
                            mention: false
                        }

                        ConversationRow {
                            id: mockBuildRow
                            conversationName: "#build"
                            networkId: win.mockOftcId
                            unread: 2
                            mention: false
                        }

                        Item {
                            width: parent.width
                            height: win.scaledSize(36)
                            Text {
                                anchors.left: parent.left
                                anchors.leftMargin: win.scaledSize(19)
                                anchors.bottom: parent.bottom
                                anchors.bottomMargin: win.scaledSize(8)
                                text: "DIRECT MESSAGES"
                                color: win.mutedColor
                                font.family: "iA Writer Mono S"
                                font.bold: true
                                font.letterSpacing: win.scaledSize(0.8)
                                font.pixelSize: win.scaledSize(9)
                            }
                        }

                        ConversationRow {
                            id: mockRioRow
                            conversationName: "rio"
                            networkId: win.mockOftcId
                            unread: 0
                            mention: false
                            direct: true
                        }

                        Repeater {
                            objectName: "directConversationRepeater-mock-oftc"
                            model: win.irc ? null : directConversations
                            delegate: ConversationRow {
                                required property string conversation
                                required property int index
                                required property int directUnread
                                required property bool directMention
                                width: sidebar.width
                                conversationName: conversation
                                networkId: {
                                    var row = directConversations.get(index);
                                    return row ? row.networkId : win.mockOftcId;
                                }
                                unread: directUnread
                                mention: directMention
                                direct: true
                                visible: networkId === win.mockOftcId
                                objectName: visible
                                    ? "conversation-oftc-" + conversation : ""
                                height: visible ? win.scaledSize(36) : 0
                            }
                        }
                    }
                }
            }

            Rectangle {
                id: identityFooter
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                height: win.scaledSize(66)
                color: win.mixColors(win.panelColor, win.inkColor, win.darkMode ? 0.025 : 0.018)

                Rectangle {
                    anchors.top: parent.top
                    width: parent.width
                    height: 1
                    color: win.dividerColor
                }

                Rectangle {
                    anchors.left: parent.left
                    anchors.leftMargin: win.scaledSize(17)
                    anchors.verticalCenter: parent.verticalCenter
                    width: win.scaledSize(34)
                    height: width
                    radius: width / 2
                    color: win.mixColors(win.pageColor, win.nickColor(win.selfNick), 0.24)

                    Text {
                        anchors.centerIn: parent
                        text: win.initials(win.selfNick)
                        color: win.nickColor(win.selfNick)
                        font.family: "iA Writer Mono S"
                        font.bold: true
                        font.pixelSize: win.scaledSize(14)
                    }

                    Rectangle {
                        objectName: "selfPresenceDot"
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        width: win.scaledSize(9)
                        height: width
                        radius: width / 2
                        color: win.selfAway ? "#d6a552" : "#69b978"
                        border.width: win.scaledSize(2)
                        border.color: win.panelColor
                    }
                }

                Column {
                    anchors.left: parent.left
                    anchors.leftMargin: win.scaledSize(63)
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: win.scaledSize(1)

                    Text {
                        objectName: "selfNickLabel"
                        text: win.selfNick
                        color: win.inkColor
                        font.family: "iA Writer Mono S"
                        font.bold: true
                        font.pixelSize: win.scaledSize(13)
                    }

                    Text {
                        objectName: "selfPresenceLabel"
                        text: win.selfAway ? "away" : "available"
                        color: win.mutedColor
                        font.family: "iA Writer Mono S"
                        font.pixelSize: win.scaledSize(10)
                    }
                }
            }
        }

        Item {
            id: conversation
            Layout.fillWidth: true
            Layout.fillHeight: true

            Item {
                id: conversationHeader
                visible: !win.consoleVisible
                anchors.top: parent.top
                anchors.left: parent.left
                anchors.right: parent.right
                height: visible ? win.scaledSize(72) : 0

                Column {
                    anchors.left: parent.left
                    anchors.leftMargin: win.scaledSize(24)
                    anchors.right: win.currentConversationIsChannel ? peopleButton.left : parent.right
                    anchors.rightMargin: win.scaledSize(win.currentConversationIsChannel ? 18 : 24)
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: win.scaledSize(3)

                    Text {
                        width: parent.width
                        text: {
                            if (win.duplicateTargetName(win.currentConversation)) {
                                var network = win.focusedNetworkDisplayName();
                                if (network.length > 0)
                                    return win.currentConversation + " · " + network;
                            }
                            return win.currentConversation;
                        }
                        color: win.inkColor
                        elide: Text.ElideRight
                        font.family: "iA Writer Mono S"
                        font.bold: true
                        font.pixelSize: win.scaledSize(17)
                    }

                    Text {
                        objectName: "conversationTopic"
                        width: parent.width
                        text: win.plainIrcText(win.currentTopic)
                        color: win.mutedColor
                        elide: Text.ElideRight
                        font.family: "iA Writer Mono S"
                        font.pixelSize: win.scaledSize(11)
                    }
                }

                Rectangle {
                    id: peopleButton
                    objectName: "peopleButton"
                    Accessible.name: win.membersVisible ? "Hide members" : "Show members"
                    Accessible.role: Accessible.Button
                    Accessible.onPressAction: win.membersVisible = !win.membersVisible
                    visible: win.currentConversationIsChannel && !win.consoleVisible
                    anchors.right: parent.right
                    anchors.rightMargin: win.scaledSize(19)
                    anchors.verticalCenter: parent.verticalCenter
                    width: win.scaledSize(74)
                    height: win.scaledSize(30)
                    radius: win.scaledSize(7)
                    color: peopleMouse.containsMouse || win.membersVisible
                        ? win.raisedColor : "transparent"
                    border.width: 1
                    border.color: win.membersVisible ? win.dividerColor : "transparent"

                    Text {
                        anchors.centerIn: parent
                        text: win.currentPeopleCount + " PEOPLE"
                        color: win.membersVisible ? win.inkColor : win.mutedColor
                        font.family: "iA Writer Mono S"
                        font.bold: true
                        font.pixelSize: win.scaledSize(9)
                    }

                    MouseArea {
                        id: peopleMouse
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: win.membersVisible = !win.membersVisible
                    }
                }

                Rectangle {
                    anchors.bottom: parent.bottom
                    width: parent.width
                    height: 1
                    color: win.dividerColor
                }
            }

            Item {
                id: consoleHeader
                visible: win.consoleVisible
                anchors.top: parent.top
                anchors.left: parent.left
                anchors.right: parent.right
                height: visible ? win.scaledSize(72) : 0

                Column {
                    anchors.left: parent.left
                    anchors.leftMargin: win.scaledSize(24)
                    anchors.right: parent.right
                    anchors.rightMargin: win.scaledSize(24)
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: win.scaledSize(3)

                    Text {
                        width: parent.width
                        text: win.statusTitleText()
                        color: win.inkColor
                        elide: Text.ElideRight
                        font.family: "iA Writer Mono S"
                        font.bold: true
                        font.pixelSize: win.scaledSize(17)
                    }

                    Text {
                        width: parent.width
                        text: win.irc
                            ? (win.irc.lastError.length > 0
                                ? win.irc.lastError : win.irc.connectionStatus)
                            : "mock connected"
                        color: win.mutedColor
                        elide: Text.ElideRight
                        font.family: "iA Writer Mono S"
                        font.pixelSize: win.scaledSize(11)
                    }
                }

                Rectangle {
                    anchors.bottom: parent.bottom
                    width: parent.width
                    height: 1
                    color: win.dividerColor
                }
            }

            TranscriptList {
                id: messageList
                objectName: "messageList"
                visible: !win.consoleVisible
                Accessible.name: "Messages in " + win.currentConversation
                anchors.top: conversationHeader.bottom
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: composerShell.top
                anchors.bottomMargin: win.scaledSize(12)
                model: win.activeMessages

                ScrollBar.vertical: ScrollBar {
                    policy: ScrollBar.AsNeeded
                    onPressedChanged: {
                        if (!pressed)
                            messageList.adoptViewport();
                    }
                }

                delegate: Item {
                    id: messageDelegate

                    required property int index
                    required property string author
                    required property string time
                    required property string body
                    required property string kind
                    readonly property bool grouped: win.continuesMessageGroup(
                        messageList.model, index, author, time, kind)

                    width: messageList.width
                    height: kind === "event"
                        ? win.scaledSize(42)
                        : (grouped
                            ? Math.max(win.scaledSize(22), messageBody.implicitHeight + win.scaledSize(8))
                            : Math.max(win.scaledSize(58), messageBody.implicitHeight + win.scaledSize(39)))

                    Text {
                        objectName: "messageEvent"
                        visible: messageDelegate.kind === "event"
                        anchors.centerIn: parent
                        width: parent.width - win.scaledSize(48)
                        horizontalAlignment: Text.AlignHCenter
                        text: win.plainIrcText(messageDelegate.body)
                        color: win.mutedColor
                        elide: Text.ElideRight
                        font.family: "iA Writer Mono S"
                        font.pixelSize: win.scaledSize(10)
                    }

                    Rectangle {
                        objectName: "messageAvatar"
                        visible: messageDelegate.kind !== "event" && !messageDelegate.grouped
                        anchors.left: parent.left
                        anchors.leftMargin: win.scaledSize(24)
                        anchors.top: parent.top
                        anchors.topMargin: win.scaledSize(9)
                        width: win.scaledSize(34)
                        height: width
                        radius: width / 2
                        color: win.mixColors(
                            win.pageColor,
                            win.nickColor(messageDelegate.author),
                            win.darkMode ? 0.23 : 0.16)

                        Text {
                            anchors.centerIn: parent
                            text: win.initials(messageDelegate.author)
                            color: win.nickColor(messageDelegate.author)
                            font.family: "iA Writer Mono S"
                            font.bold: true
                            font.pixelSize: win.scaledSize(13)
                        }
                    }

                    Row {
                        objectName: "messageHeader"
                        visible: messageDelegate.kind !== "event" && !messageDelegate.grouped
                        anchors.left: parent.left
                        anchors.leftMargin: win.scaledSize(70)
                        anchors.top: parent.top
                        anchors.topMargin: win.scaledSize(8)
                        spacing: win.scaledSize(9)

                        Text {
                            text: messageDelegate.author
                            color: win.nickColor(messageDelegate.author)
                            font.family: "iA Writer Mono S"
                            font.bold: true
                            font.pixelSize: win.scaledSize(12)
                        }

                        Text {
                            anchors.baseline: parent.children[0].baseline
                            text: messageDelegate.time
                            color: win.mutedColor
                            font.family: "iA Writer Mono S"
                            font.pixelSize: win.scaledSize(9)
                        }
                    }

                    TextEdit {
                        id: messageBody
                        objectName: "messageBody"
                        visible: messageDelegate.kind !== "event"
                        anchors.left: parent.left
                        anchors.leftMargin: win.scaledSize(70)
                        anchors.right: parent.right
                        anchors.rightMargin: win.scaledSize(34)
                        anchors.top: parent.top
                        anchors.topMargin: messageDelegate.grouped
                            ? win.scaledSize(4)
                            : win.scaledSize(29)
                        text: win.plainIrcText(messageDelegate.body)
                        color: messageDelegate.kind === "action" ? win.mutedColor : win.inkColor
                        selectionColor: win.selectionColor
                        selectedTextColor: "#ffffff"
                        wrapMode: TextEdit.Wrap
                        readOnly: true
                        selectByMouse: true
                        cursorVisible: false
                        activeFocusOnPress: false
                        activeFocusOnTab: false
                        textFormat: TextEdit.PlainText
                        padding: 0
                        font.family: "iA Writer Mono S"
                        font.italic: messageDelegate.kind === "action"
                        font.pixelSize: win.scaledSize(13)

                        PlainUrlHit { edit: messageBody }
                    }
                }
            }

            UnseenJumpButton {
                list: messageList
                objectName: "messageUnseenJump"
            }

            TranscriptList {
                id: consoleList
                objectName: "consoleList"
                visible: win.consoleVisible
                Accessible.name: "Status"
                anchors.top: consoleHeader.bottom
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: composerShell.top
                anchors.bottomMargin: win.scaledSize(12)
                model: win.networkConsole ? win.networkConsole.lines
                    : (win.mockStatusNetworkId === win.mockOftcId
                        ? oftcStatusMessages : mockStatusMessages)

                ScrollBar.vertical: ScrollBar {
                    policy: ScrollBar.AsNeeded
                    onPressedChanged: {
                        if (!pressed)
                            consoleList.adoptViewport();
                    }
                }

                delegate: Item {
                    id: consoleDelegate

                    required property string time
                    required property string label
                    required property string text
                    required property string source
                    required property string severity

                    width: consoleList.width
                    height: Math.max(win.scaledSize(22), consoleText.implicitHeight + win.scaledSize(8))

                    readonly property color labelColor: consoleDelegate.severity === "alert"
                        ? win.accentColor
                        : (consoleDelegate.severity === "trace"
                            ? win.mutedColor
                            : win.mixColors(win.pageColor, win.inkColor, 0.62))
                    readonly property color bodyColor: consoleDelegate.severity === "trace"
                        ? win.mutedColor
                        : win.inkColor
                    readonly property string glyph: consoleDelegate.source === "client"
                        ? ">>"
                        : (consoleDelegate.source === "local" ? "--" : "<<")

                    Text {
                        anchors.left: parent.left
                        anchors.leftMargin: win.scaledSize(24)
                        anchors.verticalCenter: parent.verticalCenter
                        width: win.scaledSize(68)
                        text: consoleDelegate.time
                        color: win.mutedColor
                        font.family: "iA Writer Mono S"
                        font.pixelSize: win.scaledSize(10)
                    }

                    Text {
                        anchors.left: parent.left
                        anchors.leftMargin: win.scaledSize(96)
                        anchors.verticalCenter: parent.verticalCenter
                        width: win.scaledSize(88)
                        text: consoleDelegate.glyph + " " + consoleDelegate.label
                        color: consoleDelegate.labelColor
                        elide: Text.ElideRight
                        font.family: "iA Writer Mono S"
                        font.pixelSize: win.scaledSize(10)
                    }

                    TextEdit {
                        id: consoleText
                        objectName: "consoleText"
                        anchors.left: parent.left
                        anchors.leftMargin: win.scaledSize(192)
                        anchors.right: parent.right
                        anchors.rightMargin: win.scaledSize(24)
                        anchors.verticalCenter: parent.verticalCenter
                        text: win.plainIrcText(consoleDelegate.text)
                        color: consoleDelegate.bodyColor
                        selectionColor: win.selectionColor
                        selectedTextColor: "#ffffff"
                        wrapMode: TextEdit.Wrap
                        readOnly: true
                        selectByMouse: true
                        cursorVisible: false
                        activeFocusOnPress: false
                        activeFocusOnTab: false
                        textFormat: TextEdit.PlainText
                        padding: 0
                        font.family: "iA Writer Mono S"
                        font.pixelSize: win.scaledSize(12)

                        PlainUrlHit { edit: consoleText }
                    }
                }
            }

            UnseenJumpButton {
                list: consoleList
                objectName: "consoleUnseenJump"
            }

            Item {
                objectName: "composer-typing"
                visible: win.typingVisible && !win.currentConversationIsChannel
                    && !win.consoleVisible && win.typingNicks && win.typingNicks.length > 0
                height: win.scaledSize(12)
                anchors.left: composerShell.left
                anchors.right: composerShell.right
                anchors.bottom: composerShell.top
                z: 1

                TypingDots {
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    pixelSize: win.scaledSize(10)
                }
            }

            Rectangle {
                id: composerShell
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                anchors.leftMargin: win.scaledSize(20)
                anchors.rightMargin: win.scaledSize(20)
                anchors.bottomMargin: win.scaledSize(20)
                height: Math.max(win.scaledSize(46), Math.min(win.scaledSize(112),
                    composer.contentHeight + win.scaledSize(20)))
                radius: win.scaledSize(10)
                color: win.panelColor
                border.width: 1
                border.color: composer.activeFocus ? win.accentColor : win.dividerColor

                TextField {
                    id: composer
                    objectName: "messageComposer"
                    Accessible.name: "Message composer"
                    Accessible.description: win.consoleVisible
                        ? "Command for " + win.statusTitleText().replace(" Status", "")
                        : "Write a message to " + win.currentConversation
                    anchors.left: parent.left
                    anchors.right: sendButton.left
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    anchors.leftMargin: win.scaledSize(8)
                    anchors.rightMargin: win.scaledSize(8)
                    verticalAlignment: TextEdit.AlignVCenter
                    color: win.inkColor
                    selectionColor: win.selectionColor
                    selectedTextColor: "#ffffff"
                    font.family: "iA Writer Mono S"
                    font.pixelSize: win.scaledSize(13)
                    leftPadding: win.scaledSize(8)
                    rightPadding: win.scaledSize(8)
                    topPadding: Math.max(win.scaledSize(8),
                        (height - contentHeight) / 2)
                    bottomPadding: topPadding
                    background: Item {}
                    onTextChanged: {
                        if (win.findActive) {
                            if (win.slashCommands)
                                win.slashCommands.dismiss();
                            win.advanceFind(true);
                            return;
                        }
                        if (win.slashCommands) {
                            if (win.composerHistoryIndex >= 0)
                                win.slashCommands.dismiss();
                            else
                                win.slashCommands.sync(text, win.consoleVisible);
                        }
                        if (win.irc)
                            win.irc.notifyComposerText(text);
                    }

                    Keys.onPressed: function(event) {
                        if (win.findActive) {
                            if (event.key === Qt.Key_Tab
                                || ((event.key === Qt.Key_Up || event.key === Qt.Key_Down)
                                    && win.composerHasPlainModifier(event))) {
                                event.accepted = true;
                                return;
                            }
                        }

                        var historyArrow = (event.key === Qt.Key_Up
                            || event.key === Qt.Key_Down)
                            && win.composerHasPlainModifier(event)
                            && win.composerHistoryIndex >= 0;
                        if (!win.findActive && win.slashCommands && !historyArrow) {
                            var routed = win.slashCommands.routeKey(event.key, event.modifiers);
                            if (routed.accepted) {
                                if (routed.insertion.length > 0) {
                                    composer.text = routed.insertion;
                                    composer.cursorPosition = routed.insertion.length;
                                }
                                event.accepted = true;
                                return;
                            }
                        }

                        if (event.key === Qt.Key_Tab && win.composerHasPlainModifier(event)) {
                            win.completeNick();
                            event.accepted = true;
                            return;
                        }

                        win.resetNickComplete();

                        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                            win.sendMessage();
                            event.accepted = true;
                            return;
                        }

                        if (event.key === Qt.Key_Up && win.composerHasPlainModifier(event)) {
                            event.accepted = win.recallComposerHistory(-1);
                            return;
                        }

                        if (event.key === Qt.Key_Down && win.composerHasPlainModifier(event)) {
                            event.accepted = win.recallComposerHistory(1);
                            return;
                        }
                    }
                }

                Rectangle {
                    id: sendButton
                    objectName: "sendButton"
                    Accessible.name: "Send message"
                    Accessible.role: Accessible.Button
                    Accessible.onPressAction: {
                        if (composer.text.trim().length > 0)
                            win.sendMessage();
                    }
                    anchors.right: parent.right
                    anchors.rightMargin: win.scaledSize(8)
                    anchors.verticalCenter: parent.verticalCenter
                    width: win.scaledSize(54)
                    height: win.scaledSize(30)
                    radius: win.scaledSize(7)
                    color: composer.text.trim().length > 0
                        ? (sendMouse.containsMouse
                            ? win.mixColors(win.accentColor, win.inkColor, 0.14)
                            : win.accentColor)
                        : win.raisedColor

                    Text {
                        anchors.centerIn: parent
                        text: "SEND"
                        color: composer.text.trim().length > 0 ? "#ffffff" : win.mutedColor
                        font.family: "iA Writer Mono S"
                        font.bold: true
                        font.pixelSize: win.scaledSize(9)
                    }

                    MouseArea {
                        id: sendMouse
                        anchors.fill: parent
                        enabled: composer.text.trim().length > 0
                        hoverEnabled: true
                        cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                        onClicked: win.sendMessage()
                    }
                }
            }

            Rectangle {
                id: slashCompleteList
                objectName: "slashCompleteList"
                visible: win.slashCommands && win.slashCommands.open
                z: 2
                anchors.left: composerShell.left
                anchors.right: composerShell.right
                anchors.bottom: composerShell.top
                anchors.bottomMargin: win.scaledSize(6)
                height: slashHitColumn.implicitHeight + win.scaledSize(12)
                radius: win.scaledSize(10)
                color: win.raisedColor
                border.width: 1
                border.color: win.dividerColor

                Column {
                    id: slashHitColumn
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    anchors.margins: win.scaledSize(6)
                    spacing: win.scaledSize(2)

                    Repeater {
                        model: win.slashCommands ? win.slashCommands.matches : []
                        delegate: Rectangle {
                            objectName: "slashHit-" + String(modelData.label).slice(1)
                            width: slashHitColumn.width
                            height: win.scaledSize(28)
                            radius: win.scaledSize(7)
                            color: {
                                if (index === win.slashCommands.selectedIndex)
                                    return win.mixColors(win.selectionColor, win.raisedColor,
                                                         win.darkMode ? 0.45 : 0.35);
                                if (hitMouse.containsMouse)
                                    return win.hoverColor;
                                return "transparent";
                            }

                            Row {
                                anchors.fill: parent
                                anchors.leftMargin: win.scaledSize(8)
                                anchors.rightMargin: win.scaledSize(8)
                                spacing: win.scaledSize(12)

                                Text {
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: modelData.label
                                    color: win.inkColor
                                    font.family: "iA Writer Mono S"
                                    font.pixelSize: win.scaledSize(12)
                                }

                                Text {
                                    anchors.verticalCenter: parent.verticalCenter
                                    visible: usageHint.trim().length > 0
                                    width: Math.max(0, parent.width - x)
                                    text: usageHint
                                    elide: Text.ElideRight
                                    color: win.mutedColor
                                    font.family: "iA Writer Mono S"
                                    font.pixelSize: win.scaledSize(12)

                                    readonly property string usageHint: {
                                        var usage = String(modelData.usage)
                                        var label = String(modelData.label)
                                        if (usage.indexOf(label) === 0)
                                            return usage.substring(label.length)
                                        return usage
                                    }
                                }
                            }

                            MouseArea {
                                id: hitMouse
                                anchors.fill: parent
                                hoverEnabled: true
                                onEntered: win.slashCommands.selectedIndex = index
                                onClicked: {
                                    var replacement = win.slashCommands.activate(index);
                                    if (replacement.length > 0) {
                                        composer.text = replacement;
                                        composer.cursorPosition = replacement.length;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Rectangle {
                id: connectionSheet
                objectName: "connectionSheet"
                anchors.fill: parent
                visible: win.connection && (win.connection.setupRequired || win.connectionSheetOpen)
                color: win.mixColors(win.pageColor, win.inkColor, win.darkMode ? 0.18 : 0.12)

                MouseArea {
                    anchors.fill: parent
                    onClicked: {
                        if (win.connection && !win.connection.setupRequired)
                            win.connectionSheetOpen = false;
                    }
                }

                Rectangle {
                    anchors.centerIn: parent
                    width: Math.min(win.scaledSize(700), parent.width - win.scaledSize(40))
                    height: Math.min(win.scaledSize(620), parent.height - win.scaledSize(40))
                    radius: win.scaledSize(10)
                    color: win.raisedColor
                    border.width: 1
                    border.color: win.dividerColor

                    MouseArea {
                        id: sheetCardClickSink
                        anchors.fill: parent
                        onClicked: function(mouse) { mouse.accepted = true; }
                    }

                    Row {
                        anchors.fill: parent
                        anchors.margins: win.scaledSize(18)
                        spacing: win.scaledSize(16)

                        ColumnLayout {
                            id: networkRail
                            objectName: "networkChoiceList"
                            width: win.scaledSize(170)
                            height: parent.height
                            spacing: win.scaledSize(6)

                            Text {
                                id: networkRailLabel
                                Layout.fillWidth: true
                                text: "Networks"
                                color: win.mutedColor
                                font.family: "iA Writer Mono S"
                                font.bold: true
                                font.pixelSize: win.scaledSize(10)
                            }

                            Flickable {
                                id: networkChoiceScroll
                                Layout.fillWidth: true
                                Layout.fillHeight: true
                                Layout.minimumHeight: win.scaledSize(32)
                                clip: true
                                contentWidth: width
                                contentHeight: networkChoiceColumn.implicitHeight
                                boundsBehavior: Flickable.StopAtBounds
                                ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

                                Column {
                                    id: networkChoiceColumn
                                    width: networkChoiceScroll.width
                                    spacing: win.scaledSize(6)

                                    Repeater {
                                        id: networkChoiceRepeater
                                        objectName: "networkChoiceRepeater"
                                        model: win.connection && win.connection.networks
                                            ? win.connection.networks : null
                                        delegate: Rectangle {
                                            required property string networkId
                                            required property string displayName
                                            required property bool selected
                                            width: networkRail.width
                                            height: win.scaledSize(32)
                                            radius: win.scaledSize(6)
                                            color: selected
                                                ? win.mixColors(win.selectionColor, win.raisedColor,
                                                                win.darkMode ? 0.45 : 0.35)
                                                : (choiceMouse.containsMouse
                                                    || activeFocus
                                                    ? win.hoverColor : "transparent")
                                            objectName: "networkChoice-" + networkId
                                            activeFocusOnTab: true
                                            Accessible.role: Accessible.Button
                                            Accessible.name: displayName
                                            Accessible.onPressAction: win.selectSheetNetwork(networkId)
                                            Keys.onReturnPressed: function(event) {
                                                win.selectSheetNetwork(networkId);
                                                event.accepted = true;
                                            }
                                            Keys.onSpacePressed: function(event) {
                                                win.selectSheetNetwork(networkId);
                                                event.accepted = true;
                                            }

                                            Text {
                                                anchors.fill: parent
                                                anchors.leftMargin: win.scaledSize(8)
                                                anchors.rightMargin: win.scaledSize(8)
                                                text: displayName
                                                color: win.inkColor
                                                elide: Text.ElideRight
                                                verticalAlignment: Text.AlignVCenter
                                                font.family: "iA Writer Mono S"
                                                font.pixelSize: win.scaledSize(11)
                                            }

                                            MouseArea {
                                                id: choiceMouse
                                                anchors.fill: parent
                                                hoverEnabled: true
                                                cursorShape: Qt.PointingHandCursor
                                                onClicked: {
                                                    win.selectSheetNetwork(networkId);
                                                    parent.forceActiveFocus();
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            Rectangle {
                                id: connectionAddNetwork
                                objectName: "connectionAddNetwork"
                                Layout.fillWidth: true
                                Layout.preferredHeight: win.scaledSize(28)
                                radius: win.scaledSize(6)
                                visible: win.connection ? win.connection.canAdd : false
                                color: addNetworkMouse.containsMouse ? win.hoverColor : "transparent"

                                Text {
                                    anchors.centerIn: parent
                                    text: "Add network"
                                    color: win.mutedColor
                                    font.family: "iA Writer Mono S"
                                    font.pixelSize: win.scaledSize(11)
                                }

                                MouseArea {
                                    id: addNetworkMouse
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: {
                                        if (!win.connection || !win.connection.add())
                                            return;
                                        connectionPassword.text = "";
                                        win.connectionPasswordEdited = false;
                                        win.connectionRemoveArmed = false;
                                    }
                                }
                            }
                        }

                        Item {
                            width: parent.width - networkRail.width - parent.spacing
                            height: parent.height

                        Flickable {
                        id: sheetFlick
                        objectName: "sheetFlick"
                        anchors.top: parent.top
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.bottom: sheetActions.top
                        anchors.bottomMargin: win.scaledSize(10)
                        contentWidth: width
                        contentHeight: sheetColumn.implicitHeight
                        clip: true
                        boundsBehavior: Flickable.StopAtBounds

                    Column {
                        id: sheetColumn
                        width: sheetFlick.width
                        spacing: win.scaledSize(10)

                        Text {
                            text: "Connect"
                            color: win.inkColor
                            font.family: "iA Writer Mono S"
                            font.bold: true
                            font.pixelSize: win.scaledSize(15)
                        }

                        ConnectionField {
                            label: "Host"
                            fieldObjectName: "connectionHost"
                            text: win.connection ? win.connection.host : ""
                            onTextEdited: function(value) {
                                if (win.connection)
                                    win.connection.host = value;
                            }
                        }

                        Row {
                            width: parent.width
                            spacing: win.scaledSize(12)

                            ConnectionField {
                                width: parent.width - win.scaledSize(120)
                                label: "Port"
                                fieldObjectName: "connectionPort"
                                text: win.connection ? String(win.connection.port) : "6697"
                                onTextEdited: function(value) {
                                    if (win.connection)
                                        win.connection.port = Number(value) || 0;
                                }
                            }

                            Column {
                                anchors.bottom: parent.bottom
                                spacing: win.scaledSize(4)

                                Text {
                                    text: "TLS"
                                    color: win.mutedColor
                                    font.family: "iA Writer Mono S"
                                    font.pixelSize: win.scaledSize(10)
                                }

                                Switch {
                                    id: connectionTls
                                    objectName: "connectionTls"
                                    checked: win.connection ? win.connection.tlsEnabled : true
                                    onToggled: {
                                        if (win.connection)
                                            win.connection.tlsEnabled = checked;
                                    }
                                }
                            }
                        }

                        ConnectionField {
                            label: "Nick"
                            fieldObjectName: "connectionNick"
                            text: win.connection ? win.connection.nick : ""
                            onTextEdited: function(value) {
                                if (win.connection)
                                    win.connection.nick = value;
                            }
                        }

                        ConnectionField {
                            label: "Username"
                            fieldObjectName: "connectionUsername"
                            text: win.connection ? win.connection.username : ""
                            onTextEdited: function(value) {
                                if (win.connection)
                                    win.connection.username = value;
                            }
                        }

                        ConnectionField {
                            label: "Real name"
                            fieldObjectName: "connectionRealname"
                            text: win.connection ? win.connection.realname : ""
                            onTextEdited: function(value) {
                                if (win.connection)
                                    win.connection.realname = value;
                            }
                        }

                        ConnectionField {
                            label: "Autojoin"
                            fieldObjectName: "connectionAutojoin"
                            text: win.connection ? win.connection.autojoin : ""
                            onTextEdited: function(value) {
                                if (win.connection)
                                    win.connection.autojoin = value;
                            }
                        }

                        Row {
                            width: parent.width
                            spacing: win.scaledSize(10)

                            Switch {
                                id: connectionConnectOnStartup
                                objectName: "connectionConnectOnStartup"
                                onToggled: {
                                    if (win.connection)
                                        win.connection.connectOnStartup = checked;
                                }
                            }

                            Binding {
                                target: connectionConnectOnStartup
                                property: "checked"
                                value: win.connection ? win.connection.connectOnStartup : false
                                restoreMode: Binding.RestoreBinding
                            }

                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                text: "Connect automatically on startup"
                                color: win.inkColor
                                font.family: "iA Writer Mono S"
                                font.pixelSize: win.scaledSize(11)
                            }
                        }

                        ConnectionField {
                            id: connectionPassword
                            label: "Password"
                            fieldObjectName: "connectionPassword"
                            secret: true
                            onTextEdited: function(value) {
                                win.connectionPasswordEdited = true;
                            }
                        }

                        Text {
                            objectName: "connectionProblem"
                            width: parent.width
                            visible: win.connection && win.connection.problem.length > 0
                            text: win.connection ? win.connection.problem : ""
                            color: win.accentColor
                            wrapMode: Text.Wrap
                            font.family: "iA Writer Mono S"
                            font.pixelSize: win.scaledSize(11)
                        }
                    }
                    }

                    Row {
                        id: sheetActions
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        spacing: win.scaledSize(8)

                            Rectangle {
                                objectName: "connectionRemove"
                                visible: win.connection ? win.connection.canRemove : false
                                width: visible ? win.scaledSize(win.connectionRemoveArmed ? 148 : 88) : 0
                                height: win.scaledSize(30)
                                radius: win.scaledSize(7)
                                color: removeMouse.containsMouse ? win.hoverColor : "transparent"
                                border.width: 1
                                border.color: win.dividerColor

                                Text {
                                    anchors.centerIn: parent
                                    text: win.connectionRemoveArmed
                                        ? "Remove " + (win.connection ? win.connection.displayName : "") + "?"
                                        : "Remove"
                                    color: win.accentColor
                                    elide: Text.ElideRight
                                    width: parent.width - win.scaledSize(8)
                                    horizontalAlignment: Text.AlignHCenter
                                    font.family: "iA Writer Mono S"
                                    font.pixelSize: win.scaledSize(11)
                                }

                                MouseArea {
                                    id: removeMouse
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: {
                                        if (!win.connection || !win.connection.canRemove)
                                            return;
                                        if (!win.connectionRemoveArmed) {
                                            win.connectionRemoveArmed = true;
                                            return;
                                        }
                                        win.connection.removeSelected();
                                        connectionPassword.text = "";
                                        win.connectionPasswordEdited = false;
                                        win.connectionRemoveArmed = false;
                                        if (win.connection.setupRequired)
                                            win.connectionSheetOpen = true;
                                    }
                                }
                            }

                            Item { width: 1; height: 1 }

                            Rectangle {
                                objectName: "connectionDiscard"
                                width: win.scaledSize(88)
                                height: win.scaledSize(30)
                                radius: win.scaledSize(7)
                                color: discardMouse.containsMouse ? win.hoverColor : "transparent"
                                border.width: 1
                                border.color: win.dividerColor

                                Text {
                                    anchors.centerIn: parent
                                    text: "Discard"
                                    color: win.mutedColor
                                    font.family: "iA Writer Mono S"
                                    font.pixelSize: win.scaledSize(11)
                                }

                                MouseArea {
                                    id: discardMouse
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: {
                                        if (!win.connection)
                                            return;
                                        win.connection.discard();
                                        connectionPassword.text = "";
                                        win.connectionPasswordEdited = false;
                                        win.connectionRemoveArmed = false;
                                    }
                                }
                            }

                            Rectangle {
                                objectName: "connectionApply"
                                width: win.scaledSize(88)
                                height: win.scaledSize(30)
                                radius: win.scaledSize(7)
                                color: win.connection && win.connection.problem.length === 0
                                    ? win.accentColor : win.raisedColor

                                Text {
                                    anchors.centerIn: parent
                                    text: "Apply"
                                    color: win.connection && win.connection.problem.length === 0
                                        ? "#ffffff" : win.mutedColor
                                    font.family: "iA Writer Mono S"
                                    font.bold: true
                                    font.pixelSize: win.scaledSize(11)
                                }

                                MouseArea {
                                    anchors.fill: parent
                                    enabled: win.connection && win.connection.problem.length === 0
                                    cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                                    onClicked: win.submitConnection()
                                }
                            }
                        }
                    }
                    }
                }
            }

        }

        Rectangle {
            id: membersPanel
            objectName: "membersPanel"
            Accessible.name: "Members of " + win.currentConversation
            visible: win.currentConversationIsChannel
                && win.membersVisible
                && !win.consoleVisible
                && win.width >= win.scaledSize(980)
            Layout.preferredWidth: visible ? win.scaledSize(216) : 0
            Layout.minimumWidth: visible ? win.scaledSize(196) : 0
            Layout.fillHeight: true
            color: win.panelColor

            Rectangle {
                anchors.left: parent.left
                width: 1
                height: parent.height
                color: win.dividerColor
            }

            Text {
                id: membersHeading
                anchors.top: parent.top
                anchors.topMargin: win.scaledSize(23)
                anchors.left: parent.left
                anchors.leftMargin: win.scaledSize(20)
                text: "ONLINE - " + win.currentPeopleCount
                color: win.mutedColor
                font.family: "iA Writer Mono S"
                font.bold: true
                font.letterSpacing: win.scaledSize(0.7)
                font.pixelSize: win.scaledSize(9)
            }

            ListView {
                id: membersList
                objectName: "membersList"
                anchors.top: membersHeading.bottom
                anchors.topMargin: win.scaledSize(14)
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                anchors.bottomMargin: win.scaledSize(10)
                clip: true
                model: win.irc ? win.irc.members : win.currentPeopleCount
                boundsBehavior: Flickable.StopAtBounds
                keyNavigationEnabled: true
                highlightFollowsCurrentItem: true
                highlightMoveDuration: 0
                currentIndex: 0

                Keys.onPressed: function(event) {
                    if (event.key !== Qt.Key_Return && event.key !== Qt.Key_Enter)
                        return;
                    win.activateFocusedMember();
                    event.accepted = true;
                }

                delegate: Item {
                    id: memberDelegate

                    readonly property var memberData: win.irc
                        ? ({nick: model.nick, label: model.label, status: model.status, away: model.away})
                        : win.memberDataFor(index)
                    readonly property string nick: memberData.nick
                    readonly property string label: memberData.label
                    readonly property string status: memberData.status
                    readonly property bool away: win.awayPresenceVisible && memberData.away
                    readonly property bool typing: win.typingVisible
                        && (win.irc
                            ? win.irc.nickIsTyping(memberDelegate.nick)
                            : win.typingNicks.indexOf(memberDelegate.nick) !== -1)

                    objectName: "member-" + nick
                    Accessible.name: label
                    Accessible.description: win.memberStatusVisible ? status : ""
                    Accessible.role: Accessible.Button
                    Accessible.onPressAction: {
                        if (nick !== win.selfNick)
                            win.openDirectMessage(nick);
                    }
                    width: ListView.view.width
                    height: win.scaledSize(43)

                    Rectangle {
                        objectName: "memberHighlight"
                        anchors.fill: parent
                        anchors.leftMargin: win.scaledSize(8)
                        anchors.rightMargin: win.scaledSize(8)
                        radius: win.scaledSize(7)
                        color: memberMouse.containsMouse
                            ? win.hoverColor
                            : (memberDelegate.ListView.isCurrentItem && membersList.activeFocus
                                ? win.raisedColor
                                : "transparent")
                    }

                    Rectangle {
                        anchors.left: parent.left
                        anchors.leftMargin: win.scaledSize(18)
                        anchors.verticalCenter: parent.verticalCenter
                        width: win.scaledSize(28)
                        height: width
                        radius: width / 2
                        color: win.mixColors(
                            win.pageColor,
                            win.nickColor(memberDelegate.nick),
                            win.darkMode ? 0.23 : 0.16)
                        opacity: memberDelegate.away ? 0.62 : 1

                        Text {
                            anchors.centerIn: parent
                            text: win.initials(memberDelegate.nick)
                            color: win.nickColor(memberDelegate.nick)
                            font.family: "iA Writer Mono S"
                            font.bold: true
                            font.pixelSize: win.scaledSize(11)
                        }

                        Rectangle {
                            objectName: "presence-dot-" + memberDelegate.nick
                            visible: win.awayPresenceVisible
                            anchors.right: parent.right
                            anchors.bottom: parent.bottom
                            width: win.scaledSize(7)
                            height: width
                            radius: width / 2
                            color: memberDelegate.away ? "#d6a552" : "#69b978"
                            border.width: win.scaledSize(2)
                            border.color: win.panelColor
                        }
                    }

                    Column {
                        anchors.left: parent.left
                        anchors.leftMargin: win.scaledSize(56)
                        anchors.right: parent.right
                        anchors.rightMargin: win.scaledSize(10)
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 0

                        Row {
                            width: parent.width
                            spacing: win.scaledSize(4)

                            Text {
                                width: Math.max(0, parent.width
                                    - (memberDelegate.typing
                                        ? memberTypingGlyph.implicitWidth + parent.spacing
                                        : 0))
                                text: memberDelegate.label
                                color: memberDelegate.away ? win.mutedColor : win.inkColor
                                elide: Text.ElideRight
                                font.family: "iA Writer Mono S"
                                font.bold: memberDelegate.nick === win.selfNick
                                font.pixelSize: win.scaledSize(12)
                            }

                            TypingDots {
                                id: memberTypingGlyph
                                objectName: "member-typing-" + memberDelegate.nick
                                visible: memberDelegate.typing
                                pixelSize: win.scaledSize(12)
                            }
                        }

                        Text {
                            objectName: "member-status-" + memberDelegate.nick
                            visible: win.memberStatusVisible
                                && memberDelegate.status.length > 0
                            width: parent.width
                            text: memberDelegate.status
                            color: win.mutedColor
                            elide: Text.ElideRight
                            font.family: "iA Writer Mono S"
                            font.pixelSize: win.scaledSize(9)
                        }
                    }

                    MouseArea {
                        id: memberMouse
                        anchors.fill: parent
                        enabled: memberDelegate.nick !== win.selfNick
                        hoverEnabled: true
                        cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                        onClicked: win.openDirectMessage(memberDelegate.nick)
                    }
                }
            }
        }
    }

    Popup {
        id: shortcutsSheet
        objectName: "shortcutsSheet"
        x: Math.round((win.width - width) / 2)
        y: Math.round((win.height - height) / 2)
        width: win.scaledSize(348)
        padding: win.scaledSize(16)
        modal: true
        focus: true
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        onOpened: shortcutsSheetEscapeGuard = true
        onClosed: Qt.callLater(function() { shortcutsSheetEscapeGuard = false })

        background: Rectangle {
            color: win.raisedColor
            border.width: 1
            border.color: win.dividerColor
            radius: win.scaledSize(9)
        }

        contentItem: Column {
            spacing: win.scaledSize(4)

            Repeater {
                model: [
                    { keys: "Alt+Down / Alt+Up", action: "walk conversations" },
                    { keys: "Alt+A", action: "next unread" },
                    { keys: "Ctrl+`", action: "Status" },
                    { keys: "Ctrl+,", action: "Connect" },
                    { keys: "Ctrl+Shift+M", action: "members panel" },
                    { keys: "Ctrl+Shift+P", action: "focus members" },
                    { keys: "Ctrl+W", action: "close direct message" },
                    { keys: "Ctrl+L", action: "composer" },
                    { keys: "Ctrl+F", action: "find" },
                    { keys: "Enter", action: "send" },
                    { keys: "Page Up / Page Down", action: "scroll" },
                    { keys: "Tab", action: "nick complete" },
                    { keys: "Up / Down", action: "history" },
                    { keys: "Escape", action: "dismiss" },
                    { keys: "Ctrl+/", action: "this sheet" },
                    { keys: "Ctrl+Q", action: "quit" }
                ]

                Row {
                    spacing: win.scaledSize(12)
                    width: parent.width

                    Text {
                        width: win.scaledSize(168)
                        text: modelData.keys
                        color: win.inkColor
                        font.family: "iA Writer Mono S"
                        font.pixelSize: win.scaledSize(11)
                    }

                    Text {
                        text: modelData.action
                        color: win.mutedColor
                        font.family: "iA Writer Mono S"
                        font.pixelSize: win.scaledSize(11)
                    }
                }
            }
        }
    }

    property rect normalGeometry: Qt.rect(x, y, width, height)
    property bool wasMaximized: false

    function trackNormalGeometry() {
        if (visibility === Window.Windowed)
            normalGeometry = Qt.rect(x, y, width, height);
    }

    onXChanged: trackNormalGeometry()
    onYChanged: trackNormalGeometry()
    onWidthChanged: trackNormalGeometry()
    onHeightChanged: trackNormalGeometry()

    onVisibilityChanged: function() {
        if (visibility === Window.Maximized || visibility === Window.FullScreen)
            wasMaximized = true;
        else if (visibility === Window.Windowed)
            wasMaximized = false;
    }

    Component.onCompleted: {
        composerDraftKey = composerHistoryKey();
        var geometry = backend.windowGeometry();
        if (geometry.valid) {
            x = geometry.x;
            y = geometry.y;
            width = geometry.width;
            height = geometry.height;
            if (geometry.maximized)
                showMaximized();
        } else {
            width = Math.round(1180 * backend.textScale);
            height = Math.round(760 * backend.textScale);
        }
    }

    Component.onDestruction: backend.saveWindowGeometry(
        normalGeometry.x,
        normalGeometry.y,
        normalGeometry.width,
        normalGeometry.height,
        wasMaximized)
}
