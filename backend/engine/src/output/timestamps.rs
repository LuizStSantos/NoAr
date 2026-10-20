use std::time::{Duration, Instant};

use ffmpeg_next::{Packet, format};
use log::warn;

/// How often a recurring timestamp problem is logged again.
const WARN_INTERVAL: Duration = Duration::from_secs(60);

/// Counts a recurring problem and says when to log it: the first time, then
/// at most once a minute, with how often it happened meanwhile.
#[derive(Default)]
pub(super) struct WarnThrottle {
    count: u64,
    warned_at: Option<Instant>,
}

impl WarnThrottle {
    /// Count one more; `Some(total)` when it is time to log.
    pub(super) fn hit(&mut self) -> Option<u64> {
        self.count += 1;

        if self
            .warned_at
            .is_some_and(|warned| warned.elapsed() < WARN_INTERVAL)
        {
            return None;
        }

        self.warned_at = Some(Instant::now());

        Some(self.count)
    }
}

/// Keeps every stream's decoding timestamps rising, as muxers require. A
/// packet that would go back in time is moved right after the previous one,
/// the way the ffmpeg command line tool does it: one bad timestamp then costs
/// a glitch instead of failing the whole output, and the fallback after it.
pub(super) struct DtsGuard {
    /// MPEG-TS, FLV and HLS refuse a repeated timestamp, not only a lower one.
    strict: bool,
    last: Vec<Option<i64>>,
    warning: WarnThrottle,
    label: &'static str,
    channel_id: i32,
}

impl DtsGuard {
    pub(super) fn new(
        octx: &format::context::Output,
        label: &'static str,
        channel_id: i32,
    ) -> Self {
        Self {
            strict: !octx.format().flags().contains(format::Flags::TS_NONSTRICT),
            last: Vec::new(),
            warning: WarnThrottle::default(),
            label,
            channel_id,
        }
    }

    /// Correct `packet`, already in the time base of output stream `stream`.
    pub(super) fn fix(&mut self, packet: &mut Packet, stream: usize) {
        let Some(original) = packet.dts() else {
            return;
        };

        if self.last.len() <= stream {
            self.last.resize(stream + 1, None);
        }

        let previous = self.last[stream];
        let (dts, pts) = monotonic(original, packet.pts(), previous, self.strict);

        self.last[stream] = Some(dts);

        if dts == original && pts == packet.pts() {
            return;
        }

        packet.set_dts(Some(dts));
        packet.set_pts(pts);

        if let Some(count) = self.warning.hit() {
            warn!(channel = self.channel_id;
                "{}: stream {stream} timestamps out of order (dts {original} after {}); moved to {dts}, {count} correction(s) so far",
                self.label,
                previous.map_or_else(|| "none".to_string(), |previous| previous.to_string())
            );
        }
    }
}

/// The decoding and presentation timestamps a packet gets after `last`.
fn monotonic(dts: i64, pts: Option<i64>, last: Option<i64>, strict: bool) -> (i64, Option<i64>) {
    // Decoding cannot come after presentation.
    let (mut dts, mut pts) = match pts {
        Some(pts) if dts > pts => (pts, Some(pts)),
        _ => (dts, pts),
    };

    if let Some(last) = last {
        let min = last + i64::from(strict);

        if dts < min {
            pts = pts.map(|pts| pts.max(min));
            dts = min;
        }
    }

    (dts, pts)
}

#[cfg(test)]
mod tests {
    use super::monotonic;

    #[test]
    fn rising_timestamps_pass_unchanged() {
        assert_eq!(monotonic(10, Some(12), Some(9), true), (10, Some(12)));
        assert_eq!(monotonic(10, Some(10), None, true), (10, Some(10)));
    }

    #[test]
    fn a_repeated_timestamp_moves_after_the_last_on_strict_muxers() {
        assert_eq!(
            monotonic(51_000, Some(51_000), Some(51_000), true),
            (51_001, Some(51_001))
        );
        assert_eq!(
            monotonic(51_000, Some(51_000), Some(51_000), false),
            (51_000, Some(51_000))
        );
    }

    #[test]
    fn a_timestamp_that_goes_back_moves_after_the_last() {
        assert_eq!(monotonic(40, Some(46), Some(50), true), (51, Some(51)));
        assert_eq!(monotonic(40, Some(80), Some(50), true), (51, Some(80)));
    }

    #[test]
    fn decoding_never_comes_after_presentation() {
        assert_eq!(monotonic(20, Some(15), Some(10), true), (15, Some(15)));
    }
}
