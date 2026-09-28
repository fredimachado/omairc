//go:build windows

package gate

import (
	"fmt"
	"os"
	"os/exec"
	"unicode/utf16"
	"unsafe"

	"golang.org/x/sys/windows"
)

const procThreadAttributePseudoConsole = 0x00020016

func startPty(opts Options) (*Process, error) {
	ptyIn, inOur, err := os.Pipe()
	if err != nil {
		return nil, fmt.Errorf("create conpty input pipe: %w", err)
	}
	outOur, ptyOut, err := os.Pipe()
	if err != nil {
		_ = ptyIn.Close()
		_ = inOur.Close()
		return nil, fmt.Errorf("create conpty output pipe: %w", err)
	}

	var hpc windows.Handle
	coord := windows.Coord{X: int16(opts.Cols), Y: int16(opts.Rows)}
	if err := windows.CreatePseudoConsole(coord, windows.Handle(ptyIn.Fd()), windows.Handle(ptyOut.Fd()), 0, &hpc); err != nil {
		closeAll(ptyIn, inOur, outOur, ptyOut)
		return nil, fmt.Errorf("create pseudo console: %w", err)
	}
	_ = ptyIn.Close()
	_ = ptyOut.Close()

	argv := append([]string{opts.Binary}, opts.Args...)
	cmdline, err := windows.UTF16PtrFromString(windows.ComposeCommandLine(argv))
	if err != nil {
		closeAll(inOur, outOur)
		windows.ClosePseudoConsole(hpc)
		return nil, err
	}
	argv0, err := windows.UTF16PtrFromString(opts.Binary)
	if err != nil {
		closeAll(inOur, outOur)
		windows.ClosePseudoConsole(hpc)
		return nil, err
	}

	var dirp *uint16
	if opts.Dir != "" {
		dirp, err = windows.UTF16PtrFromString(opts.Dir)
		if err != nil {
			closeAll(inOur, outOur)
			windows.ClosePseudoConsole(hpc)
			return nil, err
		}
	}

	env := opts.Env
	if env == nil {
		env = os.Environ()
	}
	envBlock := createEnvBlock(env)

	attrs, err := windows.NewProcThreadAttributeList(1)
	if err != nil {
		closeAll(inOur, outOur)
		windows.ClosePseudoConsole(hpc)
		return nil, fmt.Errorf("initialize proc thread attribute list: %w", err)
	}
	defer attrs.Delete()

	if err := attrs.Update(procThreadAttributePseudoConsole, unsafe.Pointer(&hpc), unsafe.Sizeof(hpc)); err != nil {
		closeAll(inOur, outOur)
		windows.ClosePseudoConsole(hpc)
		return nil, fmt.Errorf("attach pseudo console to process attributes: %w", err)
	}

	siEx := &windows.StartupInfoEx{}
	siEx.Cb = uint32(unsafe.Sizeof(*siEx))
	siEx.ProcThreadAttributeList = attrs.List()

	flags := uint32(windows.CREATE_UNICODE_ENVIRONMENT | windows.EXTENDED_STARTUPINFO_PRESENT)
	pi := &windows.ProcessInformation{}
	if err := windows.CreateProcess(argv0, cmdline, nil, nil, false, flags, envBlock, dirp, &siEx.StartupInfo, pi); err != nil {
		closeAll(inOur, outOur)
		windows.ClosePseudoConsole(hpc)
		return nil, fmt.Errorf("start %s on a conpty: %w", opts.Binary, err)
	}
	_ = windows.CloseHandle(pi.Thread)

	proc, err := os.FindProcess(int(pi.ProcessId))
	if err != nil {
		_ = windows.TerminateProcess(pi.Process, 1)
		closeAll(inOur, outOur)
		windows.ClosePseudoConsole(hpc)
		return nil, fmt.Errorf("find child process: %w", err)
	}

	cmd := &exec.Cmd{Process: proc}
	p := &Process{
		readFd:  outOur,
		writeFd: inOur,
		cmd:     cmd,
		exited:  make(chan struct{}),
		cleanup: func() error {
			windows.ClosePseudoConsole(hpc)
			return nil
		},
	}
	go func() {
		p.waitErr = cmd.Wait()
		close(p.exited)
	}()
	go p.readLoop()
	return p, nil
}

func closeAll(files ...*os.File) {
	for _, f := range files {
		if f != nil {
			_ = f.Close()
		}
	}
}

func createEnvBlock(envv []string) *uint16 {
	if len(envv) == 0 {
		return &utf16.Encode([]rune("\x00\x00"))[0]
	}
	length := 0
	for _, s := range envv {
		length += len(s) + 1
	}
	length++

	b := make([]byte, length)
	i := 0
	for _, s := range envv {
		l := len(s)
		copy(b[i:i+l], s)
		b[i+l] = 0
		i += l + 1
	}
	b[i] = 0
	return &utf16.Encode([]rune(string(b)))[0]
}
