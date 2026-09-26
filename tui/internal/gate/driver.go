package gate

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"image"
	"image/png"
	"io"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"

	"github.com/fredimachado/omairc/tui/internal/version"
)

// Why a daemon
//
// A recipe is a sequence of separate CLI invocations, but the child PTY, the
// reconstructed Screen, and the OSC title must survive across them. So
// `launch` starts one detached `serve` process that owns the PTY and serves a
// Unix socket; every other verb is a short-lived client that connects, sends
// one JSON request, and reads one JSON response. `cleanup` tells the daemon to
// quit.
//
// State lives in $OMAIRC_TUI_STATE, or os.TempDir()/omairc-verify-tui-$USER:
// state.json (pid, socket, binary, root, demo, size, started), daemon.log, and
// the socket itself.

const (
	defaultCols        = 118
	defaultRows        = 30
	defaultFeature     = "shell"
	defaultShotName    = "screenshot"
	defaultWaitTimeout = 10 * time.Second
	launchTimeout      = 15 * time.Second
	daemonQuitGrace    = 2 * time.Second
	daemonStopTimeout  = 10 * time.Second
)

// Request is one client-to-daemon command. Verb is the only required field.
type Request struct {
	Verb    string `json:"verb"`
	Key     string `json:"key,omitempty"`
	Text    string `json:"text,omitempty"`
	Feature string `json:"feature,omitempty"`
	Name    string `json:"name,omitempty"`
}

// Response is the single reply per request. OK=false carries Error.
type Response struct {
	OK     bool     `json:"ok"`
	Error  string   `json:"error,omitempty"`
	Title  string   `json:"title,omitempty"`
	Lines  []string `json:"lines,omitempty"`
	Text   string   `json:"text,omitempty"`
	Path   string   `json:"path,omitempty"`
	Pid    int      `json:"pid,omitempty"`
	Binary string   `json:"binary,omitempty"`
	Size   string   `json:"size,omitempty"`
	Demo   bool     `json:"demo,omitempty"`
}

// stateFile is the on-disk handshake between launch and the later clients.
type stateFile struct {
	Pid     int    `json:"pid"`
	Socket  string `json:"socket"`
	Binary  string `json:"binary"`
	Root    string `json:"root"`
	Demo    bool   `json:"demo"`
	Size    string `json:"size"`
	Started string `json:"started"`
}

// ServeOptions configures the daemon. Socket and StateDir default from the
// environment when empty.
type ServeOptions struct {
	Binary   string
	Root     string
	StateDir string
	Socket   string
	Cols     int
	Rows     int
	Demo     bool
}

func stateDir() string {
	if d := os.Getenv("OMAIRC_TUI_STATE"); d != "" {
		return d
	}
	user := os.Getenv("USER")
	if user == "" {
		user = os.Getenv("USERNAME")
	}
	if user == "" {
		user = "unknown"
	}
	return filepath.Join(os.TempDir(), "omairc-verify-tui-"+user)
}

func socketPath() string    { return filepath.Join(stateDir(), "sock") }
func stateFilePath() string { return filepath.Join(stateDir(), "state.json") }
func daemonLogPath() string { return filepath.Join(stateDir(), "daemon.log") }

// findRoot locates the repository root: the directory containing tui/go.mod
// and the feature map. OMAIRC_TUI_ROOT overrides it.
func findRoot() string {
	if r := os.Getenv("OMAIRC_TUI_ROOT"); r != "" {
		return r
	}
	starts := make([]string, 0, 2)
	if exe, err := os.Executable(); err == nil {
		starts = append(starts, filepath.Dir(exe))
	}
	if wd, err := os.Getwd(); err == nil {
		starts = append(starts, wd)
	}
	for _, start := range starts {
		for dir := start; ; {
			if isRepoRoot(dir) {
				return dir
			}
			parent := filepath.Dir(dir)
			if parent == dir {
				break
			}
			dir = parent
		}
	}
	if wd, err := os.Getwd(); err == nil {
		return wd
	}
	return "."
}

func isRepoRoot(dir string) bool {
	if _, err := os.Stat(filepath.Join(dir, "tui", "go.mod")); err != nil {
		return false
	}
	if _, err := os.Stat(filepath.Join(dir, ".cursor", "skills", "verify-omairc", "features")); err != nil {
		return false
	}
	return true
}

func defaultBinary(root string) string {
	return filepath.Join(root, "tui", "bin", "omairc-tui")
}

// client talks to the daemon over the Unix socket.
type client struct{ socket string }

func newClient() *client { return &client{socket: socketPath()} }

func (c *client) do(req Request) (Response, error) {
	return c.doWithin(req, 30*time.Second)
}

// doWithin is do with a caller-chosen request/response deadline, so teardown
// paths can give a wedged daemon a short leash instead of 30s.
func (c *client) doWithin(req Request, timeout time.Duration) (Response, error) {
	conn, err := net.DialTimeout("unix", c.socket, 2*time.Second)
	if err != nil {
		return Response{}, err
	}
	defer conn.Close()
	_ = conn.SetDeadline(time.Now().Add(timeout))
	if err := json.NewEncoder(conn).Encode(req); err != nil {
		return Response{}, err
	}
	var resp Response
	if err := json.NewDecoder(conn).Decode(&resp); err != nil {
		return Response{}, err
	}
	if !resp.OK {
		if resp.Error != "" {
			return resp, errors.New(resp.Error)
		}
		return resp, errors.New("daemon reported failure")
	}
	return resp, nil
}

func (c *client) healthy() bool {
	_, err := c.do(Request{Verb: "status"})
	return err == nil
}

// ---- daemon ----

type daemon struct {
	opts     ServeOptions
	proc     *Process
	screen   *Screen
	mu       sync.Mutex
	lastOut  time.Time
	listener net.Listener
	quitOnce sync.Once
	quitCh   chan struct{}
}

