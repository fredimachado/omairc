#!/usr/bin/env python3
"""Generate client/docs/qml-ui-test-checklist.md from tst_omairc.qml."""

from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
QML = ROOT / "tests" / "ui" / "tst_omairc.qml"
OUT = Path(__file__).resolve().parent / "qml-ui-test-checklist.md"

M6 = {
    "shortTranscriptSitsAtBottom": "ui::tests::transcript::short_transcript_sits_at_bottom",
    "shortTranscriptGrowsUpward": "ui::tests::transcript::short_transcript_grows_upward",
    "shortTranscriptResizeKeepsBottom": "ui::tests::transcript::short_transcript_resize_keeps_bottom",
    "followingAppendStaysPinnedThroughLayout": (
        "ui::tests::transcript::following_append_stays_pinned_through_layout"
    ),
    "detachedArrivalKeepsViewportAndJumpsToFirstUnseen": (
        "ui::tests::transcript::detached_arrival_keeps_viewport_and_jumps_to_first_unseen"
    ),
    "historyPrependKeepsAnchorMessageY": (
        "ui::tests::transcript::history_prepend_keeps_anchor_message "
        "(msgid/place; pixel Y is QML ListView-specific)"
    ),
    "historySpliceKeepsTheReaderOnTheSameMessage": (
        "ui::tests::transcript::history_splice_keeps_the_reader_on_the_same_message"
    ),
    "cappedTrimKeepsDetachedPlace": "ui::tests::transcript::capped_trim_keeps_detached_place",
    "typingIndicatorRidesTheTranscriptEnd": (
        "ui::tests::typing::typing_indicator_rides_the_transcript_end"
    ),
    "typingIndicatorMovesBelowMyOwnMessage": (
        "ui::tests::typing::typing_indicator_moves_below_my_own_message"
    ),
    "typingIndicatorKeepsScrollWhileReadingHistory": (
        "ui::tests::typing::typing_indicator_keeps_scroll_while_reading_history"
    ),
    "connectionSheetCloseRestoresComposerFocus": (
        "ui::tests::sheets::connection_sheet_close_restores_composer_focus"
    ),
    "aboutSheetLeftClickDimmerDismisses": (
        "ui::tests::sheets::about_sheet_left_click_dimmer_dismisses"
    ),
    "aboutSheetIgnoresRightClickAndCardClick": (
        "ui::tests::sheets::about_sheet_ignores_right_click_and_card_click"
    ),
    "connectionSheetBlocksSidebarAndMembers": (
        "ui::tests::sheets::connection_sheet_blocks_sidebar_and_members "
        "(chords + veiled sidebar/member clicks)"
    ),
    "memberListPageDownMovesByVisiblePage": (
        "ui::tests::sheets::member_list_page_down_moves_by_visible_page"
    ),
}

PENDING = {
    "openDirectMessageFromTranscriptNick": (
        "pending (M6 wave 2: real pointer hit through the view)"
    ),
    "messageBodyClickOpensHttpsUrl": "pending (M6 wave 2: observe TestApp open_url)",
    "messageBodyClickOpensHttpsUrlInsideEmphasis": "pending (M6 wave 2)",
    "droppedFileUploadsAndInsertsTheLink": "pending (M6 wave 2: demo file host)",
}


def status_for(short: str) -> str:
    if short in M6:
        return f"rust: `{M6[short]}`"
    if short in PENDING:
        return PENDING[short]
    low = short.lower()
    if any(x in low for x in ("secret", "password", "redact")):
        return "model: irc secret policy / storage credential tests"
    if any(x in low for x in ("scrollplace", "remember", "restore")):
        return "model: controller/tests/scrollplace.rs"
    if "unread" in low or "mention" in low or "badge" in low:
        return "model: model/unread + controller notify/selection"
    if "composer" in low or "tabcomplete" in low or "draft" in low:
        return "model: model/composer + controller/tests/composer.rs"
    if "walkconversation" in low or "jumptonextunread" in low:
        return "model: model/navigation.rs"
    if "sendmessage" in low:
        return "model: controller/tests/sends.rs"
    if low.startswith("typing") and short not in M6:
        return "model: model/typing_tail.rs / session typing"
    if "connect" in low and short not in M6:
        return "model: model/connect/*; remaining Connect view cases pending"
    if "about" in low and short not in M6:
        return "pending (About variants beyond dimmer/card)"
    if "member" in low and short not in M6:
        return "pending (member list view variants)"
    if any(x in short for x in ("PageUp", "PageDown", "ctrlHome", "Transcript", "Scrollbar")):
        return "pending (transcript chrome) or model: model/cursor.rs"
    return "pending (cutover inventory)"


def main() -> None:
    names = re.findall(r"function (test_\w+)\s*\(", QML.read_text(encoding="utf-8"))
    lines = [
        "# QML to Rust UI test checklist",
        "",
        f"Cutover evidence for M6. Source: `tests/ui/tst_omairc.qml` ({len(names)} tests).",
        "Harness: `client/src/ui/tests/fixture.rs`.",
        "",
        "Status values:",
        "- `rust:` GPUI view test path under `client/src/ui/tests/`",
        "- `model:` covered outside `ui/` (controller / model / irc)",
        "- `pending:` not yet ported to a GPUI view test",
        "",
        "## M6 named wave",
        "",
    ]
    for key, value in M6.items():
        lines.append(f"- `test_{key}` -> rust: `{value}`")
    # Pointer cases stay pending; deleted ui::tests::pointer stubs must not return.
    lines.append(
        "- `test_openDirectMessageFromTranscriptNick` -> "
        f"{PENDING['openDirectMessageFromTranscriptNick']}"
    )
    lines.append(
        "- middle-click close (sidebar) -> pending "
        "(M6 wave 2: real middle-click through the view)"
    )
    for key, value in PENDING.items():
        if key == "openDirectMessageFromTranscriptNick":
            continue
        lines.append(f"- `test_{key}` -> {value}")
    lines.extend(["", "## Full inventory", ""])
    for name in names:
        short = name[len("test_") :]
        lines.append(f"- `{name}` -> {status_for(short)}")
    OUT.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {OUT} ({len(names)} tests)")


if __name__ == "__main__":
    main()
