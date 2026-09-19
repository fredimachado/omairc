import QtQuick
import QtQuick.Controls

Popup {
    id: sheet

    required property OmaircStyle style
    property var matches: null
    property int selectedIndex: 0

    property alias jumpFilter: jumpFilter
    property alias jumpList: jumpList

    signal stepRequested(int delta)
    signal activateRequested()
    signal filterChanged()

    objectName: "jumpSheet"
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
            border.color: jumpFilter.activeFocus ? sheet.style.accentColor : sheet.style.dividerColor
            radius: sheet.style.scaledSize(7)

            TextField {
                id: jumpFilter
                objectName: "jumpFilter"
                Accessible.name: "Jump to conversation"
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
                objectName: "jumpFilterPlaceholder"
                z: 0
                anchors.left: parent.left
                anchors.leftMargin: sheet.style.scaledSize(8)
                anchors.verticalCenter: parent.verticalCenter
                text: "Jump to conversation…"
                color: sheet.style.mutedColor
                font.family: "iA Writer Mono S"
                font.pixelSize: sheet.style.scaledSize(13)
                visible: jumpFilter.text.length === 0
            }
        }

        ListView {
            id: jumpList
            objectName: "jumpList"
            width: parent.width
            height: Math.min(contentHeight, sheet.style.scaledSize(252))
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            model: sheet.matches
            currentIndex: sheet.selectedIndex
            highlightFollowsCurrentItem: true

            delegate: Item {
                id: jumpDelegate
                required property int index
                required property string label
                required property string name
                required property string kind
                required property string networkId
                width: jumpList.width
                height: sheet.style.scaledSize(28)
                readonly property bool current: index === sheet.selectedIndex

                Rectangle {
                    anchors.fill: parent
                    radius: sheet.style.scaledSize(7)
                    color: jumpDelegate.current ? sheet.style.hoverColor : "transparent"
                }

                Text {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.leftMargin: sheet.style.scaledSize(8)
                    anchors.rightMargin: sheet.style.scaledSize(8)
                    anchors.verticalCenter: parent.verticalCenter
                    text: jumpDelegate.label
                    color: jumpDelegate.current ? sheet.style.inkColor : sheet.style.mutedColor
                    elide: Text.ElideRight
                    font.family: "iA Writer Mono S"
                    font.pixelSize: sheet.style.scaledSize(12)
                }
            }
        }
    }
}