// Serve owns one PTY child and answers requests until `quit`. It blocks.
func Serve(opts ServeOptions) error {
	if opts.Binary == "" {
		return errors.New("serve requires a binary")
	}
	if opts.StateDir == "" {
		opts.StateDir = stateDir()
	}
	if opts.Socket == "" {
		opts.Socket = filepath.Join(opts.StateDir, "sock")
	}
	if opts.Cols <= 0 {
		opts.Cols = defaultCols
	}
	if opts.Rows <= 0 {
		opts.Rows = defaultRows
	}
	if err := os.MkdirAll(opts.StateDir, 0o755); err != nil {
		return err
	}
	// A leftover socket from a dead daemon would block Listen; only the
	// launching client removes stale state, so bind over it.
	_ = os.Remove(opts.Socket)
	listener, err := net.Listen("unix", opts.Socket)
	if err != nil {
		return fmt.Errorf("listen on %s: %w", opts.Socket, err)
	}

	d := &daemon{opts: opts, screen: NewScreen(opts.Cols, opts.Rows), listener: listener, quitCh: make(chan struct{}), lastOut: time.Now()}

	args := []string{}
	if opts.Demo {
		args = append(args, "--demo-server")
	}
	proc, err := Start(Options{
		Binary: opts.Binary,
		Args:   args,
		Cols:   opts.Cols,
		Rows:   opts.Rows,
		Dir:    opts.Root,
		Env:    terminalEnv(),
	})
	if err != nil {
		_ = listener.Close()
		_ = os.Remove(opts.Socket)
		return err
	}
	d.proc = proc
	proc.OnOutput(func(b []byte) {
		d.mu.Lock()
		d.screen.Feed(b)
		d.lastOut = time.Now()
		d.mu.Unlock()
	})
	// The daemon must not outlive its PTY child. Watch the child and stop the
	// accept loop the moment it exits, for any reason including a crash, so a
	// dead child never leaves a daemon (and its socket) wedging the next
	// launch. stop() is sync.Once-guarded, so this cannot race the `quit`
	// request.
	go func() {
		_ = proc.Wait()
		d.stop()
	}()

	st := stateFile{
		Pid:     os.Getpid(),
		Socket:  opts.Socket,
		Binary:  opts.Binary,
		Root:    opts.Root,
		Demo:    opts.Demo,
		Size:    fmt.Sprintf("%dx%d", opts.Cols, opts.Rows),
		Started: time.Now().Format(time.RFC3339),
	}
	if err := writeStateFile(st); err != nil {
		_ = proc.Close()
		_ = listener.Close()
		_ = os.Remove(opts.Socket)
		return err
	}
	d.logf("serving pid=%d binary=%s child=%d demo=%t size=%s", st.Pid, opts.Binary, proc.Pid(), opts.Demo, st.Size)

	d.serve()
	return nil
}

func (d *daemon) logf(format string, args ...any) {
	fmt.Fprintf(os.Stderr, "control-omairc-tui[serve]: "+format+"\n", args...)
}

func (d *daemon) serve() {
	for {
		conn, err := d.listener.Accept()
		if err != nil {
			break
		}
		d.handleConn(conn)
		select {
		case <-d.quitCh:
			d.shutdown()
			return
		default:
		}
	}
	d.shutdown()
}

// stop asks the daemon to shut down. It is safe to call more than once and
// from any goroutine: the first caller closes quitCh and the listener so the
// accept loop wakes even when the PTY child dies with no client connected.
func (d *daemon) stop() {
	d.quitOnce.Do(func() {
		close(d.quitCh)
		_ = d.listener.Close()
	})
}

func (d *daemon) handleConn(conn net.Conn) {
	defer conn.Close()
	_ = conn.SetDeadline(time.Now().Add(30 * time.Second))
	var req Request
	if err := json.NewDecoder(conn).Decode(&req); err != nil {
		return
	}
	resp := d.respond(req)
	_ = json.NewEncoder(conn).Encode(resp)
}

func (d *daemon) respond(req Request) Response {
	switch req.Verb {
	case "title":
		return Response{OK: true, Title: d.title()}
	case "text":
		d.mu.Lock()
		text, lines, title := d.screen.Text(), d.screen.Lines(), d.screen.Title()
		d.mu.Unlock()
		return Response{OK: true, Text: text, Lines: lines, Title: title}
	case "key":
		b, err := KeySequence(req.Key)
		if err != nil {
			return failure(err)
		}
		after := time.Now()
		if _, err := d.proc.Write(b); err != nil {
			return failure(err)
		}
		d.waitRedraw(after)
		return Response{OK: true}
	case "type":
		after := time.Now()
		if _, err := d.proc.Write([]byte(req.Text)); err != nil {
			return failure(err)
		}
		d.waitRedraw(after)
		return Response{OK: true}
	case "send":
		after := time.Now()
		if _, err := d.proc.Write(append([]byte(req.Text), '\r')); err != nil {
			return failure(err)
		}
		d.waitRedraw(after)
		return Response{OK: true}
	case "screenshot":
		return d.screenshot(req)
	case "status", "doctor":
		return d.status()
	case "quit":
		d.stop()
		return Response{OK: true}
	default:
		return failure(fmt.Errorf("unknown verb %q", req.Verb))
	}
}

func (d *daemon) title() string {
	d.mu.Lock()
	defer d.mu.Unlock()
	return d.screen.Title()
}

// screenQuiet is how long the PTY must be silent before the grid is considered
// settled; screenSettleMax bounds the wait for a screen that never redraws.
const (
	screenQuiet     = 80 * time.Millisecond
	screenSettleMax = 2 * time.Second
)

// waitRedraw blocks until the PTY has repainted after `after` and then stayed
// quiet for screenQuiet. A key/type/send request calls it so a following
// screenshot or compare sees the new frame instead of racing the redraw. A
// chord that produces no output returns after screenQuiet.
func (d *daemon) waitRedraw(after time.Time) {
	deadline := time.Now().Add(screenSettleMax)
	sawOutput := false
	for {
		d.mu.Lock()
		last := d.lastOut
		d.mu.Unlock()
		if last.After(after) {
			sawOutput = true
		}
		if sawOutput && time.Since(last) >= screenQuiet {
			return
		}
		if !sawOutput && time.Since(after) >= screenQuiet {
			return
		}
		if time.Now().After(deadline) {
			return
		}
		time.Sleep(10 * time.Millisecond)
	}
}

