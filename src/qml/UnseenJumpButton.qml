import QtQuick

Rectangle {
    id: jump

    required property OmaircStyle style
    required property TranscriptList list

    visible: jump.list.visible && jump.list.jumpArmed
    width: jump.style.scaledSize(36)
    height: width
    radius: width / 2
    z: 2
    anchors.right: jump.list.right
    anchors.bottom: jump.list.bottom
    anchors.rightMargin: jump.style.scaledSize(16)
    anchors.bottomMargin: jump.style.scaledSize(16)
    color: jumpMouse.containsMouse
        ? jump.style.mixColors(jump.style.raisedColor, jump.style.accentColor,
                               jump.style.darkMode ? 0.35 : 0.22)
        : jump.style.raisedColor
    border.width: 1
    border.color: jump.style.dividerColor

    Accessible.role: Accessible.Button
    Accessible.name: jump.list.hasUnreadMark && jump.list.firstUnseenIndex < 0
        ? "Scroll to latest messages"
        : "Jump to first new message"
    Accessible.onPressAction: jump.list.jumpToUnseen()

    // Finite Animation.running bindings assign false when the loops end,
    // which either drops the binding or starts another burst. Drive restart
    // from the arming edge instead.
    readonly property bool bounceArmed: jump.visible

    onBounceArmedChanged: {
        if (bounceArmed)
            bounceBurst.restart()
        else {
            bounceBurst.stop()
            arrow.bounceOffset = 0
        }
    }

    Text {
        id: arrow
        anchors.centerIn: parent
        anchors.verticalCenterOffset: bounceOffset
        text: "\u2193"
        color: jump.style.inkColor
        font.family: "iA Writer Mono S"
        font.pixelSize: jump.style.scaledSize(16)

        property real bounceOffset: 0

        SequentialAnimation {
            id: bounceBurst
            objectName: "bounceBurst"
            loops: Animation.Infinite
            onStopped: arrow.bounceOffset = 0

            NumberAnimation {
                target: arrow
                property: "bounceOffset"
                from: 0
                to: jump.style.scaledSize(3)
                duration: 700
                easing.type: Easing.InOutSine
            }
            NumberAnimation {
                target: arrow
                property: "bounceOffset"
                from: jump.style.scaledSize(3)
                to: 0
                duration: 700
                easing.type: Easing.InOutSine
            }
        }
    }

    Rectangle {
        objectName: "unseenJumpDot"
        visible: jump.visible
        width: jump.style.scaledSize(8)
        height: width
        radius: width / 2
        color: jump.style.accentColor
        anchors.top: parent.top
        anchors.right: parent.right
        anchors.topMargin: jump.style.scaledSize(2)
        anchors.rightMargin: jump.style.scaledSize(2)
    }

    MouseArea {
        id: jumpMouse
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: jump.list.jumpToUnseen()
    }
}
