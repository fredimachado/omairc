// Package filehost uploads one local file to the HTTP endpoint a server
// advertised and returns the absolute web link from the response. It is the
// Go port of IrcFileUploader in src/irc/ircfilehost.cpp. The IRC core stays
// free of net/http; this package owns the socket.
package filehost

import (
	"bytes"
	"io"
	"mime"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

const (
	maximumBytes    = 32 * 1024 * 1024
	transferTimeout = 60 * time.Second
)

// Fixed failure strings. They match the desktop client and never include
// the password or the upload URL.
const (
	couldNotUpload = "Could not upload the file."
	fileTooLarge   = "The file is too large."
	fileEmpty      = "The file is empty."
	notAFile       = "That is not a file."
)

// Request is one file the composer asked to upload.
type Request struct {
	Endpoint        string
	Path            string
	User            string
	Secret          string
	ServerHost      string
	ServerEncrypted bool
	// HTTPClient is optional. Tests pass a client; redirects are still refused.
	HTTPClient *http.Client
}

// Upload posts the file and returns the absolute http(s) link, or a fixed
// failure message. A successful call returns a link and an empty message.
func Upload(req Request) (string, string) {
	body, fileName, contentType, message := readFile(req.Path)
	if message != "" {
		return "", message
	}
	endpoint, err := url.Parse(req.Endpoint)
	if err != nil || endpoint.Scheme == "" {
		return "", couldNotUpload
	}
	if req.ServerEncrypted && !strings.EqualFold(endpoint.Scheme, "https") {
		return "", couldNotUpload
	}

	httpReq, err := http.NewRequest(http.MethodPost, req.Endpoint, bytes.NewReader(body))
	if err != nil {
		return "", couldNotUpload
	}
	httpReq.Header.Set("Content-Type", contentType)
	httpReq.Header.Set("Content-Disposition", `attachment; filename="`+fileName+`"`)
	httpReq.ContentLength = int64(len(body))
	user := req.User
	secret := req.Secret
	if irc.FileHostSendsBasicAuth(req.ServerHost, req.Endpoint, req.ServerEncrypted) {
		if header := irc.BasicAuthorization(user, secret); header != "" {
			httpReq.Header.Set("Authorization", header)
		}
	}
	user = ""
	secret = ""

	resp, err := uploadClient(req.HTTPClient).Do(httpReq)
	if err != nil {
		return "", couldNotUpload
	}
	defer resp.Body.Close()
	_, _ = io.Copy(io.Discard, io.LimitReader(resp.Body, 1024))
	if resp.StatusCode != http.StatusCreated {
		return "", couldNotUpload
	}
	link := irc.AbsoluteFileLink(req.Endpoint, resp.Header.Get("Location"))
	if link == "" {
		return "", couldNotUpload
	}
	return link, ""
}

func uploadClient(base *http.Client) *http.Client {
	client := &http.Client{
		Timeout: transferTimeout,
		CheckRedirect: func(*http.Request, []*http.Request) error {
			return http.ErrUseLastResponse
		},
	}
	if base != nil {
		if base.Transport != nil {
			client.Transport = base.Transport
		}
		if base.Timeout > 0 {
			client.Timeout = base.Timeout
		}
	}
	return client
}

func readFile(path string) (body []byte, fileName, contentType, message string) {
	info, err := os.Stat(path)
	if err != nil || !info.Mode().IsRegular() {
		return nil, "", "", notAFile
	}
	if info.Size() <= 0 {
		return nil, "", "", fileEmpty
	}
	if info.Size() > maximumBytes {
		return nil, "", "", fileTooLarge
	}
	body, err = os.ReadFile(path)
	if err != nil {
		return nil, "", "", couldNotUpload
	}
	if len(body) == 0 {
		return nil, "", "", fileEmpty
	}
	if len(body) > maximumBytes {
		return nil, "", "", fileTooLarge
	}
	return body, safeFileName(info.Name()), contentTypeFor(path), ""
}

func safeFileName(name string) string {
	base := filepath.Base(name)
	var out []rune
	for _, character := range base {
		if character < 0x20 || character == '"' || character == '\\' || character == '/' || character == ';' {
			out = append(out, '_')
		} else {
			out = append(out, character)
		}
	}
	if len(out) == 0 {
		return "file"
	}
	if len(out) > 180 {
		out = out[len(out)-180:]
	}
	return string(out)
}

func contentTypeFor(path string) string {
	detected := mime.TypeByExtension(filepath.Ext(path))
	if semi := strings.IndexByte(detected, ';'); semi >= 0 {
		detected = strings.TrimSpace(detected[:semi])
	}
	if detected == "" {
		return "application/octet-stream"
	}
	return detected
}
