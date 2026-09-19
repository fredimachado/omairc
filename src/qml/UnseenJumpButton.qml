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
    Accessible.name: "Jump to first new message"
    Accessible.onPressAction: jump.list.jumpToUnseen()

    Text {
        anchors.centerIn: parent
        text: "\u2193"
        color: jump.style.inkColor
        font.family: "iA Writer Mono S"
        font.pixelSize: jump.style.scaledSize(16)
    }

    MouseArea {
        id: jumpMouse
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: jump.list.jumpToUnseen()
    }
}
