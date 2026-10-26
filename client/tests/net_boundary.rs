//! Rule 4 of the GPUI plan: sockets, TLS and the async runtime live in `src/net/`
//! alone, and `src/net/` reaches no toolkit and no app layer above the transport seam.
//!
//! Three gates scan this crate's `src/` sources with comments blanked:
//!
//! - `tokio`, `tokio_rustls`, `rustls`, `rustls_native_certs` and `webpki_roots` appear
//!   only under `src/net/`. Anywhere else they would be a second transport stack.
//! - `std::net` is forbidden in `src/{irc,session,controller,model,ui,storage,util,demo}`
//!   and in `src/main.rs`. Those layers run and test without a socket. `desktop/` keeps
//!   its loopback tests, and `src/net/` is where sockets belong.
//! - `src/net/` names neither `gpui`, `crate::ui` nor `crate::controller`, in any spelling
//!   that reaches them: from the crate root (`crate::`), relative to the net module
//!   (`super::`), inside a `{...}` group, or through a glob (`crate::*`, `super::*`). The
//!   one exception is `controller::transports::TransportFactory` in `src/net/factory.rs`.
//!
//! `tests/` directories under `src/` are skipped. The session and controller gates
//! keep forbidden names as string literals, and test code is not the production
//! boundary. The matcher is `common/forbidden.rs`, shared with `irc_portability.rs`.

mod common;
#[path = "common/forbidden.rs"]
mod forbidden;

use std::path::{Path, PathBuf};

use common::{code_only, crate_root, line_of, read_code, rust_files_under};
use forbidden::{group_heads, segment_offsets};

/// Crates only `src/net/` may name. `tokio_rustls` is listed by name because `tokio`
/// does not match inside it.
const NET_CRATES: &[&str] = &[
    "tokio",
    "tokio_rustls",
    "rustls",
    "rustls_native_certs",
    "webpki_roots",
];
/// The directories under `src/` that must run without a socket.
const NO_SOCKET_DIRS: &[&str] = &[
    "irc",
    "session",
    "controller",
    "model",
    "ui",
    "storage",
    "util",
    "demo",
];
/// The files under `src/` that must run without a socket. `app.rs` is the process entry
/// (called from the thin `main.rs`), and it reaches the sockets only through `src/net/`.
const NO_SOCKET_FILES: &[&str] = &["main.rs", "app.rs"];
/// Paths `src/net/` must not name: the toolkit, and the app layers above the transport,
/// written from the crate root (`crate::`) or relative to the net module (`super::`).
const NET_FORBIDDEN_PATHS: &[&str] = &[
    "gpui",
    "crate::ui",
    "crate::controller",
    "super::ui",
    "super::controller",
];
/// Globs `src/net/` must not use. A glob brings the app layers into scope without naming
/// them, so `use super::*;` in `src/net/mod.rs` would reach `ui` and `controller`.
const NET_FORBIDDEN_GLOBS: &[&str] = &["crate::*", "super::*"];
/// The `crate` and `super` modules above, by name, for `use crate::{ui::Shell, ...}`
/// and `use super::{controller}` groups.
const NET_FORBIDDEN_CRATE_MODULES: &[&str] = &["ui", "controller"];
/// The path prefixes whose `{...}` groups are checked for `NET_FORBIDDEN_CRATE_MODULES`.
const NET_GROUP_PREFIXES: &[&str] = &["crate", "super"];
/// The one path into the controller that `src/net/factory.rs` may name.
const FACTORY_PATH: &str = "crate::controller::transports::TransportFactory";

/// Every use of a net-only crate in `code` (comments already blanked), with its line.
fn net_crate_uses(code: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    for krate in NET_CRATES {
        for offset in segment_offsets(code, krate) {
            found.push((line_of(code, offset), (*krate).to_string()));
        }
    }
    found.sort();
    found
}

/// Every `std::net` use in `code`: the path form and the `std::{net, ..}` group form.
fn std_net_uses(code: &str) -> Vec<(usize, String)> {
    let mut found: Vec<(usize, String)> = segment_offsets(code, "std::net")
        .into_iter()
        .map(|offset| (line_of(code, offset), "std::net".to_string()))
        .collect();
    found.extend(
        group_heads(code, "std")
            .into_iter()
            .filter(|(_, head)| head == "net")
            .map(|(line, head)| (line, format!("std::{{{head}}}"))),
    );
    found.sort();
    found
}

