//! The CLI's requests, answered on the UI thread. Port of `src/omaircipchandler.{h,cpp}` (C++). Each
//! request reads the controller or calls one of its intents, then answers with one JSON line.
//!
//! `serve` is the loop that runs on the window. It drains the `IpcServer` queue on a gpui task, the
//! same way the controller tick runs, and borrows the shell's controller for each request. Nothing here
//! holds the controller across requests, and nothing touches it from another thread.

use std::time::Duration;

use gpui::{prelude::*, AnyWindowHandle, App, Entity};

use crate::controller::cli::{CliWindow, CliMessage};
use crate::controller::controller::Controller;
use crate::irc::casemapping::IrcCaseMapping;
use crate::ui::shell::Shell;

use super::cli_cursor::CursorStore;
use super::ipc::{self, Command, ConnectionInfo, ConversationRow, MemberRow, MessageRow, ReadWindow, Request, DEFAULT_LAST};
use super::singleinstance::IpcServer;

/// How often the loop looks for requests waiting on the queue.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// One answer: the JSON line, and whether the window should be raised for it.
pub struct Reply {
    pub line: String,
    pub raise: bool,
}

impl Reply {
    fn plain(line: String) -> Self {
        Self { line, raise: false }
    }
}

/// Answers CLI requests against a controller. It owns the `--unread` cursor store and nothing else.
pub struct CliHandler {
    cursors: CursorStore,
}

impl CliHandler {
    pub fn new(cursors: CursorStore) -> Self {
        Self { cursors }
    }

    /// Answers one request line (`OmaircIpcHandler::handleLine`).
    pub fn handle_line(&self, controller: &mut Controller, line: &str) -> Reply {
        let request = match ipc::parse_request(line) {
            Ok(request) => request,
            Err(message) => return Reply::plain(ipc::error_response(&message)),
        };
        match request.command {
            Command::Raise => Reply {
                line: ipc::ok_response(),
                raise: true,
            },
            Command::Connections => {
                let rows: Vec<ConnectionInfo> = controller
                    .cli_network_ids()
                    .iter()
                    .map(|network_id| connection_info(controller, network_id))
                    .collect();
                Reply::plain(ipc::ok_connections(&rows))
            }
            _ => {
                let networks = controller.cli_network_ids();
                match ipc::resolve_network_id(&request.network_id, &networks) {
                    Ok(network_id) => Reply::plain(self.scoped(controller, &network_id, &request)),
                    Err(message) => Reply::plain(ipc::error_response(&message)),
                }
            }
        }
    }

    /// The requests that name one connection, once it is resolved.
    fn scoped(&self, controller: &mut Controller, network_id: &str, request: &Request) -> String {
        match request.command {
            Command::Status => ipc::ok_status(&connection_info(controller, network_id)),
            Command::Send => send(controller, network_id, request),
            Command::Read => self.read(controller, network_id, request),
            Command::Names => names(controller, network_id, &request.target),
            Command::Conversations => {
                let rows: Vec<ConversationRow> = controller
                    .cli_conversations(network_id)
                    .into_iter()
                    .map(|row| ConversationRow {
                        channel: row.channel,
                        mention: row.mention,
                        target: row.target,
                        topic: row.topic,
                        unread: row.unread,
                    })
                    .collect();
                ipc::ok_conversations(&rows)
            }
            // Raise and Connections are answered before the network is resolved.
            Command::Raise | Command::Connections => ipc::error_response("Unknown command"),
        }
    }

    /// `read`: the transcript lines for the request's window. `--unread` resumes after the saved
    /// cursor, and saves the newest line it returns.
    fn read(&self, controller: &Controller, network_id: &str, request: &Request) -> String {
        let target = request.target.as_str();
        let (mapping, known) = case_mapping(controller, network_id);
        let unread = matches!(request.window, ReadWindow::Unread);
        let window = match &request.window {
            ReadWindow::Last(count) => CliWindow::Last(*count),
            ReadWindow::Since(since) => CliWindow::Since(since.cutoff),
            ReadWindow::Unread => match self.cursors.load(network_id, target, mapping, known) {
                Some(cursor) => CliWindow::After(cursor),
                None => CliWindow::Last(DEFAULT_LAST),
            },
        };
        let snapshot = match controller.cli_messages(network_id, target, window) {
            Ok(snapshot) => snapshot,
            Err(message) => return ipc::error_response(&message),
        };
        if unread {
            if let Some(newest) = snapshot.lines.last() {
                if let Err(error) = self.cursors.save(network_id, target, mapping, known, &newest.cursor()) {
                    log::warn!("Could not save the CLI cursor for {network_id} {target}: {error}");
                }
            }
        }
        let rows: Vec<MessageRow> = snapshot.lines.iter().map(message_row).collect();
        ipc::ok_messages(&rows, snapshot.truncated)
    }
}

