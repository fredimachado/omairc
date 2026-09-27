package storage

import (
	"bytes"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"unicode/utf16"
	"unicode/utf8"
)

// IniProbe is the result of a fail-closed raw-bytes probe of the ini file. It
// mirrors IrcProfileStore::IniProbe.
type IniProbe int

const (
	// IniProbeOK means the file is missing or is a readable, well-formed ini.
	IniProbeOK IniProbe = iota
	// IniProbeMalformed means the raw bytes are not a valid ini. Writing over
	// them would silently drop data, so Sync refuses.
	IniProbeMalformed
	// IniProbeUnreadable means the file exists but cannot be read. Writing
	// would rebuild it from a partial cache, so Sync refuses.
	IniProbeUnreadable
)

// Status mirrors IrcProfileStore::Status.
type Status int

const (
	// StatusWritten means the change was persisted.
	StatusWritten Status = iota
	// StatusAbsent means the addressed item does not exist.
	StatusAbsent
	// StatusAccessError means the file could not be read or written.
	StatusAccessError
	// StatusFormatError means the file is not a well-formed ini.
	StatusFormatError
)

// iniValue is one persisted setting. QSettings stores a QString for a plain
// value, a QStringList for a list, and an invalid QVariant for @Invalid(). A
// single-element list is written without a comma, exactly as QSettings does,
// so it is indistinguishable from a string once it round-trips through the
// file; that matches QSettings.
type iniValue struct {
	isList  bool
	invalid bool
	text    string
	items   []string
}

// Settings is an INI-backed key/value store mirroring the QSettings semantics
// the Qt client uses: nested groups written as [group] sections with
// backslash-escaped key paths, string keys, bool/int/uint keys, string-list
// keys, and JSON string values.
//
// Group and key names are matched case-sensitively. The Qt client only uses
// canonical names, and keeping the map case-sensitive makes writes
// deterministic. A group path is the slash-joined Sprintf of its segments
// ("networks/abc"); Value("networks/abc", "host") reads the same line QSettings
// writes for beginGroup("networks"); beginGroup("abc"); value("host").
type Settings struct {
	path   string
	values map[string]iniValue
	dirty  bool
}

// OpenSettings loads path, or ConfigPath() when path is "". A missing or
// unreadable file yields an empty store; Sync refuses to overwrite a malformed
// or unreadable file through WriteBlocked.
func OpenSettings(path string) *Settings {
	if path == "" {
		path = ConfigPath()
	}
	s := &Settings{path: path, values: make(map[string]iniValue)}
	s.load()
	return s
}

// Path returns the backing file path.
func (s *Settings) Path() string {
	return s.path
}

// Value returns the string stored at group/key, or "" when absent. A stored
// string list or an invalid value also returns "", matching
// QVariant::toString() on those variants.
func (s *Settings) Value(group, key string) string {
	v, ok := s.lookup(group, key)
	if !ok || v.isList || v.invalid {
		return ""
	}
	return v.text
}

// Bool parses the stored value as a QSettings bool: "true"/"1" and
// "false"/"0" case-insensitively. It returns fallback when the key is absent
// or the stored string is none of those forms.
func (s *Settings) Bool(group, key string, fallback bool) bool {
	v, ok := s.lookup(group, key)
	if !ok || v.isList || v.invalid {
		return fallback
	}
	switch strings.ToLower(v.text) {
	case "true", "1":
		return true
	case "false", "0":
		return false
	}
	return fallback
}

// Uint parses the stored value as a base-10 unsigned integer, returning
// fallback when the key is absent or does not parse. It mirrors the
// toUInt(&ok) path in IrcProfileStore::profiles.
func (s *Settings) Uint(group, key string, fallback uint64) uint64 {
	v, ok := s.lookup(group, key)
	if !ok || v.isList || v.invalid {
		return fallback
	}
	n, err := strconv.ParseUint(strings.TrimSpace(v.text), 10, 64)
	if err != nil {
		return fallback
	}
	return n
}

// Int parses the stored value as a base-10 signed integer, returning fallback
// when the key is absent or does not parse. It mirrors toInt(&ok).
func (s *Settings) Int(group, key string, fallback int) int {
	v, ok := s.lookup(group, key)
	if !ok || v.isList || v.invalid {
		return fallback
	}
	n, err := strconv.Atoi(strings.TrimSpace(v.text))
	if err != nil {
		return fallback
	}
	return n
}

