import QtQuick
import QtQuick.Controls

Popup {
    id: sheet

    required property OmaircStyle style
    property var matches: null
    property int selectedIndex: 0
    property string selfNick: ""
    property bool awayPresenceVisible: false
    property bool memberStatusVisible: false
    property var avatarStore: null
    property bool loadPeerAvatars: true

    property alias nickFilter: nickFilter
    property alias nickList: nickList

    signal stepRequested(int delta)
    signal activateRequested()
    signal filterChanged()
    signal nickActivated(int index)

    objectName: "nickSheet"
    width: sheet.style.scaledSize(348)
    padding: sheet.style.scaledSize(16)
    modal: true
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

    background: Rectangle {
        color: sheet.style.raisedColor
        border.width: 1
        border.color: sheet.style.dividerColor
        radius: sheet.style.scaledSize(9)
    }

    contentItem: Column {
        spacing: sheet.style.scaledSize(8)
        width: sheet.availableWidth

        Rectangle {
            width: parent.width
            height: sheet.style.scaledSize(32)
            color: sheet.style.panelColor
            border.width: 1
            border.color: nickFilter.activeFocus ? sheet.style.accentColor : sheet.style.dividerColor
            radius: sheet.style.scaledSize(7)

            TextField {
                id: nickFilter
                objectName: "nickFilter"
                Accessible.name: "Jump to nick"
                anchors.fill: parent
                z: 1
                color: sheet.style.inkColor
                selectionColor: sheet.style.selectionColor
                selectedTextColor: "#ffffff"
                font.family: "iA Writer Mono S"
                font.pixelSize: sheet.style.scaledSize(13)
                placeholderText: ""
                verticalAlignment: TextInput.AlignVCenter
                leftPadding: sheet.style.scaledSize(8)
                rightPadding: sheet.style.scaledSize(8)
                background: Item {}
                Keys.onPressed: function(event) {
                    if (event.key === Qt.Key_Down) {
                        sheet.stepRequested(1);
                        event.accepted = true;
                        return;
                    }
                    if (event.key === Qt.Key_Up) {
                        sheet.stepRequested(-1);
                        event.accepted = true;
                        return;
                    }
                    if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                        sheet.activateRequested();
                        event.accepted = true;
                        return;
                    }
                    if (event.key === Qt.Key_Tab) {
                        event.accepted = true;
                    }
                }
                onTextChanged: sheet.filterChanged()
            }

            Text {
                objectName: "nickFilterPlaceholder"
                z: 0
                anchors.left: parent.left
                anchors.leftMargin: sheet.style.scaledSize(8)
                anchors.verticalCenter: parent.verticalCenter
                text: "Jump to nick…"
                color: sheet.style.mutedColor
                font.family: "iA Writer Mono S"
                font.pixelSize: sheet.style.scaledSize(13)
                visible: nickFilter.text.length === 0
            }
        }

        ListView {
            id: nickList
            objectName: "nickList"
            width: parent.width
            height: Math.min(contentHeight, sheet.style.scaledSize(252))
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            model: sheet.matches
            currentIndex: sheet.selectedIndex
            highlightFollowsCurrentItem: true

            delegate: Item {
                id: nickRow
                required property int index
                required property string name
                required property string label
                required property string memberStatus
                required property int awayFlag
                required property int botFlag
                required property string avatar
                required property string glyph
                required property int paletteIndex

                readonly property string nick: name
                readonly property bool away: awayFlag === 1
                readonly property bool bot: botFlag === 1
                // Exact match, like `openable`; the reducer's overlay is the CASEMAPPING-aware path.
                readonly property bool isSelf: nick.length > 0 && nick === sheet.selfNick
                readonly property bool showAway: away
                    && (sheet.awayPresenceVisible || isSelf)
                readonly property bool selected: index === sheet.selectedIndex
                readonly property bool openable: name !== sheet.selfNick
                readonly property color nickTint: sheet.style.nickPalette[paletteIndex]
                readonly property color avatarFill: sheet.style.nickAvatarFills[paletteIndex]

                objectName: "nickPick-" + name
                Accessible.name: label
                Accessible.description: sheet.memberStatusVisible ? memberStatus : ""
                Accessible.role: Accessible.Button
                Accessible.onPressAction: {
                    if (openable)
                        sheet.nickActivated(index);
                }
                width: nickList.width
                height: sheet.style.scaledSize(43)

                Rectangle {
                    objectName: "nickPickHighlight"
                    anchors.fill: parent
                    anchors.leftMargin: sheet.style.scaledSize(8)
                    anchors.rightMargin: sheet.style.scaledSize(8)
                    radius: sheet.style.scaledSize(7)
                    color: nickMouse.containsMouse || nickRow.selected
                        ? sheet.style.hoverColor
                        : "transparent"
                }

                Item {
                    anchors.left: parent.left
                    anchors.leftMargin: sheet.style.scaledSize(18)
                    anchors.verticalCenter: parent.verticalCenter
                    width: sheet.style.scaledSize(28)
                    height: width

                    NickGlyph {
                        anchors.fill: parent
                        style: sheet.style
                        avatarStore: sheet.avatarStore
                        loadPeerAvatars: sheet.loadPeerAvatars
                        nick: nickRow.nick
                        avatarUrl: nickRow.avatar
                        dimmed: nickRow.showAway
                        ink: nickRow.nickTint
                        fill: nickRow.avatarFill
                        fontPixelSize: sheet.style.scaledSize(11)
                    }

                    Rectangle {
                        objectName: "nickPick-presence-" + nickRow.nick
                        visible: sheet.awayPresenceVisible || nickRow.isSelf
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        width: sheet.style.scaledSize(7)
                        height: width
                        radius: width / 2
                        color: nickRow.showAway
                            ? sheet.style.presenceMarkColor("away")
                            : sheet.style.presenceMarkColor("online")
                        border.width: sheet.style.scaledSize(2)
                        border.color: sheet.style.raisedColor
                    }
                }

                Column {
                    anchors.left: parent.left
                    anchors.leftMargin: sheet.style.scaledSize(56)
                    anchors.right: parent.right
                    anchors.rightMargin: sheet.style.scaledSize(10)
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 0

                    Row {
                        width: parent.width
                        spacing: sheet.style.scaledSize(4)

                        Text {
                            width: {
                                var reserved = nickRow.bot
                                    ? nickBotMark.width + parent.spacing : 0;
                                var cap = Math.max(0, parent.width - reserved);
                                return Math.min(implicitWidth, cap);
                            }
                            text: nickRow.label
                            color: nickRow.showAway ? sheet.style.mutedColor : sheet.style.inkColor
                            elide: Text.ElideRight
                            font.family: "iA Writer Mono S"
                            font.bold: nickRow.nick === sheet.selfNick
                            font.pixelSize: sheet.style.scaledSize(12)
                        }

                        BotMark {
                            id: nickBotMark
                            style: sheet.style
                            objectName: "nickPick-bot-" + nickRow.nick
                            shown: nickRow.bot
                            anchors.verticalCenter: parent.verticalCenter
                        }
                    }

                    Text {
                        objectName: "nickPick-status-" + nickRow.nick
                        visible: sheet.memberStatusVisible && nickRow.memberStatus.length > 0
                        width: parent.width
                        text: nickRow.memberStatus
                        color: sheet.style.mutedColor
                        elide: Text.ElideRight
                        font.family: "iA Writer Mono S"
                        font.pixelSize: sheet.style.scaledSize(9)
                    }
                }

                MouseArea {
                    id: nickMouse
                    anchors.fill: parent
                    enabled: nickRow.openable
                    hoverEnabled: true
                    cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                    onClicked: sheet.nickActivated(nickRow.index)
                }
            }
        }
    }
}
