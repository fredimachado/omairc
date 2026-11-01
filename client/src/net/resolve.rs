//! Unix host resolution through getaddrinfo. Production runs this on a blocking thread so
//! the hub's current-thread runtime stays responsive.

use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

/// Whether a getaddrinfo status means the name does not exist.
pub(super) fn gai_name_not_found(code: i32) -> bool {
    code == libc::EAI_NONAME || code == libc::EAI_NODATA
}

/// Maps a getaddrinfo failure into `io::Error`, using `NotFound` for missing names.
pub(super) fn io_error_from_gai_code(code: i32) -> io::Error {
    if gai_name_not_found(code) {
        io::Error::new(io::ErrorKind::NotFound, gai_message(code))
    } else {
        io::Error::other(gai_message(code))
    }
}

fn gai_message(code: i32) -> String {
    use std::ffi::CStr;
    unsafe { CStr::from_ptr(libc::gai_strerror(code)).to_string_lossy().into_owned() }
}

/// Resolves `host` for TCP `port` with one getaddrinfo call. The caller must run this on a
/// blocking thread.
pub(super) fn blocking_lookup_host(host: &str, port: u16) -> io::Result<Vec<SocketAddr>> {
    use std::ffi::CString;
    use std::ptr;

    let host = CString::new(host).map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidInput, "invalid dns name")
    })?;
    let port = CString::new(port.to_string()).map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidInput, "invalid port")
    })?;
    let hints = libc::addrinfo {
        ai_flags: libc::AI_ADDRCONFIG,
        ai_family: libc::AF_UNSPEC,
        ai_socktype: libc::SOCK_STREAM,
        ai_protocol: 0,
        ai_addrlen: 0,
        ai_addr: ptr::null_mut(),
        ai_canonname: ptr::null_mut(),
        ai_next: ptr::null_mut(),
    };
    let mut result: *mut libc::addrinfo = ptr::null_mut();
    let code =
        unsafe { libc::getaddrinfo(host.as_ptr(), port.as_ptr(), &hints, &mut result) };
    if code != 0 {
        return Err(io_error_from_gai_code(code));
    }
    let addresses = unsafe { collect_addresses(result) };
    unsafe { libc::freeaddrinfo(result) };
    if addresses.is_empty() {
        return Err(io::Error::from(io::ErrorKind::NotFound));
    }
    Ok(addresses)
}

unsafe fn collect_addresses(head: *mut libc::addrinfo) -> Vec<SocketAddr> {
    let mut addresses = Vec::new();
    let mut cursor = head;
    while !cursor.is_null() {
        let entry = unsafe { &*cursor };
        if let Some(address) = socket_addr_from(entry) {
            addresses.push(address);
        }
        cursor = entry.ai_next;
    }
    addresses
}

fn socket_addr_from(info: &libc::addrinfo) -> Option<SocketAddr> {
    if info.ai_addr.is_null() {
        return None;
    }
    unsafe {
        match info.ai_family {
            libc::AF_INET => {
                let addr = &*(info.ai_addr as *const libc::sockaddr_in);
                let ip = Ipv4Addr::from(u32::from_be(addr.sin_addr.s_addr));
                let port = u16::from_be(addr.sin_port);
                Some(SocketAddr::new(IpAddr::V4(ip), port))
            }
            libc::AF_INET6 => {
                let addr = &*(info.ai_addr as *const libc::sockaddr_in6);
                let ip = Ipv6Addr::from(addr.sin6_addr.s6_addr);
                let port = u16::from_be(addr.sin6_port);
                Some(SocketAddr::new(IpAddr::V6(ip), port))
            }
            _ => None,
        }
    }
}
