import QtQuick
import QtQuick.Controls

Popup {
    id: sheet

    required property OmaircStyle style
    property var inbox: null
    property int selectedIndex: 0

    property alias inboxList: inboxList

    signal stepRequested(int delta)
    signal activateRequested()
    signal rowActivated(int index)
    signal dismissRequested(int index)

    objectName: "inboxSheet"
    width: sheet.style.scaledSize(404)
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

    contentItem: Item {
        width: sheet.availableWidth
        implicitHeight: column.implicitHeight
        focus: true

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
            if (event.key === Qt.Key_Delete) {
                sheet.dismissRequested(sheet.selectedIndex);
                event.accepted = true;
                return;
            }
            if (event.key === Qt.Key_Tab) {
                event.accepted = true;
            }
        }

        Column {
            id: column
            spacing: sheet.style.scaledSize(8)
            width: parent.width

        Text {
            objectName: "inboxEmptyLabel"
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: "Nothing waiting"
            color: sheet.style.mutedColor
            font.family: "iA Writer Mono S"
            font.pixelSize: sheet.style.scaledSize(12)
            visible: inboxList.count === 0
        }

        ListView {
            id: inboxList
            objectName: "inboxList"
            width: parent.width
            height: count === 0 ? 0 : Math.min(contentHeight, sheet.style.scaledSize(252))
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            model: sheet.inbox
            currentIndex: sheet.selectedIndex
            highlightFollowsCurrentItem: true
            visible: count > 0

            delegate: Item {
                id: inboxDelegate
                required property int index
                required property string label
                required property string preview
                width: inboxList.width
                height: sheet.style.scaledSize(36)
                readonly property bool current: index === sheet.selectedIndex

                Rectangle {
                    anchors.fill: parent
                    radius: sheet.style.scaledSize(7)
                    color: inboxDelegate.current ? sheet.style.hoverColor : "transparent"
                }

                Column {
                    anchors.left: parent.left
                    anchors.right: dismissHit.left
                    anchors.leftMargin: sheet.style.scaledSize(8)
                    anchors.rightMargin: sheet.style.scaledSize(4)
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: sheet.style.scaledSize(1)

                    Text {
                        width: parent.width
                        text: inboxDelegate.label
                        color: inboxDelegate.current ? sheet.style.inkColor : sheet.style.mutedColor
                        elide: Text.ElideRight
                        font.family: "iA Writer Mono S"
                        font.pixelSize: sheet.style.scaledSize(12)
                    }

                    Text {
                        width: parent.width
                        text: inboxDelegate.preview
                        color: sheet.style.mutedColor
                        elide: Text.ElideRight
                        font.family: "iA Writer Mono S"
                        font.pixelSize: sheet.style.scaledSize(10)
                        visible: inboxDelegate.preview.length > 0
                    }
                }

                Rectangle {
                    id: dismissHit
                    objectName: "inboxDismissButton"
                    anchors.right: parent.right
                    anchors.rightMargin: sheet.style.scaledSize(4)
                    anchors.verticalCenter: parent.verticalCenter
                    width: sheet.style.scaledSize(44)
                    height: sheet.style.scaledSize(24)
                    radius: sheet.style.scaledSize(5)
                    color: dismissMouse.containsMouse ? sheet.style.hoverColor : "transparent"
                    border.width: 1
                    border.color: sheet.style.dividerColor

                    Text {
                        anchors.centerIn: parent
                        text: "delete"
                        color: dismissMouse.containsMouse ? sheet.style.inkColor : sheet.style.mutedColor
                        font.family: "iA Writer Mono S"
                        font.pixelSize: sheet.style.scaledSize(10)
                    }

                    MouseArea {
                        id: dismissMouse
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: sheet.dismissRequested(inboxDelegate.index)
                    }
                }

                MouseArea {
                    anchors.left: parent.left
                    anchors.right: dismissHit.left
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: sheet.rowActivated(inboxDelegate.index)
                }
            }
        }
        }
    }
}