// waitQuiet blocks until the PTY has been silent for screenQuiet.
func (d *daemon) waitQuiet() {
	deadline := time.Now().Add(screenSettleMax)
	for {
		d.mu.Lock()
		last := d.lastOut
		d.mu.Unlock()
		if time.Since(last) >= screenQuiet {
			return
		}
		if time.Now().After(deadline) {
			return
		}
		time.Sleep(10 * time.Millisecond)
	}
}

func (d *daemon) status() Response {
	return Response{
		OK:     true,
		Title:  d.title(),
		Pid:    os.Getpid(),
		Binary: d.opts.Binary,
		Size:   fmt.Sprintf("%dx%d", d.opts.Cols, d.opts.Rows),
		Demo:   d.opts.Demo,
	}
}

func (d *daemon) screenshot(req Request) Response {
	feature := req.Feature
	if feature == "" {
		feature = defaultFeature
	}
	name := req.Name
	if name == "" {
		name = defaultShotName
	}
	if !validSegment(feature) {
		return failure(fmt.Errorf("invalid feature name %q", feature))
	}
	if !validSegment(name) {
		return failure(fmt.Errorf("invalid screenshot name %q", name))
	}
	d.waitQuiet()
	d.mu.Lock()
	pngBytes, err := d.screen.PNG()
	d.mu.Unlock()
	if err != nil {
		return failure(err)
	}
	path := filepath.Join(d.opts.Root, "test-artifacts", "verify-tui", feature, name+".png")
	if err := writeFileAtomic(path, pngBytes, 0o644); err != nil {
		return failure(err)
	}
	return Response{OK: true, Path: path}
}

func (d *daemon) shutdown() {
	if d.proc != nil {
		if b, err := KeySequence("ctrl+q"); err == nil && d.proc.Alive() {
			_, _ = d.proc.Write(b)
		}
		waitForExit(d.proc, daemonQuitGrace)
		if d.proc.Alive() {
			_ = d.proc.Terminate()
			waitForExit(d.proc, daemonQuitGrace)
		}
		if d.proc.Alive() {
			_ = d.proc.Kill()
		}
		_ = d.proc.Close()
	}
	_ = d.listener.Close()
	_ = os.Remove(d.opts.Socket)
	_ = os.Remove(filepath.Join(d.opts.StateDir, "state.json"))
	d.logf("daemon stopped")
}

func waitForExit(p *Process, d time.Duration) {
	deadline := time.Now().Add(d)
	for p.Alive() && time.Now().Before(deadline) {
		time.Sleep(50 * time.Millisecond)
	}
}

func failure(err error) Response { return Response{OK: false, Error: err.Error()} }

// ---- client verbs ----

// Run dispatches one control-omairc-tui invocation. It is the whole CLI; the
// command's main is a thin os.Exit(run(os.Args[1:])).
func Run(args []string, stdout, stderr io.Writer) int {
	if len(args) == 0 {
		fmt.Fprint(stdout, usage)
		return 0
	}
	verb, rest := args[0], args[1:]
	var err error
	switch verb {
	case "-h", "--help", "help":
		fmt.Fprint(stdout, usage)
		return 0
	case "--version", "version":
		fmt.Fprintf(stdout, "control-omairc-tui %s\n", version.Value)
		return 0
	case "launch":
		err = cmdLaunch(rest, stdout)
	case "serve":
		err = cmdServe(rest)
	case "doctor":
		err = cmdDoctor(rest, stdout)
	case "title":
		err = cmdTitle(rest, stdout)
	case "text":
		err = cmdText(rest, stdout)
	case "wait-title":
		err = cmdWaitTitle(rest, stdout)
	case "key":
		err = cmdKey(rest)
	case "type":
		err = cmdType(rest)
	case "send":
		err = cmdSend(rest)
	case "walk":
		err = cmdWalk(rest)
	case "unread":
		err = cmdUnread(rest)
	case "jump":
		err = cmdJump(rest)
	case "nick-jump":
		err = cmdNickJump(rest)
	case "status":
		err = cmdStatus(rest)
	case "connect":
		err = cmdConnect(rest)
	case "composer":
		err = cmdComposer(rest)
	case "compare":
		err = cmdCompare(rest, stdout)
	case "screenshot":
		err = cmdScreenshot(rest, stdout)
	case "run":
		err = cmdRun(rest, stdout, stderr)
	case "cleanup":
		err = cmdCleanup(rest, stdout)
	default:
		fmt.Fprintf(stderr, "control-omairc-tui: unknown verb %q\n", verb)
		fmt.Fprint(stderr, usage)
		return 2
	}
	if err != nil {
		fmt.Fprintf(stderr, "control-omairc-tui: %v\n", err)
		return 1
	}
	return 0
}

const usage = `usage: control-omairc-tui <verb> [options]

PTY driver for the compiled omairc-tui terminal client. It launches the
binary in a virtual terminal, reconstructs the screen and OSC title, and
replays the same desktop-recipe fences as control-omairc (minus mouse verbs).

Verbs:
  launch [--demo-server] [--size 118x30] [--binary PATH]
                                   start a detached daemon over a PTY
  doctor                           report daemon and child status
  title                            print the current OSC window title
  text                             print the reconstructed grid text
  wait-title --exact TEXT [--timeout 10s]
                                   block until the title matches exactly
  key --key NAME                   send one chord (e.g. ctrl+q, alt+Down)
  type --text TEXT                 send raw text, no Enter
  send --text TEXT                 send text then Enter
  walk --down|--up [--times N]     Alt+Down / Alt+Up N times
  unread                           Alt+A (next unread)
  jump --query TEXT                Ctrl+K, type the query, then Enter
  nick-jump --query TEXT           Ctrl+Shift+K, type the query, then Enter
  status                           Ctrl+backtick (toggle the Status console)
  connect                          Ctrl+, (open the Connect sheet)
  composer                         focus the composer (no-op; keeps shared
                                   recipes runnable)
  compare --before PATH --after PATH
                                   assert two PNGs differ
  screenshot [--feature NAME] [--name NAME]
                                   write a PNG of the grid under
                                   test-artifacts/verify-tui/<feature>/
  run <feature> [--dry-run]        replay the desktop-recipe fence
  cleanup                          stop the daemon and remove its state
  serve ...                        internal daemon entry point (launch uses it)

Global:
  --help                           print this help
  --version                        print the driver version
`

