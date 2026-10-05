import QtQuick

// TitleMark is the unfocused mention or direct message shown in the window
// title. OmaircWindow notes an arrival, and focus or opening that conversation
// clears it. The text matches tui/internal/ui/title.go's attentionTitle.
QtObject {
    id: mark

    property string shown: ""
    property string networkId: ""
    property string target: ""

    function note(windowActive, author, plainBody, networkId, target, place) {
        if (windowActive)
            return;
        if (!target || target.length === 0)
            return;
        mark.networkId = networkId || "";
        mark.target = target;
        mark.shown = format(author, plainBody, place || target);
    }

    function clear() {
        shown = "";
        networkId = "";
        target = "";
    }

    function clearIfOpened(networkId, target) {
        if (shown.length === 0)
            return;
        if (networkId === mark.networkId && target === mark.target)
            clear();
    }

    function format(author, plainBody, place) {
        var who = collapse(author);
        var text = collapse(plainBody);
        if (who.length === 0)
            who = place;
        var lead = text.length > 0 ? (who + ": " + text) : who;
        if (place && place.length > 0 && place !== who)
            return lead + " · " + place + " - Omairc";
        return lead + " - Omairc";
    }

    // collapse drops C0 controls and DEL, including ESC and BEL, and folds the
    // gap into one space. IRC formatting is already gone before this runs.
    function collapse(text) {
        if (!text)
            return "";
        var out = "";
        var pendingSpace = false;
        for (var index = 0; index < text.length; ++index) {
            var code = text.charCodeAt(index);
            if (code <= 0x20 || code === 0x7f) {
                if (out.length > 0)
                    pendingSpace = true;
                continue;
            }
            if (pendingSpace) {
                out += " ";
                pendingSpace = false;
            }
            out += text.charAt(index);
        }
        return out;
    }
}
