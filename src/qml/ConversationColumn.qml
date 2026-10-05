import QtQuick
import QtQuick.Controls

Item {
    id: column
    objectName: "conversationColumn"

    required property OmaircStyle style
    property bool consoleVisible: false
    property bool currentConversationIsChannel: false
    property bool channelJoined: false
    property bool canCloseSelection: false
    property bool serverJoinEnabled: false
    property bool membersVisible: true
    property int currentPeopleCount: 0
    property string headerTitle: ""
    property string topicText: ""
    property var httpUrlAt: null
    property var channelNameAt: null
    property var openAllowedUrl: null
    property var openChannelName: null
    property bool queryIdentity: false
    property bool awayPresenceVisible: false
    property string queryPresence: "offline"
    property var queryLabels: []
    property string statusTitle: ""
    property string statusSubtitle: ""
    property string currentConversation: ""
    property bool findActive: false
    property bool composerEnabled: true
    property bool fileHostOffered: false
    property var slashCommands: null
    property var activeMessages: null
    property var consoleLines: null
    property var onTranscriptPinnedToEnd: null
    property Component messageDelegate
    property Component messageFooter
    property Component consoleDelegate
    property var readMarkerSync: null

    property alias composer: composer
    property alias messageList: messageList
    property alias consoleList: consoleList

    signal membersToggleRequested()
    signal leaveRequested()
    signal joinRequested()
    signal closeRequested()
    signal serverJoinRequested(string channel)
    signal sendRequested()
    signal composerTextEdited(string text)
    signal composerKeyPressed(var event)
    signal slashHitHovered(int index)
    signal slashHitActivated(int index)
    signal filePicked(string path)

    function openFilePick() {
        if (column.fileHostOffered && !column.findActive && column.composerEnabled)
            filePick.open();
    }

    function acceptDroppedUrls(urls) {
        var i = 0
        for (; i < urls.length; ++i)
            column.filePicked(urls[i].toString())
    }

    Item {
        id: conversationHeader
        visible: !column.consoleVisible
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        height: visible ? column.style.scaledSize(72) : 0

        TextMetrics {
            id: queryLabelMetrics
            font.family: "iA Writer Mono S"
            font.pixelSize: column.style.scaledSize(11)
            text: {
                var labels = column.queryLabels || [];
                var joined = "";
                for (var index = 0; index < labels.length; ++index)
                    joined += String(labels[index]);
                return joined;
            }
        }

        Column {
            anchors.left: parent.left
            anchors.leftMargin: column.style.scaledSize(24)
            anchors.right: headerChrome.visible ? headerChrome.left : parent.right
            anchors.rightMargin: column.style.scaledSize(headerChrome.visible ? 18 : 24)
            anchors.verticalCenter: parent.verticalCenter
            spacing: column.style.scaledSize(3)

            Text {
                visible: !column.queryIdentity
                width: parent.width
                text: column.headerTitle
                color: column.style.inkColor
                elide: Text.ElideRight
                font.family: "iA Writer Mono S"
                font.bold: true
                font.pixelSize: column.style.scaledSize(17)
            }

            Row {
                id: queryTitleRow
                visible: column.queryIdentity
                width: parent.width
                spacing: column.style.scaledSize(8)

                Item {
                    id: queryPresenceSlot
                    visible: column.queryIdentity && column.awayPresenceVisible
                    width: visible ? column.style.scaledSize(8) : 0
                    height: queryNick.implicitHeight

                    Rectangle {
                        objectName: "queryPresenceDot"
                        visible: queryPresenceSlot.visible
                        anchors.verticalCenter: parent.verticalCenter
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: column.style.scaledSize(8)
                        height: width
                        radius: width / 2
                        color: column.style.presenceMarkColor(column.queryPresence)
                    }
                }

                Text {
                    id: queryNick
                    text: column.headerTitle
                    color: column.style.inkColor
                    elide: Text.ElideRight
                    font.family: "iA Writer Mono S"
                    font.bold: true
                    font.pixelSize: column.style.scaledSize(17)
                    width: {
                        // The face is monospace, so the joined advance width is
                        // the sum of the label widths. Reading the repeater's
                        // itemAt() misses the pass that creates the delegates.
                        var used = 0;
                        var labels = column.queryLabels || [];
                        if (queryPresenceSlot.visible)
                            used += queryPresenceSlot.width + queryTitleRow.spacing;
                        if (labels.length > 0) {
                            used += queryLabelMetrics.width
                                + labels.length * queryTitleRow.spacing;
                        }
                        return Math.max(0, Math.min(implicitWidth,
                                                    queryTitleRow.width - used));
                    }
                }

                Repeater {
                    id: queryLabelRepeater
                    model: column.queryLabels

                    Text {
                        objectName: "queryFactLabel"
                        text: modelData
                        color: column.style.mutedColor
                        font.family: "iA Writer Mono S"
                        font.pixelSize: column.style.scaledSize(11)
                        height: queryNick.implicitHeight
                        verticalAlignment: Text.AlignVCenter
                    }
                }
            }

            TopicLine {
                width: parent.width
                style: column.style
                text: column.topicText
                httpUrlAt: column.httpUrlAt
                channelNameAt: column.channelNameAt
                openAllowedUrl: column.openAllowedUrl
                openChannelName: column.openChannelName
            }
        }

        Row {
            id: headerChrome
            anchors.right: parent.right
            anchors.rightMargin: column.style.scaledSize(19)
            anchors.verticalCenter: parent.verticalCenter
            spacing: column.style.scaledSize(8)
            // Do not read headerActions.visible here. A parent visible flag
            // that depends on a child's visible breaks that child's binding.
            visible: column.currentConversationIsChannel || column.canCloseSelection

            ConversationHeaderActions {
                id: headerActions
                style: column.style
                channel: column.currentConversationIsChannel
                joined: column.channelJoined
                canClose: column.canCloseSelection
                onLeaveRequested: column.leaveRequested()
                onJoinRequested: column.joinRequested()
                onCloseRequested: column.closeRequested()
            }

            Rectangle {
                id: peopleButton
                objectName: "peopleButton"
                Accessible.name: column.membersVisible ? "Hide members" : "Show members"
                Accessible.role: Accessible.Button
                Accessible.onPressAction: column.membersToggleRequested()
                visible: column.currentConversationIsChannel
                    && column.channelJoined
                    && !column.consoleVisible
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
            anchors.right: serverJoin.visible ? serverJoin.left : parent.right
            anchors.rightMargin: column.style.scaledSize(serverJoin.visible ? 18 : 24)
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

        ServerJoinField {
            id: serverJoin
            style: column.style
            joinEnabled: column.serverJoinEnabled
            visible: column.serverJoinEnabled
            anchors.right: parent.right
            anchors.rightMargin: column.style.scaledSize(19)
            anchors.verticalCenter: parent.verticalCenter
            onSubmitted: function(channel) { column.serverJoinRequested(channel) }
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
        onPinnedToEnd: column.onTranscriptPinnedToEnd
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
        readMarkerSync: column.readMarkerSync
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
        border.color: fileDrop.containsDrag
            ? column.style.accentColor
            : (composer.activeFocus ? column.style.accentColor : column.style.dividerColor)

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
            anchors.right: filePick.left
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

        FilePickButton {
            id: filePick
            style: column.style
            visible: column.fileHostOffered && !column.findActive && column.composerEnabled
            width: visible ? implicitWidth : 0
            anchors.right: sendButton.left
            anchors.rightMargin: visible ? column.style.scaledSize(8) : 0
            anchors.verticalCenter: parent.verticalCenter
            onPicked: function(path) {
                column.filePicked(path);
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

    DropArea {
        id: fileDrop
        objectName: "fileDrop"
        anchors.fill: parent
        enabled: column.fileHostOffered && !column.findActive && column.composerEnabled
        onDropped: function(drop) {
            if (drop.hasUrls)
                column.acceptDroppedUrls(drop.urls);
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
