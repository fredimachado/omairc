import QtQuick

Column {
    id: section

    required property OmaircStyle style
    property string networkId
    property string displayName
    property int iconColor: -1
    property string iconUrl: ""
    property string statusText: ""
    property int alerts: 0
    property int unread: 0
    property bool mention: false
    property bool showEdit: false
    property bool headerFocused: false
    property var irc: null
    property var networkConsole: null
    property var avatarStore: null

    signal statusRequested(string networkId)
    signal editRequested(string networkId)

    width: parent ? parent.width : 0
    spacing: 0

    readonly property string liveStatus: {
        var pulse = section.irc ? section.irc.connectionStatus : "";
        if (!section.irc)
            return section.statusText;
        var error = section.irc.lastErrorFor ? section.irc.lastErrorFor(section.networkId) : "";
        if (error && error.length > 0)
            return error;
        return section.irc.connectionStatusFor
            ? section.irc.connectionStatusFor(section.networkId)
            : pulse;
    }
    readonly property int liveUnread: {
        var epoch = section.irc ? section.irc.conversationEpoch : 0;
        var pulse = section.irc ? section.irc.connectionStatus : "";
        if (!section.irc)
            return section.unread;
        return section.irc.unreadCountFor ? section.irc.unreadCountFor(section.networkId) : 0;
    }
    readonly property bool liveMention: {
        var epoch = section.irc ? section.irc.conversationEpoch : 0;
        var pulse = section.irc ? section.irc.connectionStatus : "";
        if (!section.irc)
            return section.mention;
        return section.irc.mentionFor ? section.irc.mentionFor(section.networkId) : false;
    }
    readonly property int liveAlerts: {
        var pulse = section.networkConsole ? section.networkConsole.alerts : 0;
        if (!section.networkConsole)
            return section.alerts;
        return section.networkConsole.alertsFor
            ? section.networkConsole.alertsFor(section.networkId)
            : pulse;
    }

    Item {
        id: networkHeader
        objectName: "networkHeader-" + section.networkId
        width: parent.width
        height: section.style.scaledSize(64)

        Rectangle {
            anchors.fill: parent
            anchors.leftMargin: section.style.scaledSize(8)
            anchors.rightMargin: section.style.scaledSize(8)
            radius: section.style.scaledSize(7)
            color: section.headerFocused
                ? section.style.raisedColor
                : networkHeaderButton.containsMouse ? section.style.hoverColor : "transparent"
        }

        Rectangle {
            visible: section.headerFocused
            anchors.left: parent.left
            anchors.leftMargin: section.style.scaledSize(8)
            anchors.verticalCenter: parent.verticalCenter
            width: section.style.scaledSize(3)
            height: section.style.scaledSize(18)
            radius: width
            color: section.style.accentColor
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
            onClicked: section.statusRequested(section.networkId)
        }

        Rectangle {
            id: networkIconRect
            objectName: "networkIcon-" + section.networkId
            anchors.left: parent.left
            anchors.leftMargin: section.style.scaledSize(18)
            anchors.verticalCenter: parent.verticalCenter
            width: section.style.scaledSize(28)
            height: width
            radius: section.style.scaledSize(8)
            color: section.iconColor >= 0 ? section.style.paletteColor(section.iconColor)
                                          : section.style.accentColor
            property int avatarEpoch: 0

            readonly property string storeSource: {
                networkIconRect.avatarEpoch
                if (!section.avatarStore || section.iconUrl.length === 0 || width <= 0)
                    return ""
                return section.avatarStore.source(section.iconUrl, Math.round(width),
                                                  "square")
            }

            Image {
                id: networkIconPhoto
                objectName: "networkIconPhoto-" + section.networkId
                anchors.fill: parent
                visible: status === Image.Ready
                asynchronous: true
                cache: true
                fillMode: Image.PreserveAspectCrop
                source: networkIconRect.storeSource
                sourceSize: Qt.size(Math.round(width), Math.round(height))
            }

            Text {
                objectName: "networkIconInitial-" + section.networkId
                visible: networkIconPhoto.status !== Image.Ready
                anchors.centerIn: parent
                text: section.displayName.length > 0
                    ? section.displayName.charAt(0).toUpperCase()
                    : "?"
                color: "#ffffff"
                font.family: "iA Writer Mono S"
                font.bold: true
                font.pixelSize: section.style.scaledSize(14)
            }

            Connections {
                target: section.avatarStore
                function onReady(rawUrl) {
                    if (rawUrl !== section.iconUrl)
                        return
                    networkIconRect.avatarEpoch += 1
                }
            }
        }

        Item {
            id: networkEditButton
            objectName: "networkEditButton-" + section.networkId
            z: 2
            visible: section.showEdit
            anchors.right: parent.right
            anchors.rightMargin: section.style.scaledSize(10)
            anchors.verticalCenter: parent.verticalCenter
            width: visible ? section.style.scaledSize(28) : 0
            height: section.style.scaledSize(28)
            Accessible.name: "Edit connection"
            Accessible.role: Accessible.Button
            Accessible.onPressAction: section.editRequested(section.networkId)

            Text {
                anchors.centerIn: parent
                text: "edit"
                color: editMouse.containsMouse ? section.style.inkColor : section.style.mutedColor
                font.family: "iA Writer Mono S"
                font.pixelSize: section.style.scaledSize(9)
            }

            MouseArea {
                id: editMouse
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: section.editRequested(section.networkId)
            }
        }

        Column {
            anchors.left: parent.left
            anchors.leftMargin: section.style.scaledSize(56)
            anchors.right: networkEditButton.left
            anchors.rightMargin: section.style.scaledSize(8)
            anchors.verticalCenter: parent.verticalCenter
            spacing: section.style.scaledSize(2)

            Row {
                width: parent.width
                spacing: section.style.scaledSize(6)

                Text {
                    width: Math.max(0, parent.width - (unreadMark.visible ? unreadMark.width + parent.spacing : 0))
                    text: section.displayName
                    color: section.style.inkColor
                    elide: Text.ElideRight
                    font.family: "iA Writer Mono S"
                    font.bold: true
                    font.pixelSize: section.style.scaledSize(13)
                }

                Rectangle {
                    id: unreadMark
                    objectName: "networkUnreadMark-" + section.networkId
                    visible: section.liveUnread > 0 || section.liveMention
                    anchors.verticalCenter: parent.verticalCenter
                    width: section.style.scaledSize(7)
                    height: width
                    radius: width / 2
                    color: section.liveMention ? section.style.accentColor : section.style.inkColor
                }
            }

            Row {
                width: parent.width
                spacing: section.style.scaledSize(6)

                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    width: section.style.scaledSize(7)
                    height: width
                    radius: width / 2
                    color: section.liveAlerts > 0
                        ? section.style.accentColor
                        : section.style.presenceMarkColor(section.liveStatus === "Connected"
                            ? "online" : "offline")
                }

                Text {
                    width: Math.max(0, parent.width - section.style.scaledSize(13))
                    text: section.liveStatus
                    color: section.style.mutedColor
                    elide: Text.ElideRight
                    font.family: "iA Writer Mono S"
                    font.pixelSize: section.style.scaledSize(10)
                }
            }
        }
    }
}
