import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: sheet

    required property OmaircStyle style
    property var connection: null
    property var irc: null
    property string currentTab: "connection"
    property bool removeArmed: false

    property alias connectionNameField: connectionNameField
    property alias connectionNickField: connectionNickField
    property alias connectionPassword: connectionPassword
    property alias connectionNickServ: connectionNickServ
    property alias connectionTabs: connectionTabs
    property alias clickSink: connectionSheetClickSink
    property alias cardClickSink: sheetCardClickSink

    signal opened()
    signal closed()
    signal dimmerClicked(var mouse)
    signal cardClicked(var mouse)
    signal tabSelected(string tabName)
    signal tabStepRequested(int direction)
    signal networkSelected(string networkId)
    signal submitRequested()
    signal addNetworkRequested()
    signal discardRequested()
    signal removeRequested()
    signal disconnectRequested()
    signal applyKeyRequested(var event)
    signal passwordEdited()
    signal nickServEdited()
    signal shortcutsRequested()

    objectName: "connectionSheet"
    color: sheet.style.overlayVeilColor
    onVisibleChanged: {
        if (!visible) {
            closed();
            return;
        }
        opened();
    }

    readonly property int networkChoiceStopCount: networkChoiceRepeater
        ? networkChoiceRepeater.count + (connectionAddNetwork.visible ? 1 : 0) : 0

    function focusNetworkChoiceStop(stop) {
        var total = sheet.networkChoiceStopCount;
        if (total <= 0)
            return;
        var clamped = Math.max(0, Math.min(stop, total - 1));
        var target = clamped < networkChoiceRepeater.count
            ? networkChoiceRepeater.itemAt(clamped)
            : connectionAddNetwork;
        if (!target)
            return;
        target.forceActiveFocus();
    }

    function revealInScroll(flick, item) {
        if (!flick || !item)
            return;
        var margin = sheet.style.scaledSize(6);
        var top = item.y - margin;
        var bottom = item.y + item.height + margin;
        var target = flick.contentY;
        if (top < flick.contentY)
            target = top;
        else if (bottom > flick.contentY + flick.height)
            target = bottom - flick.height;
        var lowest = Math.max(0, flick.contentHeight - flick.height);
        flick.contentY = Math.max(0, Math.min(target, lowest));
    }

    MouseArea {
        id: connectionSheetClickSink
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.AllButtons
        onPressed: function(mouse) { mouse.accepted = true; }
        onClicked: function(mouse) {
            if (mouse.button !== Qt.LeftButton)
                return;
            sheet.dimmerClicked(mouse);
        }
        onWheel: function(wheel) { wheel.accepted = true; }
    }

    Rectangle {
        id: connectionSheetCard
        objectName: "connectionSheetCard"
        anchors.centerIn: parent
        width: Math.min(sheet.style.scaledSize(860), parent.width - sheet.style.scaledSize(40))
        height: Math.min(sheet.style.scaledSize(720), parent.height - sheet.style.scaledSize(40))
        radius: sheet.style.scaledSize(10)
        color: sheet.style.raisedColor
        border.width: 1
        border.color: sheet.style.dividerColor

        MouseArea {
            id: sheetCardClickSink
            anchors.fill: parent
            onClicked: function(mouse) {
                sheet.cardClicked(mouse);
                mouse.accepted = true;
            }
        }

        ColumnLayout {
            anchors.fill: parent
            spacing: 0

            Item {
                id: connectionTabsHeader
                objectName: "connectionTabsHeader"
                Layout.fillWidth: true
                Layout.preferredHeight: connectionTabConnection.height

                Row {
                    id: connectionTabs
                    objectName: "connectionTabs"
                    anchors.left: parent.left
                    anchors.leftMargin: sheet.style.scaledSize(10)
                    anchors.bottom: parent.bottom
                    spacing: sheet.style.scaledSize(2)

                    ConnectionTabButton {
                        id: connectionTabConnection
                        style: sheet.style
                        tabName: "connection"
                        label: "Connection"
                        glyph: "#"
                        current: sheet.currentTab === "connection"
                        onSelected: function(tabName) {
                            sheet.tabSelected(tabName);
                        }
                        onStepRequested: function(direction) {
                            sheet.tabStepRequested(direction);
                        }
                    }

                    ConnectionTabButton {
                        id: connectionTabPreferences
                        style: sheet.style
                        tabName: "preferences"
                        label: "Preferences"
                        current: sheet.currentTab === "preferences"
                        onSelected: function(tabName) {
                            sheet.tabSelected(tabName);
                        }
                        onStepRequested: function(direction) {
                            sheet.tabStepRequested(direction);
                        }
                    }
                }
            }

            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: 1
                color: sheet.style.dividerColor
            }

            ColumnLayout {
                id: connectionTab
                objectName: "connectionTab"
                Layout.fillWidth: true
                Layout.fillHeight: true
                visible: sheet.currentTab === "connection"
                enabled: visible
                spacing: 0

                RowLayout {
                    id: connectionBody
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    Layout.leftMargin: sheet.style.scaledSize(18)
                    Layout.rightMargin: sheet.style.scaledSize(18)
                    Layout.topMargin: sheet.style.scaledSize(14)
                    Layout.bottomMargin: sheet.style.scaledSize(12)
                    spacing: sheet.style.scaledSize(16)

                    ColumnLayout {
                        id: networkRail
                        objectName: "networkChoiceList"
                        Layout.fillWidth: false
                        Layout.preferredWidth: sheet.style.scaledSize(170)
                        Layout.maximumWidth: sheet.style.scaledSize(170)
                        Layout.minimumWidth: sheet.style.scaledSize(150)
                        Layout.fillHeight: true
                        spacing: sheet.style.scaledSize(6)

                        Text {
                            id: networkRailLabel
                            Layout.fillWidth: true
                            text: "Networks"
                            color: sheet.style.mutedColor
                            font.family: "iA Writer Mono S"
                            font.bold: true
                            font.pixelSize: sheet.style.scaledSize(10)
                        }

                        Flickable {
                            id: networkChoiceScroll
                            objectName: "networkChoiceScroll"
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            Layout.minimumHeight: sheet.style.scaledSize(32)
                            clip: true
                            contentWidth: width
                            contentHeight: networkChoiceColumn.implicitHeight
                            boundsBehavior: Flickable.StopAtBounds
                            ScrollBar.vertical: ScrollBar {
                                objectName: "networkChoiceScrollBar"
                                policy: networkChoiceScroll.contentHeight > networkChoiceScroll.height
                                    ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
                            }

                            Column {
                                id: networkChoiceColumn
                                width: networkChoiceScroll.width
                                spacing: sheet.style.scaledSize(6)

                                Repeater {
                                    id: networkChoiceRepeater
                                    objectName: "networkChoiceRepeater"
                                    model: sheet.connection && sheet.connection.networks
                                        ? sheet.connection.networks : null
                                    delegate: Rectangle {
                                        id: networkRow
                                        required property string networkId
                                        required property string displayName
                                        required property bool selected
                                        required property int index
                                        property bool applyArmed: false
                                        width: networkRail.width
                                        height: sheet.style.scaledSize(32)
                                        radius: sheet.style.scaledSize(6)
                                        color: selected
                                            ? sheet.style.mixColors(sheet.style.selectionColor, sheet.style.raisedColor,
                                                            sheet.style.darkMode ? 0.45 : 0.35)
                                            : (choiceMouse.containsMouse
                                                || activeFocus
                                                ? sheet.style.hoverColor : "transparent")
                                        objectName: "networkChoice-" + networkId
                                        activeFocusOnTab: true
                                        Accessible.role: Accessible.Button
                                        Accessible.name: displayName
                                        Accessible.onPressAction: sheet.networkSelected(networkId)
                                        onActiveFocusChanged: {
                                            if (activeFocus)
                                                sheet.revealInScroll(networkChoiceScroll, networkRow);
                                            else
                                                applyArmed = false;
                                        }
                                        Keys.onPressed: function(event) {
                                            if (event.key === Qt.Key_Down) {
                                                sheet.focusNetworkChoiceStop(index + 1);
                                                event.accepted = true;
                                                return;
                                            }
                                            if (event.key === Qt.Key_Up) {
                                                sheet.focusNetworkChoiceStop(index - 1);
                                                event.accepted = true;
                                                return;
                                            }
                                            if (event.key === Qt.Key_Home) {
                                                sheet.focusNetworkChoiceStop(0);
                                                event.accepted = true;
                                                return;
                                            }
                                            if (event.key === Qt.Key_End) {
                                                sheet.focusNetworkChoiceStop(
                                                    sheet.networkChoiceStopCount - 1);
                                                event.accepted = true;
                                                return;
                                            }
                                            if (event.key === Qt.Key_Space) {
                                                sheet.networkSelected(networkId);
                                                event.accepted = true;
                                                return;
                                            }
                                            if (event.key !== Qt.Key_Return
                                                    && event.key !== Qt.Key_Enter)
                                                return;
                                            // Ctrl+Enter is handled by the
                                            // window-level apply shortcut.
                                            if (event.modifiers & Qt.ControlModifier)
                                                return;
                                            if (selected && applyArmed)
                                                sheet.submitRequested();
                                            else {
                                                sheet.networkSelected(networkId);
                                                applyArmed = true;
                                            }
                                            event.accepted = true;
                                        }

                                        Text {
                                            anchors.fill: parent
                                            anchors.leftMargin: sheet.style.scaledSize(8)
                                            anchors.rightMargin: sheet.style.scaledSize(8)
                                            text: displayName
                                            color: sheet.style.inkColor
                                            elide: Text.ElideRight
                                            verticalAlignment: Text.AlignVCenter
                                            font.family: "iA Writer Mono S"
                                            font.pixelSize: sheet.style.scaledSize(11)
                                        }

                                        MouseArea {
                                            id: choiceMouse
                                            anchors.fill: parent
                                            hoverEnabled: true
                                            cursorShape: Qt.PointingHandCursor
                                            onClicked: {
                                                sheet.networkSelected(networkId);
                                                parent.forceActiveFocus();
                                            }
                                        }
                                    }
                                }

                                Rectangle {
                                    id: connectionAddNetwork
                                    objectName: "connectionAddNetwork"
                                    width: networkChoiceColumn.width
                                    height: sheet.style.scaledSize(28)
                                    radius: sheet.style.scaledSize(6)
                                    visible: sheet.connection ? sheet.connection.canAdd : false
                                    activeFocusOnTab: visible
                                    Accessible.role: Accessible.Button
                                    Accessible.name: "Add network"
                                    Accessible.onPressAction: sheet.addNetworkRequested()
                                    color: addNetworkMouse.containsMouse || activeFocus
                                        ? sheet.style.hoverColor : "transparent"
                                    border.width: activeFocus ? 1 : 0
                                    border.color: sheet.style.accentColor
                                    onActiveFocusChanged: {
                                        if (activeFocus)
                                            sheet.revealInScroll(networkChoiceScroll,
                                                               connectionAddNetwork);
                                    }
                                    Keys.onPressed: function(event) {
                                        if (event.key === Qt.Key_Up) {
                                            sheet.focusNetworkChoiceStop(
                                                sheet.networkChoiceStopCount - 2);
                                            event.accepted = true;
                                            return;
                                        }
                                        if (event.key === Qt.Key_Return
                                                || event.key === Qt.Key_Enter
                                                || event.key === Qt.Key_Space) {
                                            sheet.addNetworkRequested();
                                            event.accepted = true;
                                        }
                                    }

                                    Text {
                                        anchors.centerIn: parent
                                        text: "+ Add network"
                                        color: sheet.style.mutedColor
                                        font.family: "iA Writer Mono S"
                                        font.pixelSize: sheet.style.scaledSize(11)
                                    }

                                    MouseArea {
                                        id: addNetworkMouse
                                        anchors.fill: parent
                                        hoverEnabled: true
                                        cursorShape: Qt.PointingHandCursor
                                        onClicked: {
                                            sheet.addNetworkRequested();
                                            parent.forceActiveFocus();
                                        }
                                    }
                                }
                            }
                        }

                        Rectangle {
                            id: connectionShortcutsHint
                            objectName: "connectionShortcutsHint"
                            Layout.fillWidth: true
                            Layout.preferredHeight: sheet.style.scaledSize(30)
                            radius: sheet.style.scaledSize(7)
                            color: shortcutsHintMouse.containsMouse
                                ? sheet.style.hoverColor : sheet.style.panelColor
                            border.width: 1
                            border.color: activeFocus ? sheet.style.accentColor : sheet.style.dividerColor
                            activeFocusOnTab: true
                            Accessible.role: Accessible.Button
                            Accessible.name: "Keyboard shortcuts"
                            Accessible.description: "Open the keyboard shortcuts sheet"
                            Accessible.onPressAction: sheet.shortcutsRequested()
                            Keys.onPressed: function(event) {
                                if (event.key === Qt.Key_Return
                                        || event.key === Qt.Key_Enter
                                        || event.key === Qt.Key_Space) {
                                    sheet.shortcutsRequested();
                                    event.accepted = true;
                                }
                            }

                            Text {
                                anchors.left: parent.left
                                anchors.leftMargin: sheet.style.scaledSize(9)
                                anchors.verticalCenter: parent.verticalCenter
                                text: "Shortcuts"
                                color: sheet.style.mutedColor
                                font.family: "iA Writer Mono S"
                                font.pixelSize: sheet.style.scaledSize(10)
                            }

                            Rectangle {
                                id: shortcutsKey
                                anchors.right: parent.right
                                anchors.rightMargin: sheet.style.scaledSize(7)
                                anchors.verticalCenter: parent.verticalCenter
                                width: shortcutsKeyLabel.implicitWidth + sheet.style.scaledSize(10)
                                height: sheet.style.scaledSize(18)
                                radius: sheet.style.scaledSize(4)
                                color: sheet.style.raisedColor
                                border.width: 1
                                border.color: sheet.style.dividerColor

                                Text {
                                    id: shortcutsKeyLabel
                                    objectName: "connectionShortcutsHintKeys"
                                    anchors.centerIn: parent
                                    text: sheet.style.shortcutKeys("Ctrl + /")
                                    color: sheet.style.inkColor
                                    font.family: "iA Writer Mono S"
                                    font.pixelSize: sheet.style.scaledSize(10)
                                }
                            }

                            MouseArea {
                                id: shortcutsHintMouse
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: sheet.shortcutsRequested()
                            }
                        }
                    }

                    Item {
                        id: connectionFormArea
                        objectName: "connectionFormArea"
                        Layout.fillWidth: true
                        Layout.fillHeight: true

                        Flickable {
                            id: sheetFlick
                            objectName: "sheetFlick"
                            anchors.fill: parent
                            contentWidth: width
                            contentHeight: sheetColumn.implicitHeight
                            clip: true
                            boundsBehavior: Flickable.StopAtBounds
                            ScrollBar.vertical: ScrollBar {
                                id: sheetFlickScrollBar
                                objectName: "sheetFlickScrollBar"
                                policy: sheetFlick.contentHeight > sheetFlick.height
                                    ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
                            }

                            Column {
                                id: sheetColumn
                                width: sheetFlick.width - sheetFlickScrollBar.width
                                spacing: sheet.style.scaledSize(10)

                                Text {
                                    text: "Connect"
                                    color: sheet.style.inkColor
                                    font.family: "iA Writer Mono S"
                                    font.bold: true
                                    font.pixelSize: sheet.style.scaledSize(15)
                                }

                                ConnectionField {
                                    style: sheet.style
                                    id: connectionNameField
                                    label: "Name"
                                    fieldObjectName: "connectionName"
                                    text: sheet.connection ? sheet.connection.name : ""
                                    onTextEdited: function(value) {
                                        if (sheet.connection)
                                            sheet.connection.name = value;
                                    }

                                    onApplyKeyRequested: function(event) {
                                        sheet.applyKeyRequested(event);
                                    }
                                }

                                Row {
                                    width: parent.width
                                    spacing: sheet.style.scaledSize(12)

                                    ConnectionField {
                                        style: sheet.style
                                        id: connectionHostField
                                        width: parent.width - sheet.style.scaledSize(104)
                                            - sheet.style.scaledSize(72) - sheet.style.scaledSize(24)
                                        label: "Host"
                                        fieldObjectName: "connectionHost"
                                        text: sheet.connection ? sheet.connection.host : ""
                                        onTextEdited: function(value) {
                                            if (sheet.connection)
                                                sheet.connection.host = value;
                                        }

                                        onApplyKeyRequested: function(event) {
                                            sheet.applyKeyRequested(event);
                                        }
                                    }

                                    ConnectionField {
                                        style: sheet.style
                                        width: sheet.style.scaledSize(104)
                                        label: "Port"
                                        fieldObjectName: "connectionPort"
                                        text: sheet.connection ? String(sheet.connection.port) : "6697"
                                        onTextEdited: function(value) {
                                            if (sheet.connection)
                                                sheet.connection.port = Number(value) || 0;
                                        }

                                        onApplyKeyRequested: function(event) {
                                            sheet.applyKeyRequested(event);
                                        }
                                    }

                                    Item {
                                        width: sheet.style.scaledSize(72)
                                        height: sheet.style.scaledSize(55)

                                        Column {
                                            anchors.left: parent.left
                                            anchors.right: parent.right
                                            anchors.bottom: parent.bottom
                                            spacing: sheet.style.scaledSize(4)

                                            Text {
                                                text: "TLS"
                                                color: sheet.style.mutedColor
                                                font.family: "iA Writer Mono S"
                                                font.pixelSize: sheet.style.scaledSize(10)
                                            }

                                            Switch {
                                                id: connectionTls
                                                objectName: "connectionTls"
                                                checked: sheet.connection ? sheet.connection.tlsEnabled : true
                                                Keys.onPressed: function(event) {
                                                    sheet.applyKeyRequested(event)
                                                }
                                                onToggled: {
                                                    if (sheet.connection)
                                                        sheet.connection.tlsEnabled = checked;
                                                }
                                            }
                                        }
                                    }
                                }

                                Row {
                                    width: parent.width
                                    spacing: sheet.style.scaledSize(12)

                                    ConnectionField {
                                        style: sheet.style
                                        id: connectionNickField
                                        width: Math.round((parent.width - sheet.style.scaledSize(12)) / 2)
                                        label: "Nick"
                                        fieldObjectName: "connectionNick"
                                        text: sheet.connection ? sheet.connection.nick : ""
                                        onTextEdited: function(value) {
                                            if (sheet.connection)
                                                sheet.connection.nick = value;
                                        }

                                        onApplyKeyRequested: function(event) {
                                            sheet.applyKeyRequested(event);
                                        }
                                    }

                                    ConnectionField {
                                        style: sheet.style
                                        width: parent.width - connectionNickField.width
                                            - sheet.style.scaledSize(12)
                                        label: "Username"
                                        fieldObjectName: "connectionUsername"
                                        text: sheet.connection ? sheet.connection.username : ""
                                        onTextEdited: function(value) {
                                            if (sheet.connection)
                                                sheet.connection.username = value;
                                        }

                                        onApplyKeyRequested: function(event) {
                                            sheet.applyKeyRequested(event);
                                        }
                                    }
                                }

                                ConnectionField {
                                    style: sheet.style
                                    label: "Real name"
                                    fieldObjectName: "connectionRealname"
                                    text: sheet.connection ? sheet.connection.realname : ""
                                    onTextEdited: function(value) {
                                        if (sheet.connection)
                                            sheet.connection.realname = value;
                                    }

                                    onApplyKeyRequested: function(event) {
                                        sheet.applyKeyRequested(event);
                                    }
                                }

                                ConnectionField {
                                    style: sheet.style
                                    label: "Autojoin"
                                    fieldObjectName: "connectionAutojoin"
                                    text: sheet.connection ? sheet.connection.autojoin : ""
                                    onTextEdited: function(value) {
                                        if (sheet.connection)
                                            sheet.connection.autojoin = value;
                                    }

                                    onApplyKeyRequested: function(event) {
                                        sheet.applyKeyRequested(event);
                                    }
                                }

                                Row {
                                    width: parent.width
                                    spacing: sheet.style.scaledSize(10)

                                    Switch {
                                        id: connectionConnectOnStartup
                                        objectName: "connectionConnectOnStartup"
                                        Keys.onPressed: function(event) {
                                            sheet.applyKeyRequested(event)
                                        }
                                        onToggled: {
                                            if (sheet.connection)
                                                sheet.connection.connectOnStartup = checked;
                                        }
                                    }

                                    Binding {
                                        target: connectionConnectOnStartup
                                        property: "checked"
                                        value: sheet.connection ? sheet.connection.connectOnStartup : false
                                        restoreMode: Binding.RestoreBinding
                                    }

                                    Item {
                                        width: parent.width - connectionConnectOnStartup.width
                                            - sheet.style.scaledSize(10)
                                        height: connectionConnectOnStartup.height

                                        Text {
                                            anchors.left: parent.left
                                            anchors.verticalCenter: parent.verticalCenter
                                            text: "Connect automatically on startup"
                                            color: sheet.style.inkColor
                                            font.family: "iA Writer Mono S"
                                            font.pixelSize: sheet.style.scaledSize(11)
                                        }
                                    }
                                }

                                Rectangle {
                                    width: parent.width
                                    height: 1
                                    color: sheet.style.dividerColor
                                }

                                ConnectionField {
                                    style: sheet.style
                                    id: connectionPassword
                                    label: "Server password"
                                    badge: "PASS"
                                    help: "Used as the SASL secret when no NickServ password is set; otherwise sent as PASS while connecting."
                                    fieldObjectName: "connectionPassword"
                                    secret: true
                                    onTextEdited: function(value) {
                                        sheet.passwordEdited();
                                    }

                                    onApplyKeyRequested: function(event) {
                                        sheet.applyKeyRequested(event);
                                    }
                                }

                                ConnectionField {
                                    style: sheet.style
                                    id: connectionNickServ
                                    label: "NickServ password"
                                    help: "Preferred SASL secret. Sent as NickServ IDENTIFY when SASL did not succeed."
                                    fieldObjectName: "connectionNickServ"
                                    secret: true
                                    onTextEdited: function(value) {
                                        sheet.nickServEdited();
                                    }

                                    onApplyKeyRequested: function(event) {
                                        sheet.applyKeyRequested(event);
                                    }
                                }

                                Text {
                                    id: connectionCredentialStatus
                                    objectName: "connectionCredentialStatus"
                                    width: parent.width
                                    visible: !!(sheet.connection && sheet.connection.credentialStatus)
                                    text: (sheet.connection && sheet.connection.credentialStatus)
                                          ? sheet.connection.credentialStatus : ""
                                    color: sheet.style.mutedColor
                                    wrapMode: Text.Wrap
                                    font.family: "iA Writer Mono S"
                                    font.pixelSize: sheet.style.scaledSize(10)
                                }

                                Row {
                                    width: parent.width
                                    spacing: sheet.style.scaledSize(16)

                                    Text {
                                        id: connectionForgetPassword
                                        objectName: "connectionForgetPassword"
                                        visible: !!(sheet.connection && sheet.connection.canForgetPassword)
                                        width: visible ? implicitWidth : 0
                                        text: "forget saved server password"
                                        color: sheet.style.accentColor
                                        font.family: "iA Writer Mono S"
                                        font.pixelSize: sheet.style.scaledSize(10)
                                        font.underline: activeFocus
                                        Accessible.role: Accessible.Button
                                        Accessible.name: "Forget saved server password"
                                        Accessible.description: "Remove the saved server password"
                                        activeFocusOnTab: visible
                                        Keys.onPressed: function(event) {
                                            if (event.key === Qt.Key_Return
                                                    || event.key === Qt.Key_Enter
                                                    || event.key === Qt.Key_Space) {
                                                forgetSavedPassword();
                                                event.accepted = true;
                                            }
                                        }
                                        Accessible.onPressAction: {
                                            forgetSavedPassword();
                                        }
                                        function forgetSavedPassword() {
                                            connectionPassword.text = "";
                                            sheet.connection.forgetPassword();
                                            sheet.connection.removeStoredPassword();
                                        }
                                        MouseArea {
                                            anchors.fill: parent
                                            hoverEnabled: true
                                            cursorShape: Qt.PointingHandCursor
                                            onClicked: {
                                                parent.forgetSavedPassword();
                                            }
                                        }
                                    }

                                    Text {
                                        id: connectionForgetNickServ
                                        objectName: "connectionForgetNickServ"
                                        visible: !!(sheet.connection && sheet.connection.canForgetNickServ)
                                        width: visible ? implicitWidth : 0
                                        text: "forget saved NickServ password"
                                        color: sheet.style.accentColor
                                        font.family: "iA Writer Mono S"
                                        font.pixelSize: sheet.style.scaledSize(10)
                                        font.underline: activeFocus
                                        Accessible.role: Accessible.Button
                                        Accessible.name: "Forget saved NickServ password"
                                        Accessible.description: "Remove the saved NickServ password"
                                        activeFocusOnTab: visible
                                        Keys.onPressed: function(event) {
                                            if (event.key === Qt.Key_Return
                                                    || event.key === Qt.Key_Enter
                                                    || event.key === Qt.Key_Space) {
                                                forgetSavedNickServ();
                                                event.accepted = true;
                                            }
                                        }
                                        Accessible.onPressAction: {
                                            forgetSavedNickServ();
                                        }
                                        function forgetSavedNickServ() {
                                            connectionNickServ.text = "";
                                            sheet.connection.forgetNickServ();
                                            sheet.connection.removeStoredNickServ();
                                        }
                                        MouseArea {
                                            anchors.fill: parent
                                            hoverEnabled: true
                                            cursorShape: Qt.PointingHandCursor
                                            onClicked: {
                                                parent.forgetSavedNickServ();
                                            }
                                        }
                                    }
                                }

                                Text {
                                    id: connectionProblem
                                    objectName: "connectionProblem"
                                    width: parent.width
                                    visible: !!(sheet.connection && sheet.connection.problem)
                                    text: (sheet.connection && sheet.connection.problem)
                                          ? sheet.connection.problem : ""
                                    color: sheet.style.accentColor
                                    wrapMode: Text.Wrap
                                    font.family: "iA Writer Mono S"
                                    font.pixelSize: sheet.style.scaledSize(11)
                                }
                            }
                        }
                    }
                }

                Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 1
                    color: sheet.style.dividerColor
                }

                Item {
                    id: connectionFooter
                    objectName: "connectionFooter"
                    Layout.fillWidth: true
                    Layout.preferredHeight: sheet.style.scaledSize(52)

                    Rectangle {
                        id: connectionFooterDot
                        anchors.left: parent.left
                        anchors.leftMargin: sheet.style.scaledSize(18)
                        anchors.verticalCenter: parent.verticalCenter
                        width: sheet.style.scaledSize(6)
                        height: width
                        radius: width / 2
                        color: sheet.style.accentColor
                    }

                    Text {
                        id: connectionFooterName
                        objectName: "connectionFooterNetwork"
                        anchors.left: connectionFooterDot.right
                        anchors.leftMargin: sheet.style.scaledSize(8)
                        anchors.right: sheetActions.left
                        anchors.rightMargin: sheet.style.scaledSize(16)
                        anchors.verticalCenter: parent.verticalCenter
                        text: sheet.connection ? sheet.connection.displayName : ""
                        color: sheet.style.mutedColor
                        elide: Text.ElideRight
                        font.family: "iA Writer Mono S"
                        font.pixelSize: sheet.style.scaledSize(11)
                    }

                    Row {
                        id: sheetActions
                        anchors.right: parent.right
                        anchors.rightMargin: sheet.style.scaledSize(18)
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: sheet.style.scaledSize(8)

                        Rectangle {
                            objectName: "connectionRemove"
                            visible: sheet.connection ? sheet.connection.canRemove : false
                            width: visible ? sheet.style.scaledSize(sheet.removeArmed ? 148 : 88) : 0
                            height: sheet.style.scaledSize(30)
                            radius: sheet.style.scaledSize(7)
                            activeFocusOnTab: visible
                            Accessible.role: Accessible.Button
                            Accessible.name: sheet.removeArmed
                                ? "Confirm remove " + (sheet.connection ? sheet.connection.displayName : "")
                                : "Remove"
                            Accessible.onPressAction: sheet.removeRequested()
                            color: removeMouse.containsMouse || activeFocus
                                ? sheet.style.hoverColor : "transparent"
                            border.width: 1
                            border.color: activeFocus ? sheet.style.accentColor : sheet.style.dividerColor
                            Keys.onPressed: function(event) {
                                if (event.key === Qt.Key_Return
                                        || event.key === Qt.Key_Enter
                                        || event.key === Qt.Key_Space) {
                                    sheet.removeRequested();
                                    event.accepted = true;
                                }
                            }

                            Text {
                                anchors.centerIn: parent
                                text: sheet.removeArmed
                                    ? "Remove " + (sheet.connection ? sheet.connection.displayName : "") + "?"
                                    : "Remove"
                                color: sheet.style.accentColor
                                elide: Text.ElideRight
                                width: parent.width - sheet.style.scaledSize(8)
                                horizontalAlignment: Text.AlignHCenter
                                font.family: "iA Writer Mono S"
                                font.pixelSize: sheet.style.scaledSize(11)
                            }

                            MouseArea {
                                id: removeMouse
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: {
                                    sheet.removeRequested();
                                    parent.forceActiveFocus();
                                }
                            }
                        }

                        Item { width: 1; height: 1 }

                        Rectangle {
                            objectName: "connectionDiscard"
                            width: sheet.style.scaledSize(88)
                            height: sheet.style.scaledSize(30)
                            radius: sheet.style.scaledSize(7)
                            activeFocusOnTab: true
                            Accessible.role: Accessible.Button
                            Accessible.name: "Discard"
                            Accessible.onPressAction: sheet.discardRequested()
                            color: discardMouse.containsMouse || activeFocus
                                ? sheet.style.hoverColor : "transparent"
                            border.width: 1
                            border.color: activeFocus ? sheet.style.accentColor : sheet.style.dividerColor
                            Keys.onPressed: function(event) {
                                if (event.key === Qt.Key_Return
                                        || event.key === Qt.Key_Enter
                                        || event.key === Qt.Key_Space) {
                                    sheet.discardRequested();
                                    event.accepted = true;
                                }
                            }

                            Text {
                                anchors.centerIn: parent
                                text: "Discard"
                                color: sheet.style.mutedColor
                                font.family: "iA Writer Mono S"
                                font.pixelSize: sheet.style.scaledSize(11)
                            }

                            MouseArea {
                                id: discardMouse
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: {
                                    sheet.discardRequested();
                                    parent.forceActiveFocus();
                                }
                            }
                        }

                        Rectangle {
                            objectName: "connectionDisconnect"
                            visible: sheet.connection ? sheet.connection.canDisconnect : false
                            width: visible ? sheet.style.scaledSize(108) : 0
                            height: sheet.style.scaledSize(30)
                            radius: sheet.style.scaledSize(7)
                            activeFocusOnTab: visible
                            Accessible.role: Accessible.Button
                            Accessible.name: "Disconnect"
                            Accessible.onPressAction: sheet.disconnectRequested()
                            color: disconnectMouse.containsMouse || activeFocus
                                ? sheet.style.hoverColor : "transparent"
                            border.width: 1
                            border.color: activeFocus ? sheet.style.accentColor : sheet.style.dividerColor
                            Keys.onPressed: function(event) {
                                if (event.key === Qt.Key_Return
                                        || event.key === Qt.Key_Enter
                                        || event.key === Qt.Key_Space) {
                                    sheet.disconnectRequested();
                                    event.accepted = true;
                                }
                            }

                            Text {
                                anchors.centerIn: parent
                                text: "Disconnect"
                                color: sheet.style.mutedColor
                                font.family: "iA Writer Mono S"
                                font.pixelSize: sheet.style.scaledSize(11)
                            }

                            MouseArea {
                                id: disconnectMouse
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: {
                                    sheet.disconnectRequested();
                                    parent.forceActiveFocus();
                                }
                            }
                        }

                        Rectangle {
                            objectName: "connectionApply"
                            width: sheet.style.scaledSize(88)
                            height: sheet.style.scaledSize(30)
                            radius: sheet.style.scaledSize(7)
                            activeFocusOnTab: true
                            Accessible.role: Accessible.Button
                            Accessible.name: "Apply"
                            Accessible.onPressAction: sheet.submitRequested()
                            color: sheet.connection && sheet.connection.problem.length === 0
                                ? sheet.style.accentColor : sheet.style.raisedColor
                            border.width: 1
                            border.color: activeFocus
                                ? (sheet.connection && sheet.connection.problem.length === 0
                                    ? sheet.style.inkColor : sheet.style.accentColor)
                                : "transparent"
                            Keys.onPressed: function(event) {
                                if (event.key === Qt.Key_Return
                                        || event.key === Qt.Key_Enter
                                        || event.key === Qt.Key_Space) {
                                    sheet.submitRequested();
                                    event.accepted = true;
                                }
                            }

                            Text {
                                anchors.centerIn: parent
                                text: "Apply"
                                color: sheet.connection && sheet.connection.problem.length === 0
                                    ? "#ffffff" : sheet.style.mutedColor
                                font.family: "iA Writer Mono S"
                                font.bold: true
                                font.pixelSize: sheet.style.scaledSize(11)
                            }

                            MouseArea {
                                anchors.fill: parent
                                cursorShape: sheet.connection && sheet.connection.problem.length === 0
                                    ? Qt.PointingHandCursor : Qt.ArrowCursor
                                onClicked: {
                                    parent.forceActiveFocus();
                                    sheet.submitRequested();
                                }
                            }
                        }
                    }
                }
            }

            Item {
                id: connectionPreferencesPanel
                objectName: "connectionPreferencesPanel"
                Layout.fillWidth: true
                Layout.fillHeight: true
                visible: sheet.currentTab === "preferences"
                enabled: visible

                Column {
                    anchors.left: parent.left
                    anchors.top: parent.top
                    anchors.leftMargin: sheet.style.scaledSize(18)
                    anchors.topMargin: sheet.style.scaledSize(18)
                    spacing: sheet.style.scaledSize(8)

                    Text {
                        text: "Preferences"
                        color: sheet.style.inkColor
                        font.family: "iA Writer Mono S"
                        font.bold: true
                        font.pixelSize: sheet.style.scaledSize(15)
                    }

                    Row {
                        spacing: sheet.style.scaledSize(10)

                        Switch {
                            id: connectionReopenDirects
                            objectName: "connectionReopenDirects"
                            Accessible.name: "Reopen direct messages on startup"
                            Keys.onPressed: function(event) {
                                sheet.applyKeyRequested(event)
                            }
                            onToggled: {
                                if (sheet.irc)
                                    sheet.irc.reopenDirectMessages = checked;
                            }
                        }

                        Binding {
                            target: connectionReopenDirects
                            property: "checked"
                            value: sheet.irc ? sheet.irc.reopenDirectMessages : true
                            restoreMode: Binding.RestoreBinding
                        }

                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: "Reopen direct messages on startup"
                            color: sheet.style.inkColor
                            font.family: "iA Writer Mono S"
                            font.pixelSize: sheet.style.scaledSize(11)
                        }
                    }

                    Row {
                        spacing: sheet.style.scaledSize(10)

                        Switch {
                            id: connectionLoadPeerAvatars
                            objectName: "connectionLoadPeerAvatars"
                            Accessible.name: "Show peer avatars"
                            Keys.onPressed: function(event) {
                                sheet.applyKeyRequested(event)
                            }
                            onToggled: {
                                if (sheet.irc)
                                    sheet.irc.loadPeerAvatars = checked;
                            }
                        }

                        Binding {
                            target: connectionLoadPeerAvatars
                            property: "checked"
                            value: sheet.irc ? sheet.irc.loadPeerAvatars : true
                            restoreMode: Binding.RestoreBinding
                        }

                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: "Show peer avatars"
                            color: sheet.style.inkColor
                            font.family: "iA Writer Mono S"
                            font.pixelSize: sheet.style.scaledSize(11)
                        }
                    }
                }
            }
        }
    }
}