// StringList returns the stored string list, or nil when the key is absent. A
// stored empty value and a stored @Invalid() both yield an empty, non-nil
// list; a plain non-empty string yields a one-element list, matching
// QVariant::toStringList(). (Qt returns one empty item for an empty string;
// this store follows the phase-11 contract that an empty value is an empty
// list.)
func (s *Settings) StringList(group, key string) []string {
	v, ok := s.lookup(group, key)
	if !ok {
		return nil
	}
	if v.isList {
		out := make([]string, len(v.items))
		copy(out, v.items)
		return out
	}
	if v.invalid || v.text == "" {
		return []string{}
	}
	return []string{v.text}
}

// SetValue stores a plain string. A value that looks like a JSON document is
// not special-cased: it is escaped and quoted as an ordinary string.
func (s *Settings) SetValue(group, key, value string) {
	s.values[settingsKey(group, key)] = iniValue{text: value}
	s.dirty = true
}

// SetBool stores a QSettings bool ("true"/"false").
func (s *Settings) SetBool(group, key string, value bool) {
	text := "false"
	if value {
		text = "true"
	}
	s.SetValue(group, key, text)
}

// SetUint stores a base-10 unsigned integer.
func (s *Settings) SetUint(group, key string, value uint64) {
	s.SetValue(group, key, strconv.FormatUint(value, 10))
}

// SetInt stores a base-10 signed integer.
func (s *Settings) SetInt(group, key string, value int) {
	s.SetValue(group, key, strconv.Itoa(value))
}

// SetStringList stores a string list. An empty list is written as @Invalid(),
// the same sentinel QSettings uses so an empty list survives a round trip
// distinctly from a one-item list containing an empty string.
func (s *Settings) SetStringList(group, key string, values []string) {
	items := make([]string, len(values))
	copy(items, values)
	s.values[settingsKey(group, key)] = iniValue{isList: true, items: items}
	s.dirty = true
}

// RemoveKey removes one key from a group (no-op when absent).
func (s *Settings) RemoveKey(group, key string) {
	delete(s.values, settingsKey(group, key))
	s.dirty = true
}

// RemoveGroup removes a whole group and its children, mirroring
// QSettings::remove(group): the key itself and every key beneath it go.
func (s *Settings) RemoveGroup(group string) {
	if group == "" {
		for path := range s.values {
			delete(s.values, path)
		}
		s.dirty = true
		return
	}
	prefix := group + "/"
	for path := range s.values {
		if path == group || strings.HasPrefix(path, prefix) {
			delete(s.values, path)
		}
	}
	s.dirty = true
}

// ChildGroups returns the direct child group names of group, sorted, mirroring
// QSettings::childGroups(): only the next path segment is returned, so deeper
// groups collapse to their first segment and duplicates collapse to one entry.
func (s *Settings) ChildGroups(group string) []string {
	prefix := ""
	if group != "" {
		prefix = group + "/"
	}
	seen := make(map[string]struct{})
	for path := range s.values {
		if !strings.HasPrefix(path, prefix) {
			continue
		}
		rest := path[len(prefix):]
		if slash := strings.IndexByte(rest, '/'); slash >= 0 {
			seen[rest[:slash]] = struct{}{}
		}
	}
	groups := make([]string, 0, len(seen))
	for name := range seen {
		groups = append(groups, name)
	}
	sort.Strings(groups)
	return groups
}

// Sync writes pending changes to disk atomically and returns the resulting
// status. It refuses before writing when WriteBlocked reports the file must
// not be overwritten, so a malformed or unreadable ini is never rebuilt from
// cache.
func (s *Settings) Sync() Status {
	if blocked := s.WriteBlocked(); blocked != StatusWritten {
		return blocked
	}
	if s.path == "" {
		return StatusAccessError
	}
	if !s.dirty {
		return StatusWritten
	}
	if err := writeSettingsAtomic(s.path, s.encode()); err != nil {
		return StatusAccessError
	}
	s.dirty = false
	return StatusWritten
}

