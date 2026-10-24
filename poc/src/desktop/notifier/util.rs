//! Shared helpers for toast XML and activation URLs. Ports the free functions in
//! `src/windowsnotifications.cpp`.

#[cfg(any(windows, test))]
use std::path::{Path, PathBuf};

use sha1_smol::Sha1;
use url::form_urlencoded;

/// `network_id` and `target` joined the way Qt and the TUI store notification keys.
pub fn conversation_key(network_id: &str, target: &str) -> String {
    format!("{network_id}\n{target}")
}

/// The first sixteen hex digits of SHA-1(`conversation_key`), for toast tag/group.
pub fn conversation_tag(network_id: &str, target: &str) -> String {
    let digest = Sha1::from(conversation_key(network_id, target).as_bytes()).digest();
    digest
        .bytes()
        .iter()
        .take(8)
        .fold(String::new(), |tag, byte| format!("{tag}{byte:02x}"))
}

/// XML 1.0 text content allowed in toast bodies (C0 controls stripped).
pub fn sanitize_xml_text(value: &str) -> String {
    let mut sanitized = String::with_capacity(value.len());
    let chars: Vec<char> = value.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let code = chars[index];
        let scalar = code as u32;
        let allowed = scalar == 0x09
            || scalar == 0x0A
            || scalar == 0x0D
            || (scalar >= 0x20 && scalar <= 0xD7FF)
            || (scalar >= 0xE000 && scalar <= 0xFFFD);
        if allowed {
            sanitized.push(code);
            index += 1;
            continue;
        }
        if (0xD800..=0xDBFF).contains(&scalar)
            && index + 1 < chars.len()
            && (0xDC00..=0xDFFF).contains(&(chars[index + 1] as u32))
        {
            sanitized.push(chars[index]);
            sanitized.push(chars[index + 1]);
            index += 2;
            continue;
        }
        index += 1;
    }
    sanitized
}

/// Escapes `value` for a toast XML attribute or text node.
pub fn escape_xml(value: &str) -> String {
    let mut escaped = sanitize_xml_text(value);
    escaped = escaped.replace('&', "&amp;");
    escaped = escaped.replace('<', "&lt;");
    escaped = escaped.replace('>', "&gt;");
    escaped = escaped.replace('"', "&quot;");
    escaped = escaped.replace('\'', "&apos;");
    escaped = escaped.replace('\n', "&#10;");
    escaped = escaped.replace('\r', "&#13;");
    escaped
}

/// The `launch` attribute for a Windows toast (`omairc://notify?...`).
pub fn build_launch_url(network_id: &str, target: &str, msgid: &str) -> String {
    let mut serializer = form_urlencoded::Serializer::new(String::from("omairc://notify?"));
    serializer.append_pair("network", network_id);
    serializer.append_pair("target", target);
    serializer.append_pair("msgid", msgid);
    serializer.finish()
}

/// Parses `launch` from a toast activation or foreground launch.
pub fn parse_launch_url(launch: &str) -> Option<(String, String, String)> {
    let url = url::Url::parse(launch).ok()?;
    if url.scheme() != "omairc" || url.host_str() != Some("notify") {
        return None;
    }
    let mut network_id = String::new();
    let mut target = String::new();
    let mut msgid = String::new();
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "network" => network_id = value.into_owned(),
            "target" => target = value.into_owned(),
            "msgid" => msgid = value.into_owned(),
            _ => {}
        }
    }
    if network_id.is_empty() || target.is_empty() {
        return None;
    }
    Some((network_id, target, msgid))
}

/// Records a freedesktop notification id for a conversation, replacing any prior id.
pub fn remember_conversation_notify_id(
    conversation_ids: &mut std::collections::HashMap<String, u32>,
    by_id: &mut std::collections::HashMap<u32, (String, String, String)>,
    network_id: &str,
    target: &str,
    msgid: &str,
    id: u32,
) {
    if network_id.is_empty() || target.is_empty() || id == 0 {
        return;
    }
    let key = conversation_key(network_id, target);
    let previous = conversation_ids.get(&key).copied().unwrap_or(0);
    if previous != 0 && previous != id {
        by_id.remove(&previous);
    }
    conversation_ids.insert(key, id);
    by_id.insert(
        id,
        (network_id.to_owned(), target.to_owned(), msgid.to_owned()),
    );
}

/// Generic toast XML for Windows (`ToastGeneric` template).
pub fn build_toast_xml(summary: &str, body: &str, launch: &str) -> String {
    format!(
        "<toast activationType=\"foreground\" launch=\"{}\"><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>",
        escape_xml(launch),
        escape_xml(summary),
        escape_xml(body)
    )
}

