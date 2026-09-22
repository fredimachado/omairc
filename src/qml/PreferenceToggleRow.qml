import QtQuick
import QtQuick.Controls

Row {
    id: row

    required property OmaircStyle style
    property string label: ""
    property string help: ""
    property string switchObjectName: ""
    property bool checked: false

    signal toggled(bool checked)
    signal applyKeyRequested(var event)

    spacing: style.scaledSize(10)

    Switch {
        id: toggle
        objectName: row.switchObjectName
        Accessible.name: row.label
        Accessible.description: row.help
        checked: row.checked
        Keys.onPressed: function(event) {
            row.applyKeyRequested(event)
        }
        onToggled: row.toggled(checked)
    }

    Row {
        spacing: row.style.scaledSize(6)
        height: toggle.height

        Text {
            height: parent.height
            text: row.label
            color: row.style.inkColor
            verticalAlignment: Text.AlignVCenter
            font.family: "iA Writer Mono S"
            font.pixelSize: row.style.scaledSize(11)
        }

        FieldHelpMark {
            objectName: row.switchObjectName + "Help"
            style: row.style
            accessibleName: row.label + " help"
            help: row.help
            tipObjectName: row.switchObjectName + "HelpTip"
            height: parent.height
        }
    }
}
