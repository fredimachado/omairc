import QtQuick
import QtQuick.Controls

Popup {
    id: sheet

    required property OmaircStyle style
    property var matches: null
    property int selectedIndex: 0

    property alias linkFilter: linkFilter
    property alias linkList: linkList

    signal stepRequested(int delta)
    signal activateRequested()
    signal filterChanged()

    objectName: "linkSheet"
    width: sheet.style.scaledSize(420)
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
            border.color: linkFilter.activeFocus ? sheet.style.accentColor : sheet.style.dividerColor
            radius: sheet.style.scaledSize(7)

            TextField {
                id: linkFilter
                objectName: "linkFilter"
                Accessible.name: "Open link"
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
                objectName: "linkFilterPlaceholder"
                z: 0
                anchors.left: parent.left
                anchors.leftMargin: sheet.style.scaledSize(8)
                anchors.verticalCenter: parent.verticalCenter
                text: "Open link…"
                color: sheet.style.mutedColor
                font.family: "iA Writer Mono S"
                font.pixelSize: sheet.style.scaledSize(13)
                visible: linkFilter.text.length === 0
            }
        }

        ListView {
            id: linkList
            objectName: "linkList"
            width: parent.width
            height: Math.min(contentHeight, sheet.style.scaledSize(252))
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            model: sheet.matches
            currentIndex: sheet.selectedIndex
            highlightFollowsCurrentItem: true

            delegate: Item {
                id: linkDelegate
                required property int index
                required property string label
                required property string value
                required property string kind
                required property int row
                width: linkList.width
                height: sheet.style.scaledSize(28)
                readonly property bool current: index === sheet.selectedIndex

                Rectangle {
                    anchors.fill: parent
                    radius: sheet.style.scaledSize(7)
                    color: linkDelegate.current ? sheet.style.hoverColor : "transparent"
                }

                Text {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.leftMargin: sheet.style.scaledSize(8)
                    anchors.rightMargin: sheet.style.scaledSize(8)
                    anchors.verticalCenter: parent.verticalCenter
                    text: linkDelegate.label
                    color: linkDelegate.current ? sheet.style.inkColor : sheet.style.mutedColor
                    elide: Text.ElideMiddle
                    font.family: "iA Writer Mono S"
                    font.pixelSize: sheet.style.scaledSize(12)
                }
            }
        }
    }
}
