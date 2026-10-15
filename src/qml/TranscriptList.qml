import QtQuick
import QtQuick.Controls

ListView {
    id: list

    readonly property int stickFollowing: 0
    readonly property int stickDetached: 1
    property int stick: 0
    property int firstUnseenIndex: -1
    property int firstUnseenSequence: -1
    property int unreadMarkRow: -1
    readonly property bool hasUnreadMark: unreadMarkRow >= 0
    readonly property bool canScrollDown: !viewportPinned()
    readonly property bool jumpArmed: canScrollDown && (
        (stick === stickDetached
            && firstUnseenIndex >= 0
            && firstUnseenIndex < count)
        || hasUnreadMark)
    // Fired once when a bottom row arrives while detached. The following
    // stick, shrinks, and noteSplice do not emit it.
    signal detachedArrival()

    property bool pinning: false
    property int trackedCount: 0
    // Bumped on count change, model reset, and dataChanged that covers
    // the last row. Bindings that read a row through field() have no
    // NOTIFY: the model object is stable, and a same-size rewrite leaves
    // count unchanged.
    property int rowRevision: 0
    property int restoreOffset: -1
    property int restoreAnchorSequence: -1
    // How far the anchored message sits below the viewport top. A date rule
    // or unread mark above it is part of this distance, so a prepend that
    // moves or drops that chrome can put the same line back on the same pixel.
    property real restoreAnchorPixel: 0
    property int detachedViewportSequence: -1
    property int detachedViewportOffset: -1
    property real detachedViewportPixel: 0
    property bool detachedViewportPixelValid: false
    property var onPinnedToEnd: null
    property int pinGeneration: 0
    property bool resetPending: false
    property int resetSavedCount: 0
    property bool prependPending: false
    property int prependSavedCount: 0
    // Rows removed strictly above the unseen marker. A cap trim keeps the
    // leading date rule and drops the lines under it, so this is not a
    // prefix. noteGrowth and rowsRemoved both see the count change; the
    // first one consumes this and the other must not move the marker again.
    property int removedAboveUnseen: 0
    property real previousContentHeight: 0
    property var readMarkerSync: null
    // Window supplies the selected conversation's saved place, or null.
    // {known, follow, row} matches IrcController.currentScrollPlace.
    property var rememberedPlace: null
    signal viewportSettled()
    signal userViewportSettled()

    boundsBehavior: Flickable.StopAtBounds
    clip: true
    spacing: 0
    // Slack above a short transcript, so the newest row sits on the bottom.
    // BottomToTop would invert model order and every indexAt path.
    // Applied by syncAnchoredMargin(). A live binding on contentHeight
    // re-enters while rows are realized and can settle a few pixels short.
    property bool syncingMargin: false
    highlightFollowsCurrentItem: false

    function syncAnchoredMargin() {
        if (syncingMargin)
            return;
        syncingMargin = true;
        var slack = Math.max(0, height - contentHeight);
        if (Math.abs(topMargin - slack) > 0.5)
            topMargin = slack;
        syncingMargin = false;
    }

    function topContentY() {
        return originY - topMargin;
    }

    function endContentY() {
        return topContentY() + Math.max(0, topMargin + contentHeight + bottomMargin - height);
    }

    function viewportAtTop() {
        return contentY <= topContentY() + 2;
    }

    function firstRenderedIndex() {
        var x = Math.max(1, width / 2);
        var y = Math.max(contentY, originY);
        var index = indexAt(x, y + 1);
        return index >= 0 ? index : indexAt(x, y + 8);
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

    function syncReadMarkerViewport() {
        if (resetPending || pinning)
            return;
        if (!readMarkerSync || typeof readMarkerSync.setTranscriptCaughtUp !== "function")
            return;
        readMarkerSync.setTranscriptCaughtUp(viewportPinned());
    }

    function stickToEnd() {
        contentY = endContentY();
    }

    function rowSequence(row) {
        if (!model || typeof model.field !== "function" || row < 0)
            return -1;
        if (row >= modelRowCount())
            return -1;
        var seqText = model.field(row, "sequence");
        if (seqText === undefined || seqText === null || seqText === "")
            return -1;
        return parseInt(seqText, 10);
    }

    function syncFirstUnseenIndex() {
        if (firstUnseenSequence < 0)
            return;
        var total = modelRowCount();
        for (var row = 0; row < total; ++row) {
            if (rowSequence(row) === firstUnseenSequence) {
                firstUnseenIndex = row;
                return;
            }
        }
        firstUnseenIndex = -1;
        firstUnseenSequence = -1;
    }

    function rememberDetachedViewportSequence() {
        if (!model || typeof model.field !== "function")
            return;
        var index = firstRenderedIndex();
        if (index < 0)
            return;
        detachedViewportOffset = count - index;
        detachedViewportPixel = 0;
        detachedViewportPixelValid = false;
        var total = modelRowCount();
        for (var row = index; row < total; ++row) {
            var seqText = model.field(row, "sequence");
            if (seqText !== undefined && seqText !== null && seqText !== "") {
                detachedViewportSequence = parseInt(seqText, 10);
                var item = itemAtIndex(row);
                if (item) {
                    detachedViewportPixel = item.mapToItem(list, 0, 0).y;
                    detachedViewportPixelValid = true;
                }
                return;
            }
        }
    }

    function pinToEnd() {
        stick = stickFollowing;
        firstUnseenIndex = -1;
        firstUnseenSequence = -1;
        detachedViewportSequence = -1;
        detachedViewportOffset = -1;
        detachedViewportPixel = 0;
        detachedViewportPixelValid = false;
        pinning = true;
        trackedCount = count;
        stickToEnd();
        positionViewAtEnd();
        if (onPinnedToEnd)
            onPinnedToEnd();
        var generation = ++pinGeneration;
        Qt.callLater(function() {
            if (generation !== pinGeneration)
                return;
            stickToEnd();
            positionViewAtEnd();
            pinning = false;
            trackedCount = count;
            syncReadMarkerViewport();
        });
    }

    function adoptViewport(fromUser) {
        if (pinning)
            return;
        if (viewportPinned())
            pinToEnd();
        else {
            stick = stickDetached;
            rememberDetachedViewportSequence();
        }
        syncReadMarkerViewport();
        if (fromUser)
            userViewportSettled();
        else
            viewportSettled();
    }

    function applyRememberedPlace() {
        if (typeof rememberedPlace !== "function")
            return false;
        var place = rememberedPlace();
        if (!place || place.known !== true)
            return false;
        if (place.follow === true) {
            pinToEnd();
            return true;
        }
        if (place.row >= 0) {
            pinToUnread(place.row);
            return true;
        }
        // The line is not loaded yet. Stay put so a later page can land
        // on it, and do not fall through to the end.
        return true;
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

    function settleRemovedAboveUnseen() {
        var removed = removedAboveUnseen;
        removedAboveUnseen = 0;
        if (removed <= 0 || firstUnseenIndex < 0)
            return;
        if (firstUnseenSequence >= 0)
            syncFirstUnseenIndex();
        else {
            firstUnseenIndex -= removed;
            if (firstUnseenIndex < 0)
                firstUnseenIndex = -1;
        }
    }

    function noteGrowth(previousCount, newCount) {
        if (newCount < previousCount) {
            if (newCount <= 0) {
                removedAboveUnseen = 0;
                pinToEnd();
                return;
            }
            if (removedAboveUnseen > 0) {
                settleRemovedAboveUnseen();
                trackedCount = newCount;
                return;
            }
            if (firstUnseenIndex >= newCount) {
                firstUnseenIndex = -1;
                firstUnseenSequence = -1;
            }
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
        firstUnseenSequence = rowSequence(firstUnseenIndex);
        trackedCount = newCount;
        detachedArrival();
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
        // arming a fresh one over backfilled history. At capacity a prepend
        // and tail-trim can leave the count unchanged while every row shifts.
        if (firstUnseenSequence >= 0)
            syncFirstUnseenIndex();
        else if (firstUnseenIndex >= 0) {
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

    function measuredAnchorPixel(sequence) {
        if (sequence < 0)
            return Number.NaN;
        var total = modelRowCount();
        for (var row = 0; row < total; ++row) {
            if (rowSequence(row) !== sequence)
                continue;
            var item = itemAtIndex(row);
            if (!item)
                return Number.NaN;
            return item.mapToItem(list, 0, 0).y;
        }
        return Number.NaN;
    }

    // measure is false while the model is resetting: ListView has already
    // released delegates, so item positions are not the reader's viewport.
    function snapshotAnchor(measure) {
        restoreAnchorSequence = -1;
        restoreOffset = -1;
        restoreAnchorPixel = 0;
        if (detachedViewportSequence >= 0)
            restoreAnchorSequence = detachedViewportSequence;
        if (detachedViewportOffset >= 0)
            restoreOffset = detachedViewportOffset;
        if (restoreAnchorSequence < 0 && restoreOffset < 0) {
            var index = firstRenderedIndex();
            if (index < 0)
                return;
            restoreOffset = count - index;
            if (model && typeof model.field === "function") {
                var total = modelRowCount();
                for (var row = index; row < total; ++row) {
                    var sequence = rowSequence(row);
                    if (sequence >= 0) {
                        restoreAnchorSequence = sequence;
                        detachedViewportSequence = sequence;
                        break;
                    }
                }
            }
        }
        if (measure !== false) {
            var measured = measuredAnchorPixel(restoreAnchorSequence);
            if (!isNaN(measured)) {
                restoreAnchorPixel = measured;
                detachedViewportPixel = measured;
                detachedViewportPixelValid = true;
                return;
            }
        }
        if (detachedViewportPixelValid)
            restoreAnchorPixel = detachedViewportPixel;
    }

    function clearAnchorSnapshot() {
        restoreAnchorSequence = -1;
        restoreOffset = -1;
        restoreAnchorPixel = 0;
    }

    function placeAnchor(row, pixel) {
        if (row < 0)
            return;
        positionViewAtIndex(row, ListView.Beginning);
        if (Math.abs(pixel) < 0.5)
            return;
        var item = itemAtIndex(row);
        if (!item)
            return;
        var visual = item.mapToItem(list, 0, 0).y;
        var next = contentY + (visual - pixel);
        var low = topContentY();
        var high = endContentY();
        if (next < low)
            next = low;
        if (next > high)
            next = high;
        contentY = next;
    }

    function restoreAnchor() {
        var sequence = restoreAnchorSequence;
        var offset = restoreOffset;
        var pixel = restoreAnchorPixel;
        restoreAnchorSequence = -1;
        restoreOffset = -1;
        restoreAnchorPixel = 0;
        if (stick === stickFollowing) {
            pinToEnd();
            return;
        }
        pinning = true;
        var generation = ++pinGeneration;
        var targetRow = -1;
        if (sequence >= 0 && model && typeof model.field === "function") {
            var total = modelRowCount();
            for (var row = 0; row < total; ++row) {
                var seqText = model.field(row, "sequence");
                if (seqText !== undefined && seqText !== null && seqText !== ""
                        && parseInt(seqText, 10) === sequence) {
                    targetRow = row;
                    break;
                }
            }
        }
        if (targetRow < 0 && offset >= 0) {
            var total = modelRowCount();
            targetRow = total - offset;
            if (targetRow < 0 || targetRow >= total)
                targetRow = -1;
        }
        placeAnchor(targetRow, pixel);
        Qt.callLater(function() {
            if (generation !== pinGeneration)
                return;
            placeAnchor(targetRow, pixel);
            pinning = false;
        });
    }

    function jumpToUnseen() {
        if (!jumpArmed)
            return;
        syncFirstUnseenIndex();
        if (stick === stickDetached
                && firstUnseenIndex >= 0
                && firstUnseenIndex < count) {
            var target = firstUnseenIndex;
            firstUnseenIndex = -1;
            firstUnseenSequence = -1;
            pinning = true;
            var generation = ++pinGeneration;
            positionViewAtIndex(target, ListView.Beginning);
            Qt.callLater(function() {
                if (generation !== pinGeneration)
                    return;
                pinning = false;
                adoptViewport(false);
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
            adoptViewport(false);
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
        firstUnseenSequence = -1;
        pinning = true;
        var generation = ++pinGeneration;
        positionViewAtIndex(row, ListView.Beginning);
        Qt.callLater(function() {
            if (generation !== pinGeneration)
                return;
            positionViewAtIndex(row, ListView.Beginning);
            pinning = false;
            adoptViewport(false);
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
        if (!applyRememberedPlace())
            pinToEnd();
    }

    onMovementEnded: adoptViewport(true)
    onFlickEnded: adoptViewport(true)
    onContentHeightChanged: {
        syncAnchoredMargin();
        var wasAtEnd = previousContentHeight <= height
            || contentY + height >= originY + previousContentHeight - 2;
        if (stick === stickFollowing && wasAtEnd)
            stickToEnd();
        previousContentHeight = contentHeight;
    }
    onHeightChanged: {
        syncAnchoredMargin();
        if (stick === stickFollowing)
            stickToEnd();
        else
            adoptViewport(false);
    }
    // The margin can update after onContentHeightChanged has already read it.
    onTopMarginChanged: if (stick === stickFollowing) stickToEnd()

    Connections {
        target: list.model
        ignoreUnknownSignals: true
        function onModelAboutToBeReset() {
            list.cancelDeferredPin();
            list.resetPending = true;
            list.resetSavedCount = list.count;
            if (list.stick === list.stickDetached)
                list.snapshotAnchor(false);
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
        function onRowsAboutToBeInserted(parent, first, last) {
            if (first === 0 && list.stick === list.stickDetached) {
                list.prependPending = true;
                list.prependSavedCount = list.count;
                list.snapshotAnchor();
            }
        }
        function onRowsInserted(parent, first, last) {
            if (list.prependPending && first === 0) {
                var previous = list.prependSavedCount;
                var newCount = list.count;
                list.noteSplice(previous, newCount);
                Qt.callLater(function() {
                    list.prependPending = false;
                    list.restoreAnchor();
                });
                return;
            }
            // A history-cap trim drops a leading line and appends the new
            // one, so the list count does not change and noteGrowth never
            // sees an arrival. The inserted range still ends at the tail.
            var modelCount = list.modelRowCount();
            if (last === modelCount - 1 && list.trackedCount >= modelCount) {
                list.trackedCount = modelCount;
                if (list.stick === list.stickFollowing) {
                    list.stickToEnd();
                    Qt.callLater(function() {
                        if (list.stick === list.stickFollowing)
                            list.stickToEnd();
                    });
                    return;
                }
                if (list.firstUnseenIndex < 0)
                    list.firstUnseenIndex = first;
                list.firstUnseenSequence = list.rowSequence(list.firstUnseenIndex);
                list.detachedArrival();
                return;
            }
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
        function onRowsAboutToBeRemoved(parent, first, last) {
            list.removedAboveUnseen = 0;
            if (list.stick !== list.stickDetached || list.firstUnseenIndex < 0)
                return;
            if (last < list.firstUnseenIndex)
                list.removedAboveUnseen = last - first + 1;
        }
        function onRowsRemoved(parent, first, last) {
            if (list.removedAboveUnseen > 0) {
                list.settleRemovedAboveUnseen();
                return;
            }
            if (first !== 0
                    || list.stick !== list.stickDetached
                    || list.firstUnseenIndex < 0
                    || last < list.firstUnseenIndex) {
                return;
            }
            if (list.firstUnseenSequence >= 0)
                list.syncFirstUnseenIndex();
            else {
                list.firstUnseenIndex -= (last - first + 1);
                if (list.firstUnseenIndex < 0)
                    list.firstUnseenIndex = -1;
            }
        }
    }

    Component.onCompleted: {
        syncAnchoredMargin();
        refreshUnreadMarkRow();
        if (!applyRememberedPlace())
            pinToEnd();
    }

    ScrollBar.vertical: ScrollBar {
        policy: list.contentHeight > list.height
            ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
        onPressedChanged: {
            if (!pressed)
                list.adoptViewport(true);
        }
    }
}
