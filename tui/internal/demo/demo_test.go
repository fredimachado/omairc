package demo

import (
	"sort"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
)

// attachController seeds a fresh controller and returns it with the demo
// server. Tests are in-package so they can drive the unexported tryAnswer*
// handlers directly,.
func attachController(t *testing.T, autoEcho bool) (*controller.Controller, *DemoServer) {
	t.Helper()
	c := controller.New()
	d := New()
	if !d.Attach(c, autoEcho) {
		t.Fatalf("Attach failed: %q", d.LastError())
	}
	return c, d
}

func wireLines(payload []byte) []string {
	text := strings.TrimSuffix(string(payload), "\r\n")
	if text == "" {
		return nil
	}
	return strings.Split(text, "\r\n")
}

func TestAnswersClientPing(t *testing.T) {
	_, d := attachController(t, true)
	got := d.tryAnswerPing([]byte("PING :omairc-watchdog\r\n"))
	want := ":server PONG irc.example :omairc-watchdog\r\n"
	if string(got) != want {
		t.Fatalf("PONG = %q, want %q", got, want)
	}
}

func TestAnswersMonitorAdd(t *testing.T) {
	_, d := attachController(t, true)
	got := d.tryAnswerMonitor("omarchy", "fred",
		[]byte("MONITOR + anna,ghost\r\n"), demoOnlineNicks(omarchyWorld(), "fred"))
	want := ":server 730 fred :anna!u@h\r\n" + ":server 731 fred :ghost\r\n"
	if string(got) != want {
		t.Fatalf("MONITOR + = %q, want %q", got, want)
	}
}

func TestAnswersList(t *testing.T) {
	_, d := attachController(t, true)

	reply := d.tryAnswerList(omarchyWorld(), "fred", []byte("LIST\r\n"))
	if reply == nil {
		t.Fatal("LIST was not answered")
	}
	lines := wireLines(reply)
	if len(lines) == 0 {
		t.Fatal("LIST reply was empty")
	}
	if lines[0] != ":server 321 fred Channel :Users  Name" {
		t.Fatalf("321 header = %q", lines[0])
	}
	if lines[len(lines)-1] != ":server 323 fred :End of /LIST" {
		t.Fatalf("323 footer = %q", lines[len(lines)-1])
	}

	var rows []SeedListed
	for _, line := range lines {
		if !strings.HasPrefix(line, ":server 322 ") {
			continue
		}
		message, err := irc.Parse(line)
		if err != nil || len(message.Params) != 4 {
			t.Fatalf("bad 322 line %q: %v %+v", line, err, message.Params)
		}
		users, err := strconv.Atoi(message.Params[2])
		if err != nil {
			t.Fatalf("bad 322 users %q: %v", message.Params[2], err)
		}
		rows = append(rows, SeedListed{Name: message.Params[1], Users: users, Topic: message.Params[3]})
	}
	if len(rows) != 6 {
		t.Fatalf("322 row count = %d, want 6", len(rows))
	}

	// The C++ test observes ChannelListModel's users-descending sort.
	sort.SliceStable(rows, func(i, j int) bool { return rows[i].Users > rows[j].Users })
	if rows[0].Name != "#linux" || rows[0].Users != 42 {
		t.Fatalf("row 0 = %+v, want #linux/42", rows[0])
	}
	// The directory topic carries literal STX and mIRC color bytes.
	wantLinuxTopic := "\x02Kernel\x02 discussion and \x03" + "04distro" + "\x03 help."
	if rows[0].Topic != wantLinuxTopic {
		t.Fatalf("row 0 topic = %q, want %q", rows[0].Topic, wantLinuxTopic)
	}
	if rows[1].Name != "#omarchy" || rows[1].Users != 12 {
		t.Fatalf("row 1 = %+v, want #omarchy/12", rows[1])
	}

	filtered := d.tryAnswerList(omarchyWorld(), "fred", []byte("LIST #l*\r\n"))
	var filteredRows []string
	for _, line := range wireLines(filtered) {
		if strings.HasPrefix(line, ":server 322 ") {
			filteredRows = append(filteredRows, line)
		}
	}
	if len(filteredRows) != 1 || !strings.Contains(filteredRows[0], " #linux ") {
		t.Fatalf("LIST #l* = %v, want one #linux row", filteredRows)
	}

	if got := d.tryAnswerList(omarchyWorld(), "fred", []byte("MODE #omarchy\r\n")); got != nil {
		t.Fatalf("non-LIST frame answered %q", got)
	}
}

