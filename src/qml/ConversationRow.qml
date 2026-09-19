import QtQuick

Item {
    id: conversationRow

    required property OmaircStyle style
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
    property string avatar: ""
    property bool bot: false
    property string conversationId: networkId + "\n" + conversationName
    property bool current: false
    property bool awayPresenceVisible: false
    property var avatarStore: null
    property bool loadPeerAvatars: true

    signal activated()

    objectName: networkId.length > 0
        ? "conversation-" + networkId + "-" + conversationName
        : "conversation-" + conversationName
    Accessible.name: conversationName
    Accessible.description: typing ? "Typing" : ""
    Accessible.role: Accessible.Button
    Accessible.onPressAction: conversationRow.activated()
    width: parent ? parent.width : 0
    height: conversationRow.style.scaledSize(36)

    Rectangle {
        anchors.fill: parent
        anchors.leftMargin: conversationRow.style.scaledSize(8)
        anchors.rightMargin: conversationRow.style.scaledSize(8)
        radius: conversationRow.style.scaledSize(7)
        color: conversationRow.current
            ? conversationRow.style.raisedColor
            : rowMouse.containsMouse ? conversationRow.style.hoverColor : "transparent"
    }

    Rectangle {
        visible: conversationRow.current
        anchors.left: parent.left
        anchors.leftMargin: conversationRow.style.scaledSize(8)
        anchors.verticalCenter: parent.verticalCenter
        width: conversationRow.style.scaledSize(3)
        height: conversationRow.style.scaledSize(18)
        radius: width
        color: conversationRow.style.accentColor
    }

    Item {
        visible: conversationRow.direct
        anchors.left: parent.left
        anchors.leftMargin: conversationRow.style.scaledSize(18)
        anchors.verticalCenter: parent.verticalCenter
        width: conversationRow.style.scaledSize(22)
        height: width

        NickGlyph {
            anchors.fill: parent
            style: conversationRow.style
            avatarStore: conversationRow.avatarStore
            loadPeerAvatars: conversationRow.loadPeerAvatars
            nick: conversationRow.conversationName
            avatarUrl: conversationRow.avatar
            fill: conversationRow.style.mixColors(conversationRow.style.pageColor,
                                conversationRow.style.nickColor(conversationRow.conversationName),
                                0.24)
            fontPixelSize: conversationRow.style.scaledSize(11)
        }

        Rectangle {
            objectName: "conversation-presence-" + conversationRow.networkId
                + "-" + conversationRow.conversationName
            // Other people's away state needs away-notify, exactly like the
            // member rows. An unknown peer reads as offline, not online.
            visible: conversationRow.direct && conversationRow.awayPresenceVisible
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            width: conversationRow.style.scaledSize(7)
            height: width
            radius: width / 2
            color: conversationRow.style.presenceMarkColor(
                conversationRow.presence === "away" ? "away"
                    : (conversationRow.presence === "online"
                        ? "online" : "offline"))
            border.width: conversationRow.style.scaledSize(2)
            border.color: conversationRow.style.panelColor
        }
    }

    Row {
        anchors.left: parent.left
        anchors.leftMargin: conversationRow.direct ? conversationRow.style.scaledSize(49)
                                                   : conversationRow.style.scaledSize(20)
        anchors.right: rowTrail.left
        anchors.rightMargin: conversationRow.style.scaledSize(8)
        anchors.verticalCenter: parent.verticalCenter
        spacing: conversationRow.style.scaledSize(4)

        Text {
            objectName: "conversationLabel"
            width: {
                var reserved = (conversationRow.direct && conversationRow.bot
                    ? dmBotMark.width + parent.spacing : 0);
                var cap = Math.max(0, parent.width - reserved);
                return Math.min(implicitWidth, cap);
            }
            text: (conversationRow.direct ? "" : "#  ") + conversationRow.conversationName.replace("#", "")
            color: conversationRow.current
                ? conversationRow.style.inkColor
                : conversationRow.muted || conversationRow.unread === 0
                    ? conversationRow.style.mutedColor
                    : conversationRow.style.inkColor
            elide: Text.ElideRight
            font.family: "iA Writer Mono S"
            font.bold: conversationRow.current
                       || (!conversationRow.muted && conversationRow.unread > 0)
            font.pixelSize: conversationRow.style.scaledSize(13)
        }

        BotMark {
            id: dmBotMark
            style: conversationRow.style
            objectName: "conversation-bot-" + conversationRow.conversationName
            shown: conversationRow.direct && conversationRow.bot
            anchors.verticalCenter: parent.verticalCenter
        }
    }

    Item {
        id: rowTrail
        anchors.right: parent.right
        anchors.rightMargin: conversationRow.style.scaledSize(17)
        anchors.verticalCenter: parent.verticalCenter
        height: conversationRow.style.scaledSize(19)
        width: {
            var badge = unreadBadge.visible ? unreadBadge.width : 0;
            var dots = rowTyping.visible ? rowTyping.implicitWidth : 0;
            var gap = (badge > 0 && dots > 0) ? conversationRow.style.scaledSize(6) : 0;
            return badge + dots + gap;
        }

        TypingDots {
            id: rowTyping
            style: conversationRow.style
            objectName: conversationRow.networkId.length > 0
                ? "conversation-typing-" + conversationRow.networkId
                    + "-" + conversationRow.conversationName
                : "conversation-typing-" + conversationRow.conversationName
            visible: conversationRow.visible
                && conversationRow.direct
                && conversationRow.typing
            anchors.right: unreadBadge.visible ? unreadBadge.left : parent.right
            anchors.rightMargin: unreadBadge.visible ? conversationRow.style.scaledSize(6) : 0
            anchors.verticalCenter: parent.verticalCenter
        }

        Rectangle {
            id: unreadBadge
            visible: conversationRow.unread > 0
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            width: Math.max(conversationRow.style.scaledSize(19),
                            badgeText.implicitWidth + conversationRow.style.scaledSize(10))
            height: conversationRow.style.scaledSize(19)
            radius: height / 2
            color: conversationRow.mention && !conversationRow.muted
                ? conversationRow.style.accentColor : conversationRow.style.raisedColor

            Text {
                id: badgeText
                anchors.centerIn: parent
                text: conversationRow.unread
                color: conversationRow.mention && !conversationRow.muted
                    ? "#ffffff" : conversationRow.style.inkColor
                font.family: "iA Writer Mono S"
                font.bold: true
                font.pixelSize: conversationRow.style.scaledSize(10)
            }
        }
    }

    MouseArea {
        id: rowMouse
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: conversationRow.activated()
    }
}
