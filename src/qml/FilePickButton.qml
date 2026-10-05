import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs

Rectangle {
    id: button

    required property OmaircStyle style

    signal picked(string path)

    function open() {
        dialog.open();
    }

    objectName: "filePickButton"
    Accessible.name: "Insert a file"
    Accessible.role: Accessible.Button
    Accessible.onPressAction: button.open()
    implicitWidth: style.scaledSize(46)
    implicitHeight: style.scaledSize(30)
    width: implicitWidth
    height: implicitHeight
    radius: style.scaledSize(7)
    color: fileMouse.containsMouse
        ? style.mixColors(style.accentColor, style.panelColor, 0.82)
        : style.raisedColor

    Text {
        anchors.centerIn: parent
        text: "FILE"
        color: style.inkColor
        font.family: "iA Writer Mono S"
        font.bold: true
        font.pixelSize: style.scaledSize(9)
    }

    MouseArea {
        id: fileMouse
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: button.open()
    }

    FileDialog {
        id: dialog
        title: "Choose a file"
        fileMode: FileDialog.OpenFile
        onAccepted: button.picked(dialog.selectedFile)
    }
}
