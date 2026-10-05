package irc

import "strings"

// MembershipNoise is how join, part, quit, and nick lines are shown. Folded is
// the ordinary case. It mirrors IrcMembershipNoise.
type MembershipNoise int

const (
	// MembershipNoiseFolded merges consecutive lines and drops a leave that is
	// followed at once by a rejoin of the same nick.
	MembershipNoiseFolded MembershipNoise = iota
	// MembershipNoiseEvery shows each line on its own.
	MembershipNoiseEvery
	// MembershipNoiseHidden omits these lines, including history playback.
	MembershipNoiseHidden
)

// MembershipNoiseToken is the stored and /pref value. It mirrors
// ircMembershipNoiseToken.
func MembershipNoiseToken(noise MembershipNoise) string {
	switch noise {
	case MembershipNoiseEvery:
		return "every"
	case MembershipNoiseHidden:
		return "hidden"
	default:
		return "folded"
	}
}

// ParseMembershipNoise resolves a stored or typed value. An empty or unknown
// token is not folded; the caller applies the ordinary default. It mirrors
// ircMembershipNoiseFromToken.
func ParseMembershipNoise(token string) (MembershipNoise, bool) {
	switch strings.ToLower(strings.TrimSpace(token)) {
	case "folded":
		return MembershipNoiseFolded, true
	case "every":
		return MembershipNoiseEvery, true
	case "hidden":
		return MembershipNoiseHidden, true
	default:
		return MembershipNoiseFolded, false
	}
}

// ShownAccount is the parenthetical account, or "" when it adds nothing. It
// mirrors ircShownAccount.
func ShownAccount(sameAsNick bool, account string) string {
	if account == "" || account == "*" || sameAsNick {
		return ""
	}
	return account
}

// JoinLine renders one join the way a lone line reads today. It mirrors
// ircJoinLine.
func JoinLine(nick, shownAccount string) string {
	if shownAccount == "" {
		return nick + " joined"
	}
	return nick + " (" + shownAccount + ") joined"
}

// PartLine renders one part. It mirrors ircPartLine.
func PartLine(nick string) string { return nick + " left" }

// QuitLine renders one quit. It mirrors ircQuitLine.
func QuitLine(nick string) string { return nick + " quit" }

// NickLine renders one nick change. It mirrors ircNickLine.
func NickLine(oldNick, newNick string) string { return oldNick + " is now " + newNick }

// FoldKind is how one incoming membership line meets the line before it.
type FoldKind int

const (
	// FoldMerge appends with the ordinary same-suffix collapse.
	FoldMerge FoldKind = iota
	// FoldCancel drops a leave that the incoming join undoes.
	FoldCancel
	// FoldChain keeps the first old nick and the latest new nick.
	FoldChain
)

// MembershipFold is the result of folding one incoming line onto an existing
// collapsible event body.
type MembershipFold struct {
	Kind    FoldKind
	DropRow bool
	Body    string
}

type membershipClauseKind int

const (
	clauseJoined membershipClauseKind = iota
	clauseLeft
	clauseQuit
	clauseNick
)

type membershipClause struct {
	kind   membershipClauseKind
	tokens []string
}

func membershipNick(token string) string {
	const marker = " ("
	if strings.HasSuffix(token, ")") {
		if at := strings.Index(token, marker); at > 0 {
			return token[:at]
		}
	}
	return token
}

func renderMembershipClause(clause membershipClause) string {
	switch clause.kind {
	case clauseLeft:
		return strings.Join(clause.tokens, ", ") + " left"
	case clauseQuit:
		return strings.Join(clause.tokens, ", ") + " quit"
	case clauseNick:
		return clause.tokens[0] + " is now " + clause.tokens[1]
	default:
		return strings.Join(clause.tokens, ", ") + " joined"
	}
}

func renderMembershipClauses(clauses []membershipClause) string {
	parts := make([]string, len(clauses))
	for index, clause := range clauses {
		parts[index] = renderMembershipClause(clause)
	}
	return strings.Join(parts, ", ")
}

