package controller

import "time"

// DateSeparators returns one label per selected message. An empty string means
// no date rule sits above that row. The first day is labeled only when it is
// not today; a later day is labeled whenever it differs, including today after
// an older day. Recomputing the labels after older history is prepended moves
// or drops the top rule so a date is never shown twice. It mirrors
// MessageListModel::buildView.
func (c *Controller) DateSeparators() []string {
	labels := make([]string, len(c.messages))
	if len(c.messages) == 0 {
		return labels
	}
	today := c.now()
	if today.IsZero() {
		today = time.Now()
	}
	today = today.In(c.displayLocation())
	var previous time.Time
	hasPrevious := false
	for index, message := range c.messages {
		date, ok := assignedDisplayDate(message, previous, hasPrevious, today)
		if !ok {
			continue
		}
		opensDay := !hasPrevious || !sameCalendarDay(date, previous)
		if opensDay && (hasPrevious || !sameCalendarDay(date, today)) {
			labels[index] = formatTranscriptDate(date, today)
		}
		previous = date
		hasPrevious = true
	}
	return labels
}

// assignedDisplayDate is the calendar day a row belongs to. A real timestamp
// uses the display clock the snapshot already localized. A live row with no
// timestamp is today, and a replay row with no timestamp keeps the previous
// day. It mirrors assignedDate in messagelistmodel.cpp.
func assignedDisplayDate(message MessageSnapshot, previous time.Time, hasPrevious bool, today time.Time) (time.Time, bool) {
	if !message.Time.IsZero() {
		return message.Time, true
	}
	if message.Origin == "live" {
		return today, true
	}
	if hasPrevious {
		return previous, true
	}
	return time.Time{}, false
}

func sameCalendarDay(a, b time.Time) bool {
	ay, am, ad := a.Date()
	by, bm, bd := b.Date()
	return ay == by && am == bm && ad == bd
}

// formatTranscriptDate is Today, Yesterday, or an unpadded M/d/yy. Qt uses
// QLocale::ShortFormat for the older case; the terminal client has no locale
// package and matches the en_US short date those tests print.
func formatTranscriptDate(date, today time.Time) string {
	if sameCalendarDay(date, today) {
		return "Today"
	}
	if sameCalendarDay(date, today.AddDate(0, 0, -1)) {
		return "Yesterday"
	}
	return date.Format("1/2/06")
}
