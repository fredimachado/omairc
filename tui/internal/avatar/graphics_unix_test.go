//go:build unix

package avatar

import (
	"bytes"
	"io"
	"testing"
	"time"

	"github.com/creack/pty"
)

func TestProbeGraphicsPTYReply(t *testing.T) {
	master, slave, err := pty.Open()
	if err != nil {
		t.Skip(err)
	}
	defer master.Close()
	defer slave.Close()

	done := make(chan struct{})
	go func() {
		defer close(done)
		buf := make([]byte, 0, 256)
		tmp := make([]byte, 128)
		_ = master.SetReadDeadline(time.Now().Add(2 * time.Second))
		for {
			n, err := master.Read(tmp)
			if n > 0 {
				buf = append(buf, tmp[:n]...)
				if bytes.Contains(buf, []byte("a=q")) && bytes.Contains(buf, []byte("\x1b[c")) {
					_, _ = master.Write([]byte("\x1b_Gi=4294967295;OK\x1b\\\x1b[?1;2c"))
					return
				}
			}
			if err != nil {
				return
			}
		}
	}()

	started := time.Now()
	ok, _ := ProbeGraphics(slave, slave, time.Second)
	if !ok {
		t.Fatal("pty OK did not enable graphics")
	}
	if waited := time.Since(started); waited > 500*time.Millisecond {
		t.Fatalf("pty OK waited %s for DA1", waited)
	}
	<-done
}

func TestProbeGraphicsPTYTimeout(t *testing.T) {
	master, slave, err := pty.Open()
	if err != nil {
		t.Skip(err)
	}
	defer master.Close()
	defer slave.Close()

	go func() { _, _ = io.Copy(io.Discard, master) }()

	started := time.Now()
	if ok, _ := ProbeGraphics(slave, slave, 80*time.Millisecond); ok {
		t.Fatal("silent pty enabled graphics")
	}
	if waited := time.Since(started); waited < 40*time.Millisecond {
		t.Fatalf("silent pty returned in %s", waited)
	}
}
