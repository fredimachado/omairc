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
	if len(paths) > 0 {
		m.enqueueFilePaths(paths)
		return true
	}
	bad := nonRegularPastePaths(text)
	if len(bad) == 0 {
		return false
	}
	m.enqueueFilePaths(bad[:1])
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

// queuedFile is one path waiting to upload, with the draft and network that
// were current when it was queued.
type queuedFile struct {
	path      string
	draftKey  string
	networkID string
}

func (m *Model) enqueueFilePaths(paths []string) {
	if m == nil || m.ctrl == nil || m.ctrl.FileHost() == "" || len(paths) == 0 {
		return
	}
	draftKey := m.composerDraftKey()
	networkID := m.ctrl.FocusedNetworkID()
	for _, path := range paths {
		m.fileQueue = append(m.fileQueue, queuedFile{
			path: path, draftKey: draftKey, networkID: networkID,
		})
	}
	m.pumpFileUploads()
}

func (m *Model) pumpFileUploads() {
	if m == nil || m.ctrl == nil || m.fileActive != nil || m.ctrl.FileUploadBusy() {
		return
	}
	for len(m.fileQueue) > 0 {
		entry := m.fileQueue[0]
		m.fileQueue = m.fileQueue[1:]
		started := entry
		m.fileActive = &started
		if m.ctrl.BeginFileUpload(entry.path) {
			return
		}
		m.fileActive = nil
		m.ctrl.NoteFileUploadFailure(entry.networkID, "Could not upload the file.")
	}
}

func (m *Model) finishFileLink(msg FileLinkMsg) {
	if m == nil {
		return
	}
	started := m.fileActive
	m.fileActive = nil
	if m.ctrl != nil {
		m.ctrl.FinishFileUpload()
		if msg.Message != "" && started != nil {
			m.ctrl.NoteFileUploadFailure(started.networkID, msg.Message)
		}
	}
	if msg.URL != "" && started != nil {
		m.insertFileLink(msg.URL, started.draftKey)
	}
	m.pumpFileUploads()
}

func appendStoredFileLink(stored, raw string) string {
	if stored == "" {
		return raw
	}
	if strings.HasSuffix(stored, " ") {
		return stored + raw
	}
	return stored + " " + raw
}

// insertFileLink writes an allowlisted web address into the draft that was
// current when the file was queued. The caret moves only when that draft is
// still on screen, find is off, and history is not being browsed.
func (m *Model) insertFileLink(raw, draftKey string) {
	if m == nil || !isAllowedHTTPURL(raw) {
		return
	}
	current := m.composerDraftKey()
	if draftKey == current && !m.find.active && m.composerHistoryIndex < 0 {
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
		return
	}
	if draftKey == current && m.find.active {
		m.find.draft = appendStoredFileLink(m.find.draft, raw)
		return
	}
	if draftKey == current && m.composerHistoryIndex >= 0 {
		m.composerHistoryDraft = appendStoredFileLink(m.composerHistoryDraft, raw)
		return
	}
	if m.drafts == nil {
		m.drafts = map[string]string{}
	}
	m.drafts[draftKey] = appendStoredFileLink(m.drafts[draftKey], raw)
}

func pasteLines(text string) []string {
	text = strings.ReplaceAll(text, "\r\n", "\n")
	text = strings.TrimSpace(text)
	if text == "" {
		return nil
	}
	var lines []string
	for _, line := range strings.Split(text, "\n") {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		lines = append(lines, line)
	}
	return lines
}

// localFileLines returns every absolute regular file in text. One line that
// is not a regular file rejects the whole paste so the caller can decide
// whether it is a directory paste or ordinary text.
func localFileLines(text string) []string {
	lines := pasteLines(text)
	if len(lines) == 0 {
		return nil
	}
	var paths []string
	for _, line := range lines {
		path, ok := absoluteRegularFile(line)
		if !ok {
			return nil
		}
		paths = append(paths, path)
	}
	return paths
}

// nonRegularPastePaths returns the non-regular paths when every non-empty
// line is an existing absolute path and at least one is not a regular file.
// Any other paste returns nil so it stays text.
func nonRegularPastePaths(text string) []string {
	lines := pasteLines(text)
	if len(lines) == 0 {
		return nil
	}
	var bad []string
	for _, line := range lines {
		path, ok := localPathToken(line)
		if !ok {
			return nil
		}
		info, err := os.Stat(path)
		if err != nil {
			return nil
		}
		if !info.Mode().IsRegular() {
			bad = append(bad, path)
		}
	}
	if len(bad) == 0 {
		return nil
	}
	return bad
}

func localPathToken(token string) (string, bool) {
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
	return path, true
}

func absoluteRegularFile(token string) (string, bool) {
	path, ok := localPathToken(token)
	if !ok {
		return "", false
	}
	info, err := os.Stat(path)
	if err != nil || !info.Mode().IsRegular() {
		return "", false
	}
	return path, true
}
