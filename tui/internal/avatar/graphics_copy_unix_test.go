//go:build unix

package avatar

import (
	"os"
	"testing"
)

func TestWakeSignaled(t *testing.T) {
	wakeR, wakeW, err := os.Pipe()
	if err != nil {
		t.Fatal(err)
	}
	defer wakeR.Close()
	if wakeSignaled(wakeR) {
		t.Fatal("idle wake pipe must not signal stop")
	}
	if err := wakeW.Close(); err != nil {
		t.Fatal(err)
	}
	if !wakeSignaled(wakeR) {
		t.Fatal("closed wake pipe must signal stop")
	}
}
