package demo

import "strconv"

// This file is the Go port of the anonymous seed helpers and the omarchyWorld /
// oftcWorld builders in src/irc/ircdemoserver.cpp. The data is content- and
// order-identical to the C++; only the container types differ (slices for
// QVector/QStringList, RankPair for QPair<QString, QString>).

const (
	// kToday and kYesterday are the demo clock's fixed days. The seed uses them
	// verbatim as the IRCv3 server-time tag date.
	kToday     = "2026-09-12"
	kYesterday = "2026-09-11"

	// kCaps is the CAP LS/ACK list the demo server advertises.
	kCaps = "echo-message message-tags labeled-response away-notify multi-prefix batch " +
		"draft/metadata-2 server-time account-tag account-notify extended-join"

	// kIsupport is the 005 token list the demo server advertises.
	kIsupport = "CHANTYPES=# PREFIX=(qaohv)~&@%+ MONITOR=100"

	// kMonitorLimit is the MONITOR entry ceiling, mirroring IrcServerFeatures.
	kMonitorLimit = 100
)

// SeedKind distinguishes a transcript chat line from a join line. It mirrors
// SeedLine::Kind.
type SeedKind int

const (
	// SeedChat is a PRIVMSG row.
	SeedChat SeedKind = iota
	// SeedJoin is a JOIN row; only Nick and Account are meaningful.
	SeedJoin
)

// SeedLine is one transcript row. It mirrors the C++ SeedLine.
type SeedLine struct {
	Kind    SeedKind
	Nick    string
	Body    string
	Day     string
	HHMM    string
	Account string
}

// RankPair joins a nick to a PREFIX rank or a metadata value. The C++ uses the
// same QPair<QString, QString> shape for both ranks and statuses.
type RankPair struct {
	Nick  string
	Value string
}

// SeedChannel is one seeded channel. It mirrors the C++ SeedChannel.
type SeedChannel struct {
	Name     string
	Topic    string
	Members  []string
	LateJoin []string
	Away     []string
	Ranks    []RankPair
	Statuses []RankPair
	Lines    []SeedLine
	MarkRead bool
}

// SeedDirect is one seeded direct message. It mirrors the C++ SeedDirect.
type SeedDirect struct {
	Nick     string
	Lines    []SeedLine
	MarkRead bool
	Typing   bool
}

// SeedListed is one /list directory entry. It mirrors the C++ SeedListed.
type SeedListed struct {
	Name  string
	Users int
	Topic string
}

// SeedNetwork is one seeded network. It mirrors the C++ SeedNetwork.
type SeedNetwork struct {
	NetworkID string
	Nick      string
	Welcome   string
	IconURL   string
	Channels  []SeedChannel
	Directs   []SeedDirect
	Directory []SeedListed
}

// chat builds a Chat seed line. day is optional and defaults to kToday,
// mirroring the C++ default argument.
func chat(nick, body, hhmm string, day ...string) SeedLine {
	date := kToday
	if len(day) > 0 {
		date = day[0]
	}
	return SeedLine{Kind: SeedChat, Nick: nick, Body: body, Day: date, HHMM: hhmm}
}

// chatAccount is chat with an explicit account tag, used by the one seeded row
// that carries both a day and an account.
func chatAccount(nick, body, hhmm, day, account string) SeedLine {
	row := chat(nick, body, hhmm, day)
	row.Account = account
	return row
}

// join builds a Join seed line. account is optional.
func join(nick string, account ...string) SeedLine {
	row := SeedLine{Kind: SeedJoin, Nick: nick}
	if len(account) > 0 {
		row.Account = account[0]
	}
	return row
}

