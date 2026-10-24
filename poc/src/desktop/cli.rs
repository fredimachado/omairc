//! The product CLI: `omairc <command>` talks to the running window and prints one JSON line. Port of
//! `src/omairccli.{h,cpp}` (C++). The verbs are `connections` (alias `list`), `status`, `send`,
//! `read`, `names`, `conversations`, and `raise`, and `skills/omairc` documents them. Help and parse
//! errors are text; control answers are the JSON in `ipc`.
//!
//! The binary dispatches here before clap sees the arguments, as Qt's `main` does. `--version` stays
//! with clap, which prints the version it already reads from `version.pri`.

use std::io::{Read, Write};
use std::time::{Duration, Instant};

use chrono::Utc;

use super::endpoint::{self, Endpoint};
use super::ipc::{self, Command, ReadWindow, Request, MAX_LINE_BYTES};

/// How long a connect may take before the window counts as not running (`waitForConnected`).
pub const CONNECT_TIMEOUT: Duration = Duration::from_millis(1000);
/// How long the reply may take after the request is written (`kResponseTimeoutMs`).
pub const RESPONSE_TIMEOUT: Duration = Duration::from_millis(3000);

/// The text a help screen can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    Help,
    Connections,
    Status,
    Send,
    Read,
    Names,
    Conversations,
    Raise,
}

/// What a parsed command line asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// `None` is the overview; `Some` is one command's page.
    Help(Option<Verb>),
    Request(Request),
    Error(String),
}

struct Spec {
    verb: Verb,
    primary: &'static str,
    alias: Option<&'static str>,
    usage: &'static str,
    summary: &'static str,
    detail: &'static str,
}

/// The width of the usage column in the overview (`kOverviewUsageWidth`).
const OVERVIEW_USAGE_WIDTH: usize = 33;

