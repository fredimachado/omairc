import QtQuick
import QtQuick.Controls

Popup {
    id: sheet

    required property OmaircStyle style

    readonly property bool windowsTerminalAliases: Qt.platform.os === "windows"

    function chordLabel(primary, windowsExtra) {
        var label = primary
        if (windowsTerminalAliases && windowsExtra !== undefined)
            label = primary + " / " + windowsExtra
        return style.shortcutKeys(label)
    }

    readonly property var shortcutGroups: [
        {
            title: "MOVE",
            rows: [
                { keys: "Alt+Down / Alt+Up", winKeys: "Ctrl+Alt+Down / Up", action: "walk conversations" },
                { keys: "Alt+Left / Alt+Right", winKeys: "Ctrl+Alt+Left / Right", action: "walk networks" },
                { keys: "Alt+Shift+Left / Right", winKeys: "Ctrl+Shift+Left / Right", action: "collapse / expand network" },
                { keys: "Ctrl+Alt+Shift+Left / Right", action: "collapse / expand all" },
                { keys: "Alt+Shift+Up / Down", winKeys: "Ctrl+Alt+Shift+Up / Down", action: "move network" },
                { keys: "Alt+A", action: "next unread" },
                { keys: "Alt+Shift+A", action: "mark all read" }
            ]
        },
        {
            title: "JUMP",
            rows: [
                { keys: "Ctrl+K", action: "jump to conversation" },
                { keys: "Ctrl+Shift+O", action: "open link" },
                { keys: "Ctrl+Shift+K", action: "jump to nick" },
                { keys: "Ctrl+Shift+A", action: "inbox" },
                { keys: "Ctrl+`", action: "Status" },
                { keys: "Ctrl+W", action: "close direct message" }
            ]
        },
        {
            title: "WRITE",
            rows: [
                { keys: "Ctrl+L", action: "composer" },
                { keys: "Ctrl+C", action: "copy selection" },
                { keys: "Ctrl+F", action: "find" },
                { keys: "Enter", action: "send" },
                { keys: "Page Up / Page Down", action: "scroll transcript" },
                { keys: "Shift+Page Up / Shift+Page Down", action: "scroll transcript half page" },
                { keys: "Ctrl+Home / Ctrl+End", action: "top / bottom" },
                { keys: "Tab", action: "nick complete" },
                { keys: "Up / Down", action: "history" },
                { keys: "Escape", action: "dismiss" }
            ]
        },
        {
            title: "CONNECT",
            rows: [
                { keys: "Ctrl+,", winKeys: "Ctrl+]", action: "Connect" },
                { keys: "Ctrl+Tab", winKeys: "Ctrl+PgDn", action: "Connect tabs" },
                { keys: "Ctrl+N", action: "add network" },
                { keys: "Ctrl+Shift+Delete", action: "remove network" },
                { keys: "Ctrl+Enter", action: "apply selected network" }
            ]
        },
        {
            title: "WINDOW",
            rows: [
                { keys: "Ctrl+Shift+S", action: "server list" },
                { keys: "Ctrl+Shift+M", action: "members panel" },
                { keys: "Ctrl+Shift+P", action: "focus members" },
                { keys: "Page Up / Page Down", action: "page focused members" },
                { keys: "Shift+Page Up / Shift+Page Down", action: "page focused members half page" },
                { keys: "Home / End", action: "first / last focused nick" },
                { keys: "Ctrl+/", action: "this sheet" },
                { keys: "Ctrl+Shift+/", action: "About" },
                { keys: "Ctrl+Q", action: "quit" }
            ]
        }
    ]

    objectName: "shortcutsSheet"
    width: sheet.style.scaledSize(460)
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
        spacing: sheet.style.scaledSize(12)
        width: sheet.availableWidth

        Repeater {
            model: sheet.shortcutGroups

            Column {
                id: shortcutGroup

                required property var modelData

                spacing: sheet.style.scaledSize(4)
                width: parent.width

                Text {
                    text: shortcutGroup.modelData.title
                    color: sheet.style.mutedColor
                    font.family: "iA Writer Mono S"
                    font.bold: true
                    font.letterSpacing: sheet.style.scaledSize(0.8)
                    font.pixelSize: sheet.style.scaledSize(9)
                }

                Repeater {
                    model: shortcutGroup.modelData.rows

                    Row {
                        required property var modelData

                        spacing: sheet.style.scaledSize(12)
                        width: parent.width

                        Text {
                            width: sheet.style.scaledSize(220)
                            text: sheet.chordLabel(modelData.keys, modelData.winKeys)
                            color: sheet.style.inkColor
                            font.family: "iA Writer Mono S"
                            font.pixelSize: sheet.style.scaledSize(11)
                            wrapMode: Text.NoWrap
                            elide: Text.ElideNone
                        }

                        Text {
                            text: modelData.action
                            color: sheet.style.mutedColor
                            font.family: "iA Writer Mono S"
                            font.pixelSize: sheet.style.scaledSize(11)
                        }
                    }
                }
            }
        }
    }
}
