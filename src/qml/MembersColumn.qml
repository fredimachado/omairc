import QtQuick
import QtQuick.Controls

Rectangle {
    id: column

    required property OmaircStyle style
    property var irc: null
    property var avatarStore: null
    property bool loadPeerAvatars: true
    property string currentConversation: ""
    property string selfNick: ""
    property int currentPeopleCount: 0
    property bool awayPresenceVisible: false
    property bool memberStatusVisible: false
    property bool typingVisible: false
    property var typingNicks: []

    property alias membersList: membersList

    signal nickSheetRequested()
    signal directMessageRequested(string nick)
    signal memberActivateRequested()

    objectName: "membersPanel"
    Accessible.name: "Members of " + column.currentConversation
    color: column.style.panelColor

    Rectangle {
        anchors.left: parent.left
        width: 1
        height: parent.height
        color: column.style.dividerColor
    }

    Text {
        id: membersHeading
        objectName: "membersHeading"
        anchors.top: parent.top
        anchors.topMargin: column.style.scaledSize(23)
        anchors.left: parent.left
        anchors.leftMargin: column.style.scaledSize(20)
        text: "ONLINE - " + column.currentPeopleCount
        color: membersHeadingHit.containsMouse ? column.style.inkColor : column.style.mutedColor
        font.family: "iA Writer Mono S"
        font.bold: true
        font.letterSpacing: column.style.scaledSize(0.7)
        font.pixelSize: column.style.scaledSize(9)
    }

    MouseArea {
        id: membersHeadingHit
        objectName: "membersHeadingButton"
        anchors.fill: membersHeading
        anchors.margins: -column.style.scaledSize(8)
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        Accessible.role: Accessible.Button
        Accessible.name: "Jump to nick"
        Accessible.onPressAction: column.nickSheetRequested()
        onClicked: column.nickSheetRequested()
    }

    ListView {
        id: membersList
        objectName: "membersList"
        anchors.top: membersHeading.bottom
        anchors.topMargin: column.style.scaledSize(14)
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.bottomMargin: column.style.scaledSize(10)
        clip: true
        model: column.irc ? column.irc.members : null
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
        keyNavigationEnabled: true
        highlightFollowsCurrentItem: true
        highlightMoveDuration: 0
        currentIndex: 0

        Keys.onPressed: function(event) {
            if (event.key !== Qt.Key_Return && event.key !== Qt.Key_Enter)
                return;
            column.memberActivateRequested();
            event.accepted = true;
        }

        delegate: Item {
            id: memberDelegate

            readonly property var memberData: column.irc
                ? ({nick: model.nick, label: model.label, status: model.status,
                    away: model.away, avatar: model.avatar || "", bot: !!model.bot,
                    account: model.account || ""})
                : ({nick: "", label: "", status: "", away: false, avatar: "",
                    bot: false, account: ""})
            readonly property string nick: memberData.nick
            readonly property string label: memberData.label
            readonly property string status: memberData.status
            readonly property string avatar: memberData.avatar
            readonly property bool bot: memberData.bot
            readonly property string account: memberData.account || ""
            // Exact match, like `openable`; the reducer's overlay is the CASEMAPPING-aware path.
            readonly property bool isSelf: nick.length > 0 && nick === column.selfNick
            // Other members' away state needs away-notify, but our own
            // arrives as the 305/306 numerics, so it stays visible and
            // agrees with the identity footer.
            readonly property bool away: memberData.away
                && (column.awayPresenceVisible || isSelf)
            readonly property bool typing: column.typingVisible
                && (column.irc
                    ? column.irc.nickIsTyping(memberDelegate.nick)
                    : column.typingNicks.indexOf(memberDelegate.nick) !== -1)

            objectName: "member-" + nick
            Accessible.name: label
            Accessible.description: column.memberStatusVisible ? status : ""
            Accessible.role: Accessible.Button
            Accessible.onPressAction: {
                if (memberDelegate.nick.length > 0
                        && memberDelegate.nick !== column.selfNick)
                    column.directMessageRequested(memberDelegate.nick);
            }
            width: ListView.view.width
            height: column.style.scaledSize(43)

            Rectangle {
                objectName: "memberHighlight"
                anchors.fill: parent
                anchors.leftMargin: column.style.scaledSize(8)
                anchors.rightMargin: column.style.scaledSize(8)
                radius: column.style.scaledSize(7)
                color: memberMouse.containsMouse
                    ? column.style.hoverColor
                    : (memberDelegate.ListView.isCurrentItem && membersList.activeFocus
                        ? column.style.raisedColor
                        : "transparent")
            }

            Item {
                anchors.left: parent.left
                anchors.leftMargin: column.style.scaledSize(18)
                anchors.verticalCenter: parent.verticalCenter
                width: column.style.scaledSize(28)
                height: width

                NickGlyph {
                    anchors.fill: parent
                    style: column.style
                    avatarStore: column.avatarStore
                    loadPeerAvatars: column.loadPeerAvatars
                    nick: memberDelegate.nick
                    avatarUrl: memberDelegate.avatar
                    dimmed: memberDelegate.away
                    fontPixelSize: column.style.scaledSize(11)
                }

                Rectangle {
                    objectName: "presence-dot-" + memberDelegate.nick
                    visible: column.awayPresenceVisible || memberDelegate.isSelf
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    width: column.style.scaledSize(7)
                    height: width
                    radius: width / 2
                    color: column.style.presenceMarkColor(
                        memberDelegate.away ? "away" : "online")
                    border.width: column.style.scaledSize(2)
                    border.color: column.style.panelColor
                }
            }

            Column {
                anchors.left: parent.left
                anchors.leftMargin: column.style.scaledSize(56)
                anchors.right: parent.right
                anchors.rightMargin: column.style.scaledSize(10)
                anchors.verticalCenter: parent.verticalCenter
                spacing: 0

                Row {
                    width: parent.width
                    spacing: column.style.scaledSize(4)

                    Text {
                        width: {
                            var reserved = (memberDelegate.bot
                                ? memberBotMark.width + parent.spacing : 0)
                                + (memberAccountLabel.visible
                                    ? memberAccountLabel.width + parent.spacing : 0)
                                + (memberDelegate.typing
                                    ? memberTypingGlyph.implicitWidth + parent.spacing
                                    : 0);
                            var cap = Math.max(0, parent.width - reserved);
                            return Math.min(implicitWidth, cap);
                        }
                        text: memberDelegate.label
                        color: memberDelegate.away ? column.style.mutedColor : column.style.inkColor
                        elide: Text.ElideRight
                        font.family: "iA Writer Mono S"
                        font.bold: memberDelegate.nick === column.selfNick
                        font.pixelSize: column.style.scaledSize(12)
                    }

                    Text {
                        id: memberAccountLabel
                        objectName: "member-account-" + memberDelegate.nick
                        visible: memberDelegate.account.length > 0
                        text: memberDelegate.account
                        color: column.style.mutedColor
                        elide: Text.ElideRight
                        font.family: "iA Writer Mono S"
                        font.pixelSize: column.style.scaledSize(12)
                        width: visible
                            ? Math.min(implicitWidth,
                                       Math.max(column.style.scaledSize(48),
                                                parent.width * 0.4))
                            : 0
                        anchors.verticalCenter: parent.verticalCenter
                    }

                    BotMark {
                        id: memberBotMark
                        style: column.style
                        objectName: "member-bot-" + memberDelegate.nick
                        shown: memberDelegate.bot
                        anchors.verticalCenter: parent.verticalCenter
                    }

                    TypingDots {
                        id: memberTypingGlyph
                        style: column.style
                        objectName: "member-typing-" + memberDelegate.nick
                        visible: memberDelegate.typing
                        pixelSize: column.style.scaledSize(12)
                    }
                }

                Text {
                    objectName: "member-status-" + memberDelegate.nick
                    visible: column.memberStatusVisible
                        && memberDelegate.status.length > 0
                    width: parent.width
                    text: memberDelegate.status
                    color: column.style.mutedColor
                    elide: Text.ElideRight
                    font.family: "iA Writer Mono S"
                    font.pixelSize: column.style.scaledSize(9)
                }
            }

            MouseArea {
                id: memberMouse
                anchors.fill: parent
                enabled: memberDelegate.nick.length > 0
                    && memberDelegate.nick !== column.selfNick
                hoverEnabled: true
                cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                onClicked: column.directMessageRequested(memberDelegate.nick)
            }
        }
    }
}
