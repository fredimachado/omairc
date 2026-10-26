//! The avatar fetch: one GET of a nick's avatar, judged at each step, decoded within
//! budget and handed back as a PNG of its first frame. Go `tui/internal/avatar/fetch.go`
//! (the download and the decode) and Qt `src/irc/ircavatarstore.cpp` and
//! `src/irc/ircavatarhttp.cpp` (the redirects, the content type and the budgets).
//!
//! The controller decides whether an avatar is fetched (`controller/avatars.rs`). This
//! module does the network and the image work. It runs on `cx.background_executor()`
//! through the `HttpFetch` seam, and it never touches the controller. Every rule that
//! can be checked without a socket lives in `irc/avatarurl.rs` and `irc/avatarstore.rs`.
//!
//! The image is decoded here, not by gpui. gpui's own loader decodes every GIF frame
//! and has no size check, so the bytes it receives are already one frame and inside
//! the budget: a PNG no wider or taller than `MAXIMUM_DECODED_EDGE`, scaled down to
//! `KEPT_EDGE` for drawing. Only PNG, JPEG, GIF and WebP are accepted. SVG and the rest
//! fail closed, whatever the server calls them.

use std::fmt;
use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;

use image::{DynamicImage, ImageFormat, ImageReader, Limits};
use url::Url;

use crate::desktop::httpfetch::{get_following, FetchError, FetchRequest, HttpFetch};
use crate::irc::avatarstore::{
    content_type_is_image, decoded_size_within_budget, MAXIMUM_AVATAR_BYTES,
    MAXIMUM_DECODED_ALLOCATION_MB, MAXIMUM_DECODED_EDGE, MAXIMUM_REDIRECTS,
};
use crate::irc::avatarurl::avatar_url_is_safe;

/// Qt `kFetchTimeoutMs`.
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
/// Qt `kMaximumCircledEdge`: the largest edge kept for drawing. A bigger decode is
/// scaled down before it is encoded, so the cache holds small PNGs.
pub const KEPT_EDGE: u32 = 128;

const ACCEPT: &str = "image/*";
const USER_AGENT: &str = "Omairc";

/// Why an avatar was not kept. The UI shows initials for every one of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AvatarFailure {
    /// The transfer failed, or a redirect was refused. The text is for the log.
    Transfer(String),
    /// A status other than 200 OK.
    Status(u16),
    /// The `Content-Type` is not an image type.
    NotAnImage(String),
    /// The body is not one of the accepted formats.
    UnknownFormat,
    /// The body is empty.
    Empty,
    /// The body is over `MAXIMUM_AVATAR_BYTES`.
    TooLarge,
    /// The declared or decoded size is over the budget.
    OutOfBudget,
    /// The decoder refused the bytes.
    Decode(String),
}

impl fmt::Display for AvatarFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transfer(reason) => write!(f, "avatar transfer failed: {reason}"),
            Self::Status(status) => write!(f, "avatar answered {status}"),
            Self::NotAnImage(content_type) => write!(f, "avatar is not an image: {content_type}"),
            Self::UnknownFormat => write!(f, "avatar is not PNG, JPEG, GIF or WebP"),
            Self::Empty => write!(f, "avatar body is empty"),
            Self::TooLarge => write!(f, "avatar body is over {MAXIMUM_AVATAR_BYTES} bytes"),
            Self::OutOfBudget => write!(f, "avatar is over the decoded-size budget"),
            Self::Decode(reason) => write!(f, "avatar did not decode: {reason}"),
        }
    }
}

/// One avatar to fetch: the cache key and resolved URL the controller chose, and the
/// seam to fetch through. It is `Send`, so the background executor can own it.
pub struct AvatarJob {
    pub key: String,
    pub url: Url,
    pub fetch: Arc<dyn HttpFetch>,
}

/// What a finished job hands back: its cache key and the PNG, or the reason it failed.
pub type AvatarOutcome = (String, Result<Arc<[u8]>, AvatarFailure>);

impl AvatarJob {
    /// Runs the fetch and the decode. It blocks, so callers run it on the background
    /// executor.
    pub fn run(self) -> AvatarOutcome {
        let result = fetch_avatar(self.fetch.as_ref(), &self.url).map(Arc::from);
        (self.key, result)
    }
}