/// One connection row, from the session and the controller's own answers (`infoFor`).
fn connection_info(controller: &Controller, network_id: &str) -> ConnectionInfo {
    let Some(session) = controller.session(network_id) else {
        return ConnectionInfo {
            id: network_id.to_owned(),
            ..ConnectionInfo::default()
        };
    };
    ConnectionInfo {
        host: session.host(),
        id: network_id.to_owned(),
        last_error: controller.last_error_for(network_id).unwrap_or_default().to_owned(),
        name: session.name(),
        nick: session.nick(),
        port: session.port(),
        selected: controller.selected_network_id() == Some(network_id),
        state: controller.connection_status_for(network_id).to_owned(),
        tls: session.tls_enabled(),
    }
}

/// `send`: a PRIVMSG to the target through the controller's send intent. The selection does not move.
/// A refusal answers with the reason the controller recorded.
fn send(controller: &mut Controller, network_id: &str, request: &Request) -> String {
    if controller.send_to_target(network_id, &request.target, &request.text) {
        return ipc::ok_response();
    }
    let message = controller
        .last_error_for(network_id)
        .unwrap_or("Failed to send message")
        .to_owned();
    ipc::error_response(&message)
}

/// `names`: the member panel for a joined channel, in panel order.
fn names(controller: &Controller, network_id: &str, target: &str) -> String {
    match controller.cli_members(network_id, target) {
        Ok(members) => {
            let rows: Vec<MemberRow> = members
                .into_iter()
                .map(|member| MemberRow {
                    away: member.is_away(),
                    label: member.label,
                    nick: member.nick,
                    status: member.status,
                })
                .collect();
            ipc::ok_members(&rows)
        }
        Err(message) => ipc::error_response(&message),
    }
}

fn message_row(line: &CliMessage) -> MessageRow {
    MessageRow {
        kind: line.kind.to_owned(),
        mention: line.mention,
        message: line.message.clone(),
        msgid: line.msgid.clone(),
        network: line.network_id.clone(),
        sender: line.sender.clone(),
        target: line.target.clone(),
        timestamp: ipc::iso_millis(line.timestamp),
    }
}

fn case_mapping(controller: &Controller, network_id: &str) -> (IrcCaseMapping, bool) {
    let features = controller.features(network_id);
    (features.case_mapping(), features.case_mapping_known())
}

