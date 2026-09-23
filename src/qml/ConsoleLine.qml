import QtQuick

Item {
    id: consoleDelegate

    required property OmaircStyle style
    required property int index
    required property string time
    required property string label
    required property string text
    required property string source
    required property string severity

    property bool findActive: false
    property int findIndex: -1
    readonly property bool findMatch: findActive && findIndex === index
    readonly property var transcriptView: ListView.view

    required property var plainIrcText
    required property var editVisibleText
    required property var httpUrlAt
    required property var inviteChannelAt
    required property var openAllowedUrl
    required property var joinInviteChannel

    signal transcriptSelectionChanged(Item edit)

    width: transcriptView ? transcriptView.width : 0
    height: Math.max(style.scaledSize(22), consoleText.implicitHeight + style.scaledSize(8))

    readonly property color labelColor: consoleDelegate.severity === "alert"
        ? style.accentColor
        : (consoleDelegate.severity === "trace"
            ? style.mutedColor
            : style.mixColors(style.pageColor, style.inkColor, 0.62))
    readonly property color bodyColor: consoleDelegate.severity === "trace"
        ? style.mutedColor
        : style.inkColor
    readonly property string glyph: consoleDelegate.source === "client"
        ? ">>"
        : (consoleDelegate.source === "local" ? "--" : "<<")

    Rectangle {
        objectName: "findMatch"
        anchors.fill: parent
        visible: consoleDelegate.findMatch
        color: consoleDelegate.style.mixColors(
            consoleDelegate.style.pageColor, consoleDelegate.style.selectionColor, 0.42)
    }

    Text {
        anchors.left: parent.left
        anchors.leftMargin: consoleDelegate.style.scaledSize(24)
        anchors.verticalCenter: parent.verticalCenter
        width: consoleDelegate.style.scaledSize(68)
        text: consoleDelegate.time
        color: consoleDelegate.style.mutedColor
        font.family: "iA Writer Mono S"
        font.pixelSize: consoleDelegate.style.scaledSize(10)
    }

    Text {
        anchors.left: parent.left
        anchors.leftMargin: consoleDelegate.style.scaledSize(96)
        anchors.verticalCenter: parent.verticalCenter
        width: consoleDelegate.style.scaledSize(88)
        text: consoleDelegate.glyph + " " + consoleDelegate.label
        color: consoleDelegate.labelColor
        elide: Text.ElideRight
        font.family: "iA Writer Mono S"
        font.pixelSize: consoleDelegate.style.scaledSize(10)
    }

    TextEdit {
        id: consoleText
        objectName: "consoleText"
        anchors.left: parent.left
        anchors.leftMargin: consoleDelegate.style.scaledSize(192)
        anchors.right: parent.right
        anchors.rightMargin: consoleDelegate.style.scaledSize(24)
        anchors.verticalCenter: parent.verticalCenter
        text: plainIrcText(consoleDelegate.text)
        color: consoleDelegate.bodyColor
        selectionColor: consoleDelegate.style.selectionColor
        selectedTextColor: "#ffffff"
        wrapMode: TextEdit.Wrap
        readOnly: true
        selectByMouse: true
        cursorVisible: false
        activeFocusOnPress: false
        activeFocusOnTab: false
        textFormat: TextEdit.PlainText
        padding: 0
        font.family: "iA Writer Mono S"
        font.pixelSize: consoleDelegate.style.scaledSize(12)
        onSelectedTextChanged: consoleDelegate.transcriptSelectionChanged(consoleText)

        PlainUrlHit {
            edit: consoleText
            inviteHits: consoleDelegate.label === "INVITE"
            editVisibleText: function(item) { return consoleDelegate.editVisibleText(item) }
            httpUrlAt: function(text, at) { return consoleDelegate.httpUrlAt(text, at) }
            inviteChannelAt: function(text, at) { return consoleDelegate.inviteChannelAt(text, at) }
            openAllowedUrl: function(url) { return consoleDelegate.openAllowedUrl(url) }
            joinInviteChannel: function(channel) { return consoleDelegate.joinInviteChannel(channel) }
        }
    }
}
