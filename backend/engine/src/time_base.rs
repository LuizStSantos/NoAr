//! Time bases and timestamp conversion between them.
//!
//! The engine counts video in frames and audio in samples; a [`Rational`]
//! names the length of one unit in seconds (`1/25`, `1/48000`), and
//! [`Rescale::rescale`] moves a timestamp from one time base to another,
//! rounding to the nearest unit with halves away from zero (as FFmpeg's
//! `av_rescale_q` did, so timestamps keep the same values).

/// A time base: `numerator / denominator` seconds per unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rational(pub i32, pub i32);

/// Microseconds, the time base of wall-clock waits.
pub const TIME_BASE: Rational = Rational(1, 1_000_000);

impl Rational {
    pub fn new(numerator: i32, denominator: i32) -> Self {
        Self(numerator, denominator)
    }

    pub fn numerator(self) -> i32 {
        self.0
    }

    pub fn denominator(self) -> i32 {
        self.1
    }
}

impl From<Rational> for f64 {
    fn from(value: Rational) -> Self {
        f64::from(value.0) / f64::from(value.1)
    }
}

/// Moving a timestamp between time bases.
pub trait Rescale {
    /// The same instant in `destination` units, rounded to the nearest one
    /// (halves away from zero).
    fn rescale(&self, source: Rational, destination: Rational) -> i64;
}

impl<T: Into<i64> + Copy> Rescale for T {
    fn rescale(&self, source: Rational, destination: Rational) -> i64 {
        let value = i128::from((*self).into());
        // value * source / destination, as one fraction.
        let numerator = i128::from(source.0) * i128::from(destination.1);
        let denominator = i128::from(source.1) * i128::from(destination.0);

        if denominator == 0 {
            return 0;
        }

        let (numerator, denominator) = if denominator < 0 {
            (-numerator, -denominator)
        } else {
            (numerator, denominator)
        };
        let product = value * numerator;
        let half = denominator / 2;
        let rounded = if product >= 0 {
            (product + half) / denominator
        } else {
            -((-product + half) / denominator)
        };

        rounded.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_become_samples_and_back() {
        let video = Rational(1, 25);
        let audio = Rational(1, 48_000);

        assert_eq!(1_i64.rescale(video, audio), 1920);
        assert_eq!(250_i64.rescale(video, audio), 480_000);
        assert_eq!(480_000_i64.rescale(audio, video), 250);
        assert_eq!(7_i64.rescale(video, TIME_BASE), 280_000);
    }

    #[test]
    fn rounding_is_to_the_nearest_with_halves_away_from_zero() {
        // 1/3 s in halves of a second: 0.666 rounds to 1.
        assert_eq!(1_i64.rescale(Rational(1, 3), Rational(1, 2)), 1);
        // Exactly half a unit rounds away from zero, either sign.
        assert_eq!(1_i64.rescale(Rational(1, 4), Rational(1, 2)), 1);
        assert_eq!((-1_i64).rescale(Rational(1, 4), Rational(1, 2)), -1);
        assert_eq!(3_i64.rescale(Rational(1, 4), Rational(1, 2)), 2);
        assert_eq!((-3_i64).rescale(Rational(1, 4), Rational(1, 2)), -2);
        // Below half rounds towards zero.
        assert_eq!(1_i64.rescale(Rational(1, 5), Rational(1, 2)), 0);
        assert_eq!((-1_i64).rescale(Rational(1, 5), Rational(1, 2)), 0);
    }

    #[test]
    fn fractional_frame_rates_keep_their_exact_timing() {
        // 29.97 frames/s: 30000 frames are 1001 seconds.
        let ntsc = Rational(1001, 30_000);

        assert_eq!(30_000_i64.rescale(ntsc, Rational(1, 1)), 1001);
        assert_eq!(1_i64.rescale(ntsc, Rational(1, 48_000)), 1602);
        assert!((f64::from(ntsc) - 1.001 / 30.0).abs() < 1e-12);
    }

    #[test]
    fn large_timestamps_do_not_overflow() {
        let a_year_of_samples = 48_000_i64 * 60 * 60 * 24 * 365;

        assert_eq!(
            a_year_of_samples.rescale(Rational(1, 48_000), TIME_BASE),
            a_year_of_samples / 48 * 1000
        );
    }
}
