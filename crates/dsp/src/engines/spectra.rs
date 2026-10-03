//! SPECTRA: a random wavetable, played additively so it can be warped in ways
//! a sampled table can't.
//!
//! A table is four spectral frames of 24 partial amplitudes, drawn from a
//! seeded generator. Each frame has its own roll-off, sparsity and a few
//! emphasized "formant" partials.
//!
//! * SEED picks one of 64 fixed tables. At the top of its travel (RND) every
//!   hit rolls a brand new table.
//! * SCAN is how far the hit sweeps through the frames as it decays. Each
//!   partial scans at a different speed (upper partials run ahead), like
//!   Vital's "spectral time skew", so the timbre smears as it evolves
//!   instead of crossfading.
//! * STRETCH is Vital-style "harmonic stretch". Partial k sits at `k^e`
//!   times the fundamental, compressing (e < 1) or stretching (e > 1) the
//!   series into bell- and gong-like inharmonicity. The fundamental stays
//!   put.
//!
//! Upper partials also decay faster than the fundamental, as in most struck
//! objects. Partials above Nyquist are simply skipped, so it never aliases.

use super::Ctx;
use crate::params::map;
use crate::util::{Noise, cos_turns, sin_turns};

pub const PARTIALS: usize = 24;
pub const FRAMES: usize = 4;

pub type Table = [[f32; PARTIALS]; FRAMES];

/// Build the table for a seed. Deterministic: the same seed always gives the
/// same table, so presets and the panel's preview are repeatable.
pub fn generate(seed: u32) -> Table {
    let mut rng = Noise::new(seed.wrapping_add(0x5eed));
    let mut table = [[0.0; PARTIALS]; FRAMES];
    for frame in table.iter_mut() {
        let tilt = 0.4 + 1.4 * rng.uniform();
        let sparsity = 0.65 * rng.uniform();
        for (k, a) in frame.iter_mut().enumerate() {
            let n = (k + 1) as f32;
            let r = rng.uniform();
            *a = if k > 0 && rng.uniform() < sparsity {
                0.0
            } else {
                r * r * n.powf(-tilt)
            };
        }
        // A couple of emphasized partials give each frame a character.
        for _ in 0..2 {
            let k = 1 + (rng.uniform() * (PARTIALS - 1) as f32) as usize;
            frame[k.min(PARTIALS - 1)] += 0.3 + 0.4 * rng.uniform();
        }
        // Keep a solid fundamental so PITCH means something.
        let max = frame.iter().cloned().fold(0.0f32, f32::max);
        frame[0] = frame[0].max(0.6 * max);
        let sum: f32 = frame.iter().sum();
        frame.iter_mut().for_each(|a| *a /= sum.max(1e-6));
    }
    table
}

#[derive(Clone, Debug)]
pub struct Spectra {
    table: Table,
    /// Seed the current table was built from (`None` = random).
    seed: Option<u32>,
    /// Unit phasors, one per partial: im is the sine output.
    re: [f32; PARTIALS],
    im: [f32; PARTIALS],
    /// Per-sample rotation.
    cr: [f32; PARTIALS],
    ci: [f32; PARTIALS],
    amp: [f32; PARTIALS],
}

const OUTPUT_GAIN: f32 = 1.3;

impl Spectra {
    pub fn new() -> Self {
        Self {
            table: generate(0),
            seed: Some(0),
            re: [1.0; PARTIALS],
            im: [0.0; PARTIALS],
            cr: [1.0; PARTIALS],
            ci: [0.0; PARTIALS],
            amp: [0.0; PARTIALS],
        }
    }

    pub fn reset(&mut self) {
        self.re = [1.0; PARTIALS];
        self.im = [0.0; PARTIALS];
    }

    /// The table currently loaded (for the panel's display).
    pub fn table(&self) -> &Table {
        &self.table
    }