func cmdServe(args []string) error {
	opts := ServeOptions{}
	size := ""
	for i := 0; i < len(args); i++ {
		switch args[i] {
		case "--binary":
			if i+1 >= len(args) {
				return errors.New("serve --binary needs a path")
			}
			i++
			opts.Binary = args[i]
		case "--root":
			if i+1 >= len(args) {
				return errors.New("serve --root needs a path")
			}
			i++
			opts.Root = args[i]
		case "--state":
			if i+1 >= len(args) {
				return errors.New("serve --state needs a directory")
			}
			i++
			opts.StateDir = args[i]
		case "--socket":
			if i+1 >= len(args) {
				return errors.New("serve --socket needs a path")
			}
			i++
			opts.Socket = args[i]
		case "--size":
			if i+1 >= len(args) {
				return errors.New("serve --size needs WxH")
			}
			i++
			size = args[i]
		case "--demo-server":
			opts.Demo = true
		default:
			return fmt.Errorf("unknown serve argument %q", args[i])
		}
	}
	if size != "" {
		cols, rows, err := parseSize(size)
		if err != nil {
			return err
		}
		opts.Cols, opts.Rows = cols, rows
	}
	return Serve(opts)
}

func cmdLaunch(args []string, stdout io.Writer) error {
	demo := false
	size := fmt.Sprintf("%dx%d", defaultCols, defaultRows)
	binary := ""
	for i := 0; i < len(args); i++ {
		switch args[i] {
		case "--demo-server":
			demo = true
		case "--size":
			if i+1 >= len(args) {
				return errors.New("launch --size needs WxH")
			}
			i++
			size = args[i]
		case "--binary":
			if i+1 >= len(args) {
				return errors.New("launch --binary needs a path")
			}
			i++
			binary = args[i]
		default:
			return fmt.Errorf("unknown launch argument %q", args[i])
		}
	}
	if _, _, err := parseSize(size); err != nil {
		return err
	}

	c := newClient()
	if c.healthy() {
		st, _ := readStateFile()
		return fmt.Errorf("a daemon is already running (pid %d demo=%s); run cleanup first", st.Pid, yesno(st.Demo))
	}
	// Stale state from a dead daemon: clear it before starting a new one.
	removeStateFiles()

	root := findRoot()
	if binary == "" {
		binary = defaultBinary(root)
	}
	if _, err := os.Stat(binary); err != nil {
		return fmt.Errorf("missing omairc-tui binary %s (build it with tui/bin/build)", binary)
	}
	dir := stateDir()
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}
	logFile, err := os.OpenFile(daemonLogPath(), os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0o644)
	if err != nil {
		return err
	}
	defer logFile.Close()

	self, err := os.Executable()
	if err != nil {
		return err
	}
	serveArgs := []string{
		"serve",
		"--binary", binary,
		"--root", root,
		"--state", dir,
		"--socket", socketPath(),
		"--size", size,
	}
	if demo {
		serveArgs = append(serveArgs, "--demo-server")
	}
	cmd := exec.Command(self, serveArgs...)
	cmd.Dir = root
	cmd.Stdin = nil
	cmd.Stdout = logFile
	cmd.Stderr = logFile
	detachProcess(cmd)
	if err := cmd.Start(); err != nil {
		return fmt.Errorf("start daemon: %w", err)
	}
	daemonPid := cmd.Process.Pid
	_ = cmd.Process.Release()

	deadline := time.Now().Add(launchTimeout)
	for {
		if resp, err := c.doWithin(Request{Verb: "status"}, time.Second); err == nil && resp.Title != "" {
			fmt.Fprintf(stdout, "ok launched pid=%d binary=%s size=%s demo=%s\n",
				resp.Pid, resp.Binary, resp.Size, yesno(resp.Demo))
			return nil
		}
		// The daemon exited (bad binary, bind failure) instead of serving a
		// title: tear down what it left behind instead of waiting out the full
		// timeout with stale state on disk.
		if !pidAlive(daemonPid) {
			stopSpawnedDaemon(daemonPid)
			return fmt.Errorf("daemon exited before reporting a window title; see %s\n%s",
				daemonLogPath(), tailOfFile(daemonLogPath(), 20))
		}
		if time.Now().After(deadline) {
			stopSpawnedDaemon(daemonPid)
			return fmt.Errorf("daemon did not report a window title within %s; see %s\n%s",
				launchTimeout, daemonLogPath(), tailOfFile(daemonLogPath(), 20))
		}
		time.Sleep(100 * time.Millisecond)
	}
}

func cmdDoctor(args []string, stdout io.Writer) error {
	if len(args) != 0 {
		return errors.New("doctor takes no arguments")
	}
	resp, err := newClient().do(Request{Verb: "status"})
	if err != nil {
		return fmt.Errorf("no running omairc-tui daemon: %w", err)
	}
	fmt.Fprintln(stdout, "ok omairc-tui")
	fmt.Fprintf(stdout, "binary=%s\n", resp.Binary)
	fmt.Fprintf(stdout, "pid=%d\n", resp.Pid)
	fmt.Fprintf(stdout, "title=%s\n", resp.Title)
	fmt.Fprintf(stdout, "size=%s\n", resp.Size)
	fmt.Fprintf(stdout, "demo=%s\n", yesno(resp.Demo))
	return nil
}

func cmdTitle(args []string, stdout io.Writer) error {
	if len(args) != 0 {
		return errors.New("title takes no arguments")
	}
	resp, err := newClient().do(Request{Verb: "title"})
	if err != nil {
		return err
	}
	fmt.Fprintln(stdout, resp.Title)
	return nil
}

// cmdText prints the reconstructed grid text (the daemon's `text` request), so
// a recipe or gate script can assert the rows actually on screen.
func cmdText(args []string, stdout io.Writer) error {
	if len(args) != 0 {
		return errors.New("text takes no arguments")
	}
	resp, err := newClient().do(Request{Verb: "text"})
	if err != nil {
		return err
	}
	fmt.Fprintln(stdout, resp.Text)
	return nil
}

