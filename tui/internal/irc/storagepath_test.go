package irc

import (
	"strings"
	"testing"
)

func TestEncodesReservedCharacters(t *testing.T) {
	cases := []struct {
		text string
		want string
	}{
		{"#omarchy", "#omarchy"},
		{"alice", "alice"},
		{"libera", "libera"},
		{"net-1", "net-1"},
		{"#a|b", "#a%7cb"},
	}
	for _, test := range cases {
		if got := OmaircStorageSegment(test.text); got != test.want {
			t.Errorf("OmaircStorageSegment(%q) = %q, want %q", test.text, got, test.want)
		}
	}
}

func TestPercentEncodingIsInjective(t *testing.T) {
	slash := OmaircStorageSegment("a/b")
	literal := OmaircStorageSegment("a%2fb")
	if slash == literal {
		t.Fatalf("OmaircStorageSegment(%q) = OmaircStorageSegment(%q) = %q", "a/b", "a%2fb", slash)
	}
}

func TestEncodesTrailingDotAndSpace(t *testing.T) {
	cases := []struct {
		text string
		want string
	}{
		{"tail.", "tail%2e"},
		{"tail ", "tail%20"},
	}
	for _, test := range cases {
		if got := OmaircStorageSegment(test.text); got != test.want {
			t.Errorf("OmaircStorageSegment(%q) = %q, want %q", test.text, got, test.want)
		}
	}
}

func TestEncodesEmptyAndDotSegments(t *testing.T) {
	cases := []struct {
		text string
		want string
	}{
		{"", "_"},
		{".", "_"},
		{"..", "_"},
	}
	for _, test := range cases {
		if got := OmaircStorageSegment(test.text); got != test.want {
			t.Errorf("OmaircStorageSegment(%q) = %q, want %q", test.text, got, test.want)
		}
	}
}

func TestEncodesDeviceNames(t *testing.T) {
	cases := []struct {
		text      string
		extension string
		want      string
	}{
		{"con", "", "%63on"},
		{"con", ".json", "%63on"},
		{"aux", "", "%61ux"},
		{"nul", "", "%6eul"},
		{"com1", "", "%63om1"},
		{"lpt9", "", "%6cpt9"},
		{"prn", "", "%70rn"},
		{"con.", "", "%63on%2e"},
	}
	for _, test := range cases {
		if got := OmaircStorageSegment(test.text, test.extension); got != test.want {
			t.Errorf("OmaircStorageSegment(%q, %q) = %q, want %q",
				test.text, test.extension, got, test.want)
		}
	}
}

func TestEncodesDottedDeviceNames(t *testing.T) {
	cases := []struct {
		text string
		want string
	}{
		{"nul.tar.gz", "%6eul.tar.gz"},
		{"com1.tar.gz", "%63om1.tar.gz"},
		{"aux.foo.bar", "%61ux.foo.bar"},
		{"con.txt.", "%63on.txt%2e"},
		{"com\u00b9", "%63om%c2%b9"},
		{"lpt\u00b3", "%6cpt%c2%b3"},
	}
	for _, test := range cases {
		if got := OmaircStorageSegment(test.text); got != test.want {
			t.Errorf("OmaircStorageSegment(%q) = %q, want %q", test.text, got, test.want)
		}
	}
}

func TestCapsSegmentLengthWithExtension(t *testing.T) {
	extension := ".json"
	withinLimit := strings.Repeat("a", 250)
	segmentWithin := OmaircStorageSegment(withinLimit, extension)
	if !strings.HasPrefix(segmentWithin, "a") {
		t.Errorf("within-limit segment = %q, want an 'a' prefix", segmentWithin)
	}
	if len(segmentWithin)+len(extension) != 255 {
		t.Errorf("within-limit length = %d, want 255", len(segmentWithin)+len(extension))
	}

	overLimit := strings.Repeat("a", 251)
	segmentOver := OmaircStorageSegment(overLimit, extension)
	if !strings.HasPrefix(segmentOver, "h") {
		t.Errorf("over-limit segment = %q, want an 'h' prefix", segmentOver)
	}
	if len(segmentOver)+len(extension) > 255 {
		t.Errorf("over-limit length = %d, want <= 255", len(segmentOver)+len(extension))
	}

	mapping := NewCaseMapping(KindRfc1459)
	targetSegment := OmaircTargetSegment(overLimit, mapping, extension)
	if targetSegment != segmentOver {
		t.Errorf("OmaircTargetSegment(overLimit, rfc1459, %q) = %q, want %q",
			extension, targetSegment, segmentOver)
	}
}

