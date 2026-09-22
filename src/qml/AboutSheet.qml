import QtQuick

Rectangle {
    id: sheet

    required property OmaircStyle style
    property string appVersion: ""
    property var updateCheck: null

    readonly property bool opened: visible
    function open() { visible = true; }
    function close() { visible = false; }

    function updateStatusActivates() {
        var status = sheet.updateCheck.status;
        return status === "updateAvailable"
            || status === "readyToRestart"
            || status === "downloadFailed";
    }

    function activateUpdateStatus() {
        var checker = sheet.updateCheck;
        var status = checker.status;
        if (status === "readyToRestart") {
            checker.launchInstaller();
            return;
        }
        if ((status === "updateAvailable" || status === "downloadFailed")
                && checker.installerUpdateAvailable) {
            checker.download();
            return;
        }
        if (status === "updateAvailable" || status === "downloadFailed")
            sheet.urlRequested(checker.latestUrl);
    }

    signal closed()
    signal shown()
    signal urlRequested(string url)
    signal checkUpdatesRequested()

    objectName: "aboutSheet"
    visible: false
    color: sheet.style.overlayVeilColor
    onVisibleChanged: {
        if (visible) {
            Qt.callLater(function() {
                if (aboutOkButton)
                    aboutOkButton.forceActiveFocus();
            });
            shown();
            return;
        }
        closed();
    }

    MouseArea {
        id: aboutSheetClickSink
        objectName: "aboutSheetDimmer"
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.AllButtons
        onPressed: function(mouse) { mouse.accepted = true; }
        onClicked: function(mouse) {
            if (mouse.button !== Qt.LeftButton)
                return;
            Qt.callLater(function() {
                if (sheet)
                    sheet.close();
            });
        }
        onWheel: function(wheel) { wheel.accepted = true; }
    }

    Rectangle {
        id: aboutSheetCard
        objectName: "aboutSheetCard"
        anchors.centerIn: parent
        width: sheet.style.scaledSize(440)
        height: aboutSheetBody.implicitHeight
        radius: sheet.style.scaledSize(9)
        color: sheet.style.raisedColor
        border.width: 1
        border.color: sheet.style.dividerColor
        clip: true

        MouseArea {
            id: aboutSheetCardClickSink
            anchors.fill: parent
            onClicked: function(mouse) { mouse.accepted = true; }
        }

        Column {
            id: aboutSheetBody
            width: parent.width
            spacing: 0

        Column {
            width: parent.width
            leftPadding: sheet.style.scaledSize(22)
            rightPadding: sheet.style.scaledSize(22)
            topPadding: sheet.style.scaledSize(18)
            bottomPadding: sheet.style.scaledSize(16)
            spacing: sheet.style.scaledSize(14)

            Text {
                objectName: "aboutTitle"
                text: "About Omairc"
                color: sheet.style.mutedColor
                font.family: "iA Writer Mono S"
                font.pixelSize: sheet.style.scaledSize(11)
            }

            Item {
                width: parent.width - parent.leftPadding - parent.rightPadding
                height: Math.max(aboutBrand.implicitHeight, aboutLogo.height)

                Column {
                    id: aboutBrand
                    anchors.left: parent.left
                    anchors.right: aboutLogo.left
                    anchors.rightMargin: sheet.style.scaledSize(16)
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: sheet.style.scaledSize(8)

                    Text {
                        objectName: "aboutName"
                        text: "Omairc"
                        color: sheet.style.inkColor
                        font.family: "iA Writer Mono S"
                        font.bold: true
                        font.pixelSize: sheet.style.scaledSize(28)
                    }

                    Text {
                        objectName: "aboutVersion"
                        text: sheet.appVersion
                        color: sheet.style.mutedColor
                        font.family: "iA Writer Mono S"
                        font.pixelSize: sheet.style.scaledSize(13)
                    }

                    Rectangle {
                        id: aboutCheckUpdates
                        objectName: "aboutCheckUpdates"
                        width: aboutCheckUpdatesLabel.implicitWidth + sheet.style.scaledSize(18)
                        height: sheet.style.scaledSize(28)
                        radius: sheet.style.scaledSize(7)
                        activeFocusOnTab: true
                        Accessible.role: Accessible.Button
                        Accessible.name: "Check for Updates"
                        Accessible.onPressAction: sheet.checkUpdatesRequested()
                        enabled: sheet.updateCheck.status !== "checking"
                            && sheet.updateCheck.status !== "downloading"
                        color: aboutCheckUpdatesMouse.containsMouse || activeFocus
                            ? sheet.style.hoverColor : "transparent"
                        border.width: 1
                        border.color: activeFocus ? sheet.style.accentColor : sheet.style.dividerColor
                        Keys.onPressed: function(event) {
                            if (event.key === Qt.Key_Return
                                    || event.key === Qt.Key_Enter
                                    || event.key === Qt.Key_Space) {
                                sheet.checkUpdatesRequested();
                                event.accepted = true;
                            }
                        }

                        Text {
                            id: aboutCheckUpdatesLabel
                            anchors.centerIn: parent
                            text: "Check for Updates"
                            color: aboutCheckUpdates.enabled ? sheet.style.inkColor : sheet.style.mutedColor
                            font.family: "iA Writer Mono S"
                            font.pixelSize: sheet.style.scaledSize(11)
                        }

                        MouseArea {
                            id: aboutCheckUpdatesMouse
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: aboutCheckUpdates.enabled
                                ? Qt.PointingHandCursor : Qt.ArrowCursor
                            onClicked: {
                                aboutCheckUpdates.forceActiveFocus();
                                sheet.checkUpdatesRequested();
                            }
                        }
                    }
                }

                Image {
                    id: aboutLogo
                    objectName: "aboutLogo"
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    width: sheet.style.scaledSize(88)
                    height: width
                    source: "qrc:/icons/omairc.svg"
                    sourceSize.width: sheet.style.scaledSize(88)
                    sourceSize.height: sheet.style.scaledSize(88)
                    fillMode: Image.PreserveAspectFit
                    smooth: true
                }
            }

            Text {
                objectName: "aboutDescription"
                width: parent.width - parent.leftPadding - parent.rightPadding
                wrapMode: Text.WordWrap
                text: "Omairc is an Internet Relay Chat client for Omarchy. People use it to communicate, share, play, and work with each other on IRC networks around the world."
                color: sheet.style.inkColor
                font.family: "iA Writer Mono S"
                font.pixelSize: sheet.style.scaledSize(12)
            }

            Column {
                width: parent.width - parent.leftPadding - parent.rightPadding
                spacing: sheet.style.scaledSize(6)

                Text {
                    objectName: "aboutOpenSource"
                    width: parent.width
                    wrapMode: Text.WordWrap
                    text: "This project is open-source."
                    color: sheet.style.inkColor
                    font.family: "iA Writer Mono S"
                    font.pixelSize: sheet.style.scaledSize(12)
                }

                Text {
                    id: aboutGithubLink
                    objectName: "aboutGithubLink"
                    text: "View the source on GitHub"
                    color: aboutGithubMouse.containsMouse ? sheet.style.accentColor : sheet.style.inkColor
                    font.family: "iA Writer Mono S"
                    font.pixelSize: sheet.style.scaledSize(12)
                    font.underline: true
                    Accessible.role: Accessible.Link
                    Accessible.name: "View the source on GitHub"
                    Accessible.onPressAction: sheet.urlRequested(sheet.updateCheck.repoUrl)

                    MouseArea {
                        id: aboutGithubMouse
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: sheet.urlRequested(sheet.updateCheck.repoUrl)
                    }
                }

                Text {
                    id: aboutUpdateStatus
                    objectName: "aboutUpdateStatus"
                    width: parent.width
                    visible: sheet.updateCheck.message.length > 0
                    wrapMode: Text.WordWrap
                    text: sheet.updateCheck.message
                    color: aboutUpdateStatusMouse.enabled && aboutUpdateStatusMouse.containsMouse
                        ? sheet.style.accentColor : sheet.style.mutedColor
                    font.family: "iA Writer Mono S"
                    font.pixelSize: sheet.style.scaledSize(12)
                    font.underline: sheet.updateStatusActivates()
                    Accessible.role: sheet.updateStatusActivates()
                        ? Accessible.Link : Accessible.StaticText
                    Accessible.name: sheet.updateCheck.message
                    Accessible.onPressAction: sheet.activateUpdateStatus()

                    MouseArea {
                        id: aboutUpdateStatusMouse
                        anchors.fill: parent
                        enabled: sheet.updateStatusActivates()
                        hoverEnabled: true
                        cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                        onClicked: sheet.activateUpdateStatus()
                    }
                }
            }

            Item {
                width: parent.width - parent.leftPadding - parent.rightPadding
                height: sheet.style.scaledSize(30)

                Rectangle {
                    id: aboutOkButton
                    objectName: "aboutOk"
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: sheet.style.scaledSize(88)
                    height: sheet.style.scaledSize(30)
                    radius: sheet.style.scaledSize(7)
                    activeFocusOnTab: true
                    Accessible.role: Accessible.Button
                    Accessible.name: "OK"
                    Accessible.onPressAction: sheet.close()
                    color: aboutOkMouse.containsMouse || activeFocus
                        ? sheet.style.hoverColor : "transparent"
                    border.width: 1
                    border.color: activeFocus ? sheet.style.accentColor : sheet.style.dividerColor
                    Keys.onPressed: function(event) {
                        if (event.key === Qt.Key_Return
                                || event.key === Qt.Key_Enter
                                || event.key === Qt.Key_Space) {
                            sheet.close();
                            event.accepted = true;
                        }
                    }

                    Text {
                        anchors.centerIn: parent
                        text: "OK"
                        color: sheet.style.inkColor
                        font.family: "iA Writer Mono S"
                        font.pixelSize: sheet.style.scaledSize(11)
                    }

                    MouseArea {
                        id: aboutOkMouse
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: sheet.close()
                    }
                }
            }
        }

        Rectangle {
            width: parent.width
            height: 1
            color: sheet.style.dividerColor
        }

        Item {
            width: parent.width
            height: sheet.style.scaledSize(40)

            Text {
                objectName: "aboutCopyright"
                anchors.centerIn: parent
                text: "Copyright © 2026 Fredi Machado"
                color: sheet.style.mutedColor
                font.family: "iA Writer Mono S"
                font.pixelSize: sheet.style.scaledSize(10)
            }
        }
    }
}
}
