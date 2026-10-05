package irc

import "strings"

// JumpResultLimit is how many Ctrl+K rows the overlay keeps. Name hits stay
// above detail-only hits, and sidebar order holds inside each group.
const JumpResultLimit = 20

// JumpScore scores one jump row. A name hit is 2 and a topic or real-name hit
// is 1, so both is 3. Callers keep every name hit above every detail-only hit
// and keep sidebar order inside each group. An empty query scores 0, which
// the caller includes. A non-empty query with no hit also scores 0, which the
// caller skips. It mirrors ircJumpScore.
func JumpScore(query, name, detail string) int {
	folded := strings.ToLower(strings.TrimSpace(query))
	if folded == "" {
		return 0
	}
	score := 0
	if strings.Contains(strings.ToLower(name), folded) {
		score += 2
	}
	if detail != "" && strings.Contains(strings.ToLower(detail), folded) {
		score += 1
	}
	return score
}
