//! Normalization of the (video URL, timestamp) key under which corrections
//! are stored.
//!
//! Two users pointing at the same moment in the same video must produce the
//! same key, regardless of URL formatting noise (tracking params, `youtu.be`
//! vs `youtube.com`, http vs https, trailing slashes). Timestamps are bucketed
//! to whole seconds so that 12.3s and 12.7s of the "same" caption line collide
//! sensibly rather than fragmenting into many near-duplicate keys.

use crate::error::SubFixerError;

/// A canonical key identifying a caption span in a video.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VideoKey {
    /// Canonical video identifier (a YouTube video id when recognizable,
    /// otherwise a normalized URL).
    pub video_id: String,
    /// Start of the caption span, in whole seconds.
    pub start_sec: u32,
}

impl VideoKey {
    /// Stable string form used as the SQLite primary key.
    pub fn as_storage_key(&self) -> String {
        format!("{}@{}", self.video_id, self.start_sec)
    }
}

/// Build a canonical [`VideoKey`] from a raw URL and a raw timestamp.
///
/// `start_sec` is the caption span start in seconds; fractional input should be
/// floored by the caller, but we also reject obviously-bad values here.
pub fn make_key(raw_url: &str, start_sec: i64) -> Result<VideoKey, SubFixerError> {
    if start_sec < 0 {
        return Err(SubFixerError::Validation(
            "timestamp must be non-negative".into(),
        ));
    }
    // 24h cap: nothing legitimate is a caption at 86,400+ seconds and it guards
    // against overflow/garbage.
    if start_sec > 86_400 {
        return Err(SubFixerError::Validation(
            "timestamp exceeds 24h cap".into(),
        ));
    }
    let video_id = canonical_video_id(raw_url)?;
    Ok(VideoKey {
        video_id,
        start_sec: start_sec as u32,
    })
}

/// Extract a canonical video identifier from a URL.
///
/// Recognizes the common YouTube URL shapes and returns the bare 11-char video
/// id. For anything else, returns a normalized URL (scheme lowercased to https,
/// host lowercased, common tracking params stripped, trailing slash removed).
pub fn canonical_video_id(raw_url: &str) -> Result<String, SubFixerError> {
    let url = raw_url.trim();
    if url.is_empty() {
        return Err(SubFixerError::Validation("url is empty".into()));
    }

    // Strip scheme for parsing convenience.
    let (_scheme, rest) = match url.split_once("://") {
        Some((s, r)) => (s.to_ascii_lowercase(), r),
        None => ("https".to_string(), url),
    };

    let (host_port, path_and_query) = match rest.split_once('/') {
        Some((h, p)) => (h.to_ascii_lowercase(), p),
        None => (rest.to_ascii_lowercase(), ""),
    };
    let host = host_port.split(':').next().unwrap_or("").to_string();
    let host_no_www = host.strip_prefix("www.").unwrap_or(&host).to_string();

    let (path, query) = match path_and_query.split_once('?') {
        Some((p, q)) => (p, q),
        None => (path_and_query, ""),
    };

    // youtu.be/<id>
    if host_no_www == "youtu.be" {
        let id = path.trim_matches('/');
        if let Some(v) = valid_yt_id(id) {
            return Ok(v);
        }
    }

    // youtube.com/watch?v=<id>  (also m.youtube.com)
    if host_no_www == "youtube.com" || host_no_www == "m.youtube.com" {
        if path.trim_matches('/') == "watch" {
            if let Some(v) = query_param(query, "v").and_then(|id| valid_yt_id(&id)) {
                return Ok(v);
            }
        }
        // youtube.com/shorts/<id> and /embed/<id>
        for prefix in ["shorts/", "embed/", "v/"] {
            if let Some(idpart) = path.trim_start_matches('/').strip_prefix(prefix) {
                let id = idpart.split('/').next().unwrap_or("");
                if let Some(v) = valid_yt_id(id) {
                    return Ok(v);
                }
            }
        }
    }

    // Generic fallback: normalized URL without tracking params.
    let kept_query = strip_tracking(query);
    let mut norm = format!("https://{}", host_no_www);
    let clean_path = path.trim_end_matches('/');
    if !clean_path.is_empty() {
        norm.push('/');
        norm.push_str(clean_path);
    }
    if !kept_query.is_empty() {
        norm.push('?');
        norm.push_str(&kept_query);
    }
    Ok(norm)
}