// WriteBlocked mirrors blockedIniWrite + existingSettingsFileIsNotWritable: it
// returns StatusFormatError/StatusAccessError when the file must not be
// written, or StatusWritten when writing is allowed. A missing file is
// writable; a file that exists but cannot accept a write is not.
func (s *Settings) WriteBlocked() Status {
	switch ProbeSettingsIni(s.path) {
	case IniProbeMalformed:
		return StatusFormatError
	case IniProbeUnreadable:
		return StatusAccessError
	}
	if settingsFileNotWritable(s.path) {
		return StatusAccessError
	}
	return StatusWritten
}

// ProbeSettingsIni mirrors IrcProfileStore::probeSettingsIni(path): OK when the
// file is missing or a readable, well-formed ini; Malformed when the raw bytes
// are not a valid ini; Unreadable when the file exists but cannot be read.
func ProbeSettingsIni(path string) IniProbe {
	if path == "" {
		return IniProbeOK
	}
	info, err := os.Stat(path)
	if err != nil {
		if os.IsNotExist(err) {
			return IniProbeOK
		}
		return IniProbeUnreadable
	}
	if info.IsDir() {
		return IniProbeUnreadable
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		return IniProbeUnreadable
	}
	if iniBytesAreMalformed(raw) {
		return IniProbeMalformed
	}
	return IniProbeOK
}

// load parses the backing file. A read error or malformed bytes leave the map
// empty; WriteBlocked still refuses to overwrite the file.
func (s *Settings) load() {
	if s.path == "" {
		return
	}
	raw, err := os.ReadFile(s.path)
	if err != nil {
		return
	}
	s.values = parseSettingsIni(raw)
}

func (s *Settings) lookup(group, key string) (iniValue, bool) {
	v, ok := s.values[settingsKey(group, key)]
	return v, ok
}

// settingsKey joins a group path and a leaf key. An empty group is the top
// level (the [General] section on disk).
func settingsKey(group, key string) string {
	if group == "" {
		return key
	}
	return group + "/" + key
}

// settingsFileNotWritable reports whether an existing file cannot accept a
// write. A missing file is writable (Sync creates it). It combines a
// permission-bit check with an O_WRONLY open, which never truncates, so it
// works without a Unix-only syscall and matches QFileInfo::isWritable for the
// common case.
func settingsFileNotWritable(path string) bool {
	if path == "" {
		return false
	}
	info, err := os.Stat(path)
	if err != nil {
		return false
	}
	if info.Mode().Perm()&0o222 == 0 {
		return true
	}
	file, err := os.OpenFile(path, os.O_WRONLY, 0)
	if err != nil {
		return true
	}
	file.Close()
	return false
}

// writeSettingsAtomic writes data to path through a same-directory temporary
// file and a rename, creating parents with owner-only permissions and setting
// 0600 on the file, so an interrupted Sync cannot leave a half-written ini.
func writeSettingsAtomic(path string, data []byte) error {
	dir := filepath.Dir(path)
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return err
	}
	tmp, err := os.CreateTemp(dir, ".omairc-settings-*.tmp")
	if err != nil {
		return err
	}
	tmpName := tmp.Name()
	defer func() {
		if tmpName != "" {
			os.Remove(tmpName)
		}
	}()
	if err := tmp.Chmod(0o600); err != nil {
		tmp.Close()
		return err
	}
	if _, err := tmp.Write(data); err != nil {
		tmp.Close()
		return err
	}
	if err := tmp.Sync(); err != nil {
		tmp.Close()
		return err
	}
	if err := tmp.Close(); err != nil {
		return err
	}
	if err := os.Rename(tmpName, path); err != nil {
		return err
	}
	tmpName = ""
	return nil
}

// sectionEntry is one key line within a written section.
type sectionEntry struct {
	key   string
	value iniValue
}

