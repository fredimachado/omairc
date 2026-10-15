package avatar

import (
	"bytes"
	"encoding/base64"
	"fmt"
	"image"
	"image/png"
	"io"
	"os"
	"strings"
	"time"

	"github.com/charmbracelet/x/term"
)

const (
	// graphicsChunkSize is the Kitty graphics protocol's maximum base64 chunk.
	// 4096 is a multiple of 4, so every chunk but a short tail stays aligned.
	graphicsChunkSize = 4096

	// placeholderRune is the Kitty Unicode placeholder (U+10EEEE). Row and
	// column diacritics combine with it; the image id lives in the cell's
	// foreground color.
	placeholderRune = '\U0010EEEE'

	// probeImageID is the id on the startup query. a=q does not store the
	// image, and the id sits outside the 24-bit range the placeholder color
	// can name, so it cannot collide with an avatar we transmit.
	probeImageID = 4294967295

	// ProbeTimeout is how long startup waits for a Kitty graphics reply.
	// Terminals that support the protocol answer immediately; anything else
	// falls back to the half-block raster.
	ProbeTimeout = 150 * time.Millisecond
)

// graphicsOp is one transmit or delete waiting to be written outside the view.
type graphicsOp struct {
	id     uint32
	png    []byte
	remove bool
}

// QueryEscape is the startup graphics probe: a 1x1 black RGB pixel with a=q,
// which a supporting terminal answers with OK and does not store.
func QueryEscape() string {
	return apc(fmt.Sprintf("i=%d,s=1,v=1,a=q,t=d,f=24", probeImageID), "AAAA")
}

// TransmitPNG builds the quiet (q=2) chunked transmit for one PNG. The first
// chunk carries a=t, the image id, f=100, and U=1. Every chunk carries m=1
// until the last, which carries m=0. Later chunks carry only m and q.
func TransmitPNG(id uint32, pngBytes []byte) string {
	if id == 0 || len(pngBytes) == 0 {
		return ""
	}
	encoded := base64.StdEncoding.EncodeToString(pngBytes)
	chunks := splitBase64(encoded, graphicsChunkSize)
	var b strings.Builder
	for i, chunk := range chunks {
		more := "1"
		if i == len(chunks)-1 {
			more = "0"
		}
		var control string
		if i == 0 {
			control = fmt.Sprintf("a=t,f=100,i=%d,q=2,U=1,m=%s", id, more)
		} else {
			control = fmt.Sprintf("m=%s,q=2", more)
		}
		b.WriteString(apc(control, chunk))
	}
	return b.String()
}

// VirtualPlacement builds the invisible placement a placeholder cell refers to.
func VirtualPlacement(id uint32, cols, rows int) string {
	if id == 0 || cols <= 0 || rows <= 0 {
		return ""
	}
	return apc(fmt.Sprintf("a=p,U=1,i=%d,c=%d,r=%d,q=2", id, cols, rows), "")
}

// DeleteImage builds an a=d command that frees one of our images. d=I deletes
// the stored image (a bare a=d only clears visible placements, which does not
// cover a virtual Unicode placement). q=2 keeps the reply off the input stream.
func DeleteImage(id uint32) string {
	if id == 0 {
		return ""
	}
	return apc(fmt.Sprintf("a=d,d=I,i=%d,q=2", id), "")
}

// Placeholder renders rows*cols Unicode placeholder cells for id. The image id's
// low 24 bits are the foreground color. Each cell is U+10EEEE plus the row
// diacritic and the column diacritic. A multi-row result contains newlines;
// the sidebar glyph uses a single row.
func Placeholder(id uint32, cols, rows int) string {
	if id == 0 || cols <= 0 || rows <= 0 {
		return ""
	}
	r, g, b := idColor(id)
	var bld strings.Builder
	fmt.Fprintf(&bld, "\x1b[38;2;%d;%d;%dm", r, g, b)
	for row := 0; row < rows; row++ {
		if row > 0 {
			bld.WriteByte('\n')
		}
		for col := 0; col < cols; col++ {
			cell, ok := placeholderCell(row, col)
			if !ok {
				return ""
			}
			bld.WriteString(cell)
		}
	}
	bld.WriteString("\x1b[0m")
	return bld.String()
}