/// A valid YouTube video id is exactly 11 chars of [A-Za-z0-9_-].
fn valid_yt_id(id: &str) -> Option<String> {
    if id.len() == 11
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        Some(id.to_string())
    } else {
        None
    }
}

fn query_param(query: &str, key: &str) -> Option<String> {
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            if k == key {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// Strip common tracking params, keep the rest sorted for determinism.
fn strip_tracking(query: &str) -> String {
    const DROP: &[&str] = &[
        "utm_source",
        "utm_medium",
        "utm_campaign",
        "utm_term",
        "utm_content",
        "feature",
        "si",
        "fbclid",
        "gclid",
    ];
    if query.is_empty() {
        return String::new();
    }
    let mut kept: Vec<&str> = query
        .split('&')
        .filter(|pair| {
            let k = pair.split_once('=').map(|(k, _)| k).unwrap_or(pair);
            !DROP.contains(&k)
        })
        .filter(|p| !p.is_empty())
        .collect();
    kept.sort_unstable();
    kept.join("&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn youtube_long_and_short_urls_collapse() {
        let a = canonical_video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ").unwrap();
        let b = canonical_video_id("https://youtu.be/dQw4w9WgXcQ").unwrap();
        let c = canonical_video_id("http://m.youtube.com/watch?v=dQw4w9WgXcQ&t=42s").unwrap();
        assert_eq!(a, "dQw4w9WgXcQ");
        assert_eq!(a, b);
        assert_eq!(a, c);
    }

    #[test]
    fn youtube_tracking_params_are_ignored_for_id() {
        let a = canonical_video_id("https://youtu.be/dQw4w9WgXcQ?si=abc123&feature=share").unwrap();
        assert_eq!(a, "dQw4w9WgXcQ");
    }

    #[test]
    fn shorts_and_embed_forms_recognized() {
        assert_eq!(
            canonical_video_id("https://youtube.com/shorts/abcdefghijk").unwrap(),
            "abcdefghijk"
        );
        assert_eq!(
            canonical_video_id("https://www.youtube.com/embed/abcdefghijk").unwrap(),
            "abcdefghijk"
        );
    }

    #[test]
    fn invalid_yt_id_falls_back_to_url() {
        // Too short to be a real id -> treated as a generic URL, not an id.
        let v = canonical_video_id("https://youtu.be/short").unwrap();
        assert!(v.starts_with("https://youtu.be"));
    }

    #[test]
    fn generic_url_normalized_and_tracking_stripped() {
        let a = canonical_video_id("http://Example.COM/video/123/?utm_source=x&q=1").unwrap();
        let b = canonical_video_id("https://example.com/video/123?q=1").unwrap();
        assert_eq!(a, b);
        assert_eq!(a, "https://example.com/video/123?q=1");
    }

    #[test]
    fn empty_url_errors() {
        assert!(matches!(
            canonical_video_id("   "),
            Err(SubFixerError::Validation(_))
        ));
    }

    #[test]
    fn negative_timestamp_errors() {
        assert!(matches!(
            make_key("https://youtu.be/dQw4w9WgXcQ", -1),
            Err(SubFixerError::Validation(_))
        ));
    }

    #[test]
    fn oversized_timestamp_errors() {
        assert!(matches!(
            make_key("https://youtu.be/dQw4w9WgXcQ", 90_000),
            Err(SubFixerError::Validation(_))
        ));
    }

    #[test]
    fn storage_key_is_stable() {
        let k = make_key("https://www.youtube.com/watch?v=dQw4w9WgXcQ", 42).unwrap();
        assert_eq!(k.as_storage_key(), "dQw4w9WgXcQ@42");
    }

    #[test]
    fn same_moment_different_url_forms_same_storage_key() {
        let k1 = make_key("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=42s", 42).unwrap();
        let k2 = make_key("https://youtu.be/dQw4w9WgXcQ?si=zzz", 42).unwrap();
        assert_eq!(k1.as_storage_key(), k2.as_storage_key());
    }
}