func TestPreservesNetworkCaseAndLowercaseHex(t *testing.T) {
	if OmaircStorageSegment("Ab") == OmaircStorageSegment("ab") {
		t.Fatal("segment encoding must preserve case")
	}
	if got := OmaircStorageSegment("#a|b"); got != "#a%7cb" {
		t.Errorf("OmaircStorageSegment(%q) = %q, want %q", "#a|b", got, "#a%7cb")
	}
}

func TestAccentedCharactersEncodeDistinctly(t *testing.T) {
	lower := OmaircStorageSegment("\u00e9")
	upper := OmaircStorageSegment("\u00c9")
	if lower == upper {
		t.Fatalf("lower and upper accents must encode distinctly, both %q", lower)
	}
	if lower != "%c3%a9" {
		t.Errorf("OmaircStorageSegment(é) = %q, want %q", lower, "%c3%a9")
	}
	if upper != "%c3%89" {
		t.Errorf("OmaircStorageSegment(É) = %q, want %q", upper, "%c3%89")
	}
}

func TestTargetSegmentFollowsCaseMapping(t *testing.T) {
	rfc1459 := NewCaseMapping(KindRfc1459)
	ascii := NewCaseMapping(KindAscii)
	strict := NewCaseMapping(KindRfc1459Strict)

	if OmaircTargetSegment("#Chan", rfc1459) != OmaircTargetSegment("#chan", rfc1459) {
		t.Error("rfc1459 must fold #Chan and #chan to one segment")
	}
	if OmaircTargetSegment("#Chan", ascii) != OmaircTargetSegment("#chan", ascii) {
		t.Error("ascii must fold #Chan and #chan to one segment")
	}

	if OmaircTargetSegment("#foo[", rfc1459) != OmaircTargetSegment("#foo{", rfc1459) {
		t.Error("rfc1459 must fold #foo[ and #foo{ to one segment")
	}
	if OmaircTargetSegment("#foo[", ascii) == OmaircTargetSegment("#foo{", ascii) {
		t.Error("ascii must keep #foo[ and #foo{ distinct")
	}

	if OmaircTargetSegment("#foo^", rfc1459) != OmaircTargetSegment("#foo~", rfc1459) {
		t.Error("rfc1459 must fold #foo^ and #foo~ to one segment")
	}
	if OmaircTargetSegment("#foo^", strict) == OmaircTargetSegment("#foo~", strict) {
		t.Error("strict rfc1459 must keep #foo^ and #foo~ distinct")
	}
}

func TestDecodesOmaircStorageSegments(t *testing.T) {
	cases := []struct {
		segment string
		want    string
	}{
		{"#%4fmarchy", "#Omarchy"},
		{"#omarchy", "#omarchy"},
		{"#foo%7cbar", "#foo|bar"},
	}
	for _, test := range cases {
		if got := DecodeOmaircStorageSegment(test.segment); got != test.want {
			t.Errorf("DecodeOmaircStorageSegment(%q) = %q, want %q",
				test.segment, got, test.want)
		}
	}
}