/// Fetches one avatar and returns it as PNG. Every hop is judged by
/// `avatar_url_is_safe` before it is requested, and each request connects only to a
/// global address (the DNS pin, in `UreqFetch`). A redirect is followed, a
/// non-200 status or a non-image type is not, and the body is capped at
/// `MAXIMUM_AVATAR_BYTES` as it arrives.
pub fn fetch_avatar(fetch: &dyn HttpFetch, url: &Url) -> Result<Vec<u8>, AvatarFailure> {
    let request = FetchRequest::get(url.as_str(), FETCH_TIMEOUT, MAXIMUM_AVATAR_BYTES)
        .header("Accept", ACCEPT)
        .header("User-Agent", USER_AGENT)
        .public_addresses_only();
    let cap = MAXIMUM_AVATAR_BYTES as usize;
    let mut body = Vec::new();
    let mut too_large = false;
    let fetched = get_following(
        fetch,
        request,
        MAXIMUM_REDIRECTS,
        &avatar_url_is_safe,
        &mut |bytes| {
            if body.len() + bytes.len() > cap {
                too_large = true;
                return false;
            }
            body.extend_from_slice(bytes);
            true
        },
    );
    let fetched = match fetched {
        Ok(fetched) => fetched,
        Err(FetchError::Stopped) if too_large => return Err(AvatarFailure::TooLarge),
        Err(err) => return Err(AvatarFailure::Transfer(err.to_string())),
    };
    if fetched.status != 200 {
        return Err(AvatarFailure::Status(fetched.status));
    }
    let content_type = fetched.content_type.unwrap_or_default();
    if !content_type_is_image(&content_type) {
        return Err(AvatarFailure::NotAnImage(content_type));
    }
    if body.is_empty() {
        return Err(AvatarFailure::Empty);
    }
    decode_avatar(&body)
}

/// Decodes an avatar body within the budget and returns its first frame as PNG,
/// scaled down to `KEPT_EDGE` when it is larger. The declared size is checked from
/// the header before any pixel is decoded, and the decoded size again after.
pub fn decode_avatar(body: &[u8]) -> Result<Vec<u8>, AvatarFailure> {
    let format = sniff(body).ok_or(AvatarFailure::UnknownFormat)?;
    let (width, height) = budget_reader(body, format)
        .into_dimensions()
        .map_err(decode_failure)?;
    if !decoded_size_within_budget(width, height) {
        return Err(AvatarFailure::OutOfBudget);
    }
    let decoded = budget_reader(body, format).decode().map_err(decode_failure)?;
    if !decoded_size_within_budget(decoded.width(), decoded.height()) {
        return Err(AvatarFailure::OutOfBudget);
    }
    encode_png(&keep_edge(decoded))
}

/// A decoder error as a failure. The decoder's own limit is the budget, so it is
/// `OutOfBudget`, and anything else is a refused body.
fn decode_failure(err: image::ImageError) -> AvatarFailure {
    match err {
        image::ImageError::Limits(_) => AvatarFailure::OutOfBudget,
        other => AvatarFailure::Decode(other.to_string()),
    }
}

/// The format a body's magic bytes name, or `None` for anything else. The
/// `Content-Type` is not trusted for this: a server may claim any image type.
fn sniff(body: &[u8]) -> Option<ImageFormat> {
    if body.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(ImageFormat::Png)
    } else if body.starts_with(&[0xff, 0xd8, 0xff]) {
        Some(ImageFormat::Jpeg)
    } else if body.starts_with(b"GIF87a") || body.starts_with(b"GIF89a") {
        Some(ImageFormat::Gif)
    } else if body.len() >= 12 && &body[..4] == b"RIFF" && &body[8..12] == b"WEBP" {
        Some(ImageFormat::WebP)
    } else {
        None
    }
}

/// Scales an image whose larger edge is over `KEPT_EDGE` down to that edge, keeping
/// its aspect ratio. A smaller image is returned as it is.
fn keep_edge(image: DynamicImage) -> DynamicImage {
    if image.width().max(image.height()) > KEPT_EDGE {
        image.thumbnail(KEPT_EDGE, KEPT_EDGE)
    } else {
        image
    }
}

