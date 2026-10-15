package ui

import (
	"strings"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/controller"
)

func TestOnlyVisibleToYouMarker(t *testing.T) {
	m := seededModel(t)
	at := time.Date(2026, 9, 27, 12, 0, 0, 0, time.UTC)
	marked := controller.MessageSnapshot{
		Author:           "bot",
		Kind:             "message",
		Body:             "just you",
		Time:             at,
		OnlyVisibleToYou: true,
	}
	plain := ansiPattern.ReplaceAllString(m.messageRow(0, marked), "")
	if !strings.Contains(plain, "just you") || !strings.Contains(plain, onlyVisibleToYouLabel) {
		t.Fatalf("marked row = %q, want the body and %q", plain, onlyVisibleToYouLabel)
	}
	if strings.Count(plain, "\n") != 1 {
		t.Fatalf("marked row = %q, want the marker on its own line", plain)
	}
	if haystack := m.transcriptLine(marked); strings.Contains(haystack, onlyVisibleToYouLabel) {
		t.Fatalf("find haystack = %q, want the marker left out", haystack)
	}

	ordinary := marked
	ordinary.OnlyVisibleToYou = false
	ordinary.Body = "everyone"
	ordinaryPlain := ansiPattern.ReplaceAllString(m.messageRow(0, ordinary), "")
	if strings.Contains(ordinaryPlain, onlyVisibleToYouLabel) {
		t.Fatalf("ordinary row = %q, want no marker", ordinaryPlain)
	}

	status := controller.MessageSnapshot{Author: "op", Kind: "message", Body: "ops", Time: at}
	statusPlain := ansiPattern.ReplaceAllString(m.messageRow(0, status), "")
	if strings.Contains(statusPlain, onlyVisibleToYouLabel) {
		t.Fatalf("statusmsg-shaped row = %q, want no marker without the flag", statusPlain)
	}

	mention := marked
	mention.Mentioned = true
	mention.Body = "mentions you"
	mentionPlain := ansiPattern.ReplaceAllString(m.messageRow(0, mention), "")
	if !strings.Contains(mentionPlain, onlyVisibleToYouLabel) {
		t.Fatalf("mention row = %q, want the marker", mentionPlain)
	}

	action := controller.MessageSnapshot{
		Author: "bot", Kind: "action", Body: "waves", Time: at, OnlyVisibleToYou: true,
	}
	actionPlain := ansiPattern.ReplaceAllString(m.messageRow(0, action), "")
	if !strings.Contains(actionPlain, onlyVisibleToYouLabel) || !strings.Contains(actionPlain, "waves") {
		t.Fatalf("action row = %q, want the marker under the action", actionPlain)
	}

	m.find.active = true
	m.find.index = 0
	findAction := ansiPattern.ReplaceAllString(m.messageRow(0, action), "")
	findLines := strings.Split(findAction, "\n")
	if len(findLines) != 2 || !strings.HasPrefix(findLines[1], "  "+onlyVisibleToYouLabel) {
		t.Fatalf("find action = %q, want the marker indented like the action", findAction)
	}
	findChat := ansiPattern.ReplaceAllString(m.messageRow(0, marked), "")
	chatLines := strings.Split(findChat, "\n")
	if len(chatLines) != 2 || !strings.HasPrefix(chatLines[1], " ") {
		t.Fatalf("find chat = %q, want the marker indented under the body", findChat)
	}
}
