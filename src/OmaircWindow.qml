import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Layouts
import QtQuick.Window

ApplicationWindow {
    id: win

    required property var backend
    required property var irc
    property var connection: null
    property var slashCommands: null
    property bool connectionSheetOpen: false
    property string connectionSheetTab: "connection"
    property bool connectionRemoveArmed: false
    property bool connectionPasswordEdited: false
    property bool connectionNickServEdited: false

    objectName: "omaircWindow"
    width: 1180
    height: 760
    minimumWidth: 760
    minimumHeight: 540
    visible: true
    title: consoleVisible ? statusTitleText() : conversationTitleText()
    onActiveChanged: {
        if (active)
            Qt.callLater(focusConnectionSheetStart);
    }

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
    readonly property string appVersion: Qt.application.version
    readonly property var nickPalette: [
        accentColor,
        darkMode ? "#c099ff" : "#7950b8",
        darkMode ? "#7fc8a9" : "#237a58",
        darkMode ? "#efb366" : "#a45f14",
        darkMode ? "#ed8f9d" : "#b44355"
    ]
    readonly property real nickAvatarMix: darkMode ? 0.23 : 0.16
    readonly property var nickAvatarFills: [
        mixColors(pageColor, nickPalette[0], nickAvatarMix),
        mixColors(pageColor, nickPalette[1], nickAvatarMix),
        mixColors(pageColor, nickPalette[2], nickAvatarMix),
        mixColors(pageColor, nickPalette[3], nickAvatarMix),
        mixColors(pageColor, nickPalette[4], nickAvatarMix)
    ]

    property bool membersVisible: true
    property bool shortcutsSheetEscapeGuard: false
    property bool pickerEscapeGuard: false
    property string sidebarNetworkFocusId: ""
    property int jumpSelectedIndex: 0
    property int nickSelectedIndex: 0
    property var nickSourceRows: []
    readonly property bool shortcutOverlayOpen: shortcutsSheet.opened
        || jumpSheet.opened
        || nickSheet.opened
    readonly property var networkConsole: irc
        ? (irc.statusConsole ? irc.statusConsole : irc.console)
        : null
    readonly property bool consoleVisible: networkConsole
        ? (networkConsole.open || irc.selectedTarget.length === 0)
        : false
    onConsoleVisibleChanged: {
        resetNickComplete();
        if (!abandonFind() && !suppressComposerStash)
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
    readonly property string currentConversation: irc ? irc.selectedTarget : ""
    readonly property string currentNetworkId: irc ? irc.focusedNetworkId : ""
    readonly property string currentConversationId: irc ? irc.selectedConversationId : ""
    onCurrentConversationIdChanged: {
        resetNickComplete();
        if (!abandonFind() && !suppressComposerStash)
            stashComposerDraft();
        restoreComposerDraft();
        resetComposerHistoryBrowse();
    }
    onCurrentNetworkIdChanged: {
        if (!consoleVisible)
            return;
        resetNickComplete();
        if (!abandonFind() && !suppressComposerStash)
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
        : ""
    readonly property var activeMessages: irc ? irc.messages : null
    readonly property font transcriptBodyFont: Qt.font({
        family: "iA Writer Mono S",
        pixelSize: scaledSize(13)
    })
    // One line of transcript body text. The typing indicator claims the slot a
    // one-line message would occupy, so its box comes from this rather than
    // from the dots, whose glyphs are taller than a line of text.
    readonly property int messageLineHeight: messageLineProbe.implicitHeight
    readonly property bool currentConversationIsChannel: irc
        ? irc.isChannel : currentConversation.charAt(0) === "#"
    readonly property int currentPeopleCount: irc ? irc.peopleCount : 0
    readonly property bool memberStatusVisible: !irc || irc.hasMemberStatus
    readonly property bool awayPresenceVisible: !irc || irc.hasAwayPresence
    readonly property bool selfAway: irc ? irc.selfAway : false
    readonly property bool typingVisible: !irc || irc.hasTyping
    readonly property var typingNicks: irc ? irc.typingNicks : []
    readonly property string selfNick: {
        if (irc) {
            var live = irc.currentNick
            if (live && live.length > 0)
                return live
            if (connection && connection.nick && connection.nick.length > 0)
                return connection.nick
        }
        return ""
    }
    readonly property bool connectionOverlayVisible: connection
        && (connection.setupRequired || connectionSheetOpen)

    property var composerHistories: ({})
    property var composerDrafts: ({})
    property string composerDraftKey: ""
    property bool suppressComposerStash: false
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

    component TranscriptNickHit: MouseArea {
        required property string nick
        property bool nickOpensDirect: false

        objectName: "transcriptNickHit"
        anchors.fill: parent
        enabled: nickOpensDirect && nick.length > 0 && nick !== win.selfNick
        hoverEnabled: true
        cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
        Accessible.role: Accessible.Button
        Accessible.name: nick
        Accessible.ignored: !enabled
        Accessible.onPressAction: {
            if (enabled)
                win.openDirectMessage(nick);
        }
        onClicked: win.openDirectMessage(nick)
    }

    component PlainUrlHit: MouseArea {
        required property Item edit
        property bool inviteHits: false

        objectName: "urlHit"
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton
        cursorShape: {
            var pos = edit.positionAt(mouseX, mouseY);
            var visible = win.editVisibleText(edit);
            if (win.httpUrlAt(visible, pos).length > 0)
                return Qt.PointingHandCursor;
            if (inviteHits && win.inviteChannelAt(visible, pos).length > 0)
                return Qt.PointingHandCursor;
            return Qt.IBeamCursor;
        }
        onPressed: function(mouse) {
            var pos = edit.positionAt(mouse.x, mouse.y);
            var visible = win.editVisibleText(edit);
            if (win.httpUrlAt(visible, pos).length > 0)
                return;
            if (inviteHits && win.inviteChannelAt(visible, pos).length > 0)
                return;
            mouse.accepted = false;
        }
        onClicked: function(mouse) {
            var pos = edit.positionAt(mouse.x, mouse.y);
            var visible = win.editVisibleText(edit);
            var url = win.httpUrlAt(visible, pos);
            if (url.length > 0) {
                win.openAllowedUrl(url);
                return;
            }
            if (inviteHits)
                win.joinInviteChannel(win.inviteChannelAt(visible, pos));
        }
    }

    component TypingDots: Row {
        id: dots

        property color ink: win.mutedColor
        property int pixelSize: win.scaledSize(12)
        property string describedAs: ""
        property int pulse: 0

        Accessible.role: Accessible.StaticText
        Accessible.ignored: dots.describedAs.length === 0
        // Name is for inspection when focus lands; appearance is not a live region.
        Accessible.name: dots.describedAs
        spacing: 0

        Timer {
            interval: 320
            repeat: true
            running: dots.visible
            onTriggered: dots.pulse = (dots.pulse + 1) % 3
        }

        Repeater {
            model: 3
            Text {
                text: "."
                color: dots.ink
                opacity: dots.pulse === index ? 1 : 0.28
                font.family: "iA Writer Mono S"
                font.pixelSize: dots.pixelSize
            }
        }
    }

    Text {
        id: messageLineProbe
        visible: false
        text: "X"
        font.family: win.transcriptBodyFont.family
        font.pixelSize: win.transcriptBodyFont.pixelSize
    }

    component MessageAvatar: Rectangle {
        id: avatar

        required property string author
        required property bool replayed
        property bool nickOpensDirect: false

        anchors.left: parent.left
        anchors.leftMargin: win.scaledSize(24)
        anchors.top: parent.top
        anchors.topMargin: win.scaledSize(9)
        width: win.scaledSize(34)
        height: width
        radius: width / 2
        color: win.mixColors(win.pageColor,
                             avatar.replayed ? win.mutedColor : win.nickColor(avatar.author),
                             win.darkMode ? 0.23 : 0.16)

        Text {
            objectName: "messageAvatarInitial"
            anchors.centerIn: parent
            text: win.initials(avatar.author)
            color: avatar.replayed ? win.mutedColor : win.nickColor(avatar.author)
            font.family: "iA Writer Mono S"
            font.bold: true
            font.pixelSize: win.scaledSize(13)
        }

        TranscriptNickHit {
            nick: avatar.author
            nickOpensDirect: avatar.nickOpensDirect
        }
    }

    component MessageHeader: Row {
        id: header

        required property string author
        required property string time
        required property bool replayed
        property bool nickOpensDirect: false

        anchors.left: parent.left
        anchors.leftMargin: win.scaledSize(70)
        anchors.top: parent.top
        anchors.topMargin: win.scaledSize(8)
        spacing: win.scaledSize(9)

        Text {
            objectName: "messageAuthor"
            text: header.author
            color: header.replayed ? win.mutedColor : win.nickColor(header.author)
            font.family: "iA Writer Mono S"
            font.bold: true
            font.pixelSize: win.scaledSize(12)

            TranscriptNickHit {
                nick: header.author
                nickOpensDirect: header.nickOpensDirect
            }
        }

        Text {
            anchors.baseline: parent.children[0].baseline
            text: header.time
            color: win.mutedColor
            font.family: "iA Writer Mono S"
            font.pixelSize: win.scaledSize(9)
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

    function paletteColor(index) {
        return nickPalette[index];
    }

    function nickPaletteIndex(nick) {
        var hash = 0;
        for (var index = 0; index < nick.length; ++index)
            hash = (hash + nick.charCodeAt(index)) % 5;
        return hash;
    }

    function nickColor(nick) {
        return paletteColor(nickPaletteIndex(nick));
    }

    function initials(nick) {
        return nick.length > 0 ? nick.charAt(0).toUpperCase() : "?";
    }

    function stripIrcColors(text) {
        return text.replace(/\x03(?:\d{1,2}(?:,\d{1,2})?)?/g, "")
            .replace(/\x04(?:[0-9A-Fa-f]{6}(?:,[0-9A-Fa-f]{6})?)?/g, "");
    }

    function plainIrcText(text) {
        return stripIrcColors(text)
            .replace(/[\x02\x0f\x11\x16\x1d\x1e\x1f]/g, "");
    }

    function escapeHtml(text) {
        return String(text).replace(/&/g, "&amp;")
            .replace(/</g, "&lt;")
            .replace(/>/g, "&gt;");
    }

    function hasIrcEmphasis(text) {
        return /[\x02\x1d\x1f]/.test(text);
    }

    function editVisibleText(edit) {
        if (!edit)
            return "";
        if (edit.textFormat === TextEdit.RichText)
            return edit.getText(0, edit.length);
        return edit.text;
    }

    function emphasizedIrcText(text) {
        var input = stripIrcColors(text);
        var bold = false;
        var italic = false;
        var underline = false;
        var openBold = false;
        var openItalic = false;
        var openUnderline = false;
        var html = "";
        var index;
        for (index = 0; index < input.length; ++index) {
            var code = input.charCodeAt(index);
            var changed = false;
            if (code === 0x02) {
                bold = !bold;
                changed = true;
            } else if (code === 0x1d) {
                italic = !italic;
                changed = true;
            } else if (code === 0x1f) {
                underline = !underline;
                changed = true;
            } else if (code === 0x0f) {
                bold = false;
                italic = false;
                underline = false;
                changed = true;
            } else if (code !== 0x16 && code !== 0x11 && code !== 0x1e) {
                html += escapeHtml(input.charAt(index));
            }
            if (!changed)
                continue;
            if (openUnderline) {
                html += "</u>";
                openUnderline = false;
            }
            if (openItalic) {
                html += "</i>";
                openItalic = false;
            }
            if (openBold) {
                html += "</b>";
                openBold = false;
            }
            if (bold) {
                html += "<b>";
                openBold = true;
            }
            if (italic) {
                html += "<i>";
                openItalic = true;
            }
            if (underline) {
                html += "<u>";
                openUnderline = true;
            }
        }
        if (openUnderline)
            html += "</u>";
        if (openItalic)
            html += "</i>";
        if (openBold)
            html += "</b>";
        // Defence-in-depth: unreachable while every non-control character goes through escapeHtml.
        if (/</.test(html.replace(/<\/?[biu]>/g, "")))
            html = escapeHtml(plainIrcText(text));
        return "<span style=\"white-space: pre-wrap;\">" + html + "</span>";
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
        if (typeof model.field !== "function")
            return "";
        var value = model.field(row, name);
        return value == null ? "" : String(value);
    }

    function continuesMessageGroup(model, row, author, time, kind, origin) {
        if (kind === "event" || kind === "whois"
            || row <= 0 || author.length === 0 || time.length === 0)
            return false;
        var previousKind = transcriptField(model, row - 1, "kind");
        if (previousKind === "event" || previousKind === "whois"
            || previousKind.length === 0)
            return false;
        if (transcriptField(model, row - 1, "origin") !== origin)
            return false;
        return transcriptField(model, row - 1, "author") === author
            && transcriptField(model, row - 1, "time") === time;
    }

    function currentTranscriptMinute() {
        // Same local HH:mm as MessageListModel::displayTime.
        return Qt.formatDateTime(new Date(), "HH:mm");
    }

    function bodyTextTopMargin(grouped) {
        return scaledSize(grouped ? 4 : 29);
    }

    // compact is grouped-or-whois: the shorter continuation row.
    function transcriptRowHeight(isEvent, compact, contentHeight) {
        if (isEvent)
            return Math.max(scaledSize(42), contentHeight + scaledSize(8));
        return compact
            ? Math.max(scaledSize(22), contentHeight + scaledSize(8))
            : Math.max(scaledSize(58), contentHeight + scaledSize(39));
    }

    function typingFollowsPeerChat(model, nick, currentMinute) {
        // The footer has no timestamp of its own. Treat it as the next live
        // chat row from the peer arriving now, using the same local HH:mm
        // MessageListModel::displayTime would assign that row.
        return continuesMessageGroup(model, transcriptRowCount(model), nick,
                                     currentMinute, "message", "live");
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

    function inviteChannelAt(text, index) {
        if (!text || index < 0 || index >= text.length)
            return "";
        var marker = " invited you to ";
        var at = text.lastIndexOf(marker);
        if (at < 0)
            return "";
        var start = at + marker.length;
        var channel = text.substring(start);
        if (channel.length === 0)
            return "";
        if (index >= start && index < start + channel.length)
            return channel;
        return "";
    }

    function joinInviteChannel(channel) {
        if (!channel || !networkConsole)
            return false;
        return networkConsole.submit("/join " + channel);
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

    function notifyMentionIfUnfocused(windowActive, author, body, networkId, target, msgid) {
        if (windowActive)
            return;
        var text = plainIrcText(body);
        lastNotification = { author: author, body: text };
        if (networkId)
            lastNotification.networkId = networkId;
        if (target)
            lastNotification.target = target;
        if (msgid)
            lastNotification.msgid = msgid;
        if (suppressDesktopNotification)
            return;
        backend.notifyDesktop(author, text, networkId || "", target || "", msgid || "");
    }

    function msgidRow(msgid) {
        if (!msgid)
            return -1;
        var model = irc ? irc.messages : null;
        if (!model)
            return -1;
        var needle = String(msgid);
        var count = transcriptRowCount(model);
        for (var row = 0; row < count; ++row) {
            if (transcriptField(model, row, "msgid") === needle)
                return row;
        }
        return -1;
    }

    function activateNotifiedConversation(networkId, target, msgid) {
        win.show();
        win.raise();
        win.requestActivate();
        if (!irc || !networkId || !target)
            return;
        sidebarNetworkFocusId = "";
        irc.revealConversation(networkId, target);
        Qt.callLater(function() {
            if (irc.selectedNetworkId !== networkId || irc.selectedTarget !== target)
                return;
            var id = msgid ? String(msgid).trim() : "";
            var row = id.length > 0 ? win.msgidRow(id) : -1;
            if (row < 0)
                messageList.pinToEnd();
            else
                win.revealFindMatch(row);
            composer.forceActiveFocus();
        });
    }

    function canOpenDirectMessage(nick) {
        return nick.length > 0 && nick !== selfNick;
    }

    function openDirectMessage(nick) {
        if (!irc)
            return;
        irc.openDirectMessage(nick);
        Qt.callLater(function() {
            messageList.pinToEnd();
            composer.forceActiveFocus();
        });
    }

    function closeDirectMessage() {
        if (!irc)
            return;
        irc.closeDirectMessage();
        Qt.callLater(function() {
            messageList.pinToEnd();
            composer.forceActiveFocus();
        });
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
        if (!canOpenDirectMessage(nick))
            return;
        win.openDirectMessage(nick);
    }

    function selectConversation(name, networkId) {
        sidebarNetworkFocusId = "";
        if (!irc)
            return;
        var id = networkId && networkId.length ? networkId : irc.selectedNetworkId;
        irc.selectConversation(id, name);
        Qt.callLater(function() {
            messageList.pinToEnd();
            composer.forceActiveFocus();
        });
    }

    function openNetworkStatus(networkId) {
        sidebarNetworkFocusId = "";
        if (!irc)
            return;
        irc.openStatus(networkId);
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
        }
        return rows;
    }

    function sectionHasDirects(networkId) {
        if (!irc)
            return false;
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

    function sidebarNetworkSections() {
        var sections = [];

        function appendSection(section) {
            if (!section || section.visible === false)
                return;
            if (section.networkId === undefined || section.networkId.length === 0)
                return;
            if (section.headerItem === undefined)
                return;
            sections.push(section);
        }

        if (irc && connection) {
            for (var liveIndex = 0; liveIndex < liveNetworkRepeater.count; ++liveIndex) {
                var column = liveNetworkRepeater.itemAt(liveIndex);
                if (!column)
                    continue;
                var kids = column.children;
                for (var child = 0; child < kids.length; ++child) {
                    if (kids[child] && kids[child].headerItem !== undefined) {
                        appendSection(kids[child]);
                        break;
                    }
                }
            }
        }
        return sections;
    }

    function jumpTargetLabel(kind, name, networkName) {
        if (kind === "status") {
            if (networkName && networkName.length > 0)
                return networkName + " Status";
            return "Status";
        }
        if (duplicateTargetName(name) && networkName && networkName.length > 0)
            return name + " · " + networkName;
        return name;
    }

    function refreshJumpMatches() {
        var query = jumpFilter ? jumpFilter.text.trim().toLowerCase() : "";
        var rows = sidebarConversationRows();
        var sections = sidebarNetworkSections();
        jumpModel.clear();
        for (var sectionIndex = 0; sectionIndex < sections.length; ++sectionIndex) {
            var section = sections[sectionIndex];
            var networkName = section.displayName || "";
            var rowIndex;
            for (rowIndex = 0; rowIndex < rows.length; ++rowIndex) {
                if (rows[rowIndex].networkId !== section.networkId)
                    continue;
                var conversationLabel = jumpTargetLabel("conversation",
                    rows[rowIndex].conversationName, networkName);
                if (query.length === 0
                        || conversationLabel.toLowerCase().indexOf(query) !== -1)
                    jumpModel.append({
                        kind: "conversation",
                        name: rows[rowIndex].conversationName,
                        networkId: rows[rowIndex].networkId,
                        conversationId: rows[rowIndex].conversationId,
                        label: conversationLabel
                    });
            }
            var statusLabel = jumpTargetLabel("status", "Status", networkName);
            if (query.length === 0
                    || statusLabel.toLowerCase().indexOf(query) !== -1)
                jumpModel.append({
                    kind: "status",
                    name: "Status",
                    networkId: section.networkId,
                    conversationId: "",
                    label: statusLabel
                });
        }
        if (jumpSelectedIndex >= jumpModel.count)
            jumpSelectedIndex = Math.max(0, jumpModel.count - 1);
    }

    function liveMemberRow(row) {
        var empty = { nick: "", label: "", status: "", away: false };
        if (!irc)
            return empty;
        var model = irc.members;
        if (!model)
            return empty;
        if (typeof model.get === "function") {
            var rowData = model.get(row);
            return {
                nick: rowData && rowData.nick ? rowData.nick : "",
                label: rowData && rowData.label ? rowData.label : "",
                status: rowData && rowData.status ? rowData.status : "",
                away: !!(rowData && rowData.away)
            };
        }
        var idx = model.index(row, 0);
        return {
            nick: model.data(idx, Qt.UserRole + 1) || "",
            label: model.data(idx, Qt.UserRole + 2) || "",
            status: model.data(idx, Qt.UserRole + 3) || "",
            away: model.data(idx, Qt.UserRole + 4) === true
        };
    }

    function snapshotChannelMembers() {
        var rows = [];
        var count = memberCount();
        for (var row = 0; row < count; ++row) {
            var member = liveMemberRow(row);
            if (member.nick.length === 0)
                continue;
            rows.push(member);
        }
        return rows;
    }

    function refreshNickMatches() {
        var query = nickFilter ? nickFilter.text.trim().toLowerCase() : "";
        nickModel.clear();
        var source = nickSourceRows;
        for (var row = 0; row < source.length; ++row) {
            var member = source[row];
            if (query.length === 0
                    || member.nick.toLowerCase().indexOf(query) !== -1)
                nickModel.append({
                    name: member.nick,
                    label: member.label,
                    memberStatus: member.status,
                    awayFlag: member.away ? 1 : 0,
                    glyph: initials(member.nick),
                    paletteIndex: nickPaletteIndex(member.nick)
                });
        }
        if (nickSelectedIndex >= nickModel.count)
            nickSelectedIndex = Math.max(0, nickModel.count - 1);
    }

    function firstOpenableNickIndex() {
        for (var index = 0; index < nickModel.count; ++index) {
            if (canOpenDirectMessage(nickModel.get(index).name))
                return index;
        }
        return 0;
    }

    function openJumpSheet() {
        jumpSelectedIndex = 0;
        jumpSheet.open();
    }

    function openNickSheet() {
        nickSourceRows = snapshotChannelMembers();
        nickSelectedIndex = 0;
        nickSheet.open();
    }

    function stepJump(delta) {
        if (jumpModel.count === 0)
            return;
        jumpSelectedIndex = (jumpSelectedIndex + delta + jumpModel.count) % jumpModel.count;
        if (jumpList)
            jumpList.positionViewAtIndex(jumpSelectedIndex, ListView.Contain);
    }

    function stepNick(delta) {
        if (nickModel.count === 0)
            return;
        nickSelectedIndex = (nickSelectedIndex + delta + nickModel.count) % nickModel.count;
        if (nickList)
            nickList.positionViewAtIndex(nickSelectedIndex, ListView.Contain);
    }

    function activateJumpSelection() {
        if (jumpSelectedIndex < 0 || jumpSelectedIndex >= jumpModel.count)
            return;
        var target = jumpModel.get(jumpSelectedIndex);
        var kind = target.kind;
        var networkId = target.networkId;
        var name = target.name;
        var conversationId = target.conversationId;
        jumpSheet.close();
        if (kind === "status") {
            openNetworkStatus(networkId);
            return;
        }
        var rows = sidebarConversationRows();
        for (var index = 0; index < rows.length; ++index) {
            if (rows[index].conversationId === conversationId) {
                rows[index].activate();
                return;
            }
        }
        selectConversation(name, networkId);
    }

    function activateNickSelection() {
        if (nickSelectedIndex < 0 || nickSelectedIndex >= nickModel.count)
            return;
        var nick = nickModel.get(nickSelectedIndex).name;
        if (!canOpenDirectMessage(nick))
            return;
        nickSheet.close();
        win.openDirectMessage(nick);
    }

    function focusNetworkHeader(section) {
        if (!section)
            return;
        sidebarNetworkFocusId = section.networkId;
        revealSidebarRow(section.headerItem);
    }

    function activateFocusedNetworkHeader() {
        var id = sidebarNetworkFocusId;
        if (id.length === 0)
            return;
        openNetworkStatus(id);
    }

    function stepNetwork(delta) {
        var sections = sidebarNetworkSections();
        if (sections.length === 0)
            return;

        var current = -1;
        var index;
        for (index = 0; index < sections.length; ++index) {
            if (sections[index].networkId === sidebarNetworkFocusId) {
                current = index;
                break;
            }
        }
        if (current < 0) {
            for (index = 0; index < sections.length; ++index) {
                if (sections[index].networkId === currentNetworkId) {
                    current = index;
                    break;
                }
            }
        }

        var nextIndex = current < 0
            ? (delta > 0 ? 0 : sections.length - 1)
            : (current + delta + sections.length) % sections.length;
        focusNetworkHeader(sections[nextIndex]);
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
            if (mentionRow === null && row.mention === true && row.muted !== true)
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
            var statusNetwork = irc ? irc.focusedNetworkId : "";
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
        if (consoleVisible)
            return transcriptField(model, row, "label")
                + " " + transcriptField(model, row, "text");
        return transcriptField(model, row, "author")
            + " " + transcriptField(model, row, "body");
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
        if (!irc)
            return "";
        return liveMemberNick(irc.members, index);
    }

    function nickCompleteCandidates() {
        if (consoleVisible)
            return [];
        if (!currentConversationIsChannel)
            return currentConversation.length > 0 ? [currentConversation] : [];

        var nicks = [];
        if (!irc)
            return nicks;
        var model = irc.members;
        var count = liveMemberCount(model);
        for (var row = 0; row < count; ++row) {
            var liveNick = liveMemberNick(model, row);
            if (liveNick.length > 0)
                nicks.push(liveNick);
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

        var fromConsole = consoleVisible;
        var sentFromKey = composerDraftKey;
        suppressComposerStash = true;
        if (sentFromKey.length > 0)
            composerDrafts[sentFromKey] = "";
        composer.clear();

        var sent = false;
        if (fromConsole)
            sent = irc && networkConsole && networkConsole.submit(original);
        else
            sent = irc && irc.sendMessage(original);
        suppressComposerStash = false;

        if (!sent) {
            composer.text = original;
            composer.cursorPosition = composer.text.length;
            if (sentFromKey.length > 0 && composerDraftKey === sentFromKey)
                composerDrafts[sentFromKey] = original;
            if (fromConsole)
                consoleList.pinToEnd();
            else
                messageList.pinToEnd();
            return;
        }

        rememberSentComposerLine(original);
        var kept = composerDrafts[composerDraftKey] || "";
        composer.text = kept;
        composer.cursorPosition = composer.text.length;
        if (consoleVisible)
            consoleList.pinToEnd();
        else
            messageList.pinToEnd();
        Qt.callLater(function() {
            if (composer.text.trim() === original && kept !== original) {
                composer.text = kept;
                composer.cursorPosition = composer.text.length;
            }
            composer.forceActiveFocus();
        });
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
        onActivated: {
            sidebarNetworkFocusId = "";
            composer.forceActiveFocus();
        }
    }

    Shortcut {
        sequence: "Ctrl+F"
        context: Qt.ApplicationShortcut
        enabled: !win.connectionOverlayVisible && !win.shortcutOverlayOpen
        onActivated: win.beginOrAdvanceFind()
    }

    Shortcut {
        sequence: "Ctrl+K"
        context: Qt.ApplicationShortcut
        enabled: !win.connectionOverlayVisible
            && !shortcutsSheet.opened
            && !nickSheet.opened
        onActivated: {
            if (jumpSheet.opened)
                jumpSheet.close();
            else
                win.openJumpSheet();
        }
    }

    Shortcut {
        sequence: "Ctrl+Shift+K"
        context: Qt.ApplicationShortcut
        enabled: currentConversationIsChannel
            && !consoleVisible
            && !win.connectionOverlayVisible
            && !win.shortcutOverlayOpen
        onActivated: win.openNickSheet()
    }

    Shortcut {
        sequence: "Ctrl+Shift+M"
        context: Qt.ApplicationShortcut
        enabled: currentConversationIsChannel
            && !consoleVisible
            && !win.connectionOverlayVisible
            && !win.shortcutOverlayOpen
        onActivated: membersVisible = !membersVisible
    }

    Shortcut {
        sequence: "Ctrl+Shift+P"
        context: Qt.ApplicationShortcut
        enabled: currentConversationIsChannel
            && !consoleVisible
            && !win.connectionOverlayVisible
            && !win.shortcutOverlayOpen
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
            if (jumpSheet.opened || nickSheet.opened)
                return;
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
                if (win.networkConsole)
                    win.networkConsole.open = false;
                return;
            }
            var id = win.irc
                ? (win.irc.focusedNetworkId || win.irc.selectedNetworkId)
                : "";
            if (id && id.length > 0)
                win.openNetworkStatus(id);
        }
    }

    Shortcut {
        sequence: "Ctrl+,"
        context: Qt.ApplicationShortcut
        enabled: win.connection !== null && !win.shortcutOverlayOpen
        onActivated: {
            var id = win.sidebarNetworkFocusId;
            if (id.length === 0)
                id = win.irc ? win.irc.focusedNetworkId : "";
            if (id.length > 0)
                win.selectSheetNetwork(id);
            win.connectionSheetOpen = true;
        }
    }

    // Apply is a window-level action, not a per-control one. Wiring it into
    // each control meant Ctrl+Enter silently did nothing whenever focus sat
    // somewhere without its own handler, such as a network row or a footer
    // button. Qt matches Return and the keypad's Enter separately.
    Shortcut {
        sequence: "Ctrl+Return"
        context: Qt.ApplicationShortcut
        enabled: win.connectionOverlayVisible
            && win.connectionSheetTab === "connection"
        onActivated: win.submitConnection()
    }

    Shortcut {
        sequence: "Ctrl+Enter"
        context: Qt.ApplicationShortcut
        enabled: win.connectionOverlayVisible
            && win.connectionSheetTab === "connection"
        onActivated: win.submitConnection()
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
        sequence: "Alt+Right"
        context: Qt.ApplicationShortcut
        enabled: !win.shortcutOverlayOpen
        onActivated: stepNetwork(1)
    }

    Shortcut {
        sequence: "Alt+Left"
        context: Qt.ApplicationShortcut
        enabled: !win.shortcutOverlayOpen
        onActivated: stepNetwork(-1)
    }

    Shortcut {
        sequence: "Return"
        context: Qt.ApplicationShortcut
        enabled: win.sidebarNetworkFocusId.length > 0
            && !win.findActive
            && !win.connectionOverlayVisible
            && !win.shortcutOverlayOpen
        onActivated: activateFocusedNetworkHeader()
    }

    Shortcut {
        sequence: "Enter"
        context: Qt.ApplicationShortcut
        enabled: win.sidebarNetworkFocusId.length > 0
            && !win.findActive
            && !win.connectionOverlayVisible
            && !win.shortcutOverlayOpen
        onActivated: activateFocusedNetworkHeader()
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
            if (jumpSheet.opened || nickSheet.opened || pickerEscapeGuard)
                return true;
            if (win.connection && win.connection.setupRequired)
                return false;
            if (win.connection && win.connectionSheetOpen)
                return true;
            if (win.findActive)
                return true;
            if (!win.consoleVisible)
                return false;
            return win.irc && win.irc.selectedTarget.length > 0;
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
            if (jumpSheet.opened || nickSheet.opened || pickerEscapeGuard) {
                jumpSheet.close();
                nickSheet.close();
                pickerEscapeGuard = false;
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
            if (win.networkConsole)
                win.networkConsole.open = false;
        }
    }

    component ConnectionField: Column {
        id: field

        property string label
        property string badge: ""
        property string help: ""
        property alias fieldObjectName: input.objectName
        property alias text: input.text
        property bool secret: false

        signal textEdited(string text)

        function focusInput() {
            input.forceActiveFocus();
        }

        width: parent ? parent.width : 0
        spacing: win.scaledSize(4)

        Row {
            id: labelRow
            height: win.scaledSize(15)
            spacing: win.scaledSize(6)

            Text {
                height: labelRow.height
                text: field.label
                color: win.mutedColor
                verticalAlignment: Text.AlignVCenter
                font.family: "iA Writer Mono S"
                font.pixelSize: win.scaledSize(10)
            }

            Rectangle {
                id: badgeChip
                objectName: field.fieldObjectName + "Badge"
                visible: field.badge.length > 0
                width: visible ? badgeChipLabel.implicitWidth + win.scaledSize(10) : 0
                height: labelRow.height
                radius: win.scaledSize(4)
                color: win.panelColor
                border.width: 1
                border.color: win.dividerColor

                Text {
                    id: badgeChipLabel
                    anchors.centerIn: parent
                    text: field.badge
                    color: win.mutedColor
                    font.family: "iA Writer Mono S"
                    font.pixelSize: win.scaledSize(9)
                }
            }

            Item {
                id: helpMark
                objectName: field.fieldObjectName + "Help"
                visible: field.help.length > 0
                width: win.scaledSize(14)
                height: labelRow.height
                activeFocusOnTab: visible
                Accessible.role: Accessible.Button
                Accessible.name: field.label + " help"
                Accessible.description: field.help
                Accessible.onPressAction: helpMark.forceActiveFocus()

                Rectangle {
                    id: helpCircle
                    anchors.verticalCenter: parent.verticalCenter
                    width: win.scaledSize(14)
                    height: width
                    radius: width / 2
                    color: "transparent"
                    border.width: 1
                    border.color: helpMark.activeFocus ? win.accentColor : win.dividerColor
                }

                Text {
                    anchors.centerIn: helpCircle
                    text: "?"
                    color: helpMark.activeFocus ? win.inkColor : win.mutedColor
                    font.family: "iA Writer Mono S"
                    font.pixelSize: win.scaledSize(9)
                }

                MouseArea {
                    id: helpMouse
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.WhatsThisCursor
                    // Fully declarative: mixing ToolTip.show()/hide() with a
                    // `visible` binding writes underneath the binding and can
                    // desync it. Shown on hover and on keyboard focus so the
                    // help is readable without a mouse.
                    ToolTip.visible: containsMouse || helpMark.activeFocus
                    ToolTip.text: field.help
                    ToolTip.delay: 400
                    ToolTip.objectName: field.fieldObjectName + "HelpTip"
                    onClicked: helpMark.forceActiveFocus()
                }
            }
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
                Accessible.description: field.help
                background: Item {}
                onTextEdited: field.textEdited(text)
                Keys.onPressed: function(event) {
                    if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter)
                        win.applyFromSheetKey(event);
                }
            }
        }
    }

    component ConnectionTabButton: Rectangle {
        id: tabButton

        property string tabName
        property string label
        property string glyph: ""

        readonly property bool current: win.connectionSheetTab === tabName
        readonly property color tone: current ? win.inkColor : win.mutedColor

        signal stepRequested(int direction)

        objectName: "connectionSheetTab-" + tabName
        width: tabContent.implicitWidth + win.scaledSize(26)
        height: win.scaledSize(52)
        color: tabMouse.containsMouse || activeFocus ? win.hoverColor : "transparent"
        activeFocusOnTab: true
        Accessible.role: Accessible.PageTab
        Accessible.name: label
        Accessible.selected: current
        Accessible.onPressAction: win.selectConnectionSheetTab(tabName)

        Keys.onPressed: function(event) {
            if (event.key === Qt.Key_Left || event.key === Qt.Key_Right) {
                stepRequested(event.key === Qt.Key_Right ? 1 : -1);
                event.accepted = true;
                return;
            }
            if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter
                    || event.key === Qt.Key_Space) {
                win.selectConnectionSheetTab(tabName);
                event.accepted = true;
            }
        }

        Row {
            id: tabContent
            anchors.centerIn: parent
            spacing: win.scaledSize(7)

            Text {
                visible: tabButton.glyph.length > 0
                text: tabButton.glyph
                color: tabButton.tone
                font.family: "iA Writer Mono S"
                font.bold: true
                font.pixelSize: win.scaledSize(12)
            }

            Item {
                visible: tabButton.glyph.length === 0
                width: win.scaledSize(12)
                height: win.scaledSize(15)

                Rectangle {
                    x: 0
                    y: win.scaledSize(4)
                    width: parent.width
                    height: Math.max(1, win.scaledSize(1))
                    color: tabButton.tone
                }

                Rectangle {
                    x: win.scaledSize(7)
                    y: win.scaledSize(2)
                    width: win.scaledSize(5)
                    height: win.scaledSize(5)
                    radius: win.scaledSize(1)
                    color: tabButton.tone
                }

                Rectangle {
                    x: 0
                    y: win.scaledSize(10)
                    width: parent.width
                    height: Math.max(1, win.scaledSize(1))
                    color: tabButton.tone
                }

                Rectangle {
                    x: win.scaledSize(1)
                    y: win.scaledSize(8)
                    width: win.scaledSize(5)
                    height: win.scaledSize(5)
                    radius: win.scaledSize(1)
                    color: tabButton.tone
                }
            }

            Text {
                text: tabButton.label
                color: tabButton.tone
                font.family: "iA Writer Mono S"
                font.pixelSize: win.scaledSize(12)
            }
        }

        Rectangle {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: Math.max(1, win.scaledSize(2))
            radius: height / 2
            visible: tabButton.current
            color: win.accentColor
        }

        MouseArea {
            id: tabMouse
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: {
                win.selectConnectionSheetTab(tabButton.tabName);
                tabButton.forceActiveFocus();
            }
        }
    }

    function clearConnectionPassword() {
        connectionPassword.text = "";
        connectionPasswordEdited = false;
        connectionNickServ.text = "";
        connectionNickServEdited = false;
    }

    function submitConnection() {
        if (!connection || connection.problem.length > 0)
            return;
        if (connectionPasswordEdited)
            connection.setPassword(connectionPassword.text);
        if (connectionNickServEdited)
            connection.setNickServPassword(connectionNickServ.text);
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

    // One ordered list drives tab order, stepping and focus, so adding a third
    // tab does not leave hard-coded pairs behind.
    readonly property var connectionSheetTabOrder: ["connection", "preferences"]

    function selectConnectionSheetTab(tabName) {
        connectionSheetTab = tabName;
    }

    function focusConnectionSheetTab(tabName) {
        var buttons = connectionTabs.children;
        for (var index = 0; index < buttons.length; ++index) {
            if (buttons[index].tabName === tabName) {
                buttons[index].forceActiveFocus();
                return;
            }
        }
    }

    function stepConnectionSheetTab(direction) {
        var tabs = connectionSheetTabOrder;
        var current = tabs.indexOf(connectionSheetTab);
        if (current < 0)
            current = 0;
        var step = direction < 0 ? -1 : 1;
        var next = (current + step + tabs.length) % tabs.length;
        connectionSheetTab = tabs[next];
        focusConnectionSheetTab(connectionSheetTab);
    }

    function addSheetNetwork() {
        if (!connection || !connection.add())
            return;
        clearConnectionPassword();
        connectionRemoveArmed = false;
    }

    function discardSheetConnection() {
        if (!connection)
            return;
        connection.discard();
        clearConnectionPassword();
        connectionRemoveArmed = false;
    }

    function removeSheetNetwork() {
        if (!connection || !connection.canRemove)
            return;
        if (!connectionRemoveArmed) {
            connectionRemoveArmed = true;
            return;
        }
        connection.removeSelected();
        clearConnectionPassword();
        connectionRemoveArmed = false;
        if (connection.setupRequired)
            connectionSheetOpen = true;
    }

    function disconnectSheetNetwork() {
        if (!connection)
            return;
        connection.disconnectSelected();
    }

    function focusConnectionSheetStart() {
        if (!connectionOverlayVisible || !win.active)
            return;
        if (connection && (connection.focusPassword || connection.focusNickServ))
            return;
        var focused = win.activeFocusItem;
        while (focused) {
            if (focused.objectName && focused.objectName.indexOf("connection") === 0
                    && focused.objectName !== "connectionSheet")
                return;
            focused = focused.parent;
        }
        if (connection && connection.nick.length === 0)
            connectionNickField.focusInput();
        else
            connectionHostField.focusInput();
    }

    // Enter walks the form the way a form should; the window-level Ctrl+Enter
    // shortcut is the commit. Plain Enter applying from every field dismissed
    // the sheet mid-edit.
    function applyFromSheetKey(event) {
        if (event.key !== Qt.Key_Return && event.key !== Qt.Key_Enter)
            return;
        if (event.modifiers & Qt.ControlModifier)
            return;
        advanceConnectionFocus();
        event.accepted = true;
    }

    function advanceConnectionFocus() {
        var item = win.activeFocusItem;
        if (!item)
            return;
        var next = item.nextItemInFocusChain(true);
        if (next && next !== item)
            next.forceActiveFocus();
    }

    // The rail keeps its own arrow-key navigation so a long network list does
    // not have to be walked with Tab.
    readonly property int networkChoiceStopCount: networkChoiceRepeater
        ? networkChoiceRepeater.count + (connectionAddNetwork.visible ? 1 : 0) : 0

    function focusNetworkChoiceStop(stop) {
        var total = win.networkChoiceStopCount;
        if (total <= 0)
            return;
        var clamped = Math.max(0, Math.min(stop, total - 1));
        var target = clamped < networkChoiceRepeater.count
            ? networkChoiceRepeater.itemAt(clamped)
            : connectionAddNetwork;
        if (!target)
            return;
        target.forceActiveFocus();
    }

    // Focus does not scroll a Flickable by itself, so a focused row could sit
    // entirely outside the viewport.
    function revealInScroll(flick, item) {
        if (!flick || !item)
            return;
        var margin = win.scaledSize(6);
        var top = item.y - margin;
        var bottom = item.y + item.height + margin;
        var target = flick.contentY;
        if (top < flick.contentY)
            target = top;
        else if (bottom > flick.contentY + flick.height)
            target = bottom - flick.height;
        var lowest = Math.max(0, flick.contentHeight - flick.height);
        flick.contentY = Math.max(0, Math.min(target, lowest));
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
        function onFocusNickServChanged() {
            if (win.connection && win.connection.focusNickServ) {
                win.connectionSheetOpen = true;
                connectionNickServ.focusInput();
            }
        }
    }

    Connections {
        target: backend
        ignoreUnknownSignals: true
        function onNotificationActivated(networkId, target, msgid) {
            win.activateNotifiedConversation(networkId, target, msgid);
        }
    }

    Connections {
        target: win.irc
        ignoreUnknownSignals: true
        function onMentionArrived(author, body, networkId, target, msgid) {
            win.notifyMentionIfUnfocused(win.active, author, body, networkId, target, msgid);
        }
    }

    component NetworkSection: Column {
        id: section

        property string networkId
        property string displayName
        property int iconColor: -1
        property string statusText: ""
        property int alerts: 0
        property int unread: 0
        property bool mention: false
        property bool showEdit: win.connection !== null
        property alias headerItem: networkHeader
        readonly property bool headerFocused: section.networkId.length > 0
            && section.networkId === win.sidebarNetworkFocusId

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
            objectName: "networkHeader-" + section.networkId
            width: parent.width
            height: win.scaledSize(64)

            Rectangle {
                anchors.fill: parent
                anchors.leftMargin: win.scaledSize(8)
                anchors.rightMargin: win.scaledSize(8)
                radius: win.scaledSize(7)
                color: section.headerFocused
                    ? win.raisedColor
                    : networkHeaderButton.containsMouse ? win.hoverColor : "transparent"
            }

            Rectangle {
                visible: section.headerFocused
                anchors.left: parent.left
                anchors.leftMargin: win.scaledSize(8)
                anchors.verticalCenter: parent.verticalCenter
                width: win.scaledSize(3)
                height: win.scaledSize(18)
                radius: width
                color: win.accentColor
            }

            MouseArea {
                id: networkHeaderButton
                objectName: "networkHeaderButton-" + section.networkId
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
                objectName: "networkIcon-" + section.networkId
                anchors.left: parent.left
                anchors.leftMargin: win.scaledSize(18)
                anchors.verticalCenter: parent.verticalCenter
                width: win.scaledSize(28)
                height: width
                radius: win.scaledSize(8)
                color: section.iconColor >= 0 ? win.paletteColor(section.iconColor)
                                              : win.accentColor

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
                objectName: "networkEditButton-" + section.networkId
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
                        objectName: "networkUnreadMark-" + section.networkId
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
                            : (section.liveStatus === "Connected"
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
        property bool muted: false
        property bool direct: false
        property bool typing: false
        // "online" / "away" / "offline" for a direct message, empty for a
        // channel. Direct rows reuse the member list's presence source.
        property string presence: ""
        property string networkId: ""
        property string conversationId: networkId + "\n" + conversationName

        objectName: networkId.length > 0
            ? "conversation-" + networkId + "-" + conversationName
            : "conversation-" + conversationName
        Accessible.name: conversationName
        Accessible.description: typing ? "Typing" : ""
        Accessible.role: Accessible.Button
        Accessible.onPressAction: activate()
        width: parent ? parent.width : 0
        height: win.scaledSize(36)

        readonly property bool current: conversationId === win.currentConversationId
            && !win.consoleVisible

        function activate() {
            win.selectConversation(conversationRow.conversationName,
                                   conversationRow.networkId);
            Qt.callLater(function() {
                if (win)
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
                objectName: "conversation-presence-" + conversationRow.networkId
                    + "-" + conversationRow.conversationName
                // Other people's away state needs away-notify, exactly like the
                // member rows. An unknown peer reads as offline, not online.
                visible: conversationRow.direct && win.awayPresenceVisible
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                width: win.scaledSize(7)
                height: width
                radius: width / 2
                color: conversationRow.presence === "away" ? "#d6a552"
                    : (conversationRow.presence === "online"
                        ? "#69b978" : win.mutedColor)
                border.width: win.scaledSize(2)
                border.color: win.panelColor
            }
        }

        Text {
            objectName: "conversationLabel"
            anchors.left: parent.left
            anchors.leftMargin: conversationRow.direct ? win.scaledSize(49) : win.scaledSize(20)
            anchors.right: rowTrail.left
            anchors.rightMargin: win.scaledSize(8)
            anchors.verticalCenter: parent.verticalCenter
            text: (conversationRow.direct ? "" : "#  ") + conversationRow.conversationName.replace("#", "")
            color: conversationRow.current
                ? win.inkColor
                : conversationRow.muted || conversationRow.unread === 0
                    ? win.mutedColor
                    : win.inkColor
            elide: Text.ElideRight
            font.family: "iA Writer Mono S"
            font.bold: conversationRow.current
                       || (!conversationRow.muted && conversationRow.unread > 0)
            font.pixelSize: win.scaledSize(13)
        }

        Item {
            id: rowTrail
            anchors.right: parent.right
            anchors.rightMargin: win.scaledSize(17)
            anchors.verticalCenter: parent.verticalCenter
            height: win.scaledSize(19)
            width: {
                var badge = unreadBadge.visible ? unreadBadge.width : 0;
                var dots = rowTyping.visible ? rowTyping.implicitWidth : 0;
                var gap = (badge > 0 && dots > 0) ? win.scaledSize(6) : 0;
                return badge + dots + gap;
            }

            TypingDots {
                id: rowTyping
                objectName: conversationRow.networkId.length > 0
                    ? "conversation-typing-" + conversationRow.networkId
                        + "-" + conversationRow.conversationName
                    : "conversation-typing-" + conversationRow.conversationName
                visible: conversationRow.visible
                    && conversationRow.direct
                    && conversationRow.typing
                anchors.right: unreadBadge.visible ? unreadBadge.left : parent.right
                anchors.rightMargin: unreadBadge.visible ? win.scaledSize(6) : 0
                anchors.verticalCenter: parent.verticalCenter
            }

            Rectangle {
                id: unreadBadge
                visible: conversationRow.unread > 0
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                width: Math.max(win.scaledSize(19), badgeText.implicitWidth + win.scaledSize(10))
                height: win.scaledSize(19)
                radius: height / 2
                color: conversationRow.mention && !conversationRow.muted
                    ? win.accentColor : win.raisedColor

                Text {
                    id: badgeText
                    anchors.centerIn: parent
                    text: conversationRow.unread
                    color: conversationRow.mention && !conversationRow.muted
                        ? "#ffffff" : win.inkColor
                    font.family: "iA Writer Mono S"
                    font.bold: true
                    font.pixelSize: win.scaledSize(10)
                }
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
        // Bumped on count change, model reset, and dataChanged that covers
        // the last row. Bindings that read a row through field() have no
        // NOTIFY: the model object is stable, and a same-size rewrite leaves
        // count unchanged.
        property int rowRevision: 0
        property int restoreOffset: -1
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
                Qt.callLater(function() {
                    if (list.stick === list.stickFollowing)
                        list.stickToEnd();
                });
                return;
            }
            if (firstUnseenIndex < 0)
                firstUnseenIndex = previousCount;
            trackedCount = newCount;
        }

        function noteSplice(previousCount, newCount) {
            trackedCount = newCount;
            if (newCount <= 0) {
                pinToEnd();
                return;
            }
            if (stick === stickFollowing) {
                stickToEnd();
                return;
            }
            // Replay rows land above the reader, so nothing new arrived at the
            // bottom. Carry an armed marker along with its row rather than
            // arming a fresh one over backfilled history.
            if (firstUnseenIndex >= 0) {
                firstUnseenIndex += newCount - previousCount;
                if (firstUnseenIndex < 0 || firstUnseenIndex >= newCount)
                    firstUnseenIndex = -1;
            }
        }

        function modelRowCount() {
            if (model && typeof model.rowCount === "function")
                return model.rowCount();
            return count;
        }

        function snapshotAnchor() {
            var index = indexAt(Math.max(1, width / 2), contentY + 1);
            if (index < 0)
                index = indexAt(Math.max(1, width / 2), contentY + 8);
            if (index < 0)
                index = 0;
            // A history splice inserts replay rows above the reader and may trim
            // the front, so a raw index names a different message afterwards.
            // Distance from the last row survives both.
            restoreOffset = count - index;
        }

        function restoreAnchor() {
            var offset = restoreOffset;
            restoreOffset = -1;
            if (stick === stickFollowing) {
                pinToEnd();
                return;
            }
            pinning = true;
            var generation = ++pinGeneration;
            var total = modelRowCount();
            var target = total - offset;
            if (offset >= 0 && target >= 0 && target < total)
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
            rowRevision += 1;
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
                list.rowRevision += 1;
                // ListView.count is still the pre-reset value here. The C++
                // model already has the spliced rows, so growth after this
                // handler would look like a bottom append.
                var previous = list.resetSavedCount;
                var newCount = list.modelRowCount();
                list.noteSplice(previous, newCount);
                Qt.callLater(function() {
                    list.resetPending = false;
                    list.restoreAnchor();
                });
            }
            function onRowsInserted(parent, first, last) {
                list.noteGrowth(list.trackedCount, list.count);
            }
            function onDataChanged(topLeft, bottomRight) {
                // The typing footer reads the last row through
                // transcriptField / field(), a Q_INVOKABLE with no NOTIFY.
                // Same-size reload and ListModel setProperty emit
                // dataChanged without changing count, so this bump
                // re-reads grouping.
                if (bottomRight.row >= list.count - 1)
                    list.rowRevision += 1;
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

        ScrollBar.vertical: ScrollBar {
            policy: list.contentHeight > list.height
                ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
            onPressedChanged: {
                if (!pressed)
                    list.adoptViewport();
            }
        }
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
                        objectName: "liveNetworkRepeater"
                        model: win.irc && win.connection ? win.connection.networks : null

                        Column {
                            id: liveNet
                            required property int index
                            required property string networkId
                            required property string displayName
                            required property int iconColor
                            width: parent ? parent.width : 0
                            spacing: 0

                            NetworkSection {
                                networkId: liveNet.networkId
                                displayName: liveNet.displayName
                                iconColor: liveNet.iconColor
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
                                    muted: model.muted
                                    direct: model.direct
                                    typing: model.typing
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
                                    muted: model.muted
                                    direct: model.direct
                                    typing: model.typing
                                    presence: model.presence || ""
                                    networkId: model.networkId
                                    visible: model.direct
                                        && model.networkId === liveNet.networkId
                                    width: sidebar.width
                                    height: visible ? win.scaledSize(36) : 0
                                }
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
                    id: identityText
                    anchors.left: parent.left
                    anchors.leftMargin: win.scaledSize(63)
                    anchors.right: selfVersionLabel.visible ? selfVersionLabel.left : parent.right
                    anchors.rightMargin: win.scaledSize(selfVersionLabel.visible ? 8 : 17)
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: win.scaledSize(1)

                    Text {
                        objectName: "selfNickLabel"
                        width: parent.width
                        text: win.selfNick
                        color: win.inkColor
                        elide: Text.ElideRight
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

                Text {
                    id: selfVersionLabel
                    objectName: "selfVersionLabel"
                    anchors.right: parent.right
                    anchors.rightMargin: win.scaledSize(17)
                    anchors.bottom: identityText.bottom
                    text: win.appVersion
                    visible: text.length > 0
                    color: win.mutedColor
                    font.family: "iA Writer Mono S"
                    font.pixelSize: win.scaledSize(10)
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
                        textFormat: Text.PlainText
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
                            : ""
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

                delegate: Item {
                    id: messageDelegate

                    required property int index
                    required property string author
                    required property string time
                    required property string body
                    required property string kind
                    readonly property string origin: win.transcriptField(messageList.model, index, "origin")
                    readonly property bool replayed: origin === "replay"
                    readonly property bool isChat: kind !== "event" && kind !== "whois"
                    readonly property bool grouped: win.continuesMessageGroup(
                        messageList.model, index, author, time, kind, origin)
                    readonly property bool findMatch: win.findActive
                        && win.findIndex === index

                    width: messageList.width
                    height: win.transcriptRowHeight(
                        kind === "event", grouped || kind === "whois",
                        kind === "event"
                            ? messageEvent.implicitHeight
                            : (kind === "whois"
                                ? messageWhois.implicitHeight
                                : messageBody.implicitHeight))

                    Rectangle {
                        objectName: "findMatch"
                        anchors.fill: parent
                        visible: messageDelegate.findMatch
                        color: win.mixColors(win.pageColor, win.selectionColor, 0.42)
                    }

                    Text {
                        id: messageEvent
                        objectName: "messageEvent"
                        visible: messageDelegate.kind === "event"
                        x: win.scaledSize(24)
                        y: Math.round((parent.height - implicitHeight) / 2)
                        width: parent.width - win.scaledSize(48)
                        horizontalAlignment: Text.AlignHCenter
                        text: win.plainIrcText(messageDelegate.body)
                        textFormat: Text.PlainText
                        color: win.mutedColor
                        wrapMode: Text.Wrap
                        font.family: "iA Writer Mono S"
                        font.pixelSize: win.scaledSize(10)
                    }

                    Text {
                        id: messageWhois
                        objectName: "messageWhois"
                        visible: messageDelegate.kind === "whois"
                        anchors.left: parent.left
                        anchors.leftMargin: win.scaledSize(70)
                        anchors.right: parent.right
                        anchors.rightMargin: win.scaledSize(34)
                        anchors.top: parent.top
                        anchors.topMargin: win.transcriptField(
                            messageList.model, messageDelegate.index - 1, "kind") === "whois"
                            ? win.scaledSize(4)
                            : win.scaledSize(8)
                        horizontalAlignment: Text.AlignLeft
                        text: win.plainIrcText(messageDelegate.body)
                        textFormat: Text.PlainText
                        color: win.mutedColor
                        wrapMode: Text.Wrap
                        font.family: "iA Writer Mono S"
                        font.pixelSize: win.scaledSize(12)
                    }

                    MessageAvatar {
                        objectName: "messageAvatar"
                        visible: messageDelegate.isChat && !messageDelegate.grouped
                        author: messageDelegate.author
                        replayed: messageDelegate.replayed
                        nickOpensDirect: true
                    }

                    MessageHeader {
                        objectName: "messageHeader"
                        visible: messageDelegate.isChat && !messageDelegate.grouped
                        author: messageDelegate.author
                        time: messageDelegate.time
                        replayed: messageDelegate.replayed
                        nickOpensDirect: true
                    }

                    TextEdit {
                        id: messageBody
                        objectName: "messageBody"
                        visible: messageDelegate.isChat
                        anchors.left: parent.left
                        anchors.leftMargin: win.scaledSize(70)
                        anchors.right: parent.right
                        anchors.rightMargin: win.scaledSize(34)
                        anchors.top: parent.top
                        anchors.topMargin: win.bodyTextTopMargin(messageDelegate.grouped)
                        text: win.hasIrcEmphasis(messageDelegate.body)
                            ? win.emphasizedIrcText(messageDelegate.body)
                            : win.plainIrcText(messageDelegate.body)
                        color: (messageDelegate.replayed || messageDelegate.kind === "action")
                            ? win.mutedColor : win.inkColor
                        selectionColor: win.selectionColor
                        selectedTextColor: "#ffffff"
                        wrapMode: TextEdit.Wrap
                        readOnly: true
                        selectByMouse: true
                        cursorVisible: false
                        activeFocusOnPress: false
                        activeFocusOnTab: false
                        textFormat: win.hasIrcEmphasis(messageDelegate.body)
                            ? TextEdit.RichText
                            : TextEdit.PlainText
                        padding: 0
                        font.family: win.transcriptBodyFont.family
                        font.italic: messageDelegate.kind === "action"
                        font.pixelSize: win.transcriptBodyFont.pixelSize

                        PlainUrlHit { edit: messageBody }
                    }
                }

                footer: Item {
                    id: typingRow
                    objectName: "typingTranscript"

                    readonly property var transcriptModel: messageList.model
                    readonly property string nick: win.typingNicks.length > 0
                        ? String(win.typingNicks[0]) : ""
                    readonly property bool show: win.typingVisible
                        && !win.currentConversationIsChannel
                        && !win.consoleVisible
                        && typingRow.nick.length > 0
                    property int minuteTick: 0
                    readonly property string currentMinute: {
                        typingRow.show;
                        typingRow.minuteTick;
                        return win.currentTranscriptMinute();
                    }
                    readonly property bool grouped: {
                        // The revision read re-reads the last row after a model
                        // reset that leaves the row count unchanged.
                        messageList.rowRevision;
                        return win.typingFollowsPeerChat(
                            typingRow.transcriptModel, typingRow.nick,
                            typingRow.currentMinute);
                    }

                    Timer {
                        // Grouping treats the footer as the next live chat row
                        // arriving now. Snapshotting new Date() only when
                        // rowRevision changes would stay grouped after the
                        // minute rolls, then jump when the message arrives.
                        // Tick while the indicator is shown so a minute
                        // boundary ungroups the placeholder the same way the
                        // arriving row would.
                        interval: 1000
                        running: typingRow.show
                        repeat: true
                        onTriggered: typingRow.minuteTick += 1
                    }

                    width: messageList.width
                    visible: typingRow.show
                    // A hidden footer with a real height reserves blank space at
                    // the content bottom and stickToEnd scrolls into it.
                    height: typingRow.show
                        ? win.transcriptRowHeight(false, typingRow.grouped,
                                                  win.messageLineHeight)
                        : 0

                    MessageAvatar {
                        objectName: "typingTranscriptAvatar"
                        visible: typingRow.show && !typingRow.grouped
                        author: typingRow.nick
                        replayed: false
                    }

                    MessageHeader {
                        objectName: "typingTranscriptHeader"
                        visible: typingRow.show && !typingRow.grouped
                        author: typingRow.nick
                        time: ""
                        replayed: false
                    }

                    TypingDots {
                        id: typingRowDots
                        objectName: "typingTranscriptDots"
                        visible: typingRow.show
                        describedAs: typingRow.show
                            ? typingRow.nick + " is typing" : ""
                        anchors.left: parent.left
                        anchors.leftMargin: win.scaledSize(70)
                        anchors.top: parent.top
                        anchors.topMargin: win.bodyTextTopMargin(typingRow.grouped)
                            + Math.round((win.messageLineHeight
                                - typingRowDots.implicitHeight) / 2)
                        pixelSize: win.scaledSize(16)
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
                model: win.networkConsole ? win.networkConsole.lines : null

                delegate: Item {
                    id: consoleDelegate

                    required property int index
                    required property string time
                    required property string label
                    required property string text
                    required property string source
                    required property string severity
                    readonly property bool findMatch: win.findActive
                        && win.findIndex === index

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

                    Rectangle {
                        objectName: "findMatch"
                        anchors.fill: parent
                        visible: consoleDelegate.findMatch
                        color: win.mixColors(win.pageColor, win.selectionColor, 0.42)
                    }

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

                        PlainUrlHit {
                            edit: consoleText
                            inviteHits: consoleDelegate.label === "INVITE"
                        }
                    }
                }
            }

            UnseenJumpButton {
                list: consoleList
                objectName: "consoleUnseenJump"
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
                    Accessible.name: win.findActive ? "Find" : "Message composer"
                    Accessible.description: win.findActive
                        ? "Find in the current transcript"
                        : (win.consoleVisible
                            ? "Command for " + win.statusTitleText().replace(" Status", "")
                            : "Write a message to " + win.currentConversation)
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
                    placeholderText: win.findActive ? "Find" : ""
                    placeholderTextColor: win.mutedColor
                    enabled: !win.connectionOverlayVisible
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
                            if (!win.findActive && win.sidebarNetworkFocusId.length > 0) {
                                win.activateFocusedNetworkHeader();
                                event.accepted = true;
                                return;
                            }
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
                onVisibleChanged: {
                    if (!visible) {
                        // Closing used to drop focus on the window itself, so
                        // typing did nothing until the composer was clicked.
                        Qt.callLater(function() {
                            if (win.active)
                                composer.forceActiveFocus();
                        });
                        return;
                    }
                    win.connectionSheetTab = "connection";
                    Qt.callLater(win.focusConnectionSheetStart);
                }

                MouseArea {
                    anchors.fill: parent
                    onClicked: {
                        if (win.connection && !win.connection.setupRequired)
                            win.connectionSheetOpen = false;
                    }
                }

                Rectangle {
                    id: connectionSheetCard
                    objectName: "connectionSheetCard"
                    anchors.centerIn: parent
                    width: Math.min(win.scaledSize(860), parent.width - win.scaledSize(40))
                    height: Math.min(win.scaledSize(720), parent.height - win.scaledSize(40))
                    radius: win.scaledSize(10)
                    color: win.raisedColor
                    border.width: 1
                    border.color: win.dividerColor

                    MouseArea {
                        id: sheetCardClickSink
                        anchors.fill: parent
                        onClicked: function(mouse) { mouse.accepted = true; }
                    }

                    ColumnLayout {
                        anchors.fill: parent
                        spacing: 0

                        Item {
                            id: connectionTabsHeader
                            objectName: "connectionTabsHeader"
                            Layout.fillWidth: true
                            Layout.preferredHeight: connectionTabConnection.height

                            Row {
                                id: connectionTabs
                                objectName: "connectionTabs"
                                anchors.left: parent.left
                                anchors.leftMargin: win.scaledSize(10)
                                anchors.bottom: parent.bottom
                                spacing: win.scaledSize(2)

                                ConnectionTabButton {
                                    id: connectionTabConnection
                                    tabName: "connection"
                                    label: "Connection"
                                    glyph: "#"
                                    onStepRequested: function(direction) {
                                        win.stepConnectionSheetTab(direction);
                                    }
                                }

                                ConnectionTabButton {
                                    id: connectionTabPreferences
                                    tabName: "preferences"
                                    label: "Preferences"
                                    onStepRequested: function(direction) {
                                        win.stepConnectionSheetTab(direction);
                                    }
                                }
                            }
                        }

                        Rectangle {
                            Layout.fillWidth: true
                            Layout.preferredHeight: 1
                            color: win.dividerColor
                        }

                        ColumnLayout {
                            id: connectionTab
                            objectName: "connectionTab"
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            visible: win.connectionSheetTab === "connection"
                            enabled: visible
                            spacing: 0

                            RowLayout {
                                id: connectionBody
                                Layout.fillWidth: true
                                Layout.fillHeight: true
                                Layout.leftMargin: win.scaledSize(18)
                                Layout.rightMargin: win.scaledSize(18)
                                Layout.topMargin: win.scaledSize(14)
                                Layout.bottomMargin: win.scaledSize(12)
                                spacing: win.scaledSize(16)

                                ColumnLayout {
                                    id: networkRail
                                    objectName: "networkChoiceList"
                                    Layout.fillWidth: false
                                    Layout.preferredWidth: win.scaledSize(170)
                                    Layout.maximumWidth: win.scaledSize(170)
                                    Layout.minimumWidth: win.scaledSize(150)
                                    Layout.fillHeight: true
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
                                        objectName: "networkChoiceScroll"
                                        Layout.fillWidth: true
                                        Layout.fillHeight: true
                                        Layout.minimumHeight: win.scaledSize(32)
                                        clip: true
                                        contentWidth: width
                                        contentHeight: networkChoiceColumn.implicitHeight
                                        boundsBehavior: Flickable.StopAtBounds
                                        ScrollBar.vertical: ScrollBar {
                                            objectName: "networkChoiceScrollBar"
                                            policy: networkChoiceScroll.contentHeight > networkChoiceScroll.height
                                                ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
                                        }

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
                                                    id: networkRow
                                                    required property string networkId
                                                    required property string displayName
                                                    required property bool selected
                                                    required property int index
                                                    property bool applyArmed: false
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
                                                    onActiveFocusChanged: {
                                                        if (activeFocus)
                                                            win.revealInScroll(networkChoiceScroll, networkRow);
                                                        else
                                                            applyArmed = false;
                                                    }
                                                    Keys.onPressed: function(event) {
                                                        if (event.key === Qt.Key_Down) {
                                                            win.focusNetworkChoiceStop(index + 1);
                                                            event.accepted = true;
                                                            return;
                                                        }
                                                        if (event.key === Qt.Key_Up) {
                                                            win.focusNetworkChoiceStop(index - 1);
                                                            event.accepted = true;
                                                            return;
                                                        }
                                                        if (event.key === Qt.Key_Home) {
                                                            win.focusNetworkChoiceStop(0);
                                                            event.accepted = true;
                                                            return;
                                                        }
                                                        if (event.key === Qt.Key_End) {
                                                            win.focusNetworkChoiceStop(
                                                                win.networkChoiceStopCount - 1);
                                                            event.accepted = true;
                                                            return;
                                                        }
                                                        if (event.key === Qt.Key_Space) {
                                                            win.selectSheetNetwork(networkId);
                                                            event.accepted = true;
                                                            return;
                                                        }
                                                        if (event.key !== Qt.Key_Return
                                                                && event.key !== Qt.Key_Enter)
                                                            return;
                                                        // Ctrl+Enter is handled by the
                                                        // window-level apply shortcut.
                                                        if (event.modifiers & Qt.ControlModifier)
                                                            return;
                                                        if (selected && applyArmed)
                                                            win.submitConnection();
                                                        else {
                                                            win.selectSheetNetwork(networkId);
                                                            applyArmed = true;
                                                        }
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

                                            Rectangle {
                                                id: connectionAddNetwork
                                                objectName: "connectionAddNetwork"
                                                width: networkChoiceColumn.width
                                                height: win.scaledSize(28)
                                                radius: win.scaledSize(6)
                                                visible: win.connection ? win.connection.canAdd : false
                                                activeFocusOnTab: visible
                                                Accessible.role: Accessible.Button
                                                Accessible.name: "Add network"
                                                Accessible.onPressAction: win.addSheetNetwork()
                                                color: addNetworkMouse.containsMouse || activeFocus
                                                    ? win.hoverColor : "transparent"
                                                border.width: activeFocus ? 1 : 0
                                                border.color: win.accentColor
                                                onActiveFocusChanged: {
                                                    if (activeFocus)
                                                        win.revealInScroll(networkChoiceScroll,
                                                                           connectionAddNetwork);
                                                }
                                                Keys.onPressed: function(event) {
                                                    if (event.key === Qt.Key_Up) {
                                                        win.focusNetworkChoiceStop(
                                                            win.networkChoiceStopCount - 2);
                                                        event.accepted = true;
                                                        return;
                                                    }
                                                    if (event.key === Qt.Key_Return
                                                            || event.key === Qt.Key_Enter
                                                            || event.key === Qt.Key_Space) {
                                                        win.addSheetNetwork();
                                                        event.accepted = true;
                                                    }
                                                }

                                                Text {
                                                    anchors.centerIn: parent
                                                    text: "+ Add network"
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
                                                        win.addSheetNetwork();
                                                        parent.forceActiveFocus();
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    Rectangle {
                                        id: connectionShortcutsHint
                                        objectName: "connectionShortcutsHint"
                                        Layout.fillWidth: true
                                        Layout.preferredHeight: win.scaledSize(30)
                                        radius: win.scaledSize(7)
                                        color: shortcutsHintMouse.containsMouse
                                            ? win.hoverColor : win.panelColor
                                        border.width: 1
                                        border.color: activeFocus ? win.accentColor : win.dividerColor
                                        activeFocusOnTab: true
                                        Accessible.role: Accessible.Button
                                        Accessible.name: "Keyboard shortcuts"
                                        Accessible.description: "Open the keyboard shortcuts sheet"
                                        Accessible.onPressAction: shortcutsSheet.open()
                                        Keys.onPressed: function(event) {
                                            if (event.key === Qt.Key_Return
                                                    || event.key === Qt.Key_Enter
                                                    || event.key === Qt.Key_Space) {
                                                shortcutsSheet.open();
                                                event.accepted = true;
                                            }
                                        }

                                        Text {
                                            anchors.left: parent.left
                                            anchors.leftMargin: win.scaledSize(9)
                                            anchors.verticalCenter: parent.verticalCenter
                                            text: "Shortcuts"
                                            color: win.mutedColor
                                            font.family: "iA Writer Mono S"
                                            font.pixelSize: win.scaledSize(10)
                                        }

                                        Rectangle {
                                            id: shortcutsKey
                                            anchors.right: parent.right
                                            anchors.rightMargin: win.scaledSize(7)
                                            anchors.verticalCenter: parent.verticalCenter
                                            width: shortcutsKeyLabel.implicitWidth + win.scaledSize(10)
                                            height: win.scaledSize(18)
                                            radius: win.scaledSize(4)
                                            color: win.raisedColor
                                            border.width: 1
                                            border.color: win.dividerColor

                                            Text {
                                                id: shortcutsKeyLabel
                                                anchors.centerIn: parent
                                                text: "Ctrl + /"
                                                color: win.inkColor
                                                font.family: "iA Writer Mono S"
                                                font.pixelSize: win.scaledSize(10)
                                            }
                                        }

                                        MouseArea {
                                            id: shortcutsHintMouse
                                            anchors.fill: parent
                                            hoverEnabled: true
                                            cursorShape: Qt.PointingHandCursor
                                            onClicked: shortcutsSheet.open()
                                        }
                                    }
                                }

                                Item {
                                    id: connectionFormArea
                                    objectName: "connectionFormArea"
                                    Layout.fillWidth: true
                                    Layout.fillHeight: true

                                    Flickable {
                                        id: sheetFlick
                                        objectName: "sheetFlick"
                                        anchors.fill: parent
                                        contentWidth: width
                                        contentHeight: sheetColumn.implicitHeight
                                        clip: true
                                        boundsBehavior: Flickable.StopAtBounds
                                        ScrollBar.vertical: ScrollBar {
                                            id: sheetFlickScrollBar
                                            objectName: "sheetFlickScrollBar"
                                            policy: sheetFlick.contentHeight > sheetFlick.height
                                                ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
                                        }

                                        Column {
                                            id: sheetColumn
                                            width: sheetFlick.width - sheetFlickScrollBar.width
                                            spacing: win.scaledSize(10)

                                            Text {
                                                text: "Connect"
                                                color: win.inkColor
                                                font.family: "iA Writer Mono S"
                                                font.bold: true
                                                font.pixelSize: win.scaledSize(15)
                                            }

                                            Row {
                                                width: parent.width
                                                spacing: win.scaledSize(12)

                                                ConnectionField {
                                                    id: connectionHostField
                                                    width: parent.width - win.scaledSize(104)
                                                        - win.scaledSize(72) - win.scaledSize(24)
                                                    label: "Host"
                                                    fieldObjectName: "connectionHost"
                                                    text: win.connection ? win.connection.host : ""
                                                    onTextEdited: function(value) {
                                                        if (win.connection)
                                                            win.connection.host = value;
                                                    }
                                                }

                                                ConnectionField {
                                                    width: win.scaledSize(104)
                                                    label: "Port"
                                                    fieldObjectName: "connectionPort"
                                                    text: win.connection ? String(win.connection.port) : "6697"
                                                    onTextEdited: function(value) {
                                                        if (win.connection)
                                                            win.connection.port = Number(value) || 0;
                                                    }
                                                }

                                                Item {
                                                    width: win.scaledSize(72)
                                                    height: win.scaledSize(55)

                                                    Column {
                                                        anchors.left: parent.left
                                                        anchors.right: parent.right
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
                                                            Keys.onPressed: function(event) {
                                                                win.applyFromSheetKey(event)
                                                            }
                                                            onToggled: {
                                                                if (win.connection)
                                                                    win.connection.tlsEnabled = checked;
                                                            }
                                                        }
                                                    }
                                                }
                                            }

                                            Row {
                                                width: parent.width
                                                spacing: win.scaledSize(12)

                                                ConnectionField {
                                                    id: connectionNickField
                                                    width: Math.round((parent.width - win.scaledSize(12)) / 2)
                                                    label: "Nick"
                                                    fieldObjectName: "connectionNick"
                                                    text: win.connection ? win.connection.nick : ""
                                                    onTextEdited: function(value) {
                                                        if (win.connection)
                                                            win.connection.nick = value;
                                                    }
                                                }

                                                ConnectionField {
                                                    width: parent.width - connectionNickField.width
                                                        - win.scaledSize(12)
                                                    label: "Username"
                                                    fieldObjectName: "connectionUsername"
                                                    text: win.connection ? win.connection.username : ""
                                                    onTextEdited: function(value) {
                                                        if (win.connection)
                                                            win.connection.username = value;
                                                    }
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
                                                    Keys.onPressed: function(event) {
                                                        win.applyFromSheetKey(event)
                                                    }
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

                                                Item {
                                                    width: parent.width - connectionConnectOnStartup.width
                                                        - win.scaledSize(10)
                                                    height: connectionConnectOnStartup.height

                                                    Text {
                                                        anchors.left: parent.left
                                                        anchors.verticalCenter: parent.verticalCenter
                                                        text: "Connect automatically on startup"
                                                        color: win.inkColor
                                                        font.family: "iA Writer Mono S"
                                                        font.pixelSize: win.scaledSize(11)
                                                    }
                                                }
                                            }

                                            Rectangle {
                                                width: parent.width
                                                height: 1
                                                color: win.dividerColor
                                            }

                                            ConnectionField {
                                                id: connectionPassword
                                                label: "Server password"
                                                badge: "PASS"
                                                help: "Used as the SASL secret when no NickServ password is set; otherwise sent as PASS while connecting."
                                                fieldObjectName: "connectionPassword"
                                                secret: true
                                                onTextEdited: function(value) {
                                                    win.connectionPasswordEdited = true;
                                                }
                                            }

                                            ConnectionField {
                                                id: connectionNickServ
                                                label: "NickServ password"
                                                help: "Preferred SASL secret. Sent as NickServ IDENTIFY when SASL did not succeed."
                                                fieldObjectName: "connectionNickServ"
                                                secret: true
                                                onTextEdited: function(value) {
                                                    win.connectionNickServEdited = true;
                                                }
                                            }

                                            Text {
                                                id: connectionCredentialStatus
                                                objectName: "connectionCredentialStatus"
                                                width: parent.width
                                                visible: !!(win.connection && win.connection.credentialStatus)
                                                text: (win.connection && win.connection.credentialStatus)
                                                      ? win.connection.credentialStatus : ""
                                                color: win.mutedColor
                                                wrapMode: Text.Wrap
                                                font.family: "iA Writer Mono S"
                                                font.pixelSize: win.scaledSize(10)
                                            }

                                            Row {
                                                width: parent.width
                                                spacing: win.scaledSize(16)

                                                Text {
                                                    id: connectionForgetPassword
                                                    objectName: "connectionForgetPassword"
                                                    visible: !!(win.connection && win.connection.canForgetPassword)
                                                    width: visible ? implicitWidth : 0
                                                    text: "forget saved server password"
                                                    color: win.accentColor
                                                    font.family: "iA Writer Mono S"
                                                    font.pixelSize: win.scaledSize(10)
                                                    font.underline: activeFocus
                                                    Accessible.role: Accessible.Button
                                                    Accessible.name: "Forget saved server password"
                                                    Accessible.description: "Remove the saved server password"
                                                    activeFocusOnTab: visible
                                                    Keys.onPressed: function(event) {
                                                        if (event.key === Qt.Key_Return
                                                                || event.key === Qt.Key_Enter
                                                                || event.key === Qt.Key_Space) {
                                                            forgetSavedPassword();
                                                            event.accepted = true;
                                                        }
                                                    }
                                                    Accessible.onPressAction: {
                                                        forgetSavedPassword();
                                                    }
                                                    function forgetSavedPassword() {
                                                        connectionPassword.text = "";
                                                        win.connection.forgetPassword();
                                                        win.connection.removeStoredPassword();
                                                    }
                                                    MouseArea {
                                                        anchors.fill: parent
                                                        hoverEnabled: true
                                                        cursorShape: Qt.PointingHandCursor
                                                        onClicked: {
                                                            parent.forgetSavedPassword();
                                                        }
                                                    }
                                                }

                                                Text {
                                                    id: connectionForgetNickServ
                                                    objectName: "connectionForgetNickServ"
                                                    visible: !!(win.connection && win.connection.canForgetNickServ)
                                                    width: visible ? implicitWidth : 0
                                                    text: "forget saved NickServ password"
                                                    color: win.accentColor
                                                    font.family: "iA Writer Mono S"
                                                    font.pixelSize: win.scaledSize(10)
                                                    font.underline: activeFocus
                                                    Accessible.role: Accessible.Button
                                                    Accessible.name: "Forget saved NickServ password"
                                                    Accessible.description: "Remove the saved NickServ password"
                                                    activeFocusOnTab: visible
                                                    Keys.onPressed: function(event) {
                                                        if (event.key === Qt.Key_Return
                                                                || event.key === Qt.Key_Enter
                                                                || event.key === Qt.Key_Space) {
                                                            forgetSavedNickServ();
                                                            event.accepted = true;
                                                        }
                                                    }
                                                    Accessible.onPressAction: {
                                                        forgetSavedNickServ();
                                                    }
                                                    function forgetSavedNickServ() {
                                                        connectionNickServ.text = "";
                                                        win.connection.forgetNickServ();
                                                        win.connection.removeStoredNickServ();
                                                    }
                                                    MouseArea {
                                                        anchors.fill: parent
                                                        hoverEnabled: true
                                                        cursorShape: Qt.PointingHandCursor
                                                        onClicked: {
                                                            parent.forgetSavedNickServ();
                                                        }
                                                    }
                                                }
                                            }

                                            Text {
                                                id: connectionProblem
                                                objectName: "connectionProblem"
                                                width: parent.width
                                                visible: !!(win.connection && win.connection.problem)
                                                text: (win.connection && win.connection.problem)
                                                      ? win.connection.problem : ""
                                                color: win.accentColor
                                                wrapMode: Text.Wrap
                                                font.family: "iA Writer Mono S"
                                                font.pixelSize: win.scaledSize(11)
                                            }
                                        }
                                    }
                                }
                            }

                            Rectangle {
                                Layout.fillWidth: true
                                Layout.preferredHeight: 1
                                color: win.dividerColor
                            }

                            Item {
                                id: connectionFooter
                                objectName: "connectionFooter"
                                Layout.fillWidth: true
                                Layout.preferredHeight: win.scaledSize(52)

                                Rectangle {
                                    id: connectionFooterDot
                                    anchors.left: parent.left
                                    anchors.leftMargin: win.scaledSize(18)
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: win.scaledSize(6)
                                    height: width
                                    radius: width / 2
                                    color: win.accentColor
                                }

                                Text {
                                    id: connectionFooterName
                                    objectName: "connectionFooterNetwork"
                                    anchors.left: connectionFooterDot.right
                                    anchors.leftMargin: win.scaledSize(8)
                                    anchors.right: sheetActions.left
                                    anchors.rightMargin: win.scaledSize(16)
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: win.connection ? win.connection.displayName : ""
                                    color: win.mutedColor
                                    elide: Text.ElideRight
                                    font.family: "iA Writer Mono S"
                                    font.pixelSize: win.scaledSize(11)
                                }

                                Row {
                                    id: sheetActions
                                    anchors.right: parent.right
                                    anchors.rightMargin: win.scaledSize(18)
                                    anchors.verticalCenter: parent.verticalCenter
                                    spacing: win.scaledSize(8)

                                    Rectangle {
                                        objectName: "connectionRemove"
                                        visible: win.connection ? win.connection.canRemove : false
                                        width: visible ? win.scaledSize(win.connectionRemoveArmed ? 148 : 88) : 0
                                        height: win.scaledSize(30)
                                        radius: win.scaledSize(7)
                                        activeFocusOnTab: visible
                                        Accessible.role: Accessible.Button
                                        Accessible.name: win.connectionRemoveArmed
                                            ? "Confirm remove " + (win.connection ? win.connection.displayName : "")
                                            : "Remove"
                                        Accessible.onPressAction: win.removeSheetNetwork()
                                        color: removeMouse.containsMouse || activeFocus
                                            ? win.hoverColor : "transparent"
                                        border.width: 1
                                        border.color: activeFocus ? win.accentColor : win.dividerColor
                                        Keys.onPressed: function(event) {
                                            if (event.key === Qt.Key_Return
                                                    || event.key === Qt.Key_Enter
                                                    || event.key === Qt.Key_Space) {
                                                win.removeSheetNetwork();
                                                event.accepted = true;
                                            }
                                        }

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
                                                win.removeSheetNetwork();
                                                parent.forceActiveFocus();
                                            }
                                        }
                                    }

                                    Item { width: 1; height: 1 }

                                    Rectangle {
                                        objectName: "connectionDiscard"
                                        width: win.scaledSize(88)
                                        height: win.scaledSize(30)
                                        radius: win.scaledSize(7)
                                        activeFocusOnTab: true
                                        Accessible.role: Accessible.Button
                                        Accessible.name: "Discard"
                                        Accessible.onPressAction: win.discardSheetConnection()
                                        color: discardMouse.containsMouse || activeFocus
                                            ? win.hoverColor : "transparent"
                                        border.width: 1
                                        border.color: activeFocus ? win.accentColor : win.dividerColor
                                        Keys.onPressed: function(event) {
                                            if (event.key === Qt.Key_Return
                                                    || event.key === Qt.Key_Enter
                                                    || event.key === Qt.Key_Space) {
                                                win.discardSheetConnection();
                                                event.accepted = true;
                                            }
                                        }

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
                                                win.discardSheetConnection();
                                                parent.forceActiveFocus();
                                            }
                                        }
                                    }

                                    Rectangle {
                                        objectName: "connectionDisconnect"
                                        visible: win.connection ? win.connection.canDisconnect : false
                                        width: visible ? win.scaledSize(108) : 0
                                        height: win.scaledSize(30)
                                        radius: win.scaledSize(7)
                                        activeFocusOnTab: visible
                                        Accessible.role: Accessible.Button
                                        Accessible.name: "Disconnect"
                                        Accessible.onPressAction: win.disconnectSheetNetwork()
                                        color: disconnectMouse.containsMouse || activeFocus
                                            ? win.hoverColor : "transparent"
                                        border.width: 1
                                        border.color: activeFocus ? win.accentColor : win.dividerColor
                                        Keys.onPressed: function(event) {
                                            if (event.key === Qt.Key_Return
                                                    || event.key === Qt.Key_Enter
                                                    || event.key === Qt.Key_Space) {
                                                win.disconnectSheetNetwork();
                                                event.accepted = true;
                                            }
                                        }

                                        Text {
                                            anchors.centerIn: parent
                                            text: "Disconnect"
                                            color: win.mutedColor
                                            font.family: "iA Writer Mono S"
                                            font.pixelSize: win.scaledSize(11)
                                        }

                                        MouseArea {
                                            id: disconnectMouse
                                            anchors.fill: parent
                                            hoverEnabled: true
                                            cursorShape: Qt.PointingHandCursor
                                            onClicked: {
                                                win.disconnectSheetNetwork();
                                                parent.forceActiveFocus();
                                            }
                                        }
                                    }

                                    Rectangle {
                                        objectName: "connectionApply"
                                        width: win.scaledSize(88)
                                        height: win.scaledSize(30)
                                        radius: win.scaledSize(7)
                                        activeFocusOnTab: true
                                        Accessible.role: Accessible.Button
                                        Accessible.name: "Apply"
                                        Accessible.onPressAction: win.submitConnection()
                                        color: win.connection && win.connection.problem.length === 0
                                            ? win.accentColor : win.raisedColor
                                        border.width: 1
                                        border.color: activeFocus
                                            ? (win.connection && win.connection.problem.length === 0
                                                ? win.inkColor : win.accentColor)
                                            : "transparent"
                                        Keys.onPressed: function(event) {
                                            if (event.key === Qt.Key_Return
                                                    || event.key === Qt.Key_Enter
                                                    || event.key === Qt.Key_Space) {
                                                win.submitConnection();
                                                event.accepted = true;
                                            }
                                        }

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
                                            cursorShape: win.connection && win.connection.problem.length === 0
                                                ? Qt.PointingHandCursor : Qt.ArrowCursor
                                            onClicked: {
                                                parent.forceActiveFocus();
                                                win.submitConnection();
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        Item {
                            id: connectionPreferencesPanel
                            objectName: "connectionPreferencesPanel"
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            visible: win.connectionSheetTab === "preferences"
                            enabled: visible

                            Column {
                                anchors.left: parent.left
                                anchors.top: parent.top
                                anchors.leftMargin: win.scaledSize(18)
                                anchors.topMargin: win.scaledSize(18)
                                spacing: win.scaledSize(8)

                                Text {
                                    text: "Preferences"
                                    color: win.inkColor
                                    font.family: "iA Writer Mono S"
                                    font.bold: true
                                    font.pixelSize: win.scaledSize(15)
                                }

                                Row {
                                    spacing: win.scaledSize(10)

                                    Switch {
                                        id: connectionReopenDirects
                                        objectName: "connectionReopenDirects"
                                        Accessible.name: "Reopen direct messages on startup"
                                        Keys.onPressed: function(event) {
                                            win.applyFromSheetKey(event)
                                        }
                                        onToggled: {
                                            if (win.irc)
                                                win.irc.reopenDirectMessages = checked;
                                        }
                                    }

                                    Binding {
                                        target: connectionReopenDirects
                                        property: "checked"
                                        value: win.irc ? win.irc.reopenDirectMessages : true
                                        restoreMode: Binding.RestoreBinding
                                    }

                                    Text {
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: "Reopen direct messages on startup"
                                        color: win.inkColor
                                        font.family: "iA Writer Mono S"
                                        font.pixelSize: win.scaledSize(11)
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
                objectName: "membersHeading"
                anchors.top: parent.top
                anchors.topMargin: win.scaledSize(23)
                anchors.left: parent.left
                anchors.leftMargin: win.scaledSize(20)
                text: "ONLINE - " + win.currentPeopleCount
                color: membersHeadingHit.containsMouse ? win.inkColor : win.mutedColor
                font.family: "iA Writer Mono S"
                font.bold: true
                font.letterSpacing: win.scaledSize(0.7)
                font.pixelSize: win.scaledSize(9)
            }

            MouseArea {
                id: membersHeadingHit
                objectName: "membersHeadingButton"
                anchors.fill: membersHeading
                anchors.margins: -win.scaledSize(8)
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                Accessible.role: Accessible.Button
                Accessible.name: "Jump to nick"
                Accessible.onPressAction: win.openNickSheet()
                onClicked: win.openNickSheet()
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
                model: win.irc ? win.irc.members : null
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
                        : ({nick: "", label: "", status: "", away: false})
                    readonly property string nick: memberData.nick
                    readonly property string label: memberData.label
                    readonly property string status: memberData.status
                    // Exact match, like `openable`; the reducer's overlay is the CASEMAPPING-aware path.
                    readonly property bool isSelf: nick.length > 0 && nick === win.selfNick
                    // Other members' away state needs away-notify, but our own
                    // arrives as the 305/306 numerics, so it stays visible and
                    // agrees with the identity footer.
                    readonly property bool away: memberData.away
                        && (win.awayPresenceVisible || isSelf)
                    readonly property bool typing: win.typingVisible
                        && (win.irc
                            ? win.irc.nickIsTyping(memberDelegate.nick)
                            : win.typingNicks.indexOf(memberDelegate.nick) !== -1)

                    objectName: "member-" + nick
                    Accessible.name: label
                    Accessible.description: win.memberStatusVisible ? status : ""
                    Accessible.role: Accessible.Button
                    Accessible.onPressAction: {
                        if (win.canOpenDirectMessage(nick))
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
                            visible: win.awayPresenceVisible || memberDelegate.isSelf
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
                        enabled: memberDelegate.nick.length > 0
                            && memberDelegate.nick !== win.selfNick
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
                    { keys: "Alt+Left / Alt+Right", action: "walk networks" },
                    { keys: "Ctrl+K", action: "jump to conversation" },
                    { keys: "Ctrl+Shift+K", action: "jump to nick" },
                    { keys: "Alt+A", action: "next unread" },
                    { keys: "Ctrl+`", action: "Status" },
                    { keys: "Ctrl+,", action: "Connect" },
                    { keys: "Ctrl+Enter", action: "apply connection" },
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
                    { keys: "/disconnect", action: "disconnect network" },
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

    ListModel {
        id: jumpModel
        objectName: "jumpModel"
    }

    ListModel {
        id: nickModel
        objectName: "nickModel"
    }

    Popup {
        id: jumpSheet
        objectName: "jumpSheet"
        x: Math.round((win.width - width) / 2)
        y: Math.round((win.height - height) / 2)
        width: win.scaledSize(348)
        padding: win.scaledSize(16)
        modal: true
        focus: true
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        onOpened: {
            win.pickerEscapeGuard = true;
            win.jumpSelectedIndex = 0;
            if (jumpFilter.text.length > 0)
                jumpFilter.clear();
            else
                win.refreshJumpMatches();
            jumpFilter.forceActiveFocus();
        }
        onClosed: {
            Qt.callLater(function() {
                win.pickerEscapeGuard = false;
                composer.forceActiveFocus();
            });
        }

        background: Rectangle {
            color: win.raisedColor
            border.width: 1
            border.color: win.dividerColor
            radius: win.scaledSize(9)
        }

        contentItem: Column {
            spacing: win.scaledSize(8)
            width: jumpSheet.availableWidth

            Rectangle {
                width: parent.width
                height: win.scaledSize(32)
                color: win.panelColor
                border.width: 1
                border.color: jumpFilter.activeFocus ? win.accentColor : win.dividerColor
                radius: win.scaledSize(7)

                TextField {
                    id: jumpFilter
                    objectName: "jumpFilter"
                    Accessible.name: "Jump to conversation"
                    anchors.fill: parent
                    z: 1
                    color: win.inkColor
                    selectionColor: win.selectionColor
                    selectedTextColor: "#ffffff"
                    font.family: "iA Writer Mono S"
                    font.pixelSize: win.scaledSize(13)
                    placeholderText: ""
                    verticalAlignment: TextInput.AlignVCenter
                    leftPadding: win.scaledSize(8)
                    rightPadding: win.scaledSize(8)
                    background: Item {}
                    Keys.onPressed: function(event) {
                        if (event.key === Qt.Key_Down) {
                            win.stepJump(1);
                            event.accepted = true;
                            return;
                        }
                        if (event.key === Qt.Key_Up) {
                            win.stepJump(-1);
                            event.accepted = true;
                            return;
                        }
                        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                            win.activateJumpSelection();
                            event.accepted = true;
                            return;
                        }
                        if (event.key === Qt.Key_Tab) {
                            event.accepted = true;
                        }
                    }
                    onTextChanged: {
                        win.jumpSelectedIndex = 0;
                        win.refreshJumpMatches();
                    }
                }

                Text {
                    objectName: "jumpFilterPlaceholder"
                    z: 0
                    anchors.left: parent.left
                    anchors.leftMargin: win.scaledSize(8)
                    anchors.verticalCenter: parent.verticalCenter
                    text: "Jump to conversation…"
                    color: win.mutedColor
                    font.family: "iA Writer Mono S"
                    font.pixelSize: win.scaledSize(13)
                    visible: jumpFilter.text.length === 0
                }
            }

            ListView {
                id: jumpList
                objectName: "jumpList"
                width: parent.width
                height: Math.min(contentHeight, win.scaledSize(252))
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                model: jumpModel
                currentIndex: win.jumpSelectedIndex
                highlightFollowsCurrentItem: true

                delegate: Item {
                    id: jumpDelegate
                    required property int index
                    required property string label
                    required property string name
                    required property string kind
                    required property string networkId
                    width: jumpList.width
                    height: win.scaledSize(28)
                    readonly property bool current: index === win.jumpSelectedIndex

                    Rectangle {
                        anchors.fill: parent
                        radius: win.scaledSize(7)
                        color: jumpDelegate.current ? win.hoverColor : "transparent"
                    }

                    Text {
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.leftMargin: win.scaledSize(8)
                        anchors.rightMargin: win.scaledSize(8)
                        anchors.verticalCenter: parent.verticalCenter
                        text: jumpDelegate.label
                        color: jumpDelegate.current ? win.inkColor : win.mutedColor
                        elide: Text.ElideRight
                        font.family: "iA Writer Mono S"
                        font.pixelSize: win.scaledSize(12)
                    }
                }
            }
        }
    }

    Popup {
        id: nickSheet
        objectName: "nickSheet"
        x: Math.round((win.width - width) / 2)
        y: Math.round((win.height - height) / 2)
        width: win.scaledSize(348)
        padding: win.scaledSize(16)
        modal: true
        focus: true
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        onOpened: {
            win.pickerEscapeGuard = true;
            if (nickFilter.text.length > 0)
                nickFilter.clear();
            else
                win.refreshNickMatches();
            win.nickSelectedIndex = win.firstOpenableNickIndex();
            nickFilter.forceActiveFocus();
        }
        onClosed: {
            Qt.callLater(function() {
                win.pickerEscapeGuard = false;
                composer.forceActiveFocus();
            });
        }

        background: Rectangle {
            color: win.raisedColor
            border.width: 1
            border.color: win.dividerColor
            radius: win.scaledSize(9)
        }

        contentItem: Column {
            spacing: win.scaledSize(8)
            width: nickSheet.availableWidth

            Rectangle {
                width: parent.width
                height: win.scaledSize(32)
                color: win.panelColor
                border.width: 1
                border.color: nickFilter.activeFocus ? win.accentColor : win.dividerColor
                radius: win.scaledSize(7)

                TextField {
                    id: nickFilter
                    objectName: "nickFilter"
                    Accessible.name: "Jump to nick"
                    anchors.fill: parent
                    z: 1
                    color: win.inkColor
                    selectionColor: win.selectionColor
                    selectedTextColor: "#ffffff"
                    font.family: "iA Writer Mono S"
                    font.pixelSize: win.scaledSize(13)
                    placeholderText: ""
                    verticalAlignment: TextInput.AlignVCenter
                    leftPadding: win.scaledSize(8)
                    rightPadding: win.scaledSize(8)
                    background: Item {}
                    Keys.onPressed: function(event) {
                        if (event.key === Qt.Key_Down) {
                            win.stepNick(1);
                            event.accepted = true;
                            return;
                        }
                        if (event.key === Qt.Key_Up) {
                            win.stepNick(-1);
                            event.accepted = true;
                            return;
                        }
                        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                            win.activateNickSelection();
                            event.accepted = true;
                            return;
                        }
                        if (event.key === Qt.Key_Tab) {
                            event.accepted = true;
                        }
                    }
                    onTextChanged: {
                        win.refreshNickMatches();
                        win.nickSelectedIndex = win.firstOpenableNickIndex();
                    }
                }

                Text {
                    objectName: "nickFilterPlaceholder"
                    z: 0
                    anchors.left: parent.left
                    anchors.leftMargin: win.scaledSize(8)
                    anchors.verticalCenter: parent.verticalCenter
                    text: "Jump to nick…"
                    color: win.mutedColor
                    font.family: "iA Writer Mono S"
                    font.pixelSize: win.scaledSize(13)
                    visible: nickFilter.text.length === 0
                }
            }

            ListView {
                id: nickList
                objectName: "nickList"
                width: parent.width
                height: Math.min(contentHeight, win.scaledSize(252))
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                model: nickModel
                currentIndex: win.nickSelectedIndex
                highlightFollowsCurrentItem: true

                delegate: Item {
                    id: nickRow
                    required property int index
                    required property string name
                    required property string label
                    required property string memberStatus
                    required property int awayFlag
                    required property string glyph
                    required property int paletteIndex

                    readonly property string nick: name
                    readonly property bool away: awayFlag === 1
                    // Exact match, like `openable`; the reducer's overlay is the CASEMAPPING-aware path.
                    readonly property bool isSelf: nick.length > 0 && nick === win.selfNick
                    readonly property bool showAway: away
                        && (win.awayPresenceVisible || isSelf)
                    readonly property bool selected: index === win.nickSelectedIndex
                    readonly property bool openable: name !== win.selfNick
                    readonly property color nickTint: win.nickPalette[paletteIndex]
                    readonly property color avatarFill: win.nickAvatarFills[paletteIndex]

                    objectName: "nickPick-" + name
                    Accessible.name: label
                    Accessible.description: win.memberStatusVisible ? memberStatus : ""
                    Accessible.role: Accessible.Button
                    Accessible.onPressAction: {
                        if (openable) {
                            win.nickSelectedIndex = index;
                            win.activateNickSelection();
                        }
                    }
                    width: nickList.width
                    height: win.scaledSize(43)

                    Rectangle {
                        objectName: "nickPickHighlight"
                        anchors.fill: parent
                        anchors.leftMargin: win.scaledSize(8)
                        anchors.rightMargin: win.scaledSize(8)
                        radius: win.scaledSize(7)
                        color: nickMouse.containsMouse || nickRow.selected
                            ? win.hoverColor
                            : "transparent"
                    }

                    Rectangle {
                        anchors.left: parent.left
                        anchors.leftMargin: win.scaledSize(18)
                        anchors.verticalCenter: parent.verticalCenter
                        width: win.scaledSize(28)
                        height: width
                        radius: width / 2
                        color: nickRow.avatarFill
                        opacity: nickRow.showAway ? 0.62 : 1

                        Text {
                            anchors.centerIn: parent
                            text: nickRow.glyph
                            color: nickRow.nickTint
                            font.family: "iA Writer Mono S"
                            font.bold: true
                            font.pixelSize: win.scaledSize(11)
                        }

                        Rectangle {
                            objectName: "nickPick-presence-" + nickRow.nick
                            visible: win.awayPresenceVisible || nickRow.isSelf
                            anchors.right: parent.right
                            anchors.bottom: parent.bottom
                            width: win.scaledSize(7)
                            height: width
                            radius: width / 2
                            color: nickRow.showAway ? "#d6a552" : "#69b978"
                            border.width: win.scaledSize(2)
                            border.color: win.raisedColor
                        }
                    }

                    Column {
                        anchors.left: parent.left
                        anchors.leftMargin: win.scaledSize(56)
                        anchors.right: parent.right
                        anchors.rightMargin: win.scaledSize(10)
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 0

                        Text {
                            width: parent.width
                            text: nickRow.label
                            color: nickRow.showAway ? win.mutedColor : win.inkColor
                            elide: Text.ElideRight
                            font.family: "iA Writer Mono S"
                            font.bold: nickRow.nick === win.selfNick
                            font.pixelSize: win.scaledSize(12)
                        }

                        Text {
                            objectName: "nickPick-status-" + nickRow.nick
                            visible: win.memberStatusVisible && nickRow.memberStatus.length > 0
                            width: parent.width
                            text: nickRow.memberStatus
                            color: win.mutedColor
                            elide: Text.ElideRight
                            font.family: "iA Writer Mono S"
                            font.pixelSize: win.scaledSize(9)
                        }
                    }

                    MouseArea {
                        id: nickMouse
                        anchors.fill: parent
                        enabled: nickRow.openable
                        hoverEnabled: true
                        cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                        onClicked: {
                            win.nickSelectedIndex = nickRow.index;
                            win.activateNickSelection();
                        }
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
