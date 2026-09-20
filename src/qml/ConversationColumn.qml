import QtQuick
import QtQuick.Controls

Item {
    id: column

    required property OmaircStyle style
    property bool consoleVisible: false
    property bool currentConversationIsChannel: false
    property bool membersVisible: true
    property int currentPeopleCount: 0
    property string headerTitle: ""
    property string topicText: ""
    property string statusTitle: ""
    property string statusSubtitle: ""
    property string currentConversation: ""
    property bool findActive: false
    property bool composerEnabled: true
    property var slashCommands: null
    property var activeMessages: null
    property var consoleLines: null
    property Component messageDelegate
    property Component messageFooter
    property Component consoleDelegate

    property alias composer: composer
    property alias messageList: messageList
    property alias consoleList: consoleList

    signal membersToggleRequested()
    signal sendRequested()
    signal composerTextEdited(string text)
    signal composerKeyPressed(var event)
    signal slashHitHovered(int index)
    signal slashHitActivated(int index)

    Item {
        id: conversationHeader
        visible: !column.consoleVisible
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        height: visible ? column.style.scaledSize(72) : 0

        Column {
            anchors.left: parent.left
            anchors.leftMargin: column.style.scaledSize(24)
            anchors.right: column.currentConversationIsChannel ? peopleButton.left : parent.right
            anchors.rightMargin: column.style.scaledSize(column.currentConversationIsChannel ? 18 : 24)
            anchors.verticalCenter: parent.verticalCenter
            spacing: column.style.scaledSize(3)

            Text {
                width: parent.width
                text: column.headerTitle
                color: column.style.inkColor
                elide: Text.ElideRight
                font.family: "iA Writer Mono S"
                font.bold: true
                font.pixelSize: column.style.scaledSize(17)
            }

            Text {
                objectName: "conversationTopic"
                width: parent.width
                text: column.topicText
                textFormat: Text.PlainText
                color: column.style.mutedColor
                elide: Text.ElideRight
                font.family: "iA Writer Mono S"
                font.pixelSize: column.style.scaledSize(11)
            }
        }

        Rectangle {
            id: peopleButton
            objectName: "peopleButton"
            Accessible.name: column.membersVisible ? "Hide members" : "Show members"
            Accessible.role: Accessible.Button
            Accessible.onPressAction: column.membersToggleRequested()
            visible: column.currentConversationIsChannel && !column.consoleVisible
            anchors.right: parent.right
            anchors.rightMargin: column.style.scaledSize(19)
            anchors.verticalCenter: parent.verticalCenter
            width: column.style.scaledSize(74)
            height: column.style.scaledSize(30)
            radius: column.style.scaledSize(7)
            color: peopleMouse.containsMouse || column.membersVisible
                ? column.style.raisedColor : "transparent"
            border.width: 1
            border.color: column.membersVisible ? column.style.dividerColor : "transparent"

            Text {
                anchors.centerIn: parent
                text: column.currentPeopleCount + " PEOPLE"
                color: column.membersVisible ? column.style.inkColor : column.style.mutedColor
                font.family: "iA Writer Mono S"
                font.bold: true
                font.pixelSize: column.style.scaledSize(9)
            }

            MouseArea {
                id: peopleMouse
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: column.membersToggleRequested()
            }
        }

        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: column.style.dividerColor
        }
    }

    Item {
        id: consoleHeader
        visible: column.consoleVisible
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        height: visible ? column.style.scaledSize(72) : 0

        Column {
            anchors.left: parent.left
            anchors.leftMargin: column.style.scaledSize(24)
            anchors.right: parent.right
            anchors.rightMargin: column.style.scaledSize(24)
            anchors.verticalCenter: parent.verticalCenter
            spacing: column.style.scaledSize(3)

            Text {
                width: parent.width
                text: column.statusTitle
                color: column.style.inkColor
                elide: Text.ElideRight
                font.family: "iA Writer Mono S"
                font.bold: true
                font.pixelSize: column.style.scaledSize(17)
            }

            Text {
                width: parent.width
                text: column.statusSubtitle
                color: column.style.mutedColor
                elide: Text.ElideRight
                font.family: "iA Writer Mono S"
                font.pixelSize: column.style.scaledSize(11)
            }
        }

        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: column.style.dividerColor
        }
    }

    TranscriptList {
        id: messageList
        objectName: "messageList"
        visible: !column.consoleVisible
        Accessible.name: "Messages in " + column.currentConversation
        anchors.top: conversationHeader.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: composerShell.top
        anchors.bottomMargin: column.style.scaledSize(12)
        model: column.activeMessages
        delegate: column.messageDelegate
        footer: column.messageFooter
    }

    UnseenJumpButton {
        style: column.style
        list: messageList
        objectName: "messageUnseenJump"
    }

    TranscriptList {
        id: consoleList
        objectName: "consoleList"
        visible: column.consoleVisible
        Accessible.name: "Status"
        anchors.top: consoleHeader.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: composerShell.top
        anchors.bottomMargin: column.style.scaledSize(12)
        model: column.consoleLines
        delegate: column.consoleDelegate
    }

    UnseenJumpButton {
        style: column.style
        list: consoleList
        objectName: "consoleUnseenJump"
    }

    Rectangle {
        id: composerShell
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.leftMargin: column.style.scaledSize(20)
        anchors.rightMargin: column.style.scaledSize(20)
        anchors.bottomMargin: column.style.scaledSize(20)
        height: Math.max(column.style.scaledSize(46), Math.min(column.style.scaledSize(112),
            composer.contentHeight + column.style.scaledSize(20)))
        radius: column.style.scaledSize(10)
        color: column.style.panelColor
        border.width: 1
        border.color: composer.activeFocus ? column.style.accentColor : column.style.dividerColor

        TextField {
            id: composer
            objectName: "messageComposer"
            Accessible.name: column.findActive ? "Find" : "Message composer"
            Accessible.description: column.findActive
                ? "Find in the current transcript"
                : (column.consoleVisible
                    ? "Command for " + column.statusTitle.replace(" Status", "")
                    : "Write a message to " + column.currentConversation)
            anchors.left: parent.left
            anchors.right: sendButton.left
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            anchors.leftMargin: column.style.scaledSize(8)
            anchors.rightMargin: column.style.scaledSize(8)
            verticalAlignment: TextEdit.AlignVCenter
            color: column.style.inkColor
            selectionColor: column.style.selectionColor
            selectedTextColor: "#ffffff"
            font.family: "iA Writer Mono S"
            font.pixelSize: column.style.scaledSize(13)
            placeholderText: column.findActive ? "Find" : ""
            placeholderTextColor: column.style.mutedColor
            enabled: column.composerEnabled
            leftPadding: column.style.scaledSize(8)
            rightPadding: column.style.scaledSize(8)
            topPadding: Math.max(column.style.scaledSize(8),
                (height - contentHeight) / 2)
            bottomPadding: topPadding
            background: Item {}
            onTextChanged: column.composerTextEdited(text)

            // BeforeItem so Option+Left/Right never become word-movement.
            // On macOS TextInput claims those as MoveToPreviousWord /
            // MoveToNextWord, which swallows the window Shortcut.
            Keys.priority: Keys.BeforeItem
            Keys.onPressed: function(event) {
                column.composerKeyPressed(event);
            }
        }

        Rectangle {
            id: sendButton
            objectName: "sendButton"
            Accessible.name: "Send message"
            Accessible.role: Accessible.Button
            Accessible.onPressAction: {
                if (composer.text.trim().length > 0)
                    column.sendRequested();
            }
            anchors.right: parent.right
            anchors.rightMargin: column.style.scaledSize(8)
            anchors.verticalCenter: parent.verticalCenter
            width: column.style.scaledSize(54)
            height: column.style.scaledSize(30)
            radius: column.style.scaledSize(7)
            color: composer.text.trim().length > 0
                ? (sendMouse.containsMouse
                    ? column.style.mixColors(column.style.accentColor, column.style.inkColor, 0.14)
                    : column.style.accentColor)
                : column.style.raisedColor

            Text {
                anchors.centerIn: parent
                text: "SEND"
                color: composer.text.trim().length > 0 ? "#ffffff" : column.style.mutedColor
                font.family: "iA Writer Mono S"
                font.bold: true
                font.pixelSize: column.style.scaledSize(9)
            }

            MouseArea {
                id: sendMouse
                anchors.fill: parent
                enabled: composer.text.trim().length > 0
                hoverEnabled: true
                cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                onClicked: column.sendRequested()
            }
        }
    }

    Rectangle {
        id: slashCompleteList
        objectName: "slashCompleteList"
        visible: column.slashCommands && column.slashCommands.open
        z: 2
        anchors.left: composerShell.left
        anchors.right: composerShell.right
        anchors.bottom: composerShell.top
        anchors.bottomMargin: column.style.scaledSize(6)
        height: slashHitColumn.implicitHeight + column.style.scaledSize(12)
        radius: column.style.scaledSize(10)
        color: column.style.raisedColor
        border.width: 1
        border.color: column.style.dividerColor

        Column {
            id: slashHitColumn
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: column.style.scaledSize(6)
            spacing: column.style.scaledSize(2)

            Repeater {
                model: column.slashCommands ? column.slashCommands.matches : []
                delegate: Rectangle {
                    objectName: "slashHit-" + String(modelData.label).slice(1)
                    width: slashHitColumn.width
                    height: column.style.scaledSize(28)
                    radius: column.style.scaledSize(7)
                    color: {
                        if (column.slashCommands
                                && index === column.slashCommands.selectedIndex)
                            return column.style.mixColors(column.style.selectionColor,
                                                          column.style.raisedColor,
                                                          column.style.darkMode ? 0.45 : 0.35);
                        if (hitMouse.containsMouse)
                            return column.style.hoverColor;
                        return "transparent";
                    }

                    Row {
                        anchors.fill: parent
                        anchors.leftMargin: column.style.scaledSize(8)
                        anchors.rightMargin: column.style.scaledSize(8)
                        spacing: column.style.scaledSize(12)

                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: modelData.label
                            color: column.style.inkColor
                            font.family: "iA Writer Mono S"
                            font.pixelSize: column.style.scaledSize(12)
                        }

                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            visible: usageHint.trim().length > 0
                            width: Math.max(0, parent.width - x)
                            text: usageHint
                            elide: Text.ElideRight
                            color: column.style.mutedColor
                            font.family: "iA Writer Mono S"
                            font.pixelSize: column.style.scaledSize(12)

                            readonly property string usageHint: {
                                var usage = String(modelData.usage)
                                var label = String(modelData.label)
                                if (usage.indexOf(label) === 0)
                                    return usage.substring(label.length)
                                return usage
                            }
                        }
                    }

                    MouseArea {
                        id: hitMouse
                        anchors.fill: parent
                        hoverEnabled: true
                        onEntered: column.slashHitHovered(index)
                        onClicked: column.slashHitActivated(index)
                    }
                }
            }
        }
    }
}
