package controller

import (
	"github.com/fredimachado/omairc/tui/internal/filehost"
	"github.com/fredimachado/omairc/tui/internal/irc"
)

// FileHost returns the upload URI for the selected network, or "". An
// encrypted IRC connection refuses a plain http URI. With no live session
// the connection is treated as encrypted, so only https is offered.
func (c *Controller) FileHost() string {
	if c == nil || c.selected == nil {
		return ""
	}
	encrypted := true
	if session := c.Session(c.selected.NetworkID); session != nil {
		encrypted = session.TLSEnabled()
	}
	features := c.reducer.ServerFeatures(c.selected.NetworkID)
	return features.FileHost(encrypted)
}

// FileUploadBusy reports whether an upload goroutine is still running.
// FinishFileUpload clears it on the Update goroutine.
func (c *Controller) FileUploadBusy() bool {
	return c != nil && c.fileUploadBusy
}

// BeginFileUpload snapshots the endpoint and credentials, then uploads path
// on another goroutine. The goroutine only calls OnFileLink. It returns
// false when no file host is offered, a snapshot is missing, or an upload
// is already running, and it does not set the busy flag in those cases.
func (c *Controller) BeginFileUpload(path string) bool {
	if c == nil || c.fileUploadBusy || path == "" || c.selected == nil {
		return false
	}
	endpoint := c.FileHost()
	if endpoint == "" {
		return false
	}
	req := filehost.Request{
		Endpoint:        endpoint,
		Path:            path,
		ServerEncrypted: true,
	}
	if session := c.Session(c.selected.NetworkID); session != nil {
		user, secret, host, encrypted := session.UploadCredential()
		req.User = user
		req.Secret = secret
		req.ServerHost = host
		req.ServerEncrypted = encrypted
	}
	callback := c.OnFileLink
	c.fileUploadBusy = true
	go func() {
		url, message := filehost.Upload(req)
		if callback != nil {
			callback(url, message)
		}
	}()
	return true
}

// FinishFileUpload clears the busy flag. The shell calls it from Update
// when the upload goroutine's message arrives.
func (c *Controller) FinishFileUpload() {
	if c == nil {
		return
	}
	c.fileUploadBusy = false
}

// NoteFileUploadFailure records a fixed failure line on Status for networkID.
// The text must already be one of the uploader's fixed strings. It does not
// read the conversation that is open now.
func (c *Controller) NoteFileUploadFailure(networkID, message string) {
	if c == nil || message == "" || networkID == "" {
		return
	}
	c.statusConsole.Append(irc.Outcome(networkID, message, c.now()))
}
