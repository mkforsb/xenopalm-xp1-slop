//! GRAIN: a cloud of windowed sine grains, spawned as a Poisson process whose
//! rate follows the hit's envelope, like shaken beads, rain on metal,
//! crackle or a swarm.
//!
//! * DENSITY is the grain rate at the peak of a full-velocity hit. The rate
//!   falls with the envelope, so the cloud thins out as it decays instead of
//!   just getting quieter.
//! * LENGTH is the grain duration: 0.8 ms clicks up to 80 ms tones.
//! * SPREAD scatters each grain's pitch randomly around PITCH, up to ±3
//!   octaves.
//!
//! Each grain also gets a random level and a small random downward chirp.
//! The sum is normalized by the expected overlap so density doesn't change
//! loudness much.

use super::Ctx;
use crate::params::map;
use crate::util::{Noise, cos_turns, sin_turns};

const MAX_GRAINS: usize = 32;
/// Grains fired at once on the hit, so every hit has a defined attack.
const ATTACK_GRAINS: usize = 3;

#[derive(Clone, Copy, Debug, Default)]
struct Grain {
    active: bool,
    phase: f32,
    inc: f32,
    chirp: f32,
    t: f32,
    dt: f32,
    amp: f32,
}

#[derive(Clone, Debug)]
pub struct Grains {
    grains: [Grain; MAX_GRAINS],
    rate: f32,
    len_s: f32,
    spread_oct: f32,
    norm: f32,
}

impl Grains {
    pub fn new() -> Self {
        Self {
            grains: [Grain::default(); MAX_GRAINS],
            rate: 0.0,
            len_s: 0.01,
            spread_oct: 0.0,
            norm: 1.0,
        }
    }

    pub fn reset(&mut self) {
        self.grains = [Grain::default(); MAX_GRAINS];
    }

    pub fn control(&mut self, c: &Ctx) {
        self.rate = map::grain_density(c.x);
        self.len_s = map::grain_seconds(c.y);
        self.spread_oct = map::grain_spread_octaves(c.z);
        let overlap = self.rate * self.len_s;
        self.norm = 0.9 / overlap.max(1.0).sqrt();
    }

    pub fn trigger(&mut self, c: &Ctx, rng: &mut Noise) {
        self.control(c);
        for _ in 0..ATTACK_GRAINS {
            self.spawn(c, c.velocity, rng);
        }
    }

    fn spawn(&mut self, c: &Ctx, env: f32, rng: &mut Noise) {
        let Some(g) = self.grains.iter_mut().find(|g| !g.active) else {
            return;
        };
        let hz = c.hz * (self.spread_oct * rng.sample()).exp2();
        if hz >= 0.45 * c.fs {
            return;
        }
        let len = (self.len_s * (0.6 + 0.8 * rng.uniform()) * c.fs).max(8.0);
        *g = Grain {
            active: true,
            phase: 0.0,
            inc: hz / c.fs,
            // Up to a third of an octave down over the grain.
            chirp: (-0.33 * rng.uniform() / len).exp2(),
            t: 0.0,
            dt: 1.0 / len,
            amp: env * (0.35 + 0.65 * rng.uniform()),
        };
    }

    #[inline]
    pub fn tick(&mut self, c: &Ctx, env: f32, rng: &mut Noise) -> f32 {
        if env > 1e-4 && rng.uniform() < self.rate * env / c.fs {
            self.spawn(c, env, rng);
        }
        let mut out = 0.0;
        for g in self.grains.iter_mut().filter(|g| g.active) {
            let w = 0.5 - 0.5 * cos_turns(g.t);
            out += g.amp * w * sin_turns(g.phase);
            g.phase += g.inc;
            if g.phase >= 1.0 {
                g.phase -= 1.0;
            }
            g.inc *= g.chirp;
            g.t += g.dt;
            if g.t >= 1.0 {
                g.active = false;
            }
        }
        out * self.norm
    }
}

impl Default for Grains {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_thins_out_with_the_envelope() {
        let fs = 48_000.0;
        let mut g = Grains::new();
        let mut rng = Noise::new(5);
        let mut c = Ctx::new(fs);
        c.hz = 1_000.0;
        c.x = 0.6;
        c.y = 0.2;
        c.z = 0.5;
        c.velocity = 1.0;
        g.trigger(&c, &mut rng);
        let mut env = 1.0f32;
        let mut counts = [0usize; 2];
        for n in 0..48_000 {
            c.env_norm = env;
            let active_before = g.grains.iter().filter(|g| g.active).count();
            let o = g.tick(&c, env, &mut rng);
            assert!(o.is_finite() && o.abs() < 4.0);
            let spawned = g
                .grains
                .iter()
                .filter(|g| g.active && g.t < 1.5 * g.dt)
                .count();
            if spawned > 0 && active_before < MAX_GRAINS {
                counts[n / 24_000] += 1;
            }
            env *= 0.9999;
        }
        assert!(counts[0] > counts[1] * 2, "{counts:?}");
    }
}