// encode renders the whole store as QSettings INI bytes. Sections come out
// sorted, then the keys inside each section, so repeated Syncs are byte-stable
// regardless of insertion order. Each full key is split at its first slash:
// the first segment becomes the [section] and the rest is backslash-escaped
// into the key, which is exactly how QSettings::writeIniFile lays out
// beginGroup paths.
func (s *Settings) encode() []byte {
	sections := make(map[string][]sectionEntry)
	for path, value := range s.values {
		section, key := splitSettingsSection(path)
		sections[section] = append(sections[section], sectionEntry{key: key, value: value})
	}

	names := make([]string, 0, len(sections))
	for name := range sections {
		names = append(names, name)
	}
	sort.Strings(names)

	var out bytes.Buffer
	for index, name := range names {
		if index != 0 {
			out.WriteByte('\n')
		}
		out.WriteString(iniSectionHeader(name))
		out.WriteByte('\n')

		entries := sections[name]
		sort.Slice(entries, func(i, j int) bool { return entries[i].key < entries[j].key })
		for _, entry := range entries {
			out.WriteString(iniEscapedKey(entry.key))
			out.WriteByte('=')
			out.WriteString(encodeIniValue(entry.value))
			out.WriteByte('\n')
		}
	}
	return out.Bytes()
}

// splitSettingsSection splits a full key into its [section] and the escaped
// remainder, matching QSettings::writeIniFile.
func splitSettingsSection(path string) (section, key string) {
	if slash := strings.IndexByte(path, '/'); slash >= 0 {
		return path[:slash], path[slash+1:]
	}
	return "", path
}

// iniSectionHeader renders a section name the way QSettings does: empty means
// [General], and a real group named "general" is written [%General] so it is
// not confused with the top level.
func iniSectionHeader(section string) string {
	escaped := iniEscapedKey(section)
	switch {
	case escaped == "":
		return "[General]"
	case strings.EqualFold(escaped, "general"):
		return "[%General]"
	default:
		return "[" + escaped + "]"
	}
}

// encodeIniValue serializes one setting. An empty list is @Invalid(); a
// one-item list is written as a bare string, matching QSettings.
func encodeIniValue(v iniValue) string {
	switch {
	case v.isList:
		return iniEscapedStringList(v.items)
	case v.invalid:
		return "@Invalid()"
	default:
		return iniEscapedString(v.text)
	}
}

// parseSettingsIni reads QSettings INI bytes into a flat map of full key paths.
// A section "[networks]" followed by "abc\\host=x" yields
// "networks/abc/host". A hand-written "[networks/abc]" section yields the same
// path, which is why the section name is decoded with iniUnescapedKey too.
func parseSettingsIni(data []byte) map[string]iniValue {
	values := make(map[string]iniValue)
	raw := data
	if bytes.HasPrefix(raw, []byte{0xEF, 0xBB, 0xBF}) {
		raw = raw[3:]
	}

	section := ""
	dataPos, lineStart, lineLen, equalsPos := 0, 0, 0, -1
	for nextIniLine(raw, &dataPos, &lineStart, &lineLen, &equalsPos) {
		line := raw[lineStart : lineStart+lineLen]
		if line[0] == '[' {
			section = iniSectionPrefix(line)
			continue
		}
		if equalsPos < 0 {
			// A comment or a value line with no '='. ProbeSettingsIni gates the
			// malformed case; skip it here rather than inventing a key.
			continue
		}
		equals := equalsPos - lineStart
		key := iniUnescapedKey(bytes.TrimSpace(line[:equals]))
		value := iniUnescapedStringList(line[equals+1:])
		values[section+key] = value
	}
	return values
}

// iniSectionPrefix decodes a "[...]" line into the group prefix QSettings
// prepends to its keys: "" for [General] (the top level), a decoded segment
// plus "/" otherwise, and "%General" (any case) for the literal-name escape.
func iniSectionPrefix(line []byte) string {
	inner := line[1:]
	if close := bytes.IndexByte(inner, ']'); close >= 0 {
		inner = inner[:close]
	}
	inner = bytes.TrimSpace(inner)

	switch {
	case bytes.EqualFold(inner, []byte("general")):
		return ""
	case bytes.EqualFold(inner, []byte("%general")):
		return string(inner[1:]) + "/"
	default:
		return iniUnescapedKey(inner) + "/"
	}
}

// iniBytesAreMalformed ports IrcProfileStore::iniBytesAreMalformed exactly: a
// '[section' line with no closing ']' on the same line, or a non-empty,
// non-comment line with no '=', is malformed. It is the fail-closed gate that
// keeps Sync from rewriting a corrupt file.
func iniBytesAreMalformed(data []byte) bool {
	raw := data
	if bytes.HasPrefix(raw, []byte{0xEF, 0xBB, 0xBF}) {
		raw = raw[3:]
	}

	dataPos, lineStart, lineLen, equalsPos := 0, 0, 0, -1
	for nextIniLine(raw, &dataPos, &lineStart, &lineLen, &equalsPos) {
		if raw[lineStart] == '[' {
			close := bytes.IndexByte(raw[lineStart:], ']')
			if close < 0 || close >= lineLen {
				return true
			}
			continue
		}
		if equalsPos < 0 && raw[lineStart] != ';' {
			return true
		}
	}
	return false
}

