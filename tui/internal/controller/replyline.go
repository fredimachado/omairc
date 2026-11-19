package controller

import (
	"strings"
	"unicode/utf8"
)

// ReplyPreviewChars is the longest parent text shown, in characters, before it
// ends in an ellipsis. It mirrors REPLY_PREVIEW_CHARS in client/src/model/reply.rs.
const ReplyPreviewChars = 60

const replyArrow = "↪"

// replyLine is the text of the line above a reply: "↪ author: text" when the
// parent is loaded, else "↪ reply". The text is flattened to one line (every run
// of whitespace, newlines included, becomes one space), trimmed and cut at
// ReplyPreviewChars. It mirrors reply_line in client/src/model/reply.rs.
func replyLine(parentLoaded bool, author, text string) string {
	if !parentLoaded {
		return replyArrow + " reply"
	}
	flat := strings.Join(strings.Fields(text), " ")
	if utf8.RuneCountInString(flat) > ReplyPreviewChars {
		kept := []rune(flat)[:ReplyPreviewChars-1]
		flat = strings.TrimRight(string(kept), " ") + "…"
	}
	return replyArrow + " " + author + ": " + flat
}
