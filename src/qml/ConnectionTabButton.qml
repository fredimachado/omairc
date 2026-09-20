import QtQuick

Rectangle {
    id: tabButton

    required property OmaircStyle style
    property string tabName
    property string label
    property string glyph: ""
    property bool current: false

    readonly property color tone: current ? tabButton.style.inkColor : tabButton.style.mutedColor

    signal selected(string tabName)
    signal stepRequested(int direction)

    objectName: "connectionSheetTab-" + tabName
    width: tabContent.implicitWidth + tabButton.style.scaledSize(26)
    height: tabButton.style.scaledSize(52)
    color: tabMouse.containsMouse || activeFocus ? tabButton.style.hoverColor : "transparent"
    activeFocusOnTab: true
    Accessible.role: Accessible.PageTab
    Accessible.name: label
    Accessible.selected: current
    Accessible.onPressAction: tabButton.selected(tabName)

    Keys.onShortcutOverride: function(event) {
        var mods = event.modifiers & (Qt.ShiftModifier | Qt.ControlModifier
            | Qt.AltModifier | Qt.MetaModifier);
        if (mods === Qt.AltModifier
                && (event.key === Qt.Key_Left || event.key === Qt.Key_Right)) {
            event.accepted = false;
            return;
        }
    }
    Keys.onPressed: function(event) {
        var mods = event.modifiers & (Qt.ShiftModifier | Qt.ControlModifier
            | Qt.AltModifier | Qt.MetaModifier);
        if (mods === Qt.AltModifier
                && (event.key === Qt.Key_Left || event.key === Qt.Key_Right))
            return;
        if (event.key === Qt.Key_Left || event.key === Qt.Key_Right) {
            stepRequested(event.key === Qt.Key_Right ? 1 : -1);
            event.accepted = true;
            return;
        }
        if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter
                || event.key === Qt.Key_Space) {
            tabButton.selected(tabName);
            event.accepted = true;
        }
    }

    Row {
        id: tabContent
        anchors.centerIn: parent
        spacing: tabButton.style.scaledSize(7)

        Text {
            visible: tabButton.glyph.length > 0
            text: tabButton.glyph
            color: tabButton.tone
            font.family: "iA Writer Mono S"
            font.bold: true
            font.pixelSize: tabButton.style.scaledSize(12)
        }

        Item {
            visible: tabButton.glyph.length === 0
            width: tabButton.style.scaledSize(12)
            height: tabButton.style.scaledSize(15)

            Rectangle {
                x: 0
                y: tabButton.style.scaledSize(4)
                width: parent.width
                height: Math.max(1, tabButton.style.scaledSize(1))
                color: tabButton.tone
            }

            Rectangle {
                x: tabButton.style.scaledSize(7)
                y: tabButton.style.scaledSize(2)
                width: tabButton.style.scaledSize(5)
                height: tabButton.style.scaledSize(5)
                radius: tabButton.style.scaledSize(1)
                color: tabButton.tone
            }

            Rectangle {
                x: 0
                y: tabButton.style.scaledSize(10)
                width: parent.width
                height: Math.max(1, tabButton.style.scaledSize(1))
                color: tabButton.tone
            }

            Rectangle {
                x: tabButton.style.scaledSize(1)
                y: tabButton.style.scaledSize(8)
                width: tabButton.style.scaledSize(5)
                height: tabButton.style.scaledSize(5)
                radius: tabButton.style.scaledSize(1)
                color: tabButton.tone
            }
        }

        Text {
            text: tabButton.label
            color: tabButton.tone
            font.family: "iA Writer Mono S"
            font.pixelSize: tabButton.style.scaledSize(12)
        }
    }

    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: Math.max(1, tabButton.style.scaledSize(2))
        radius: height / 2
        visible: tabButton.current
        color: tabButton.style.accentColor
    }

    MouseArea {
        id: tabMouse
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: {
            tabButton.selected(tabButton.tabName);
            tabButton.forceActiveFocus();
        }
    }
}
