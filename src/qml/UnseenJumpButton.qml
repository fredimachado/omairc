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

    // Five finite loops. `running` stays unbound. `stop()` writes
    // `running = false` from C++, and a `running:` binding would be
    // dropped by that assignment. Showing or hiding the chip does not
    // start the burst. A detached bottom arrival does.
    Connections {
        target: jump.list
        function onDetachedArrival() {
            // jumpArmed can still be false in this same turn.
            Qt.callLater(function() {
                if (jump.visible)
                    bounceBurst.restart()
            })
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
            loops: 5
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

        Connections {
            target: jump
            function onVisibleChanged() {
                if (jump.visible)
                    return
                bounceBurst.stop()
                arrow.bounceOffset = 0
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
