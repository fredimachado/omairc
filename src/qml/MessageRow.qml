import QtQuick

Item {
    id: messageDelegate

    required property OmaircStyle style
    required property int index
    required property string author
    required property string time
    required property string body
    required property string kind
    required property var model
    required property var plainIrcText
    required property var emphasizedIrcText
    required property var hasIrcEmphasis
    required property var transcriptField
    required property var continuesMessageGroup
    required property var transcriptRowHeight
    required property var bodyTextTopMargin
    required property font transcriptBodyFont
    required property var editVisibleText
    required property var httpUrlAt
    required property var inviteChannelAt
    required property var openAllowedUrl
    required property var joinInviteChannel
    property var avatarStore: null
    property bool peerAvatarsEnabled: true
    property string selfNick: ""
    property bool findActive: false
    property int findIndex: -1

    signal directMessageRequested(string nick)

    // Read roles through model so MessageListModel dataChanged
    // refreshes them. Keep them optional so ListModel fixtures
    // without avatar/bot roles still instantiate.
    readonly property string authorAvatar: model && model.authorAvatar
        ? String(model.authorAvatar) : ""
    readonly property bool authorBot: !!(model && model.authorBot)
    readonly property string authorAccount: model && model.authorAccount
        ? String(model.authorAccount) : ""
    // Wash: nick or /highlight hit. Not the sidebar `mention` badge.
    readonly property bool mentioned: !!(model && model.mentioned)
    // The view is null only outside a TranscriptList. Width, grouping,
    // and the whois stack margin all read that list, not a captured column.
    readonly property var transcriptView: ListView.view
    readonly property string origin: transcriptView
        ? transcriptField(transcriptView.model, index, "origin")
        : ""
    readonly property bool replayed: origin === "replay"
    readonly property bool isChat: kind !== "event" && kind !== "whois" && kind !== "unread"
    readonly property bool grouped: transcriptView
        ? continuesMessageGroup(
            transcriptView.model, index, author, time, kind, origin)
        : false
    readonly property bool findMatch: findActive && findIndex === index

    width: transcriptView ? transcriptView.width : 0
    height: transcriptRowHeight(
        kind === "event" || kind === "unread", grouped || kind === "whois",
        kind === "event"
            ? messageEvent.implicitHeight
            : (kind === "unread"
                ? unreadMark.implicitHeight
                : (kind === "whois"
                    ? messageWhois.implicitHeight
                    : messageBody.implicitHeight)))

    MentionWash {
        style: messageDelegate.style
        shown: messageDelegate.mentioned
        anchors.fill: parent
    }

    Rectangle {
        objectName: "findMatch"
        anchors.fill: parent
        visible: messageDelegate.findMatch
        color: messageDelegate.style.mixColors(
            messageDelegate.style.pageColor, messageDelegate.style.selectionColor, 0.42)
    }

    UnreadMark {
        id: unreadMark
        visible: messageDelegate.kind === "unread"
        style: messageDelegate.style
        width: parent.width
        y: Math.round((parent.height - implicitHeight) / 2)
    }

    Text {
        id: messageEvent
        objectName: "messageEvent"
        visible: messageDelegate.kind === "event"
        x: messageDelegate.style.scaledSize(24)
        y: Math.round((parent.height - implicitHeight) / 2)
        width: parent.width - messageDelegate.style.scaledSize(48)
        horizontalAlignment: Text.AlignHCenter
        text: plainIrcText(messageDelegate.body)
        textFormat: Text.PlainText
        color: messageDelegate.style.mutedColor
        wrapMode: Text.Wrap
        font.family: "iA Writer Mono S"
        font.pixelSize: messageDelegate.style.scaledSize(10)
    }

    TextEdit {
        id: messageWhois
        objectName: "messageWhois"
        visible: messageDelegate.kind === "whois"
        anchors.left: parent.left
        anchors.leftMargin: messageDelegate.style.scaledSize(70)
        anchors.right: parent.right
        anchors.rightMargin: messageDelegate.style.scaledSize(34)
        anchors.top: parent.top
        anchors.topMargin: {
            var previous = messageDelegate.transcriptView
                ? messageDelegate.transcriptField(
                    messageDelegate.transcriptView.model,
                    messageDelegate.index - 1, "kind")
                : "";
            return previous === "whois"
                ? messageDelegate.style.scaledSize(4)
                : messageDelegate.style.scaledSize(8);
        }
        horizontalAlignment: Text.AlignLeft
        text: plainIrcText(messageDelegate.body)
        textFormat: TextEdit.PlainText
        color: messageDelegate.style.mutedColor
        wrapMode: TextEdit.Wrap
        readOnly: true
        selectByMouse: true
        selectionColor: messageDelegate.style.selectionColor
        selectedTextColor: "#ffffff"
        cursorVisible: false
        activeFocusOnPress: false
        activeFocusOnTab: false
        padding: 0
        font.family: "iA Writer Mono S"
        font.pixelSize: messageDelegate.style.scaledSize(12)

        PlainUrlHit {
            edit: messageWhois
            editVisibleText: function(item) { return messageDelegate.editVisibleText(item) }
            httpUrlAt: function(text, at) { return messageDelegate.httpUrlAt(text, at) }
            inviteChannelAt: function(text, at) { return messageDelegate.inviteChannelAt(text, at) }
            openAllowedUrl: function(url) { return messageDelegate.openAllowedUrl(url) }
            joinInviteChannel: function(channel) { return messageDelegate.joinInviteChannel(channel) }
        }
    }

    MessageAvatar {
        objectName: "messageAvatar"
        visible: messageDelegate.isChat && !messageDelegate.grouped
        style: messageDelegate.style
        avatarStore: messageDelegate.avatarStore
        loadPeerAvatars: messageDelegate.peerAvatarsEnabled
        selfNick: messageDelegate.selfNick
        author: messageDelegate.author
        avatarUrl: messageDelegate.authorAvatar
        replayed: messageDelegate.replayed
        nickOpensDirect: true
        onDirectMessageRequested: function(nick) { messageDelegate.directMessageRequested(nick) }
    }

    MessageHeader {
        objectName: "messageHeader"
        visible: messageDelegate.isChat && !messageDelegate.grouped
        style: messageDelegate.style
        selfNick: messageDelegate.selfNick
        author: messageDelegate.author
        time: messageDelegate.time
        replayed: messageDelegate.replayed
        nickOpensDirect: true
        bot: messageDelegate.authorBot
        account: messageDelegate.authorAccount
        onDirectMessageRequested: function(nick) { messageDelegate.directMessageRequested(nick) }
    }

    TextEdit {
        id: messageBody
        objectName: "messageBody"
        visible: messageDelegate.isChat
        anchors.left: parent.left
        anchors.leftMargin: messageDelegate.style.scaledSize(70)
        anchors.right: parent.right
        anchors.rightMargin: messageDelegate.style.scaledSize(34)
        anchors.top: parent.top
        anchors.topMargin: bodyTextTopMargin(messageDelegate.grouped)
        text: hasIrcEmphasis(messageDelegate.body)
            ? emphasizedIrcText(messageDelegate.body)
            : plainIrcText(messageDelegate.body)
        color: (messageDelegate.replayed || messageDelegate.kind === "action")
            ? messageDelegate.style.mutedColor : messageDelegate.style.inkColor
        selectionColor: messageDelegate.style.selectionColor
        selectedTextColor: "#ffffff"
        wrapMode: TextEdit.Wrap
        readOnly: true
        selectByMouse: true
        cursorVisible: false
        activeFocusOnPress: false
        activeFocusOnTab: false
        textFormat: hasIrcEmphasis(messageDelegate.body)
            ? TextEdit.RichText
            : TextEdit.PlainText
        padding: 0
        font.family: transcriptBodyFont.family
        font.italic: messageDelegate.kind === "action"
        font.pixelSize: transcriptBodyFont.pixelSize

        PlainUrlHit {
            edit: messageBody
            editVisibleText: function(item) { return messageDelegate.editVisibleText(item) }
            httpUrlAt: function(text, at) { return messageDelegate.httpUrlAt(text, at) }
            inviteChannelAt: function(text, at) { return messageDelegate.inviteChannelAt(text, at) }
            openAllowedUrl: function(url) { return messageDelegate.openAllowedUrl(url) }
            joinInviteChannel: function(channel) { return messageDelegate.joinInviteChannel(channel) }
        }
    }
}
