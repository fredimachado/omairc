package irc

import "testing"

func TestNormalizesAdvertisedMappings(t *testing.T) {
	rfc1459 := NewCaseMapping(KindRfc1459)
	if !rfc1459.Equals("Nick[]\\", "nick{}|") {
		t.Fatal("rfc1459 must fold []\\ to {}|")
	}
	if !rfc1459.Equals("nick^", "NICK~") {
		t.Fatal("rfc1459 must fold ^ to ~")
	}

	strict := NewCaseMapping(KindRfc1459Strict)
	if !strict.Equals("Nick[]\\", "nick{}|") {
		t.Fatal("strict rfc1459 must fold []\\ to {}|")
	}
	if strict.Equals("nick^", "nick~") {
		t.Fatal("strict rfc1459 must keep ^ distinct from ~")
	}

	ascii := NewCaseMapping(KindAscii)
	if !ascii.Equals("Nick", "nick") {
		t.Fatal("ascii mapping must fold case")
	}
	if ascii.Equals("[]\\", "{}|") {
		t.Fatal("ascii mapping must keep []\\ distinct from {}|")
	}

	if _, ok := FromName("rfc1459-strict"); !ok {
		t.Fatal("rfc1459-strict must be recognized")
	}
	if _, ok := FromName("strict-rfc1459"); !ok {
		t.Fatal("strict-rfc1459 must be recognized")
	}
	if _, ok := FromName("unknown"); ok {
		t.Fatal("unknown mapping must be rejected")
	}
}

func TestParsesServerFeatures(t *testing.T) {
	features := NewServerFeatures()
	features.ApplyTokens([]string{
		"CASEMAPPING=ascii",
		"CHANTYPES=&",
		"PREFIX=(qaohv)~&@%+",
		"NICKLEN=31",
	})

	if features.CaseMapping().Kind() != KindAscii {
		t.Fatalf("case mapping = %v, want ascii", features.CaseMapping().Kind())
	}
	if !features.IsChannel("&local") {
		t.Fatal("&local must be a channel")
	}
	if features.IsChannel("#channel") {
		t.Fatal("#channel must not be a channel under CHANTYPES=&")
	}
	if length, ok := features.NickLength(); !ok || length != 31 {
		t.Fatalf("nick length = %d, %v, want 31, true", length, ok)
	}
	features.ApplyToken("MONITOR=100")
	if !features.MonitorAdvertised() {
		t.Fatal("MONITOR must be advertised")
	}
	if limit, ok := features.MonitorLimit(); !ok || limit != 100 {
		t.Fatalf("monitor limit = %d, %v, want 100, true", limit, ok)
	}
	if features.PrefixModes() != "qaohv" {
		t.Fatalf("prefix modes = %q, want qaohv", features.PrefixModes())
	}
	if features.PrefixSymbols() != "~&@%+" {
		t.Fatalf("prefix symbols = %q, want %q", features.PrefixSymbols(), "~&@%+")
	}

	op, ok := features.ParseNamesToken("@alice")
	if !ok {
		t.Fatal("@alice must parse")
	}
	if op.Nick != "alice" {
		t.Fatalf("nick = %q, want alice", op.Nick)
	}
	if got := features.MemberLabel(op.Ranks, "alice"); got != "@alice" {
		t.Fatalf("label = %q, want @alice", got)
	}

	owner, ok := features.ParseNamesToken("~alice")
	if !ok {
		t.Fatal("~alice must parse")
	}
	if got := features.MemberLabel(owner.Ranks, "alice"); got != "~alice" {
		t.Fatalf("label = %q, want ~alice", got)
	}
}

func TestCaseMappingKnownAfterTokenOrRegistration(t *testing.T) {
	features := NewServerFeatures()
	if features.CaseMappingKnown() {
		t.Fatal("fresh features must not know the case mapping")
	}

	features.ApplyToken("CASEMAPPING=ascii")
	if !features.CaseMappingKnown() {
		t.Fatal("CASEMAPPING must mark the mapping known")
	}

	fresh := NewServerFeatures()
	fresh.MarkCaseMappingKnown()
	if !fresh.CaseMappingKnown() {
		t.Fatal("MarkCaseMappingKnown must mark the mapping known")
	}
}

