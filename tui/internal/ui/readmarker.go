package ui

func (m *Model) syncReadMarkerViewport() {
	if m.ctrl == nil {
		return
	}
	m.ctrl.NoteTranscriptViewport(m.windowActive, m.transcriptFollowEnd)
}
