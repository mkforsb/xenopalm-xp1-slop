//! The strike: a raised-cosine mallet pulse plus an optional noise burst.
//!
//! The exciter produces these signals:
//!
//! * `force`: what MODAL is hit with. The mallet pulse has unit area, so a
//!   resonator that responds to a unit impulse with a unit-amplitude ring
//!   responds to a soft mallet the same way at low frequencies. A softer
//!   mallet (longer contact) simply excites fewer high modes. The noise
//!   burst is scaled by the square root of its length so its energy is
//!   comparable.
//! * `mallet` and `pulse_len`: the unit-area pulse alone and its length. The
//!   delay-line engines don't integrate their input the way a modal bank
//!   does. They circulate it, so they scale the pulse by `min(pulse_len,
//!   period)/2` (see [`Excitation::displacement`]).
//! * `noise`: the burst on its own at audio level. The oscillator engines
//!   (GRAIN, SPECTRA, FLUX, LORENZ) layer it on top as a transient, and the
//!   delay-line engines take it in directly.

use crate::params::map;
use crate::util::{Noise, one_pole_coeff};

#[derive(Clone, Copy, Debug, Default)]
pub struct Excitation {
    /// Mallet (unit area × velocity) plus the energy-scaled noise burst.
    pub force: f32,
    /// Mallet pulse only.
    pub mallet: f32,
    /// Mallet contact length in samples.
    pub pulse_len: f32,
    /// Noise burst at audio level.
    pub noise: f32,
}

impl Excitation {
    /// Input for a delay-line resonator with a loop of `period` samples:
    /// the mallet scaled so a hit rings at about velocity amplitude whether
    /// the contact is shorter than one period (the pulse circulates) or
    /// longer (successive trips pile up), plus the noise burst.
    ///
    /// A contact much shorter than the period is a narrow pulse carrying
    /// little energy, so it gets a capped boost: hard mallets sound louder,
    /// as they do on real drums, without runaway click peaks.
    #[inline]
    pub fn displacement(&self, period: f32) -> f32 {
        let contact = self.pulse_len.min(period).max(1.0);
        let boost = (period / contact).powf(0.3).min(3.0);
        self.mallet * 0.5 * contact * boost + self.noise
    }
}

#[derive(Clone, Debug)]
pub struct Exciter {
    fs: f32,
    t: u32,
    pulse_len: u32,
    pulse_step: f32,
    pulse_gain: f32,
    noise_env: f32,
    noise_mul: f32,
    noise_lp: f32,
    noise_k: f32,
    noise_amt: f32,
    noise_force_gain: f32,
    amp: f32,
}

/// How strongly velocity adds to the STRIKE hardness.
const VELOCITY_HARDNESS: f32 = 0.3;

impl Exciter {
    pub fn new(fs: f32) -> Self {
        Self {
            fs,
            t: 0,
            pulse_len: 0,
            pulse_step: 0.0,
            pulse_gain: 0.0,
            noise_env: 0.0,
            noise_mul: 0.0,
            noise_lp: 0.0,
            noise_k: 1.0,
            noise_amt: 0.0,
            noise_force_gain: 0.0,
            amp: 0.0,
        }
    }

    /// Start a strike. `strike` and `noise` are the knob positions.
    pub fn trigger(&mut self, velocity: f32, strike: f32, noise: f32) {
        let fs = self.fs;
        let hardness = (strike + VELOCITY_HARDNESS * (velocity - 0.6)).clamp(0.0, 1.0);
        let len = (map::strike_seconds(hardness) * fs).max(2.0);
        self.t = 0;
        self.pulse_len = len as u32;
        self.pulse_step = 1.0 / self.pulse_len as f32;
        // A Hann window of N samples sums to N/2: normalize to unit area.
        self.pulse_gain = 2.0 / self.pulse_len as f32;
        self.amp = velocity;
        self.noise_amt = noise;
        if noise > 0.0 {
            let tau = map::noise_seconds(noise);
            self.noise_env = 1.0;
            self.noise_mul = (-1.0 / (tau * fs)).exp();
            let cutoff = (1_000.0 * 20f32.powf(hardness)).min(0.45 * fs);
            self.noise_k = one_pole_coeff(1.0 / (core::f32::consts::TAU * cutoff), fs);
            self.noise_force_gain = 1.5 / (tau * fs).sqrt();
        } else {
            self.noise_env = 0.0;
        }
    }

    #[inline]
    pub fn tick(&mut self, rng: &mut Noise) -> Excitation {
        let mut e = Excitation {
            pulse_len: self.pulse_len as f32,
            ..Default::default()
        };
        if self.t < self.pulse_len {
            let w = 0.5 - 0.5 * crate::util::cos_turns(self.t as f32 * self.pulse_step);
            e.mallet = w * self.pulse_gain * self.amp * (1.0 - 0.6 * self.noise_amt);
            e.force = e.mallet;
            self.t += 1;
        }
        if self.noise_env > 1e-4 {
            self.noise_lp += (rng.sample() - self.noise_lp) * self.noise_k;
            let n = self.noise_lp * self.noise_env * self.amp * self.noise_amt;
            self.noise_env *= self.noise_mul;
            e.noise = n;
            e.force += n * self.noise_force_gain;
        }
        e
    }

    pub fn reset(&mut self) {
        self.t = self.pulse_len;
        self.noise_env = 0.0;
        self.noise_lp = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pulse_has_unit_area_scaled_by_velocity() {
        for strike in [0.0, 0.5, 1.0] {
            let mut ex = Exciter::new(96_000.0);
            let mut rng = Noise::new(1);
            ex.trigger(0.6, strike, 0.0);
            let area: f32 = (0..2_000).map(|_| ex.tick(&mut rng).force).sum();
            assert!((area - 0.6).abs() < 0.01, "strike {strike}: area {area}");
        }
    }

    #[test]
    fn harder_strikes_are_shorter() {
        let len = |strike: f32| {
            let mut ex = Exciter::new(96_000.0);
            let mut rng = Noise::new(1);
            ex.trigger(1.0, strike, 0.0);
            (0..2_000).filter(|_| ex.tick(&mut rng).force > 0.0).count()
        };
        assert!(len(0.0) > 10 * len(1.0));
    }
}