func TestInvalidCaseMappingTokenDoesNotMarkKnown(t *testing.T) {
	features := NewServerFeatures()
	features.ApplyToken("CASEMAPPING=unknown")
	if features.CaseMappingKnown() {
		t.Fatal("invalid CASEMAPPING must not mark the mapping known")
	}

	kept := NewServerFeatures()
	kept.ApplyToken("CASEMAPPING=rfc1459")
	if !kept.CaseMappingKnown() {
		t.Fatal("valid CASEMAPPING must mark the mapping known")
	}
	kept.ApplyToken("CASEMAPPING=unknown")
	if !kept.CaseMappingKnown() {
		t.Fatal("a later invalid CASEMAPPING must not clear the mapping known flag")
	}
	if kept.CaseMapping().Kind() != KindRfc1459 {
		t.Fatalf("case mapping = %v, want rfc1459", kept.CaseMapping().Kind())
	}
}

func TestDefaultPrefixStripsOwnerBefore005(t *testing.T) {
	features := NewServerFeatures()
	if features.PrefixModes() != "qaohv" {
		t.Fatalf("prefix modes = %q, want qaohv", features.PrefixModes())
	}
	if features.PrefixSymbols() != "~&@%+" {
		t.Fatalf("prefix symbols = %q, want %q", features.PrefixSymbols(), "~&@%+")
	}
	parsed, ok := features.ParseNamesToken("~alice")
	if !ok {
		t.Fatal("~alice must parse")
	}
	if parsed.Nick != "alice" {
		t.Fatalf("nick = %q, want alice", parsed.Nick)
	}
	if got := features.MemberLabel(parsed.Ranks, "alice"); got != "~alice" {
		t.Fatalf("label = %q, want ~alice", got)
	}

	kept := features
	kept.ApplyToken("PREFIX=bad")
	kept.ApplyToken("-PREFIX")
	kept.ApplyToken("PREFIX")
	if kept.PrefixModes() != "qaohv" {
		t.Fatalf("prefix modes = %q, want qaohv after malformed PREFIX tokens", kept.PrefixModes())
	}
}

func TestStackedNamesConvergeAndPaintHighest(t *testing.T) {
	features := NewServerFeatures()
	plusAt, ok := features.ParseNamesToken("+@alice")
	if !ok {
		t.Fatal("+@alice must parse")
	}
	atPlus, ok := features.ParseNamesToken("@+alice")
	if !ok {
		t.Fatal("@+alice must parse")
	}
	if plusAt.Nick != atPlus.Nick {
		t.Fatalf("nicks differ: %q vs %q", plusAt.Nick, atPlus.Nick)
	}
	if plusAt.Ranks != atPlus.Ranks {
		t.Fatalf("ranks differ: %v vs %v", plusAt.Ranks, atPlus.Ranks)
	}
	if got := features.MemberLabel(plusAt.Ranks, "alice"); got != "@alice" {
		t.Fatalf("label = %q, want @alice", got)
	}

	features.ApplyToken("PREFIX=(ov)@+")
	if got := features.MemberLabel(plusAt.Ranks, "alice"); got != "@alice" {
		t.Fatalf("label = %q, want @alice after PREFIX remap", got)
	}
}

func TestEmptyLeftoverNickIsNotAParse(t *testing.T) {
	features := NewServerFeatures()
	for _, token := range []string{"@", "@+", "+", ""} {
		if _, ok := features.ParseNamesToken(token); ok {
			t.Fatalf("ParseNamesToken(%q) unexpectedly succeeded", token)
		}
	}
}

