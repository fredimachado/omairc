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
    signal networkWalkRequested(int direction)
    signal aboutRequested()

    function aboutShortcutModifiers(event) {
        return event.modifiers & (Qt.ShiftModifier | Qt.ControlModifier
            | Qt.AltModifier | Qt.MetaModifier);
    }

    function handleAboutShortcut(event) {
        if (event.key !== Qt.Key_Slash)
            return false;
        var mods = field.aboutShortcutModifiers(event);
        if (mods === (Qt.ControlModifier | Qt.ShiftModifier)
                || ((Qt.platform.os === "osx" || Qt.platform.os === "macos")
                    && mods === (Qt.MetaModifier | Qt.ShiftModifier))) {
            event.accepted = true;
            field.aboutRequested();
            return true;
        }
        return false;
    }

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

        FieldHelpMark {
            id: helpMark
            objectName: field.fieldObjectName + "Help"
            style: field.style
            accessibleName: field.label + " help"
            help: field.help
            tipObjectName: field.fieldObjectName + "HelpTip"
            height: labelRow.height
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
            Keys.onShortcutOverride: function(event) {
                if (field.handleAboutShortcut(event))
                    return;
                var mods = event.modifiers & (Qt.ShiftModifier | Qt.ControlModifier
                    | Qt.AltModifier | Qt.MetaModifier);
                if (mods === Qt.AltModifier
                        && (event.key === Qt.Key_Left || event.key === Qt.Key_Right))
                    event.accepted = true;
            }
            Keys.onPressed: function(event) {
                if (field.handleAboutShortcut(event))
                    return;
                if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                    field.applyKeyRequested(event);
                    return;
                }
                var mods = event.modifiers & (Qt.ShiftModifier | Qt.ControlModifier
                    | Qt.AltModifier | Qt.MetaModifier);
                if (mods === Qt.AltModifier
                        && (event.key === Qt.Key_Left || event.key === Qt.Key_Right)) {
                    event.accepted = true;
                    field.networkWalkRequested(event.key === Qt.Key_Right ? 1 : -1);
                }
            }
        }
    }
}