// idColor maps an image id to the 24-bit foreground color Kitty reads back.
func idColor(id uint32) (r, g, b int) {
	return int((id >> 16) & 0xFF), int((id >> 8) & 0xFF), int(id & 0xFF)
}

func placeholderCell(row, col int) (string, bool) {
	rowMark, okRow := diacritic(row)
	colMark, okCol := diacritic(col)
	if !okRow || !okCol {
		return "", false
	}
	return string([]rune{placeholderRune, rowMark, colMark}), true
}

func diacritic(n int) (rune, bool) {
	if n < 0 || n >= len(rowColumnDiacritics) {
		return 0, false
	}
	return rowColumnDiacritics[n], true
}

func apc(control, payload string) string {
	if payload == "" {
		return "\x1b_G" + control + "\x1b\\"
	}
	return "\x1b_G" + control + ";" + payload + "\x1b\\"
}

func splitBase64(payload string, size int) []string {
	if payload == "" {
		return nil
	}
	if size <= 0 {
		return []string{payload}
	}
	var chunks []string
	for len(payload) > size {
		chunks = append(chunks, payload[:size])
		payload = payload[size:]
	}
	chunks = append(chunks, payload)
	return chunks
}

// ProbeGraphics asks the terminal whether it implements the Kitty graphics
// protocol. It enables raw mode for the wait, restores it, and reports false
// when the descriptor is not a terminal, the probe cannot be written, or the
// reply does not arrive before timeout. The wait polls the descriptor: a PTY
// rejects SetReadDeadline, and a blocked read would steal later input.
// Support is never inferred from TERM.
func ProbeGraphics(in, out *os.File, timeout time.Duration) bool {
	if in == nil || out == nil {
		return false
	}
	if timeout <= 0 {
		timeout = ProbeTimeout
	}
	fd := in.Fd()
	if !term.IsTerminal(fd) {
		return false
	}
	state, err := term.MakeRaw(fd)
	if err != nil {
		return false
	}
	defer func() { _ = term.Restore(fd, state) }()
	return readProbeReply(out, in, timeout)
}

// ReadGraphicsSupport writes the a=q probe to w and reports whether r returns
// a graphics OK before timeout. r must honor SetReadDeadline so a missing
// reply cannot block the caller past timeout. Real terminals reject that
// deadline; ProbeGraphics polls the file descriptor instead.
func ReadGraphicsSupport(w io.Writer, r interface {
	io.Reader
	SetReadDeadline(time.Time) error
}, timeout time.Duration) bool {
	if w == nil || r == nil || timeout <= 0 {
		return false
	}
	deadline := time.Now().Add(timeout)
	if err := r.SetReadDeadline(deadline); err != nil {
		return false
	}
	defer func() { _ = r.SetReadDeadline(time.Time{}) }()
	if _, err := io.WriteString(w, QueryEscape()); err != nil {
		return false
	}
	buf := make([]byte, 0, 512)
	tmp := make([]byte, 256)
	for {
		n, err := r.Read(tmp)
		if n > 0 {
			buf = append(buf, tmp[:n]...)
			if stop, ok := absorbGraphics(buf); stop {
				return ok
			}
		}
		if err != nil || !time.Now().Before(deadline) {
			return false
		}
	}
}

// absorbGraphics reports whether buf ends the probe and whether the terminal
// accepted graphics. stop is false while the reply is still incomplete.
func absorbGraphics(buf []byte) (stop, ok bool) {
	if replyOK(buf) {
		return true, true
	}
	if len(buf) > 4096 || responseSettled(buf) {
		return true, false
	}
	return false, false
}

// replyOK reports whether buf contains a graphics APC whose status is exactly OK.
func replyOK(buf []byte) bool {
	s := string(buf)
	for {
		i := strings.Index(s, "\x1b_G")
		if i < 0 {
			return false
		}
		s = s[i+3:]
		end := strings.Index(s, "\x1b\\")
		msg := s
		if end >= 0 {
			msg = s[:end]
			s = s[end+2:]
		}
		semi := strings.LastIndex(msg, ";")
		if semi >= 0 && msg[semi+1:] == "OK" {
			return true
		}
		if end < 0 {
			return false
		}
	}
}