func cmdWaitTitle(args []string, stdout io.Writer) error {
	exact := ""
	timeout := defaultWaitTimeout
	for i := 0; i < len(args); i++ {
		switch args[i] {
		case "--exact":
			if i+1 >= len(args) {
				return errors.New("wait-title --exact needs text")
			}
			i++
			exact = args[i]
		case "--timeout":
			if i+1 >= len(args) {
				return errors.New("wait-title --timeout needs a duration")
			}
			i++
			d, err := time.ParseDuration(args[i])
			if err != nil {
				return fmt.Errorf("wait-title --timeout: %w", err)
			}
			timeout = d
		default:
			return fmt.Errorf("unknown wait-title argument %q", args[i])
		}
	}
	if exact == "" {
		return errors.New("wait-title requires --exact")
	}
	c := newClient()
	deadline := time.Now().Add(timeout)
	observed := ""
	var lastErr error
	for {
		resp, err := c.do(Request{Verb: "title"})
		if err == nil {
			observed = resp.Title
			if observed == exact {
				fmt.Fprintln(stdout, observed)
				return nil
			}
		} else {
			lastErr = err
		}
		if time.Now().After(deadline) {
			if lastErr != nil && observed == "" {
				return fmt.Errorf("expected title %q after %s: %w", exact, timeout, lastErr)
			}
			return fmt.Errorf("expected title %q, got %q after %s", exact, observed, timeout)
		}
		time.Sleep(100 * time.Millisecond)
	}
}

func cmdKey(args []string) error {
	name := ""
	for i := 0; i < len(args); i++ {
		switch args[i] {
		case "--key":
			if i+1 >= len(args) {
				return errors.New("key --key needs a chord")
			}
			i++
			name = args[i]
		default:
			return fmt.Errorf("unknown key argument %q", args[i])
		}
	}
	if name == "" {
		return errors.New("key requires --key")
	}
	if _, err := KeySequence(name); err != nil {
		return err
	}
	_, err := newClient().do(Request{Verb: "key", Key: name})
	return err
}

// textArg extracts a required --text value.
func textArg(verb string, args []string) (string, error) {
	text := ""
	for i := 0; i < len(args); i++ {
		switch args[i] {
		case "--text":
			if i+1 >= len(args) {
				return "", fmt.Errorf("%s --text needs text", verb)
			}
			i++
			text = args[i]
		default:
			return "", fmt.Errorf("unknown %s argument %q", verb, args[i])
		}
	}
	if text == "" {
		return "", fmt.Errorf("%s requires --text", verb)
	}
	return text, nil
}

func cmdType(args []string) error {
	text, err := textArg("type", args)
	if err != nil {
		return err
	}
	_, err = newClient().do(Request{Verb: "type", Text: text})
	return err
}

func cmdSend(args []string) error {
	text, err := textArg("send", args)
	if err != nil {
		return err
	}
	_, err = newClient().do(Request{Verb: "send", Text: text})
	return err
}

func cmdWalk(args []string) error {
	direction := ""
	times := 1
	for i := 0; i < len(args); i++ {
		switch args[i] {
		case "--down", "--up":
			if direction != "" {
				return errors.New("walk accepts only one of --down or --up")
			}
			direction = strings.TrimPrefix(args[i], "--")
		case "--times":
			if i+1 >= len(args) {
				return errors.New("walk --times needs a count")
			}
			i++
			n, err := strconv.Atoi(args[i])
			if err != nil || n <= 0 {
				return errors.New("walk --times needs a positive integer")
			}
			times = n
		default:
			return fmt.Errorf("unknown walk argument %q", args[i])
		}
	}
	if direction == "" {
		return errors.New("walk requires --down or --up")
	}
	key := "alt+Down"
	if direction == "up" {
		key = "alt+Up"
	}
	c := newClient()
	for i := 0; i < times; i++ {
		if _, err := c.do(Request{Verb: "key", Key: key}); err != nil {
			return err
		}
	}
	return nil
}

func cmdUnread(args []string) error {
	if len(args) != 0 {
		return errors.New("unread takes no arguments")
	}
	_, err := newClient().do(Request{Verb: "key", Key: "alt+a"})
	return err
}

// cmdJump drives the Ctrl+K jump overlay: open it, type the query, then Enter.
func cmdJump(args []string) error {
	query := ""
	for i := 0; i < len(args); i++ {
		switch args[i] {
		case "--query":
			if i+1 >= len(args) {
				return errors.New("jump --query needs text")
			}
			i++
			query = args[i]
		default:
			return fmt.Errorf("unknown jump argument %q", args[i])
		}
	}
	if query == "" {
		return errors.New("jump requires --query")
	}
	c := newClient()
	if _, err := c.do(Request{Verb: "key", Key: "ctrl+k"}); err != nil {
		return err
	}
	if _, err := c.do(Request{Verb: "type", Text: query}); err != nil {
		return err
	}
	_, err := c.do(Request{Verb: "key", Key: "return"})
	return err
}

// cmdNickJump drives the Ctrl+Shift+K nick-jump overlay: open it, type the
// query, then Enter.
func cmdNickJump(args []string) error {
	query := ""
	for i := 0; i < len(args); i++ {
		switch args[i] {
		case "--query":
			if i+1 >= len(args) {
				return errors.New("nick-jump --query needs text")
			}
			i++
			query = args[i]
		default:
			return fmt.Errorf("unknown nick-jump argument %q", args[i])
		}
	}
	if query == "" {
		return errors.New("nick-jump requires --query")
	}
	c := newClient()
	if _, err := c.do(Request{Verb: "key", Key: "ctrl+shift+k"}); err != nil {
		return err
	}
	if _, err := c.do(Request{Verb: "type", Text: query}); err != nil {
		return err
	}
	_, err := c.do(Request{Verb: "key", Key: "return"})
	return err
}

// cmdStatus toggles the Status console with Ctrl+`.
func cmdStatus(args []string) error {
	if len(args) != 0 {
		return errors.New("status takes no arguments")
	}
	_, err := newClient().do(Request{Verb: "key", Key: "ctrl+`"})
	return err
}

// cmdConnect opens the Connect sheet with Ctrl+,.
func cmdConnect(args []string) error {
	if len(args) != 0 {
		return errors.New("connect takes no arguments")
	}
	_, err := newClient().do(Request{Verb: "key", Key: "ctrl+,"})
	return err
}

