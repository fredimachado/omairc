import QtQuick

Item {
    id: botMark

    required property OmaircStyle style
    property bool shown: false

    visible: shown
    width: shown ? style.scaledSize(10) : 0
    height: style.scaledSize(10)

    Rectangle {
        anchors.horizontalCenter: parent.horizontalCenter
        y: 0
        width: Math.max(1, style.scaledSize(1))
        height: style.scaledSize(3)
        color: style.mutedColor
    }

    Rectangle {
        anchors.horizontalCenter: parent.horizontalCenter
        y: 0
        width: style.scaledSize(3)
        height: Math.max(1, style.scaledSize(1))
        radius: width / 2
        color: style.mutedColor
    }

    Rectangle {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        width: style.scaledSize(8)
        height: style.scaledSize(6)
        radius: style.scaledSize(1)
        color: "transparent"
        border.color: style.mutedColor
        border.width: Math.max(1, style.scaledSize(1))
    }
}