func TestSeedsServiceAccounts(t *testing.T) {
	c, _ := attachController(t, false)
	want := map[string]string{
		"lena": "pinkieval",
		"fred": "fredm",
		"sol":  "solarius",
		"teo":  "teoval",
		"kai":  "kaidev",
	}
	for nick, account := range want {
		if got := c.Reducer().NickPresence("omarchy", nick).Account; got != account {
			t.Fatalf("account(%s) = %q, want %q", nick, got, account)
		}
	}
}

func TestSeedsQueryHeaderFacts(t *testing.T) {
	c, _ := attachController(t, false)
	reducer := c.Reducer()

	if got := reducer.MeaningfulRealname("omarchy", "anna"); got != "Anna Vale" {
		t.Fatalf("anna realname = %q, want Anna Vale", got)
	}
	if labels := reducer.PeerFactLabels("omarchy", "anna"); len(labels) != 0 {
		t.Fatalf("anna labels = %v, want none", labels)
	}
	if got := reducer.PeerPresence("omarchy", "anna"); got != irc.PeerOnline {
		t.Fatalf("anna presence = %v, want online", got)
	}

	if got := reducer.MeaningfulRealname("omarchy", "ivy"); got != "" {
		t.Fatalf("ivy realname = %q, want empty", got)
	}
	ivy := reducer.PeerFactLabels("omarchy", "ivy")
	if len(ivy) != 1 || ivy[0] != "unauthenticated" {
		t.Fatalf("ivy labels = %v, want unauthenticated", ivy)
	}
	if got := reducer.PeerPresence("omarchy", "ivy"); got != irc.PeerAway {
		t.Fatalf("ivy presence = %v, want away", got)
	}

	fred := reducer.PeerFactLabels("omarchy", "fred")
	if len(fred) != 2 || fred[0] != "fredm" || fred[1] != "server operator" {
		t.Fatalf("fred labels = %v, want fredm and server operator", fred)
	}
	if got := reducer.MeaningfulRealname("omarchy", "fred"); got != "Fred Machado" {
		t.Fatalf("fred realname = %q", got)
	}
	if !reducer.NickPresence("omarchy", "fred").ServerOperator {
		t.Fatal("fred must be a server operator on omarchy")
	}

	dax := reducer.PeerFactLabels("omarchy", "dax")
	if len(dax) != 1 || dax[0] != "bot" {
		t.Fatalf("dax labels = %v, want bot", dax)
	}
	if got := reducer.MeaningfulRealname("omarchy", "dax"); got != "Packet Bot" {
		t.Fatalf("dax realname = %q", got)
	}

	lena := reducer.PeerFactLabels("omarchy", "lena")
	if len(lena) != 1 || lena[0] != "pinkieval" {
		t.Fatalf("lena labels = %v, want pinkieval", lena)
	}
	if got := reducer.PeerPresence("omarchy", "lena"); got != irc.PeerAway {
		t.Fatalf("lena presence = %v, want away", got)
	}

	oak := reducer.NickPresence("oftc", "oak")
	if oak.ServerOperator || oak.Realname != "" {
		t.Fatalf("oak facts = %+v, want no operator and no real name", oak)
	}
}

func TestSkipsRedundantAccountTag(t *testing.T) {
	c, d := attachController(t, false)
	epoch := c.PeerAccountEpoch()
	d.omarchy.InjectBytes([]byte("@account=kaidev :kai!u@h PRIVMSG #omarchy :still here\r\n"))
	if got := c.Reducer().NickPresence("omarchy", "kai").Account; got != "kaidev" {
		t.Fatalf("account(kai) = %q, want kaidev", got)
	}
	if got := c.PeerAccountEpoch(); got != epoch {
		t.Fatalf("PeerAccountEpoch moved from %d to %d", epoch, got)
	}
}

