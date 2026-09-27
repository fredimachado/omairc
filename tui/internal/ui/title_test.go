package ui

import (
	"testing"

	"github.com/fredimachado/omairc/tui/internal/connection"
	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/demo"
)

func seededTitleController(t *testing.T) *controller.Controller {
	t.Helper()
	ctrl := controller.New()
	if !demo.New().Attach(ctrl, true) {
		t.Fatal("demo Attach failed")
	}
	return ctrl
}

func TestTitleSeededCases(t *testing.T) {
	ctrl := seededTitleController(t)

	tests := []struct {
		name       string
		networkID  string
		target     string
		statusOpen string
		want       string
	}{
		{
			name:      "duplicated channel appends the roster name",
			networkID: "omarchy",
			target:    "#omarchy",
			want:      "#omarchy · irc.example · fred - Omairc",
		},
		{
			name:      "unique channel",
			networkID: "omarchy",
			target:    "#ricing",
			want:      "#ricing - Omairc",
		},
		{
			name:      "direct message",
			networkID: "omarchy",
			target:    "dax",
			want:      "dax - Omairc",
		},
		{
			name:       "status surface",
			statusOpen: "omarchy",
			want:       "irc.example · fred Status",
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			if test.statusOpen != "" {
				ctrl.OpenStatus(test.statusOpen)
			} else {
				ctrl.SelectConversation(test.networkID, test.target)
			}
			if got := Title(ctrl, nil); got != test.want {
				t.Fatalf("Title() = %q, want %q", got, test.want)
			}
		})
	}
}

func TestTitleFirstRunUsesConnectionDisplayName(t *testing.T) {
	ctrl := controller.New()
	conn := connection.New(ctrl, nil)
	if got := Title(ctrl, conn); got != "irc.libera.chat Status" {
		t.Fatalf("Title(first run) = %q, want %q", got, "irc.libera.chat Status")
	}
}

func TestTitleEmptyController(t *testing.T) {
	if got := Title(controller.New(), nil); got != "Status" {
		t.Fatalf("Title(empty) = %q, want %q", got, "Status")
	}
	if got := Title(nil, nil); got != "Omairc" {
		t.Fatalf("Title(nil) = %q, want %q", got, "Omairc")
	}
}