// responseSettled reports whether buf holds a finished graphics APC that is
// not OK, so the probe can stop without using the rest of the timeout.
func responseSettled(buf []byte) bool {
	s := string(buf)
	i := strings.Index(s, "\x1b_G")
	if i < 0 {
		return false
	}
	s = s[i+3:]
	end := strings.Index(s, "\x1b\\")
	if end < 0 {
		return false
	}
	msg := s[:end]
	semi := strings.LastIndex(msg, ";")
	return semi >= 0 && msg[semi+1:] != "OK"
}

// EnableGraphics turns on transmit/delete tracking. Call it only after
// ProbeGraphics reports support, and before fetches complete, so each cached
// image is transmitted once.
func (s *Store) EnableGraphics() {
	if s == nil {
		return
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.graphics {
		return
	}
	s.graphics = true
	if s.nextImage == 0 {
		s.nextImage = 1
	}
	s.ensureGraphicsMapsLocked()
	for _, key := range s.order {
		if img, ok := s.images[key]; ok {
			s.noteGraphicsLocked(key, img)
		}
	}
}

// Placeholder is the sidebar glyph for a transmitted image: one row of cols
// placeholder cells, or false when graphics are off, the image is not drawn
// yet, or the URL is not cached. It never returns a graphics APC.
func (s *Store) Placeholder(rawURL string, layoutPixels, cols int) (string, bool) {
	if s == nil || cols <= 0 {
		return "", false
	}
	key, ok := s.resolvedKey(rawURL, layoutPixels)
	if !ok {
		return "", false
	}
	s.mu.Lock()
	graphics := s.graphics
	id := s.imageIDs[key]
	_, drawn := s.drawnIDs[id]
	s.mu.Unlock()
	if !graphics || id == 0 || !drawn {
		return "", false
	}
	cells := Placeholder(id, cols, 1)
	if cells == "" || strings.Contains(cells, "\x1b_G") {
		return "", false
	}
	return cells, true
}

// DrainEscapes returns the transmit and delete escapes queued since the last
// drain, including a virtual placement of cols by rows for each new image.
// The caller writes the result outside the view. An empty result means there
// is nothing to send.
func (s *Store) DrainEscapes(cols, rows int) string {
	if s == nil || cols <= 0 || rows <= 0 {
		return ""
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if !s.graphics {
		return ""
	}
	ops := s.graphicsOps
	s.graphicsOps = nil
	plans := collapseGraphics(ops)
	var b strings.Builder
	for _, plan := range plans {
		if plan.remove {
			if s.sentIDs[plan.id] {
				b.WriteString(DeleteImage(plan.id))
				delete(s.sentIDs, plan.id)
			}
			delete(s.drawnIDs, plan.id)
			s.inflight = removeUint(s.inflight, plan.id)
			continue
		}
		if len(plan.png) == 0 || plan.id == 0 {
			continue
		}
		b.WriteString(TransmitPNG(plan.id, plan.png))
		b.WriteString(VirtualPlacement(plan.id, cols, rows))
		s.sentIDs[plan.id] = true
		s.inflight = append(s.inflight, plan.id)
	}
	return b.String()
}

// NoteDrawn marks the images from the last drain as safe to reference from
// placeholder cells. Call it only after those escapes have been written.
func (s *Store) NoteDrawn() {
	if s == nil {
		return
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	for _, id := range s.inflight {
		if s.sentIDs[id] {
			s.drawnIDs[id] = struct{}{}
		}
	}
	s.inflight = nil
}

// DeleteEscapes returns a=d escapes for every image this store has transmitted
// and forgets them. Quit writes the result so nothing lingers in the terminal.
func (s *Store) DeleteEscapes() string {
	if s == nil {
		return ""
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	var b strings.Builder
	seen := map[uint32]bool{}
	for id, sent := range s.sentIDs {
		if !sent || id == 0 || seen[id] {
			continue
		}
		seen[id] = true
		b.WriteString(DeleteImage(id))
	}
	s.imageIDs = map[string]uint32{}
	s.sentIDs = map[uint32]bool{}
	s.drawnIDs = map[uint32]struct{}{}
	s.inflight = nil
	s.graphicsOps = nil
	return b.String()
}

func collapseGraphics(ops []graphicsOp) []graphicsOp {
	index := map[uint32]int{}
	var plans []graphicsOp
	for _, op := range ops {
		i, ok := index[op.id]
		if !ok {
			index[op.id] = len(plans)
			plans = append(plans, graphicsOp{id: op.id})
			i = len(plans) - 1
		}
		if op.remove {
			plans[i].remove = true
			plans[i].png = nil
			continue
		}
		plans[i].remove = false
		plans[i].png = op.png
	}
	return plans
}

func (s *Store) ensureGraphicsMapsLocked() {
	if s.imageIDs == nil {
		s.imageIDs = map[string]uint32{}
	}
	if s.sentIDs == nil {
		s.sentIDs = map[uint32]bool{}
	}
	if s.drawnIDs == nil {
		s.drawnIDs = map[uint32]struct{}{}
	}
}

func (s *Store) noteGraphicsLocked(key string, img image.Image) {
	if !s.graphics {
		return
	}
	s.ensureGraphicsMapsLocked()
	if id, ok := s.imageIDs[key]; ok {
		s.queueDeleteLocked(id)
		delete(s.imageIDs, key)
	}
	pngBytes, ok := encodePNG(img)
	if !ok {
		return
	}
	id := s.allocIDLocked()
	if id == 0 {
		return
	}
	s.imageIDs[key] = id
	s.graphicsOps = append(s.graphicsOps, graphicsOp{id: id, png: pngBytes})
}

func (s *Store) dropGraphicsLocked(key string) {
	if !s.graphics {
		return
	}
	id, ok := s.imageIDs[key]
	if !ok {
		return
	}
	delete(s.imageIDs, key)
	s.queueDeleteLocked(id)
}

func (s *Store) queueDeleteLocked(id uint32) {
	if id == 0 {
		return
	}
	delete(s.drawnIDs, id)
	s.inflight = removeUint(s.inflight, id)
	s.graphicsOps = append(s.graphicsOps, graphicsOp{id: id, remove: true})
}

func (s *Store) allocIDLocked() uint32 {
	for range 0xFFFFFF {
		if s.nextImage == 0 || s.nextImage > 0xFFFFFF {
			s.nextImage = 1
		}
		id := s.nextImage
		s.nextImage++
		if !s.idLiveLocked(id) {
			return id
		}
	}
	return 0
}

func (s *Store) idLiveLocked(id uint32) bool {
	for _, existing := range s.imageIDs {
		if existing == id {
			return true
		}
	}
	if s.sentIDs[id] {
		return true
	}
	for _, op := range s.graphicsOps {
		if op.id == id && !op.remove {
			return true
		}
	}
	return false
}

func encodePNG(img image.Image) ([]byte, bool) {
	if img == nil {
		return nil, false
	}
	var buf bytes.Buffer
	if err := png.Encode(&buf, img); err != nil || buf.Len() == 0 {
		return nil, false
	}
	return buf.Bytes(), true
}

func removeUint(ids []uint32, id uint32) []uint32 {
	for i, existing := range ids {
		if existing == id {
			return append(ids[:i], ids[i+1:]...)
		}
	}
	return ids
}

// rowColumnDiacritics is Kitty's row/column table (gen/rowcolumn-diacritics.txt).
// Index 0 is U+0305 and index 1 is U+030D, matching the protocol's 2x2 example.
var rowColumnDiacritics = []rune{
	0x0305, 0x030D, 0x030E, 0x0310, 0x0312, 0x033D, 0x033E, 0x033F,
	0x0346, 0x034A, 0x034B, 0x034C, 0x0350, 0x0351, 0x0352, 0x0357,
	0x035B, 0x0363, 0x0364, 0x0365, 0x0366, 0x0367, 0x0368, 0x0369,
	0x036A, 0x036B, 0x036C, 0x036D, 0x036E, 0x036F, 0x0483, 0x0484,
	0x0485, 0x0486, 0x0487, 0x0592, 0x0593, 0x0594, 0x0595, 0x0597,
	0x0598, 0x0599, 0x059C, 0x059D, 0x059E, 0x059F, 0x05A0, 0x05A1,
	0x05A8, 0x05A9, 0x05AB, 0x05AC, 0x05AF, 0x05C4, 0x0610, 0x0611,
	0x0612, 0x0613, 0x0614, 0x0615, 0x0616, 0x0617, 0x0657, 0x0658,
	0x0659, 0x065A, 0x065B, 0x065D, 0x065E, 0x06D6, 0x06D7, 0x06D8,
	0x06D9, 0x06DA, 0x06DB, 0x06DC, 0x06DF, 0x06E0, 0x06E1, 0x06E2,
	0x06E4, 0x06E7, 0x06E8, 0x06EB, 0x06EC, 0x0730, 0x0732, 0x0733,
	0x0735, 0x0736, 0x073A, 0x073D, 0x073F, 0x0740, 0x0741, 0x0743,
	0x0745, 0x0747, 0x0749, 0x074A, 0x07EB, 0x07EC, 0x07ED, 0x07EE,
	0x07EF, 0x07F0, 0x07F1, 0x07F3, 0x0816, 0x0817, 0x0818, 0x0819,
	0x081B, 0x081C, 0x081D, 0x081E, 0x081F, 0x0820, 0x0821, 0x0822,
	0x0823, 0x0825, 0x0826, 0x0827, 0x0829, 0x082A, 0x082B, 0x082C,
	0x082D, 0x0951, 0x0953, 0x0954, 0x0F82, 0x0F83, 0x0F86, 0x0F87,
	0x135D, 0x135E, 0x135F, 0x17DD, 0x193A, 0x1A17, 0x1A75, 0x1A76,
	0x1A77, 0x1A78, 0x1A79, 0x1A7A, 0x1A7B, 0x1A7C, 0x1B6B, 0x1B6D,
	0x1B6E, 0x1B6F, 0x1B70, 0x1B71, 0x1B72, 0x1B73, 0x1CD0, 0x1CD1,
	0x1CD2, 0x1CDA, 0x1CDB, 0x1CE0, 0x1DC0, 0x1DC1, 0x1DC3, 0x1DC4,
	0x1DC5, 0x1DC6, 0x1DC7, 0x1DC8, 0x1DC9, 0x1DCB, 0x1DCC, 0x1DD1,
	0x1DD2, 0x1DD3, 0x1DD4, 0x1DD5, 0x1DD6, 0x1DD7, 0x1DD8, 0x1DD9,
	0x1DDA, 0x1DDB, 0x1DDC, 0x1DDD, 0x1DDE, 0x1DDF, 0x1DE0, 0x1DE1,
	0x1DE2, 0x1DE3, 0x1DE4, 0x1DE5, 0x1DE6, 0x1DFE, 0x20D0, 0x20D1,
	0x20D4, 0x20D5, 0x20D6, 0x20D7, 0x20DB, 0x20DC, 0x20E1, 0x20E7,
	0x20E9, 0x20F0, 0x2CEF, 0x2CF0, 0x2CF1, 0x2DE0, 0x2DE1, 0x2DE2,
	0x2DE3, 0x2DE4, 0x2DE5, 0x2DE6, 0x2DE7, 0x2DE8, 0x2DE9, 0x2DEA,
	0x2DEB, 0x2DEC, 0x2DED, 0x2DEE, 0x2DEF, 0x2DF0, 0x2DF1, 0x2DF2,
	0x2DF3, 0x2DF4, 0x2DF5, 0x2DF6, 0x2DF7, 0x2DF8, 0x2DF9, 0x2DFA,
	0x2DFB, 0x2DFC, 0x2DFD, 0x2DFE, 0x2DFF, 0xA66F, 0xA67C, 0xA67D,
	0xA6F0, 0xA6F1, 0xA8E0, 0xA8E1, 0xA8E2, 0xA8E3, 0xA8E4, 0xA8E5,
	0xA8E6, 0xA8E7, 0xA8E8, 0xA8E9, 0xA8EA, 0xA8EB, 0xA8EC, 0xA8ED,
	0xA8EE, 0xA8EF, 0xA8F0, 0xA8F1, 0xAAB0, 0xAAB2, 0xAAB3, 0xAAB7,
	0xAAB8, 0xAABE, 0xAABF, 0xAAC1, 0xFE20, 0xFE21, 0xFE22, 0xFE23,
	0xFE24, 0xFE25, 0xFE26, 0x10A0F, 0x10A38, 0x1D185, 0x1D186, 0x1D187,
	0x1D188, 0x1D189, 0x1D1AA, 0x1D1AB, 0x1D1AC, 0x1D1AD, 0x1D242, 0x1D243,
	0x1D244,
}