// cmdComposer focuses the composer. The TUI composer owns focus unless a modal
// is open, so this is a no-op that keeps the shared desktop recipes runnable
// (the Qt driver's `composer` clicks the text field).
func cmdComposer(args []string) error {
	if len(args) != 0 {
		return errors.New("composer takes no arguments")
	}
	_, err := newClient().do(Request{Verb: "status"})
	return err
}

func cmdScreenshot(args []string, stdout io.Writer) error {
	feature := defaultFeature
	name := defaultShotName
	for i := 0; i < len(args); i++ {
		switch args[i] {
		case "--feature":
			if i+1 >= len(args) {
				return errors.New("screenshot --feature needs a name")
			}
			i++
			feature = args[i]
		case "--name":
			if i+1 >= len(args) {
				return errors.New("screenshot --name needs a name")
			}
			i++
			name = args[i]
		default:
			return fmt.Errorf("unknown screenshot argument %q", args[i])
		}
	}
	resp, err := newClient().do(Request{Verb: "screenshot", Feature: feature, Name: name})
	if err != nil {
		return err
	}
	fmt.Fprintln(stdout, resp.Path)
	return nil
}

// qtVerifyRoot is the evidence root the shared Qt recipes write to. The PTY
// driver keeps its screenshots under tuiVerifyRoot instead, so a compare on a
// Qt fence path is remapped before the files are opened. The trailing slash
// keeps it from matching the "verify-tui" root.
const (
	qtVerifyRoot  = "test-artifacts/verify/"
	tuiVerifyRoot = "test-artifacts/verify-tui/"
)

func remapVerifyPath(path string) string {
	if rest, ok := strings.CutPrefix(path, qtVerifyRoot); ok {
		return tuiVerifyRoot + rest
	}
	return path
}

// cmdCompare asserts two PNG screenshots differ. It is client-side only: it
// never touches the daemon, so it can run after cleanup.
func cmdCompare(args []string, stdout io.Writer) error {
	before := ""
	after := ""
	for i := 0; i < len(args); i++ {
		switch args[i] {
		case "--before":
			if i+1 >= len(args) {
				return errors.New("compare --before needs a path")
			}
			i++
			before = args[i]
		case "--after":
			if i+1 >= len(args) {
				return errors.New("compare --after needs a path")
			}
			i++
			after = args[i]
		default:
			return fmt.Errorf("unknown compare argument %q", args[i])
		}
	}
	if before == "" {
		return errors.New("compare requires --before")
	}
	if after == "" {
		return errors.New("compare requires --after")
	}
	before = remapVerifyPath(before)
	after = remapVerifyPath(after)

	beforeImage, err := readPNG(before)
	if err != nil {
		return err
	}
	afterImage, err := readPNG(after)
	if err != nil {
		return err
	}
	beforeBounds, afterBounds := beforeImage.Bounds(), afterImage.Bounds()
	if beforeBounds.Dx() != afterBounds.Dx() || beforeBounds.Dy() != afterBounds.Dy() {
		return fmt.Errorf("image sizes differ: %s is %dx%d, %s is %dx%d",
			before, beforeBounds.Dx(), beforeBounds.Dy(), after, afterBounds.Dx(), afterBounds.Dy())
	}
	if samePixels(beforeImage, afterImage) {
		return fmt.Errorf("images are identical: %s and %s", before, after)
	}
	fmt.Fprintf(stdout, "images differ (%dx%d)\n", beforeBounds.Dx(), beforeBounds.Dy())
	return nil
}

func readPNG(path string) (image.Image, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	decoded, err := png.Decode(bytes.NewReader(data))
	if err != nil {
		return nil, fmt.Errorf("decode %s: %w", path, err)
	}
	return decoded, nil
}

// samePixels compares two same-sized images pixel by pixel. color.Color.RGBA
// yields 16-bit premultiplied channels, so the comparison does not depend on
// the decoded color model.
func samePixels(left, right image.Image) bool {
	bounds := left.Bounds()
	for y := 0; y < bounds.Dy(); y++ {
		for x := 0; x < bounds.Dx(); x++ {
			lr, lg, lb, la := left.At(bounds.Min.X+x, bounds.Min.Y+y).RGBA()
			rr, rg, rb, ra := right.At(bounds.Min.X+x, bounds.Min.Y+y).RGBA()
			if lr != rr || lg != rg || lb != rb || la != ra {
				return false
			}
		}
	}
	return true
}

func cmdCleanup(args []string, stdout io.Writer) error {
	if len(args) != 0 {
		return errors.New("cleanup takes no arguments")
	}
	// stopDaemon waits for the daemon process to exit, so its shutdown-time
	// socket and state-file removal finishes before a following launch creates
	// fresh ones.
	stopDaemon(0)
	removeStateFiles()
	fmt.Fprintln(stdout, "cleaned tui driver state")
	return nil
}

// pidAlive reports whether pid names a live process the caller may signal. It
// returns false for a non-positive pid. On Linux it reads /proc/<pid>/stat and
// treats a zombie as dead: a child this process spawned and released lingers as
// a zombie, and a signal-0 probe reports that zombie as alive, which would make
// a teardown wait spin until the parent exits. Elsewhere it falls back to the
// signal-0 existence probe (unsupported on the unsupported Windows PTY target,
// where it reports false).
func pidAlive(pid int) bool {
	if pid <= 0 {
		return false
	}
	if _, err := os.Stat("/proc"); err == nil {
		data, err := os.ReadFile(fmt.Sprintf("/proc/%d/stat", pid))
		if err != nil {
			return false
		}
		// "pid (comm) state ...": comm may contain spaces and parens, so read
		// the state field after the last ')'.
		if i := strings.LastIndexByte(string(data), ')'); i >= 0 && i+2 < len(data) {
			return data[i+2] != 'Z'
		}
		return false
	}
	p, err := os.FindProcess(pid)
	if err != nil {
		return false
	}
	return p.Signal(syscall.Signal(0)) == nil
}

func killPid(pid int) {
	if p, err := os.FindProcess(pid); err == nil {
		_ = p.Kill()
	}
}

