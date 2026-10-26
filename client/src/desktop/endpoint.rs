//! The local endpoint the product CLI and the single-instance guard share: the
//! [`crate::identity::APP_NAME`] named pipe on Windows, a Unix socket under the runtime
//! directory elsewhere.
//! (`serverName`, `runtimeDir`), with Qt's `QLocalServer` and `QLocalSocket` as the
//! reference behaviour.
//! The Unix side is std only. The Windows side uses the few kernel32 calls std does not wrap, through
//! `windows-sys`, pinned to the 0.59.0 the lock file already holds, so no new crate is fetched.
//! Both sides give reads a deadline. The server uses it to expire an idle client, and the CLI uses it
//! to stop waiting for a reply. Windows named pipes have no read timeout, so the Windows stream polls
//! `PeekNamedPipe` and sleeps between polls.

use std::io;
use std::time::Duration;

#[cfg(unix)]
use std::path::PathBuf;

/// The name the window listens on: the pipe leaf on Windows, the socket file stem elsewhere.
pub use crate::identity::APP_NAME;

#[cfg(windows)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pipe: String,
}

/// Where one window listens, and the lock file its primary holds.
#[cfg(unix)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    socket: PathBuf,
    lock: PathBuf,
}

impl Endpoint {
    /// The window's own endpoint.
    pub fn app() -> Self {
        Self::named(APP_NAME)
    }

    /// The endpoint called `name`. Tests use unique names so they never meet a running window.
    #[cfg(windows)]
    pub fn named(name: &str) -> Self {
        Self {
            pipe: format!(r"\\.\pipe\{name}"),
        }
    }

    /// The endpoint called `name` under the runtime directory.
    #[cfg(unix)]
    pub fn named(name: &str) -> Self {
        let dir = runtime_dir();
        Self {
            socket: dir.join(format!("{name}.sock")),
            lock: dir.join(format!("{name}.lock")),
        }
    }

    /// The lock file the primary holds while it runs.
    #[cfg(unix)]
    pub fn lock_path(&self) -> &std::path::Path {
        &self.lock
    }
}

/// `$XDG_RUNTIME_DIR` when set, else the temp directory, as Qt's `RuntimeLocation` falls back.
#[cfg(unix)]
fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

/// Whether `error` says another window already owns the endpoint. Windows only: Unix claims the
/// endpoint through its lock file instead.
#[cfg(windows)]
pub fn is_taken(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::AddrInUse
}

/// Connects to the window at `endpoint`. Fails with `NotFound` or `ConnectionRefused` when none is
/// listening, and with `TimedOut` when a busy pipe does not free up within `timeout`.
pub fn connect(endpoint: &Endpoint, timeout: Duration) -> io::Result<Stream> {
    imp::connect(endpoint, timeout)
}

