import QtQuick

Item {
    id: mentionWash

    required property OmaircStyle style
    property bool shown: false

    objectName: "mentionWash"
    visible: shown

    readonly property color washColor: mentionWash.style.mixColors(
        mentionWash.style.pageColor, mentionWash.style.accentColor,
        mentionWash.style.darkMode ? 0.09 : 0.06)
    readonly property color barColor: mentionWash.style.mixColors(
        mentionWash.style.pageColor, mentionWash.style.accentColor,
        mentionWash.style.darkMode ? 0.44 : 0.34)

    Rectangle {
        anchors.fill: parent
        color: mentionWash.washColor
    }

    Rectangle {
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: mentionWash.style.scaledSize(2)
        color: mentionWash.barColor
    }
}