fn encode_png(image: &DynamicImage) -> Result<Vec<u8>, AvatarFailure> {
    let mut out = Cursor::new(Vec::new());
    image
        .write_to(&mut out, ImageFormat::Png)
        .map_err(|err| AvatarFailure::Decode(err.to_string()))?;
    Ok(out.into_inner())
}

/// A reader over `body` whose decoder enforces the budget: the edge caps, and the
/// allocation cap Qt sets with `setAllocationLimit`.
fn budget_reader(body: &[u8], format: ImageFormat) -> ImageReader<Cursor<&[u8]>> {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAXIMUM_DECODED_EDGE);
    limits.max_image_height = Some(MAXIMUM_DECODED_EDGE);
    limits.max_alloc = Some(u64::from(MAXIMUM_DECODED_ALLOCATION_MB) * 1024 * 1024);
    let mut reader = ImageReader::with_format(Cursor::new(body), format);
    reader.limits(limits);
    reader
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::httpfetch::FakeFetch;

    const AVATAR: &str = "https://cdn.example.com/a.png";

    fn url(text: &str) -> Url {
        Url::parse(text).expect("parsed")
    }

    /// A solid image of the given size, encoded in `format`.
    fn encoded(format: ImageFormat, width: u32, height: u32) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        DynamicImage::new_rgb8(width, height)
            .write_to(&mut out, format)
            .expect("encodes");
        out.into_inner()
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        encoded(ImageFormat::Png, width, height)
    }

    fn dimensions_of(png_bytes: &[u8]) -> (u32, u32) {
        ImageReader::with_format(Cursor::new(png_bytes), ImageFormat::Png)
            .into_dimensions()
            .expect("a PNG")
    }

    #[test]
    fn a_png_is_fetched_and_returned_as_png() {
        let fake = FakeFetch::new();
        fake.serve_typed(AVATAR, 200, Some("image/png"), &png(32, 32));
        let bytes = fetch_avatar(&fake, &url(AVATAR)).expect("kept");
        assert_eq!(sniff(&bytes), Some(ImageFormat::Png));
        assert_eq!(dimensions_of(&bytes), (32, 32));
    }

    #[test]
    fn the_request_carries_the_avatar_headers_and_the_public_only_pin() {
        let fake = FakeFetch::new();
        fake.serve_typed(AVATAR, 200, Some("image/png"), &png(8, 8));
        fetch_avatar(&fake, &url(AVATAR)).expect("kept");
        let requests = fake.requests();
        assert_eq!(requests.len(), 1);
        let request = &requests[0];
        assert!(request.public_addresses_only, "the fetch is pinned to a public address");
        assert_eq!(request.max_body, MAXIMUM_AVATAR_BYTES);
        assert!(request.headers.contains(&("Accept".into(), "image/*".into())));
        assert!(request.headers.contains(&("User-Agent".into(), "Omairc".into())));
    }

    #[test]
    fn a_non_image_content_type_is_refused() {
        let fake = FakeFetch::new();
        fake.serve_typed(AVATAR, 200, Some("text/html"), &png(8, 8));
        assert!(matches!(
            fetch_avatar(&fake, &url(AVATAR)),
            Err(AvatarFailure::NotAnImage(_))
        ));
    }

    #[test]
    fn a_missing_content_type_is_refused() {
        let fake = FakeFetch::new();
        fake.serve(AVATAR, 200, &png(8, 8));
        assert!(matches!(
            fetch_avatar(&fake, &url(AVATAR)),
            Err(AvatarFailure::NotAnImage(_))
        ));
    }

    #[test]
    fn an_svg_claimed_as_an_image_is_refused_by_its_bytes() {
        let fake = FakeFetch::new();
        let svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\"><image href=\"https://x/\"/></svg>";
        fake.serve_typed(AVATAR, 200, Some("image/svg+xml"), svg);
        assert_eq!(fetch_avatar(&fake, &url(AVATAR)), Err(AvatarFailure::UnknownFormat));
    }

    #[test]
    fn a_non_200_status_is_a_failure() {
        let fake = FakeFetch::new();
        fake.serve_typed(AVATAR, 404, Some("image/png"), b"gone");
        assert_eq!(fetch_avatar(&fake, &url(AVATAR)), Err(AvatarFailure::Status(404)));
    }

    #[test]
    fn an_empty_body_is_refused() {
        let fake = FakeFetch::new();
        fake.serve_typed(AVATAR, 200, Some("image/png"), b"");
        assert_eq!(fetch_avatar(&fake, &url(AVATAR)), Err(AvatarFailure::Empty));
    }

    #[test]
    fn a_redirect_to_an_unsafe_host_is_never_requested() {
        let fake = FakeFetch::new();
        fake.redirect(AVATAR, 302, "https://localhost/a.png");
        fake.serve_typed("https://localhost/a.png", 200, Some("image/png"), &png(8, 8));
        let result = fetch_avatar(&fake, &url(AVATAR));
        assert!(matches!(result, Err(AvatarFailure::Transfer(_))), "{result:?}");
        let urls: Vec<String> = fake.requests().into_iter().map(|r| r.url).collect();
        assert_eq!(urls, [AVATAR], "the refused hop was never requested");
    }

    #[test]
    fn a_redirect_to_a_safe_host_is_followed_and_its_type_judged() {
        let fake = FakeFetch::new();
        fake.redirect(AVATAR, 301, "https://img.example.net/b.jpg");
        fake.serve_typed("https://img.example.net/b.jpg", 200, Some("image/jpeg"), &encoded(ImageFormat::Jpeg, 16, 16));
        let bytes = fetch_avatar(&fake, &url(AVATAR)).expect("kept");
        assert_eq!(sniff(&bytes), Some(ImageFormat::Png), "kept as PNG");
        assert_eq!(fake.requests().len(), 2);
    }

    #[test]
    fn a_declared_size_over_the_budget_is_refused_before_decoding() {
        // A tiny payload that declares a huge image: the header check refuses it.
        let bytes = png(MAXIMUM_DECODED_EDGE + 1, 8);
        assert!(bytes.len() < 4096, "the bomb is small on the wire");
        assert_eq!(decode_avatar(&bytes), Err(AvatarFailure::OutOfBudget));
    }

    #[test]
    fn the_edge_cap_is_inclusive_at_the_budget_edge() {
        // 1024 x 1024 is exactly the pixel budget, so it decodes. One row more does not.
        assert!(decode_avatar(&png(MAXIMUM_DECODED_EDGE, MAXIMUM_DECODED_EDGE)).is_ok());
        assert_eq!(
            decode_avatar(&png(MAXIMUM_DECODED_EDGE, MAXIMUM_DECODED_EDGE + 1)),
            Err(AvatarFailure::OutOfBudget)
        );
    }

    #[test]
    fn a_large_image_is_scaled_to_the_kept_edge() {
        let kept = decode_avatar(&png(400, 200)).expect("decoded");
        assert_eq!(dimensions_of(&kept), (KEPT_EDGE, KEPT_EDGE / 2), "aspect ratio is kept");
    }

    #[test]
    fn gif_and_jpeg_first_frames_are_returned_as_png() {
        for format in [ImageFormat::Gif, ImageFormat::Jpeg] {
            let kept = decode_avatar(&encoded(format, 24, 24)).expect("decoded");
            assert_eq!(sniff(&kept), Some(ImageFormat::Png), "{format:?}");
            assert_eq!(dimensions_of(&kept), (24, 24), "{format:?}");
        }
    }

    #[test]
    fn a_garbage_body_is_refused() {
        assert_eq!(decode_avatar(b"not an image at all"), Err(AvatarFailure::UnknownFormat));
    }

    #[test]
    fn a_transfer_failure_is_a_failure() {
        let fake = FakeFetch::new();
        fake.fail(AVATAR, "connection reset");
        assert!(matches!(
            fetch_avatar(&fake, &url(AVATAR)),
            Err(AvatarFailure::Transfer(_))
        ));
    }

    #[test]
    fn a_job_names_its_key_and_returns_the_png() {
        let fake = Arc::new(FakeFetch::new());
        fake.serve_typed(AVATAR, 200, Some("image/png"), &png(8, 8));
        let job = AvatarJob {
            key: "key".into(),
            url: url(AVATAR),
            fetch: fake.clone(),
        };
        let (key, result) = job.run();
        assert_eq!(key, "key");
        let bytes = result.expect("kept");
        assert_eq!(sniff(&bytes), Some(ImageFormat::Png));
    }
}