// nextIniLine ports Qt 6.8 QConfFileSettingsPrivate::readIniLine. A leading
// ';' is a comment; '=' inside quotes does not count as the separator; and a
// backslash consumes the next byte, including a newline, so an escaped
// newline does not end the line. Positions are byte offsets into data.
func nextIniLine(data []byte, dataPos, lineStart, lineLen, equalsPos *int) bool {
	dataLen := len(data)
	inQuotes := false
	*equalsPos = -1

	*lineStart = *dataPos
	for *lineStart < dataLen && iniSpace(data[*lineStart]) {
		*lineStart++
	}

	i := *lineStart
	ended := false
	for i < dataLen && !ended {
		ch := data[i]
		if !iniSpecial(ch) {
			i++
			continue
		}
		i++
		switch {
		case ch == '=':
			if !inQuotes && *equalsPos == -1 {
				*equalsPos = i - 1
			}
		case ch == '\n' || ch == '\r':
			if i == *lineStart+1 {
				*lineStart++
			} else if !inQuotes {
				i--
				ended = true
			}
		case ch == '\\':
			if i < dataLen {
				escaped := data[i]
				i++
				if i < dataLen {
					next := data[i]
					if (escaped == '\n' && next == '\r') || (escaped == '\r' && next == '\n') {
						i++
					}
				}
			}
		case ch == '"':
			inQuotes = !inQuotes
		case i == *lineStart+1:
			// ';' at the start of a line: consume the whole comment.
			for i < dataLen {
				comment := data[i]
				if comment == '\n' || comment == '\r' {
					break
				}
				i++
			}
			for i < dataLen && iniSpace(data[i]) {
				i++
			}
			*lineStart = i
		case !inQuotes:
			// A ';' (or other special) mid-line ends the line.
			i--
			ended = true
		}
	}

	*dataPos = i
	*lineLen = i - *lineStart
	return *lineLen > 0
}

func iniSpace(ch byte) bool {
	return ch == ' ' || ch == '\t' || ch == '\n' || ch == '\r'
}

func iniSpecial(ch byte) bool {
	return ch == '\n' || ch == '\r' || ch == '"' || ch == ';' || ch == '=' || ch == '\\'
}

// iniUnescapedKey ports QSettingsPrivate::iniUnescapedKey. A backslash decodes
// to '/'; '%XX' and '%UXXXX' decode hex code units; anything else is literal.
// Both section names and key paths use it, so a hand-written [a/b] and
// QSettings' [a] + b\c decode to the same group path.
func iniUnescapedKey(key []byte) string {
	units := utf16.Encode([]rune(string(key)))
	out := make([]uint16, 0, len(units))
	for i := 0; i < len(units); {
		ch := units[i]
		if ch == '\\' {
			out = append(out, '/')
			i++
			continue
		}
		if ch != '%' || i == len(units)-1 {
			out = append(out, ch)
			i++
			continue
		}

		numDigits := 2
		first := i + 1
		if units[i+1] == 'U' {
			first++
			numDigits = 4
		}
		if first+numDigits > len(units) {
			out = append(out, '%')
			i++
			continue
		}
		value := 0
		ok := true
		for j := 0; j < numDigits; j++ {
			digit := iniHexValue(rune(units[first+j]))
			if digit < 0 {
				ok = false
				break
			}
			value = value*16 + digit
		}
		if !ok {
			out = append(out, '%')
			i++
			continue
		}
		out = append(out, uint16(value))
		i = first + numDigits
	}
	return string(utf16.Decode(out))
}