const SPECS: &[Spec] = &[
    Spec {
        verb: Verb::Help,
        primary: "help",
        alias: None,
        usage: "omairc help [command]",
        summary: "Show this help",
        detail: "",
    },
    Spec {
        verb: Verb::Connections,
        primary: "connections",
        alias: Some("list"),
        usage: "omairc connections",
        summary: "List connections as JSON (alias: list)",
        detail: "Usage: omairc connections\n\
            \n\
            List connections as JSON. list is an alias.\n\
            \n\
            Each row has id, name, host, port, tls, nick, state, and selected.\n",
    },
    Spec {
        verb: Verb::Status,
        primary: "status",
        alias: None,
        usage: "omairc status [--network ID]",
        summary: "Show one connection as JSON",
        detail: "Usage: omairc status [--network ID]\n\
            \n\
            Show one connection as JSON.\n\
            \n\
            \x20 --network ID   Connection to use. See connections.\n\
            \n\
            With one connection, --network may be omitted.\n",
    },
    Spec {
        verb: Verb::Send,
        primary: "send",
        alias: None,
        usage: "omairc send [--network ID] TARGET TEXT...",
        summary: "Send a message without changing UI selection",
        detail: "Usage: omairc send [--network ID] TARGET TEXT...\n\
            \n\
            Send a message to a channel or nick without changing the UI selection.\n\
            Quote channel targets. # starts a shell comment.\n\
            Quote TEXT when it contains spaces.\n\
            \n\
            \x20 --network ID   Connection to use. See connections.\n\
            \x20 --             End options. Later args are the target and text.\n\
            \n\
            Examples:\n\
            \x20 omairc send '#channel' hello\n\
            \x20 omairc send '#channel' 'hello there'\n\
            \x20 omairc send --network abc '#channel' hello\n\
            \x20 omairc send '#channel' --help\n\
            \x20 omairc send -- '#channel' --version\n",
    },
    Spec {
        verb: Verb::Read,
        primary: "read",
        alias: None,
        usage: "omairc read [--network ID] [TARGET] [--last N|--since DURATION|--unread]",
        summary: "Snapshot chat as JSON without changing UI selection",
        detail: "Usage: omairc read [--network ID] [TARGET] [--last N|--since DURATION|--unread]\n\
            \n\
            Snapshot recent chat from the running window as JSON.\n\
            \n\
            No target means every channel and DM on that network.\n\
            A named target that is not in the window is an error.\n\
            read does not invent conversations.\n\
            Quote channel targets. # starts a shell comment.\n\
            Output kinds are message, notice, and action. Join and part lines stay out.\n\
            \n\
            \x20 --network ID     Connection to use. See connections.\n\
            \x20 --last N         Newest N lines. Default 50. Maximum 100.\n\
            \x20 --since DURATION Lines in the last window, still capped at 100 newest.\n\
            \x20                  Examples: 5m, 1h. Units are s, m, h, d.\n\
            \x20 --unread         Lines after the CLI cursor for that target, or for the\n\
            \x20                  network when untargeted. Still capped at 100 newest.\n\
            \x20                  Does not change UI selection or GUI unread badges.\n\
            \x20                  Stored under omairc/ in the platform state location;\n\
            \x20                  $XDG_STATE_HOME overrides when set.\n\
            \x20 --               End options. Later args are the target.\n\
            \n\
            Give exactly one window flag. They do not combine.\n\
            When the 100-line cap drops older lines, the JSON includes \"truncated\": true.\n\
            With one connection, --network may be omitted.\n\
            \n\
            Examples:\n\
            \x20 omairc read --last 20\n\
            \x20 omairc read '#channel' --last 20\n\
            \x20 omairc read --network abc nick --since 5m\n\
            \x20 omairc read --unread\n\
            \x20 omairc read '#channel' --unread\n",
    },
    Spec {
        verb: Verb::Names,
        primary: "names",
        alias: None,
        usage: "omairc names [--network ID] TARGET",
        summary: "List members of a joined channel as JSON",
        detail: "Usage: omairc names [--network ID] TARGET\n\
            \n\
            List the current members of a joined channel as JSON.\n\
            This is the member panel snapshot, not a live NAMES round-trip.\n\
            A DM, Status, or a channel that is not joined is an error.\n\
            Quote channel targets. # starts a shell comment.\n\
            \n\
            \x20 --network ID   Connection to use. See connections.\n\
            \x20 --             End options. Later args are the target.\n\
            \n\
            With one connection, --network may be omitted.\n\
            \n\
            Examples:\n\
            \x20 omairc names '#channel'\n\
            \x20 omairc names --network abc '#channel'\n\
            \x20 omairc names -- --dash-nick\n",
    },
    Spec {
        verb: Verb::Conversations,
        primary: "conversations",
        alias: None,
        usage: "omairc conversations [--network ID]",
        summary: "List channels and DMs as JSON",
        detail: "Usage: omairc conversations [--network ID]\n\
            \n\
            List channels and DMs on that network as JSON.\n\
            Each row has target, channel, unread, and mention from the GUI.\n\
            Channel rows also include topic.\n\
            This snapshot does not clear those badges.\n\
            \n\
            \x20 --network ID   Connection to use. See connections.\n\
            \n\
            With one connection, --network may be omitted.\n",
    },
    Spec {
        verb: Verb::Raise,
        primary: "raise",
        alias: None,
        usage: "omairc raise",
        summary: "Activate the existing window",
        detail: "Usage: omairc raise\n\
            \n\
            Activate the existing Omairc window.\n",
    },
];

const OVERVIEW_HEAD: &str = "A simple IRC client for Omarchy.\n\
    \n\
    Usage:\n\
    \x20 omairc [--demo-server]          Open the client window\n\
    \x20 omairc --help                   List commands and GUI flags\n\
    \x20 omairc --version                Print version and exit\n\
    \n\
    Control commands (require a running client):\n";

fn find_spec(token: &str) -> Option<&'static Spec> {
    SPECS
        .iter()
        .find(|spec| spec.primary == token || spec.alias == Some(token))
}

fn spec_for(verb: Verb) -> Option<&'static Spec> {
    SPECS.iter().find(|spec| spec.verb == verb)
}

/// Whether the first argument names a control command or asks for help, so the binary takes the
/// CLI path before it starts the window (`looksLikeCommand`).
pub fn is_control_invocation(args: &[String]) -> bool {
    args.first()
        .is_some_and(|first| first == "--help" || find_spec(first).is_some())
}

fn is_flag(arg: &str) -> bool {
    arg.starts_with('-')
}

fn unexpected(arg: &str) -> Outcome {
    Outcome::Error(format!("Unexpected argument: {arg}"))
}

/// The value after `--network`, or the error `--network` names when there is none.
fn network_value(args: &[String], index: usize) -> Result<String, Outcome> {
    match args.get(index + 1) {
        Some(value) if !is_flag(value) => Ok(value.clone()),
        _ => Err(Outcome::Error("--network requires an id".to_owned())),
    }
}

