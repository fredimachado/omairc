import QtQuick

Item {
    id: unreadMark

    objectName: "unreadMark"

    required property OmaircStyle style

    // Accent mixed into ink/page so the rule reads as a boundary on both
    // Omarchy light and dark themes, not as a muted join/part caption.
    readonly property color markColor: style.mixColors(
        style.inkColor, style.accentColor, style.darkMode ? 0.55 : 0.42)
    readonly property color lineColor: style.mixColors(
        style.pageColor, style.accentColor, style.darkMode ? 0.48 : 0.36)

    implicitHeight: label.implicitHeight + style.scaledSize(16)

    Rectangle {
        anchors.left: parent.left
        anchors.leftMargin: style.scaledSize(24)
        anchors.right: label.left
        anchors.rightMargin: style.scaledSize(10)
        anchors.verticalCenter: parent.verticalCenter
        height: Math.max(1, style.scaledSize(1))
        color: unreadMark.lineColor
    }

    Text {
        id: label
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.verticalCenter: parent.verticalCenter
        text: "New messages"
        color: unreadMark.markColor
        font.family: "iA Writer Mono S"
        font.pixelSize: style.scaledSize(10)
    }

    Rectangle {
        anchors.left: label.right
        anchors.leftMargin: style.scaledSize(10)
        anchors.right: parent.right
        anchors.rightMargin: style.scaledSize(24)
        anchors.verticalCenter: parent.verticalCenter
        height: Math.max(1, style.scaledSize(1))
        color: unreadMark.lineColor
    }
}
