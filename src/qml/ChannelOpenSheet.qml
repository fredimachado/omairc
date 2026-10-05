import QtQuick
import QtQuick.Controls

Popup {
    id: sheet

    required property OmaircStyle style
    property string channel: ""

    signal confirmRequested()

    objectName: "channelOpenSheet"
    width: sheet.style.scaledSize(360)
    padding: sheet.style.scaledSize(16)
    modal: true
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    // An exit fade keeps the modal dimmer up after opened flips to false,
    // so the next click on the channel name never reaches the transcript.
    enter: Transition {}
    exit: Transition {}

    background: Rectangle {
        color: sheet.style.raisedColor
        border.width: 1
        border.color: sheet.style.dividerColor
        radius: sheet.style.scaledSize(9)
    }

    contentItem: Column {
        id: prompt
        focus: true
        spacing: sheet.style.scaledSize(8)
        width: sheet.availableWidth

        Text {
            id: question
            objectName: "channelOpenPrompt"
            width: parent.width
            text: "Open " + sheet.channel + "?"
            color: sheet.style.inkColor
            wrapMode: Text.Wrap
            font.family: "iA Writer Mono S"
            font.pixelSize: sheet.style.scaledSize(14)
            Accessible.name: "Open channel"
        }

        Text {
            width: parent.width
            text: "Enter opens it. Escape cancels."
            color: sheet.style.mutedColor
            wrapMode: Text.Wrap
            font.family: "iA Writer Mono S"
            font.pixelSize: sheet.style.scaledSize(12)
        }

        Keys.onPressed: function(event) {
            if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                sheet.confirmRequested();
                event.accepted = true;
            }
        }
    }

    onOpened: prompt.forceActiveFocus()
}