/// Parses a whole argument list (`parseArgs`). `args[0]` is the command word.
pub fn parse_args(args: &[String]) -> Outcome {
    let Some(first) = args.first() else {
        return Outcome::Error("Missing command".to_owned());
    };
    if first == "--help" {
        return match args.get(1) {
            Some(extra) if args.len() != 1 => unexpected(extra),
            _ => Outcome::Help(None),
        };
    }
    let Some(spec) = find_spec(first) else {
        return Outcome::Error(format!("Unknown command: {first}"));
    };
    match spec.verb {
        Verb::Help => parse_help(args),
        Verb::Connections => no_argument(args, Verb::Connections, Command::Connections),
        Verb::Raise => no_argument(args, Verb::Raise, Command::Raise),
        Verb::Status => parse_network_only(args, Verb::Status, Command::Status),
        Verb::Conversations => parse_network_only(args, Verb::Conversations, Command::Conversations),
        Verb::Send => parse_send(args),
        Verb::Read => parse_read(args),
        Verb::Names => parse_names(args),
    }
}

fn parse_help(args: &[String]) -> Outcome {
    if args.len() == 1 {
        return Outcome::Help(None);
    }
    if args.len() > 2 {
        return Outcome::Error("help takes at most one argument".to_owned());
    }
    let name = &args[1];
    if name == "--help" || name == "help" {
        return Outcome::Help(None);
    }
    match find_spec(name) {
        Some(spec) if spec.verb != Verb::Help => Outcome::Help(Some(spec.verb)),
        _ => Outcome::Error(format!("Unknown command: {name}")),
    }
}

fn no_argument(args: &[String], verb: Verb, command: Command) -> Outcome {
    if args.len() == 1 {
        return Outcome::Request(Request::new(command));
    }
    if args.len() == 2 && args[1] == "--help" {
        return Outcome::Help(Some(verb));
    }
    Outcome::Error(format!("{} takes no arguments", args[0]))
}

fn parse_network_only(args: &[String], verb: Verb, command: Command) -> Outcome {
    let mut request = Request::new(command);
    let mut index = 1;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--help" {
            return Outcome::Help(Some(verb));
        }
        if arg == "--network" {
            match network_value(args, index) {
                Ok(value) => request.network_id = value,
                Err(outcome) => return outcome,
            }
            index += 2;
            continue;
        }
        return unexpected(arg);
    }
    Outcome::Request(request)
}

fn parse_send(args: &[String]) -> Outcome {
    let mut request = Request::new(Command::Send);
    let mut positional: Vec<String> = Vec::new();
    let mut before_target = true;
    let mut index = 1;
    while index < args.len() {
        let arg = &args[index];
        if before_target && arg == "--" {
            before_target = false;
            index += 1;
            continue;
        }
        if before_target && arg == "--network" {
            match network_value(args, index) {
                Ok(value) => request.network_id = value,
                Err(outcome) => return outcome,
            }
            index += 2;
            continue;
        }
        if before_target && arg == "--help" {
            return Outcome::Help(Some(Verb::Send));
        }
        if before_target && is_flag(arg) {
            return Outcome::Error(format!("Unknown option: {arg}"));
        }
        positional.push(arg.clone());
        before_target = false;
        index += 1;
    }
    match positional.len() {
        0 => Outcome::Error("send requires a target and text".to_owned()),
        1 => Outcome::Error("send requires text after the target".to_owned()),
        _ => {
            request.target = positional.remove(0);
            request.text = positional.join(" ");
            Outcome::Request(request)
        }
    }
}

fn window_conflict() -> Outcome {
    Outcome::Error("--last, --since, and --unread cannot be combined".to_owned())
}

