import QtQuick

QtObject {
    id: style

    required property var backend

    readonly property bool darkMode: backend.darkMode
    readonly property real textScale: backend.textScale
    readonly property color pageColor: backend.themeBackground
    readonly property color inkColor: backend.themeForeground
    readonly property color accentColor: backend.themeAccent
    readonly property color selectionColor: backend.themeSelection
    readonly property color panelColor: mixColors(pageColor, inkColor, darkMode ? 0.035 : 0.025)
    readonly property color raisedColor: mixColors(pageColor, inkColor, darkMode ? 0.075 : 0.055)
    readonly property color hoverColor: mixColors(pageColor, inkColor, darkMode ? 0.10 : 0.075)
    readonly property color dividerColor: mixColors(pageColor, inkColor, darkMode ? 0.13 : 0.11)
    readonly property color mutedColor: mixColors(pageColor, inkColor, darkMode ? 0.52 : 0.47)
    readonly property color overlayVeilColor: {
        var veil = mixColors(pageColor, inkColor, darkMode ? 0.18 : 0.12);
        return Qt.rgba(veil.r, veil.g, veil.b, 0.5);
    }
    readonly property var nickPalette: [
        accentColor,
        darkMode ? "#c099ff" : "#7950b8",
        darkMode ? "#7fc8a9" : "#237a58",
        darkMode ? "#efb366" : "#a45f14",
        darkMode ? "#ed8f9d" : "#b44355"
    ]
    readonly property real nickAvatarMix: darkMode ? 0.23 : 0.16
    readonly property var nickAvatarFills: [
        mixColors(pageColor, nickPalette[0], nickAvatarMix),
        mixColors(pageColor, nickPalette[1], nickAvatarMix),
        mixColors(pageColor, nickPalette[2], nickAvatarMix),
        mixColors(pageColor, nickPalette[3], nickAvatarMix),
        mixColors(pageColor, nickPalette[4], nickAvatarMix)
    ]

    // Qt maps "Ctrl" in Shortcut sequences to Command and "Alt" to Option
    // on macOS. The physical Control key is Meta. Show Cmd and Option in
    // the sheet so a Win/Cmd or Option press matches.
    readonly property bool usesCommandModifier: Qt.platform.os === "osx"
        || Qt.platform.os === "macos"

    function scaledSize(pixels) {
        return Math.max(1, Math.round(pixels * textScale));
    }

    function shortcutKeys(label) {
        if (!style.usesCommandModifier)
            return label;
        return label.split("Ctrl").join("Cmd").split("Alt").join("Option");
    }

    function mixColors(base, tint, amount) {
        return Qt.rgba(
            base.r + (tint.r - base.r) * amount,
            base.g + (tint.g - base.g) * amount,
            base.b + (tint.b - base.b) * amount,
            1);
    }

    // Shared presence mark palette. Call sites map their own vocabulary onto
    // "away" / "online" / "offline" so the hex values live in one place.
    function presenceMarkColor(kind) {
        if (kind === "away")
            return "#d6a552"
        if (kind === "online")
            return "#69b978"
        return mutedColor
    }

    function paletteColor(index) {
        return nickPalette[index];
    }

    function nickPaletteIndex(nick) {
        var hash = 0;
        for (var index = 0; index < nick.length; ++index)
            hash = (hash + nick.charCodeAt(index)) % 5;
        return hash;
    }

    function nickColor(nick) {
        return paletteColor(nickPaletteIndex(nick));
    }

    function initials(nick) {
        return nick.length > 0 ? nick.charAt(0).toUpperCase() : "?";
    }
}
