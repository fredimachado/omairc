import QtQuick
import QtQuick.Controls

Rectangle {
    id: column

    required property OmaircStyle style
    property var irc: null
    property var connection: null
    property var networkConsole: null
    property var avatarStore: null
    property bool loadPeerAvatars: true
    property bool awayPresenceVisible: false
    property bool consoleVisible: false
    property string currentConversationId: ""
    property string sidebarNetworkFocusId: ""
    property string selfNick: ""
    property string selfPresence: ""
    property string selfAvatar: ""
    property bool selfBot: false
    property string appVersion: ""
    property int inboxCount: 0

    property alias sidebarScroll: sidebarScroll
    property alias selfVersionHit: selfVersionHit

    signal conversationActivated(var row)
    signal statusRequested(string networkId)
    signal editRequested(string networkId)
    signal versionClicked()
    signal inboxRequested()

    objectName: "serverList"
    // Collapse the rail by width instead of hiding it. Walk order comes from
    // irc.conversations / connection.networks, so Alt+Up/Down still reaches
    // every row while the column is clipped. clip keeps the collapsed content
    // from painting over the transcript. Alt+Left/Right restores the column
    // so the header highlight stays visible.
    clip: true
    color: column.style.panelColor

    function sectionHasDirects(networkId) {
        if (!column.irc)
            return false;
        var model = column.irc.conversations;
        if (!model)
            return false;
        if (typeof model.hasDirects === "function")
            return model.hasDirects(networkId);
        var count = 0;
        if (typeof model.count === "number")
            count = model.count;
        else if (typeof model.rowCount === "function")
            count = model.rowCount();
        for (var row = 0; row < count; ++row) {
            var item = null;
            if (typeof model.get === "function")
                item = model.get(row);
            if (item && item.direct && item.networkId === networkId)
                return true;
        }
        return false;
    }

    Rectangle {
        anchors.right: parent.right
        width: 1
        height: parent.height
        color: column.style.dividerColor
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
                model: column.irc && column.connection ? column.connection.networks : null

                Column {
                    id: liveNet
                    required property int index
                    required property string networkId
                    required property string displayName
                    required property int iconColor
                    required property string iconUrl
                    required property bool collapsed
                    width: parent ? parent.width : 0
                    spacing: 0

                    NetworkSection {
                        style: column.style
                        networkId: liveNet.networkId
                        displayName: liveNet.displayName
                        iconColor: liveNet.iconColor
                        iconUrl: liveNet.iconUrl
                        unread: 0
                        mention: false
                        showEdit: column.connection !== null
                        headerFocused: liveNet.networkId.length > 0
                            && liveNet.networkId === column.sidebarNetworkFocusId
                        collapsed: liveNet.collapsed
                        irc: column.irc
                        networkConsole: column.networkConsole
                        avatarStore: column.avatarStore
                        onStatusRequested: function(networkId) {
                            column.statusRequested(networkId);
                        }
                        onEditRequested: function(networkId) {
                            column.editRequested(networkId);
                        }
                        onCollapseToggled: {
                            if (!column.connection)
                                return;
                            column.connection.setNetworkCollapsed(
                                liveNet.networkId, !liveNet.collapsed);
                        }
                    }

                    Item {
                        objectName: "channelsHeading-" + liveNet.networkId
                        width: parent.width
                        height: liveNet.collapsed ? 0 : column.style.scaledSize(28)
                        visible: height > 0
                        Text {
                            anchors.left: parent.left
                            anchors.leftMargin: column.style.scaledSize(19)
                            anchors.verticalCenter: parent.verticalCenter
                            text: "CHANNELS"
                            color: column.style.mutedColor
                            font.family: "iA Writer Mono S"
                            font.bold: true
                            font.letterSpacing: column.style.scaledSize(0.8)
                            font.pixelSize: column.style.scaledSize(9)
                        }
                    }

                    Repeater {
                        objectName: liveNet.index === 0
                            ? "channelConversationRepeater"
                            : "channelConversationRepeater-" + liveNet.networkId
                        model: column.irc ? column.irc.conversations : null
                        delegate: ConversationRow {
                            id: channelRow
                            required property var model
                            style: column.style
                            conversationName: model.conversation
                            conversationId: model.conversationId
                            unread: model.unread || 0
                            mention: !!model.mention
                            muted: !!model.muted
                            direct: model.direct
                            typing: model.typing
                            networkId: model.networkId
                            current: conversationId === column.currentConversationId
                                && !column.consoleVisible
                            awayPresenceVisible: column.awayPresenceVisible
                            avatarStore: column.avatarStore
                            loadPeerAvatars: column.loadPeerAvatars
                            visible: !model.direct
                                && model.networkId === liveNet.networkId
                                && !liveNet.collapsed
                            width: column.width
                            height: visible ? column.style.scaledSize(36) : 0
                            onActivated: column.conversationActivated(channelRow)
                        }
                    }

                    Item {
                        objectName: "directsHeading-" + liveNet.networkId
                        width: parent.width
                        height: {
                            if (liveNet.collapsed)
                                return 0;
                            var epoch = column.irc ? column.irc.conversationEpoch : 0;
                            return column.sectionHasDirects(liveNet.networkId)
                                ? column.style.scaledSize(36) : 0;
                        }
                        visible: height > 0
                        Text {
                            anchors.left: parent.left
                            anchors.leftMargin: column.style.scaledSize(19)
                            anchors.bottom: parent.bottom
                            anchors.bottomMargin: column.style.scaledSize(8)
                            text: "DIRECT MESSAGES"
                            color: column.style.mutedColor
                            font.family: "iA Writer Mono S"
                            font.bold: true
                            font.letterSpacing: column.style.scaledSize(0.8)
                            font.pixelSize: column.style.scaledSize(9)
                        }
                    }

                    Repeater {
                        objectName: liveNet.index === 0
                            ? "directConversationRepeater"
                            : "directConversationRepeater-" + liveNet.networkId
                        model: column.irc ? column.irc.conversations : null
                        delegate: ConversationRow {
                            id: directRow
                            required property var model
                            style: column.style
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
                            current: conversationId === column.currentConversationId
                                && !column.consoleVisible
                            awayPresenceVisible: column.awayPresenceVisible
                            avatarStore: column.avatarStore
                            loadPeerAvatars: column.loadPeerAvatars
                            visible: model.direct
                                && model.networkId === liveNet.networkId
                                && !liveNet.collapsed
                            width: column.width
                            height: visible ? column.style.scaledSize(36) : 0
                            onActivated: column.conversationActivated(directRow)
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
        height: column.style.scaledSize(66)
        color: column.style.mixColors(column.style.panelColor, column.style.inkColor,
                                      column.style.darkMode ? 0.025 : 0.018)

        Rectangle {
            anchors.top: parent.top
            width: parent.width
            height: 1
            color: column.style.dividerColor
        }

        Item {
            anchors.left: parent.left
            anchors.leftMargin: column.style.scaledSize(17)
            anchors.verticalCenter: parent.verticalCenter
            width: column.style.scaledSize(34)
            height: width

            NickGlyph {
                objectName: "selfNickGlyph"
                anchors.fill: parent
                style: column.style
                avatarStore: column.avatarStore
                loadPeerAvatars: column.loadPeerAvatars
                nick: column.selfNick
                avatarUrl: column.selfAvatar
                fill: column.style.mixColors(column.style.pageColor,
                    column.style.nickColor(column.selfNick), 0.24)
                fontPixelSize: column.style.scaledSize(14)
            }

            Rectangle {
                objectName: "selfPresenceDot"
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                width: column.style.scaledSize(9)
                height: width
                radius: width / 2
                color: column.style.presenceMarkColor(
                    column.selfPresence === "away" ? "away"
                        : (column.selfPresence === "available"
                            ? "online" : "offline"))
                border.width: column.style.scaledSize(2)
                border.color: column.style.panelColor
            }
        }

        Column {
            id: identityText
            anchors.left: parent.left
            anchors.leftMargin: column.style.scaledSize(63)
            anchors.right: footerRightColumn.visible ? footerRightColumn.left : parent.right
            anchors.rightMargin: column.style.scaledSize(footerRightColumn.visible ? 8 : 17)
            anchors.verticalCenter: parent.verticalCenter
            spacing: column.style.scaledSize(1)

            Row {
                width: parent.width
                spacing: column.style.scaledSize(4)

                Text {
                    objectName: "selfNickLabel"
                    width: {
                        var reserved = column.selfBot
                            ? selfBotMark.width + parent.spacing : 0;
                        var cap = Math.max(0, parent.width - reserved);
                        return Math.min(implicitWidth, cap);
                    }
                    text: column.selfNick
                    color: column.style.inkColor
                    elide: Text.ElideRight
                    font.family: "iA Writer Mono S"
                    font.bold: true
                    font.pixelSize: column.style.scaledSize(13)
                }

                BotMark {
                    id: selfBotMark
                    style: column.style
                    objectName: "selfBotMark"
                    shown: column.selfBot
                    anchors.verticalCenter: parent.verticalCenter
                }
            }

            Text {
                objectName: "selfPresenceLabel"
                text: column.selfPresence
                color: column.style.mutedColor
                font.family: "iA Writer Mono S"
                font.pixelSize: column.style.scaledSize(10)
            }
        }

        Column {
            id: footerRightColumn
            anchors.right: parent.right
            anchors.rightMargin: column.style.scaledSize(8)
            anchors.verticalCenter: parent.verticalCenter
            spacing: column.style.scaledSize(4)
            visible: inboxMarkHit.visible || selfVersionHit.visible

            MouseArea {
                id: inboxMarkHit
                objectName: "inboxMark"
                Accessible.role: Accessible.Button
                Accessible.name: "Inbox"
                Accessible.onPressAction: column.inboxRequested()
                width: Math.max(inboxMarkLabel.implicitWidth + column.style.scaledSize(18),
                                inboxBadge.visible
                                    ? inboxBadge.width + column.style.scaledSize(4)
                                        + inboxMarkLabel.implicitWidth
                                    : 0)
                height: Math.max(inboxMarkLabel.implicitHeight, inboxBadge.height)
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: column.inboxRequested()

                Text {
                    id: inboxMarkLabel
                    anchors.right: parent.right
                    anchors.rightMargin: column.style.scaledSize(9)
                    anchors.verticalCenter: parent.verticalCenter
                    text: "inbox"
                    color: inboxMarkHit.containsMouse ? column.style.inkColor : column.style.mutedColor
                    font.family: "iA Writer Mono S"
                    font.pixelSize: column.style.scaledSize(10)
                    font.underline: inboxMarkHit.containsMouse
                }

                Rectangle {
                    id: inboxBadge
                    visible: column.inboxCount > 0
                    anchors.right: inboxMarkLabel.left
                    anchors.rightMargin: column.style.scaledSize(4)
                    anchors.verticalCenter: parent.verticalCenter
                    width: Math.max(column.style.scaledSize(19),
                                    inboxBadgeText.implicitWidth + column.style.scaledSize(10))
                    height: column.style.scaledSize(19)
                    radius: height / 2
                    color: column.style.accentColor

                    Text {
                        id: inboxBadgeText
                        objectName: "inboxBadgeText"
                        anchors.centerIn: parent
                        text: column.inboxCount
                        color: "#ffffff"
                        font.family: "iA Writer Mono S"
                        font.bold: true
                        font.pixelSize: column.style.scaledSize(10)
                    }
                }
            }

            MouseArea {
                id: selfVersionHit
                objectName: "selfVersionHit"
                Accessible.role: Accessible.Button
                Accessible.name: "About Omairc"
                Accessible.onPressAction: column.versionClicked()
                width: selfVersionLabel.implicitWidth + column.style.scaledSize(18)
                height: selfVersionLabel.implicitHeight
                visible: selfVersionLabel.text.length > 0
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: column.versionClicked()

                Text {
                    id: selfVersionLabel
                    objectName: "selfVersionLabel"
                    anchors.right: parent.right
                    anchors.rightMargin: column.style.scaledSize(9)
                    anchors.verticalCenter: parent.verticalCenter
                    text: column.appVersion
                    color: selfVersionHit.containsMouse ? column.style.inkColor : column.style.mutedColor
                    font.family: "iA Writer Mono S"
                    font.pixelSize: column.style.scaledSize(10)
                    font.underline: selfVersionHit.containsMouse
                }
            }
        }
    }
}
