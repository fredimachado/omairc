package session

import (
	"testing"
	"time"
)

func slotStamp(seconds int) time.Time {
	return time.Date(2024, 6, 1, 12, 0, seconds, 0, time.UTC)
}

func TestSetSlotOfferTransitions(t *testing.T) {
	known := slotStamp(5)
	tests := []struct {
		name      string
		slot      setSlot
		when      time.Time
		lastKnown *time.Time
		want      setSlot
		wantSend  *sendSet
	}{
		{"idle sends", slotIdle{}, slotStamp(10), nil, slotInFlight{Sent: slotStamp(10)}, &sendSet{"#a", slotStamp(10)}},
		{"idle ignores stamp not after last known", slotIdle{}, slotStamp(5), &known, slotIdle{}, nil},
		{"idle sends after last known", slotIdle{}, slotStamp(6), &known, slotInFlight{Sent: slotStamp(6)}, &sendSet{"#a", slotStamp(6)}},
		{"in flight queues newer", slotInFlight{Sent: slotStamp(10)}, slotStamp(20), nil, slotQueued{Sent: slotStamp(10), Next: slotStamp(20), Target: "#a"}, nil},
		{"in flight ignores equal", slotInFlight{Sent: slotStamp(10)}, slotStamp(10), nil, slotInFlight{Sent: slotStamp(10)}, nil},
		{"in flight ignores older", slotInFlight{Sent: slotStamp(10)}, slotStamp(9), nil, slotInFlight{Sent: slotStamp(10)}, nil},
		{"queued replaces next with newer", slotQueued{Sent: slotStamp(10), Next: slotStamp(20), Target: "#old"}, slotStamp(30), nil, slotQueued{Sent: slotStamp(10), Next: slotStamp(30), Target: "#a"}, nil},
		{"queued ignores equal next", slotQueued{Sent: slotStamp(10), Next: slotStamp(20), Target: "#old"}, slotStamp(20), nil, slotQueued{Sent: slotStamp(10), Next: slotStamp(20), Target: "#old"}, nil},
		{"queued ignores older next", slotQueued{Sent: slotStamp(10), Next: slotStamp(20), Target: "#old"}, slotStamp(15), nil, slotQueued{Sent: slotStamp(10), Next: slotStamp(20), Target: "#old"}, nil},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			got, send := offerSlot(test.slot, test.when, "#a", test.lastKnown)
			if got != test.want {
				t.Fatalf("slot = %#v, want %#v", got, test.want)
			}
			if (send == nil) != (test.wantSend == nil) || (send != nil && *send != *test.wantSend) {
				t.Fatalf("send = %#v, want %#v", send, test.wantSend)
			}
		})
	}
}

func TestSetSlotEchoedTransitions(t *testing.T) {
	queued := slotQueued{Sent: slotStamp(10), Next: slotStamp(20), Target: "#a"}
	tests := []struct {
		name     string
		slot     setSlot
		echoed   time.Time
		want     setSlot
		wantSend *sendSet
	}{
		{"idle stays idle", slotIdle{}, slotStamp(10), slotIdle{}, nil},
		{"equal echo releases in flight", slotInFlight{Sent: slotStamp(10)}, slotStamp(10), slotIdle{}, nil},
		{"newer echo releases in flight", slotInFlight{Sent: slotStamp(10)}, slotStamp(11), slotIdle{}, nil},
		{"older echo leaves in flight", slotInFlight{Sent: slotStamp(10)}, slotStamp(9), slotInFlight{Sent: slotStamp(10)}, nil},
		{"older echo leaves queued", queued, slotStamp(9), queued, nil},
		{"echo of sent promotes next", queued, slotStamp(10), slotInFlight{Sent: slotStamp(20)}, &sendSet{"#a", slotStamp(20)}},
		{"echo equal to next drops it", queued, slotStamp(20), slotIdle{}, nil},
		{"echo past next drops it", queued, slotStamp(25), slotIdle{}, nil},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			got, send := echoedSlot(test.slot, test.echoed)
			if got != test.want {
				t.Fatalf("slot = %#v, want %#v", got, test.want)
			}
			if (send == nil) != (test.wantSend == nil) || (send != nil && *send != *test.wantSend) {
				t.Fatalf("send = %#v, want %#v", send, test.wantSend)
			}
		})
	}
}

func TestSetSlotReleasedTransitions(t *testing.T) {
	got, send := releasedSlot(slotIdle{})
	if got != (slotIdle{}) || send != nil {
		t.Fatalf("idle released = %#v %#v", got, send)
	}
	got, send = releasedSlot(slotInFlight{Sent: slotStamp(10)})
	if got != (slotIdle{}) || send != nil {
		t.Fatalf("in flight released = %#v %#v", got, send)
	}
	got, send = releasedSlot(slotQueued{Sent: slotStamp(10), Next: slotStamp(20), Target: "#a"})
	if got != (slotInFlight{Sent: slotStamp(20)}) || send == nil || *send != (sendSet{"#a", slotStamp(20)}) {
		t.Fatalf("queued released = %#v %#v", got, send)
	}
	// A FAIL leaves the same stamp free to retry.
	got, send = offerSlot(slotIdle{}, slotStamp(10), "#a", nil)
	if got != (slotInFlight{Sent: slotStamp(10)}) || send == nil {
		t.Fatalf("same stamp must offer again after release: %#v %#v", got, send)
	}
}
