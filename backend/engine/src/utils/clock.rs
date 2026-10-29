use std::{
    thread,
    time::{Duration, Instant},
};

use ffmpeg_next::{Rational, Rescale, rescale};

/// How far the output may fall behind the clock and still catch up by
/// sending faster. Past it (a slow file open, a disk stall, a long fallback),
/// the clock starts over from the late packet: sending the whole backlog at
/// once would flood RTMP, SRT and UDP receivers.
const MAX_CATCH_UP: Duration = Duration::from_secs(1);

/// The longest the output waits for one packet. Packets follow each other a
/// frame apart; a longer wait is media time that jumped ahead, and waiting
/// for it would stop reading the source (a live's receive buffer then
/// overflows). Past it, the clock starts over from that packet.
const MAX_WAIT: Duration = Duration::from_secs(2);

pub(crate) struct PlayoutClock {
    anchor_wall: Option<Instant>,
    anchor_media_us: i64,
    rate: f64,
    pending_rate: f64,
    reanchor_next: bool,
}

impl PlayoutClock {
    pub(crate) fn new() -> Self {
        Self {
            anchor_wall: None,
            anchor_media_us: 0,
            rate: 1.0,
            pending_rate: 1.0,
            reanchor_next: false,
        }
    }

    pub(crate) fn set_rate(&mut self, rate: f64) {
        let rate = if rate.is_finite() && rate > 0.0 {
            rate
        } else {
            1.0
        };
        if self.pending_rate == rate {
            return;
        }

        self.pending_rate = rate;
        self.reanchor_next = true;
    }

    pub(crate) fn wait_until(&mut self, timestamp: Option<i64>, time_base: Rational) {
        let Some(timestamp) = timestamp else {
            return;
        };

        let target_us = timestamp.rescale(time_base, rescale::TIME_BASE);

        if let Some(delay) = self.delay(target_us, Instant::now()) {
            thread::sleep(delay);
        }
    }

    /// How long to wait at `now` before sending media time `target_us`.
    fn delay(&mut self, target_us: i64, now: Instant) -> Option<Duration> {
        if self.anchor_wall.is_none() || self.reanchor_next {
            self.anchor(target_us, now);
            self.rate = self.pending_rate;
            self.reanchor_next = false;
        }

        if target_us <= self.anchor_media_us {
            return None;
        }

        let anchor_wall = self.anchor_wall.unwrap_or(now);
        let media_delta_us = target_us - self.anchor_media_us;
        let paced_target_us = ((media_delta_us as f64) / self.rate).max(0.0) as u64;
        let target = Duration::from_micros(paced_target_us);
        let elapsed = now.saturating_duration_since(anchor_wall);

        match target.checked_sub(elapsed) {
            Some(delay) if delay > MAX_WAIT => {
                log::warn!(
                    "Output media time jumped {:.1} s ahead; pacing restarts without waiting",
                    delay.as_secs_f64()
                );
                self.anchor(target_us, now);
                None
            }
            Some(delay) => Some(delay),
            None if elapsed - target > MAX_CATCH_UP => {
                log::warn!(
                    "Output fell {:.1} s behind; pacing restarts without catching up",
                    (elapsed - target).as_secs_f64()
                );
                self.anchor(target_us, now);
                None
            }
            None => None,
        }
    }

    fn anchor(&mut self, media_us: i64, now: Instant) {
        self.anchor_wall = Some(now);
        self.anchor_media_us = media_us;
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::PlayoutClock;

    const SECOND: i64 = 1_000_000;

    #[test]
    fn packets_are_paced_to_their_media_time() {
        let mut clock = PlayoutClock::new();
        let start = Instant::now();

        assert_eq!(clock.delay(0, start), None);
        assert_eq!(clock.delay(SECOND, start), Some(Duration::from_secs(1)));
        assert_eq!(
            clock.delay(2 * SECOND, start + Duration::from_millis(1500)),
            Some(Duration::from_millis(500))
        );
    }

    #[test]
    fn media_time_that_jumps_ahead_is_not_waited_for() {
        let mut clock = PlayoutClock::new();
        let start = Instant::now();
        clock.delay(0, start);

        // Four minutes ahead at once: sent now, and pacing goes on from there.
        assert_eq!(clock.delay(240 * SECOND, start), None);
        assert_eq!(
            clock.delay(240 * SECOND + SECOND / 25, start),
            Some(Duration::from_millis(40))
        );
    }

    #[test]
    fn a_short_delay_is_caught_up() {
        let mut clock = PlayoutClock::new();
        let start = Instant::now();
        clock.delay(0, start);

        // Half a second late: sent at once, and the next packet still keeps
        // the original pace.
        assert_eq!(
            clock.delay(SECOND, start + Duration::from_millis(1500)),
            None
        );
        assert_eq!(
            clock.delay(2 * SECOND, start + Duration::from_millis(1600)),
            Some(Duration::from_millis(400))
        );
    }

    #[test]
    fn consecutive_clips_at_the_same_rate_preserve_the_wall_clock() {
        let mut clock = PlayoutClock::new();
        let start = Instant::now();
        clock.delay(0, start);
        clock.set_rate(1.0);

        assert_eq!(
            clock.delay(SECOND, start + Duration::from_millis(500)),
            Some(Duration::from_millis(500))
        );
    }

    #[test]
    fn a_changed_rate_anchors_once_and_uses_the_new_pace() {
        let mut clock = PlayoutClock::new();
        let start = Instant::now();
        clock.delay(0, start);
        clock.set_rate(2.0);
        let changed = start + Duration::from_millis(500);
        assert_eq!(clock.delay(SECOND, changed), None);
        clock.set_rate(2.0);

        assert_eq!(
            clock.delay(2 * SECOND, changed),
            Some(Duration::from_millis(500))
        );
    }

    #[test]
    fn a_long_stall_restarts_the_pace_instead_of_bursting() {
        let mut clock = PlayoutClock::new();
        let start = Instant::now();
        clock.delay(0, start);

        // Five seconds late: the late packet goes out, and the ones after it
        // are paced from there instead of rushing out to catch up.
        let late = start + Duration::from_secs(6);
        assert_eq!(clock.delay(SECOND, late), None);
        assert_eq!(
            clock.delay(SECOND + SECOND / 25, late),
            Some(Duration::from_millis(40))
        );
    }
}