fn parse_read(args: &[String]) -> Outcome {
    let mut request = Request::new(Command::Read);
    let mut saw_window = false;
    let mut options_ended = false;
    let mut index = 1;
    while index < args.len() {
        let arg = &args[index];
        if !options_ended && arg == "--" {
            options_ended = true;
            index += 1;
            continue;
        }
        if !options_ended && arg == "--network" {
            match network_value(args, index) {
                Ok(value) => request.network_id = value,
                Err(outcome) => return outcome,
            }
            index += 2;
            continue;
        }
        if !options_ended && arg == "--help" {
            return Outcome::Help(Some(Verb::Read));
        }
        if !options_ended && arg == "--last" {
            if saw_window {
                return window_conflict();
            }
            let Some(value) = args.get(index + 1).filter(|value| !is_flag(value)) else {
                return Outcome::Error("--last requires a count".to_owned());
            };
            let count = value.parse::<i64>().ok().filter(|count| (1..=100).contains(count));
            let Some(count) = count else {
                return Outcome::Error("--last must be between 1 and 100".to_owned());
            };
            request.window = ReadWindow::Last(count as usize);
            saw_window = true;
            index += 2;
            continue;
        }
        if !options_ended && arg == "--since" {
            if saw_window {
                return window_conflict();
            }
            let Some(token) = args.get(index + 1).filter(|value| !is_flag(value)) else {
                return Outcome::Error("--since requires a duration".to_owned());
            };
            let Some(since) = ipc::parse_since(token, Utc::now()) else {
                return Outcome::Error("Invalid --since value".to_owned());
            };
            request.window = ReadWindow::Since(since);
            saw_window = true;
            index += 2;
            continue;
        }
        if !options_ended && arg == "--unread" {
            if saw_window {
                return window_conflict();
            }
            request.window = ReadWindow::Unread;
            saw_window = true;
            index += 1;
            continue;
        }
        if !options_ended && is_flag(arg) {
            return Outcome::Error(format!("Unknown option: {arg}"));
        }
        if !request.target.is_empty() {
            return unexpected(arg);
        }
        request.target = arg.clone();
        index += 1;
    }
    Outcome::Request(request)
}

fn parse_names(args: &[String]) -> Outcome {
    let mut request = Request::new(Command::Names);
    let mut options_ended = false;
    let mut index = 1;
    while index < args.len() {
        let arg = &args[index];
        if !options_ended && arg == "--" {
            options_ended = true;
            index += 1;
            continue;
        }
        if !options_ended && arg == "--help" {
            return Outcome::Help(Some(Verb::Names));
        }
        if !options_ended && arg == "--network" {
            match network_value(args, index) {
                Ok(value) => request.network_id = value,
                Err(outcome) => return outcome,
            }
            index += 2;
            continue;
        }
        if !options_ended && is_flag(arg) {
            return Outcome::Error(format!("Unknown option: {arg}"));
        }
        if !request.target.is_empty() {
            return unexpected(arg);
        }
        request.target = arg.clone();
        index += 1;
    }
    if request.target.is_empty() {
        return Outcome::Error("names requires a target".to_owned());
    }
    Outcome::Request(request)
}

/// The text of a help page (`formatHelp`). Every page ends with a newline.
pub fn format_help(topic: Option<Verb>) -> String {
    let Some(verb) = topic else {
        return overview();
    };
    match spec_for(verb) {
        Some(spec) if spec.verb != Verb::Help => spec.detail.to_owned(),
        _ => overview(),
    }
}

fn overview() -> String {
    let mut text = OVERVIEW_HEAD.to_owned();
    for spec in SPECS.iter().filter(|spec| spec.verb != Verb::Help) {
        text.push_str("  ");
        if spec.usage.len() <= OVERVIEW_USAGE_WIDTH {
            text.push_str(&format!("{:<width$}", spec.usage, width = OVERVIEW_USAGE_WIDTH));
            text.push_str(spec.summary);
            text.push('\n');
        } else {
            text.push_str(spec.usage);
            text.push('\n');
            text.push_str("  ");
            text.push_str(&" ".repeat(OVERVIEW_USAGE_WIDTH));
            text.push_str(spec.summary);
            text.push('\n');
        }
    }
    text.push_str("\nRun 'omairc <command> --help' for command detail.\n");
    text
}

/// Runs one command line against the window at `endpoint`, writing the reply to `out` and the
/// message to `err`. Returns the process exit code: 0 for `ok`, 1 for an error, 2 when a `send`'s
/// delivery is unknown.
pub fn execute(args: &[String], endpoint: &Endpoint, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    match parse_args(args) {
        Outcome::Help(topic) => {
            let _ = out.write_all(format_help(topic).as_bytes());
            0
        }
        Outcome::Error(message) => fail(out, err, &message),
        Outcome::Request(request) => send_request(&request, endpoint, out, err),
    }
}

/// The binary's entry point for a control invocation: stdout and stderr, the app's endpoint.
pub fn run(args: &[String]) -> i32 {
    let endpoint = Endpoint::app();
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let mut out = stdout.lock();
    let mut err = stderr.lock();
    let code = execute(args, &endpoint, &mut out, &mut err);
    let _ = out.flush();
    let _ = err.flush();
    code
}