func TestSeedsParityWorld(t *testing.T) {
	c := controller.New()
	// A fake clock keeps the typing hints from expiring and stops every
	// session timer. Do not Advance it.
	clock := session.NewFakeClock(time.Date(2026, 9, 12, 12, 0, 0, 0, time.UTC))
	c.SetClock(clock)
	d := New()
	if !d.Attach(c, false) {
		t.Fatalf("Attach failed: %q", d.LastError())
	}

	// Manager.NetworkIDs sorts; the C++ insertion order is not preserved.
	networkIDs := c.NetworkIDs()
	sort.Strings(networkIDs)
	if strings.Join(networkIDs, ",") != "oftc,omarchy" {
		t.Fatalf("NetworkIDs = %v, want oftc,omarchy", c.NetworkIDs())
	}

	type row struct {
		network   string
		target    string
		unread    int
		mention   bool
		typing    bool
		isChannel bool
	}
	var wantRows = []row{
		{"omarchy", "#desktop", 3, false, false, true},
		{"omarchy", "#help", 0, false, false, true},
		{"omarchy", "#omarchy", 0, false, false, true},
		{"omarchy", "#ricing", 12, true, false, true},
		{"omarchy", "anna", 1, true, true, false},
		{"omarchy", "dax", 0, false, false, false},
		{"oftc", "#build", 2, false, false, true},
		{"oftc", "#lab", 0, false, false, true},
		{"oftc", "#omarchy", 0, false, false, true},
		{"oftc", "rio", 0, false, false, false},
	}
	gotRows := c.Conversations()
	if len(gotRows) != len(wantRows) {
		t.Fatalf("sidebar row count = %d, want %d: %+v", len(gotRows), len(wantRows), gotRows)
	}
	for index, want := range wantRows {
		got := gotRows[index]
		if got.NetworkID != want.network || got.Conversation != want.target ||
			got.Unread != want.unread || got.Mention != want.mention ||
			got.Typing != want.typing || got.Direct != !want.isChannel {
			t.Fatalf("row[%d] = %+v, want %+v", index, got, want)
		}
	}

	key := c.Reducer().ConversationKey("omarchy", "#omarchy")
	var nicks []string
	for _, member := range c.Reducer().OrderedMembers(key) {
		nicks = append(nicks, member.Nick)
	}
	wantNicks := []string{"fred", "anna", "dax", "mira", "kai", "teo", "ivy", "lena", "max", "nora", "sam", "sol"}
	if strings.Join(nicks, " ") != strings.Join(wantNicks, " ") {
		t.Fatalf("ordered members = %v, want %v", nicks, wantNicks)
	}

	wantLabels := map[string]string{
		"fred": "~fred",
		"anna": "&anna",
		"dax":  "@dax",
		"mira": "@mira",
		"kai":  "%kai",
		"teo":  "+teo",
	}
	for nick, label := range wantLabels {
		view, ok := c.Reducer().MemberView(key, nick)
		if !ok {
			t.Fatalf("MemberView(%s) missing", nick)
		}
		if view.Label != label {
			t.Fatalf("label(%s) = %q, want %q", nick, view.Label, label)
		}
	}

	for _, nick := range []string{"teo", "lena", "sam", "ivy", "max"} {
		if c.Reducer().NickPresence("omarchy", nick).Away == nil {
			t.Fatalf("presence(%s).Away = nil, want away", nick)
		}
	}
	for _, nick := range []string{"anna", "dax"} {
		if c.Reducer().NickPresence("omarchy", nick).Away != nil {
			t.Fatalf("presence(%s).Away set, want nil", nick)
		}
	}

	wantStatus := map[string]string{
		"anna": "writing docs",
		"dax":  "on #desktop",
		"mira": "making tea",
		"sol":  "new here",
		"fred": "building Omairc",
	}
	for nick, status := range wantStatus {
		view, ok := c.Reducer().MemberView(key, nick)
		if !ok {
			t.Fatalf("MemberView(%s) missing", nick)
		}
		if view.Status != status {
			t.Fatalf("status(%s) = %q, want %q", nick, view.Status, status)
		}
	}

	c.SelectConversation("omarchy", "#omarchy")
	if !containsString(c.TypingNicks(), "anna") {
		t.Fatalf("typing in #omarchy = %v, want anna", c.TypingNicks())
	}
	c.SelectConversation("omarchy", "anna")
	if !containsString(c.TypingNicks(), "anna") {
		t.Fatalf("typing in anna = %v, want anna", c.TypingNicks())
	}

	// The operator's own avatar is a real HTTPS URL so the demo exercises the
	// avatar store's network path; a peer keeps the bundled resource.
	if got := c.Reducer().NickPresence("omarchy", "fred").Avatar(); got != kDemoSelfAvatarURL {
		t.Fatalf("fred avatar = %q, want %q", got, kDemoSelfAvatarURL)
	}
	if got := c.Reducer().NickPresence("oftc", "oak").Avatar(); got != kDemoSelfAvatarURL {
		t.Fatalf("oak avatar = %q, want %q", got, kDemoSelfAvatarURL)
	}
	if got := c.Reducer().NickPresence("omarchy", "mira").Avatar(); got != "qrc:/demo/mira-avatar.png" {
		t.Fatalf("mira avatar = %q, want the bundled resource", got)
	}

	// The recording walkthrough depends on two seeded edges: the one https link
	// in OFTC #lab (Ctrl+Shift+O) and the OFTC INVITE. The invite matters because
	// the walkthrough visits #ricing first (Alt+A), which consumes the seeded
	// #ricing mention row; the invite is what still gives Ctrl+Shift+A a row.
	c.SelectConversation("oftc", "#lab")
	foundLink := false
	for _, message := range c.Messages() {
		if strings.Contains(message.Body, "https://ci.example.com/omairc/42") {
			foundLink = true
		}
	}
	if !foundLink {
		t.Fatal("oftc #lab must carry the seeded demo link")
	}
	invites := 0
	for _, item := range c.InboxItems() {
		if item.Kind == "invite" && item.NetworkID == "oftc" && item.Target == "#debian" {
			invites++
		}
	}
	if invites != 1 {
		t.Fatalf("seeded OFTC invite rows = %d, want 1: %+v", invites, c.InboxItems())
	}
}

