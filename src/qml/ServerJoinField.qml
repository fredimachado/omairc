import QtQuick
import QtQuick.Controls

// Join a channel that is not already a sidebar row, from the server buffer.
Row {
    id: field

    required property OmaircStyle style
    property bool joinEnabled: true

    signal submitted(string channel)

    spacing: field.style.scaledSize(8)

    function submit() {
        var text = input.text.trim();
        if (text.length === 0)
            return;
        field.submitted(text);
        input.text = "";
    }

    Rectangle {
        width: field.style.scaledSize(160)
        height: field.style.scaledSize(30)
        radius: field.style.scaledSize(7)
        color: field.style.panelColor
        border.width: 1
        border.color: input.activeFocus ? field.style.accentColor : field.style.dividerColor

        TextField {
            id: input
            objectName: "serverJoinField"
            anchors.fill: parent
            enabled: field.joinEnabled
            placeholderText: "Channel"
            color: field.style.inkColor
            placeholderTextColor: field.style.mutedColor
            selectionColor: field.style.selectionColor
            selectedTextColor: "#ffffff"
            font.family: "iA Writer Mono S"
            font.pixelSize: field.style.scaledSize(12)
            leftPadding: field.style.scaledSize(10)
            rightPadding: field.style.scaledSize(10)
            verticalAlignment: TextInput.AlignVCenter
            Accessible.name: "Join channel"
            background: Item {}
            onAccepted: field.submit()
        }
    }

    Rectangle {
        objectName: "serverJoinButton"
        Accessible.name: "Join channel"
        Accessible.role: Accessible.Button
        Accessible.onPressAction: field.submit()
        width: field.style.scaledSize(58)
        height: field.style.scaledSize(30)
        radius: field.style.scaledSize(7)
        color: joinMouse.containsMouse ? field.style.raisedColor : "transparent"
        border.width: 1
        border.color: field.style.dividerColor

        Text {
            anchors.centerIn: parent
            text: "JOIN"
            color: field.style.inkColor
            font.family: "iA Writer Mono S"
            font.bold: true
            font.pixelSize: field.style.scaledSize(9)
        }

        MouseArea {
            id: joinMouse
            anchors.fill: parent
            hoverEnabled: true
            enabled: field.joinEnabled
            cursorShape: Qt.PointingHandCursor
            onClicked: field.submit()
        }
    }
}
