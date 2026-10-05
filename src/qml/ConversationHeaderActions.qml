import QtQuick

// Leave, Join, and Close for the conversation header. A joined channel
// offers Leave. A channel you have left offers Join and Close. A direct
// message offers Close.
Row {
    id: actions

    required property OmaircStyle style
    property bool channel: false
    property bool joined: false
    property bool canClose: false

    // Size and visibility come from these flags. Reading a child's visible
    // to decide the row's visible breaks the child's binding.
    readonly property bool showLeave: actions.channel && actions.joined
    readonly property bool showJoin: actions.channel && !actions.joined
    readonly property bool showClose: actions.canClose

    signal leaveRequested()
    signal joinRequested()
    signal closeRequested()

    spacing: actions.style.scaledSize(8)
    visible: actions.showLeave || actions.showJoin || actions.showClose

    function press(kind) {
        if (kind === "leave")
            actions.leaveRequested();
        else if (kind === "join")
            actions.joinRequested();
        else if (kind === "close")
            actions.closeRequested();
    }

    Rectangle {
        id: leaveButton
        objectName: "headerLeaveButton"
        visible: actions.showLeave
        width: actions.showLeave ? actions.style.scaledSize(58) : 0
        height: actions.style.scaledSize(30)
        radius: actions.style.scaledSize(7)
        color: leaveMouse.containsMouse ? actions.style.raisedColor : "transparent"
        border.width: 1
        border.color: leaveMouse.containsMouse ? actions.style.dividerColor : "transparent"
        Accessible.name: "Leave channel"
        Accessible.role: Accessible.Button
        Accessible.onPressAction: actions.press("leave")

        Text {
            anchors.centerIn: parent
            text: "LEAVE"
            color: actions.style.inkColor
            font.family: "iA Writer Mono S"
            font.bold: true
            font.pixelSize: actions.style.scaledSize(9)
        }

        MouseArea {
            id: leaveMouse
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: actions.press("leave")
        }
    }

    Rectangle {
        id: joinButton
        objectName: "headerJoinButton"
        visible: actions.showJoin
        width: actions.showJoin ? actions.style.scaledSize(58) : 0
        height: actions.style.scaledSize(30)
        radius: actions.style.scaledSize(7)
        color: joinMouse.containsMouse ? actions.style.raisedColor : "transparent"
        border.width: 1
        border.color: joinMouse.containsMouse ? actions.style.dividerColor : "transparent"
        Accessible.name: "Join channel"
        Accessible.role: Accessible.Button
        Accessible.onPressAction: actions.press("join")

        Text {
            anchors.centerIn: parent
            text: "JOIN"
            color: actions.style.inkColor
            font.family: "iA Writer Mono S"
            font.bold: true
            font.pixelSize: actions.style.scaledSize(9)
        }

        MouseArea {
            id: joinMouse
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: actions.press("join")
        }
    }

    Rectangle {
        id: closeButton
        objectName: "headerCloseButton"
        visible: actions.showClose
        width: actions.showClose ? actions.style.scaledSize(64) : 0
        height: actions.style.scaledSize(30)
        radius: actions.style.scaledSize(7)
        color: closeMouse.containsMouse ? actions.style.raisedColor : "transparent"
        border.width: 1
        border.color: closeMouse.containsMouse ? actions.style.dividerColor : "transparent"
        Accessible.name: "Close conversation"
        Accessible.role: Accessible.Button
        Accessible.onPressAction: actions.press("close")

        Text {
            anchors.centerIn: parent
            text: "CLOSE"
            color: actions.style.inkColor
            font.family: "iA Writer Mono S"
            font.bold: true
            font.pixelSize: actions.style.scaledSize(9)
        }

        MouseArea {
            id: closeMouse
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: actions.press("close")
        }
    }
}
