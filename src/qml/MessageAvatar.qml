import QtQuick

Item {
    id: avatar

    required property OmaircStyle style
    required property string author
    required property bool replayed
    property var avatarStore: null
    property bool loadPeerAvatars: true
    property string selfNick: ""
    property bool nickOpensDirect: false
    property string avatarUrl: ""

    signal directMessageRequested(string nick)

    anchors.left: parent.left
    anchors.leftMargin: style.scaledSize(24)
    anchors.top: parent.top
    anchors.topMargin: style.scaledSize(9)
    width: style.scaledSize(34)
    height: width

    NickGlyph {
        anchors.fill: parent
        style: avatar.style
        avatarStore: avatar.avatarStore
        loadPeerAvatars: avatar.loadPeerAvatars
        nick: avatar.author
        avatarUrl: avatar.avatarUrl
        replayed: avatar.replayed
        initialObjectName: "messageAvatarInitial"
        fontPixelSize: avatar.style.scaledSize(13)
    }

    TranscriptNickHit {
        nick: avatar.author
        nickOpensDirect: avatar.nickOpensDirect
        selfNick: avatar.selfNick
        onDirectMessageRequested: avatar.directMessageRequested(nick)
    }
}
