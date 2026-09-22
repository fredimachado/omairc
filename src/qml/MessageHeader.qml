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

    anchors.left: parent.left
    anchors.leftMargin: style.scaledSize(70)
    anchors.top: parent.top
    anchors.topMargin: style.scaledSize(8)
    spacing: style.scaledSize(9)

    Row {
        id: authorRow
        spacing: (header.bot || header.account.length > 0)
            ? header.style.scaledSize(4) : 0

        Text {
            id: authorLabel
            objectName: "messageAuthor"
            text: header.author
            color: header.replayed ? header.style.mutedColor : header.style.nickColor(header.author)
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
            objectName: "message-account-" + header.author
            visible: header.account.length > 0
            text: header.account
            color: header.style.mutedColor
            font.family: "iA Writer Mono S"
            font.pixelSize: header.style.scaledSize(12)
            anchors.verticalCenter: authorLabel.verticalCenter
        }

        BotMark {
            style: header.style
            objectName: "message-bot-" + header.author
            shown: header.bot
        }
    }

    Text {
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