func TestRejoinEchoesSelfJoin(t *testing.T) {
	c, _ := attachController(t, true)
	c.SelectConversation("omarchy", "#omarchy")
	if !c.ChannelJoined() {
		t.Fatal("seeded #omarchy should start joined")
	}
	if !c.SendMessage("/part") {
		t.Fatalf("part: %s", c.LastError())
	}
	if c.ChannelJoined() {
		t.Fatal("part should leave the channel")
	}
	if !c.SendMessage("/join #omarchy") {
		t.Fatalf("join: %s", c.LastError())
	}
	if !c.ChannelJoined() {
		t.Fatal("demo JOIN echo should rejoin without a hand-injected line")
	}
}

func TestClosedChannelsRejoinFromDemoEcho(t *testing.T) {
	c, _ := attachController(t, true)
	c.SelectConversation("omarchy", "#desktop")
	if !c.SendMessage("/part") || !c.SendMessage("/close") {
		t.Fatalf("close #desktop: %s", c.LastError())
	}
	c.SelectConversation("omarchy", "#omarchy")
	if !c.SendMessage("/part") || !c.SendMessage("/close") {
		t.Fatalf("close #omarchy: %s", c.LastError())
	}
	c.SelectConversation("omarchy", "#help")
	if !c.SendMessage("/join #desktop,#omarchy") {
		t.Fatalf("join: %s", c.LastError())
	}
	if c.SelectedTarget() != "#omarchy" || !c.ChannelJoined() {
		t.Fatalf("selected %q joined=%v", c.SelectedTarget(), c.ChannelJoined())
	}
	if !containsString(conversationNames(c), "#desktop") || !containsString(conversationNames(c), "#omarchy") {
		t.Fatalf("rows = %v", conversationNames(c))
	}
	// The echo restores the seeded topic and does not send a fresh 353, so the
	// member column stays the nick who just joined.
	if c.Topic() != "A cozy corner for Omarchy users and builders." {
		t.Fatalf("topic = %q", c.Topic())
	}
	if c.PeopleCount() != 1 {
		t.Fatalf("people = %d, want 1", c.PeopleCount())
	}
	c.SelectConversation("omarchy", "#desktop")
	if c.SelectedTarget() != "#desktop" || !c.ChannelJoined() {
		t.Fatal("#desktop was not joined by the demo echo")
	}
	if c.Topic() != "Desktops should feel personal, fast, and calm." {
		t.Fatalf("#desktop topic = %q", c.Topic())
	}
	if c.PeopleCount() != 1 {
		t.Fatalf("#desktop people = %d, want 1", c.PeopleCount())
	}
}