fn fail(out: &mut dyn Write, err: &mut dyn Write, message: &str) -> i32 {
    let _ = writeln!(out, "{}", ipc::error_response(message));
    let _ = writeln!(err, "{message}");
    1
}

fn uncertain(out: &mut dyn Write, err: &mut dyn Write, message: &str) -> i32 {
    let _ = writeln!(out, "{}", ipc::uncertain_response(message));
    let _ = writeln!(err, "{message}");
    2
}

fn send_request(request: &Request, endpoint: &Endpoint, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let mut stream = match endpoint::connect(endpoint, CONNECT_TIMEOUT) {
        Ok(stream) => stream,
        Err(_) => {
            return fail(
                out,
                err,
                "Omairc is not running (no local socket). Start the client first.",
            )
        }
    };
    let mut payload = ipc::encode_request(request);
    payload.push('\n');
    if stream.write_all(payload.as_bytes()).is_err() {
        return fail(out, err, "Failed to write to Omairc socket");
    }
    stream.set_read_deadline(Some(Instant::now() + RESPONSE_TIMEOUT));
    let reply = read_reply_line(&mut stream);

    if request.command == Command::Send {
        // A send whose reply is missing or not a JSON `ok` may still have been delivered.
        return match reply {
            Some(line) if ipc::response_has_ok(&line) => {
                let _ = writeln!(out, "{line}");
                if ipc::response_ok(&line) {
                    0
                } else {
                    1
                }
            }
            _ => uncertain(
                out,
                err,
                "No response from Omairc after send; the message may already have been delivered",
            ),
        };
    }
    match reply {
        None => fail(out, err, "Invalid or missing response from Omairc"),
        Some(line) if line.is_empty() => fail(out, err, "No response from Omairc"),
        Some(line) => {
            let _ = writeln!(out, "{line}");
            if ipc::response_ok(&line) {
                0
            } else {
                1
            }
        }
    }
}

