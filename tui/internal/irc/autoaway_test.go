package irc

import "testing"

func TestParseAutoawayDuration(t *testing.T) {
	cases := []struct {
		token string
		want  int
		ok    bool
	}{
		{"", 0, false},
		{"abc", 0, false},
		{".5", 0, false},
		{"1.", 0, false},
		{"1.5.5", 0, false},
		{"1h30", 0, false},
		{"1 s", 0, false},
		{"0", 0, false},
		{"29", 1740, true},
		{"29s", 0, false},
		{"30", 1800, true},
		{"30s", 30, true},
		{"30S", 30, true},
		{"1m", 60, true},
		{"1.5m", 90, true},
		{"0.5m", 30, true},
		{"1h", 3600, true},
		{"1.5h", 5400, true},
		{"35791", 2147460, true},
		{"35792", 0, false},
		{"2147483", 0, false},
	}
	for _, test := range cases {
		got, ok := ParseAutoawayDuration(test.token)
		if ok != test.ok || (ok && got != test.want) {
			t.Errorf("ParseAutoawayDuration(%q) = (%d, %v), want (%d, %v)",
				test.token, got, ok, test.want, test.ok)
		}
	}
}

func TestParseAutoawayArgument(t *testing.T) {
	cases := []struct {
		argument string
		want     AutoawayRequest
	}{
		{"", AutoawayRequest{Kind: AutoawayQuery}},
		{"   ", AutoawayRequest{Kind: AutoawayQuery}},
		{"off", AutoawayRequest{Kind: AutoawayDisable}},
		{"OFF", AutoawayRequest{Kind: AutoawayDisable}},
		{"0", AutoawayRequest{Kind: AutoawayDisable}},
		{"off now", AutoawayRequest{Kind: AutoawayUsage}},
		{"on", AutoawayRequest{Kind: AutoawayEnableOn}},
		{"on now", AutoawayRequest{Kind: AutoawayUsage}},
		{"reason", AutoawayRequest{Kind: AutoawayClearDefaultReason}},
		{"reason busy", AutoawayRequest{Kind: AutoawaySetDefaultReason, Text: "busy"}},
		{"reason  busy now", AutoawayRequest{Kind: AutoawaySetDefaultReason, Text: "busy now"}},
		{"30s", AutoawayRequest{Kind: AutoawaySetTimeout, TimeoutSeconds: 30}},
		{"5", AutoawayRequest{Kind: AutoawaySetTimeout, TimeoutSeconds: 300}},
		{"1h", AutoawayRequest{Kind: AutoawaySetTimeout, TimeoutSeconds: 3600}},
		{"30s lunch", AutoawayRequest{Kind: AutoawaySetTimeout, TimeoutSeconds: 30, Text: "lunch"}},
		{"30s  meeting now", AutoawayRequest{Kind: AutoawaySetTimeout, TimeoutSeconds: 30, Text: "meeting now"}},
		{"abc", AutoawayRequest{Kind: AutoawayUsage}},
		{"29s", AutoawayRequest{Kind: AutoawayUsage}},
	}
	for _, test := range cases {
		if got := ParseAutoawayArgument(test.argument); got != test.want {
			t.Errorf("ParseAutoawayArgument(%q) = %+v, want %+v", test.argument, got, test.want)
		}
	}
}

func TestFormatAutoawayDuration(t *testing.T) {
	cases := []struct {
		seconds int
		want    string
	}{
		{1, "1 second"},
		{30, "30 seconds"},
		{60, "1 minute"},
		{1800, "30 minutes"},
		{3600, "1 hour"},
		{7200, "2 hours"},
		{5400, "90 minutes"},
	}
	for _, test := range cases {
		if got := FormatAutoawayDuration(test.seconds); got != test.want {
			t.Errorf("FormatAutoawayDuration(%d) = %q, want %q", test.seconds, got, test.want)
		}
	}
}

func TestFormatAutoawayQueryAndConfirmation(t *testing.T) {
	off := AutoawayConfig{TimeoutSeconds: 1800}
	if got := FormatAutoawayQuery(off); got != "Auto-away off, 30 minutes" {
		t.Errorf("disabled query = %q", got)
	}
	if got := FormatAutoawayConfirmation(off); got != "Auto-away off" {
		t.Errorf("disabled confirmation = %q", got)
	}

	on := AutoawayConfig{Enabled: true, TimeoutSeconds: 1800, DefaultReason: "lunch"}
	if got := FormatAutoawayQuery(on); got != "Auto-away 30 minutes, reason: lunch" {
		t.Errorf("enabled query = %q", got)
	}
	if got := FormatAutoawayConfirmation(on); got != "Auto-away 30 minutes, reason: lunch" {
		t.Errorf("enabled confirmation = %q", got)
	}

	oneShot := AutoawayConfig{
		Enabled:        true,
		TimeoutSeconds: 1800,
		DefaultReason:  "lunch",
		OneShotReason:  "brb",
	}
	if got := FormatAutoawayQuery(oneShot); got != "Auto-away in 30 minutes: brb (this time only), reason: lunch" {
		t.Errorf("one-shot query = %q", got)
	}
	if got := FormatAutoawayConfirmation(oneShot); got != "Auto-away in 30 minutes: brb (this time only)" {
		t.Errorf("one-shot confirmation = %q", got)
	}

	disabledOneShot := AutoawayConfig{OneShotReason: "brb", DefaultReason: "lunch"}
	if got := FormatAutoawayConfirmation(disabledOneShot); got != "Auto-away off: brb (this time only), reason: lunch" {
		t.Errorf("disabled one-shot confirmation = %q", got)
	}
}

func TestFormatAutoawayStatus(t *testing.T) {
	if got := FormatAutoawayTrippedStatus(""); got != "Auto-away triggered." {
		t.Errorf("empty tripped status = %q", got)
	}
	if got := FormatAutoawayTrippedStatus("  brb  "); got != "Auto-away triggered: brb" {
		t.Errorf("tripped status = %q", got)
	}
	if got := FormatAutoawayClearedStatus(); got != "Auto-away cleared — back online." {
		t.Errorf("cleared status = %q", got)
	}
}

func TestAutoawayGraceSeconds(t *testing.T) {
	cases := []struct {
		timeout int
		want    int
	}{
		{0, 0},
		{-30, 0},
		{30, 5},
		{100, 5},
		{150, 7},
		{200, 10},
		{600, 10},
	}
	for _, test := range cases {
		if got := AutoawayGraceSeconds(test.timeout); got != test.want {
			t.Errorf("AutoawayGraceSeconds(%d) = %d, want %d", test.timeout, got, test.want)
		}
	}
}
