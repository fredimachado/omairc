import QtQuick

Rectangle {
    id: glyph

    required property OmaircStyle style
    property var avatarStore: null
    property bool loadPeerAvatars: true
    property string nick: ""
    property string avatarUrl: ""
    property bool replayed: false
    property bool dimmed: false
    property string initialObjectName: "nickGlyphInitial"
    property color ink: glyph.replayed ? style.mutedColor : style.nickColor(glyph.nick)
    property color fill: style.mixColors(style.pageColor, ink, style.darkMode ? 0.23 : 0.16)
    property int fontPixelSize: style.scaledSize(11)

    radius: width / 2
    color: fill
    opacity: dimmed ? 0.62 : 1
    property int avatarEpoch: 0

    readonly property string storeSource: {
        glyph.avatarEpoch // re-run when avatarStore.ready(rawUrl) fires
        if (!glyph.avatarStore || glyph.avatarUrl.length === 0 || width <= 0)
            return ""
        if (!glyph.loadPeerAvatars)
            return ""
        return glyph.avatarStore.source(glyph.avatarUrl, Math.round(width))
    }

    Image {
        id: photo
        objectName: "nickGlyphPhoto"
        anchors.fill: parent
        visible: status === Image.Ready
        asynchronous: true
        cache: true // avatarEpoch rebinding covers ready(); sourceSize drives decode
        fillMode: Image.PreserveAspectCrop
        source: glyph.storeSource
        sourceSize: Qt.size(Math.round(width), Math.round(height))
    }

    Text {
        objectName: glyph.initialObjectName
        visible: photo.status !== Image.Ready
        anchors.centerIn: parent
        text: glyph.style.initials(glyph.nick)
        color: glyph.ink
        font.family: "iA Writer Mono S"
        font.bold: true
        font.pixelSize: glyph.fontPixelSize
    }

    Connections {
        target: glyph.avatarStore
        function onReady(rawUrl) {
            if (rawUrl !== glyph.avatarUrl)
                return
            glyph.avatarEpoch += 1
        }
    }
}
