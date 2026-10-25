package storage

// secretServiceUnlockIncomplete reports whether Unlock left any searched item
// locked or asked for interactive authorization. Write cleanup and remove fail
// closed on true so a still-locked single-set copy cannot outlive Available.
func secretServiceUnlockIncomplete(items, unlocked []string, prompt string) bool {
	if prompt != "" && prompt != "/" {
		return true
	}
	seen := make(map[string]struct{}, len(unlocked))
	for _, item := range unlocked {
		seen[item] = struct{}{}
	}
	for _, item := range items {
		if _, ok := seen[item]; !ok {
			return true
		}
	}
	return false
}
