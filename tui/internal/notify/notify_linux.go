//go:build linux

package notify

import (
	"sync"

	"github.com/godbus/dbus/v5"
)

const (
	notifyBusName  = "org.freedesktop.Notifications"
	notifyObject   = dbus.ObjectPath("/org/freedesktop/Notifications")
	notifyIface    = "org.freedesktop.Notifications"
	notifyMethod   = "org.freedesktop.Notifications.Notify"
	actionInvoked  = "org.freedesktop.Notifications.ActionInvoked"
	notifyAppName  = "Omairc"
	notifyIconName = "omairc"
)

// conversationKey mirrors notifyConversationKey in src/backend.cpp.
func conversationKey(networkID, target string) string {
	return networkID + "\n" + target
}

// linuxNotifier mirrors Backend::notifyDesktop over the session bus. It fails
// closed: when the session bus is unavailable it degrades to noopNotifier
// rather than panicking.
type linuxNotifier struct {
	conn        *dbus.Conn
	signals     chan *dbus.Signal
	done        chan struct{}
	closeOnce   sync.Once
	onActivated func(Activation)

	mu              sync.Mutex
	conversationIDs map[string]uint32
	byID            map[uint32]Activation
}

// newPlatform mirrors Backend's constructor: connect to the session bus if
// present and register for the ActionInvoked signal. Any failure returns the
// fail-closed no-op notifier.
func newPlatform(onActivated func(Activation)) Notifier {
	conn, err := dbus.SessionBus()
	if err != nil {
		return noopNotifier{}
	}

	// Register the signal match before subscribing so no ActionInvoked is
	// missed between the two calls, mirroring Backend's connect().
	if err := conn.AddMatchSignal(
		dbus.WithMatchObjectPath(notifyObject),
		dbus.WithMatchInterface(notifyIface),
		dbus.WithMatchMember("ActionInvoked"),
	); err != nil {
		// We opened the shared session bus but cannot use it; fail closed.
		_ = conn.Close()
		return noopNotifier{}
	}

	n := &linuxNotifier{
		conn:            conn,
		signals:         make(chan *dbus.Signal, 16),
		done:            make(chan struct{}),
		onActivated:     onActivated,
		conversationIDs: make(map[string]uint32),
		byID:            make(map[uint32]Activation),
	}
	conn.Signal(n.signals)
	go n.listen()
	return n
}

// listen ranges over the signal channel until Close is called. The channel is
// always drained so a signal burst never blocks the D-Bus connection.
func (n *linuxNotifier) listen() {
	for {
		select {
		case <-n.done:
			return
		case sig, ok := <-n.signals:
			if !ok {
				return
			}
			n.handleSignal(sig)
		}
	}
}

// handleSignal mirrors Backend::handleActionInvoked.
func (n *linuxNotifier) handleSignal(sig *dbus.Signal) {
	if sig == nil || sig.Name != actionInvoked || len(sig.Body) < 2 {
		return
	}
	id, ok := sig.Body[0].(uint32)
	if !ok {
		return
	}
	actionKey, ok := sig.Body[1].(string)
	if !ok {
		return
	}
	if actionKey != "default" && actionKey != "" {
		return
	}

	n.mu.Lock()
	activation, found := n.byID[id]
	n.mu.Unlock()
	if !found || n.onActivated == nil {
		return
	}
	n.onActivated(activation)
}

// Notify mirrors Backend::notifyDesktop. It never blocks: the D-Bus method
// call runs in its own goroutine and a failed call is silently ignored.
func (n *linuxNotifier) Notify(summary, body, networkID, target, msgid string) {
	var replacesID uint32
	if networkID != "" && target != "" {
		n.mu.Lock()
		replacesID = n.conversationIDs[conversationKey(networkID, target)]
		n.mu.Unlock()
	}

	obj := n.conn.Object(notifyBusName, notifyObject)
	go func() {
		call := obj.Call(notifyMethod, 0,
			notifyAppName,
			replacesID,
			notifyIconName,
			summary,
			body,
			[]string{"default", "Open"},
			map[string]dbus.Variant{},
			int32(-1),
		)
		if call.Err != nil {
			return
		}
		var id uint32
		if err := call.Store(&id); err != nil {
			return
		}
		n.rememberNotifyID(networkID, target, msgid, id)
	}()
}

// rememberNotifyID mirrors Backend::rememberNotifyId.
func (n *linuxNotifier) rememberNotifyID(networkID, target, msgid string, id uint32) {
	if networkID == "" || target == "" || id == 0 {
		return
	}
	key := conversationKey(networkID, target)

	n.mu.Lock()
	defer n.mu.Unlock()
	previous := n.conversationIDs[key]
	if previous != 0 && previous != id {
		delete(n.byID, previous)
	}
	n.conversationIDs[key] = id
	n.byID[id] = Activation{NetworkID: networkID, Target: target, MsgID: msgid}
}

// Close stops the signal goroutine and closes the session bus connection. It
// is idempotent and safe to call when the notifier is the fail-closed no-op.
func (n *linuxNotifier) Close() {
	n.closeOnce.Do(func() {
		close(n.done)
		if n.conn != nil {
			if n.signals != nil {
				n.conn.RemoveSignal(n.signals)
			}
			_ = n.conn.Close()
		}
	})
}
