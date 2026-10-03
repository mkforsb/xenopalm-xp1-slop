//! The eight synthesis engines.
//!
//! Every engine sees the same small interface. [`Ctx`] carries the per-block
//! control values (pitch after sweep/chaos/jitter, the three macros, decay),
//! `control` recomputes coefficients once per control block, and `tick`
//! renders one sample given the exciter output and the amplitude envelope.
//!
//! The resonator engines (MODAL, STRING, COMB, GONG) are driven by the
//! exciter's `force` and decay on their own: DECAY sets their losses. The
//! oscillator engines (GRAIN, SPECTRA, FLUX, LORENZ) are shaped by the
//! amplitude envelope and get the noise burst layered on top.

pub mod comb;
pub mod flux;
pub mod gong;
pub mod grain;
pub mod lorenz;
pub mod modal;
pub mod spectra;
pub mod string;

use crate::exciter::Excitation;
use crate::util::Noise;

/// Lowest frequency the delay-based engines allocate for.
pub const MIN_DELAY_HZ: f32 = 12.0;

/// Control values for one block.
#[derive(Clone, Copy, Debug)]
pub struct Ctx {
    /// Rate `tick` is called at.
    pub fs: f32,
    /// Pitch in Hz, after sweep, chaos and jitter.
    pub hz: f32,
    /// Engine macros, `0..=1`, after chaos and jitter.
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// -60 dB decay time in seconds.
    pub decay_s: f32,
    /// Velocity of the current hit after SENSE, `0..=1`.
    pub velocity: f32,
    /// Amplitude envelope relative to the hit's velocity (1 at the peak, then
    /// decaying towards 0).
    pub env_norm: f32,
}

impl Ctx {
    pub fn new(fs: f32) -> Self {
        Self {
            fs,
            hz: 200.0,
            x: 0.5,
            y: 0.5,
            z: 0.5,
            decay_s: 0.5,
            velocity: 1.0,
            env_norm: 0.0,
        }
    }
}

/// All engines, each with its own state. Only the selected one runs.
#[derive(Clone, Debug)]
pub struct Engines {
    pub modal: modal::Modal,
    pub string: string::KString,
    pub comb: comb::Comb,
    pub gong: gong::Gong,
    pub grain: grain::Grains,
    pub spectra: spectra::Spectra,
    pub flux: flux::Flux,
    pub lorenz: lorenz::Lorenz,
}

impl Engines {
    pub fn new(fs: f32) -> Self {
        Self {
            modal: modal::Modal::new(),
            string: string::KString::new(fs),
            comb: comb::Comb::new(fs),
            gong: gong::Gong::new(fs),
            grain: grain::Grains::new(),
            spectra: spectra::Spectra::new(),
            flux: flux::Flux::new(),
            lorenz: lorenz::Lorenz::new(fs),
        }
    }

    pub fn reset(&mut self, engine: u32) {
        match engine {
            0 => self.modal.reset(),
            1 => self.string.reset(),
            2 => self.comb.reset(),
            3 => self.gong.reset(),
            4 => self.grain.reset(),
            5 => self.spectra.reset(),
            6 => self.flux.reset(),
            _ => self.lorenz.reset(),
        }
    }

    pub fn control(&mut self, engine: u32, c: &Ctx) {
        match engine {
            0 => self.modal.control(c),
            1 => self.string.control(c),
            2 => self.comb.control(c),
            3 => self.gong.control(c),
            4 => self.grain.control(c),
            5 => self.spectra.control(c),
            6 => self.flux.control(c),
            _ => self.lorenz.control(c),
        }
    }

    pub fn trigger(&mut self, engine: u32, c: &Ctx, rng: &mut Noise) {
        match engine {
            0..=3 => {}
            4 => self.grain.trigger(c, rng),
            5 => self.spectra.trigger(c, rng),
            6 => self.flux.trigger(),
            _ => self.lorenz.trigger(c),
        }
    }

    /// One sample. `env` is the velocity-scaled amplitude envelope.
    #[inline]
    pub fn tick(&mut self, engine: u32, c: &Ctx, e: Excitation, env: f32, rng: &mut Noise) -> f32 {
        match engine {
            0 => self.modal.tick(e.force),
            1 => self.string.tick(&e),
            2 => self.comb.tick(&e),
            3 => self.gong.tick(&e),
            4 => self.grain.tick(c, env, rng) + e.noise,
            5 => self.spectra.tick(env) + e.noise,
            6 => self.flux.tick(env) + e.noise,
            _ => self.lorenz.tick(env) + e.noise,
        }
    }
}
