import QtQuick
import QtQuick.Controls

Popup {
    id: sheet

    required property OmaircStyle style

    readonly property var shortcutGroups: [
        {
            title: "MOVE",
            rows: [
                { keys: "Alt+Down / Alt+Up", action: "walk conversations" },
                { keys: "Alt+Left / Alt+Right", action: "walk networks" },
                { keys: "Alt+Shift+Left / Right", action: "collapse / expand network" },
                { keys: "Ctrl+Alt+Shift+Left / Right", action: "collapse / expand all" },
                { keys: "Alt+Shift+Up / Down", action: "move network" },
                { keys: "Alt+A", action: "next unread" }
            ]
        },
        {
            title: "JUMP",
            rows: [
                { keys: "Ctrl+K", action: "jump to conversation" },
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
                { keys: "Page Up / Page Down", action: "scroll" },
                { keys: "Shift+Page Up / Shift+Page Down", action: "scroll half page" },
                { keys: "Ctrl+Home / Ctrl+End", action: "top / bottom" },
                { keys: "Tab", action: "nick complete" },
                { keys: "Up / Down", action: "history" },
                { keys: "Escape", action: "dismiss" }
            ]
        },
        {
            title: "CONNECT",
            rows: [
                { keys: "Ctrl+,", action: "Connect" },
                { keys: "Ctrl+Tab", action: "Connect tabs" },
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
                { keys: "Ctrl+/", action: "this sheet" },
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
                            text: sheet.style.shortcutKeys(modelData.keys)
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
