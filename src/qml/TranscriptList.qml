import QtQuick
import QtQuick.Controls

ListView {
    id: list

    readonly property int stickFollowing: 0
    readonly property int stickDetached: 1
    property int stick: 0
    property int firstUnseenIndex: -1
    property int unreadMarkRow: -1
    readonly property bool hasUnreadMark: unreadMarkRow >= 0
    readonly property bool canScrollDown: !viewportPinned()
    readonly property bool jumpArmed: canScrollDown && (
        (stick === stickDetached
            && firstUnseenIndex >= 0
            && firstUnseenIndex < count)
        || hasUnreadMark)

    property bool pinning: false
    property int trackedCount: 0
    // Bumped on count change, model reset, and dataChanged that covers
    // the last row. Bindings that read a row through field() have no
    // NOTIFY: the model object is stable, and a same-size rewrite leaves
    // count unchanged.
    property int rowRevision: 0
    property int restoreOffset: -1
    property int pinGeneration: 0
    property bool resetPending: false
    property int resetSavedCount: 0
    property real previousContentHeight: 0

    boundsBehavior: Flickable.StopAtBounds
    clip: true
    spacing: 0
    highlightFollowsCurrentItem: false

    function endContentY() {
        return originY + Math.max(0, contentHeight - height);
    }

    function refreshUnreadMarkRow() {
        unreadMarkRow = (model && typeof model.unreadMarkRow === "function")
            ? model.unreadMarkRow() : -1;
    }

    function viewportPinned() {
        if (count === 0 || contentHeight <= height || atYEnd)
            return true;
        return contentY >= endContentY() - 2;
    }

    function stickToEnd() {
        contentY = endContentY();
    }

    function pinToEnd() {
        stick = stickFollowing;
        firstUnseenIndex = -1;
        pinning = true;
        trackedCount = count;
        stickToEnd();
        positionViewAtEnd();
        var generation = ++pinGeneration;
        Qt.callLater(function() {
            if (generation !== pinGeneration)
                return;
            stickToEnd();
            positionViewAtEnd();
            pinning = false;
            trackedCount = count;
        });
    }

    function adoptViewport() {
        if (pinning)
            return;
        if (viewportPinned())
            pinToEnd();
        else
            stick = stickDetached;
    }

    function cancelDeferredPin() {
        if (!pinning)
            return;
        var generation = ++pinGeneration;
        Qt.callLater(function() {
            if (generation !== pinGeneration)
                return;
            pinning = false;
        });
    }

    function noteGrowth(previousCount, newCount) {
        if (newCount < previousCount) {
            if (newCount <= 0) {
                pinToEnd();
                return;
            }
            if (firstUnseenIndex >= newCount)
                firstUnseenIndex = -1;
            trackedCount = newCount;
            return;
        }
        if (newCount <= previousCount) {
            trackedCount = newCount;
            return;
        }
        if (stick === stickFollowing) {
            trackedCount = newCount;
            stickToEnd();
            Qt.callLater(function() {
                if (list.stick === list.stickFollowing)
                    list.stickToEnd();
            });
            return;
        }
        if (firstUnseenIndex < 0)
            firstUnseenIndex = previousCount;
        trackedCount = newCount;
    }

    function noteSplice(previousCount, newCount) {
        trackedCount = newCount;
        if (newCount <= 0) {
            pinToEnd();
            return;
        }
        if (stick === stickFollowing) {
            stickToEnd();
            return;
        }
        // Replay rows land above the reader, so nothing new arrived at the
        // bottom. Carry an armed marker along with its row rather than
        // arming a fresh one over backfilled history.
        if (firstUnseenIndex >= 0) {
            firstUnseenIndex += newCount - previousCount;
            if (firstUnseenIndex < 0 || firstUnseenIndex >= newCount)
                firstUnseenIndex = -1;
        }
    }

    function modelRowCount() {
        if (model && typeof model.rowCount === "function")
            return model.rowCount();
        return count;
    }

    function snapshotAnchor() {
        var index = indexAt(Math.max(1, width / 2), contentY + 1);
        if (index < 0)
            index = indexAt(Math.max(1, width / 2), contentY + 8);
        if (index < 0)
            index = 0;
        // A history splice inserts replay rows above the reader and may trim
        // the front, so a raw index names a different message afterwards.
        // Distance from the last row survives both.
        restoreOffset = count - index;
    }

    function restoreAnchor() {
        var offset = restoreOffset;
        restoreOffset = -1;
        if (stick === stickFollowing) {
            pinToEnd();
            return;
        }
        pinning = true;
        var generation = ++pinGeneration;
        var total = modelRowCount();
        var target = total - offset;
        if (offset >= 0 && target >= 0 && target < total)
            positionViewAtIndex(target, ListView.Beginning);
        Qt.callLater(function() {
            if (generation !== pinGeneration)
                return;
            pinning = false;
        });
    }

    function jumpToUnseen() {
        if (!jumpArmed)
            return;
        if (stick === stickDetached
                && firstUnseenIndex >= 0
                && firstUnseenIndex < count) {
            var target = firstUnseenIndex;
            firstUnseenIndex = -1;
            pinning = true;
            var generation = ++pinGeneration;
            positionViewAtIndex(target, ListView.Beginning);
            Qt.callLater(function() {
                if (generation !== pinGeneration)
                    return;
                pinning = false;
                adoptViewport();
            });
            return;
        }
        if (hasUnreadMark)
            pinToEnd();
    }

    function revealRow(index) {
        if (index < 0 || index >= modelRowCount())
            return;
        stick = stickDetached;
        pinning = true;
        var generation = ++pinGeneration;
        positionViewAtIndex(index, ListView.Beginning);
        Qt.callLater(function() {
            if (generation !== pinGeneration)
                return;
            pinning = false;
            adoptViewport();
        });
    }

    function pinToUnread(row) {
        var total = modelRowCount();
        if (row < 0 || row >= total) {
            pinToEnd();
            return;
        }
        stick = stickDetached;
        firstUnseenIndex = -1;
        pinning = true;
        var generation = ++pinGeneration;
        positionViewAtIndex(row, ListView.Beginning);
        Qt.callLater(function() {
            if (generation !== pinGeneration)
                return;
            positionViewAtIndex(row, ListView.Beginning);
            pinning = false;
            adoptViewport();
        });
    }

    onCountChanged: {
        rowRevision += 1;
        refreshUnreadMarkRow();
        if (resetPending)
            return;
        noteGrowth(trackedCount, count);
    }

    onModelChanged: {
        refreshUnreadMarkRow();
        pinToEnd();
    }

    onMovementEnded: adoptViewport()
    onFlickEnded: adoptViewport()
    onContentHeightChanged: {
        var wasAtEnd = previousContentHeight <= height
            || contentY + height >= originY + previousContentHeight - 2;
        if (stick === stickFollowing && wasAtEnd)
            stickToEnd();
        previousContentHeight = contentHeight;
    }
    onHeightChanged: {
        if (stick === stickFollowing)
            stickToEnd();
        else
            adoptViewport();
    }

    Connections {
        target: list.model
        ignoreUnknownSignals: true
        function onModelAboutToBeReset() {
            list.cancelDeferredPin();
            list.resetPending = true;
            list.resetSavedCount = list.count;
            if (list.stick === list.stickDetached)
                list.snapshotAnchor();
        }
        function onModelReset() {
            list.rowRevision += 1;
            list.refreshUnreadMarkRow();
            // ListView.count is still the pre-reset value here. The C++
            // model already has the spliced rows, so growth after this
            // handler would look like a bottom append.
            var previous = list.resetSavedCount;
            var newCount = list.modelRowCount();
            list.noteSplice(previous, newCount);
            Qt.callLater(function() {
                list.resetPending = false;
                list.restoreAnchor();
            });
        }
        function onRowsInserted(parent, first, last) {
            list.noteGrowth(list.trackedCount, list.count);
        }
        function onDataChanged(topLeft, bottomRight) {
            // The typing footer reads the last row through
            // transcriptField / field(), a Q_INVOKABLE with no NOTIFY.
            // Same-size reload and ListModel setProperty emit
            // dataChanged without changing count, so this bump
            // re-reads grouping.
            if (bottomRight.row >= list.count - 1)
                list.rowRevision += 1;
        }
        function onRowsRemoved(parent, first, last) {
            if (first === 0
                    && list.stick === list.stickDetached
                    && list.firstUnseenIndex >= 0) {
                list.firstUnseenIndex -= (last - first + 1);
                if (list.firstUnseenIndex < 0)
                    list.firstUnseenIndex = -1;
            }
        }
    }

    Component.onCompleted: {
        refreshUnreadMarkRow();
        pinToEnd();
    }

    ScrollBar.vertical: ScrollBar {
        policy: list.contentHeight > list.height
            ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
        onPressedChanged: {
            if (!pressed)
                list.adoptViewport();
        }
    }
}