func TestDecodesHSegmentsDistinctlyFromHashes(t *testing.T) {
	if got := OmaircStorageSegment("hEllo"); got != "h%45llo" {
		t.Errorf("OmaircStorageSegment(hEllo) = %q, want %q", got, "h%45llo")
	}
	if got := OmaircStorageSegment("hello"); got != "hello" {
		t.Errorf("OmaircStorageSegment(hello) = %q, want %q", got, "hello")
	}
	if got := DecodeOmaircStorageSegment("h%45llo"); got != "hEllo" {
		t.Errorf("DecodeOmaircStorageSegment(h%%45llo) = %q, want %q", got, "hEllo")
	}
	if got := DecodeOmaircStorageSegment("hello"); got != "hello" {
		t.Errorf("DecodeOmaircStorageSegment(hello) = %q, want %q", got, "hello")
	}

	if got := OmaircStorageSegment("homeUser"); got != "home%55ser" {
		t.Errorf("OmaircStorageSegment(homeUser) = %q, want %q", got, "home%55ser")
	}
	if got := OmaircStorageSegment("homeuser"); got != "homeuser" {
		t.Errorf("OmaircStorageSegment(homeuser) = %q, want %q", got, "homeuser")
	}
	if got := DecodeOmaircStorageSegment("home%55ser"); got != "homeUser" {
		t.Errorf("DecodeOmaircStorageSegment(home%%55ser) = %q, want %q", got, "homeUser")
	}
	if got := DecodeOmaircStorageSegment("homeuser"); got != "homeuser" {
		t.Errorf("DecodeOmaircStorageSegment(homeuser) = %q, want %q", got, "homeuser")
	}

	overLimit := strings.Repeat("a", 251)
	hashed := OmaircStorageSegment(overLimit, ".json")
	if !OmaircStorageSegmentIsHash(hashed) {
		t.Errorf("OmaircStorageSegment(overLimit) = %q, want a hash", hashed)
	}
	if got := DecodeOmaircStorageSegment(hashed); got != hashed {
		t.Errorf("DecodeOmaircStorageSegment(%q) = %q, want itself", hashed, got)
	}
}

func TestReservesHashShapedEncodedSegments(t *testing.T) {
	hashLike := "h" + strings.Repeat("a", 64)
	encoded := OmaircStorageSegment(hashLike)
	want := "%68" + strings.Repeat("a", 64)
	if encoded != want {
		t.Errorf("OmaircStorageSegment(hashLike) = %q, want %q", encoded, want)
	}
	if OmaircStorageSegmentIsHash(encoded) {
		t.Errorf("encoded hash-like segment %q must not look like a hash", encoded)
	}
	if got := DecodeOmaircStorageSegment(encoded); got != hashLike {
		t.Errorf("DecodeOmaircStorageSegment(%q) = %q, want %q", encoded, got, hashLike)
	}

	overLimit := strings.Repeat("a", 251)
	hashed := OmaircStorageSegment(overLimit, ".json")
	if !OmaircStorageSegmentIsHash(hashed) {
		t.Errorf("OmaircStorageSegment(overLimit) = %q, want a hash", hashed)
	}
}

func TestNulTargetsDoNotSharePaths(t *testing.T) {
	mapping := NewCaseMapping(KindRfc1459)
	left := "a\x00b"
	right := "a\x00\x00b"
	if OmaircTargetSegment(left, mapping) == OmaircTargetSegment(right, mapping) {
		t.Fatalf("NUL-collapsed targets must not share a segment: %q and %q",
			left, right)
	}
}

func TestLegacyStorageSegment(t *testing.T) {
	cases := []struct {
		text string
		want string
	}{
		{"#omarchy", "#omarchy"},
		{"a/b", "a%2fb"},
		{"a\\b", "a%5cb"},
		{"%", "%25"},
		{"a%2fb", "a%252fb"},
		{"", "_"},
		{".", "_"},
		{"..", "_"},
	}
	for _, test := range cases {
		if got := LegacyStorageSegment(test.text); got != test.want {
			t.Errorf("LegacyStorageSegment(%q) = %q, want %q", test.text, got, test.want)
		}
	}
}

func TestDecodeLegacySegment(t *testing.T) {
	cases := []struct {
		text string
		want string
	}{
		{"a%2fb", "a/b"},
		{"a%5cb", "a\\b"},
		{"%25", "%"},
		{"a%252fb", "a%2fb"},
		{"a%00b", "a\x00b"},
		{"#omarchy", "#omarchy"},
	}
	for _, test := range cases {
		if got := DecodeLegacySegment(test.text); got != test.want {
			t.Errorf("DecodeLegacySegment(%q) = %q, want %q", test.text, got, test.want)
		}
	}
}
