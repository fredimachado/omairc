import QtQuick

MouseArea {
    required property string nick
    property bool nickOpensDirect: false
    property string selfNick: ""

    signal directMessageRequested(string nick)

    objectName: "transcriptNickHit"
    anchors.fill: parent
    enabled: nickOpensDirect && nick.length > 0 && nick !== selfNick
    hoverEnabled: true
    cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
    Accessible.role: Accessible.Button
    Accessible.name: nick
    Accessible.ignored: !enabled
    Accessible.onPressAction: {
        if (enabled)
            directMessageRequested(nick);
    }
    onClicked: directMessageRequested(nick)
}
