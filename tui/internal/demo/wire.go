package demo

import (
	"sort"
	"strings"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
)

// This file is the Go port of the anonymous wire builders in
// src/irc/ircdemoserver.cpp: line assembly, inbound frame parsing, the
// registration burst, channel state, presence, account, transcript, and typing
// bytes, plus the world/transcript injectors.

// line appends CRLF to text, mirroring the anonymous line() helper.
func line(text string) []byte {
	return []byte(text + "\r\n")
}

// stripCrlf removes one trailing CRLF or LF, mirroring stripCrlf().
func stripCrlf(frame []byte) []byte {
	if len(frame) >= 2 && frame[len(frame)-2] == '\r' && frame[len(frame)-1] == '\n' {
		return frame[:len(frame)-2]
	}
	if len(frame) >= 1 && frame[len(frame)-1] == '\n' {
		return frame[:len(frame)-1]
	}
	return frame
}

// parseDemoFrame strips CRLF and parses the line. ok is false on a parse error,
// mirroring parseDemoFrame().
func parseDemoFrame(frame []byte) (irc.Message, bool) {
	message, err := irc.Parse(string(stripCrlf(frame)))
	if err != nil {
		return irc.Message{}, false
	}
	return message, true
}

// demoTagValue returns the first tag value named name, or "". It mirrors
// demoTagValue().
func demoTagValue(message irc.Message, name string) string {
	for _, tag := range message.Tags {
		if tag.Name == name && tag.Value != nil {
			return irc.WireText([]byte(*tag.Value))
		}
	}
	return ""
}

// demoLabelPrefix renders the IRCv3 labeled-response prefix, or nothing.
func demoLabelPrefix(label string) []byte {
	if label == "" {
		return nil
	}
	return []byte("@label=" + label + " ")
}

// privmsg renders one seed chat row as a tagged PRIVMSG. The account tag is
// emitted first, then the server-time tag, and only the present tags are joined
// with a semicolon, mirroring privmsg().
func privmsg(row SeedLine, target string) []byte {
	var tags []byte
	if row.Account != "" {
		tags = append(tags, "account="...)
		tags = append(tags, row.Account...)
	}
	if row.Day != "" && row.HHMM != "" {
		if len(tags) > 0 {
			tags = append(tags, ';')
		}
		tags = append(tags, "time="...)
		tags = append(tags, row.Day...)
		tags = append(tags, 'T')
		tags = append(tags, row.HHMM...)
		tags = append(tags, ":00.000Z"...)
	}

	var out []byte
	if len(tags) > 0 {
		out = append(out, '@')
		out = append(out, tags...)
		out = append(out, ' ')
	}
	out = append(out, ':')
	out = append(out, row.Nick...)
	out = append(out, "!u@h PRIVMSG "...)
	out = append(out, target...)
	out = append(out, " :"...)
	out = append(out, row.Body...)
	out = append(out, "\r\n"...)
	return out
}

// joinLine renders a JOIN. An account carrier uses the extended-join trailing
// form, mirroring joinLine().
func joinLine(nick, channel, account string) []byte {
	if account != "" {
		return line(":" + nick + "!u@h JOIN " + channel + " " + account + " :joined")
	}
	return line(":" + nick + "!u@h JOIN :" + channel)
}

// accountLine renders an account-notify ACCOUNT line.
func accountLine(nick, account string) []byte {
	return line(":" + nick + "!u@h ACCOUNT " + account)
}

// removeAll drops every occurrence of unwanted from values, mirroring
// QStringList::removeAll.
func removeAll(values []string, unwanted string) []string {
	kept := values[:0]
	for _, value := range values {
		if value != unwanted {
			kept = append(kept, value)
		}
	}
	return kept
}

// initialMembers is the member list without the lateJoin arrivals, mirroring
// initialMembers().
func initialMembers(channel SeedChannel) []string {
	names := append([]string(nil), channel.Members...)
	for _, late := range channel.LateJoin {
		names = removeAll(names, late)
	}
	return names
}

// namesTokens prepends each member's PREFIX symbol. A NAMES reply carries the
// PREFIX symbols in front of the nick, and the demo network advertises the full
// ladder, so a member may carry more than one (mira holds "@+"). It mirrors
// namesTokens().
func namesTokens(channel SeedChannel) []string {
	tokens := initialMembers(channel)
	for index, token := range tokens {
		for _, rank := range channel.Ranks {
			if rank.Nick != token {
				continue
			}
			tokens[index] = rank.Value + token
			break
		}
	}
	return tokens
}