func TestPrefixChangesWalkLettersAndApplyIdempotently(t *testing.T) {
	features := NewServerFeatures()
	changes := features.PrefixChanges("+ov-o", []string{"alice", "bob", "alice"})
	if len(changes) != 3 {
		t.Fatalf("changes = %+v, want 3", changes)
	}
	if changes[0].Nick != "alice" || changes[1].Nick != "bob" || changes[2].Nick != "alice" {
		t.Fatalf("changes = %+v, want alice bob alice", changes)
	}

	named, ok := features.ParseNamesToken("alice")
	if !ok {
		t.Fatal("alice must parse")
	}
	ranks := named.Ranks
	ranks = features.Apply(ranks, changes[0])
	ranks = features.Apply(ranks, changes[0])
	if got := features.MemberLabel(ranks, "alice"); got != "@alice" {
		t.Fatalf("label = %q, want @alice", got)
	}
	ranks = features.Apply(ranks, features.PrefixChanges("+v", []string{"alice"})[0])
	if got := features.MemberLabel(ranks, "alice"); got != "@alice" {
		t.Fatalf("label = %q, want @alice", got)
	}
	ranks = features.Apply(ranks, features.PrefixChanges("-o", []string{"alice"})[0])
	if got := features.MemberLabel(ranks, "alice"); got != "+alice" {
		t.Fatalf("label = %q, want +alice", got)
	}
	ranks = features.Apply(ranks, features.PrefixChanges("-v", []string{"alice"})[0])
	if got := features.MemberLabel(ranks, "alice"); got != "alice" {
		t.Fatalf("label = %q, want alice", got)
	}
}

func TestLaterPrefixRemapsGlyphsWithoutRewritingSets(t *testing.T) {
	features := NewServerFeatures()
	stacked, ok := features.ParseNamesToken("~@alice")
	if !ok {
		t.Fatal("~@alice must parse")
	}
	if got := features.MemberLabel(stacked.Ranks, "alice"); got != "~alice" {
		t.Fatalf("label = %q, want ~alice", got)
	}

	features.ApplyToken("PREFIX=(ov)@+")
	if got := features.MemberLabel(stacked.Ranks, "alice"); got != "@alice" {
		t.Fatalf("label = %q, want @alice", got)
	}

	features.ApplyToken("PREFIX=(ov)!+")
	if got := features.MemberLabel(stacked.Ranks, "alice"); got != "!alice" {
		t.Fatalf("label = %q, want !alice", got)
	}

	features.ApplyToken("PREFIX=broken")
	if features.PrefixModes() != "ov" {
		t.Fatalf("prefix modes = %q, want ov", features.PrefixModes())
	}
	if got := features.MemberLabel(stacked.Ranks, "alice"); got != "!alice" {
		t.Fatalf("label = %q, want !alice", got)
	}
}

func TestParsesChanModesFromIsupport(t *testing.T) {
	features := NewServerFeatures()
	if features.ChanModesA() != "b" || features.ChanModesB() != "k" ||
		features.ChanModesC() != "l" || features.ChanModesD() != "imnpst" {
		t.Fatalf("defaults = %q,%q,%q,%q, want b,k,l,imnpst",
			features.ChanModesA(), features.ChanModesB(),
			features.ChanModesC(), features.ChanModesD())
	}

	features.ApplyToken("CHANMODES=eIbq,k,l,imnpst")
	if features.ChanModesA() != "eIbq" || features.ChanModesB() != "k" ||
		features.ChanModesC() != "l" || features.ChanModesD() != "imnpst" {
		t.Fatalf("advertised = %q,%q,%q,%q, want eIbq,k,l,imnpst",
			features.ChanModesA(), features.ChanModesB(),
			features.ChanModesC(), features.ChanModesD())
	}

	features.ApplyToken("CHANMODES=bad")
	features.ApplyToken("-CHANMODES")
	features.ApplyToken("CHANMODES")
	if features.ChanModesA() != "eIbq" {
		t.Fatalf("chan modes A = %q, want eIbq after malformed tokens", features.ChanModesA())
	}
}