/// Starts the loop that answers CLI requests on the window's UI thread. Each request runs against the
/// shell's controller, then the shell follows whatever changed, as it does after a click. A raise
/// request activates the window.
pub fn serve(shell: Entity<Shell>, window: AnyWindowHandle, server: IpcServer, cx: &mut App) {
    let handler = CliHandler::new(CursorStore::from_env());
    cx.spawn(async move |cx| {
        loop {
            while let Some(job) = server.try_next() {
                let answered = shell.update(cx, |shell, cx| {
                    let reply = handler.handle_line(&mut shell.controller, job.line());
                    shell.drain_notifications(cx);
                    reply
                });
                match answered {
                    Ok(reply) => {
                        let raise = reply.raise;
                        job.respond(reply.line);
                        if raise {
                            cx.update_window(window, |_, window, _| window.activate_window()).ok();
                        }
                    }
                    // The shell is gone, so the window is closing. The client gets no answer.
                    Err(_) => drop(job),
                }
            }
            cx.background_executor().timer(POLL_INTERVAL).await;
        }
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;
    use std::time::Instant;

    use super::*;
    use crate::controller::cli::CliCursor;
    use crate::desktop::cli;
    use crate::desktop::endpoint::Endpoint;
    use crate::desktop::singleinstance::{self, Startup};
    use crate::session::clock::FakeClock;
    use chrono::TimeZone;

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    /// A unique endpoint, so parallel tests never meet each other's window.
    fn endpoint() -> Endpoint {
        let serial = NEXT.fetch_add(1, Ordering::SeqCst);
        Endpoint::named(&format!("omairc-poc-loop-{}-{serial}", std::process::id()))
    }

    fn demo() -> Controller {
        let clock = Rc::new(FakeClock::new(
            chrono::Utc.with_ymd_and_hms(2026, 9, 12, 12, 0, 0).unwrap(),
        ));
        Controller::new_demo_with(clock).expect("the demo starts")
    }

    /// A folder under the temp directory for one test's cursors, removed when the test ends.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn scratch_handler(tag: &str) -> (CliHandler, Scratch) {
        let root = std::env::temp_dir().join(format!(
            "omairc-poc-loop-{tag}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        (CliHandler::new(CursorStore::at(root.clone())), Scratch(root))
    }

    /// Runs `args` through the real CLI client against `endpoint`, while this thread plays the UI loop
    /// over the same queue `serve` drains. Returns the exit code, stdout, and stderr.
    fn run_cli(
        server: &IpcServer,
        controller: &mut Controller,
        handler: &CliHandler,
        endpoint: &Endpoint,
        args: &[&str],
    ) -> (i32, String, String) {
        let client_endpoint = endpoint.clone();
        let args: Vec<String> = args.iter().map(|word| (*word).to_owned()).collect();
        let client = thread::spawn(move || {
            let mut out = Vec::new();
            let mut err = Vec::new();
            let code = cli::execute(&args, &client_endpoint, &mut out, &mut err);
            (code, out, err)
        });
        let deadline = Instant::now() + Duration::from_secs(20);
        while !client.is_finished() {
            match server.try_next() {
                Some(job) => {
                    let reply = handler.handle_line(controller, job.line());
                    job.respond(reply.line);
                }
                None => thread::sleep(Duration::from_millis(2)),
            }
            assert!(Instant::now() < deadline, "the CLI did not finish");
        }
        let (code, out, err) = client.join().expect("the client thread");
        (
            code,
            String::from_utf8(out).expect("utf-8 stdout"),
            String::from_utf8(err).expect("utf-8 stderr"),
        )
    }

    fn primary(endpoint: &Endpoint) -> IpcServer {
        match singleinstance::acquire(endpoint).expect("the endpoint is free") {
            Startup::Primary(server) => server,
            _ => panic!("the first claim becomes the primary"),
        }
    }

    fn parsed(line: &str) -> serde_json::Value {
        serde_json::from_str(line.trim()).unwrap_or_else(|error| panic!("{line}: {error}"))
    }

    #[test]
    fn connections_and_list_answer_the_same_rows() {
        let endpoint = endpoint();
        let server = primary(&endpoint);
        let mut controller = demo();
        let (handler, _scratch) = scratch_handler("connections");
        let (code, out, _) = run_cli(&server, &mut controller, &handler, &endpoint, &["connections"]);
        assert_eq!(code, 0);
        let body = parsed(&out);
        assert_eq!(body["ok"], true);
        let rows = body["connections"].as_array().expect("rows");
        let ids: Vec<&str> = rows.iter().map(|row| row["id"].as_str().unwrap_or_default()).collect();
        assert_eq!(ids, ["oftc", "omarchy"]);
        assert!(rows.iter().all(|row| row["state"] == "Connected"));
        let omarchy = rows.iter().find(|row| row["id"] == "omarchy").expect("omarchy");
        assert_eq!(omarchy["selected"], true);
        assert!(omarchy.get("lastError").is_none());

        let (list_code, list_out, _) = run_cli(&server, &mut controller, &handler, &endpoint, &["list"]);
        assert_eq!(list_code, 0);
        assert_eq!(list_out, out, "the list alias answers exactly like connections");
    }

    #[test]
    fn status_resolves_the_network_and_names_the_choices() {
        let endpoint = endpoint();
        let server = primary(&endpoint);
        let mut controller = demo();
        let (handler, _scratch) = scratch_handler("status");
        let (code, out, _) = run_cli(&server, &mut controller, &handler, &endpoint, &["status", "--network", "oftc"]);
        assert_eq!(code, 0);
        assert_eq!(parsed(&out)["status"]["id"], "oftc");

        // A server-side refusal prints its JSON on stdout only, as Qt's client does for a reply.
        let (code, out, err) = run_cli(&server, &mut controller, &handler, &endpoint, &["status"]);
        assert_eq!(code, 1);
        assert_eq!(out.trim(), r#"{"error":"Multiple connections are available; specify a network id.","ok":false}"#);
        assert_eq!(err, "");

        let (code, out, _) = run_cli(&server, &mut controller, &handler, &endpoint, &["status", "--network", "zz"]);
        assert_eq!(code, 1);
        assert_eq!(parsed(&out)["error"], "Unknown network id 'zz'.");
    }

    #[test]
    fn names_lists_the_panel_in_order_and_refuses_a_direct_message() {
        let endpoint = endpoint();
        let server = primary(&endpoint);
        let mut controller = demo();
        let (handler, _scratch) = scratch_handler("names");
        let (code, out, _) = run_cli(
            &server,
            &mut controller,
            &handler,
            &endpoint,
            &["names", "--network", "omarchy", "#omarchy"],
        );
        assert_eq!(code, 0);
        let members = parsed(&out)["members"].as_array().cloned().unwrap_or_default();
        let nicks: Vec<String> = members
            .iter()
            .map(|row| row["nick"].as_str().unwrap_or_default().to_owned())
            .collect();
        assert!(!nicks.is_empty());
        let expected: Vec<String> = controller
            .cli_members("omarchy", "#omarchy")
            .expect("joined")
            .into_iter()
            .map(|member| member.nick)
            .collect();
        assert_eq!(nicks, expected, "names answers in the member panel's order");

        let (code, out, _) = run_cli(
            &server,
            &mut controller,
            &handler,
            &endpoint,
            &["names", "--network", "omarchy", "status"],
        );
        assert_eq!(code, 1);
        assert_eq!(parsed(&out)["error"], "No conversation for 'status'.");
    }

    #[test]
    fn read_snapshots_chat_without_moving_the_selection() {
        let endpoint = endpoint();
        let server = primary(&endpoint);
        let mut controller = demo();
        let (handler, _scratch) = scratch_handler("read");
        let selected_before = controller.selected_target().to_owned();
        let (code, out, _) = run_cli(
            &server,
            &mut controller,
            &handler,
            &endpoint,
            &["read", "--network", "omarchy", "#omarchy", "--last", "3"],
        );
        assert_eq!(code, 0);
        let messages = parsed(&out)["messages"].as_array().cloned().unwrap_or_default();
        assert!(!messages.is_empty() && messages.len() <= 3);
        for row in &messages {
            let kind = row["kind"].as_str().unwrap_or_default();
            assert!(matches!(kind, "message" | "notice" | "action"), "{row}");
            assert_eq!(row["target"], "#omarchy");
            assert!(row["timestamp"].as_str().unwrap_or_default().ends_with('Z'));
        }
        assert_eq!(controller.selected_target(), selected_before);

        let (code, out, _) = run_cli(
            &server,
            &mut controller,
            &handler,
            &endpoint,
            &["read", "--network", "omarchy", "#nope"],
        );
        assert_eq!(code, 1);
        assert_eq!(parsed(&out)["error"], "No conversation for '#nope'.");
    }

    #[test]
    fn read_unread_resumes_after_the_saved_cursor() {
        let endpoint = endpoint();
        let server = primary(&endpoint);
        let mut controller = demo();
        let (handler, _scratch) = scratch_handler("unread");
        let first = run_cli(
            &server,
            &mut controller,
            &handler,
            &endpoint,
            &["read", "--network", "omarchy", "#omarchy", "--unread"],
        );
        assert_eq!(first.0, 0);
        assert!(!parsed(&first.1)["messages"].as_array().cloned().unwrap_or_default().is_empty());

        let second = run_cli(
            &server,
            &mut controller,
            &handler,
            &endpoint,
            &["read", "--network", "omarchy", "#omarchy", "--unread"],
        );
        assert_eq!(second.0, 0);
        assert_eq!(parsed(&second.1)["messages"], serde_json::json!([]));
        assert_eq!(second.1.trim(), r#"{"messages":[],"ok":true}"#);
    }

    #[test]
    fn send_delivers_through_the_controller_and_leaves_the_selection_alone() {
        let endpoint = endpoint();
        let server = primary(&endpoint);
        let mut controller = demo();
        let (handler, _scratch) = scratch_handler("send");
        let selected_before = controller.selected_target().to_owned();
        let (code, out, err) = run_cli(
            &server,
            &mut controller,
            &handler,
            &endpoint,
            &["send", "--network", "omarchy", "#omarchy", "hello", "from", "the", "loop"],
        );
        assert_eq!(code, 0, "{err}");
        assert_eq!(out.trim(), r#"{"ok":true}"#);
        assert_eq!(controller.selected_target(), selected_before);

        let (code, out, _) = run_cli(
            &server,
            &mut controller,
            &handler,
            &endpoint,
            &["read", "--network", "omarchy", "#omarchy", "--last", "5"],
        );
        assert_eq!(code, 0);
        assert!(out.contains("hello from the loop"), "{out}");
    }

    #[test]
    fn send_to_a_refused_network_answers_with_the_controller_reason() {
        let endpoint = endpoint();
        let server = primary(&endpoint);
        let mut controller = demo();
        let (handler, _scratch) = scratch_handler("refused");
        let (code, out, _) = run_cli(
            &server,
            &mut controller,
            &handler,
            &endpoint,
            &["send", "--network", "zz", "#omarchy", "hi"],
        );
        assert_eq!(code, 1);
        assert_eq!(parsed(&out)["error"], "Unknown network id 'zz'.");
    }

    #[test]
    fn a_secret_sent_to_a_service_never_reaches_the_answer_or_status() {
        let endpoint = endpoint();
        let server = primary(&endpoint);
        let mut controller = demo();
        let (handler, _scratch) = scratch_handler("secret");
        let (code, out, _) = run_cli(
            &server,
            &mut controller,
            &handler,
            &endpoint,
            &["send", "--network", "omarchy", "NickServ", "identify", "hunter2hunter2"],
        );
        assert_eq!(code, 0);
        assert!(!out.contains("hunter2"));
        let status: String = controller
            .status_lines("omarchy")
            .iter()
            .map(|entry| entry.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        // The demo echoes the PRIVMSG back, and Status keeps that self-echo to a service with its
        // secret masked, as the UI's `/ns` path does. The outbound frame itself is not a Status row.
        assert!(status.contains("IDENTIFY ***"), "{status}");
        assert!(!status.contains("hunter2"), "{status}");
        assert!(controller.cli_messages("omarchy", "NickServ", CliWindow::Last(10)).is_err());
    }

    #[test]
    fn conversations_list_channels_and_direct_messages() {
        let endpoint = endpoint();
        let server = primary(&endpoint);
        let mut controller = demo();
        let (handler, _scratch) = scratch_handler("conversations");
        let (code, out, _) = run_cli(
            &server,
            &mut controller,
            &handler,
            &endpoint,
            &["conversations", "--network", "omarchy"],
        );
        assert_eq!(code, 0);
        let rows = parsed(&out)["conversations"].as_array().cloned().unwrap_or_default();
        let channel = rows.iter().find(|row| row["target"] == "#omarchy").expect("the channel");
        assert_eq!(channel["channel"], true);
        assert!(rows.iter().all(|row| row["target"] != "Status"));
    }

    #[test]
    fn raise_answers_ok_and_asks_for_the_window() {
        let mut controller = demo();
        let (handler, _scratch) = scratch_handler("raise");
        let reply = handler.handle_line(&mut controller, r#"{"cmd":"raise"}"#);
        assert!(reply.raise);
        assert_eq!(reply.line, r#"{"ok":true}"#);
        let bare = handler.handle_line(&mut controller, "!");
        assert!(bare.raise);
    }

    #[test]
    fn a_bad_request_answers_with_its_error() {
        let mut controller = demo();
        let (handler, _scratch) = scratch_handler("bad");
        let reply = handler.handle_line(&mut controller, r#"{"cmd":"read","last":101}"#);
        assert!(!reply.raise);
        assert_eq!(reply.line, r#"{"error":"last must be between 1 and 100","ok":false}"#);
        let unknown = handler.handle_line(&mut controller, "{}");
        assert_eq!(unknown.line, r#"{"error":"Missing cmd","ok":false}"#);
    }

    #[test]
    fn cursors_from_the_handler_are_the_ones_the_store_writes() {
        let (handler, _scratch) = scratch_handler("store");
        let saved = CliCursor {
            timestamp: chrono::Utc.with_ymd_and_hms(2026, 9, 12, 12, 0, 0).unwrap(),
            msgid: "m".to_owned(),
            sequence: 2,
        };
        handler
            .cursors
            .save("net", "#c", IrcCaseMapping::default(), true, &saved)
            .expect("save");
        assert_eq!(
            handler.cursors.load("net", "#c", IrcCaseMapping::default(), true),
            Some(saved)
        );
    }
}
