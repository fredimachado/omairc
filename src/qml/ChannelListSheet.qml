import QtQuick
import QtQuick.Controls
import Omairc.App 1.0

Popup {
    id: sheet

    required property OmaircStyle style
    property var matches: null
    property int selectedIndex: 0

    property alias channelListFilter: channelListFilter
    property alias channelListList: channelListList

    IrcTextFormatter {
        id: ircText
    }

    signal stepRequested(int delta)
    signal activateRequested()
    signal filterChanged()
    signal rowActivated(int index)

    objectName: "channelListSheet"
    width: sheet.style.scaledSize(480)
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
            border.color: channelListFilter.activeFocus ? sheet.style.accentColor : sheet.style.dividerColor
            radius: sheet.style.scaledSize(7)

            TextField {
                id: channelListFilter
                objectName: "channelListFilter"
                Accessible.name: "Filter channels"
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
                objectName: "channelListFilterPlaceholder"
                z: 0
                anchors.left: parent.left
                anchors.leftMargin: sheet.style.scaledSize(8)
                anchors.verticalCenter: parent.verticalCenter
                text: "Filter channels…"
                color: sheet.style.mutedColor
                font.family: "iA Writer Mono S"
                font.pixelSize: sheet.style.scaledSize(13)
                visible: channelListFilter.text.length === 0
            }
        }

        ListView {
            id: channelListList
            objectName: "channelListList"
            width: parent.width
            height: Math.min(contentHeight, sheet.style.scaledSize(280))
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            model: sheet.matches
            currentIndex: sheet.selectedIndex
            highlightFollowsCurrentItem: false
            ScrollBar.vertical: ScrollBar {
                policy: channelListList.contentHeight > channelListList.height
                    ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff
            }

            delegate: Item {
                id: listDelegate
                required property int index
                required property string channel
                required property int users
                required property string topic
                required property string label
                width: channelListList.width
                height: sheet.style.scaledSize(28)
                readonly property bool current: index === sheet.selectedIndex
                objectName: "channelListRow-" + channel

                Rectangle {
                    anchors.fill: parent
                    radius: sheet.style.scaledSize(7)
                    color: listDelegate.current ? sheet.style.hoverColor : "transparent"
                }

                Text {
                    id: channelName
                    anchors.left: parent.left
                    anchors.leftMargin: sheet.style.scaledSize(8)
                    anchors.verticalCenter: parent.verticalCenter
                    text: listDelegate.channel
                    color: listDelegate.current ? sheet.style.inkColor : sheet.style.mutedColor
                    font.family: "iA Writer Mono S"
                    font.pixelSize: sheet.style.scaledSize(12)
                }

                Text {
                    id: userCount
                    anchors.left: channelName.right
                    anchors.leftMargin: sheet.style.scaledSize(10)
                    anchors.verticalCenter: parent.verticalCenter
                    text: String(listDelegate.users)
                    color: sheet.style.mutedColor
                    font.family: "iA Writer Mono S"
                    font.pixelSize: sheet.style.scaledSize(12)
                }

                Text {
                    objectName: "channelListTopic"
                    anchors.left: userCount.right
                    anchors.leftMargin: sheet.style.scaledSize(10)
                    anchors.right: parent.right
                    anchors.rightMargin: sheet.style.scaledSize(8)
                    anchors.verticalCenter: parent.verticalCenter
                    text: ircText.hasIrcEmphasis(listDelegate.topic)
                        ? ircText.emphasizedIrcText(listDelegate.topic)
                        : ircText.plainIrcText(listDelegate.topic)
                    textFormat: ircText.hasIrcEmphasis(listDelegate.topic)
                        ? Text.RichText
                        : Text.PlainText
                    color: sheet.style.mutedColor
                    elide: Text.ElideRight
                    font.family: "iA Writer Mono S"
                    font.pixelSize: sheet.style.scaledSize(12)
                }

                MouseArea {
                    anchors.fill: parent
                    onClicked: sheet.rowActivated(listDelegate.index)
                }
            }
        }

        Text {
            objectName: "channelListStatus"
            width: parent.width
            text: {
                var model = sheet.matches;
                if (!model)
                    return "";
                if (model.error)
                    return model.error;
                var loading = model.loading === true;
                var source = model.sourceCount || 0;
                var visible = channelListList.count;
                var filter = channelListFilter.text;
                if (loading && source === 0)
                    return "Loading channels…";
                if (loading)
                    return "Loading… " + source + " channels";
                if (visible === 0)
                    return filter.length > 0 ? "No matches" : "No channels";
                if (visible !== source)
                    return visible + " of " + source + " channels";
                return source + " channels";
            }
            color: sheet.style.mutedColor
            font.family: "iA Writer Mono S"
            font.pixelSize: sheet.style.scaledSize(11)
        }
    }
}