/// Every reach in `code` into a toolkit or an app layer above the transport. When
/// `is_factory` is set, `FACTORY_PATH` is the only `crate::controller` path allowed.
fn net_reach_uses(code: &str, is_factory: bool) -> Vec<(usize, String)> {
    let allowed = if is_factory {
        segment_offsets(code, FACTORY_PATH)
    } else {
        Vec::new()
    };
    let mut found = Vec::new();
    for path in NET_FORBIDDEN_PATHS.iter().chain(NET_FORBIDDEN_GLOBS) {
        for offset in segment_offsets(code, path) {
            if !allowed.contains(&offset) {
                found.push((line_of(code, offset), (*path).to_string()));
            }
        }
    }
    for prefix in NET_GROUP_PREFIXES {
        for (line, head) in group_heads(code, prefix) {
            if NET_FORBIDDEN_CRATE_MODULES.contains(&head.as_str()) {
                found.push((line, format!("{prefix}::{{{head}}}")));
            }
        }
    }
    found.sort();
    found
}

fn src_dir() -> PathBuf {
    crate_root().join("src")
}

/// The production `.rs` files under `dir`, sorted. `tests` directories are skipped.
fn production_sources(dir: &Path) -> Vec<PathBuf> {
    rust_files_under(dir)
        .into_iter()
        .filter(|file| {
            !file
                .strip_prefix(dir)
                .expect("a file under its directory")
                .components()
                .any(|part| part.as_os_str() == "tests")
        })
        .collect()
}

fn relative(path: &Path) -> String {
    path.strip_prefix(crate_root()).unwrap_or(path).display().to_string()
}

#[test]
fn net_crates_appear_only_under_src_net() {
    let net = src_dir().join("net");
    let files = production_sources(&src_dir());
    assert!(!files.is_empty(), "no production sources under src/");
    // The scan covers all of src/, so the binary and the layers without socket code of
    // their own are named here: a change to the walk must not drop them unnoticed.
    for pinned in ["main.rs", "app.rs", "util", "demo"] {
        assert!(
            files.iter().any(|file| file.starts_with(src_dir().join(pinned))),
            "src/{pinned} is missing from the net-crate scan"
        );
    }

    let mut problems = Vec::new();
    for file in files.iter().filter(|file| !file.starts_with(&net)) {
        for (line, krate) in net_crate_uses(&read_code(file)) {
            problems.push(format!("{}:{line}: {krate}", relative(file)));
        }
    }
    assert!(
        problems.is_empty(),
        "tokio, tokio_rustls, rustls, rustls_native_certs and webpki_roots belong under src/net/ only:\n{}",
        problems.join("\n")
    );
}

/// The production sources that must run without a socket: every file under
/// `NO_SOCKET_DIRS`, and each of `NO_SOCKET_FILES`.
fn no_socket_sources() -> Vec<PathBuf> {
    let mut files = Vec::new();
    for dir in NO_SOCKET_DIRS {
        let found = production_sources(&src_dir().join(dir));
        assert!(!found.is_empty(), "no production sources under src/{dir}");
        files.extend(found);
    }
    for name in NO_SOCKET_FILES {
        let file = src_dir().join(name);
        assert!(file.is_file(), "no src/{name} to scan");
        files.push(file);
    }
    files
}

#[test]
fn std_net_stays_out_of_the_layers_that_run_without_sockets() {
    let mut problems = Vec::new();
    for file in no_socket_sources() {
        for (line, path) in std_net_uses(&read_code(&file)) {
            problems.push(format!("{}:{line}: {path}", relative(&file)));
        }
    }
    assert!(
        problems.is_empty(),
        "std::net is forbidden in irc, session, controller, model, ui, storage, util, demo, app.rs and main.rs; sockets belong in src/net/:\n{}",
        problems.join("\n")
    );
}

#[test]
fn net_reaches_no_toolkit_or_app_layer_but_the_factory_seam() {
    let net = src_dir().join("net");
    let factory = net.join("factory.rs");
    let files = production_sources(&net);
    assert!(!files.is_empty(), "no production sources under src/net/");

    let mut problems = Vec::new();
    for file in files {
        let is_factory = file == factory;
        for (line, path) in net_reach_uses(&read_code(&file), is_factory) {
            problems.push(format!("{}:{line}: {path}", relative(&file)));
        }
    }
    assert!(
        problems.is_empty(),
        "src/net/ must not name gpui, crate::ui or crate::controller (factory.rs may name the TransportFactory path only):\n{}",
        problems.join("\n")
    );
}

