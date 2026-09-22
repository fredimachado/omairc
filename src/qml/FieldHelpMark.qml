import QtQuick
import QtQuick.Controls

Item {
    id: helpMark

    required property OmaircStyle style
    property string accessibleName: ""
    property string help: ""
    property string tipObjectName: ""

    readonly property int markSize: style.scaledSize(12)

    visible: help.length > 0
    width: markSize
    height: markSize
    readonly property color restingGlyphColor: style.mixColors(
        style.pageColor, style.inkColor, style.darkMode ? 0.64 : 0.47)
    activeFocusOnTab: visible
    Accessible.role: Accessible.Button
    Accessible.name: accessibleName
    Accessible.description: help
    Accessible.onPressAction: helpMark.forceActiveFocus()

    Rectangle {
        id: helpCircle
        anchors.verticalCenter: parent.verticalCenter
        width: helpMark.markSize
        height: width
        radius: width / 2
        color: "transparent"
        border.width: 1
        border.color: helpMark.activeFocus ? style.accentColor : style.dividerColor
    }

    Text {
        anchors.centerIn: helpCircle
        text: "?"
        color: helpMark.activeFocus ? style.accentColor : helpMark.restingGlyphColor
        // Pixel sizes below the platform minimum clamp in Qt Quick Text.
        // Scale the glyph so the resting mark can sit smaller than that floor.
        font: Qt.font({
            family: "iA Writer Mono S",
            pixelSize: style.scaledSize(9)
        })
        scale: 0.68
        transformOrigin: Item.Center
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
        ToolTip.text: helpMark.help
        ToolTip.delay: 400
        ToolTip.objectName: helpMark.tipObjectName
        onClicked: helpMark.forceActiveFocus()
    }
}