// iniEscapedKey ports QSettingsPrivate::iniEscapedKey. Alphanumerics, '_', '-'
// and '.' stay literal; '/' becomes a single backslash; other code units
// become '%XX' or '%UXXXX'. It walks UTF-16 code units so a non-BMP rune is
// written as its two surrogates, exactly as Qt does.
func iniEscapedKey(key string) string {
	units := utf16.Encode([]rune(key))
	var out strings.Builder
	for _, ch := range units {
		switch {
		case ch == '/':
			out.WriteByte('\\')
		case (ch >= '0' && ch <= '9') || (ch >= 'a' && ch <= 'z') ||
			(ch >= 'A' && ch <= 'Z') || ch == '_' || ch == '-' || ch == '.':
			out.WriteByte(byte(ch))
		case ch <= 0xFF:
			out.WriteByte('%')
			out.WriteByte(iniHexUpper(ch / 16))
			out.WriteByte(iniHexUpper(ch % 16))
		default:
			out.WriteString("%U")
			out.WriteByte(iniHexUpper((ch >> 12) & 0xF))
			out.WriteByte(iniHexUpper((ch >> 8) & 0xF))
			out.WriteByte(iniHexUpper((ch >> 4) & 0xF))
			out.WriteByte(iniHexUpper(ch & 0xF))
		}
	}
	return out.String()
}

// iniEscapedString ports QSettingsPrivate::iniEscapedString for the UTF-8 ini
// format. Rules implemented:
//
//   - ';', ',' and '=' force the whole value to be wrapped in double quotes;
//     they are not backslash-escaped.
//   - '\' and '"' are backslash-escaped.
//   - '\0', '\a', '\b', '\f', '\n', '\r', '\t', '\v' use their named escapes.
//   - other control bytes become lowercase "\x<hex>". After a "\0" or "\x.."
//     escape, a following hex digit is itself written "\x<hex>" so the value
//     cannot be re-read as a longer octal/hex escape.
//   - a leading or trailing space forces quoting; tabs do not.
//   - all other characters are emitted as UTF-8, so values are never mangled.
func iniEscapedString(str string) string {
	var out strings.Builder
	needsQuotes := false
	escapeNextIfDigit := false

	for _, ch := range str {
		if ch == ';' || ch == ',' || ch == '=' {
			needsQuotes = true
		}
		if escapeNextIfDigit && iniIsHexDigit(ch) {
			out.WriteString(`\x`)
			out.WriteString(strconv.FormatInt(int64(ch), 16))
			continue
		}
		escapeNextIfDigit = false

		switch ch {
		case 0:
			out.WriteString(`\0`)
			escapeNextIfDigit = true
		case '\a':
			out.WriteString(`\a`)
		case '\b':
			out.WriteString(`\b`)
		case '\f':
			out.WriteString(`\f`)
		case '\n':
			out.WriteString(`\n`)
		case '\r':
			out.WriteString(`\r`)
		case '\t':
			out.WriteString(`\t`)
		case '\v':
			out.WriteString(`\v`)
		case '"', '\\':
			out.WriteByte('\\')
			out.WriteRune(ch)
		default:
			if ch <= 0x1F {
				out.WriteString(`\x`)
				out.WriteString(strconv.FormatInt(int64(ch), 16))
				escapeNextIfDigit = true
			} else {
				out.WriteRune(ch)
			}
		}
	}

	result := out.String()
	if needsQuotes || strings.HasPrefix(result, " ") || strings.HasSuffix(result, " ") {
		return `"` + result + `"`
	}
	return result
}

// iniEscapedStringList ports QSettingsPrivate::iniEscapedStringList: an empty
// list is @Invalid(), otherwise items are escaped and joined with ", ".
func iniEscapedStringList(items []string) string {
	if len(items) == 0 {
		return "@Invalid()"
	}
	var out strings.Builder
	for i, item := range items {
		if i != 0 {
			out.WriteString(", ")
		}
		out.WriteString(iniEscapedString(item))
	}
	return out.String()
}