    pub fn trigger(&mut self, c: &Ctx, rng: &mut Noise) {
        match map::spectra_seed(c.x) {
            None => {
                self.table = generate(rng.next_u32());
                self.seed = None;
            }
            Some(s) if self.seed != Some(s) => {
                self.table = generate(s);
                self.seed = Some(s);
            }
            Some(_) => {}
        }
        // Restart every partial at zero phase: a clean, click-free onset.
        self.reset();
        self.control(c);
    }

    pub fn control(&mut self, c: &Ctx) {
        // Follow the SEED knob between hits too, so turning it is audible
        // on a ringing note.
        if let Some(s) = map::spectra_seed(c.x)
            && self.seed != Some(s)
        {
            self.table = generate(s);
            self.seed = Some(s);
        }
        let e = map::stretch_exponent(c.z);
        let scan = c.y * (1.0 - c.env_norm).clamp(0.0, 1.0) * (FRAMES - 1) as f32;
        for k in 0..PARTIALS {
            let n = (k + 1) as f32;
            let hz = c.hz * n.powf(e);
            // Keep the phasor on the unit circle despite rounding.
            let m2 = self.re[k] * self.re[k] + self.im[k] * self.im[k];
            let fix = 1.5 - 0.5 * m2;
            self.re[k] *= fix;
            self.im[k] *= fix;
            if hz >= 0.45 * c.fs {
                self.amp[k] = 0.0;
                self.cr[k] = 1.0;
                self.ci[k] = 0.0;
                continue;
            }
            let w = hz / c.fs;
            self.cr[k] = cos_turns(w);
            self.ci[k] = sin_turns(w);
            // Spectral time skew: partial k scans (1 + 1.5·k/K) times as fast.
            let pos = (scan * (1.0 + 1.5 * k as f32 / PARTIALS as f32)).min((FRAMES - 1) as f32);
            let i = (pos as usize).min(FRAMES - 2);
            let f = pos - i as f32;
            let a = self.table[i][k] * (1.0 - f) + self.table[i + 1][k] * f;
            // Upper partials decay faster.
            let decay = c.env_norm.clamp(0.0, 1.0).powf(0.05 * k as f32);
            self.amp[k] = a * decay * OUTPUT_GAIN;
        }
    }

    #[inline]
    pub fn tick(&mut self, env: f32) -> f32 {
        let mut out = 0.0;
        for k in 0..PARTIALS {
            let (re, im) = (self.re[k], self.im[k]);
            self.re[k] = re * self.cr[k] - im * self.ci[k];
            self.im[k] = re * self.ci[k] + im * self.cr[k];
            out += self.amp[k] * self.im[k];
        }
        out * env
    }
}

impl Default for Spectra {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_are_deterministic_and_normalized() {
        assert_eq!(generate(7), generate(7));
        assert_ne!(generate(7), generate(8));
        for seed in 0..64 {
            for frame in generate(seed) {
                let sum: f32 = frame.iter().sum();
                assert!((sum - 1.0).abs() < 1e-4);
                assert!(frame[0] > 0.0);
            }
        }
    }

    #[test]
    fn rnd_seed_rolls_a_new_table_each_hit() {
        let mut s = Spectra::new();
        let mut rng = Noise::new(3);
        let mut c = Ctx::new(48_000.0);
        c.x = 1.0;
        s.trigger(&c, &mut rng);
        let a = *s.table();
        s.trigger(&c, &mut rng);
        assert_ne!(a, *s.table());
        // A fixed seed keeps its table.
        c.x = 0.5;
        s.trigger(&c, &mut rng);
        let b = *s.table();
        s.trigger(&c, &mut rng);
        assert_eq!(b, *s.table());
    }

    #[test]
    fn no_partial_above_nyquist() {
        let mut s = Spectra::new();
        let mut c = Ctx::new(48_000.0);
        c.hz = 3_000.0;
        c.z = 1.0;
        c.env_norm = 1.0;
        s.control(&c);
        for k in 0..PARTIALS {
            // Anything with a level must rotate slower than 0.45 turns/sample.
            if s.amp[k] > 0.0 {
                assert!(s.ci[k].atan2(s.cr[k]).abs() / core::f32::consts::TAU < 0.45);
            }
        }
    }
}
