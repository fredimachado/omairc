import QtQuick

MouseArea {
    id: hit

    required property Item edit
    property bool inviteHits: false
    required property var editVisibleText
    required property var httpUrlAt
    required property var inviteChannelAt
    required property var openAllowedUrl
    required property var joinInviteChannel

    objectName: "urlHit"
    anchors.fill: parent
    hoverEnabled: true
    acceptedButtons: Qt.LeftButton
    cursorShape: {
        var pos = edit.positionAt(mouseX, mouseY);
        var visible = hit.editVisibleText(edit);
        if (hit.httpUrlAt(visible, pos).length > 0)
            return Qt.PointingHandCursor;
        if (inviteHits && hit.inviteChannelAt(visible, pos).length > 0)
            return Qt.PointingHandCursor;
        return Qt.IBeamCursor;
    }
    onPressed: function(mouse) {
        var pos = edit.positionAt(mouse.x, mouse.y);
        var visible = hit.editVisibleText(edit);
        if (hit.httpUrlAt(visible, pos).length > 0)
            return;
        if (inviteHits && hit.inviteChannelAt(visible, pos).length > 0)
            return;
        mouse.accepted = false;
    }
    onClicked: function(mouse) {
        var pos = edit.positionAt(mouse.x, mouse.y);
        var visible = hit.editVisibleText(edit);
        var url = hit.httpUrlAt(visible, pos);
        if (url.length > 0) {
            hit.openAllowedUrl(url);
            return;
        }
        if (inviteHits)
            hit.joinInviteChannel(hit.inviteChannelAt(visible, pos));
    }
}