/// Reads one reply line, trimmed. `None` when the reply times out, is oversized, or the window
/// closes before a newline (`readLine`).
fn read_reply_line(stream: &mut endpoint::Stream) -> Option<String> {
    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return None,
            Ok(read) => {
                buffer.extend_from_slice(&chunk[..read]);
                if let Some(newline) = buffer.iter().position(|byte| *byte == b'\n') {
                    if newline > MAX_LINE_BYTES {
                        return None;
                    }
                    return Some(String::from_utf8_lossy(&buffer[..newline]).trim().to_owned());
                }
                if buffer.len() > MAX_LINE_BYTES {
                    return None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_owned()).collect()
    }

    fn request_of(outcome: Outcome) -> Request {
        match outcome {
            Outcome::Request(request) => request,
            other => panic!("expected a request, got {other:?}"),
        }
    }

    fn error_of(outcome: Outcome) -> String {
        match outcome {
            Outcome::Error(message) => message,
            other => panic!("expected an error, got {other:?}"),
        }
    }

    #[test]
    fn the_overview_help_matches_qt_exactly() {
        assert_eq!(
            format_help(None),
            "A simple IRC client for Omarchy.\n\
             \n\
             Usage:\n\
             \x20 omairc [--demo-server]          Open the client window\n\
             \x20 omairc --help                   List commands and GUI flags\n\
             \x20 omairc --version                Print version and exit\n\
             \n\
             Control commands (require a running client):\n\
             \x20 omairc connections               List connections as JSON (alias: list)\n\
             \x20 omairc status [--network ID]     Show one connection as JSON\n\
             \x20 omairc send [--network ID] TARGET TEXT...\n\
             \x20                                  Send a message without changing UI selection\n\
             \x20 omairc read [--network ID] [TARGET] [--last N|--since DURATION|--unread]\n\
             \x20                                  Snapshot chat as JSON without changing UI selection\n\
             \x20 omairc names [--network ID] TARGET\n\
             \x20                                  List members of a joined channel as JSON\n\
             \x20 omairc conversations [--network ID]\n\
             \x20                                  List channels and DMs as JSON\n\
             \x20 omairc raise                     Activate the existing window\n\
             \n\
             Run 'omairc <command> --help' for command detail.\n"
        );
        assert_eq!(parse_args(&args(&["--help"])), Outcome::Help(None));
        assert_eq!(error_of(parse_args(&args(&["--help", "foo"]))), "Unexpected argument: foo");
    }

    #[test]
    fn the_connections_page_names_its_fields() {
        assert_eq!(
            parse_args(&args(&["connections", "--help"])),
            Outcome::Help(Some(Verb::Connections))
        );
        assert_eq!(
            format_help(Some(Verb::Connections)),
            "Usage: omairc connections\n\
             \n\
             List connections as JSON. list is an alias.\n\
             \n\
             Each row has id, name, host, port, tls, nick, state, and selected.\n"
        );
        assert_eq!(parse_args(&args(&["list", "--help"])), Outcome::Help(Some(Verb::Connections)));
    }

    #[test]
    fn help_takes_one_command_name() {
        assert_eq!(parse_args(&args(&["help", "send"])), Outcome::Help(Some(Verb::Send)));
        assert_eq!(parse_args(&args(&["help"])), Outcome::Help(None));
        assert_eq!(
            error_of(parse_args(&args(&["help", "nope"]))),
            "Unknown command: nope"
        );
        assert_eq!(
            error_of(parse_args(&args(&["help", "a", "b"]))),
            "help takes at most one argument"
        );
    }

    #[test]
    fn the_status_page_and_no_argument_commands() {
        assert_eq!(
            format_help(Some(Verb::Status)),
            "Usage: omairc status [--network ID]\n\
             \n\
             Show one connection as JSON.\n\
             \n\
             \x20 --network ID   Connection to use. See connections.\n\
             \n\
             With one connection, --network may be omitted.\n"
        );
        assert_eq!(
            format_help(Some(Verb::Raise)),
            "Usage: omairc raise\n\
             \n\
             Activate the existing Omairc window.\n"
        );
        assert_eq!(error_of(parse_args(&args(&["raise", "now"]))), "raise takes no arguments");
        assert_eq!(request_of(parse_args(&args(&["raise"]))).command, Command::Raise);
    }

    #[test]
    fn status_takes_an_optional_network() {
        let request = request_of(parse_args(&args(&["status", "--network", "abc"])));
        assert_eq!(request.command, Command::Status);
        assert_eq!(request.network_id, "abc");
        assert_eq!(
            error_of(parse_args(&args(&["status", "--network", "--help"]))),
            "--network requires an id"
        );
        assert_eq!(error_of(parse_args(&args(&["status", "extra"]))), "Unexpected argument: extra");
    }

    #[test]
    fn send_help_is_a_flag_only_before_the_target() {
        assert_eq!(parse_args(&args(&["send", "--help"])), Outcome::Help(Some(Verb::Send)));
        assert_eq!(
            format_help(Some(Verb::Send)),
            "Usage: omairc send [--network ID] TARGET TEXT...\n\
             \n\
             Send a message to a channel or nick without changing the UI selection.\n\
             Quote channel targets. # starts a shell comment.\n\
             Quote TEXT when it contains spaces.\n\
             \n\
             \x20 --network ID   Connection to use. See connections.\n\
             \x20 --             End options. Later args are the target and text.\n\
             \n\
             Examples:\n\
             \x20 omairc send '#channel' hello\n\
             \x20 omairc send '#channel' 'hello there'\n\
             \x20 omairc send --network abc '#channel' hello\n\
             \x20 omairc send '#channel' --help\n\
             \x20 omairc send -- '#channel' --version\n"
        );
        let text = request_of(parse_args(&args(&["send", "#chan", "--help"])));
        assert_eq!(text.target, "#chan");
        assert_eq!(text.text, "--help");
    }

    #[test]
    fn send_joins_the_text_and_keeps_dash_prefixed_words() {
        let plain = request_of(parse_args(&args(&["send", "#chan", "-hello"])));
        assert_eq!(plain.text, "-hello");
        let spaced = request_of(parse_args(&args(&["send", "#chan", "hello", "there"])));
        assert_eq!(spaced.text, "hello there");
        let after_dashes = request_of(parse_args(&args(&[
            "send",
            "--",
            "#chan",
            "--network",
            "not-an-option",
        ])));
        assert_eq!(after_dashes.target, "#chan");
        assert_eq!(after_dashes.text, "--network not-an-option");
        let network = request_of(parse_args(&args(&["send", "--network", "abc", "#chan", "hi"])));
        assert_eq!(network.network_id, "abc");
    }

    #[test]
    fn send_refuses_a_missing_target_or_text() {
        assert_eq!(error_of(parse_args(&args(&["send"]))), "send requires a target and text");
        assert_eq!(
            error_of(parse_args(&args(&["send", "#chan"]))),
            "send requires text after the target"
        );
        assert_eq!(
            error_of(parse_args(&args(&["send", "--bogus", "#chan", "x"]))),
            "Unknown option: --bogus"
        );
    }

    #[test]
    fn the_read_page_and_defaults() {
        assert_eq!(parse_args(&args(&["read", "--help"])), Outcome::Help(Some(Verb::Read)));
        assert_eq!(
            format_help(Some(Verb::Read)),
            r#"Usage: omairc read [--network ID] [TARGET] [--last N|--since DURATION|--unread]

Snapshot recent chat from the running window as JSON.

No target means every channel and DM on that network.
A named target that is not in the window is an error.
read does not invent conversations.
Quote channel targets. # starts a shell comment.
Output kinds are message, notice, and action. Join and part lines stay out.

  --network ID     Connection to use. See connections.
  --last N         Newest N lines. Default 50. Maximum 100.
  --since DURATION Lines in the last window, still capped at 100 newest.
                   Examples: 5m, 1h. Units are s, m, h, d.
  --unread         Lines after the CLI cursor for that target, or for the
                   network when untargeted. Still capped at 100 newest.
                   Does not change UI selection or GUI unread badges.
                   Stored under omairc/ in the platform state location;
                   $XDG_STATE_HOME overrides when set.
  --               End options. Later args are the target.

Give exactly one window flag. They do not combine.
When the 100-line cap drops older lines, the JSON includes "truncated": true.
With one connection, --network may be omitted.

Examples:
  omairc read --last 20
  omairc read '#channel' --last 20
  omairc read --network abc nick --since 5m
  omairc read --unread
  omairc read '#channel' --unread
"#
        );
        let default = request_of(parse_args(&args(&["read"])));
        assert_eq!(default.command, Command::Read);
        assert_eq!(default.target, "");
        assert_eq!(default.window, ReadWindow::Last(50));
    }

    #[test]
    fn read_windows_take_one_flag_and_a_bounded_count() {
        let last = request_of(parse_args(&args(&["read", "--last", "20", "#chan"])));
        assert_eq!(last.target, "#chan");
        assert_eq!(last.window, ReadWindow::Last(20));
        let after_target = request_of(parse_args(&args(&["read", "#chan", "--unread"])));
        assert_eq!(after_target.window, ReadWindow::Unread);
        assert!(error_of(parse_args(&args(&["read", "--last", "101"]))).contains("100"));
        assert_eq!(
            error_of(parse_args(&args(&["read", "--unread", "--last", "1"]))),
            "--last, --since, and --unread cannot be combined"
        );
        assert_eq!(error_of(parse_args(&args(&["read", "--last"]))), "--last requires a count");
        assert_eq!(
            error_of(parse_args(&args(&["read", "--last", "0"]))),
            "--last must be between 1 and 100"
        );
    }

    #[test]
    fn read_since_parses_its_duration() {
        let since = request_of(parse_args(&args(&["read", "--since", "5m"])));
        match since.window {
            ReadWindow::Since(window) => assert_eq!(window.token, "5m"),
            other => panic!("expected since, got {other:?}"),
        }
        assert_eq!(
            error_of(parse_args(&args(&["read", "--since", "no"]))),
            "Invalid --since value"
        );
        assert_eq!(
            error_of(parse_args(&args(&["read", "--since", "9999999999999999s"]))),
            "Invalid --since value"
        );
        assert_eq!(
            error_of(parse_args(&args(&["read", "--since"]))),
            "--since requires a duration"
        );
    }

    #[test]
    fn names_needs_a_target_and_takes_dash_targets_after_separator() {
        assert_eq!(parse_args(&args(&["names", "--help"])), Outcome::Help(Some(Verb::Names)));
        assert_eq!(error_of(parse_args(&args(&["names"]))), "names requires a target");
        let names = request_of(parse_args(&args(&["names", "#chan"])));
        assert_eq!(names.command, Command::Names);
        assert_eq!(names.target, "#chan");
        let dash = request_of(parse_args(&args(&["names", "--", "--dash-nick"])));
        assert_eq!(dash.target, "--dash-nick");
        assert_eq!(
            format_help(Some(Verb::Names)),
            "Usage: omairc names [--network ID] TARGET\n\
             \n\
             List the current members of a joined channel as JSON.\n\
             This is the member panel snapshot, not a live NAMES round-trip.\n\
             A DM, Status, or a channel that is not joined is an error.\n\
             Quote channel targets. # starts a shell comment.\n\
             \n\
             \x20 --network ID   Connection to use. See connections.\n\
             \x20 --             End options. Later args are the target.\n\
             \n\
             With one connection, --network may be omitted.\n\
             \n\
             Examples:\n\
             \x20 omairc names '#channel'\n\
             \x20 omairc names --network abc '#channel'\n\
             \x20 omairc names -- --dash-nick\n"
        );
    }

    #[test]
    fn conversations_takes_only_the_network() {
        assert_eq!(
            format_help(Some(Verb::Conversations)),
            "Usage: omairc conversations [--network ID]\n\
             \n\
             List channels and DMs on that network as JSON.\n\
             Each row has target, channel, unread, and mention from the GUI.\n\
             Channel rows also include topic.\n\
             This snapshot does not clear those badges.\n\
             \n\
             \x20 --network ID   Connection to use. See connections.\n\
             \n\
             With one connection, --network may be omitted.\n"
        );
        let conversations = request_of(parse_args(&args(&["conversations", "--network", "n"])));
        assert_eq!(conversations.command, Command::Conversations);
        assert_eq!(conversations.network_id, "n");
        assert_eq!(
            parse_args(&args(&["conversations", "--help"])),
            Outcome::Help(Some(Verb::Conversations))
        );
        assert_eq!(
            error_of(parse_args(&args(&["conversations", "x"]))),
            "Unexpected argument: x"
        );
    }

    #[test]
    fn unknown_and_empty_commands_are_errors() {
        assert_eq!(error_of(parse_args(&args(&["nope"]))), "Unknown command: nope");
        assert_eq!(error_of(parse_args(&[])), "Missing command");
    }

    #[test]
    fn a_send_whose_reply_is_lost_is_uncertain_and_exits_two() {
        let endpoint = Endpoint::named(&format!(
            "{}-cli-uncertain-{}",
            crate::identity::APP_FOLDER,
            std::process::id()
        ));
        let mut listener = endpoint::Listener::bind(&endpoint).expect("the test endpoint is free");
        let server = std::thread::spawn(move || {
            let mut stream = listener.accept().expect("a client connects");
            let mut byte = [0u8; 1];
            loop {
                match stream.read(&mut byte) {
                    Ok(1) if byte[0] != b'\n' => continue,
                    _ => break,
                }
            }
            // The request is read and the stream dropped with no reply, which the client sees as lost.
        });
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = execute(&args(&["send", "#chan", "hi"]), &endpoint, &mut out, &mut err);
        server.join().expect("the server thread");
        assert_eq!(code, 2);
        assert_eq!(
            String::from_utf8(out).expect("utf-8"),
            format!(
                "{}\n",
                ipc::uncertain_response(
                    "No response from Omairc after send; the message may already have been delivered"
                )
            )
        );
        assert_eq!(
            String::from_utf8(err).expect("utf-8"),
            "No response from Omairc after send; the message may already have been delivered\n"
        );
    }

    #[test]
    fn a_window_that_is_not_running_is_reported_as_such() {
        let endpoint = Endpoint::named(&format!(
            "{}-cli-absent-{}",
            crate::identity::APP_FOLDER,
            std::process::id()
        ));
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = execute(&args(&["connections"]), &endpoint, &mut out, &mut err);
        assert_eq!(code, 1);
        let message = "Omairc is not running (no local socket). Start the client first.";
        assert_eq!(
            String::from_utf8(out).expect("utf-8"),
            format!("{}\n", ipc::error_response(message))
        );
        assert_eq!(String::from_utf8(err).expect("utf-8"), format!("{message}\n"));
    }

    #[test]
    fn control_invocations_are_the_verbs_and_help() {
        assert!(is_control_invocation(&args(&["list"])));
        assert!(is_control_invocation(&args(&["conversations"])));
        assert!(is_control_invocation(&args(&["--help"])));
        assert!(!is_control_invocation(&args(&["--demo-server"])));
        assert!(!is_control_invocation(&args(&["--version"])));
        assert!(!is_control_invocation(&[]));
    }
}
