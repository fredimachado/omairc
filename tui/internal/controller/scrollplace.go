package controller

import (
	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/storage"
)

// ScrollView is the selected conversation's saved place, resolved to a row in
// Messages(). Known is false when this conversation has no place. Follow means
// the viewport was pinned to the bottom. Row is -1 when the anchor is not in
// the loaded transcript yet; Known stays true so the caller does not fall
// through to open-at-unread.
type ScrollView struct {
	Known  bool
	Follow bool
	Row    int
}

// RememberScrollPlace records the open conversation's viewport. followEnd
// pins the bottom. Otherwise anchorRow is the first visible message index in
// Messages(). Status is not a saved conversation.
func (c *Controller) RememberScrollPlace(followEnd bool, anchorRow int) {
	if c == nil || c.consoleOpen || c.selected == nil || c.scrollPlaces == nil {
		return
	}
	conversation := c.reducer.Find(*c.selected)
	if conversation == nil {
		return
	}
	place := storage.ScrollPlace{FollowEnd: followEnd}
	if !followEnd {
		if anchorRow < 0 || anchorRow >= len(conversation.Messages) {
			return
		}
		message := conversation.Messages[anchorRow]
		place.MsgID = message.MsgID.Value
		place.Author = message.Author
		place.Body = message.Body
		place.Kind = messageKindName(message.Kind)
		if !message.Timestamp.IsZero() {
			place.EpochMs = message.Timestamp.UTC().UnixMilli()
			place.HasTime = true
		}
	}
	features := c.reducer.ServerFeatures(c.selected.NetworkID)
	c.scrollPlaces.Remember(c.selected.NetworkID, c.selectedTarget, place, features.CaseMapping())
}

// CurrentScrollPlace resolves the selected conversation's saved place.
func (c *Controller) CurrentScrollPlace() ScrollView {
	view := ScrollView{Row: -1}
	if c == nil || c.consoleOpen || c.selected == nil || c.scrollPlaces == nil {
		return view
	}
	features := c.reducer.ServerFeatures(c.selected.NetworkID)
	place, ok := c.scrollPlaces.Place(c.selected.NetworkID, c.selectedTarget, features.CaseMapping())
	if !ok {
		return view
	}
	view.Known = true
	if place.FollowEnd {
		view.Follow = true
		return view
	}
	conversation := c.reducer.Find(*c.selected)
	if conversation == nil {
		return view
	}
	for index, message := range conversation.Messages {
		if scrollPlaceMatches(place, message) {
			view.Row = index
			return view
		}
	}
	return view
}

func scrollPlaceMatches(place storage.ScrollPlace, message irc.ReducedMessage) bool {
	if place.MsgID != "" {
		return message.MsgID.Value == place.MsgID
	}
	if messageKindName(message.Kind) != place.Kind || message.Author != place.Author || message.Body != place.Body {
		return false
	}
	if !place.HasTime {
		return true
	}
	if message.Timestamp.IsZero() {
		return false
	}
	return message.Timestamp.UTC().UnixMilli() == place.EpochMs
}