// TestAnswersPartForAnyChannel echoes our PART under our own prefix for any
// channel name, as JOIN does. The client's frame is kept as sent, so a reason
// stays as written. A target that is not a channel gets no echo, and each
// network answers under its own nick.
func TestAnswersPartForAnyChannel(t *testing.T) {
	_, d := attachController(t, true)

	if got := string(d.tryAnswerPart("omarchy", []byte("PART #desktop\r\n"))); got != ":fred!u@h PART #desktop\r\n" {
		t.Fatalf("PART = %q", got)
	}
	if got := string(d.tryAnswerPart("omarchy", []byte("PART #desktop :see you\r\n"))); got != ":fred!u@h PART #desktop :see you\r\n" {
		t.Fatalf("PART with reason = %q", got)
	}
	if got := string(d.tryAnswerPart("omarchy", []byte("PART #Desktop\r\n"))); got != ":fred!u@h PART #Desktop\r\n" {
		t.Fatalf("PART with other case = %q", got)
	}
	if got := string(d.tryAnswerPart("omarchy", []byte("PART #linux\r\n"))); got != ":fred!u@h PART #linux\r\n" {
		t.Fatalf("PART on a directory channel = %q", got)
	}
	if got := d.tryAnswerPart("omarchy", []byte("PART fred\r\n")); got != nil {
		t.Fatalf("PART on a non-channel target answered %q", got)
	}
	if got := string(d.tryAnswerPart("oftc", []byte("PART #desktop\r\n"))); got != ":oak!u@h PART #desktop\r\n" {
		t.Fatalf("oftc PART = %q", got)
	}
	if got := string(d.tryAnswerPart("oftc", []byte("PART #omarchy\r\n"))); got != ":oak!u@h PART #omarchy\r\n" {
		t.Fatalf("oftc PART = %q", got)
	}
	if got := d.tryAnswerPart("omarchy", []byte("MODE #desktop\r\n")); got != nil {
		t.Fatalf("non-PART frame answered %q", got)
	}
}

// TestPartEchoLeavesChannel writes our PART through the transport, so no
// controller-side /part runs and only the demo echo can mark the channel left.
func TestPartEchoLeavesChannel(t *testing.T) {
	c, d := attachController(t, true)
	c.SelectConversation("omarchy", "#desktop")
	if !c.ChannelJoined() {
		t.Fatal("seeded #desktop should start joined")
	}
	d.omarchy.Write([]byte("PART #desktop\r\n"))
	if c.ChannelJoined() {
		t.Fatal("the demo PART echo should leave #desktop")
	}
}

// TestAnswersNickChangeMovesLaterReplies checks that the NICK echo carries the
// old nick as its prefix, and that the next echo and the next NICK both use the
// nick the demo now holds. The other network keeps its own nick.
func TestAnswersNickChangeMovesLaterReplies(t *testing.T) {
	_, d := attachController(t, true)

	if got := string(d.tryAnswerNick("omarchy", []byte("NICK bob\r\n"))); got != ":fred!u@h NICK :bob\r\n" {
		t.Fatalf("NICK = %q", got)
	}
	if got := d.selfNicks["omarchy"]; got != "bob" {
		t.Fatalf("self nick = %q, want bob", got)
	}
	if got := string(d.tryAnswerPart("omarchy", []byte("PART #desktop\r\n"))); got != ":bob!u@h PART #desktop\r\n" {
		t.Fatalf("PART after NICK = %q", got)
	}
	// MONITOR rebuilds its online set from the new nick: bob is online, and the
	// seeded fred no longer is.
	wantMonitor := ":server 730 bob :bob!u@h\r\n:server 731 bob :fred\r\n"
	if got := string(d.tryAnswerMonitor("omarchy", "bob", []byte("MONITOR + fred,bob\r\n"), demoOnlineNicks(omarchyWorld(), "bob"))); got != wantMonitor {
		t.Fatalf("MONITOR after NICK = %q, want %q", got, wantMonitor)
	}
	if got := string(d.tryAnswerNick("omarchy", []byte("NICK alice\r\n"))); got != ":bob!u@h NICK :alice\r\n" {
		t.Fatalf("second NICK = %q", got)
	}
	if got := d.tryAnswerNick("omarchy", []byte("NICK\r\n")); got != nil {
		t.Fatalf("NICK without a new nick answered %q", got)
	}
	if got := d.tryAnswerNick("omarchy", []byte("NICK two words\r\n")); got != nil {
		t.Fatalf("NICK with two words answered %q", got)
	}
	if got := d.selfNicks["oftc"]; got != "oak" {
		t.Fatalf("oftc self nick = %q, want oak", got)
	}
}

