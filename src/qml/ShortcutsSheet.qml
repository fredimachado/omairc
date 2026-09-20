import QtQuick
import QtQuick.Controls

Popup {
    id: sheet

    required property OmaircStyle style

    objectName: "shortcutsSheet"
    width: sheet.style.scaledSize(400)
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
        spacing: sheet.style.scaledSize(4)

        Repeater {
            model: [
                { keys: "Alt+Down / Alt+Up", action: "walk conversations" },
                { keys: "Alt+Left / Alt+Right", action: "walk networks" },
                { keys: "Alt+Shift+Left", action: "collapse network" },
                { keys: "Alt+Shift+Right", action: "expand network" },
                { keys: "Ctrl+Alt+Shift+Left", action: "collapse all networks" },
                { keys: "Ctrl+Alt+Shift+Right", action: "expand all networks" },
                { keys: "Alt+Shift+Up", action: "move network up" },
                { keys: "Alt+Shift+Down", action: "move network down" },
                { keys: "Ctrl+K", action: "jump to conversation" },
                { keys: "Ctrl+Shift+K", action: "jump to nick" },
                { keys: "Alt+A", action: "next unread" },
                { keys: "Ctrl+`", action: "Status" },
                { keys: "Ctrl+,", action: "Connect" },
                { keys: "Ctrl+Enter", action: "apply connection" },
                { keys: "Ctrl+Shift+M", action: "members panel" },
                { keys: "Ctrl+Shift+P", action: "focus members" },
                { keys: "Ctrl+Shift+S", action: "server list" },
                { keys: "Ctrl+W", action: "close direct message" },
                { keys: "Ctrl+L", action: "composer" },
                { keys: "Ctrl+F", action: "find" },
                { keys: "Enter", action: "send" },
                { keys: "Page Up / Page Down", action: "scroll" },
                { keys: "Tab", action: "nick complete" },
                { keys: "Up / Down", action: "history" },
                { keys: "Escape", action: "dismiss" },
                { keys: "Ctrl+/", action: "this sheet" },
                { keys: "/disconnect", action: "disconnect network" },
                { keys: "Ctrl+Q", action: "quit" }
            ]

            Row {
                spacing: sheet.style.scaledSize(12)
                width: parent.width

                Text {
                    width: sheet.style.scaledSize(210)
                    text: modelData.keys
                    color: sheet.style.inkColor
                    font.family: "iA Writer Mono S"
                    font.pixelSize: sheet.style.scaledSize(11)
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