// omarchyWorld returns the primary demo network. It is ported verbatim from
// omarchyWorld() in src/irc/ircdemoserver.cpp.
func omarchyWorld() SeedNetwork {
	network := SeedNetwork{
		NetworkID: "omarchy",
		Nick:      "fred",
		Welcome:   "Welcome to the demo network",
		IconURL:   "qrc:/demo/omarchy-icon.png",
	}

	omarchy := SeedChannel{
		Name:  "#omarchy",
		Topic: "A cozy corner for Omarchy users and builders.",
		Members: []string{
			"anna", "dax", "mira", "sol", "fred", "kai",
			"nora", "teo", "lena", "sam", "ivy", "max",
		},
		LateJoin: []string{"sol", "nora"},
		Away:     []string{"teo", "lena", "sam", "ivy", "max"},
		// The demo ladder: founder, admin, two ops, halfop, voice, then the
		// rest plain. mira holds two ranks at once, which multi-prefix allows,
		// and teo is away and voiced, because rank and presence are
		// independent.
		//
		// The lateJoin members arrive through the transcript, and an arriving
		// member carries no rank until a server says so, so they stay out of
		// this list and out of the initial NAMES.
		Ranks: []RankPair{
			{"fred", "~"},
			{"anna", "&"},
			{"dax", "@"},
			{"mira", "@+"},
			{"kai", "%"},
			{"teo", "+"},
		},
		Statuses: []RankPair{
			{"anna", "writing docs"},
			{"dax", "on #desktop"},
			{"mira", "making tea"},
			{"sol", "new here"},
			{"fred", "building Omairc"},
		},
		Lines: []SeedLine{
			chat("anna", "Morning! Has anyone tried the new minimal install flow yet?", "09:41"),
			chat("dax", "Yes. Fresh install on my Framework took about twelve minutes. The defaults feel really considered.", "09:43"),
			chat("mira", "The way the theme carries across the terminal and native apps is my favorite detail.", "09:46"),
			join("sol", "solarius"),
			chat("sol", "Hey all. Just landed here from Arch. This feels surprisingly calm.", "09:52"),
			chat("anna", "Welcome, sol. Calm is the whole idea.", "09:53"),
			chat("dax", "If you have not already, try the keyboard-first app launcher. It becomes muscle memory fast.", "09:55"),
			chat("sol", "I found it. The shortcuts sheet is a nice touch too.", "09:56"),
			chat("mira", "Most of the system makes sense once you learn three or four core bindings.", "09:57"),
			chat("anna", "And everything important is still plain text when you want to look underneath.", "09:58"),
			chatAccount("kai", "That balance is hard to get right: friendly defaults without hiding the actual system.", "09:59", kToday, "kaidev"),
			chat("dax", "Exactly. Start simple, then make it yours one deliberate change at a time.", "10:00"),
			join("nora"),
			chat("nora", "Good timing. I was just looking for a quiet place to ask about native Omarchy apps.", "10:01"),
			chat("fred", "I am sketching a tiny IRC client that belongs here. No browser chrome, no clutter.", "10:02"),
			chat("mira", "Keep the member list optional and I am sold.", "10:04"),
		},
	}

	desktop := SeedChannel{
		Name:    "#desktop",
		Topic:   "Desktops should feel personal, fast, and calm.",
		Members: []string{"anna", "dax", "mira", "sol", "fred", "kai", "nora", "teo"},
		Lines: []SeedLine{
			chat("dax", "I finally moved every workspace rule into a small, readable file.", "08:22"),
			chat("mira", "That is the dream. Configuration you can understand in one sitting.", "08:24"),
			chat("sol", "Does anyone use a vertical monitor alongside the main display?", "10:08"),
		},
	}

	ricing := SeedChannel{
		Name:    "#ricing",
		Topic:   "Themes, type, wallpapers, and the tiny details.",
		Members: []string{"anna", "dax", "mira", "sol", "fred", "kai", "nora", "teo", "lena", "sam"},
		Lines: []SeedLine{
			chat("anna", "Muted colors, one strong accent, and enough breathing room.", "18:10", kYesterday),
			chat("mira", "Typography does more work than decoration ever will.", "18:13", kYesterday),
			chat("dax", "Dropped a new warm theme in the usual place. It looks great after sunset.", "18:20", kYesterday),
		},
	}
	const (
		ricingUnreadTarget = 12
		ricingMentionLine  = 1
	)
	ricingPadCount := ricingUnreadTarget - len(ricing.Lines) - ricingMentionLine
	for index := 1; index <= ricingPadCount; index++ {
		ricing.Lines = append(ricing.Lines,
			chat("anna", "rice-pad-"+strconv.Itoa(index), "18:2"+strconv.Itoa(index), kYesterday))
	}
	ricing.Lines = append(ricing.Lines,
		chat("mira", "looks good, fred.", "18:29", kYesterday))

	help := SeedChannel{
		Name:     "#help",
		Topic:    "Ask a clear question. Share what you already tried.",
		Members:  []string{"anna", "dax", "mira", "sol", "fred"},
		MarkRead: true,
		Lines: []SeedLine{
			chat("mira", "Tip: include the command output and the exact behavior you expected.", "09:11"),
			chat("sol", "That made my monitor issue much easier to diagnose. Thanks.", "09:15"),
		},
	}

	anna := SeedDirect{
		Nick:   "anna",
		Typing: true,
		Lines: []SeedLine{
			chat("anna", "fred: The prototype already feels at home. Nice work.", "10:12"),
		},
	}

	dax := SeedDirect{
		Nick:     "dax",
		MarkRead: true,
		Lines: []SeedLine{
			chat("dax", "Send me the build when the mock is ready.", "18:40", kYesterday),
		},
	}

	network.Channels = []SeedChannel{omarchy, desktop, ricing, help}
	network.Directs = []SeedDirect{anna, dax}
	network.Directory = []SeedListed{
		{
			// Literal control bytes: QChar(0x02) STX around "Kernel", then
			// QChar(0x03) with "04" as mIRC color 04 around "distro", then a
			// bare QChar(0x03) reset.
			Name:  "#linux",
			Users: 42,
			Topic: "\x02Kernel\x02 discussion and \x03" + "04distro" + "\x03 help.",
		},
		{Name: "#random", Users: 4, Topic: "Off-topic chatter."},
	}
	return network
}