// iniUnescapedStringList ports QSettingsPrivate::iniUnescapedStringList. It
// returns the parsed list (isList true when a top-level comma separated it),
// the plain string otherwise, and applies QVariant's @Invalid()/@@ sentinels.
// The escape table, \x/octal escapes, quoted commas, and trailing-space
// trimming all match Qt, so a QSettings file round-trips byte-for-byte in
// meaning.
func iniUnescapedStringList(value []byte) iniValue {
	str := string(value)
	length := len(str)
	var buf []byte
	i := 0

	var items []string
	isList := false
	inQuotes := false
	currentQuoted := false
	chopLimit := 0
	var escapeVal rune

	emit := func(r rune) { buf = utf8.AppendRune(buf, r) }
	chop := func() {
		for len(buf) > chopLimit {
			last := buf[len(buf)-1]
			if last != ' ' && last != '\t' {
				break
			}
			buf = buf[:len(buf)-1]
		}
	}
	finishItem := func() {
		if !currentQuoted {
			chop()
		}
		if !isList {
			isList = true
			items = items[:0]
		}
		items = append(items, string(buf))
		buf = buf[:0]
		currentQuoted = false
	}

stSkipSpaces:
	for i < length && (str[i] == ' ' || str[i] == '\t') {
		i++
	}
stNormal:
	chopLimit = len(buf)
	for i < length {
		switch str[i] {
		case '\\':
			i++
			if i >= length {
				goto end
			}
			escaped := str[i]
			i++
			if decoded, ok := iniEscapeCode(escaped); ok {
				emit(decoded)
				goto stNormal
			}
			if escaped == 'x' {
				escapeVal = 0
				if i >= length {
					goto end
				}
				if iniIsHexDigit(rune(str[i])) {
					goto stHexEscape
				}
			} else if octal := iniOctValue(escaped); octal >= 0 {
				escapeVal = rune(octal)
				goto stOctEscape
			} else if escaped == '\n' || escaped == '\r' {
				if i < length {
					next := str[i]
					if (next == '\n' || next == '\r') && next != escaped {
						i++
					}
				}
			}
			// Any other escape is skipped, as in Qt.
			chopLimit = len(buf)
		case '"':
			i++
			currentQuoted = true
			inQuotes = !inQuotes
			if !inQuotes {
				goto stSkipSpaces
			}
		case ',':
			if !inQuotes {
				finishItem()
				i++
				goto stSkipSpaces
			}
			fallthrough
		default:
			j := i + 1
			for j < length {
				c := str[j]
				if c == '\\' || c == '"' || c == ',' {
					break
				}
				j++
			}
			buf = append(buf, str[i:j]...)
			i = j
		}
	}
	if !currentQuoted {
		chop()
	}
	goto end

stHexEscape:
	for i < length {
		digit := iniHexValue(rune(str[i]))
		if digit < 0 {
			emit(escapeVal)
			goto stNormal
		}
		escapeVal = escapeVal<<4 + rune(digit)
		i++
	}
	emit(escapeVal)
	goto end

stOctEscape:
	for i < length {
		digit := iniOctValue(str[i])
		if digit < 0 {
			emit(escapeVal)
			goto stNormal
		}
		escapeVal = escapeVal<<3 + rune(digit)
		i++
	}
	emit(escapeVal)

end:
	if isList {
		return iniValue{isList: true, items: append(items, string(buf))}
	}
	switch {
	case string(buf) == "@Invalid()":
		return iniValue{invalid: true}
	case strings.HasPrefix(string(buf), "@@"):
		return iniValue{text: string(buf)[1:]}
	default:
		return iniValue{text: string(buf)}
	}
}

// iniEscapeCode is the escape table from QSettingsPrivate::iniUnescapedStringList.
func iniEscapeCode(ch byte) (rune, bool) {
	switch ch {
	case 'a':
		return '\a', true
	case 'b':
		return '\b', true
	case 'f':
		return '\f', true
	case 'n':
		return '\n', true
	case 'r':
		return '\r', true
	case 't':
		return '\t', true
	case 'v':
		return '\v', true
	case '"':
		return '"', true
	case '?':
		return '?', true
	case '\'':
		return '\'', true
	case '\\':
		return '\\', true
	}
	return 0, false
}

func iniIsHexDigit(ch rune) bool {
	return iniHexValue(ch) >= 0
}

func iniHexValue(ch rune) int {
	switch {
	case ch >= '0' && ch <= '9':
		return int(ch - '0')
	case ch >= 'a' && ch <= 'f':
		return int(ch-'a') + 10
	case ch >= 'A' && ch <= 'F':
		return int(ch-'A') + 10
	}
	return -1
}

func iniOctValue(ch byte) int {
	if ch >= '0' && ch <= '7' {
		return int(ch - '0')
	}
	return -1
}

func iniHexUpper(value uint16) byte {
	return "0123456789ABCDEF"[value&0xF]
}
