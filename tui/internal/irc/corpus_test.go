package irc

import (
	"os"
	"path/filepath"
	"reflect"
	"testing"
)

type expectedTag struct {
	name  string
	value *string
}

type expectedFrame struct {
	err     Error
	command string
	tags    []expectedTag
	prefix  *Prefix
	params  []string
}

type expectedFault struct {
	err       Error
	byteCount int
	preview   string
}

type corpusCase struct {
	fileName string
	frames   []expectedFrame
	faults   []expectedFault
}

func strptr(value string) *string {
	return &value
}

// corpusCases mirrors the registry in tests/protocol/tst_corpus.cpp verbatim.
var corpusCases = []corpusCase{
	{
		fileName: "01-classic-privmsg.txt",
		frames: []expectedFrame{{
			err:     ErrorNone,
			command: "PRIVMSG",
			prefix:  &Prefix{Raw: "n!u@h", Nick: "n", User: "u", Host: "h"},
			params:  []string{"#c", "hello world"},
		}},
	},
	{
		fileName: "02-tagged-privmsg.txt",
		frames: []expectedFrame{{
			err:     ErrorNone,
			command: "PRIVMSG",
			tags: []expectedTag{
				{name: "msgid", value: strptr("abc")},
				{name: "+typing", value: strptr("active")},
			},
			prefix: &Prefix{Raw: "n!u@h", Nick: "n", User: "u", Host: "h"},
			params: []string{"#c", "tagged"},
		}},
	},
	{
		fileName: "03-latin1-e9-body.txt",
		frames: []expectedFrame{{
			err:     ErrorNone,
			command: "PRIVMSG",
			prefix:  &Prefix{Raw: "n!u@h", Nick: "n", User: "u", Host: "h"},
			params:  []string{"#c", "caf\xe9"},
		}},
	},
	{
		fileName: "04-overlong.txt",
		frames: []expectedFrame{{
			err:     ErrorNone,
			command: repeatString("X", 513),
		}},
	},
	{
		fileName: "05-nul.txt",
		faults: []expectedFault{{
			err:       ErrorInvalidCharacter,
			byteCount: 18,
			preview:   "PRIVMSG #c :hel\x00lo",
		}},
	},
}

func TestCorpusRegistryMatchesDirectory(t *testing.T) {
	entries, err := os.ReadDir(filepath.Join("testdata", "corpus"))
	if err != nil {
		t.Fatalf("read corpus dir: %v", err)
	}

	actual := map[string]bool{}
	for _, entry := range entries {
		if entry.IsDir() || filepath.Ext(entry.Name()) != ".txt" {
			continue
		}
		actual[entry.Name()] = true
	}

	registered := map[string]bool{}
	for _, testCase := range corpusCases {
		if registered[testCase.fileName] {
			t.Fatalf("duplicate corpus case: %s", testCase.fileName)
		}
		registered[testCase.fileName] = true
	}

	if !reflect.DeepEqual(actual, registered) {
		t.Fatalf("corpus files = %v, registry = %v", actual, registered)
	}
}

func TestCorpusReplays(t *testing.T) {
	chunkings := []string{"whole", "halves", "bytes"}
	for _, testCase := range corpusCases {
		count := 3
		if len(testCase.faults) > 0 {
			count = 1
		}
		for chunking := 0; chunking < count; chunking++ {
			name := testCase.fileName + ":" + chunkings[chunking]
			t.Run(name, func(t *testing.T) {
				replayCorpus(t, testCase, chunking)
			})
		}
	}
}

func replayCorpus(t *testing.T, expected corpusCase, chunking int) {
	t.Helper()

	raw, err := os.ReadFile(filepath.Join("testdata", "corpus", expected.fileName))
	if err != nil {
		t.Fatalf("read %s: %v", expected.fileName, err)
	}

	var framer Framer
	var frames []string
	var faults []FrameFault
	feed := func(offset, size int) {
		result := framer.Feed(raw[offset : offset+size])
		frames = append(frames, result.Frames...)
		faults = append(faults, result.Faults...)
	}

	switch chunking {
	case 0:
		feed(0, len(raw))
	case 1:
		half := len(raw) / 2
		feed(0, half)
		feed(half, len(raw)-half)
	default:
		for index := 0; index < len(raw); index++ {
			feed(index, 1)
		}
	}

	if len(frames) != len(expected.frames) {
		t.Fatalf("frames = %q, want %d", frames, len(expected.frames))
	}
	if len(faults) != len(expected.faults) {
		t.Fatalf("faults = %+v, want %d", faults, len(expected.faults))
	}

	for index := range faults {
		if faults[index].Err != expected.faults[index].err {
			t.Fatalf("fault %d err = %v, want %v", index, faults[index].Err, expected.faults[index].err)
		}
		if faults[index].ByteCount != expected.faults[index].byteCount {
			t.Fatalf("fault %d bytes = %d, want %d", index, faults[index].ByteCount, expected.faults[index].byteCount)
		}
		if faults[index].Preview != expected.faults[index].preview {
			t.Fatalf("fault %d preview = %q, want %q", index, faults[index].Preview, expected.faults[index].preview)
		}
	}

	for index := range frames {
		wanted := expected.frames[index]
		message, parseErr := Parse(frames[index])
		if got := errorValue(parseErr); got != wanted.err {
			t.Fatalf("frame %d parse error = %v, want %v", index, got, wanted.err)
		}
		if wanted.err != ErrorNone {
			continue
		}
		if message.Command != wanted.command {
			t.Fatalf("frame %d command = %q, want %q", index, message.Command, wanted.command)
		}
		if !reflect.DeepEqual(message.Params, wanted.params) {
			t.Fatalf("frame %d params = %q, want %q", index, message.Params, wanted.params)
		}
		if len(message.Tags) != len(wanted.tags) {
			t.Fatalf("frame %d tags = %+v, want %+v", index, message.Tags, wanted.tags)
		}
		for tagIndex := range message.Tags {
			if message.Tags[tagIndex].Name != wanted.tags[tagIndex].name {
				t.Fatalf("frame %d tag %d name = %q, want %q",
					index, tagIndex, message.Tags[tagIndex].Name, wanted.tags[tagIndex].name)
			}
			gotValue := message.Tags[tagIndex].Value
			wantValue := wanted.tags[tagIndex].value
			if (gotValue == nil) != (wantValue == nil) {
				t.Fatalf("frame %d tag %d value presence differs", index, tagIndex)
			}
			if gotValue != nil && *gotValue != *wantValue {
				t.Fatalf("frame %d tag %d value = %q, want %q",
					index, tagIndex, *gotValue, *wantValue)
			}
		}
		if (message.Prefix == nil) != (wanted.prefix == nil) {
			t.Fatalf("frame %d prefix presence differs", index)
		}
		if wanted.prefix != nil {
			if message.Prefix.Raw != wanted.prefix.Raw ||
				message.Prefix.Nick != wanted.prefix.Nick ||
				message.Prefix.User != wanted.prefix.User ||
				message.Prefix.Host != wanted.prefix.Host {
				t.Fatalf("frame %d prefix = %+v, want %+v", index, message.Prefix, wanted.prefix)
			}
		}
	}
}

func repeatString(value string, count int) string {
	result := make([]byte, 0, len(value)*count)
	for index := 0; index < count; index++ {
		result = append(result, value...)
	}
	return string(result)
}
