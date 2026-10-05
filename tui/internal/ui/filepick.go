package ui

import (
	"net/url"
	"os"
	"path/filepath"
	"strings"

	"charm.land/bubbles/v2/textinput"
	tea "charm.land/bubbletea/v2"
	"github.com/atotto/clipboard"
)

// This file is the terminal file-link path. Paste a path or pick one with
// Ctrl+Shift+U and, when the server offers a file host, the resolved web
// link is inserted into the draft. There is no drop target.

const filePickPlaceholder = "File path"

// filePickState is the Ctrl+Shift+U path prompt.
type filePickState struct {
	open  bool
	input textinput.Model
}

// FileLinkMsg is the upload goroutine's result, delivered on the Update
// goroutine. URL is empty when Message is a fixed failure string.
type FileLinkMsg struct {
	URL     string
	Message string
}

func newFilePickState() filePickState {
	return filePickState{input: newTextInput(defaultStyles(), filePickPlaceholder, overlayFilterPrompt)}
}

func (m *Model) filePickVisible() bool { return m != nil && m.file.open }

// openFilePick opens the path prompt. It does nothing unless the selected
// server offers a file host, and nothing while find is active.
func (m *Model) openFilePick() {
	if m == nil || m.ctrl == nil || m.find.active || m.ctrl.FileHost() == "" {
		return
	}
	m.saveDraft()
	m.closeAllOverlays()
	m.file.open = true
	m.file.input = newTextInput(m.styles, filePickPlaceholder, overlayFilterPrompt)
	m.file.input.SetWidth(m.overlayInputWidth())
	m.composer.Blur()
	_ = m.file.input.Focus()
}

func (m *Model) closeFilePick() {
	if !m.file.open {
		return
	}
	m.file.open = false
	m.file.input.Blur()
	_ = m.composer.Focus()
	m.loadDraft()
}

func (m *Model) submitFilePick() {
	path := strings.TrimSpace(m.file.input.Value())
	m.closeFilePick()
	if path != "" {
		m.enqueueFilePaths([]string{path})
	}
}

func (m *Model) handleFilePickKey(key string, msg tea.KeyPressMsg) (tea.Model, tea.Cmd) {
	switch key {
	case "esc", "escape":
		m.closeFilePick()
		return m, nil
	case "enter", "return":
		m.submitFilePick()
		return m, nil
	}
	var cmd tea.Cmd
	m.file.input, cmd = m.file.input.Update(msg)
	return m, cmd
}

func (m *Model) fileCard(width int) string {
	return m.overlayCardBlock(width, m.fileCardBody)
}

func (m *Model) fileCardBody(inner int) []string {
	lines := m.overlaySheetHeader(inner, "File", 0)
	lines = append(lines, truncateLine(m.file.input.View(), inner))
	return lines
}

// tryUploadPastedText uploads when every line is an existing absolute file
// or file:// URL. Ordinary text returns false so the composer pastes it.
func (m *Model) tryUploadPastedText(text string) bool {
	if m == nil || m.find.active || m.ctrl == nil || m.ctrl.FileHost() == "" {
		return false
	}
	paths := localFileLines(text)
	if len(paths) == 0 {
		return false
	}
	m.enqueueFilePaths(paths)
	return true
}

// tryUploadClipboard reads the clipboard and uploads when it is a list of
// local files. A read error or ordinary text returns false.
func (m *Model) tryUploadClipboard() bool {
	if m == nil || m.find.active || m.ctrl == nil || m.ctrl.FileHost() == "" {
		return false
	}
	text, err := clipboard.ReadAll()
	if err != nil {
		return false
	}
	return m.tryUploadPastedText(text)
}

func (m *Model) enqueueFilePaths(paths []string) {
	if m == nil || m.ctrl == nil || m.ctrl.FileHost() == "" || len(paths) == 0 {
		return
	}
	m.fileQueue = append(m.fileQueue, paths...)
	m.pumpFileUploads()
}

func (m *Model) pumpFileUploads() {
	if m == nil || m.ctrl == nil || m.ctrl.FileUploadBusy() || len(m.fileQueue) == 0 {
		return
	}
	path := m.fileQueue[0]
	m.fileQueue = m.fileQueue[1:]
	if !m.ctrl.BeginFileUpload(path) {
		m.fileQueue = nil
	}
}

func (m *Model) finishFileLink(msg FileLinkMsg) {
	if m == nil {
		return
	}
	if m.ctrl != nil {
		m.ctrl.FinishFileUpload()
		if msg.Message != "" {
			m.ctrl.NoteFileUploadFailure(msg.Message)
		}
	}
	if msg.URL != "" {
		m.insertFileLink(msg.URL)
	}
	m.pumpFileUploads()
}

// insertFileLink inserts an allowlisted web address at the caret, with a
// space on either side when the neighboring character is not already one.
func (m *Model) insertFileLink(raw string) {
	if m == nil || m.find.active || !isAllowedHTTPURL(raw) {
		return
	}
	runes := []rune(m.composer.Value())
	cursor := m.composer.Position()
	if cursor < 0 {
		cursor = 0
	}
	if cursor > len(runes) {
		cursor = len(runes)
	}
	piece := []rune(raw)
	if cursor > 0 && runes[cursor-1] != ' ' {
		piece = append([]rune{' '}, piece...)
	}
	if cursor < len(runes) && runes[cursor] != ' ' {
		piece = append(piece, ' ')
	}
	result := make([]rune, 0, len(runes)+len(piece))
	result = append(result, runes[:cursor]...)
	result = append(result, piece...)
	result = append(result, runes[cursor:]...)
	m.composer.SetValue(string(result))
	m.composer.SetCursor(cursor + len(piece))
	m.saveDraft()
}

// localFileLines returns every absolute regular file in text. One line that
// is not a file rejects the whole paste so it stays text.
func localFileLines(text string) []string {
	text = strings.ReplaceAll(text, "\r\n", "\n")
	text = strings.TrimSpace(text)
	if text == "" {
		return nil
	}
	var paths []string
	for _, line := range strings.Split(text, "\n") {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		path, ok := absoluteRegularFile(line)
		if !ok {
			return nil
		}
		paths = append(paths, path)
	}
	return paths
}

func absoluteRegularFile(token string) (string, bool) {
	path := token
	if strings.HasPrefix(strings.ToLower(token), "file://") {
		parsed, err := url.Parse(token)
		if err != nil || !strings.EqualFold(parsed.Scheme, "file") {
			return "", false
		}
		if parsed.Host != "" && !strings.EqualFold(parsed.Host, "localhost") {
			return "", false
		}
		path = parsed.Path
		if decoded, err := url.PathUnescape(path); err == nil {
			path = decoded
		}
	}
	if !filepath.IsAbs(path) {
		return "", false
	}
	info, err := os.Stat(path)
	if err != nil || !info.Mode().IsRegular() {
		return "", false
	}
	return path, true
}