#[cfg(unix)]
mod imp {
    use std::io::{self, Read, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::time::{Duration, Instant};

    use super::Endpoint;

    /// One connection to or from the window.
    pub struct Stream(UnixStream);

    impl Stream {
        /// Reads give up with `TimedOut` once `deadline` passes. `None` waits.
        pub fn set_read_deadline(&mut self, deadline: Option<Instant>) {
            let timeout = deadline.map(|at| at.saturating_duration_since(Instant::now()).max(Duration::from_millis(1)));
            let _ = self.0.set_read_timeout(timeout);
        }

        /// Sends what is buffered and closes the connection.
        pub fn finish(mut self) -> io::Result<()> {
            self.0.flush()
        }
    }

    impl Read for Stream {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            match self.0.read(buf) {
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    Err(io::Error::new(io::ErrorKind::TimedOut, "no reply from the window"))
                }
                other => other,
            }
        }
    }

    impl Write for Stream {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.write(buf)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.0.flush()
        }
    }

    /// The listening socket. Only the holder of the endpoint's lock file may bind it.
    pub struct Listener(UnixListener);

    impl Listener {
        /// Binds the endpoint. A socket file left by a window that exited is stale, because the
        /// caller holds the lock, so it is removed first.
        pub fn bind(endpoint: &Endpoint) -> io::Result<Self> {
            let _ = std::fs::remove_file(&endpoint.socket);
            let listener = UnixListener::bind(&endpoint.socket)?;
            let _ = std::fs::set_permissions(&endpoint.socket, std::fs::Permissions::from_mode(0o600));
            Ok(Self(listener))
        }

        pub fn accept(&mut self) -> io::Result<Stream> {
            self.0.accept().map(|(stream, _)| Stream(stream))
        }
    }

    pub fn connect(endpoint: &Endpoint, timeout: Duration) -> io::Result<Stream> {
        let stream = UnixStream::connect(&endpoint.socket)?;
        let _ = stream.set_write_timeout(Some(timeout));
        Ok(Stream(stream))
    }
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::io::{self, Read, Write};
    use std::iter::once;
    use std::ptr::null_mut;
    use std::thread;
    use std::time::{Duration, Instant};

    use windows_sys::Win32::Foundation::{
        CloseHandle, LocalFree, ERROR_ACCESS_DENIED, ERROR_BROKEN_PIPE, ERROR_FILE_NOT_FOUND,
        ERROR_NO_DATA, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, ERROR_PIPE_NOT_CONNECTED, GENERIC_READ,
        GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FlushFileBuffers, ReadFile, WriteFile, FILE_FLAG_FIRST_PIPE_INSTANCE,
        OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
    };
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PeekNamedPipe, WaitNamedPipeW,
        PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES,
        PIPE_WAIT,
    };

    use super::Endpoint;

    /// The kernel's buffer for each direction of one pipe instance.
    const BUFFER_BYTES: u32 = 64 * 1024;
    /// Owner-only: the window's own user, as Qt's `UserAccessOption` grants. `OW` is the owner rights
    /// SID, which names the creating user.
    const OWNER_ONLY_SDDL: &str = "D:(A;;GA;;;OW)";

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(once(0)).collect()
    }

    fn last_error() -> io::Error {
        io::Error::last_os_error()
    }

    fn code_of(error: &io::Error) -> Option<u32> {
        error.raw_os_error().map(|code| code as u32)
    }

    /// Whether a pipe read or peek failed because the other end went away, which reads as end of file.
    fn peer_closed(error: &io::Error) -> bool {
        matches!(
            code_of(error),
            Some(ERROR_BROKEN_PIPE | ERROR_NO_DATA | ERROR_PIPE_NOT_CONNECTED)
        )
    }

    /// One pipe handle, client or server side.
    pub struct Stream {
        handle: HANDLE,
        server: bool,
        deadline: Option<Instant>,
    }

    // SAFETY: a pipe handle is an OS object with no thread affinity. The stream is moved to one
    // connection thread and used there only.
    unsafe impl Send for Stream {}

    impl Stream {
        /// Reads give up with `TimedOut` once `deadline` passes. `None` waits.
        pub fn set_read_deadline(&mut self, deadline: Option<Instant>) {
            self.deadline = deadline;
        }

        /// Flushes a server reply until the client has read it, so closing does not drop bytes.
        pub fn finish(self) -> io::Result<()> {
            if self.server && unsafe { FlushFileBuffers(self.handle) } == 0 {
                return Err(last_error());
            }
            Ok(())
        }
    }

    impl Drop for Stream {
        fn drop(&mut self) {
            // SAFETY: the handle came from CreateFileW or CreateNamedPipeW and is closed only here.
            unsafe {
                if self.server {
                    DisconnectNamedPipe(self.handle);
                }
                CloseHandle(self.handle);
            }
        }
    }

    impl Read for Stream {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if buf.is_empty() {
                return Ok(0);
            }
            let mut pause = Duration::from_millis(1);
            loop {
                let mut available: u32 = 0;
                // SAFETY: the handle is open; the null buffer asks only for the byte count.
                let peeked = unsafe {
                    PeekNamedPipe(self.handle, null_mut(), 0, null_mut(), &mut available, null_mut())
                };
                if peeked == 0 {
                    let error = last_error();
                    return if peer_closed(&error) { Ok(0) } else { Err(error) };
                }
                if available > 0 {
                    let want = available.min(buf.len().min(u32::MAX as usize) as u32);
                    let mut read: u32 = 0;
                    // SAFETY: `want` bytes fit in `buf`, and the peek said they are waiting.
                    let ok = unsafe {
                        ReadFile(self.handle, buf.as_mut_ptr(), want, &mut read, null_mut())
                    };
                    if ok == 0 {
                        let error = last_error();
                        return if peer_closed(&error) { Ok(0) } else { Err(error) };
                    }
                    return Ok(read as usize);
                }
                if self.deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                    return Err(io::Error::new(io::ErrorKind::TimedOut, "no reply from the window"));
                }
                thread::sleep(pause);
                pause = (pause * 2).min(Duration::from_millis(10));
            }
        }
    }

    impl Write for Stream {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let len = buf.len().min(u32::MAX as usize) as u32;
            let mut written: u32 = 0;
            // SAFETY: `len` bytes of `buf` are valid for reads.
            let ok = unsafe { WriteFile(self.handle, buf.as_ptr(), len, &mut written, null_mut()) };
            if ok == 0 {
                return Err(last_error());
            }
            Ok(written as usize)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// The pipe the window listens on. It always keeps one instance waiting for the next client.
    pub struct Listener {
        name: Vec<u16>,
        security: *mut c_void,
        current: HANDLE,
    }

    // SAFETY: the listener moves to the accept thread, and nothing else touches its handles.
    unsafe impl Send for Listener {}

    impl Listener {
        /// Claims the endpoint. The first instance carries `FILE_FLAG_FIRST_PIPE_INSTANCE`, so a second
        /// window fails with `AddrInUse` and the windows of one user cannot share a name.
        pub fn bind(endpoint: &Endpoint) -> io::Result<Self> {
            let name = wide(&endpoint.pipe);
            let security = owner_only_descriptor()?;
            match create_instance(&name, security, true) {
                Ok(current) => Ok(Self {
                    name,
                    security,
                    current,
                }),
                Err(error) => {
                    // SAFETY: the descriptor came from ConvertStringSecurityDescriptor and is freed once.
                    unsafe { LocalFree(security) };
                    Err(error)
                }
            }
        }

        /// Waits for the next client. The pipe's next instance is created before the connected one is
        /// returned, so a client that arrives right after sees a waiting instance.
        pub fn accept(&mut self) -> io::Result<Stream> {
            let handle = self.current;
            if handle == INVALID_HANDLE_VALUE {
                return Err(io::Error::other("the pipe could not be recreated"));
            }
            // SAFETY: `handle` is one of this listener's own pipe instances.
            let connected = unsafe { ConnectNamedPipe(handle, null_mut()) };
            if connected == 0 {
                let error = last_error();
                if code_of(&error) != Some(ERROR_PIPE_CONNECTED) {
                    self.replace_current(handle);
                    return Err(error);
                }
            }
            match create_instance(&self.name, self.security, false) {
                Ok(next) => {
                    self.current = next;
                    Ok(Stream {
                        handle,
                        server: true,
                        deadline: None,
                    })
                }
                Err(error) => {
                    self.replace_current(handle);
                    Err(error)
                }
            }
        }

        /// Closes `handle` and makes the listener wait on a fresh instance, or on none if the kernel
        /// refuses one, which ends the accept loop on its next call.
        fn replace_current(&mut self, handle: HANDLE) {
            // SAFETY: `handle` is an instance this listener owns and is no longer used.
            unsafe { CloseHandle(handle) };
            self.current = create_instance(&self.name, self.security, false).unwrap_or(INVALID_HANDLE_VALUE);
        }
    }

    impl Drop for Listener {
        fn drop(&mut self) {
            // SAFETY: each handle and the descriptor are owned here and released once.
            unsafe {
                if self.current != INVALID_HANDLE_VALUE {
                    CloseHandle(self.current);
                }
                LocalFree(self.security);
            }
        }
    }

    fn owner_only_descriptor() -> io::Result<*mut c_void> {
        let sddl = wide(OWNER_ONLY_SDDL);
        let mut descriptor: *mut c_void = null_mut();
        // SAFETY: `sddl` is NUL-terminated; the descriptor is written through the out pointer.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                null_mut(),
            )
        };
        if ok == 0 {
            return Err(last_error());
        }
        Ok(descriptor)
    }

    fn create_instance(name: &[u16], security: *mut c_void, first: bool) -> io::Result<HANDLE> {
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: security,
            bInheritHandle: 0,
        };
        let mut open_mode = PIPE_ACCESS_DUPLEX;
        if first {
            open_mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
        }
        let mode = PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS;
        // SAFETY: `name` and `attributes` outlive the call; the kernel copies the descriptor.
        let handle = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                open_mode,
                mode,
                PIPE_UNLIMITED_INSTANCES,
                BUFFER_BYTES,
                BUFFER_BYTES,
                0,
                &attributes,
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            let error = last_error();
            if first && code_of(&error) == Some(ERROR_ACCESS_DENIED) {
                return Err(io::Error::new(io::ErrorKind::AddrInUse, "the window endpoint is taken"));
            }
            return Err(error);
        }
        Ok(handle)
    }

    pub fn connect(endpoint: &Endpoint, timeout: Duration) -> io::Result<Stream> {
        let name = wide(&endpoint.pipe);
        let deadline = Instant::now() + timeout;
        loop {
            // SAFETY: `name` is NUL-terminated; the default security and template are null.
            let handle = unsafe {
                CreateFileW(
                    name.as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    0,
                    std::ptr::null(),
                    OPEN_EXISTING,
                    0,
                    null_mut(),
                )
            };
            if handle != INVALID_HANDLE_VALUE {
                return Ok(Stream {
                    handle,
                    server: false,
                    deadline: None,
                });
            }
            let error = last_error();
            let now = Instant::now();
            match code_of(&error) {
                Some(ERROR_PIPE_BUSY) if now < deadline => {
                    let wait = (deadline - now).as_millis().clamp(1, u32::MAX as u128) as u32;
                    // SAFETY: `name` is NUL-terminated. The call only waits for an instance to free up.
                    unsafe { WaitNamedPipeW(name.as_ptr(), wait) };
                }
                Some(ERROR_FILE_NOT_FOUND) if now < deadline => {
                    // A client that connects while the server is between instances retries briefly.
                    thread::sleep(Duration::from_millis(20));
                }
                _ => return Err(error),
            }
        }
    }
}

pub use imp::{Listener, Stream};
