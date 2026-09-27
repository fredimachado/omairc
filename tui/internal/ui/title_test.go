package ui

import (
	"testing"

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
			if got := Title(ctrl); got != test.want {
				t.Fatalf("Title() = %q, want %q", got, test.want)
			}
		})
	}
}

func TestTitleEmptyController(t *testing.T) {
	if got := Title(controller.New()); got != "Omairc" {
		t.Fatalf("Title(empty) = %q, want %q", got, "Omairc")
	}
	if got := Title(nil); got != "Omairc" {
		t.Fatalf("Title(nil) = %q, want %q", got, "Omairc")
	}
}
