import QtQuick
import QtQuick.Controls
import QtQuick.Controls.Material
import QtQuick.Layouts
import QtQuick.Window

ApplicationWindow {
    id: win

    width: 1180
    height: 760
    minimumWidth: 760
    minimumHeight: 540
    visible: true
    title: currentConversation + " - Omairc"

    readonly property bool darkMode: backend.darkMode
    readonly property real textScale: backend.textScale
    readonly property color pageColor: backend.themeBackground
    readonly property color inkColor: backend.themeForeground
    readonly property color accentColor: backend.themeAccent
    readonly property color selectionColor: backend.themeSelection
    readonly property color panelColor: mixColors(pageColor, inkColor, darkMode ? 0.035 : 0.025)
    readonly property color raisedColor: mixColors(pageColor, inkColor, darkMode ? 0.075 : 0.055)
    readonly property color hoverColor: mixColors(pageColor, inkColor, darkMode ? 0.10 : 0.075)
    readonly property color dividerColor: mixColors(pageColor, inkColor, darkMode ? 0.13 : 0.11)
    readonly property color mutedColor: mixColors(pageColor, inkColor, darkMode ? 0.52 : 0.47)

    property string currentConversation: "#omarchy"
    property string currentTopic: "A cozy corner for Omarchy users and builders."
    property var activeMessages: omarchyMessages
    property bool membersVisible: true
    readonly property bool currentConversationIsChannel: currentConversation.charAt(0) === "#"
    readonly property int currentPeopleCount: peopleCountFor(currentConversation)

    Material.theme: darkMode ? Material.Dark : Material.Light
    Material.accent: accentColor
    color: pageColor

    function scaledSize(pixels) {
        return Math.max(1, Math.round(pixels * textScale));
    }

    function mixColors(base, tint, amount) {
        return Qt.rgba(
            base.r + (tint.r - base.r) * amount,
            base.g + (tint.g - base.g) * amount,
            base.b + (tint.b - base.b) * amount,
            1);
    }

    function nickColor(nick) {
        var palette = [
            accentColor,
            darkMode ? "#c099ff" : "#7950b8",
            darkMode ? "#7fc8a9" : "#237a58",
            darkMode ? "#efb366" : "#a45f14",
            darkMode ? "#ed8f9d" : "#b44355"
        ];
        var hash = 0;
        for (var index = 0; index < nick.length; ++index)
            hash = (hash + nick.charCodeAt(index)) % palette.length;
        return palette[hash];
    }

    function initials(nick) {
        return nick.length > 0 ? nick.charAt(0).toUpperCase() : "?";
    }

    function topicFor(name) {
        if (name === "#omarchy")
            return "A cozy corner for Omarchy users and builders.";
        if (name === "#desktop")
            return "Desktops should feel personal, fast, and calm.";
        if (name === "#ricing")
            return "Themes, type, wallpapers, and the tiny details.";
        if (name === "#help")
            return "Ask a clear question. Share what you already tried.";
        if (name.charAt(0) !== "#")
            return "Direct message with " + name;
        return "A local mock conversation.";
    }

    function peopleCountFor(name) {
        if (name === "#omarchy")
            return 12;
        if (name === "#desktop")
            return 8;
        if (name === "#ricing")
            return 10;
        if (name === "#help")
            return 5;
        return 2;
    }

    function memberDataFor(index) {
        if (currentConversation === "anna")
            return membersModel.get(index === 0 ? 0 : 4);
        if (currentConversation === "dax")
            return membersModel.get(index === 0 ? 1 : 4);
        return membersModel.get(index);
    }

    function messagesFor(name) {
        if (name === "#desktop")
            return desktopMessages;
        if (name === "#ricing")
            return ricingMessages;
        if (name === "#help")
            return helpMessages;
        if (name === "anna")
            return annaMessages;
        if (name === "dax")
            return daxMessages;
        if (name === "mira")
            return miraMessages;
        if (name === "sol")
            return solMessages;
        if (name === "kai")
            return kaiMessages;
        if (name === "nora")
            return noraMessages;
        if (name === "teo")
            return teoMessages;
        if (name === "lena")
            return lenaMessages;
        if (name === "sam")
            return samMessages;
        if (name === "ivy")
            return ivyMessages;
        if (name === "max")
            return maxMessages;
        return omarchyMessages;
    }

    function openDirectMessage(nick) {
        for (var index = 0; index < directConversations.count; ++index) {
            if (directConversations.get(index).conversation === nick) {
                selectConversation(nick);
                return;
            }
        }

        directConversations.append({
            conversation: nick,
            directUnread: 0,
            directMention: false
        });
        selectConversation(nick);
    }

    function selectConversation(name) {
        currentConversation = name;
        currentTopic = topicFor(name);
        activeMessages = messagesFor(name);
        Qt.callLater(function() {
            messageList.positionViewAtEnd();
            composer.forceActiveFocus();
        });
    }

    function sendMessage() {
        var body = composer.text.trim();
        if (body.length === 0)
            return;

        var kind = "message";
        if (body.indexOf("/me ") === 0) {
            body = "fred " + body.substring(4);
            kind = "action";
        }

        activeMessages.append({
            author: "fred",
            time: Qt.formatTime(new Date(), "hh:mm"),
            body: body,
            kind: kind
        });
        composer.clear();
        messageList.positionViewAtEnd();
    }

    Shortcut {
        sequence: "Ctrl+Q"
        context: Qt.ApplicationShortcut
        onActivated: win.close()
    }

    Shortcut {
        sequence: "Ctrl+L"
        context: Qt.ApplicationShortcut
        onActivated: composer.forceActiveFocus()
    }

    Shortcut {
        sequence: "Ctrl+Shift+M"
        context: Qt.ApplicationShortcut
        enabled: currentConversationIsChannel
        onActivated: membersVisible = !membersVisible
    }

    component ConversationRow: Item {
        id: conversationRow

        required property string conversationName
        required property int unread
        required property bool mention
        property bool direct: false

        width: parent ? parent.width : 0
        height: win.scaledSize(36)

        Rectangle {
            anchors.fill: parent
            anchors.leftMargin: win.scaledSize(8)
            anchors.rightMargin: win.scaledSize(8)
            radius: win.scaledSize(7)
            color: conversationRow.conversationName === win.currentConversation
                ? win.raisedColor
                : rowMouse.containsMouse ? win.hoverColor : "transparent"
        }

        Rectangle {
            visible: conversationRow.conversationName === win.currentConversation
            anchors.left: parent.left
            anchors.leftMargin: win.scaledSize(8)
            anchors.verticalCenter: parent.verticalCenter
            width: win.scaledSize(3)
            height: win.scaledSize(18)
            radius: width
            color: win.accentColor
        }

        Rectangle {
            visible: conversationRow.direct
            anchors.left: parent.left
            anchors.leftMargin: win.scaledSize(18)
            anchors.verticalCenter: parent.verticalCenter
            width: win.scaledSize(22)
            height: width
            radius: width / 2
            color: win.mixColors(win.pageColor, win.nickColor(conversationRow.conversationName), 0.24)

            Text {
                anchors.centerIn: parent
                text: win.initials(conversationRow.conversationName)
                color: win.nickColor(conversationRow.conversationName)
                font.family: "iA Writer Mono S"
                font.bold: true
                font.pixelSize: win.scaledSize(11)
            }

            Rectangle {
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                width: win.scaledSize(7)
                height: width
                radius: width / 2
                color: "#69b978"
                border.width: win.scaledSize(2)
                border.color: win.panelColor
            }
        }

        Text {
            anchors.left: parent.left
            anchors.leftMargin: conversationRow.direct ? win.scaledSize(49) : win.scaledSize(20)
            anchors.right: unreadBadge.left
            anchors.rightMargin: win.scaledSize(8)
            anchors.verticalCenter: parent.verticalCenter
            text: (conversationRow.direct ? "" : "#  ") + conversationRow.conversationName.replace("#", "")
            color: conversationRow.conversationName === win.currentConversation
                ? win.inkColor
                : conversationRow.unread > 0 ? win.inkColor : win.mutedColor
            elide: Text.ElideRight
            font.family: "iA Writer Mono S"
            font.bold: conversationRow.conversationName === win.currentConversation
                       || conversationRow.unread > 0
            font.pixelSize: win.scaledSize(13)
        }

        Rectangle {
            id: unreadBadge
            visible: conversationRow.unread > 0
            anchors.right: parent.right
            anchors.rightMargin: win.scaledSize(17)
            anchors.verticalCenter: parent.verticalCenter
            width: Math.max(win.scaledSize(19), badgeText.implicitWidth + win.scaledSize(10))
            height: win.scaledSize(19)
            radius: height / 2
            color: conversationRow.mention ? win.accentColor : win.raisedColor

            Text {
                id: badgeText
                anchors.centerIn: parent
                text: conversationRow.unread
                color: conversationRow.mention ? "#ffffff" : win.inkColor
                font.family: "iA Writer Mono S"
                font.bold: true
                font.pixelSize: win.scaledSize(10)
            }
        }

        MouseArea {
            id: rowMouse
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: {
                conversationRow.unread = 0;
                conversationRow.mention = false;
                win.selectConversation(conversationRow.conversationName);
            }
        }
    }

    ListModel {
        id: omarchyMessages
        ListElement {
            author: ""
            time: ""
            body: "Today"
            kind: "event"
        }
        ListElement {
            author: "anna"
            time: "09:41"
            body: "Morning! Has anyone tried the new minimal install flow yet?"
            kind: "message"
        }
        ListElement {
            author: "dax"
            time: "09:43"
            body: "Yes. Fresh install on my Framework took about twelve minutes. The defaults feel really considered."
            kind: "message"
        }
        ListElement {
            author: "mira"
            time: "09:46"
            body: "The way the theme carries across the terminal and native apps is my favorite detail."
            kind: "message"
        }
        ListElement {
            author: ""
            time: ""
            body: "sol joined #omarchy"
            kind: "event"
        }
        ListElement {
            author: "sol"
            time: "09:52"
            body: "Hey all. Just landed here from Arch. This feels surprisingly calm."
            kind: "message"
        }
        ListElement {
            author: "anna"
            time: "09:53"
            body: "Welcome, sol. Calm is the whole idea."
            kind: "message"
        }
        ListElement {
            author: "dax"
            time: "09:55"
            body: "If you have not already, try the keyboard-first app launcher. It becomes muscle memory fast."
            kind: "message"
        }
        ListElement {
            author: "sol"
            time: "09:56"
            body: "I found it. The shortcuts sheet is a nice touch too."
            kind: "message"
        }
        ListElement {
            author: "mira"
            time: "09:57"
            body: "Most of the system makes sense once you learn three or four core bindings."
            kind: "message"
        }
        ListElement {
            author: "anna"
            time: "09:58"
            body: "And everything important is still plain text when you want to look underneath."
            kind: "message"
        }
        ListElement {
            author: "kai"
            time: "09:59"
            body: "That balance is hard to get right: friendly defaults without hiding the actual system."
            kind: "message"
        }
        ListElement {
            author: "dax"
            time: "10:00"
            body: "Exactly. Start simple, then make it yours one deliberate change at a time."
            kind: "message"
        }
        ListElement {
            author: ""
            time: ""
            body: "nora joined #omarchy"
            kind: "event"
        }
        ListElement {
            author: "nora"
            time: "10:01"
            body: "Good timing. I was just looking for a quiet place to ask about native Omarchy apps."
            kind: "message"
        }
        ListElement {
            author: "fred"
            time: "10:02"
            body: "I am sketching a tiny IRC client that belongs here. No browser chrome, no clutter."
            kind: "message"
        }
        ListElement {
            author: "mira"
            time: "10:04"
            body: "Keep the member list optional and I am sold."
            kind: "message"
        }
    }

    ListModel {
        id: desktopMessages
        ListElement { author: ""; time: ""; body: "Today"; kind: "event" }
        ListElement {
            author: "dax"
            time: "08:22"
            body: "I finally moved every workspace rule into a small, readable file."
            kind: "message"
        }
        ListElement {
            author: "mira"
            time: "08:24"
            body: "That is the dream. Configuration you can understand in one sitting."
            kind: "message"
        }
        ListElement {
            author: "sol"
            time: "10:08"
            body: "Does anyone use a vertical monitor alongside the main display?"
            kind: "message"
        }
    }

    ListModel {
        id: ricingMessages
        ListElement { author: ""; time: ""; body: "Yesterday"; kind: "event" }
        ListElement {
            author: "anna"
            time: "18:10"
            body: "Muted colors, one strong accent, and enough breathing room."
            kind: "message"
        }
        ListElement {
            author: "mira"
            time: "18:13"
            body: "Typography does more work than decoration ever will."
            kind: "message"
        }
        ListElement {
            author: "dax"
            time: "18:20"
            body: "Dropped a new warm theme in the usual place. It looks great after sunset."
            kind: "message"
        }
    }

    ListModel {
        id: helpMessages
        ListElement { author: ""; time: ""; body: "Today"; kind: "event" }
        ListElement {
            author: "mira"
            time: "09:11"
            body: "Tip: include the command output and the exact behavior you expected."
            kind: "message"
        }
        ListElement {
            author: "sol"
            time: "09:15"
            body: "That made my monitor issue much easier to diagnose. Thanks."
            kind: "message"
        }
    }

    ListModel {
        id: annaMessages
        ListElement {
            author: ""
            time: ""
            body: "This is the beginning of your conversation with anna."
            kind: "event"
        }
        ListElement {
            author: "anna"
            time: "10:12"
            body: "The prototype already feels at home. Nice work."
            kind: "message"
        }
    }

    ListModel {
        id: daxMessages
        ListElement {
            author: ""
            time: ""
            body: "This is the beginning of your conversation with dax."
            kind: "event"
        }
        ListElement {
            author: "dax"
            time: "Yesterday"
            body: "Send me the build when the mock is ready."
            kind: "message"
        }
    }

    ListModel {
        id: miraMessages
        ListElement { author: ""; time: ""; body: "This is the beginning of your conversation with mira."; kind: "event" }
    }

    ListModel {
        id: solMessages
        ListElement { author: ""; time: ""; body: "This is the beginning of your conversation with sol."; kind: "event" }
    }

    ListModel {
        id: kaiMessages
        ListElement { author: ""; time: ""; body: "This is the beginning of your conversation with kai."; kind: "event" }
    }

    ListModel {
        id: noraMessages
        ListElement { author: ""; time: ""; body: "This is the beginning of your conversation with nora."; kind: "event" }
    }

    ListModel {
        id: teoMessages
        ListElement { author: ""; time: ""; body: "This is the beginning of your conversation with teo."; kind: "event" }
    }

    ListModel {
        id: lenaMessages
        ListElement { author: ""; time: ""; body: "This is the beginning of your conversation with lena."; kind: "event" }
    }

    ListModel {
        id: samMessages
        ListElement { author: ""; time: ""; body: "This is the beginning of your conversation with sam."; kind: "event" }
    }

    ListModel {
        id: ivyMessages
        ListElement { author: ""; time: ""; body: "This is the beginning of your conversation with ivy."; kind: "event" }
    }

    ListModel {
        id: maxMessages
        ListElement { author: ""; time: ""; body: "This is the beginning of your conversation with max."; kind: "event" }
    }

    ListModel {
        id: directConversations
        ListElement {
            conversation: "anna"
            directUnread: 1
            directMention: true
        }
        ListElement {
            conversation: "dax"
            directUnread: 0
            directMention: false
        }
    }

    ListModel {
        id: membersModel
        ListElement { nick: "anna"; status: "writing docs"; away: false }
        ListElement { nick: "dax"; status: "on #desktop"; away: false }
        ListElement { nick: "mira"; status: "making tea"; away: false }
        ListElement { nick: "sol"; status: "new here"; away: false }
        ListElement { nick: "fred"; status: "building Omairc"; away: false }
        ListElement { nick: "kai"; status: ""; away: false }
        ListElement { nick: "nora"; status: ""; away: false }
        ListElement { nick: "teo"; status: ""; away: true }
        ListElement { nick: "lena"; status: ""; away: true }
        ListElement { nick: "sam"; status: ""; away: true }
        ListElement { nick: "ivy"; status: ""; away: true }
        ListElement { nick: "max"; status: ""; away: true }
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        Rectangle {
            id: sidebar
            Layout.preferredWidth: win.scaledSize(244)
            Layout.minimumWidth: win.scaledSize(214)
            Layout.fillHeight: true
            color: win.panelColor

            Rectangle {
                anchors.right: parent.right
                width: 1
                height: parent.height
                color: win.dividerColor
            }

            Item {
                id: networkHeader
                anchors.top: parent.top
                anchors.left: parent.left
                anchors.right: parent.right
                height: win.scaledSize(72)

                Rectangle {
                    anchors.left: parent.left
                    anchors.leftMargin: win.scaledSize(18)
                    anchors.verticalCenter: parent.verticalCenter
                    width: win.scaledSize(34)
                    height: width
                    radius: win.scaledSize(10)
                    color: win.accentColor

                    Text {
                        anchors.centerIn: parent
                        text: "O"
                        color: "#ffffff"
                        font.family: "iA Writer Mono S"
                        font.bold: true
                        font.pixelSize: win.scaledSize(17)
                    }
                }

                Column {
                    anchors.left: parent.left
                    anchors.leftMargin: win.scaledSize(64)
                    anchors.right: parent.right
                    anchors.rightMargin: win.scaledSize(12)
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: win.scaledSize(2)

                    Text {
                        width: parent.width
                        text: "Omarchy IRC"
                        color: win.inkColor
                        elide: Text.ElideRight
                        font.family: "iA Writer Mono S"
                        font.bold: true
                        font.pixelSize: win.scaledSize(14)
                    }

                    Row {
                        spacing: win.scaledSize(6)

                        Rectangle {
                            anchors.verticalCenter: parent.verticalCenter
                            width: win.scaledSize(7)
                            height: width
                            radius: width / 2
                            color: "#69b978"
                        }

                        Text {
                            text: "mock connected"
                            color: win.mutedColor
                            font.family: "iA Writer Mono S"
                            font.pixelSize: win.scaledSize(10)
                        }
                    }
                }
            }

            Column {
                anchors.top: networkHeader.bottom
                anchors.left: parent.left
                anchors.right: parent.right
                spacing: 0

                Item {
                    width: parent.width
                    height: win.scaledSize(34)

                    Text {
                        anchors.left: parent.left
                        anchors.leftMargin: win.scaledSize(19)
                        anchors.verticalCenter: parent.verticalCenter
                        text: "CHANNELS"
                        color: win.mutedColor
                        font.family: "iA Writer Mono S"
                        font.bold: true
                        font.letterSpacing: win.scaledSize(0.8)
                        font.pixelSize: win.scaledSize(9)
                    }

                    Text {
                        anchors.right: parent.right
                        anchors.rightMargin: win.scaledSize(20)
                        anchors.verticalCenter: parent.verticalCenter
                        text: "+"
                        color: addChannelMouse.containsMouse ? win.inkColor : win.mutedColor
                        font.family: "iA Writer Mono S"
                        font.pixelSize: win.scaledSize(17)

                        MouseArea {
                            id: addChannelMouse
                            anchors.centerIn: parent
                            width: win.scaledSize(28)
                            height: width
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: mockToast.open()
                        }
                    }
                }

                ConversationRow {
                    width: sidebar.width
                    conversationName: "#omarchy"
                    unread: 0
                    mention: false
                }

                ConversationRow {
                    width: sidebar.width
                    conversationName: "#desktop"
                    unread: 3
                    mention: false
                }

                ConversationRow {
                    width: sidebar.width
                    conversationName: "#ricing"
                    unread: 12
                    mention: true
                }

                ConversationRow {
                    width: sidebar.width
                    conversationName: "#help"
                    unread: 0
                    mention: false
                }

                Item {
                    width: parent.width
                    height: win.scaledSize(44)

                    Text {
                        anchors.left: parent.left
                        anchors.leftMargin: win.scaledSize(19)
                        anchors.bottom: parent.bottom
                        anchors.bottomMargin: win.scaledSize(9)
                        text: "DIRECT MESSAGES"
                        color: win.mutedColor
                        font.family: "iA Writer Mono S"
                        font.bold: true
                        font.letterSpacing: win.scaledSize(0.8)
                        font.pixelSize: win.scaledSize(9)
                    }
                }

                Repeater {
                    model: directConversations

                    delegate: ConversationRow {
                        required property string conversation
                        required property int directUnread
                        required property bool directMention

                        width: sidebar.width
                        conversationName: conversation
                        unread: directUnread
                        mention: directMention
                        direct: true
                    }
                }
            }

            Rectangle {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                height: win.scaledSize(66)
                color: win.mixColors(win.panelColor, win.inkColor, win.darkMode ? 0.025 : 0.018)

                Rectangle {
                    anchors.top: parent.top
                    width: parent.width
                    height: 1
                    color: win.dividerColor
                }

                Rectangle {
                    anchors.left: parent.left
                    anchors.leftMargin: win.scaledSize(17)
                    anchors.verticalCenter: parent.verticalCenter
                    width: win.scaledSize(34)
                    height: width
                    radius: width / 2
                    color: win.mixColors(win.pageColor, win.nickColor("fred"), 0.24)

                    Text {
                        anchors.centerIn: parent
                        text: "F"
                        color: win.nickColor("fred")
                        font.family: "iA Writer Mono S"
                        font.bold: true
                        font.pixelSize: win.scaledSize(14)
                    }

                    Rectangle {
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        width: win.scaledSize(9)
                        height: width
                        radius: width / 2
                        color: "#69b978"
                        border.width: win.scaledSize(2)
                        border.color: win.panelColor
                    }
                }

                Column {
                    anchors.left: parent.left
                    anchors.leftMargin: win.scaledSize(63)
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: win.scaledSize(1)

                    Text {
                        text: "fred"
                        color: win.inkColor
                        font.family: "iA Writer Mono S"
                        font.bold: true
                        font.pixelSize: win.scaledSize(13)
                    }

                    Text {
                        text: "available"
                        color: win.mutedColor
                        font.family: "iA Writer Mono S"
                        font.pixelSize: win.scaledSize(10)
                    }
                }
            }
        }

        Item {
            id: conversation
            Layout.fillWidth: true
            Layout.fillHeight: true

            Item {
                id: conversationHeader
                anchors.top: parent.top
                anchors.left: parent.left
                anchors.right: parent.right
                height: win.scaledSize(72)

                Column {
                    anchors.left: parent.left
                    anchors.leftMargin: win.scaledSize(24)
                    anchors.right: win.currentConversationIsChannel ? peopleButton.left : parent.right
                    anchors.rightMargin: win.scaledSize(win.currentConversationIsChannel ? 18 : 24)
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: win.scaledSize(3)

                    Text {
                        width: parent.width
                        text: win.currentConversation
                        color: win.inkColor
                        elide: Text.ElideRight
                        font.family: "iA Writer Mono S"
                        font.bold: true
                        font.pixelSize: win.scaledSize(17)
                    }

                    Text {
                        width: parent.width
                        text: win.currentTopic
                        color: win.mutedColor
                        elide: Text.ElideRight
                        font.family: "iA Writer Mono S"
                        font.pixelSize: win.scaledSize(11)
                    }
                }

                Rectangle {
                    id: peopleButton
                    visible: win.currentConversationIsChannel
                    anchors.right: parent.right
                    anchors.rightMargin: win.scaledSize(19)
                    anchors.verticalCenter: parent.verticalCenter
                    width: win.scaledSize(74)
                    height: win.scaledSize(30)
                    radius: win.scaledSize(7)
                    color: peopleMouse.containsMouse || win.membersVisible
                        ? win.raisedColor : "transparent"
                    border.width: 1
                    border.color: win.membersVisible ? win.dividerColor : "transparent"

                    Text {
                        anchors.centerIn: parent
                        text: win.currentPeopleCount + " PEOPLE"
                        color: win.membersVisible ? win.inkColor : win.mutedColor
                        font.family: "iA Writer Mono S"
                        font.bold: true
                        font.pixelSize: win.scaledSize(9)
                    }

                    MouseArea {
                        id: peopleMouse
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: win.membersVisible = !win.membersVisible
                    }
                }

                Rectangle {
                    anchors.bottom: parent.bottom
                    width: parent.width
                    height: 1
                    color: win.dividerColor
                }
            }

            ListView {
                id: messageList
                anchors.top: conversationHeader.bottom
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: composerShell.top
                anchors.bottomMargin: win.scaledSize(12)
                clip: true
                spacing: 0
                model: win.activeMessages
                boundsBehavior: Flickable.StopAtBounds

                ScrollBar.vertical: ScrollBar {
                    policy: ScrollBar.AsNeeded
                }

                delegate: Item {
                    id: messageDelegate

                    required property string author
                    required property string time
                    required property string body
                    required property string kind

                    width: messageList.width
                    height: kind === "event"
                        ? win.scaledSize(42)
                        : Math.max(win.scaledSize(58), messageBody.implicitHeight + win.scaledSize(39))

                    Text {
                        visible: messageDelegate.kind === "event"
                        anchors.centerIn: parent
                        width: parent.width - win.scaledSize(48)
                        horizontalAlignment: Text.AlignHCenter
                        text: messageDelegate.body
                        color: win.mutedColor
                        elide: Text.ElideRight
                        font.family: "iA Writer Mono S"
                        font.pixelSize: win.scaledSize(10)
                    }

                    Rectangle {
                        visible: messageDelegate.kind !== "event"
                        anchors.left: parent.left
                        anchors.leftMargin: win.scaledSize(24)
                        anchors.top: parent.top
                        anchors.topMargin: win.scaledSize(9)
                        width: win.scaledSize(34)
                        height: width
                        radius: width / 2
                        color: win.mixColors(
                            win.pageColor,
                            win.nickColor(messageDelegate.author),
                            win.darkMode ? 0.23 : 0.16)

                        Text {
                            anchors.centerIn: parent
                            text: win.initials(messageDelegate.author)
                            color: win.nickColor(messageDelegate.author)
                            font.family: "iA Writer Mono S"
                            font.bold: true
                            font.pixelSize: win.scaledSize(13)
                        }
                    }

                    Row {
                        visible: messageDelegate.kind !== "event"
                        anchors.left: parent.left
                        anchors.leftMargin: win.scaledSize(70)
                        anchors.top: parent.top
                        anchors.topMargin: win.scaledSize(8)
                        spacing: win.scaledSize(9)

                        Text {
                            text: messageDelegate.author
                            color: win.nickColor(messageDelegate.author)
                            font.family: "iA Writer Mono S"
                            font.bold: true
                            font.pixelSize: win.scaledSize(12)
                        }

                        Text {
                            anchors.baseline: parent.children[0].baseline
                            text: messageDelegate.time
                            color: win.mutedColor
                            font.family: "iA Writer Mono S"
                            font.pixelSize: win.scaledSize(9)
                        }
                    }

                    Text {
                        id: messageBody
                        visible: messageDelegate.kind !== "event"
                        anchors.left: parent.left
                        anchors.leftMargin: win.scaledSize(70)
                        anchors.right: parent.right
                        anchors.rightMargin: win.scaledSize(34)
                        anchors.top: parent.top
                        anchors.topMargin: win.scaledSize(29)
                        text: messageDelegate.body
                        color: messageDelegate.kind === "action" ? win.mutedColor : win.inkColor
                        wrapMode: Text.Wrap
                        font.family: "iA Writer Mono S"
                        font.italic: messageDelegate.kind === "action"
                        font.pixelSize: win.scaledSize(13)
                        lineHeight: 1.35
                    }
                }

                Component.onCompleted: positionViewAtEnd()
            }

            Rectangle {
                id: composerShell
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                anchors.leftMargin: win.scaledSize(20)
                anchors.rightMargin: win.scaledSize(20)
                anchors.bottomMargin: win.scaledSize(20)
                height: Math.max(win.scaledSize(46), Math.min(win.scaledSize(112),
                    composer.contentHeight + win.scaledSize(20)))
                radius: win.scaledSize(10)
                color: win.panelColor
                border.width: 1
                border.color: composer.activeFocus ? win.accentColor : win.dividerColor

                TextArea {
                    id: composer
                    anchors.left: parent.left
                    anchors.right: sendButton.left
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    anchors.leftMargin: win.scaledSize(8)
                    anchors.rightMargin: win.scaledSize(8)
                    verticalAlignment: TextEdit.AlignVCenter
                    color: win.inkColor
                    selectionColor: win.selectionColor
                    selectedTextColor: "#ffffff"
                    wrapMode: TextEdit.Wrap
                    font.family: "iA Writer Mono S"
                    font.pixelSize: win.scaledSize(13)
                    leftPadding: win.scaledSize(8)
                    rightPadding: win.scaledSize(8)
                    topPadding: Math.max(win.scaledSize(8),
                        (height - contentHeight) / 2)
                    bottomPadding: topPadding
                    background: Item {}

                    Keys.onPressed: function(event) {
                        if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter)
                                && !(event.modifiers & Qt.ShiftModifier)) {
                            win.sendMessage();
                            event.accepted = true;
                        }
                    }
                }

                Rectangle {
                    id: sendButton
                    anchors.right: parent.right
                    anchors.rightMargin: win.scaledSize(8)
                    anchors.verticalCenter: parent.verticalCenter
                    width: win.scaledSize(54)
                    height: win.scaledSize(30)
                    radius: win.scaledSize(7)
                    color: composer.text.trim().length > 0
                        ? (sendMouse.containsMouse
                            ? win.mixColors(win.accentColor, win.inkColor, 0.14)
                            : win.accentColor)
                        : win.raisedColor

                    Text {
                        anchors.centerIn: parent
                        text: "SEND"
                        color: composer.text.trim().length > 0 ? "#ffffff" : win.mutedColor
                        font.family: "iA Writer Mono S"
                        font.bold: true
                        font.pixelSize: win.scaledSize(9)
                    }

                    MouseArea {
                        id: sendMouse
                        anchors.fill: parent
                        enabled: composer.text.trim().length > 0
                        hoverEnabled: true
                        cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                        onClicked: win.sendMessage()
                    }
                }
            }

        }

        Rectangle {
            id: membersPanel
            visible: win.currentConversationIsChannel
                && win.membersVisible
                && win.width >= win.scaledSize(980)
            Layout.preferredWidth: visible ? win.scaledSize(216) : 0
            Layout.minimumWidth: visible ? win.scaledSize(196) : 0
            Layout.fillHeight: true
            color: win.panelColor

            Rectangle {
                anchors.left: parent.left
                width: 1
                height: parent.height
                color: win.dividerColor
            }

            Text {
                id: membersHeading
                anchors.top: parent.top
                anchors.topMargin: win.scaledSize(23)
                anchors.left: parent.left
                anchors.leftMargin: win.scaledSize(20)
                text: "ONLINE - " + win.currentPeopleCount
                color: win.mutedColor
                font.family: "iA Writer Mono S"
                font.bold: true
                font.letterSpacing: win.scaledSize(0.7)
                font.pixelSize: win.scaledSize(9)
            }

            ListView {
                anchors.top: membersHeading.bottom
                anchors.topMargin: win.scaledSize(14)
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: mockNotice.top
                anchors.bottomMargin: win.scaledSize(10)
                clip: true
                model: win.currentPeopleCount
                boundsBehavior: Flickable.StopAtBounds

                delegate: Item {
                    id: memberDelegate

                    readonly property var memberData: win.memberDataFor(index)
                    readonly property string nick: memberData.nick
                    readonly property string status: memberData.status
                    readonly property bool away: memberData.away

                    width: ListView.view.width
                    height: win.scaledSize(43)

                    Rectangle {
                        anchors.fill: parent
                        anchors.leftMargin: win.scaledSize(8)
                        anchors.rightMargin: win.scaledSize(8)
                        radius: win.scaledSize(7)
                        color: memberMouse.containsMouse ? win.hoverColor : "transparent"
                    }

                    Rectangle {
                        anchors.left: parent.left
                        anchors.leftMargin: win.scaledSize(18)
                        anchors.verticalCenter: parent.verticalCenter
                        width: win.scaledSize(28)
                        height: width
                        radius: width / 2
                        color: win.mixColors(
                            win.pageColor,
                            win.nickColor(memberDelegate.nick),
                            win.darkMode ? 0.23 : 0.16)
                        opacity: memberDelegate.away ? 0.62 : 1

                        Text {
                            anchors.centerIn: parent
                            text: win.initials(memberDelegate.nick)
                            color: win.nickColor(memberDelegate.nick)
                            font.family: "iA Writer Mono S"
                            font.bold: true
                            font.pixelSize: win.scaledSize(11)
                        }

                        Rectangle {
                            anchors.right: parent.right
                            anchors.bottom: parent.bottom
                            width: win.scaledSize(7)
                            height: width
                            radius: width / 2
                            color: memberDelegate.away ? "#d6a552" : "#69b978"
                            border.width: win.scaledSize(2)
                            border.color: win.panelColor
                        }
                    }

                    Column {
                        anchors.left: parent.left
                        anchors.leftMargin: win.scaledSize(56)
                        anchors.right: parent.right
                        anchors.rightMargin: win.scaledSize(10)
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 0

                        Text {
                            width: parent.width
                            text: memberDelegate.nick
                            color: memberDelegate.away ? win.mutedColor : win.inkColor
                            elide: Text.ElideRight
                            font.family: "iA Writer Mono S"
                            font.bold: memberDelegate.nick === "fred"
                            font.pixelSize: win.scaledSize(12)
                        }

                        Text {
                            visible: memberDelegate.status.length > 0
                            width: parent.width
                            text: memberDelegate.status
                            color: win.mutedColor
                            elide: Text.ElideRight
                            font.family: "iA Writer Mono S"
                            font.pixelSize: win.scaledSize(9)
                        }
                    }

                    MouseArea {
                        id: memberMouse
                        anchors.fill: parent
                        enabled: memberDelegate.nick !== "fred"
                        hoverEnabled: true
                        cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                        onClicked: win.openDirectMessage(memberDelegate.nick)
                    }
                }
            }

            Rectangle {
                id: mockNotice
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                anchors.margins: win.scaledSize(14)
                height: win.scaledSize(66)
                radius: win.scaledSize(9)
                color: win.raisedColor

                Column {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.margins: win.scaledSize(12)
                    spacing: win.scaledSize(4)

                    Text {
                        text: "VISUAL PROTOTYPE"
                        color: win.accentColor
                        font.family: "iA Writer Mono S"
                        font.bold: true
                        font.pixelSize: win.scaledSize(9)
                    }

                    Text {
                        width: parent.width
                        text: "No network traffic. Everything here is local."
                        color: win.mutedColor
                        wrapMode: Text.Wrap
                        font.family: "iA Writer Mono S"
                        font.pixelSize: win.scaledSize(9)
                        lineHeight: 1.2
                    }
                }
            }
        }
    }

    Popup {
        id: mockToast
        x: win.scaledSize(22)
        y: win.scaledSize(116)
        width: win.scaledSize(200)
        height: win.scaledSize(58)
        padding: 0
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

        background: Rectangle {
            color: win.raisedColor
            border.width: 1
            border.color: win.dividerColor
            radius: win.scaledSize(9)
        }

        contentItem: Text {
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            text: "Joining channels comes with\nreal IRC connectivity."
            color: win.inkColor
            font.family: "iA Writer Mono S"
            font.pixelSize: win.scaledSize(10)
            lineHeight: 1.35
        }
    }

    property rect normalGeometry: Qt.rect(x, y, width, height)
    property bool wasMaximized: false

    function trackNormalGeometry() {
        if (visibility === Window.Windowed)
            normalGeometry = Qt.rect(x, y, width, height);
    }

    onXChanged: trackNormalGeometry()
    onYChanged: trackNormalGeometry()
    onWidthChanged: trackNormalGeometry()
    onHeightChanged: trackNormalGeometry()

    onVisibilityChanged: {
        if (visibility === Window.Maximized || visibility === Window.FullScreen)
            wasMaximized = true;
        else if (visibility === Window.Windowed)
            wasMaximized = false;
    }

    Component.onCompleted: {
        var geometry = backend.windowGeometry();
        if (geometry.valid) {
            x = geometry.x;
            y = geometry.y;
            width = geometry.width;
            height = geometry.height;
            if (geometry.maximized)
                showMaximized();
        } else {
            width = Math.round(1180 * backend.textScale);
            height = Math.round(760 * backend.textScale);
        }
    }

    Component.onDestruction: backend.saveWindowGeometry(
        normalGeometry.x,
        normalGeometry.y,
        normalGeometry.width,
        normalGeometry.height,
        wasMaximized)
}
