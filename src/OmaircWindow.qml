import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Layouts
import QtQuick.Window
import Omairc.App 1.0
import "qml"

ApplicationWindow {
    id: win

    required property var backend
    required property var irc
    property var connection: null
    property var slashCommands: null
    property var avatarStore: null
    readonly property bool peerAvatarsEnabled: !irc || irc.loadPeerAvatars !== false
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

    OmaircStyle {
        id: omaircStyle
        backend: win.backend
    }
    readonly property alias style: omaircStyle

    readonly property bool darkMode: style.darkMode
    readonly property real textScale: style.textScale
    readonly property color pageColor: style.pageColor
    readonly property color inkColor: style.inkColor
    readonly property color accentColor: style.accentColor
    readonly property color selectionColor: style.selectionColor
    readonly property color panelColor: style.panelColor
    readonly property color raisedColor: style.raisedColor
    readonly property color hoverColor: style.hoverColor
    readonly property color dividerColor: style.dividerColor
    readonly property color mutedColor: style.mutedColor
    readonly property string appVersion: Qt.application.version
    readonly property var nickPalette: style.nickPalette
    readonly property real nickAvatarMix: style.nickAvatarMix
    readonly property var nickAvatarFills: style.nickAvatarFills

    property bool membersVisible: true
    property bool serverListVisible: true
    property string sidebarNetworkFocusId: ""
    // The composer routes Enter to the focused network header, so a collapsed
    // rail must not keep a selection armed that nobody can see. This handler
    // sits below the sidebarNetworkFocusId declaration so the dependency reads
    // top-to-bottom.
    onServerListVisibleChanged: {
        if (!serverListVisible)
            sidebarNetworkFocusId = "";
    }
    property bool shortcutsSheetEscapeGuard: false
    property bool aboutSheetEscapeGuard: false
    property bool pickerEscapeGuard: false
    property int jumpSelectedIndex: 0
    property int nickSelectedIndex: 0
    property var nickSourceRows: []
    readonly property bool shortcutOverlayOpen: shortcutsSheet.opened
        || jumpSheet.opened
        || nickSheet.opened
        || aboutSheet.opened
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
    // Same Connected gate as the network header status mark: Offline /
    // Connecting / Reconnecting stay muted "offline", then away or available.
    readonly property string selfPresence: {
        if (irc && irc.connectionStatus !== "Connected")
            return "offline"
        return selfAway ? "away" : "available"
    }
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
    readonly property color overlayVeilColor: style.overlayVeilColor

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

    Text {
        id: messageLineProbe
        visible: false
        text: "X"
        font.family: win.transcriptBodyFont.family
        font.pixelSize: win.transcriptBodyFont.pixelSize
    }

    function scaledSize(pixels) {
        return style.scaledSize(pixels);
    }

    function mixColors(base, tint, amount) {
        return style.mixColors(base, tint, amount);
    }

    function presenceMarkColor(kind) {
        return style.presenceMarkColor(kind);
    }

    function focusedNetworkDisplayName() {
        var count = modelRowCount(connection ? connection.networks : null);
        for (var row = 0; row < count; ++row) {
            var item = networkAt(row);
            if (item && item.networkId === currentNetworkId)
                return item.displayName || "";
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
        return style.paletteColor(index);
    }

    function nickPaletteIndex(nick) {
        return style.nickPaletteIndex(nick);
    }

    function nickColor(nick) {
        return style.nickColor(nick);
    }

    function initials(nick) {
        return style.initials(nick);
    }

    function peerFacts(nick) {
        if (!irc || typeof irc.peerMetadata !== "function" || !nick)
            return { avatar: "", bot: false };
        // peerMetadata is a plain invokable. Read peerMetadataEpoch so an
        // inbound 761 / 766 re-runs this binding without a selection churn.
        var _epoch = irc.peerMetadataEpoch
        var networkId = irc.selectedNetworkId;
        if (!networkId || networkId.length === 0)
            networkId = win.currentNetworkId;
        if (!networkId)
            return { avatar: "", bot: false };
        var facts = irc.peerMetadata(networkId, nick);
        return facts ? facts : { avatar: "", bot: false };
    }

    function peerAvatar(nick) {
        var facts = peerFacts(nick);
        return facts.avatar ? facts.avatar : "";
    }

    function peerBot(nick) {
        return !!(peerFacts(nick).bot);
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
        return modelRowCount(model);
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

    function openAboutFromVersionClick(mappedItem, mouse) {
        if (!selfVersionHit.visible)
            return false;
        var point = mappedItem.mapToItem(selfVersionHit, mouse.x, mouse.y);
        if (!selfVersionHit.contains(point))
            return false;
        aboutSheet.open();
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

    function modelRowCount(model) {
        if (!model)
            return 0;
        if (typeof model.count === "number")
            return model.count;
        if (typeof model.rowCount === "function")
            return model.rowCount();
        return 0;
    }

    function modelRowMap(model, row) {
        if (!model || row < 0 || row >= modelRowCount(model))
            return null;
        if (typeof model.get === "function")
            return model.get(row);
        return null;
    }

    function conversationAt(row) {
        var item = modelRowMap(irc ? irc.conversations : null, row);
        if (!item)
            return null;
        var name = item.conversationName || item.conversation || "";
        if (name.length === 0)
            return null;
        return {
            conversationName: name,
            conversationId: item.conversationId || "",
            networkId: item.networkId || "",
            unread: item.unread || 0,
            mention: !!item.mention,
            muted: !!item.muted,
            direct: !!item.direct
        };
    }

    function networkAt(row) {
        var item = modelRowMap(connection ? connection.networks : null, row);
        if (!item)
            return null;
        var id = item.networkId || "";
        if (id.length === 0)
            return null;
        return {
            networkId: id,
            displayName: item.displayName || ""
        };
    }

    function conversationObjectName(networkId, name) {
        return networkId && networkId.length > 0
            ? "conversation-" + networkId + "-" + name
            : "conversation-" + name;
    }

    function findVisibleNamedItem(name) {
        if (!name || name.length === 0 || !sidebarScroll)
            return null;
        var found = null;
        function walk(node) {
            if (!node || found)
                return;
            if (node.objectName === name && node.visible && node.height > 0) {
                found = node;
                return;
            }
            var kids = node.children;
            if (kids) {
                for (var index = 0; index < kids.length; ++index)
                    walk(kids[index]);
            }
            if (!found && node.contentItem)
                walk(node.contentItem);
        }
        walk(sidebarScroll.contentItem);
        return found;
    }

    function sidebarConversationRows() {
        var rows = [];
        if (!irc || !connection)
            return rows;
        var count = modelRowCount(irc.conversations);
        for (var row = 0; row < count; ++row) {
            var item = conversationAt(row);
            if (item)
                rows.push(item);
        }
        return rows;
    }

    function sectionHasDirects(networkId) {
        if (!irc)
            return false;
        var model = irc.conversations;
        if (!model)
            return false;
        if (typeof model.hasDirects === "function")
            return model.hasDirects(networkId);
        var count = modelRowCount(model);
        for (var row = 0; row < count; ++row) {
            var item = conversationAt(row);
            if (item && item.direct && item.networkId === networkId)
                return true;
        }
        return false;
    }

    function sidebarNetworkSections() {
        var sections = [];
        if (!irc || !connection)
            return sections;
        var count = modelRowCount(connection.networks);
        for (var row = 0; row < count; ++row) {
            var item = networkAt(row);
            if (item)
                sections.push(item);
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
        var query = jumpSheet.jumpFilter ? jumpSheet.jumpFilter.text.trim().toLowerCase() : "";
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
        var empty = { nick: "", label: "", status: "", away: false, avatar: "", bot: false };
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
                away: !!(rowData && rowData.away),
                avatar: rowData && rowData.avatar ? rowData.avatar : "",
                bot: !!(rowData && rowData.bot)
            };
        }
        var idx = model.index(row, 0);
        return {
            nick: model.data(idx, Qt.UserRole + 1) || "",
            label: model.data(idx, Qt.UserRole + 2) || "",
            status: model.data(idx, Qt.UserRole + 3) || "",
            away: model.data(idx, Qt.UserRole + 4) === true,
            avatar: model.data(idx, Qt.UserRole + 6) || "",
            bot: model.data(idx, Qt.UserRole + 7) === true
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
        var query = nickSheet.nickFilter ? nickSheet.nickFilter.text.trim().toLowerCase() : "";
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
                    botFlag: member.bot ? 1 : 0,
                    avatar: member.avatar || "",
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
        if (jumpSheet.jumpList)
            jumpSheet.jumpList.positionViewAtIndex(jumpSelectedIndex, ListView.Contain);
    }

    function stepNick(delta) {
        if (nickModel.count === 0)
            return;
        nickSelectedIndex = (nickSelectedIndex + delta + nickModel.count) % nickModel.count;
        if (nickSheet.nickList)
            nickSheet.nickList.positionViewAtIndex(nickSelectedIndex, ListView.Contain);
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
                activateSidebarConversation(rows[index]);
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
        revealNamedSidebarItem("networkHeader-" + section.networkId);
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
        // Walking networks is rail navigation: if the rail is collapsed the
        // highlight would land on a row nobody can see.
        serverListVisible = true;
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
        activateSidebarConversation(rows[nextIndex]);
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

    function revealNamedSidebarItem(name) {
        revealSidebarRow(findVisibleNamedItem(name));
    }

    function activateSidebarConversation(row) {
        if (!row)
            return;
        selectConversation(row.conversationName, row.networkId);
        Qt.callLater(function() {
            revealNamedSidebarItem(conversationObjectName(row.networkId,
                                                          row.conversationName));
        });
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
            activateSidebarConversation(target);
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

    function rememberSentComposerLine(text, key) {
        resetNickComplete();
        resetComposerHistoryBrowse();
        var historyKey = key && key.length ? key : composerHistoryKey();
        var lines = (composerHistories[historyKey] || []).slice();
        lines.push(text);
        if (lines.length > 50)
            lines.shift();
        composerHistories[historyKey] = lines;
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

    function dispatchComposerSend(fromConsole, original) {
        if (fromConsole) {
            return irc && networkConsole
                && networkConsole.submit(original)
                && networkConsole.lastSubmitAccepted !== false;
        }
        return irc && irc.sendMessage(original);
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
        var sentFromConversationId = currentConversationId;
        suppressComposerStash = true;
        Qt.callLater(function() { suppressComposerStash = false; });
        if (sentFromKey.length > 0)
            composerDrafts[sentFromKey] = "";
        composer.clear();

        var sent = dispatchComposerSend(fromConsole, original);
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

        rememberSentComposerLine(original, sentFromKey);
        var kept = composerDrafts[composerDraftKey] || "";
        composer.text = kept;
        composer.cursorPosition = composer.text.length;
        if (consoleVisible)
            consoleList.pinToEnd();
        else
            messageList.pinToEnd();
        if (consoleVisible !== fromConsole
                || currentConversationId !== sentFromConversationId) {
            Qt.callLater(function() {
                composer.forceActiveFocus();
            });
        }
    }

    Shortcut {
        sequence: "Ctrl+Q"
        context: Qt.ApplicationShortcut
        enabled: !win.shortcutOverlayOpen
        onActivated: win.close()
    }

    Shortcut {
        sequence: "Ctrl+L"
        context: Qt.ApplicationShortcut
        enabled: !win.connectionOverlayVisible && !win.shortcutOverlayOpen
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
            && !aboutSheet.opened
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

    // The server list is always available, so unlike the members panel this
    // toggle is not gated on the conversation being a channel.
    Shortcut {
        sequence: "Ctrl+Shift+S"
        context: Qt.ApplicationShortcut
        enabled: !win.connectionOverlayVisible && !win.shortcutOverlayOpen
        onActivated: win.serverListVisible = !win.serverListVisible
    }

    Shortcut {
        sequence: "Ctrl+W"
        context: Qt.ApplicationShortcut
        enabled: !currentConversationIsChannel && !consoleVisible
            && !win.connectionOverlayVisible && !win.shortcutOverlayOpen
        onActivated: win.closeDirectMessage()
    }

    Shortcut {
        sequence: "Ctrl+/"
        context: Qt.ApplicationShortcut
        onActivated: {
            if (jumpSheet.opened || nickSheet.opened || aboutSheet.opened)
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
        enabled: !win.connectionOverlayVisible && !win.shortcutOverlayOpen
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
        enabled: !win.connectionOverlayVisible && !win.shortcutOverlayOpen
        onActivated: stepConversation(1)
    }

    Shortcut {
        sequence: "Alt+Up"
        context: Qt.ApplicationShortcut
        enabled: !win.connectionOverlayVisible && !win.shortcutOverlayOpen
        onActivated: stepConversation(-1)
    }

    Shortcut {
        sequence: "Alt+Right"
        context: Qt.ApplicationShortcut
        enabled: !win.connectionOverlayVisible && !win.shortcutOverlayOpen
        onActivated: stepNetwork(1)
    }

    Shortcut {
        sequence: "Alt+Left"
        context: Qt.ApplicationShortcut
        enabled: !win.connectionOverlayVisible && !win.shortcutOverlayOpen
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
        enabled: !win.connectionOverlayVisible && !win.shortcutOverlayOpen
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
            if (aboutSheet.opened || aboutSheetEscapeGuard)
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
            if (aboutSheet.opened || aboutSheetEscapeGuard) {
                aboutSheet.close();
                aboutSheetEscapeGuard = false;
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

    function clearConnectionPassword() {
        connectionSheet.connectionPassword.text = "";
        connectionPasswordEdited = false;
        connectionSheet.connectionNickServ.text = "";
        connectionNickServEdited = false;
    }

    function submitConnection() {
        if (!connection || connection.problem.length > 0)
            return;
        if (connectionPasswordEdited)
            connection.setPassword(connectionSheet.connectionPassword.text);
        if (connectionNickServEdited)
            connection.setNickServPassword(connectionSheet.connectionNickServ.text);
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
        var buttons = connectionSheet.connectionTabs.children;
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
            connectionSheet.connectionNickField.focusInput();
        else
            connectionSheet.connectionNameField.focusInput();
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

    Connections {
        target: win.connection
        ignoreUnknownSignals: true
        function onSelectedNetworkChanged() {
            win.clearConnectionPassword();
        }
        function onFocusPasswordChanged() {
            if (win.connection && win.connection.focusPassword) {
                win.connectionSheetOpen = true;
                connectionSheet.connectionPassword.focusInput();
            }
        }
        function onFocusNickServChanged() {
            if (win.connection && win.connection.focusNickServ) {
                win.connectionSheetOpen = true;
                connectionSheet.connectionNickServ.focusInput();
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

    RowLayout {
        anchors.fill: parent
        spacing: 0
        enabled: !win.connectionOverlayVisible

        Rectangle {
            id: sidebar
            objectName: "serverList"
            // Collapse the rail by width instead of hiding it. Walk order
            // comes from irc.conversations / connection.networks, so
            // Alt+Up/Down still reaches every row while the column is
            // clipped. clip keeps the collapsed content from painting over
            // the transcript. Alt+Left/Right restores the column so the
            // header highlight stays visible.
            clip: true
            Layout.preferredWidth: win.serverListVisible ? win.scaledSize(244) : 0
            Layout.minimumWidth: win.serverListVisible ? win.scaledSize(214) : 0
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
                            required property string iconUrl
                            width: parent ? parent.width : 0
                            spacing: 0

                            NetworkSection {
                                style: win.style
                                networkId: liveNet.networkId
                                displayName: liveNet.displayName
                                iconColor: liveNet.iconColor
                                iconUrl: liveNet.iconUrl
                                unread: 0
                                mention: false
                                showEdit: win.connection !== null
                                headerFocused: liveNet.networkId.length > 0
                                    && liveNet.networkId === win.sidebarNetworkFocusId
                                irc: win.irc
                                networkConsole: win.networkConsole
                                avatarStore: win.avatarStore
                                onStatusRequested: function(networkId) {
                                    win.openNetworkStatus(networkId);
                                }
                                onEditRequested: function(networkId) {
                                    if (win.connection)
                                        win.selectSheetNetwork(networkId);
                                    win.connectionSheetOpen = true;
                                }
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
                                    id: channelRow
                                    required property var model
                                    style: win.style
                                    conversationName: model.conversation
                                    conversationId: model.conversationId
                                    unread: model.unread || 0
                                    mention: !!model.mention
                                    muted: !!model.muted
                                    direct: model.direct
                                    typing: model.typing
                                    networkId: model.networkId
                                    current: conversationId === win.currentConversationId
                                        && !win.consoleVisible
                                    awayPresenceVisible: win.awayPresenceVisible
                                    avatarStore: win.avatarStore
                                    loadPeerAvatars: win.peerAvatarsEnabled
                                    visible: !model.direct
                                        && model.networkId === liveNet.networkId
                                    width: sidebar.width
                                    height: visible ? win.scaledSize(36) : 0
                                    onActivated: win.activateSidebarConversation(channelRow)
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
                                    id: directRow
                                    required property var model
                                    style: win.style
                                    conversationName: model.conversation
                                    conversationId: model.conversationId
                                    unread: model.unread || 0
                                    mention: !!model.mention
                                    muted: !!model.muted
                                    direct: model.direct
                                    typing: model.typing
                                    presence: model.presence || ""
                                    networkId: model.networkId
                                    avatar: model.direct ? (model.avatar || "") : ""
                                    bot: !!(model.direct && model.bot)
                                    current: conversationId === win.currentConversationId
                                        && !win.consoleVisible
                                    awayPresenceVisible: win.awayPresenceVisible
                                    avatarStore: win.avatarStore
                                    loadPeerAvatars: win.peerAvatarsEnabled
                                    visible: model.direct
                                        && model.networkId === liveNet.networkId
                                    width: sidebar.width
                                    height: visible ? win.scaledSize(36) : 0
                                    onActivated: win.activateSidebarConversation(directRow)
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

                Item {
                    anchors.left: parent.left
                    anchors.leftMargin: win.scaledSize(17)
                    anchors.verticalCenter: parent.verticalCenter
                    width: win.scaledSize(34)
                    height: width

                    NickGlyph {
                        objectName: "selfNickGlyph"
                        anchors.fill: parent
                        style: win.style
                        avatarStore: win.avatarStore
                        loadPeerAvatars: win.peerAvatarsEnabled
                        nick: win.selfNick
                        avatarUrl: win.peerAvatar(win.selfNick)
                        fill: win.mixColors(win.pageColor, win.nickColor(win.selfNick), 0.24)
                        fontPixelSize: win.scaledSize(14)
                    }

                    Rectangle {
                        objectName: "selfPresenceDot"
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        width: win.scaledSize(9)
                        height: width
                        radius: width / 2
                        color: win.presenceMarkColor(
                            win.selfPresence === "away" ? "away"
                                : (win.selfPresence === "available"
                                    ? "online" : "offline"))
                        border.width: win.scaledSize(2)
                        border.color: win.panelColor
                    }
                }

                Column {
                    id: identityText
                    anchors.left: parent.left
                    anchors.leftMargin: win.scaledSize(63)
                    anchors.right: selfVersionHit.visible ? selfVersionHit.left : parent.right
                    anchors.rightMargin: win.scaledSize(selfVersionHit.visible ? 8 : 17)
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: win.scaledSize(1)

                    Row {
                        width: parent.width
                        spacing: win.scaledSize(4)

                        Text {
                            objectName: "selfNickLabel"
                            width: {
                                var reserved = win.peerBot(win.selfNick)
                                    ? selfBotMark.width + parent.spacing : 0;
                                var cap = Math.max(0, parent.width - reserved);
                                return Math.min(implicitWidth, cap);
                            }
                            text: win.selfNick
                            color: win.inkColor
                            elide: Text.ElideRight
                            font.family: "iA Writer Mono S"
                            font.bold: true
                            font.pixelSize: win.scaledSize(13)
                        }

                        BotMark {
                            id: selfBotMark
                            style: win.style
                            objectName: "selfBotMark"
                            shown: win.peerBot(win.selfNick)
                            anchors.verticalCenter: parent.verticalCenter
                        }
                    }

                    Text {
                        objectName: "selfPresenceLabel"
                        text: win.selfPresence
                        color: win.mutedColor
                        font.family: "iA Writer Mono S"
                        font.pixelSize: win.scaledSize(10)
                    }
                }

                MouseArea {
                    id: selfVersionHit
                    objectName: "selfVersionHit"
                    Accessible.role: Accessible.Button
                    Accessible.name: "About Omairc"
                    Accessible.onPressAction: aboutSheet.open()
                    anchors.right: parent.right
                    anchors.rightMargin: win.scaledSize(8)
                    anchors.bottom: identityText.bottom
                    anchors.top: identityText.top
                    width: selfVersionLabel.implicitWidth + win.scaledSize(18)
                    visible: selfVersionLabel.text.length > 0
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: aboutSheet.open()

                    Text {
                        id: selfVersionLabel
                        objectName: "selfVersionLabel"
                        anchors.right: parent.right
                        anchors.rightMargin: win.scaledSize(9)
                        anchors.bottom: parent.bottom
                        text: win.appVersion
                        color: selfVersionHit.containsMouse ? win.inkColor : win.mutedColor
                        font.family: "iA Writer Mono S"
                        font.pixelSize: win.scaledSize(10)
                        font.underline: selfVersionHit.containsMouse
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
                    required property var model
                    // Read roles through model so MessageListModel dataChanged
                    // refreshes them. Keep them optional so ListModel fixtures
                    // without avatar/bot roles still instantiate.
                    readonly property string authorAvatar: model && model.authorAvatar
                        ? String(model.authorAvatar) : ""
                    readonly property bool authorBot: !!(model && model.authorBot)
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

                    TextEdit {
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
                        textFormat: TextEdit.PlainText
                        color: win.mutedColor
                        wrapMode: TextEdit.Wrap
                        readOnly: true
                        selectByMouse: true
                        selectionColor: win.selectionColor
                        selectedTextColor: "#ffffff"
                        cursorVisible: false
                        activeFocusOnPress: false
                        activeFocusOnTab: false
                        padding: 0
                        font.family: "iA Writer Mono S"
                        font.pixelSize: win.scaledSize(12)

                        PlainUrlHit { edit: messageWhois }
                    }

                    MessageAvatar {
                        objectName: "messageAvatar"
                        visible: messageDelegate.isChat && !messageDelegate.grouped
                        style: win.style
                        avatarStore: win.avatarStore
                        loadPeerAvatars: win.peerAvatarsEnabled
                        selfNick: win.selfNick
                        author: messageDelegate.author
                        avatarUrl: messageDelegate.authorAvatar
                        replayed: messageDelegate.replayed
                        nickOpensDirect: true
                        onDirectMessageRequested: function(nick) { win.openDirectMessage(nick) }
                    }

                    MessageHeader {
                        objectName: "messageHeader"
                        visible: messageDelegate.isChat && !messageDelegate.grouped
                        style: win.style
                        selfNick: win.selfNick
                        author: messageDelegate.author
                        time: messageDelegate.time
                        replayed: messageDelegate.replayed
                        nickOpensDirect: true
                        bot: messageDelegate.authorBot
                        onDirectMessageRequested: function(nick) { win.openDirectMessage(nick) }
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
                        style: win.style
                        avatarStore: win.avatarStore
                        loadPeerAvatars: win.peerAvatarsEnabled
                        selfNick: win.selfNick
                        author: typingRow.nick
                        avatarUrl: win.peerAvatar(typingRow.nick)
                        replayed: false
                        onDirectMessageRequested: function(nick) { win.openDirectMessage(nick) }
                    }

                    MessageHeader {
                        objectName: "typingTranscriptHeader"
                        visible: typingRow.show && !typingRow.grouped
                        style: win.style
                        selfNick: win.selfNick
                        author: typingRow.nick
                        time: ""
                        replayed: false
                        bot: win.peerBot(typingRow.nick)
                        onDirectMessageRequested: function(nick) { win.openDirectMessage(nick) }
                    }

                    TypingDots {
                        id: typingRowDots
                        style: win.style
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
                style: win.style
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
                style: win.style
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
                ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
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
                        ? ({nick: model.nick, label: model.label, status: model.status,
                            away: model.away, avatar: model.avatar || "", bot: !!model.bot})
                        : ({nick: "", label: "", status: "", away: false, avatar: "", bot: false})
                    readonly property string nick: memberData.nick
                    readonly property string label: memberData.label
                    readonly property string status: memberData.status
                    readonly property string avatar: memberData.avatar
                    readonly property bool bot: memberData.bot
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

                    Item {
                        anchors.left: parent.left
                        anchors.leftMargin: win.scaledSize(18)
                        anchors.verticalCenter: parent.verticalCenter
                        width: win.scaledSize(28)
                        height: width

                        NickGlyph {
                            anchors.fill: parent
                            style: win.style
                            avatarStore: win.avatarStore
                            loadPeerAvatars: win.peerAvatarsEnabled
                            nick: memberDelegate.nick
                            avatarUrl: memberDelegate.avatar
                            dimmed: memberDelegate.away
                            fontPixelSize: win.scaledSize(11)
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
                                width: {
                                    var reserved = (memberDelegate.bot
                                        ? memberBotMark.width + parent.spacing : 0)
                                        + (memberDelegate.typing
                                            ? memberTypingGlyph.implicitWidth + parent.spacing
                                            : 0);
                                    var cap = Math.max(0, parent.width - reserved);
                                    return Math.min(implicitWidth, cap);
                                }
                                text: memberDelegate.label
                                color: memberDelegate.away ? win.mutedColor : win.inkColor
                                elide: Text.ElideRight
                                font.family: "iA Writer Mono S"
                                font.bold: memberDelegate.nick === win.selfNick
                                font.pixelSize: win.scaledSize(12)
                            }

                            BotMark {
                                id: memberBotMark
                                style: win.style
                                objectName: "member-bot-" + memberDelegate.nick
                                shown: memberDelegate.bot
                                anchors.verticalCenter: parent.verticalCenter
                            }

                            TypingDots {
                                id: memberTypingGlyph
                                style: win.style
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

    // Cover the whole window so servers, conversations, and members cannot
    // be clicked or focused while Connect is up. The dimmer used to live in
    // the conversation pane only, which left the side columns interactive.
    ConnectionSheet {
        id: connectionSheet
        objectName: "connectionSheet"
        style: win.style
        connection: win.connection
        irc: win.irc
        currentTab: win.connectionSheetTab
        removeArmed: win.connectionRemoveArmed
        anchors.fill: parent
        z: 1
        visible: win.connection && (win.connection.setupRequired || win.connectionSheetOpen)
        onOpened: {
            win.connectionSheetTab = "connection";
            Qt.callLater(win.focusConnectionSheetStart);
        }
        onClosed: {
            // Closing used to drop focus on the window itself, so
            // typing did nothing until the composer was clicked.
            Qt.callLater(function() {
                if (win && win.active && composer)
                    composer.forceActiveFocus();
            });
        }
        onDimmerClicked: function(mouse) {
            if (win.openAboutFromVersionClick(connectionSheet.clickSink, mouse))
                return;
            // Hide on the next tick so this same click cannot land on the
            // sidebar or member list once the dimmer is gone.
            if (win.connection && !win.connection.setupRequired)
                Qt.callLater(function() {
                    if (win)
                        win.connectionSheetOpen = false;
                });
        }
        onCardClicked: function(mouse) {
            win.openAboutFromVersionClick(connectionSheet.cardClickSink, mouse);
        }
        onTabSelected: function(tabName) {
            win.selectConnectionSheetTab(tabName);
        }
        onTabStepRequested: function(direction) {
            win.stepConnectionSheetTab(direction);
        }
        onNetworkSelected: function(networkId) {
            win.selectSheetNetwork(networkId);
        }
        onSubmitRequested: win.submitConnection()
        onAddNetworkRequested: win.addSheetNetwork()
        onDiscardRequested: win.discardSheetConnection()
        onRemoveRequested: win.removeSheetNetwork()
        onDisconnectRequested: win.disconnectSheetNetwork()
        onApplyKeyRequested: function(event) {
            win.applyFromSheetKey(event);
        }
        onPasswordEdited: win.connectionPasswordEdited = true
        onNickServEdited: win.connectionNickServEdited = true
        onShortcutsRequested: shortcutsSheet.open()
    }

    UpdateCheck {
        id: aboutUpdateCheck
        objectName: "aboutUpdateCheck"
    }

    ShortcutsSheet {
        id: shortcutsSheet
        objectName: "shortcutsSheet"
        style: win.style
        onOpened: shortcutsSheetEscapeGuard = true
        onClosed: Qt.callLater(function() { shortcutsSheetEscapeGuard = false })
    }

    AboutSheet {
        id: aboutSheet
        objectName: "aboutSheet"
        style: win.style
        appVersion: win.appVersion
        updateCheck: aboutUpdateCheck
        anchors.fill: parent
        z: 2
        onShown: aboutSheetEscapeGuard = true
        onClosed: {
            aboutUpdateCheck.cancel();
            Qt.callLater(function() {
                aboutSheetEscapeGuard = false;
                if (win.connectionOverlayVisible)
                    win.focusConnectionSheetStart();
                else
                    composer.forceActiveFocus();
            });
        }
        onUrlRequested: function(url) {
            win.openAllowedUrl(url);
        }
        onCheckUpdatesRequested: aboutUpdateCheck.check()
    }

    ListModel {
        id: jumpModel
        objectName: "jumpModel"
    }

    ListModel {
        id: nickModel
        objectName: "nickModel"
    }

    JumpSheet {
        id: jumpSheet
        objectName: "jumpSheet"
        style: win.style
        matches: jumpModel
        selectedIndex: win.jumpSelectedIndex
        onOpened: {
            win.pickerEscapeGuard = true;
            win.jumpSelectedIndex = 0;
            if (jumpSheet.jumpFilter.text.length > 0)
                jumpSheet.jumpFilter.clear();
            else
                win.refreshJumpMatches();
            jumpSheet.jumpFilter.forceActiveFocus();
        }
        onClosed: {
            Qt.callLater(function() {
                win.pickerEscapeGuard = false;
                composer.forceActiveFocus();
            });
        }
        onStepRequested: function(delta) { win.stepJump(delta); }
        onActivateRequested: win.activateJumpSelection()
        onFilterChanged: {
            win.jumpSelectedIndex = 0;
            win.refreshJumpMatches();
        }
    }

    NickSheet {
        id: nickSheet
        objectName: "nickSheet"
        style: win.style
        matches: nickModel
        selectedIndex: win.nickSelectedIndex
        selfNick: win.selfNick
        awayPresenceVisible: win.awayPresenceVisible
        memberStatusVisible: win.memberStatusVisible
        avatarStore: win.avatarStore
        loadPeerAvatars: win.peerAvatarsEnabled
        onOpened: {
            win.pickerEscapeGuard = true;
            if (nickSheet.nickFilter.text.length > 0)
                nickSheet.nickFilter.clear();
            else
                win.refreshNickMatches();
            win.nickSelectedIndex = win.firstOpenableNickIndex();
            nickSheet.nickFilter.forceActiveFocus();
        }
        onClosed: {
            Qt.callLater(function() {
                win.pickerEscapeGuard = false;
                composer.forceActiveFocus();
            });
        }
        onStepRequested: function(delta) { win.stepNick(delta); }
        onActivateRequested: win.activateNickSelection()
        onFilterChanged: {
            win.refreshNickMatches();
            win.nickSelectedIndex = win.firstOpenableNickIndex();
        }
        onNickActivated: function(index) {
            win.nickSelectedIndex = index;
            win.activateNickSelection();
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
