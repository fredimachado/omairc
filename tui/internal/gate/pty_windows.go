//go:build windows

package gate

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"unicode/utf16"
	"unsafe"

	"golang.org/x/sys/windows"
)

const procThreadAttributePseudoConsole = windows.PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE

func startPty(opts Options) (*Process, error) {
	sa := &windows.SecurityAttributes{
		Length:        uint32(unsafe.Sizeof(windows.SecurityAttributes{})),
		InheritHandle: 1,
	}

	var inRead, inWrite windows.Handle
	if err := windows.CreatePipe(&inRead, &inWrite, sa, 0); err != nil {
		return nil, fmt.Errorf("create conpty input pipe: %w", err)
	}
	var outRead, outWrite windows.Handle
	if err := windows.CreatePipe(&outRead, &outWrite, sa, 0); err != nil {
		closeHandles(inRead, inWrite)
		return nil, fmt.Errorf("create conpty output pipe: %w", err)
	}

	var hpc windows.Handle
	coord := windows.Coord{X: int16(opts.Cols), Y: int16(opts.Rows)}
	if err := windows.CreatePseudoConsole(coord, inRead, outWrite, 0, &hpc); err != nil {
		closeHandles(inRead, inWrite, outRead, outWrite)
		return nil, fmt.Errorf("create pseudo console: %w", err)
	}
	_ = windows.CloseHandle(inRead)
	_ = windows.CloseHandle(outWrite)

	binary, err := resolveWindowsBinary(opts.Binary, opts.Dir)
	if err != nil {
		closeHandles(outRead, inWrite)
		windows.ClosePseudoConsole(hpc)
		return nil, err
	}
	argv := append([]string{binary}, opts.Args...)
	cmdline, err := windows.UTF16PtrFromString(windows.ComposeCommandLine(argv))
	if err != nil {
		closeHandles(outRead, inWrite)
		windows.ClosePseudoConsole(hpc)
		return nil, err
	}

	var dirp *uint16
	if opts.Dir != "" {
		dirp, err = windows.UTF16PtrFromString(opts.Dir)
		if err != nil {
			closeHandles(outRead, inWrite)
			windows.ClosePseudoConsole(hpc)
			return nil, err
		}
	}

	envBlock := createEnvBlock(ensureCriticalEnv(opts.Env))

	attrs, err := windows.NewProcThreadAttributeList(1)
	if err != nil {
		closeHandles(outRead, inWrite)
		windows.ClosePseudoConsole(hpc)
		return nil, fmt.Errorf("initialize proc thread attribute list: %w", err)
	}
	defer attrs.Delete()

	if err := attrs.Update(procThreadAttributePseudoConsole, unsafe.Pointer(&hpc), unsafe.Sizeof(hpc)); err != nil {
		closeHandles(outRead, inWrite)
		windows.ClosePseudoConsole(hpc)
		return nil, fmt.Errorf("attach pseudo console to process attributes: %w", err)
	}

	siEx := &windows.StartupInfoEx{}
	siEx.Cb = uint32(unsafe.Sizeof(*siEx))
	siEx.ProcThreadAttributeList = attrs.List()

	flags := uint32(windows.EXTENDED_STARTUPINFO_PRESENT | windows.CREATE_UNICODE_ENVIRONMENT)
	pi := &windows.ProcessInformation{}
	if err := windows.CreateProcess(nil, cmdline, nil, nil, false, flags, envBlock, dirp, &siEx.StartupInfo, pi); err != nil {
		closeHandles(outRead, inWrite)
		windows.ClosePseudoConsole(hpc)
		return nil, fmt.Errorf("start %s on a conpty: %w", opts.Binary, err)
	}
	_ = windows.CloseHandle(pi.Thread)

	readFd := os.NewFile(uintptr(outRead), "conpty-out")
	writeFd := os.NewFile(uintptr(inWrite), "conpty-in")
	procHandle := pi.Process

	cmd := &exec.Cmd{Process: mustFindProcess(int(pi.ProcessId))}
	p := &Process{
		readFd:  readFd,
		writeFd: writeFd,
		cmd:     cmd,
		exited:  make(chan struct{}),
		cleanup: func() error {
			windows.ClosePseudoConsole(hpc)
			_ = windows.CloseHandle(procHandle)
			return nil
		},
	}
	go func() {
		p.waitErr = waitForProcess(procHandle)
		close(p.exited)
	}()
	go p.readLoop()
	return p, nil
}

func mustFindProcess(pid int) *os.Process {
	proc, err := os.FindProcess(pid)
	if err != nil {
		return &os.Process{Pid: pid}
	}
	return proc
}

func waitForProcess(handle windows.Handle) error {
	if _, err := windows.WaitForSingleObject(handle, windows.INFINITE); err != nil {
		return err
	}
	var code uint32
	if err := windows.GetExitCodeProcess(handle, &code); err != nil {
		return err
	}
	if code != 0 {
		return fmt.Errorf("exit status 0x%x", code)
	}
	return nil
}

func closeHandles(handles ...windows.Handle) {
	for _, h := range handles {
		if h != 0 {
			_ = windows.CloseHandle(h)
		}
	}
}

// resolveWindowsBinary returns an absolute path CreateProcess can start. A bare
// name such as cmd.exe is not searched on PATH when lpApplicationName is nil.
func resolveWindowsBinary(binary, dir string) (string, error) {
	if filepath.IsAbs(binary) {
		return binary, nil
	}
	if dir != "" && !strings.ContainsAny(binary, `/\`) {
		candidate := filepath.Join(dir, binary)
		if _, err := os.Stat(candidate); err == nil {
			return candidate, nil
		}
	}
	if path, err := exec.LookPath(binary); err == nil {
		return path, nil
	}
	systemRoot := os.Getenv("SystemRoot")
	if systemRoot != "" {
		candidate := filepath.Join(systemRoot, "System32", binary)
		if _, err := os.Stat(candidate); err == nil {
			return candidate, nil
		}
	}
	return binary, nil
}

// ensureCriticalEnv returns opts.Env or the current environment, guaranteeing
// SYSTEMROOT is present so ConPTY children can resolve system binaries.
func ensureCriticalEnv(env []string) []string {
	if env == nil {
		env = os.Environ()
	}
	for _, kv := range env {
		if eq := strings.IndexByte(kv, '='); eq > 0 && strings.EqualFold(kv[:eq], "SYSTEMROOT") {
			return env
		}
	}
	if root := os.Getenv("SYSTEMROOT"); root != "" {
		return append(env, "SYSTEMROOT="+root)
	}
	return env
}

func createEnvBlock(envv []string) *uint16 {
	if len(envv) == 0 {
		return nil
	}
	var block []uint16
	for _, entry := range envv {
		block = append(block, utf16.Encode([]rune(entry))...)
		block = append(block, 0)
	}
	block = append(block, 0)
	return &block[0]
}
