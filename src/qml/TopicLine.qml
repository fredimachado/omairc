import QtQuick

Item {
    id: line

    required property OmaircStyle style
    property string text: ""
    property var httpUrlAt: null
    property var channelNameAt: null
    property var openAllowedUrl: null
    property var openChannelName: null

    implicitHeight: topic.implicitHeight
    height: topic.implicitHeight

    FontMetrics {
        id: metrics
        font: topic.font
    }

    Text {
        id: topic
        objectName: "conversationTopic"
        width: line.width
        text: line.text
        textFormat: Text.PlainText
        color: line.style.mutedColor
        elide: Text.ElideRight
        font.family: "iA Writer Mono S"
        font.pixelSize: line.style.scaledSize(11)
    }

    MouseArea {
        id: hit
        objectName: "topicHit"
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton
        cursorShape: hit.hitKind(mouseX).length > 0
            ? Qt.PointingHandCursor : Qt.ArrowCursor

        function indexAt(x) {
            var value = topic.text;
            if (!value || value.length === 0 || x <= 0)
                return 0;
            var lo = 0;
            var hi = value.length;
            while (lo < hi) {
                var mid = Math.floor((lo + hi) / 2);
                var edge = metrics.advanceWidth(value.substring(0, mid + 1));
                if (edge <= x)
                    lo = mid + 1;
                else
                    hi = mid;
            }
            if (lo >= value.length)
                return value.length - 1;
            return lo;
        }

        function ellipsisCovers(x) {
            if (!topic.truncated)
                return false;
            return x >= topic.width - metrics.advanceWidth("\u2026");
        }

        function hitKind(x) {
            if (ellipsisCovers(x))
                return "";
            var index = indexAt(x);
            var visible = topic.text;
            if (line.httpUrlAt) {
                var url = line.httpUrlAt(visible, index);
                if (url && url.length > 0)
                    return "url";
            }
            if (line.channelNameAt) {
                var channel = line.channelNameAt(visible, index);
                if (channel && channel.length > 0)
                    return "channel";
            }
            return "";
        }

        onClicked: function(mouse) {
            if (ellipsisCovers(mouse.x))
                return;
            var index = indexAt(mouse.x);
            var visible = topic.text;
            if (line.httpUrlAt && line.openAllowedUrl) {
                var url = line.httpUrlAt(visible, index);
                if (url && url.length > 0) {
                    line.openAllowedUrl(url);
                    return;
                }
            }
            if (line.channelNameAt && line.openChannelName) {
                var channel = line.channelNameAt(visible, index);
                if (channel && channel.length > 0)
                    line.openChannelName(channel);
            }
        }
    }
}
