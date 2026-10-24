//! Posts one local file to a FILEHOST endpoint and returns the absolute link.
//! Ports `src/irc/ircfilehost.cpp` and `tui/internal/filehost/upload.go`.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use mime_guess::from_path;

use crate::controller::fileupload::UploadTarget;
use crate::desktop::httpfetch::{HttpFetch, PostRequest};
use crate::irc::filelink::{absolute_file_link, basic_authorization_value, file_host_sends_basic_auth};

pub const COULD_NOT_UPLOAD: &str = "Could not upload the file.";
pub const FILE_TOO_LARGE: &str = "The file is too large.";
pub const FILE_EMPTY: &str = "The file is empty.";
pub const NOT_A_FILE: &str = "That is not a file.";
const MAXIMUM_BYTES: u64 = 32 * 1024 * 1024;
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(60);

/// One file the composer asked to upload.
#[derive(Clone, Debug)]
pub struct UploadJob {
    pub source: UploadSource,
    pub file_name: String,
    pub content_type: String,
    pub target: UploadTarget,
}

#[derive(Clone, Debug)]
pub enum UploadSource {
    Path(PathBuf),
    Bytes(Vec<u8>),
}

/// Posts the file and returns the absolute http(s) link, or a fixed failure string.
pub fn upload(fetch: &dyn HttpFetch, mut job: UploadJob) -> Result<String, &'static str> {
    let (body, file_name, content_type) = read_source(&job)?;
    let endpoint = job.target.endpoint.clone();
    let endpoint_ref = endpoint.as_str();
    if url::Url::parse(endpoint_ref)
        .ok()
        .is_none_or(|parsed| parsed.scheme().is_empty())
    {
        return Err(COULD_NOT_UPLOAD);
    }
    if job.target.server_encrypted
        && !endpoint_ref
            .to_ascii_lowercase()
            .starts_with("https://")
    {
        return Err(COULD_NOT_UPLOAD);
    }
    let file_name = if file_name.is_empty() {
        job.file_name.clone()
    } else {
        file_name
    };
    let content_type = if content_type.is_empty() {
        job.content_type.clone()
    } else {
        content_type
    };
    let mut headers = vec![
        ("Content-Type".to_owned(), content_type),
        (
            "Content-Disposition".to_owned(),
            format!("attachment; filename=\"{}\"", safe_file_name(&file_name)),
        ),
    ];
    if file_host_sends_basic_auth(
        &job.target.server_host,
        endpoint_ref,
        job.target.server_encrypted,
    ) {
        if let Some(header) =
            basic_authorization_value(&job.target.user, job.target.secret.expose())
        {
            headers.push(("Authorization".to_owned(), header));
        }
    }
    job.target.wipe_credentials();
    let request = PostRequest {
        url: endpoint.clone(),
        headers,
        timeout: TRANSFER_TIMEOUT,
        body,
    };
    let reply = fetch.post(&request).map_err(|_| COULD_NOT_UPLOAD)?;
    if reply.status != 201 {
        return Err(COULD_NOT_UPLOAD);
    }
    let location = reply.location.as_deref().unwrap_or("");
    absolute_file_link(&endpoint, location).ok_or(COULD_NOT_UPLOAD)
}

fn read_source(job: &UploadJob) -> Result<(Vec<u8>, String, String), &'static str> {
    match &job.source {
        UploadSource::Path(path) => read_path(path, &job.content_type),
        UploadSource::Bytes(bytes) => {
            if bytes.is_empty() {
                return Err(FILE_EMPTY);
            }
            if bytes.len() as u64 > MAXIMUM_BYTES {
                return Err(FILE_TOO_LARGE);
            }
            let content_type = if job.content_type.is_empty() {
                "application/octet-stream".to_owned()
            } else {
                job.content_type.clone()
            };
            Ok((
                bytes.clone(),
                safe_file_name(&job.file_name),
                content_type,
            ))
        }
    }
}

fn read_path(path: &Path, override_type: &str) -> Result<(Vec<u8>, String, String), &'static str> {
    let info = fs::metadata(path).map_err(|_| NOT_A_FILE)?;
    if !info.is_file() {
        return Err(NOT_A_FILE);
    }
    if info.len() == 0 {
        return Err(FILE_EMPTY);
    }
    if info.len() > MAXIMUM_BYTES {
        return Err(FILE_TOO_LARGE);
    }
    let mut file = fs::File::open(path).map_err(|_| COULD_NOT_UPLOAD)?;
    let mut body = Vec::new();
    file.read_to_end(&mut body).map_err(|_| COULD_NOT_UPLOAD)?;
    if body.is_empty() {
        return Err(FILE_EMPTY);
    }
    if body.len() as u64 > MAXIMUM_BYTES {
        return Err(FILE_TOO_LARGE);
    }
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_owned());
    let file_name = safe_file_name(&name);
    let content_type = content_type_for(override_type, Path::new(&file_name), &body);
    Ok((body, file_name, content_type))
}

