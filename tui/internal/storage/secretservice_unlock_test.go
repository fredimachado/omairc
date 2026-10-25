package storage

import "testing"

func TestSecretServiceUnlockIncomplete(t *testing.T) {
	cases := []struct {
		name     string
		items    []string
		unlocked []string
		prompt   string
		want     bool
	}{
		{
			name:     "all unlocked no prompt",
			items:    []string{"/a", "/b"},
			unlocked: []string{"/a", "/b"},
			prompt:   "/",
			want:     false,
		},
		{
			name:     "empty search",
			items:    nil,
			unlocked: nil,
			prompt:   "/",
			want:     false,
		},
		{
			name:     "partial unlock keeps stale item",
			items:    []string{"/dual", "/qt-single"},
			unlocked: []string{"/dual"},
			prompt:   "/",
			want:     true,
		},
		{
			name:     "prompt for remaining items",
			items:    []string{"/dual", "/qt-single"},
			unlocked: []string{"/dual"},
			prompt:   "/org/freedesktop/secrets/prompt/1",
			want:     true,
		},
		{
			name:     "prompt with nothing unlocked",
			items:    []string{"/a"},
			unlocked: nil,
			prompt:   "/org/freedesktop/secrets/prompt/1",
			want:     true,
		},
		{
			name:     "nothing unlocked without prompt",
			items:    []string{"/a"},
			unlocked: nil,
			prompt:   "/",
			want:     true,
		},
		{
			name:     "empty prompt string is not interactive",
			items:    []string{"/a"},
			unlocked: []string{"/a"},
			prompt:   "",
			want:     false,
		},
		{
			name:     "unlocked superset still complete",
			items:    []string{"/a"},
			unlocked: []string{"/a", "/b"},
			prompt:   "/",
			want:     false,
		},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got := secretServiceUnlockIncomplete(tc.items, tc.unlocked, tc.prompt)
			if got != tc.want {
				t.Fatalf("incomplete = %v, want %v", got, tc.want)
			}
		})
	}
}