#[test]
fn the_gate_flags_each_forbidden_form() {
    let net_crates = [
        "use tokio::spawn;",
        "use ::tokio::time::sleep;",
        "let c = tokio_rustls::TlsConnector::from(config);",
        "use rustls::ClientConfig;",
        "use rustls :: ClientConfig;",
        "use rustls_native_certs::load_native_certs;",
        "let roots = webpki_roots::TLS_SERVER_ROOTS;",
    ];
    for source in net_crates {
        assert!(!net_crate_uses(&code_only(source)).is_empty(), "not flagged: {source}");
    }

    let std_net = [
        "use std::net::TcpStream;",
        "let l = std::net::TcpListener::bind(addr);",
        "use std::{io, net::TcpStream};",
        "use std :: net :: UdpSocket;",
        "use ::std::net::SocketAddr;",
        "use std::{net};",
    ];
    for source in std_net {
        assert!(!std_net_uses(&code_only(source)).is_empty(), "not flagged: {source}");
    }

    let reach = [
        "use gpui::App;",
        "use crate::ui::shell::Shell;",
        "use crate::controller::Controller;",
        "use crate::{ui::shell::Shell};",
        "use crate::{controller::Controller, irc};",
        "use super::ui::shell::Shell;",
        "use super::controller::Controller;",
        "use super::super::ui::Shell;",
        "use super :: controller :: Controller;",
        "use super::{ui::Shell};",
        "use super::{hub, controller::Controller};",
        "use super::*;",
        "use crate::*;",
        "use super::super::*;",
    ];
    for source in reach {
        assert!(!net_reach_uses(&code_only(source), false).is_empty(), "not flagged: {source}");
    }
    // Only the exact factory path passes, and only in factory.rs. Lookalikes and groups do not.
    let factory_reach = [
        "use crate::controller::Controller;",
        "use crate::controller::transports::TransportFactoryExtra;",
        "use crate::controller::transports::{TransportFactory};",
        "use crate::controller::transports::TransportFactory; use crate::controller::Controller;",
        "use super::controller::transports::TransportFactory;",
        "use super::*;",
    ];
    for source in factory_reach {
        assert!(!net_reach_uses(&code_only(source), true).is_empty(), "not flagged in factory.rs: {source}");
    }
}

#[test]
fn the_gate_allows_comments_lookalikes_and_the_factory_seam() {
    let allowed_crates = [
        "// tokio and rustls are named in this comment only",
        "/* webpki_roots */ fn f() {}",
        "let tokio_rate = 2; let myrustls = 3;",
        "use std::network::Thing;",
    ];
    for source in allowed_crates {
        assert!(net_crate_uses(&code_only(source)).is_empty(), "wrongly flagged: {source}");
    }

    let allowed_std = [
        "// std::net is not used here",
        "use std::network::Thing;",
        "use std::{io, networking};",
    ];
    for source in allowed_std {
        assert!(std_net_uses(&code_only(source)).is_empty(), "wrongly flagged: {source}");
    }

    let allowed_reach = [
        "// gpui, crate::ui and crate::controller in a comment",
        "let mygpui = 1; use crate::uitest::Thing;",
        "use crate::controller_model::Thing;",
        "use crate::network::Thing;",
        "use super::hub::NetHub;",
        "use super::{hub, tls};",
        "use super::uitest::Thing;",
        "use super::controller_model::Thing;",
        "use super::superglob::Thing;",
        "use crate::net::*;",
    ];
    for source in allowed_reach {
        assert!(net_reach_uses(&code_only(source), false).is_empty(), "wrongly flagged: {source}");
    }
    assert!(
        net_reach_uses(
            &code_only("use crate::controller::transports::TransportFactory;"),
            true
        )
        .is_empty(),
        "the factory seam is wrongly flagged"
    );
}

#[test]
fn the_gate_reports_line_numbers() {
    let crates = "fn a() {}\n// tokio in a comment\nuse tokio::spawn;\n";
    assert_eq!(net_crate_uses(&code_only(crates)), vec![(3, "tokio".to_string())]);

    let std_net = "fn a() {}\n// std::net in a comment\nuse std::{io, net};\n";
    assert_eq!(
        std_net_uses(&code_only(std_net)),
        vec![(3, "std::{net}".to_string())]
    );

    let reach = "use crate::ui::A;\n// gpui\nuse gpui::App;\n";
    assert_eq!(
        net_reach_uses(&code_only(reach), false),
        vec![(1, "crate::ui".to_string()), (3, "gpui".to_string())]
    );

    let relative = "fn a() {}\n// super::ui in a comment\nuse super::{controller::Controller};\n";
    assert_eq!(
        net_reach_uses(&code_only(relative), false),
        vec![(3, "super::{controller}".to_string())]
    );
}

#[test]
fn the_socket_scans_reach_util_demo_and_main() {
    let files = no_socket_sources();
    for pinned in ["main.rs", "app.rs", "util", "demo"] {
        assert!(
            files.iter().any(|file| file.starts_with(src_dir().join(pinned))),
            "src/{pinned} is missing from the no-socket scan"
        );
    }
}