func TestPrefixChangesConsumeNonPrefixParameters(t *testing.T) {
	features := NewServerFeatures()
	mixed := features.PrefixChanges("+k+o", []string{"secret", "alice"})
	if len(mixed) != 1 {
		t.Fatalf("changes = %+v, want 1", mixed)
	}
	if mixed[0].Nick != "alice" {
		t.Fatalf("nick = %q, want alice", mixed[0].Nick)
	}
	named, ok := features.ParseNamesToken("alice")
	if !ok {
		t.Fatal("alice must parse")
	}
	ranks := features.Apply(named.Ranks, mixed[0])
	if got := features.MemberLabel(ranks, "alice"); got != "@alice" {
		t.Fatalf("label = %q, want @alice", got)
	}

	plusO := features.PrefixChanges("+o", []string{"alice"})
	if len(plusO) != 1 || plusO[0].Nick != "alice" {
		t.Fatalf("changes = %+v, want one alice op", plusO)
	}

	if changes := features.PrefixChanges("+b", []string{"mask"}); len(changes) != 0 {
		t.Fatalf("changes = %+v, want none", changes)
	}
	banThenOp := features.PrefixChanges("+b+o", []string{"mask", "alice"})
	if len(banThenOp) != 1 || banThenOp[0].Nick != "alice" {
		t.Fatalf("changes = %+v, want one alice op", banThenOp)
	}

	features.ApplyToken("CHANMODES=eIbq,k,l,imnpst")
	advertised := features.PrefixChanges("+k+o", []string{"secret", "alice"})
	if len(advertised) != 1 || advertised[0].Nick != "alice" {
		t.Fatalf("changes = %+v, want one alice op", advertised)
	}
	if changes := features.PrefixChanges("+b", []string{"mask"}); len(changes) != 0 {
		t.Fatalf("changes = %+v, want none", changes)
	}
	limitThenOp := features.PrefixChanges("+l+o", []string{"10", "alice"})
	if len(limitThenOp) != 1 || limitThenOp[0].Nick != "alice" {
		t.Fatalf("changes = %+v, want one alice op", limitThenOp)
	}
}

func TestParsesDraftIconFromIsupport(t *testing.T) {
	features := NewServerFeatures()
	if features.IconURL() != "" {
		t.Fatalf("icon url = %q, want empty", features.IconURL())
	}

	features.ApplyToken("draft/ICON=https://example.org/icon.svg")
	if features.IconURL() != "https://example.org/icon.svg" {
		t.Fatalf("icon url = %q", features.IconURL())
	}

	features.ApplyToken("draft/ICON=https://example.net/icon.png?size={size}")
	if features.IconURL() != "https://example.net/icon.png?size={size}" {
		t.Fatalf("icon url = %q", features.IconURL())
	}

	ignored := NewServerFeatures()
	ignored.ApplyToken("ICON=https://example.org/x.png")
	if ignored.IconURL() != "" {
		t.Fatalf("icon url = %q, want empty for a bare ICON token", ignored.IconURL())
	}

	empty := NewServerFeatures()
	empty.ApplyToken("draft/ICON")
	empty.ApplyToken("draft/ICON=")
	if empty.IconURL() != "" {
		t.Fatalf("icon url = %q, want empty", empty.IconURL())
	}

	kept := NewServerFeatures()
	kept.ApplyToken("draft/ICON=https://example.org/icon.svg")
	kept.ApplyToken("CHANTYPES=#")
	kept.ApplyToken("NICKLEN=16")
	if kept.IconURL() != "https://example.org/icon.svg" {
		t.Fatalf("icon url = %q", kept.IconURL())
	}
}

func TestDraftIconRemovalClearsAdvertisedURL(t *testing.T) {
	features := NewServerFeatures()
	features.ApplyToken("draft/ICON=https://example.org/icon.svg")
	if features.IconURL() != "https://example.org/icon.svg" {
		t.Fatalf("icon url = %q", features.IconURL())
	}

	features.ApplyToken("-draft/ICON")
	if features.IconURL() != "" {
		t.Fatalf("icon url = %q, want empty after -draft/ICON", features.IconURL())
	}

	features.ApplyToken("draft/ICON=https://example.org/icon.svg")
	features.ApplyToken("-ICON")
	if features.IconURL() != "https://example.org/icon.svg" {
		t.Fatalf("icon url = %q, want the advertised url kept", features.IconURL())
	}

	features.ApplyToken("-PREFIX")
	if features.PrefixModes() != "qaohv" {
		t.Fatalf("prefix modes = %q, want qaohv", features.PrefixModes())
	}
	if features.IconURL() != "https://example.org/icon.svg" {
		t.Fatalf("icon url = %q, want the advertised url kept", features.IconURL())
	}
}
