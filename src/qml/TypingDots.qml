import QtQuick

Row {
    id: dots

    required property OmaircStyle style
    property color ink: style.mutedColor
    property int pixelSize: style.scaledSize(12)
    property string describedAs: ""
    property int pulse: 0

    Accessible.role: Accessible.StaticText
    Accessible.ignored: dots.describedAs.length === 0
    // Name is for inspection when focus lands; appearance is not a live region.
    Accessible.name: dots.describedAs
    spacing: 0

    Timer {
        interval: 320
        repeat: true
        running: dots.visible
        onTriggered: dots.pulse = (dots.pulse + 1) % 3
    }

    Repeater {
        model: 3
        Text {
            text: "."
            color: dots.ink
            opacity: dots.pulse === index ? 1 : 0.28
            font.family: "iA Writer Mono S"
            font.pixelSize: dots.pixelSize
        }
    }
}