// TestAnswersTopicChangeCarriesOnRejoin sets a seeded topic, then checks the
// TOPIC echo and the 332 that a later self JOIN sends with the new value. A
// channel that has no topic yet rejoins without a 332.
func TestAnswersTopicChangeCarriesOnRejoin(t *testing.T) {
	_, d := attachController(t, true)

	if got := string(d.tryAnswerTopic("omarchy", []byte("TOPIC #desktop :new banner\r\n"))); got != ":fred!u@h TOPIC #desktop :new banner\r\n" {
		t.Fatalf("TOPIC = %q", got)
	}
	want := ":fred!u@h JOIN :#desktop\r\n:server 332 fred #desktop :new banner\r\n"
	if got := string(d.tryAnswerJoin("omarchy", "fred", []byte("JOIN #desktop\r\n"))); got != want {
		t.Fatalf("JOIN #desktop = %q, want %q", got, want)
	}
	if got := string(d.tryAnswerJoin("omarchy", "fred", []byte("JOIN #nowhere\r\n"))); got != ":fred!u@h JOIN :#nowhere\r\n" {
		t.Fatalf("JOIN #nowhere = %q", got)
	}
	// A topic set on a channel the network does not seed is kept, and the rejoin
	// reports it, as a real server does for any channel.
	if got := string(d.tryAnswerTopic("omarchy", []byte("TOPIC #nowhere :x\r\n"))); got != ":fred!u@h TOPIC #nowhere :x\r\n" {
		t.Fatalf("TOPIC on a directory channel = %q", got)
	}
	want = ":fred!u@h JOIN :#nowhere\r\n:server 332 fred #nowhere :x\r\n"
	if got := string(d.tryAnswerJoin("omarchy", "fred", []byte("JOIN #nowhere\r\n"))); got != want {
		t.Fatalf("JOIN #nowhere after TOPIC = %q, want %q", got, want)
	}
}

// TestAnswersTopicClearAndQuery covers the TOPIC shapes that differ: an empty
// trailing text clears the topic, so a later JOIN has no 332, and a bare TOPIC
// query, or a TOPIC whose target is not a channel, is not answered.
func TestAnswersTopicClearAndQuery(t *testing.T) {
	_, d := attachController(t, true)

	if got := d.tryAnswerTopic("omarchy", []byte("TOPIC #desktop\r\n")); got != nil {
		t.Fatalf("TOPIC query answered %q", got)
	}
	if got := d.tryAnswerTopic("omarchy", []byte("TOPIC fred :x\r\n")); got != nil {
		t.Fatalf("TOPIC on a non-channel target answered %q", got)
	}
	if got := string(d.tryAnswerTopic("omarchy", []byte("TOPIC #desktop :\r\n"))); got != ":fred!u@h TOPIC #desktop :\r\n" {
		t.Fatalf("TOPIC clear = %q", got)
	}
	if got := string(d.tryAnswerJoin("omarchy", "fred", []byte("JOIN #desktop\r\n"))); got != ":fred!u@h JOIN :#desktop\r\n" {
		t.Fatalf("JOIN after clear = %q", got)
	}
}

// TestTopicEchoReachesController writes our TOPIC through the transport and
// checks that the controller shows the echoed topic on the channel.
func TestTopicEchoReachesController(t *testing.T) {
	c, d := attachController(t, true)
	c.SelectConversation("omarchy", "#desktop")
	d.omarchy.Write([]byte("TOPIC #desktop :new banner\r\n"))
	if got := c.Topic(); got != "new banner" {
		t.Fatalf("topic = %q, want the echoed banner", got)
	}
}

func conversationNames(c *controller.Controller) []string {
	names := make([]string, 0, len(c.Conversations()))
	for _, row := range c.Conversations() {
		names = append(names, row.Conversation)
	}
	return names
}

func TestEchoLastPrivmsg(t *testing.T) {
	c, d := attachController(t, true)
	c.SelectConversation("omarchy", "#omarchy")
	if !c.SendMessage("hello") {
		t.Fatal("SendMessage failed")
	}
	found := false
	for _, frame := range d.omarchy.WrittenFrames() {
		if strings.HasPrefix(string(frame), "PRIVMSG #omarchy :hello") {
			found = true
		}
	}
	if !found {
		t.Fatal("transport did not record the client PRIVMSG")
	}
	if !d.EchoLastOmarchyPrivmsg("fred") {
		t.Fatal("EchoLastOmarchyPrivmsg = false, want true")
	}
}

func containsString(values []string, wanted string) bool {
	for _, value := range values {
		if value == wanted {
			return true
		}
	}
	return false
}