func waitForPidExit(pid int, d time.Duration) {
	if pid <= 0 {
		return
	}
	deadline := time.Now().Add(d)
	for pidAlive(pid) && time.Now().Before(deadline) {
		time.Sleep(50 * time.Millisecond)
	}
}

// stopDaemon asks the running daemon to quit, then waits for the process to
// exit, which is when its shutdown-time socket and state-file removal is
// complete. Waiting matters: stop() closes the listener as soon as quit
// arrives, so a client that only watched the socket would start a new daemon
// while the old one was still removing those same paths. It kills the pid if it
// outlives the grace period. pidHint is a process this command spawned; a
// recorded state.json pid wins when one exists. When no daemon answers it only
// kills pidHint, never a stale state.json pid that may have been reused.
func stopDaemon(pidHint int) {
	c := newClient()
	st, _ := readStateFile()
	pid := st.Pid
	if pid <= 0 {
		pid = pidHint
	}
	if _, err := c.doWithin(Request{Verb: "quit"}, daemonQuitGrace); err != nil {
		if pidHint > 0 && pidAlive(pidHint) {
			killPid(pidHint)
		}
		return
	}
	waitForPidExit(pid, daemonStopTimeout)
	if pidAlive(pid) {
		killPid(pid)
	}
}

// stopSpawnedDaemon best-effort tears down a daemon this command started, so a
// failed launch or a failed run does not wedge the next one, and clears its
// state files. It never touches a daemon the caller did not launch, so callers
// must only invoke it for a daemon they started.
func stopSpawnedDaemon(pidHint int) {
	stopDaemon(pidHint)
	removeStateFiles()
}

// ---- recipe replay ----

func cmdRun(args []string, stdout, stderr io.Writer) error {
	feature := ""
	dry := false
	for _, a := range args {
		switch {
		case a == "--dry-run":
			dry = true
		case strings.HasPrefix(a, "-"):
			return fmt.Errorf("unknown run argument %q", a)
		case feature != "":
			return fmt.Errorf("unexpected run argument %q", a)
		default:
			feature = a
		}
	}
	if feature == "" {
		return errors.New("run requires a feature id")
	}
	if !validSegment(feature) {
		return fmt.Errorf("invalid feature id %q", feature)
	}
	path := filepath.Join(findRoot(), ".cursor", "skills", "verify-omairc", "features", feature+".md")
	data, err := os.ReadFile(path)
	if err != nil {
		return fmt.Errorf("no feature file for %q at %s", feature, path)
	}
	commands, err := extractDesktopRecipe(string(data))
	if err != nil {
		return err
	}
	if len(commands) == 0 {
		return fmt.Errorf("desktop-recipe fence is empty in %s", path)
	}
	if dry {
		for _, line := range commands {
			fmt.Fprintln(stdout, line)
		}
		return nil
	}

	parsed := make([][]string, 0, len(commands))
	for _, line := range commands {
		tokens, err := splitRecipeLine(line)
		if err != nil {
			return err
		}
		if len(tokens) == 0 {
			continue
		}
		// The gate is keyboard-first: reject input the PTY cannot deliver
		// before any command runs, naming the offending verb.
		if err := rejectRecipeVerb(tokens[0]); err != nil {
			return fmt.Errorf("%s recipe in %s: %w", feature, path, err)
		}
		parsed = append(parsed, tokens)
	}

	// Track whether this run launched the daemon. A run that skipped `launch`
	// because a matching healthy daemon already existed must not tear that
	// daemon down when a later step fails; only a daemon this run started is
	// ours to stop.
	launchedByRun := false
	for i, tokens := range parsed {
		if err := runRecipeLine(tokens, stdout, stderr, &launchedByRun); err != nil {
			if launchedByRun {
				stopSpawnedDaemon(0)
			}
			return fmt.Errorf("%s recipe step %d (%s) in %s: %w",
				feature, i+1, strings.Join(tokens, " "), path, err)
		}
	}
	return nil
}

func runRecipeLine(tokens []string, stdout, stderr io.Writer, launchedByRun *bool) error {
	switch tokens[0] {
	case "launch":
		return runRecipeLaunch(tokens[1:], stdout, stderr, launchedByRun)
	case "doctor":
		return cmdDoctor(tokens[1:], stdout)
	case "title":
		return cmdTitle(tokens[1:], stdout)
	case "wait-title":
		return cmdWaitTitle(tokens[1:], stdout)
	case "key":
		return cmdKey(tokens[1:])
	case "type":
		return cmdType(tokens[1:])
	case "send":
		return cmdSend(tokens[1:])
	case "walk":
		return cmdWalk(tokens[1:])
	case "unread":
		return cmdUnread(tokens[1:])
	case "jump":
		return cmdJump(tokens[1:])
	case "nick-jump":
		return cmdNickJump(tokens[1:])
	case "status":
		return cmdStatus(tokens[1:])
	case "connect":
		return cmdConnect(tokens[1:])
	case "composer":
		return cmdComposer(tokens[1:])
	case "compare":
		return cmdCompare(tokens[1:], stdout)
	case "screenshot":
		return cmdScreenshot(tokens[1:], stdout)
	case "cleanup":
		return cmdCleanup(tokens[1:], stdout)
	default:
		return fmt.Errorf("unsupported verb %q for the TUI driver", tokens[0])
	}
}

// runRecipeLaunch reuses a healthy daemon whose demo mode matches, and refuses
// to replace one that does not. Everything else is a normal launch. It sets
// *launchedByRun only when it actually started a daemon, so the caller knows
// what it owns.
func runRecipeLaunch(args []string, stdout, stderr io.Writer, launchedByRun *bool) error {
	demo := false
	for _, a := range args {
		if a == "--demo-server" {
			demo = true
		}
	}
	if resp, err := newClient().do(Request{Verb: "status"}); err == nil {
		if resp.Demo != demo {
			return fmt.Errorf("a daemon is already running with demo=%s; refusing to replace it", yesno(resp.Demo))
		}
		fmt.Fprintf(stderr, "launch skipped; daemon already running with demo=%s\n", yesno(demo))
		return nil
	}
	if err := cmdLaunch(args, stdout); err != nil {
		return err
	}
	if launchedByRun != nil {
		*launchedByRun = true
	}
	return nil
}