func parseMembershipClauses(body string) ([]membershipClause, bool) {
	if body == "" {
		return nil, false
	}
	var clauses []membershipClause
	index := 0
	for index < len(body) {
		rest := body[index:]
		joinedAt := strings.Index(rest, " joined")
		leftAt := strings.Index(rest, " left")
		quitAt := strings.Index(rest, " quit")
		nickAt := strings.Index(rest, " is now ")
		best := -1
		kind := clauseJoined
		suffixLen := 0
		consider := func(pos int, next membershipClauseKind, length int) {
			if pos >= 0 && (best < 0 || pos < best) {
				best = pos
				kind = next
				suffixLen = length
			}
		}
		consider(joinedAt, clauseJoined, len(" joined"))
		consider(leftAt, clauseLeft, len(" left"))
		consider(quitAt, clauseQuit, len(" quit"))
		consider(nickAt, clauseNick, len(" is now "))
		if best <= 0 {
			return nil, false
		}
		if kind == clauseNick {
			oldNick := rest[:best]
			cursor := best + suffixLen
			end := cursor
			for end < len(rest) && rest[end] != ',' {
				end++
			}
			newNick := rest[cursor:end]
			if newNick == "" || strings.Contains(newNick, " ") {
				return nil, false
			}
			clauses = append(clauses, membershipClause{kind: clauseNick, tokens: []string{oldNick, newNick}})
			index += end
		} else {
			nickList := rest[:best]
			tokens := splitCommaSpace(nickList)
			if len(tokens) == 0 {
				return nil, false
			}
			clauses = append(clauses, membershipClause{kind: kind, tokens: tokens})
			index += best + suffixLen
		}
		if index >= len(body) {
			break
		}
		if !strings.HasPrefix(body[index:], ", ") {
			return nil, false
		}
		index += 2
		if index >= len(body) {
			return nil, false
		}
	}
	if len(clauses) == 0 || renderMembershipClauses(clauses) != body {
		return nil, false
	}
	return clauses, true
}

func splitCommaSpace(text string) []string {
	if text == "" {
		return nil
	}
	parts := strings.Split(text, ", ")
	out := make([]string, 0, len(parts))
	for _, part := range parts {
		if part != "" {
			out = append(out, part)
		}
	}
	return out
}

// FoldMembership folds one incoming membership line onto the body before it.
// same reports whether two nicks are the same under the network case mapping.
// A leave followed at once by a rejoin of that nick drops the leave. A nick
// chain keeps the first name and the final name. Anything else uses the
// ordinary suffix collapse.
func FoldMembership(existing, incoming string, same func(left, right string) bool) MembershipFold {
	current, currentOK := parseMembershipClauses(existing)
	next, nextOK := parseMembershipClauses(incoming)
	if !currentOK || !nextOK || len(next) != 1 {
		return MembershipFold{Kind: FoldMerge, Body: collapseEventBody(existing, incoming)}
	}
	clauses := append([]membershipClause(nil), current...)
	for index := range clauses {
		tokens := make([]string, len(clauses[index].tokens))
		copy(tokens, clauses[index].tokens)
		clauses[index].tokens = tokens
	}
	incomingClause := next[0]
	last := &clauses[len(clauses)-1]
	if incomingClause.kind == clauseJoined && len(incomingClause.tokens) == 1 &&
		(last.kind == clauseLeft || last.kind == clauseQuit) && len(last.tokens) > 0 &&
		same(membershipNick(last.tokens[len(last.tokens)-1]), membershipNick(incomingClause.tokens[0])) {
		last.tokens = last.tokens[:len(last.tokens)-1]
		if len(last.tokens) == 0 {
			clauses = clauses[:len(clauses)-1]
		}
		if len(clauses) == 0 {
			return MembershipFold{Kind: FoldCancel, DropRow: true}
		}
		return MembershipFold{Kind: FoldCancel, Body: renderMembershipClauses(clauses)}
	}
	if incomingClause.kind == clauseNick && last.kind == clauseNick &&
		len(last.tokens) == 2 && len(incomingClause.tokens) == 2 &&
		same(last.tokens[1], incomingClause.tokens[0]) {
		last.tokens[1] = incomingClause.tokens[1]
		return MembershipFold{Kind: FoldChain, Body: renderMembershipClauses(clauses)}
	}
	return MembershipFold{Kind: FoldMerge, Body: collapseEventBody(existing, incoming)}
}