/// Whether `this_exe` may take the shared toast record (the activator's `LocalServer32`
/// value, or the Start Menu link) away from `existing`. A missing record is claimed, and so
/// is one that already names `this_exe` or an exe that no longer exists. Any other exe still
/// on disk keeps it, so an installed copy keeps its toasts and clicks.
#[cfg(any(windows, test))]
pub fn should_claim(
    existing: Option<&Path>,
    this_exe: &Path,
    exists: impl Fn(&Path) -> bool,
) -> bool {
    match existing {
        None => true,
        Some(existing) => same_exe(existing, this_exe) || !exists(existing),
    }
}

/// Whether two exe paths name one file. Windows paths ignore case, so the match does too.
#[cfg(any(windows, test))]
fn same_exe(left: &Path, right: &Path) -> bool {
    left == right
        || left
            .to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
}

/// The exe a `LocalServer32` command line names: the quoted first word, or the whole value
/// when it is unquoted. `None` when the value names nothing.
#[cfg(any(windows, test))]
pub fn command_exe(command: &str) -> Option<PathBuf> {
    let command = command.trim();
    let exe = match command.strip_prefix('"') {
        Some(quoted) => quoted.split('"').next().unwrap_or(""),
        None => command,
    };
    (!exe.is_empty()).then(|| PathBuf::from(exe))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn sanitize_xml_text_strips_controls() {
        assert_eq!(sanitize_xml_text("hello\x07world"), "helloworld");
        assert_eq!(sanitize_xml_text("tab\there"), "tab\there");
    }

    #[test]
    fn escape_xml_quotes_entities() {
        assert_eq!(escape_xml("a & b"), "a &amp; b");
        assert_eq!(escape_xml("line\nbreak"), "line&#10;break");
    }

    #[test]
    fn conversation_tag_is_stable_hex() {
        let tag = conversation_tag("net", "#chan");
        assert_eq!(tag.len(), 16);
        assert!(tag.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(tag, conversation_tag("net", "#chan"));
    }

    #[test]
    fn launch_url_round_trips() {
        let url = build_launch_url("omarchy", "#help", "abc-123");
        let (network, target, msgid) = parse_launch_url(&url).expect("parse");
        assert_eq!(network, "omarchy");
        assert_eq!(target, "#help");
        assert_eq!(msgid, "abc-123");
    }

    #[test]
    fn parse_launch_url_requires_network_and_target() {
        assert!(parse_launch_url("omairc://notify?msgid=x").is_none());
    }

    #[test]
    fn should_claim_takes_missing_or_stale_records_and_keeps_a_live_owner() {
        let this_exe = Path::new("/dev/omairc");
        let installed = Path::new("/opt/omairc");
        assert!(should_claim(None, this_exe, |_| true));
        assert!(should_claim(Some(this_exe), this_exe, |_| true));
        assert!(should_claim(Some(installed), this_exe, |_| false));
        assert!(!should_claim(Some(installed), this_exe, |_| true));
    }

    #[test]
    fn should_claim_matches_windows_paths_without_case() {
        let this_exe = Path::new("C:\\Apps\\omairc.exe");
        let recorded = Path::new("c:\\apps\\OMAIRC.exe");
        assert!(should_claim(Some(recorded), this_exe, |_| true));
    }

    #[test]
    fn command_exe_reads_quoted_and_bare_commands() {
        assert_eq!(
            command_exe("\"C:\\Program Files\\Omairc\\omairc.exe\""),
            Some(PathBuf::from("C:\\Program Files\\Omairc\\omairc.exe"))
        );
        assert_eq!(
            command_exe("C:\\Omairc\\omairc.exe"),
            Some(PathBuf::from("C:\\Omairc\\omairc.exe"))
        );
        assert_eq!(command_exe("  \"\"  "), None);
        assert_eq!(command_exe(""), None);
    }

    #[test]
    fn remember_conversation_notify_id_replaces_prior_id() {
        let mut conversation_ids = HashMap::new();
        let mut by_id = HashMap::new();
        remember_conversation_notify_id(
            &mut conversation_ids,
            &mut by_id,
            "net",
            "#chan",
            "a",
            1,
        );
        remember_conversation_notify_id(
            &mut conversation_ids,
            &mut by_id,
            "net",
            "#chan",
            "b",
            2,
        );
        assert_eq!(conversation_ids.get("net\n#chan"), Some(&2));
        assert!(!by_id.contains_key(&1));
        assert_eq!(
            by_id.get(&2),
            Some(&("net".to_owned(), "#chan".to_owned(), "b".to_owned()))
        );
    }
}
