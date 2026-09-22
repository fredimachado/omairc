import QtQuick

Row {
    id: header

    required property OmaircStyle style
    required property string author
    required property string time
    required property bool replayed
    property string selfNick: ""
    property bool nickOpensDirect: false
    property bool bot: false
    property string account: ""

    signal directMessageRequested(string nick)

    width: parent.width
    anchors.left: parent.left
    anchors.leftMargin: style.scaledSize(70)
    anchors.top: parent.top
    anchors.topMargin: style.scaledSize(8)
    spacing: style.scaledSize(9)

    Row {
        id: authorRow
        width: Math.max(0, header.width - messageTime.implicitWidth - header.spacing)
        spacing: (header.bot || header.account.length > 0)
            ? header.style.scaledSize(4) : 0

        Text {
            id: authorLabel
            objectName: "messageAuthor"
            width: {
                var reserved = (header.bot ? messageBotMark.width + parent.spacing : 0)
                    + (authorAccountLabel.visible
                        ? authorAccountLabel.width + parent.spacing : 0);
                var cap = Math.max(0, parent.width - reserved);
                return Math.min(implicitWidth, cap);
            }
            text: header.author
            color: header.replayed ? header.style.mutedColor : header.style.nickColor(header.author)
            elide: Text.ElideRight
            font.family: "iA Writer Mono S"
            font.bold: true
            font.pixelSize: header.style.scaledSize(12)

            TranscriptNickHit {
                nick: header.author
                nickOpensDirect: header.nickOpensDirect
                selfNick: header.selfNick
                onDirectMessageRequested: header.directMessageRequested(nick)
            }
        }

        Text {
            id: authorAccountLabel
            objectName: "message-account-" + header.author
            visible: header.account.length > 0
            text: header.account
            color: header.style.mutedColor
            elide: Text.ElideRight
            font.family: "iA Writer Mono S"
            font.pixelSize: header.style.scaledSize(12)
            width: visible
                ? Math.min(implicitWidth,
                           Math.max(header.style.scaledSize(48),
                                    authorRow.width * 0.4))
                : 0
            anchors.verticalCenter: authorLabel.verticalCenter
        }

        BotMark {
            id: messageBotMark
            style: header.style
            objectName: "message-bot-" + header.author
            shown: header.bot
        }
    }

    Text {
        id: messageTime
        objectName: "messageTime"
        text: header.time
        color: header.style.mutedColor
        font.family: "iA Writer Mono S"
        font.pixelSize: header.style.scaledSize(9)
        // Row has no glyph baseline (baselineOffset stays 0). Anchoring to
        // authorRow.baseline lifts the smaller stamp on Core Text; match
        // the author label inside the Row instead.
        y: authorLabel.baselineOffset - baselineOffset
    }
}
