//! Source classification shared by the native media runtime and scheduler.

pub fn even(value: u32) -> u32 {
    value & !1
}

pub(crate) fn is_network_url(path: &str) -> bool {
    path.split_once("://")
        .is_some_and(|(scheme, _)| !scheme.eq_ignore_ascii_case("file"))
}

/// Returns whether an input URL is an unbounded live transport rather than a
/// seekable media resource. HTTP(S) deliberately remains excluded because it
/// may refer to either VOD or a live manifest and needs demuxer-level handling.
pub fn is_live_input(path: &str) -> bool {
    let Some((scheme, _)) = path.split_once("://") else {
        return false;
    };

    matches!(
        scheme.to_ascii_lowercase().as_str(),
        "rtmp" | "rtmps" | "rtp" | "rtsp" | "srt" | "udp" | "tcp" | "rist"
    )
}

#[cfg(test)]
mod tests {
    use super::{even, is_live_input, is_network_url};

    #[test]
    fn rounds_dimensions_down_to_even_values() {
        assert_eq!(even(0), 0);
        assert_eq!(even(1), 0);
        assert_eq!(even(641), 640);
        assert_eq!(even(640), 640);
    }

    #[test]
    fn classifies_network_urls() {
        assert!(is_network_url("rtmp://example.com/live/stream"));
        assert!(is_network_url("http://example.com/video.mp4"));
        assert!(!is_network_url("/var/lib/media/clip.mp4"));
        assert!(!is_network_url("file:///var/lib/media/clip.mp4"));
        assert!(!is_network_url("clip.mp4"));
    }

    #[test]
    fn identifies_unseekable_live_input_transports() {
        for input in [
            "rtmp://example.com/live/stream",
            "rtmps://example.com/live/stream",
            "rtsp://example.com/live",
            "rtp://239.0.0.1:5004",
            "srt://example.com:9000",
            "udp://239.0.0.1:1234",
            "tcp://example.com:9000",
            "rist://example.com:8193",
        ] {
            assert!(is_live_input(input), "{input}");
        }
        assert!(!is_live_input("https://example.com/vod.mp4"));
        assert!(!is_live_input("/var/lib/media/clip.mp4"));
    }
}