// rejectRecipeVerb names the verbs the TUI driver cannot run. The Qt driver
// is pixel-based; this one is keyboard-only.
func rejectRecipeVerb(verb string) error {
	if strings.HasPrefix(verb, "click-") {
		return fmt.Errorf("recipe verb %q is mouse input; the TUI driver is keyboard-first", verb)
	}
	switch verb {
	case "qml-suite", "doctor-qml":
		return fmt.Errorf("recipe verb %q is the offscreen QML suite, not compiled-window proof", verb)
	}
	return nil
}

// extractDesktopRecipe returns the trimmed command lines of the
// ```desktop-recipe fence inside "## Driving it with control-omairc", stopping
// at the next "## " heading. Blank lines and # comments are dropped. It errors
// when the section, the fence, or the closing fence is missing.
func extractDesktopRecipe(md string) ([]string, error) {
	inSection := false
	inFence := false
	found := false
	closed := false
	var fence []string
	for _, raw := range strings.Split(md, "\n") {
		trimmed := strings.TrimSpace(strings.TrimRight(raw, "\r"))
		if inFence {
			if trimmed == "```" {
				inFence = false
				closed = true
				continue
			}
			fence = append(fence, trimmed)
			continue
		}
		if trimmed == "## Driving it with control-omairc" {
			inSection = true
			continue
		}
		if strings.HasPrefix(trimmed, "## ") {
			inSection = false
			continue
		}
		if inSection && trimmed == "```desktop-recipe" {
			inFence = true
			found = true
		}
	}
	if !found || !closed {
		return nil, errors.New("no desktop-recipe fence under ## Driving it with control-omairc, before ## Gotchas")
	}
	commands := make([]string, 0, len(fence))
	for _, line := range fence {
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		commands = append(commands, line)
	}
	return commands, nil
}

// splitRecipeLine tokenizes one recipe line the way a POSIX shell would for
// the words that appear in the fence: single and double quotes group words,
// an unquoted # at the start of a word begins a comment, and an unquoted #
// inside a word is literal ("#omarchy" stays one token).
func splitRecipeLine(line string) ([]string, error) {
	var tokens []string
	var cur strings.Builder
	inToken := false
	var quote byte
	for i := 0; i < len(line); i++ {
		ch := line[i]
		switch quote {
		case '\'':
			if ch == '\'' {
				quote = 0
			} else {
				cur.WriteByte(ch)
			}
			inToken = true
		case '"':
			if ch == '"' {
				quote = 0
			} else {
				cur.WriteByte(ch)
			}
			inToken = true
		default:
			switch {
			case ch == ' ' || ch == '\t':
				if inToken {
					tokens = append(tokens, cur.String())
					cur.Reset()
					inToken = false
				}
			case ch == '\'':
				quote = '\''
				inToken = true
			case ch == '"':
				quote = '"'
				inToken = true
			case ch == '#':
				if !inToken {
					// Comment: stop parsing the line.
					i = len(line)
					continue
				}
				cur.WriteByte(ch)
				inToken = true
			default:
				cur.WriteByte(ch)
				inToken = true
			}
		}
	}
	if quote != 0 {
		return nil, fmt.Errorf("unbalanced quotes in recipe line %q", line)
	}
	if inToken {
		tokens = append(tokens, cur.String())
	}
	return tokens, nil
}

// ---- small helpers ----

var segmentRE = regexp.MustCompile(`^[A-Za-z0-9._-]+$`)

func validSegment(s string) bool {
	if s == "" || s == "." || s == ".." {
		return false
	}
	return segmentRE.MatchString(s)
}

func yesno(b bool) string {
	if b {
		return "yes"
	}
	return "no"
}

func parseSize(s string) (int, int, error) {
	parts := strings.Split(strings.ToLower(s), "x")
	if len(parts) != 2 {
		return 0, 0, fmt.Errorf("size %q must be WxH", s)
	}
	cols, err := strconv.Atoi(strings.TrimSpace(parts[0]))
	if err != nil {
		return 0, 0, fmt.Errorf("size %q must be WxH", s)
	}
	rows, err := strconv.Atoi(strings.TrimSpace(parts[1]))
	if err != nil {
		return 0, 0, fmt.Errorf("size %q must be WxH", s)
	}
	if cols <= 0 || rows <= 0 {
		return 0, 0, fmt.Errorf("size %q must be positive", s)
	}
	return cols, rows, nil
}

func readStateFile() (stateFile, error) {
	var st stateFile
	data, err := os.ReadFile(stateFilePath())
	if err != nil {
		return st, err
	}
	if err := json.Unmarshal(data, &st); err != nil {
		return st, err
	}
	return st, nil
}

func writeStateFile(st stateFile) error {
	data, err := json.MarshalIndent(st, "", "  ")
	if err != nil {
		return err
	}
	data = append(data, '\n')
	return writeFileAtomic(stateFilePath(), data, 0o644)
}

func removeStateFiles() {
	_ = os.Remove(socketPath())
	_ = os.Remove(stateFilePath())
}

// writeFileAtomic writes through a temp file in the destination directory and
// renames, so a reader never sees a half-written screenshot or state file.
func writeFileAtomic(path string, data []byte, perm os.FileMode) error {
	dir := filepath.Dir(path)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}
	f, err := os.CreateTemp(dir, ".omairc-tui-tmp-*")
	if err != nil {
		return err
	}
	tmp := f.Name()
	if _, err := f.Write(data); err != nil {
		_ = f.Close()
		_ = os.Remove(tmp)
		return err
	}
	if err := f.Chmod(perm); err != nil {
		_ = f.Close()
		_ = os.Remove(tmp)
		return err
	}
	if err := f.Close(); err != nil {
		_ = os.Remove(tmp)
		return err
	}
	if err := os.Rename(tmp, path); err != nil {
		_ = os.Remove(tmp)
		return err
	}
	return nil
}

func tailOfFile(path string, n int) string {
	data, err := os.ReadFile(path)
	if err != nil {
		return ""
	}
	lines := strings.Split(strings.TrimRight(string(data), "\n"), "\n")
	if len(lines) > n {
		lines = lines[len(lines)-n:]
	}
	return strings.Join(lines, "\n")
}
