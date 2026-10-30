//go:build !linux

package avatar

import (
	"os"
	"testing"
)

func TestMain(m *testing.M) {
	clearAmbientColorEnv()
	os.Exit(m.Run())
}
