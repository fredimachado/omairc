package demo

import (
	"bytes"
	"strconv"
	"strings"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// This file is the Go port of the anonymous auto-echo handlers in
// src/irc/ircdemoserver.cpp. Each tryAnswer* returns the reply bytes, or nil
// when it does not answer; the wiring in demo.go injects a non-nil result. A
// handled case that emits nothing returns a non-nil zero-length slice so the
// caller can tell "answered" from "not mine".

// demoServerFeatures builds the demo ISUPPORT set from kIsupport, mirroring
// demoServerFeatures().
func demoServerFeatures() irc.ServerFeatures {
	features := irc.NewServerFeatures()
	tokens := make([]string, 0, 3)
	for _, token := range strings.Split(kIsupport, " ") {
		if token != "" {
			tokens = append(tokens, token)
		}
	}
	features.ApplyTokens(tokens)
	return features
}

// listMaskMatches applies a /list mask: comma-separated, an empty mask matches
// all, and each part is a case-insensitive glob (`*` and `?`) or exact match.
// It mirrors listMaskMatches().
func listMaskMatches(name, mask string) bool {
	if mask == "" {
		return true
	}
	parts := splitSkipEmpty(mask, ',')
	if len(parts) == 0 {
		return true
	}
	for _, part := range parts {
		part = strings.TrimSpace(part)
		if part == "" {
			continue
		}
		if strings.ContainsAny(part, "*?") {
			if wildcardMatches(strings.ToLower(part), strings.ToLower(name)) {
				return true
			}
		} else if strings.EqualFold(name, part) {
			return true
		}
	}
	return false
}

// wildcardMatches is a case-folded glob matcher for `*` and `?`. Callers lower
// both inputs first.
func wildcardMatches(pattern, text string) bool {
	patternIndex, textIndex := 0, 0
	star, mark := -1, 0
	for textIndex < len(text) {
		switch {
		case patternIndex < len(pattern) && (pattern[patternIndex] == '?' || pattern[patternIndex] == text[textIndex]):
			patternIndex++
			textIndex++
		case patternIndex < len(pattern) && pattern[patternIndex] == '*':
			star = patternIndex
			mark = textIndex
			patternIndex++
		case star >= 0:
			patternIndex = star + 1
			mark++
			textIndex = mark
		default:
			return false
		}
	}
	for patternIndex < len(pattern) && pattern[patternIndex] == '*' {
		patternIndex++
	}
	return patternIndex == len(pattern)
}

// listEntries is the /list source: seeded channels first, then the directory.
// It mirrors listEntries().
func listEntries(network SeedNetwork) []SeedListed {
	rows := make([]SeedListed, 0, len(network.Channels)+len(network.Directory))
	for _, channel := range network.Channels {
		rows = append(rows, SeedListed{
			Name:  channel.Name,
			Users: len(channel.Members),
			Topic: channel.Topic,
		})
	}
	rows = append(rows, network.Directory...)
	return rows
}

// tryAnswerPing answers a client PING with a PONG. It mirrors tryAnswerPing().
func (d *DemoServer) tryAnswerPing(frame []byte) []byte {
	wire := stripCrlf(frame)
	if !bytes.HasPrefix(wire, []byte("PING ")) {
		return nil
	}
	message, ok := parseDemoFrame(frame)
	if !ok || message.Command != "PING" || len(message.Params) == 0 {
		return nil
	}
	token := irc.WireText([]byte(message.Params[len(message.Params)-1]))
	return line(":server PONG irc.example :" + token)
}

// tryAnswerList answers a /list mask with 321/322/323. It mirrors
// tryAnswerList().
func (d *DemoServer) tryAnswerList(network SeedNetwork, frame []byte) []byte {
	wire := stripCrlf(frame)
	if !bytes.Equal(wire, []byte("LIST")) && !bytes.HasPrefix(wire, []byte("LIST ")) {
		return nil
	}
	mask := ""
	if len(wire) > 5 {
		mask = strings.TrimSpace(string(wire[5:]))
	}

	out := line(":server 321 " + network.Nick + " Channel :Users  Name")
	for _, entry := range listEntries(network) {
		if !listMaskMatches(entry.Name, mask) {
			continue
		}
		out = append(out, line(":server 322 "+network.Nick+" "+entry.Name+" "+
			strconv.Itoa(entry.Users)+" :"+entry.Topic)...)
	}
	out = append(out, line(":server 323 "+network.Nick+" :End of /LIST")...)
	return out
}

// tryAnswerCtcp answers a client-to-client CTCP query addressed to our nick,
// never to a channel. It mirrors tryAnswerCtcp().
func (d *DemoServer) tryAnswerCtcp(selfNick string, frame []byte) []byte {
	if selfNick == "" {
		return nil
	}
	message, ok := parseDemoFrame(frame)
	if !ok || message.Command != "PRIVMSG" || len(message.Params) < 2 {
		return nil
	}
	request, ok := irc.ParseCtcpRequest(irc.WireText([]byte(message.Params[1])))
	if !ok || request.Command == "ACTION" {
		return nil
	}
	features := demoServerFeatures()
	if features.IsChannel(message.Params[0]) {
		return nil
	}

	var argument string
	switch request.Command {
	case "PING":
		argument = request.Argument
	case "TIME":
		argument = time.Now().Format(time.RFC1123Z)
	case "VERSION":
		argument = irc.CtcpVersionReplyText()
	default:
		return nil
	}

	targetNick := irc.WireText([]byte(message.Params[0]))
	if targetNick == "" {
		return nil
	}
	out := demoLabelPrefix(demoTagValue(message, "label"))
	out = append(out, ':')
	out = append(out, targetNick...)
	out = append(out, "!u@h NOTICE "...)
	out = append(out, selfNick...)
	out = append(out, " :"...)
	out = append(out, irc.CtcpPayload(irc.CtcpRequest{Command: request.Command, Argument: argument})...)
	out = append(out, "\r\n"...)
	return out
}

// tryAnswerAway answers our own AWAY with 306 or 305. It mirrors
// tryAnswerAway().
func (d *DemoServer) tryAnswerAway(selfNick string, frame []byte) []byte {
	if selfNick == "" {
		return nil
	}
	wire := stripCrlf(frame)
	if !bytes.Equal(wire, []byte("AWAY")) && !bytes.HasPrefix(wire, []byte("AWAY :")) {
		return nil
	}
	if len(wire) > len("AWAY") {
		return line(":server 306 " + selfNick + " :You have been marked as being away")
	}
	return line(":server 305 " + selfNick + " :You are no longer marked as being away")
}

// tryAnswerMetadata answers METADATA SET on our own nick with 761 or 766. It
// mirrors tryAnswerMetadata().
func (d *DemoServer) tryAnswerMetadata(selfNick string, frame []byte) []byte {
	if selfNick == "" {
		return nil
	}
	if !bytes.HasPrefix(stripCrlf(frame), []byte("METADATA ")) {
		return nil
	}
	message, ok := parseDemoFrame(frame)
	if !ok || message.Command != "METADATA" || len(message.Params) < 3 {
		return nil
	}
	target := irc.WireText([]byte(message.Params[0]))
	if target != "*" && !strings.EqualFold(target, selfNick) {
		return nil
	}
	if !strings.EqualFold(irc.WireText([]byte(message.Params[1])), "SET") {
		return nil
	}
	stored := irc.CanonicalKey(irc.WireText([]byte(message.Params[2])))
	if stored == "" {
		return nil
	}
	value := ""
	if len(message.Params) >= 4 {
		value = irc.Clamped(irc.WireText([]byte(message.Params[3])))
	}
	if value == "" {
		return line(":server 766 " + selfNick + " " + selfNick + " " + stored + " :unset")
	}
	return line(":server 761 " + selfNick + " " + selfNick + " " + stored + " * :" + value)
}

// tryAnswerMonitor is the full MONITOR state machine: C clears, L lists, S
// re-reports, and +/- add or remove with a 734 overflow. It returns a non-nil
// zero-length slice when the command was handled but emits nothing. It mirrors
// tryAnswerMonitor().
func (d *DemoServer) tryAnswerMonitor(networkID, selfNick string, frame []byte, online []string) []byte {
	if selfNick == "" || !bytes.HasPrefix(frame, []byte("MONITOR ")) {
		return nil
	}
	message, ok := parseDemoFrame(frame)
	if !ok || message.Command != "MONITOR" || len(message.Params) == 0 {
		return nil
	}

	modifier := irc.WireText([]byte(message.Params[0]))
	targetText := ""
	if len(modifier) > 1 && (modifier[0] == '+' || modifier[0] == '-') {
		targetText = modifier[1:]
		modifier = modifier[:1]
	} else if len(message.Params) >= 2 {
		targetText = irc.WireText([]byte(message.Params[1]))
	}
	modifier = strings.ToUpper(modifier)

	watched := d.monitorLists[networkID]

	switch modifier {
	case "C":
		d.monitorLists[networkID] = nil
		return []byte{}
	case "L":
		var out []byte
		if len(watched) > 0 {
			out = append(out, injectMonitorNumeric(selfNick, "732", strings.Join(watched, ","))...)
		}
		out = append(out, line(":server 733 "+selfNick+" :End of MONITOR list")...)
		return out
	case "S":
		return injectMonitorStates(selfNick, watched, online)
	case "+", "-":
	default:
		return nil
	}

	targets := splitSkipEmpty(targetText, ',')
	if modifier == "-" {
		for _, nick := range targets {
			if index := demoMonitorIndex(watched, nick); index >= 0 {
				watched = append(watched[:index], watched[index+1:]...)
			}
		}
		d.monitorLists[networkID] = watched
		return []byte{}
	}

	var added []string
	var overflow []string
	for _, nick := range targets {
		if nick == "" || demoMonitorIndex(watched, nick) >= 0 ||
			demoMonitorIndex(added, nick) >= 0 {
			continue
		}
		if len(watched)+len(added) >= kMonitorLimit {
			overflow = append(overflow, nick)
		} else {
			added = append(added, nick)
		}
	}
	watched = append(watched, added...)
	d.monitorLists[networkID] = watched

	out := []byte{}
	if len(added) > 0 {
		out = append(out, injectMonitorStates(selfNick, added, online)...)
	}
	if len(overflow) > 0 {
		out = append(out, line(":server 734 "+selfNick+" "+
			strconv.Itoa(kMonitorLimit)+" "+strings.Join(overflow, ",")+
			" :Monitor list is full.")...)
	}
	return out
}

// injectMonitorNumeric renders one 730/731/732/733/734 numeric. It mirrors
// injectMonitorNumeric().
func injectMonitorNumeric(selfNick, code, trailing string) []byte {
	return line(":server " + code + " " + selfNick + " :" + trailing)
}

// injectMonitorStates splits nicks into online (730, with a "!u@h" suffix) and
// offline (731) batches. It mirrors injectMonitorStates().
func injectMonitorStates(selfNick string, nicks, online []string) []byte {
	var on []string
	var off []string
	for _, nick := range nicks {
		if demoNickIsOnline(online, nick) {
			on = append(on, nick+"!u@h")
		} else {
			off = append(off, nick)
		}
	}
	out := []byte{}
	if len(on) > 0 {
		out = append(out, injectMonitorNumeric(selfNick, "730", strings.Join(on, ","))...)
	}
	if len(off) > 0 {
		out = append(out, injectMonitorNumeric(selfNick, "731", strings.Join(off, ","))...)
	}
	return out
}

// injectClientEcho wraps a client frame as if the server echoed it back. It
// mirrors injectClientEcho().
func injectClientEcho(selfNick string, frame []byte) []byte {
	out := []byte(":" + selfNick + "!u@h ")
	return append(out, frame...)
}

// demoNickEquals is a case-insensitive nick comparison, mirroring
// demoNickEquals().
func demoNickEquals(left, right string) bool {
	return strings.EqualFold(left, right)
}

// demoMonitorIndex returns the watched index of nick, or -1. It mirrors
// demoMonitorIndex().
func demoMonitorIndex(nicks []string, nick string) int {
	for index, candidate := range nicks {
		if demoNickEquals(candidate, nick) {
			return index
		}
	}
	return -1
}

// demoNickIsOnline reports whether nick appears in online. It mirrors
// demoNickIsOnline().
func demoNickIsOnline(online []string, nick string) bool {
	return demoMonitorIndex(online, nick) >= 0
}

// demoOnlineNicks is our own nick, every channel member, and every direct peer.
// The C++ returns QSet::values() (unordered); this port sorts for determinism.
// It mirrors demoOnlineNicks().
func demoOnlineNicks(network SeedNetwork) []string {
	nicks := make(map[string]struct{})
	nicks[network.Nick] = struct{}{}
	for _, channel := range network.Channels {
		for _, nick := range channel.Members {
			nicks[nick] = struct{}{}
		}
	}
	for _, direct := range network.Directs {
		nicks[direct.Nick] = struct{}{}
	}
	return sortedSetKeys(nicks)
}

// echoLastPrivmsg replays the most recent client PRIVMSG back as a self echo.
// It mirrors echoLastPrivmsg().
func (d *DemoServer) echoLastPrivmsg(networkID, nick string) bool {
	transport := d.transportFor(networkID)
	if transport == nil || nick == "" {
		return false
	}
	frames := transport.WrittenFrames()
	for index := len(frames) - 1; index >= 0; index-- {
		if !bytes.HasPrefix(frames[index], []byte("PRIVMSG ")) {
			continue
		}
		d.inject(networkID, injectClientEcho(nick, frames[index]))
		return true
	}
	return false
}