// oftcWorld returns the secondary demo network. It is ported verbatim from
// oftcWorld() in src/irc/ircdemoserver.cpp.
func oftcWorld() SeedNetwork {
	network := SeedNetwork{
		NetworkID: "oftc",
		Nick:      "oak",
		Welcome:   "Welcome to the demo OFTC network",
	}

	omarchy := SeedChannel{
		Name:    "#omarchy",
		Topic:   "A different #omarchy, hosted on OFTC.",
		Members: []string{"rio", "ness", "oak", "pip"},
		Away:    []string{"pip"},
		Statuses: []RankPair{
			{"rio", "on #lab"},
			{"ness", "watching CI"},
			{"oak", "building Omairc"},
		},
		MarkRead: true,
		Lines: []SeedLine{
			chat("rio", "This #omarchy is the OFTC one. Different people, same name.", "11:02"),
			chat("oak", "Good. If the sidebar mixed them, we would already be lost.", "11:04"),
		},
	}

	lab := SeedChannel{
		Name:     "#lab",
		Topic:    "Build lab for packaging and CI.",
		Members:  []string{"rio", "ness", "oak", "pip", "jules", "remy"},
		Away:     []string{"pip"},
		MarkRead: true,
		Lines: []SeedLine{
			chat("ness", "Package build is green on the new runner.", "10:18"),
			chat("rio", "Leave the log in #build if it fails after sunset.", "10:21"),
		},
	}

	build := SeedChannel{
		Name:    "#build",
		Topic:   "Nightly builds and failing tests.",
		Members: []string{"ness", "oak", "rio"},
		Lines: []SeedLine{
			chat("ness", "Nightly failed on missing qt6keychain. Looking.", "09:05"),
			chat("oak", "Patched. Waiting on the next image.", "09:12"),
			chat("remy", "Image is cooking.", "09:13"),
		},
	}

	rio := SeedDirect{
		Nick:     "rio",
		MarkRead: true,
		Lines: []SeedLine{
			chat("rio", "Ping me on OFTC, not Libera.", "11:40"),
		},
	}

	network.Channels = []SeedChannel{omarchy, lab, build}
	network.Directs = []SeedDirect{rio}
	network.Directory = []SeedListed{
		{Name: "#debian", Users: 28, Topic: "Debian GNU/Linux."},
	}
	return network
}
