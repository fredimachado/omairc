import QtQuick
import QtQuick.Controls

Column {
    id: field

    required property OmaircStyle style
    property string label
    property string badge: ""
    property string help: ""
    property alias fieldObjectName: input.objectName
    property alias text: input.text
    property bool secret: false

    signal textEdited(string text)
    signal applyKeyRequested(var event)

    function focusInput() {
        input.forceActiveFocus();
    }

    width: parent ? parent.width : 0
    spacing: field.style.scaledSize(4)

    Row {
        id: labelRow
        height: field.style.scaledSize(15)
        spacing: field.style.scaledSize(6)

        Text {
            height: labelRow.height
            text: field.label
            color: field.style.mutedColor
            verticalAlignment: Text.AlignVCenter
            font.family: "iA Writer Mono S"
            font.pixelSize: field.style.scaledSize(10)
        }

        Rectangle {
            id: badgeChip
            objectName: field.fieldObjectName + "Badge"
            visible: field.badge.length > 0
            width: visible ? badgeChipLabel.implicitWidth + field.style.scaledSize(10) : 0
            height: labelRow.height
            radius: field.style.scaledSize(4)
            color: field.style.panelColor
            border.width: 1
            border.color: field.style.dividerColor

            Text {
                id: badgeChipLabel
                anchors.centerIn: parent
                text: field.badge
                color: field.style.mutedColor
                font.family: "iA Writer Mono S"
                font.pixelSize: field.style.scaledSize(9)
            }
        }

        Item {
            id: helpMark
            objectName: field.fieldObjectName + "Help"
            visible: field.help.length > 0
            width: field.style.scaledSize(14)
            height: labelRow.height
            activeFocusOnTab: visible
            Accessible.role: Accessible.Button
            Accessible.name: field.label + " help"
            Accessible.description: field.help
            Accessible.onPressAction: helpMark.forceActiveFocus()

            Rectangle {
                id: helpCircle
                anchors.verticalCenter: parent.verticalCenter
                width: field.style.scaledSize(14)
                height: width
                radius: width / 2
                color: "transparent"
                border.width: 1
                border.color: helpMark.activeFocus ? field.style.accentColor : field.style.dividerColor
            }

            Text {
                anchors.centerIn: helpCircle
                text: "?"
                color: helpMark.activeFocus ? field.style.inkColor : field.style.mutedColor
                font.family: "iA Writer Mono S"
                font.pixelSize: field.style.scaledSize(9)
            }

            MouseArea {
                id: helpMouse
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.WhatsThisCursor
                // Fully declarative: mixing ToolTip.show()/hide() with a
                // `visible` binding writes underneath the binding and can
                // desync it. Shown on hover and on keyboard focus so the
                // help is readable without a mouse.
                ToolTip.visible: containsMouse || helpMark.activeFocus
                ToolTip.text: field.help
                ToolTip.delay: 400
                ToolTip.objectName: field.fieldObjectName + "HelpTip"
                onClicked: helpMark.forceActiveFocus()
            }
        }
    }

    Rectangle {
        width: parent.width
        height: field.style.scaledSize(36)
        radius: field.style.scaledSize(7)
        color: field.style.panelColor
        border.width: 1
        border.color: input.activeFocus ? field.style.accentColor : field.style.dividerColor

        TextField {
            id: input
            anchors.fill: parent
            echoMode: field.secret ? TextInput.Password : TextInput.Normal
            color: field.style.inkColor
            selectionColor: field.style.selectionColor
            selectedTextColor: "#ffffff"
            font.family: "iA Writer Mono S"
            font.pixelSize: field.style.scaledSize(12)
            leftPadding: field.style.scaledSize(10)
            rightPadding: field.style.scaledSize(10)
            verticalAlignment: TextInput.AlignVCenter
            Accessible.description: field.help
            background: Item {}
            onTextEdited: field.textEdited(text)
            Keys.onPressed: function(event) {
                if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter)
                    field.applyKeyRequested(event);
            }
        }
    }
}
