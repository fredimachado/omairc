//go:build live

package live

import (
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// DaemonInfo is one daemon from OMAIRC_LIVE_DAEMONS with its published ports
// and the fixture facts the cases assert. It mirrors DaemonInfo in
// `client/src/live/env.rs`; keep the per-daemon facts in step with it.
type DaemonInfo struct {
	Name                 string
	PlainPort            uint16
	TLSPort              uint16
	SASL                 bool
	AwayNotify           bool
	MessageTags          bool
	Metadata             bool
	WantedCapsAdvertised bool
	Mapping              irc.Kind
	NickLength           int
}

// UseTLS prefers plain when the daemon publishes a plain port.
func (d DaemonInfo) UseTLS() bool { return d.PlainPort == 0 }

// Port returns the TLS or plain port.
func (d DaemonInfo) Port(tls bool) uint16 {
	if tls {
		return d.TLSPort
	}
	return d.PlainPort
}

// Enabled reports whether bin/test-live (or an equivalent) exported daemons.
func Enabled() bool {
	return strings.TrimSpace(os.Getenv("OMAIRC_LIVE_DAEMONS")) != ""
}

// Host is the address the fixtures publish on.
func Host() string {
	if host := os.Getenv("OMAIRC_LIVE_HOST"); host != "" {
		return host
	}
	return "127.0.0.1"
}

func envPort(key string) uint16 {
	value, err := strconv.ParseUint(os.Getenv(key), 10, 16)
	if err != nil {
		return 0
	}
	return uint16(value)
}

func certDir() string {
	if dir := os.Getenv("OMAIRC_LIVE_CERT_DIR"); dir != "" {
		return dir
	}
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		return filepath.Join("..", "..", "..", "tests", "live", "certs")
	}
	return filepath.Join(filepath.Dir(file), "..", "..", "..", "tests", "live", "certs")
}

// CertPEM returns the bytes of the fixtures' self-signed live.pem.
func CertPEM() ([]byte, error) {
	return os.ReadFile(filepath.Join(certDir(), "live.pem"))
}

// Daemons lists the daemons in OMAIRC_LIVE_DAEMONS that published a port.
func Daemons() []DaemonInfo {
	var daemons []DaemonInfo
	for _, name := range strings.Split(os.Getenv("OMAIRC_LIVE_DAEMONS"), ",") {
		name = strings.ToLower(strings.TrimSpace(name))
		var info DaemonInfo
		switch name {
		case "ergo":
			info = DaemonInfo{
				Name:                 name,
				PlainPort:            envPort("OMAIRC_LIVE_ERGO_PLAIN_PORT"),
				TLSPort:              envPort("OMAIRC_LIVE_ERGO_TLS_PORT"),
				SASL:                 true,
				AwayNotify:           true,
				MessageTags:          true,
				Metadata:             true,
				WantedCapsAdvertised: true,
				Mapping:              irc.KindAscii,
				NickLength:           32,
			}
		case "solanum":
			info = DaemonInfo{
				Name:                 name,
				PlainPort:            envPort("OMAIRC_LIVE_SOLANUM_PLAIN_PORT"),
				TLSPort:              envPort("OMAIRC_LIVE_SOLANUM_TLS_PORT"),
				AwayNotify:           true,
				MessageTags:          true,
				WantedCapsAdvertised: true,
				Mapping:              irc.KindRfc1459,
				NickLength:           16,
			}
		case "ngircd":
			info = DaemonInfo{
				Name:       name,
				PlainPort:  envPort("OMAIRC_LIVE_NGIRCD_PLAIN_PORT"),
				Mapping:    irc.KindAscii,
				NickLength: 9,
			}
		default:
			continue
		}
		if info.PlainPort == 0 && info.TLSPort == 0 {
			continue
		}
		daemons = append(daemons, info)
	}
	return daemons
}