// registrationBytes is the CAP/registration burst. It matches every numeric's
// parameter count and text in registrationBytes().
func registrationBytes(nick, welcome, iconURL string) []byte {
	var out []byte
	out = append(out, line(":server CAP "+nick+" LS :"+kCaps)...)
	out = append(out, line(":server CAP "+nick+" ACK :"+kCaps)...)
	out = append(out, line(":AUTH!AUTH@localhost NOTICE "+nick+" :*** Looking up your hostname...")...)
	out = append(out, line(":server 001 "+nick+" :"+welcome)...)
	out = append(out, line(":server 002 "+nick+" :Your host is demo.omairc, running version 1.0")...)
	out = append(out, line(":server 003 "+nick+" :This server was created Fri Sep 18 2026")...)
	out = append(out, line(":server 004 "+nick+" demo.omairc OmaircDemo iw abc def")...)
	out = append(out, line(":server 250 "+nick+" :Highest connection count: 42")...)
	out = append(out, line(":server 251 "+nick+" :There are 12 users and 3 invisible on this server")...)
	out = append(out, line(":server 252 "+nick+" 1 :IRC Operator online")...)
	out = append(out, line(":server 253 "+nick+" 0 :unknown connections")...)
	out = append(out, line(":server 254 "+nick+" 3 :channels formed")...)
	out = append(out, line(":server 255 "+nick+" :I have 9 clients and 1 servers")...)
	out = append(out, line(":server 265 "+nick+" 9 12 :Current local users 9, max 12")...)
	out = append(out, line(":server 266 "+nick+" 12 42 :Current global users 12, max 42")...)
	isupport := kIsupport
	if iconURL != "" {
		isupport += " draft/ICON=" + iconURL
	}
	out = append(out, line(":server 005 "+nick+" "+isupport+" :are supported by this server")...)
	return out
}

// channelStateBytes is one channel's self-JOIN, 332 topic, 353 NAMES, and 366
// end-of-NAMES burst. It mirrors channelStateBytes().
func channelStateBytes(network SeedNetwork, channel SeedChannel) []byte {
	var out []byte
	out = append(out, joinLine(network.Nick, channel.Name, "")...)
	out = append(out, line(":server 332 "+network.Nick+" "+channel.Name+" :"+channel.Topic)...)
	out = append(out, line(":server 353 "+network.Nick+" = "+channel.Name+" :"+strings.Join(namesTokens(channel), " "))...)
	out = append(out, line(":server 366 "+network.Nick+" "+channel.Name+" :End of NAMES")...)
	return out
}

// presenceBytes is the union of every channel's away and status facts plus the
// deterministic avatar/bot/display-name/pronouns metadata lines. The C++
// iterates QSet, so away and status line order is not part of the contract;
// this port sorts the nick keys but emits the same set of lines. It mirrors
// presenceBytes().
func presenceBytes(network SeedNetwork) []byte {
	away := make(map[string]struct{})
	status := make(map[string]string)
	members := make(map[string]struct{})
	for _, channel := range network.Channels {
		for _, nick := range channel.Away {
			away[nick] = struct{}{}
		}
		for _, entry := range channel.Statuses {
			status[entry.Nick] = entry.Value
		}
		for _, nick := range channel.Members {
			members[nick] = struct{}{}
		}
	}

	var out []byte
	for _, nick := range sortedSetKeys(away) {
		out = append(out, line(":"+nick+"!u@h AWAY :away")...)
	}
	for _, nick := range sortedStringKeys(status) {
		out = append(out, line(":server 761 "+network.Nick+" "+nick+" status * :"+status[nick])...)
	}
	hasMember := func(nick string) bool {
		_, ok := members[nick]
		return ok
	}
	if hasMember("dax") {
		out = append(out, line(":server 761 "+network.Nick+" dax bot * :PacketBot")...)
	}
	// Bundled demo art loads through the avatar store's qrc path so
	// --demo-server shows real glyphs without outbound HTTPS or weakening
	// ircAvatarUrlIsSafe for network URLs.
	if hasMember("mira") {
		out = append(out, line(":server 761 "+network.Nick+" mira avatar * :qrc:/demo/mira-avatar.png")...)
	}
	if hasMember("anna") {
		out = append(out, line(":server 761 "+network.Nick+" anna avatar * :qrc:/demo/anna-avatar.png")...)
		out = append(out, line(":server 761 "+network.Nick+" anna display-name * :Anna Docs")...)
		out = append(out, line(":server 761 "+network.Nick+" anna pronouns * :she/her")...)
	}
	if hasMember("kai") {
		out = append(out, line(":server 761 "+network.Nick+" kai avatar * :qrc:/demo/kai-avatar.png")...)
	}
	return out
}

