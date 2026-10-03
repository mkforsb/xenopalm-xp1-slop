//! FLUX: two sine oscillators phase-modulating each other.
//!
//! ```text
//!  y2 = sin(φ2 + m·y1[n−1])
//!  y1 = sin(φ1 + m·y2 + f·y1[n−1])
//! ```
//!
//! With one-way FM you get the familiar DX-style metallic percussion.
//! Coupling both ways closes a nonlinear loop, and past a certain index the
//! pair stops settling into a periodic orbit: the tone breaks up into
//! chaotic, noisy clang. The index has its own faster envelope, so a hit
//! starts in that turbulent region and relaxes into a clean tone as it decays.
//! Hitting harder pushes it further in.
//!
//! * RATIO: φ2's frequency ratio (0.5 … 7.13, through 1, √2, 2, 2.76 …).
//! * INDEX: peak coupling index.
//! * FEEDBK: oscillator 1's self-feedback (sine → saw → noise).

use super::Ctx;
use crate::params::map;
use crate::util::{sin_turns, t60_gain};

/// The index envelope decays this much faster than the amplitude.
const INDEX_DECAY_RATIO: f32 = 0.3;

#[derive(Clone, Debug, Default)]
pub struct Flux {
    p1: f32,
    p2: f32,
    y1: f32,
    y2: f32,
    inc1: f32,
    inc2: f32,
    index: f32,
    feedback: f32,
    index_env: f32,
    index_mul: f32,
}

impl Flux {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.p1 = 0.0;
        self.p2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
        self.index_env = 0.0;
    }

    pub fn trigger(&mut self) {
        // Phase reset for a consistent, click-free attack.
        self.reset();
        self.index_env = 1.0;
    }

    pub fn control(&mut self, c: &Ctx) {
        let max = 0.45 * c.fs;
        self.inc1 = c.hz.min(max) / c.fs;
        self.inc2 = (c.hz * map::flux_ratio(c.x)).min(max) / c.fs;
        // Harder hits drive the pair further into chaos. In turns: /2π.
        self.index = map::flux_index(c.y) * (0.4 + 0.6 * c.velocity) / core::f32::consts::TAU;
        self.feedback = 0.3 * c.z * c.z;
        self.index_mul = t60_gain(c.decay_s * INDEX_DECAY_RATIO, c.fs);
    }

    #[inline]
    pub fn tick(&mut self, env: f32) -> f32 {
        let m = self.index * self.index_env;
        self.index_env *= self.index_mul;
        let y2 = sin_turns(self.p2 + m * self.y1);
        let y1 = sin_turns(self.p1 + m * y2 + self.feedback * self.y1);
        self.y1 = y1;
        self.y2 = y2;
        self.p1 += self.inc1;
        if self.p1 >= 1.0 {
            self.p1 -= 1.0;
        }
        self.p2 += self.inc2;
        if self.p2 >= 1.0 {
            self.p2 -= 1.0;
        }
        env * (0.6 * y1 + 0.2 * y2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_index_is_a_pure_sine_at_pitch() {
        let fs = 48_000.0;
        let mut f = Flux::new();
        let mut c = Ctx::new(fs);
        c.hz = 480.0;
        c.y = 0.0;
        c.z = 0.0;
        f.control(&c);
        f.trigger();
        let out: Vec<f32> = (0..4_800).map(|_| f.tick(1.0)).collect();
        let crossings = out.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        assert!((crossings as i32 - 48).abs() <= 1);
        let peak = out.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        assert!(peak <= 1.05 + 1e-3);
    }
}