pub fn safe_file_name(name: &str) -> String {
    let base = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(name);
    let mut out = String::new();
    for character in base.chars() {
        if character < '\x20' || character == '"' || character == '\\' || character == '/' || character == ';' {
            out.push('_');
        } else {
            out.push(character);
        }
    }
    if out.is_empty() {
        return "file".to_owned();
    }
    let char_count = out.chars().count();
    if char_count > 180 {
        out = out.chars().skip(char_count - 180).collect();
    }
    out
}

fn content_type_for(override_type: &str, path: &Path, body: &[u8]) -> String {
    if !override_type.is_empty() {
        return override_type.to_owned();
    }
    if let Some(sniffed) = sniff_content_type(body) {
        return sniffed.to_owned();
    }
    from_path(path)
        .first_raw()
        .unwrap_or("application/octet-stream")
        .to_owned()
}

/// Magic-byte sniff for the upload types that matter without a new crate.
fn sniff_content_type(body: &[u8]) -> Option<&'static str> {
    if body.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        return Some("image/png");
    }
    if body.len() >= 3 && body[0] == 0xff && body[1] == 0xd8 && body[2] == 0xff {
        return Some("image/jpeg");
    }
    if body.starts_with(b"GIF87a") || body.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if body.starts_with(b"%PDF") {
        return Some("application/pdf");
    }
    if body.len() >= 12 && body.starts_with(b"RIFF") && &body[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if body.starts_with(b"PK\x03\x04")
        || body.starts_with(b"PK\x05\x06")
        || body.starts_with(b"PK\x07\x08")
    {
        return Some("application/zip");
    }
    if body.starts_with(&[0x1f, 0x8b]) {
        return Some("application/gzip");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::httpfetch::FakeFetch;
    use crate::irc::filelink::basic_authorization_value;
    use crate::util::Secret;
    use crate::util::testdir::TempDir;

    fn target(endpoint: &str, encrypted: bool) -> UploadTarget {
        UploadTarget {
            endpoint: endpoint.to_owned(),
            user: "seunghye".to_owned(),
            secret: Secret::from("no".to_owned()),
            server_host: "irc.example".to_owned(),
            server_encrypted: encrypted,
        }
    }

    #[test]
    fn wiping_clears_the_secret_and_user_name_that_drop_runs() {
        let mut target = target("http://127.0.0.1/upload", false);
        target.wipe_credentials();
        assert!(target.secret.is_empty());
        assert!(target.user.is_empty());
    }

    #[test]
    fn safe_file_name_sanitizes() {
        assert_eq!(safe_file_name("note.txt"), "note.txt");
        assert_eq!(safe_file_name("bad\"chars;.txt"), "bad_chars_.txt");
    }

    #[test]
    fn safe_file_name_truncates_by_character_count() {
        let long = "あ".repeat(200);
        let truncated = safe_file_name(&long);
        assert_eq!(truncated.chars().count(), 180);
        assert_eq!(truncated, "あ".repeat(180));
    }

    #[test]
    fn upload_posts_with_auth_headers_and_body() {
        let fake = FakeFetch::new();
        let url = "http://127.0.0.1/upload";
        fake.serve_post(url, 201, Some("/files/note.txt"));
        let dir = TempDir::new("fileupload");
        let path = dir.path().join("note.txt");
        fs::write(&path, b"hello file").expect("write");
        let link = upload(
            &fake,
            UploadJob {
                source: UploadSource::Path(path),
                file_name: String::new(),
                content_type: String::new(),
                target: target(url, false),
            },
        )
        .expect("uploaded");
        assert_eq!(link, "http://127.0.0.1/files/note.txt");
        let posts = fake.post_requests();
        assert_eq!(posts.len(), 1);
        let head = posts[0].headers.clone();
        assert!(head.iter().any(|(name, value)| name == "Content-Type" && value.contains("text")));
        assert!(head.iter().any(|(name, _)| name == "Content-Disposition"));
        let auth = head
            .iter()
            .find(|(name, _)| name == "Authorization")
            .map(|(_, value)| value.clone())
            .expect("auth");
        assert_eq!(auth, basic_authorization_value("seunghye", "no").expect("auth"));
        assert_eq!(posts[0].body, b"hello file");
    }

    #[test]
    fn upload_refuses_cleartext_on_tls_irc() {
        let fake = FakeFetch::new();
        let url = "http://127.0.0.1/upload";
        fake.serve_post(url, 201, Some("/x"));
        let dir = TempDir::new("fileupload");
        let path = dir.path().join("note.txt");
        fs::write(&path, b"x").expect("write");
        let err = upload(
            &fake,
            UploadJob {
                source: UploadSource::Path(path),
                file_name: String::new(),
                content_type: String::new(),
                target: target(url, true),
            },
        )
        .unwrap_err();
        assert_eq!(err, COULD_NOT_UPLOAD);
        assert!(fake.post_requests().is_empty());
    }

    #[test]
    fn upload_rejects_empty_directory_and_oversize() {
        let fake = FakeFetch::new();
        let url = "http://127.0.0.1/upload";
        let dir = TempDir::new("fileupload-dir");
        assert_eq!(
            upload(
                &fake,
                UploadJob {
                    source: UploadSource::Path(dir.path().to_path_buf()),
                    file_name: String::new(),
                    content_type: String::new(),
                    target: target(url, false),
                },
            )
            .unwrap_err(),
            NOT_A_FILE
        );
        let empty = dir.path().join("empty.txt");
        fs::write(&empty, []).expect("write");
        assert_eq!(
            upload(
                &fake,
                UploadJob {
                    source: UploadSource::Path(empty),
                    file_name: String::new(),
                    content_type: String::new(),
                    target: target(url, false),
                },
            )
            .unwrap_err(),
            FILE_EMPTY
        );
        let huge = dir.path().join("huge.bin");
        fs::write(&huge, vec![0u8; (MAXIMUM_BYTES as usize) + 1]).expect("write");
        assert_eq!(
            upload(
                &fake,
                UploadJob {
                    source: UploadSource::Path(huge),
                    file_name: String::new(),
                    content_type: String::new(),
                    target: target(url, false),
                },
            )
            .unwrap_err(),
            FILE_TOO_LARGE
        );
    }

    #[test]
    fn upload_redirect_is_not_followed() {
        let fake = FakeFetch::new();
        let url = "http://127.0.0.1/upload";
        fake.serve_post(url, 302, Some("https://elsewhere.example/secret-path"));
        let dir = TempDir::new("fileupload");
        let path = dir.path().join("note.txt");
        fs::write(&path, b"x").expect("write");
        let err = upload(
            &fake,
            UploadJob {
                source: UploadSource::Path(path),
                file_name: String::new(),
                content_type: String::new(),
                target: UploadTarget {
                    endpoint: url.to_owned(),
                    user: "alice".to_owned(),
                    secret: Secret::from("s3cret-token".to_owned()),
                    server_host: "irc.example".to_owned(),
                    server_encrypted: false,
                },
            },
        )
        .unwrap_err();
        assert_eq!(err, COULD_NOT_UPLOAD);
        let requests = fake.post_requests();
        assert_eq!(requests.len(), 1);
        let joined = format!("{requests:?}");
        assert!(!joined.contains("s3cret-token"));
        assert!(!joined.contains("secret-path"));
    }

    #[test]
    fn failure_text_never_contains_the_secret() {
        let fake = FakeFetch::new();
        let url = "http://127.0.0.1/upload";
        fake.serve_post(url, 403, None);
        let dir = TempDir::new("fileupload");
        let path = dir.path().join("note.txt");
        fs::write(&path, b"x").expect("write");
        let err = upload(
            &fake,
            UploadJob {
                source: UploadSource::Path(path),
                file_name: String::new(),
                content_type: String::new(),
                target: UploadTarget {
                    endpoint: url.to_owned(),
                    user: "alice".to_owned(),
                    secret: Secret::from("s3cret-token".to_owned()),
                    server_host: "irc.example".to_owned(),
                    server_encrypted: false,
                },
            },
        )
        .unwrap_err();
        assert_eq!(err, COULD_NOT_UPLOAD);
        let requests = fake.post_requests();
        let joined = format!("{requests:?}");
        assert!(!joined.contains("s3cret-token"));
    }

    #[test]
    fn ureq_upload_reads_created_location() {
        use crate::desktop::httpfetch::{serve_once, UreqFetch};
        let (url, server) = serve_once(
            "201 Created",
            "Location: /files/note.txt\r\n",
            Vec::new(),
        );
        let dir = TempDir::new("fileupload-ureq");
        let path = dir.path().join("note.txt");
        fs::write(&path, b"payload").expect("write");
        let link = upload(
            &UreqFetch,
            UploadJob {
                source: UploadSource::Path(path),
                file_name: String::new(),
                content_type: String::new(),
                target: target(&url, false),
            },
        )
        .expect("uploaded");
        let (head, body) = server.join().expect("server thread");
        assert!(head.starts_with("POST "));
        assert_eq!(body, b"payload");
        assert_eq!(link, format!("{url}files/note.txt"));
    }

    #[test]
    fn ureq_upload_does_not_follow_post_redirect() {
        use crate::desktop::httpfetch::{serve_once, UreqFetch};
        let (url, server) = serve_once(
            "302 Found",
            "Location: https://elsewhere.example/x\r\n",
            Vec::new(),
        );
        let dir = TempDir::new("fileupload-ureq-redirect");
        let path = dir.path().join("note.txt");
        fs::write(&path, b"x").expect("write");
        let err = upload(
            &UreqFetch,
            UploadJob {
                source: UploadSource::Path(path),
                file_name: String::new(),
                content_type: String::new(),
                target: target(&url, false),
            },
        )
        .unwrap_err();
        let (head, _) = server.join().expect("server thread");
        assert!(head.starts_with("POST "));
        assert_eq!(err, COULD_NOT_UPLOAD);
    }

    const PNG_MAGIC: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

    fn content_type_header(fake: &FakeFetch) -> String {
        let posts = fake.post_requests();
        assert_eq!(posts.len(), 1);
        posts[0]
            .headers
            .iter()
            .find(|(name, _)| name == "Content-Type")
            .map(|(_, value)| value.clone())
            .expect("Content-Type")
    }

    #[test]
    fn upload_sniffs_extensionless_png_bytes() {
        let fake = FakeFetch::new();
        let url = "http://127.0.0.1/upload";
        fake.serve_post(url, 201, Some("/files/blob"));
        let dir = TempDir::new("fileupload-sniff");
        let path = dir.path().join("blob");
        fs::write(&path, PNG_MAGIC).expect("write");
        upload(
            &fake,
            UploadJob {
                source: UploadSource::Path(path),
                file_name: String::new(),
                content_type: String::new(),
                target: target(url, false),
            },
        )
        .expect("uploaded");
        assert_eq!(content_type_header(&fake), "image/png");
    }

    #[test]
    fn upload_sniffs_png_bytes_over_txt_extension() {
        let fake = FakeFetch::new();
        let url = "http://127.0.0.1/upload";
        fake.serve_post(url, 201, Some("/files/note.txt"));
        let dir = TempDir::new("fileupload-sniff-txt");
        let path = dir.path().join("note.txt");
        fs::write(&path, PNG_MAGIC).expect("write");
        upload(
            &fake,
            UploadJob {
                source: UploadSource::Path(path),
                file_name: String::new(),
                content_type: String::new(),
                target: target(url, false),
            },
        )
        .expect("uploaded");
        assert_eq!(content_type_header(&fake), "image/png");
    }

    #[test]
    fn upload_keeps_path_content_type_override() {
        let fake = FakeFetch::new();
        let url = "http://127.0.0.1/upload";
        fake.serve_post(url, 201, Some("/files/note.png"));
        let dir = TempDir::new("fileupload-override");
        let path = dir.path().join("note.png");
        fs::write(&path, PNG_MAGIC).expect("write");
        upload(
            &fake,
            UploadJob {
                source: UploadSource::Path(path),
                file_name: String::new(),
                content_type: "text/plain".to_owned(),
                target: target(url, false),
            },
        )
        .expect("uploaded");
        assert_eq!(content_type_header(&fake), "text/plain");
    }

    #[test]
    fn upload_bytes_empty_content_type_is_octet_stream_not_sniffed() {
        let fake = FakeFetch::new();
        let url = "http://127.0.0.1/upload";
        fake.serve_post(url, 201, Some("/files/blob"));
        upload(
            &fake,
            UploadJob {
                source: UploadSource::Bytes(PNG_MAGIC.to_vec()),
                file_name: "blob".to_owned(),
                content_type: String::new(),
                target: target(url, false),
            },
        )
        .expect("uploaded");
        assert_eq!(content_type_header(&fake), "application/octet-stream");
    }

    #[test]
    fn upload_png_name_with_png_bytes_stays_image_png() {
        let fake = FakeFetch::new();
        let url = "http://127.0.0.1/upload";
        fake.serve_post(url, 201, Some("/files/photo.png"));
        let dir = TempDir::new("fileupload-png");
        let path = dir.path().join("photo.png");
        fs::write(&path, PNG_MAGIC).expect("write");
        upload(
            &fake,
            UploadJob {
                source: UploadSource::Path(path),
                file_name: String::new(),
                content_type: String::new(),
                target: target(url, false),
            },
        )
        .expect("uploaded");
        assert_eq!(content_type_header(&fake), "image/png");
    }
}
