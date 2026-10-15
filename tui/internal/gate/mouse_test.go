package gate

import "testing"

func TestSGRSequences(t *testing.T) {
	if got, want := string(clickSequence(0, 0)), "\x1b[<0;1;1M\x1b[<0;1;1m"; got != want {
		t.Fatalf("click origin = %q, want %q", got, want)
	}
	if got, want := string(clickSequence(29, 4)), "\x1b[<0;30;5M\x1b[<0;30;5m"; got != want {
		t.Fatalf("click = %q, want %q", got, want)
	}
	if got, want := string(wheelSequence(10, 8, true)), "\x1b[<64;11;9M"; got != want {
		t.Fatalf("wheel up = %q, want %q", got, want)
	}
	if got, want := string(wheelSequence(10, 8, false)), "\x1b[<65;11;9M"; got != want {
		t.Fatalf("wheel down = %q, want %q", got, want)
	}
}

func TestAimedColumnsMatchShellLayout(t *testing.T) {
	// These splits are ui.columnLayout for a joined channel with the sidebar
	// and the member column wanted. layout_width_test.go locks the same numbers.
	cases := []struct {
		cols            int
		sidebar, x0, x1 int
	}{
		{118, 29, 96, 118},
		{80, 20, 65, 80},
		{160, 30, 138, 160},
		{60, 15, 60, 60},
	}
	for _, tc := range cases {
		sidebar, x0, x1 := aimedColumns(tc.cols)
		if sidebar != tc.sidebar || x0 != tc.x0 || x1 != tc.x1 {
			t.Fatalf("aimedColumns(%d) = %d,%d,%d, want %d,%d,%d",
				tc.cols, sidebar, x0, x1, tc.sidebar, tc.x0, tc.x1)
		}
	}
}

func TestFindInColumns(t *testing.T) {
	lines := []string{
		"topic lives in the transcript only",
		"sidebar #desktop          anna stays left",
		"                         anna",
	}
	x, y, ok := findInColumns(lines, "#desktop", 0, 20, false)
	if !ok || y != 1 || x < 8 {
		t.Fatalf("sidebar #desktop = %d,%d,%v", x, y, ok)
	}
	if _, _, ok := findInColumns(lines, "#desktop", 20, 40, false); ok {
		t.Fatal("transcript column must not claim the sidebar #desktop")
	}
	x, y, ok = findInColumns(lines, "anna", 20, 40, true)
	if !ok || y != 2 {
		t.Fatalf("rightmost anna = %d,%d,%v, want the member row", x, y, ok)
	}
	// A wide rune before the name counts as one cell, not three bytes.
	wide := []string{"● #help"}
	x, y, ok = findInColumns(wide, "#help", 0, 10, false)
	if !ok || x != 2 || y != 0 {
		t.Fatalf("wide prefix click = %d,%d,%v, want cell 2", x, y, ok)
	}
}

func TestParseCellAndWheel(t *testing.T) {
	x, y, err := parseCellPoint("click", []string{"--x", "0", "--y", "4"})
	if err != nil || x != 0 || y != 4 {
		t.Fatalf("parseCellPoint = %d,%d,%v", x, y, err)
	}
	if _, _, err := parseCellPoint("click", []string{"--x", "1"}); err == nil {
		t.Fatal("click without --y should error")
	}
	x, y, up, err := parseWheelPoint([]string{"--y", "3", "--x", "2", "--up"})
	if err != nil || x != 2 || y != 3 || !up {
		t.Fatalf("parseWheelPoint = %d,%d,%v,%v", x, y, up, err)
	}
	if _, _, _, err := parseWheelPoint([]string{"--x", "1", "--y", "1", "--up", "--down"}); err == nil {
		t.Fatal("wheel with both directions should error")
	}
}
