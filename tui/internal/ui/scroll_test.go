package ui

import (
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/demo"
)

func TestScrollPlaceSwitchRestoresLine(t *testing.T) {
	m, _ := unreadDemoModel(t)
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyPgUp})
	if m.transcriptFollowEnd {
		t.Fatal("Page Up must leave follow-the-end")
	}
	row := m.firstVisibleMessageRow()
	body := m.ctrl.Messages()[row].Body
	if body == "" {
		t.Fatal("the anchored line needs a body")
	}

	m.switchSelection(func() {
		m.ctrl.SelectConversation("omarchy", "#desktop")
	})
	m.switchSelection(func() {
		m.ctrl.SelectConversation("omarchy", "#omarchy")
	})
	if m.transcriptFollowEnd {
		t.Fatal("returning to a scrolled conversation must stay detached")
	}
	if got := m.ctrl.Messages()[m.firstVisibleMessageRow()].Body; got != body {
		t.Fatalf("restored body = %q, want %q", got, body)
	}
}

func TestScrollPlaceFollowStaysPinned(t *testing.T) {
	m, d := unreadDemoModel(t)
	if !m.transcriptFollowEnd {
		t.Fatal("startup must follow the end")
	}
	m.switchSelection(func() {
		m.ctrl.SelectConversation("omarchy", "#desktop")
	})
	d.InjectOmarchy([]byte("@msgid=later-1 :anna!u@h PRIVMSG #omarchy :a fresh line\r\n"))
	m.switchSelection(func() {
		m.ctrl.SelectConversation("omarchy", "#omarchy")
	})
	if !m.transcriptFollowEnd {
		t.Fatal("a conversation left at the end must stay pinned")
	}
	place := m.ctrl.CurrentScrollPlace()
	if !place.Known || !place.Follow {
		t.Fatalf("saved place = %+v, want a known follow", place)
	}
}

func TestScrollPlaceStatusRoundTrip(t *testing.T) {
	m, _ := unreadDemoModel(t)
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyPgUp})
	body := m.ctrl.Messages()[m.firstVisibleMessageRow()].Body

	m = press(t, m, ctrlKey('`'))
	if !m.ctrl.ConsoleOpen() {
		t.Fatal("Ctrl+` must open Status")
	}
	if !m.transcriptFollowEnd {
		t.Fatal("Status must follow the end")
	}
	m = press(t, m, ctrlKey('`'))
	if m.ctrl.ConsoleOpen() {
		t.Fatal("Ctrl+` must close Status")
	}
	if m.transcriptFollowEnd {
		t.Fatal("closing Status must restore the scrolled conversation")
	}
	if got := m.ctrl.Messages()[m.firstVisibleMessageRow()].Body; got != body {
		t.Fatalf("restored body = %q, want %q", got, body)
	}
}

func TestScrollPlaceSurvivesRestart(t *testing.T) {
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())
	ctrl := controller.New()
	ctrl.SetEphemeral(false)
	if !demo.New().Attach(ctrl, true) {
		t.Fatal("demo Attach failed")
	}
	m := resizeModel(t, New(ctrl, nil), 118, 20)
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyPgUp})
	if m.transcriptFollowEnd {
		t.Fatal("Page Up must leave follow-the-end")
	}
	body := m.ctrl.Messages()[m.firstVisibleMessageRow()].Body
	m.rememberOpenTranscript()

	again := controller.New()
	again.SetEphemeral(false)
	if !demo.New().Attach(again, true) {
		t.Fatal("restart Attach failed")
	}
	restored := resizeModel(t, New(again, nil), 118, 20)
	if restored.transcriptFollowEnd {
		t.Fatal("a restart must reopen the scrolled line")
	}
	if got := restored.ctrl.Messages()[restored.firstVisibleMessageRow()].Body; got != body {
		t.Fatalf("restart body = %q, want %q", got, body)
	}
	if strings.TrimSpace(body) == "" {
		t.Fatal("restart anchor must be a real line")
	}
}