// accountBytes carries the peers already in NAMES and the self row. Only the
// omarchy network seeds accounts this way. It mirrors accountBytes().
func accountBytes(network SeedNetwork) []byte {
	if network.NetworkID != "omarchy" {
		return nil
	}
	var out []byte
	out = append(out, accountLine("lena", "pinkieval")...)
	out = append(out, accountLine(network.Nick, "fredm")...)
	return out
}

// liveAccountBytes is the account-notify that arrives after the seeded world is
// up. Only omarchy seeds it. It mirrors liveAccountBytes().
func liveAccountBytes(network SeedNetwork) []byte {
	if network.NetworkID != "omarchy" {
		return nil
	}
	return accountLine("teo", "teoval")
}

// transcriptBytes replays every channel and direct line. Channel JOINs carry
// their account; channel chats target the channel; direct chats target our own
// nick. It mirrors transcriptBytes().
func transcriptBytes(network SeedNetwork) []byte {
	var out []byte
	for _, channel := range network.Channels {
		for _, row := range channel.Lines {
			if row.Kind == SeedJoin {
				out = append(out, joinLine(row.Nick, channel.Name, row.Account)...)
			} else {
				out = append(out, privmsg(row, channel.Name)...)
			}
		}
	}
	for _, direct := range network.Directs {
		for _, row := range direct.Lines {
			out = append(out, privmsg(row, network.Nick)...)
		}
	}
	return out
}

// typingBytes emits an active typing hint for each direct that seeds one, once
// into #omarchy and once addressed to our own nick. It mirrors typingBytes().
func typingBytes(network SeedNetwork) []byte {
	var out []byte
	for _, direct := range network.Directs {
		if !direct.Typing {
			continue
		}
		out = append(out, line("@+typing=active :"+direct.Nick+"!u@h TAGMSG #omarchy")...)
		out = append(out, line("@+typing=active :"+direct.Nick+"!u@h TAGMSG "+network.Nick)...)
	}
	return out
}

// autojoinNames is every seeded channel name in seed order, mirroring
// autojoinNames().
func autojoinNames(network SeedNetwork) []string {
	names := make([]string, 0, len(network.Channels))
	for _, channel := range network.Channels {
		names = append(names, channel.Name)
	}
	return names
}

// injectWorld pushes each channel's state burst, then the presence and account
// bytes. It mirrors injectWorld().
func injectWorld(transport *session.LoopbackTransport, network SeedNetwork) {
	var out []byte
	for _, channel := range network.Channels {
		out = append(out, channelStateBytes(network, channel)...)
	}
	out = append(out, presenceBytes(network)...)
	out = append(out, accountBytes(network)...)
	transport.InjectBytes(out)
}

// injectTranscript pushes the replayed transcript. It mirrors
// injectTranscript().
func injectTranscript(transport *session.LoopbackTransport, network SeedNetwork) {
	transport.InjectBytes(transcriptBytes(network))
}

// markRead selects every channel and direct the seed marks read, exactly like
// markRead(). The selection clears the reducer's unread counts; the controller
// republishes the sidebar on the next conversation-dirtying event.
func markRead(c *controller.Controller, network SeedNetwork) {
	for _, channel := range network.Channels {
		if channel.MarkRead {
			c.SelectConversation(network.NetworkID, channel.Name)
		}
	}
	for _, direct := range network.Directs {
		if direct.MarkRead {
			c.SelectConversation(network.NetworkID, direct.Nick)
		}
	}
}

// splitSkipEmpty splits text on separator, dropping empty parts, mirroring
// QString::split(..., Qt::SkipEmptyParts).
func splitSkipEmpty(text string, separator byte) []string {
	parts := make([]string, 0, strings.Count(text, string(separator))+1)
	for _, part := range strings.Split(text, string(separator)) {
		if part != "" {
			parts = append(parts, part)
		}
	}
	return parts
}

func sortedSetKeys(values map[string]struct{}) []string {
	keys := make([]string, 0, len(values))
	for key := range values {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	return keys
}

func sortedStringKeys(values map[string]string) []string {
	keys := make([]string, 0, len(values))
	for key := range values {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	return keys
}
