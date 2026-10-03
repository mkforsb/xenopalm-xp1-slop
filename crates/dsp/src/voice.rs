//! One drum channel: the Xenokussion XK-1 voice, unchanged except that a
//! hit can carry an [`Articulation`] (see [`Voice::strike`]).
//!
//! ```text
//!  PITCH ─┬─ sweep EG ─┬─ chaos ─┬─ jitter ─► pitch, X, Y, Z ─┐
//!         │            │         │                           ▼
//!  TRIGGER ─► exciter (mallet + noise) ──────────────► ENGINE (1 of 8) ─► FOLD ─► FILTER ─► CRUSH ─► LEVEL ─► out
//!         └─► amp EG ──────────────────────────────────────┘                        ▲
//!                   └──────────────────────────────── × ENV ─────────────────────────┘
//! ```
//!
//! Controls are evaluated once per [`CONTROL_BLOCK`] samples (parameter
//! smoothing, modulation, engine coefficients). The envelopes, the exciter
//! and the audio path run every sample.

use crate::engines::{Ctx, Engines};
use crate::exciter::Exciter;
use crate::modulation::ChaosLfo;
use crate::params::{
    CHAOS_CUTOFF, CHAOS_PITCH, CHAOS_X, CHAOS_Y, CHAOS_Z, FILTER_BP, FILTER_HP, PARAM_COUNT, Param,
    VoicePatch, default_patch, map,
};
use crate::stroke::{Articulation, Strike};
use crate::util::{Noise, Svf, SvfCoeffs, one_pole_coeff, sin_turns, t60_gain};

/// Samples per control update.
pub const CONTROL_BLOCK: usize = 16;
/// Amplitude envelope attack (the trigger pulse charging the envelope).
const ATTACK_S: f32 = 0.000_6;
/// Parameter de-zippering.
const SMOOTH_TAU_S: f32 = 0.004;
/// Chaos modulation range at full depth.
const CHAOS_PITCH_OCT: f32 = 2.0;
const CHAOS_MACRO: f32 = 0.5;
const CHAOS_CUTOFF_OCT: f32 = 4.0;
/// Jitter range at full setting.
const JITTER_PITCH_OCT: f32 = 1.0;
const JITTER_MACRO: f32 = 0.35;

/// Which parameters are de-zippered (the continuous ones).
const SMOOTHED: [bool; PARAM_COUNT] = {
    let mut s = [true; PARAM_COUNT];
    s[Param::Engine as usize] = false;
    s[Param::ChaosDest as usize] = false;
    s[Param::FilterType as usize] = false;
    s
};

#[derive(Clone, Debug)]
pub struct Voice {
    fs: f32,
    target: VoicePatch,
    smooth: VoicePatch,
    k_smooth: f32,

    noise: Noise,
    exciter: Exciter,
    engines: Engines,
    engine: u32,
    ctx: Ctx,
    block_pos: usize,

    /// Amplitude envelope, 0..velocity.
    env: f32,
    env_mul: f32,
    attack_left: u32,
    attack_len: u32,
    k_attack: f32,
    velocity: f32,
    /// Sweep envelope, 1 → 0.
    sweep: f32,
    sweep_mul: f32,

    chaos: ChaosLfo,
    /// Per-hit offsets: pitch (octaves), X, Y, Z.
    jitter: [f32; 4],
    /// How the current hit was played.
    art: Articulation,

    svf: Svf,
    svf_coeffs: SvfCoeffs,
    filter_type: u32,
    fold_drive: f32,
    fold_mix: f32,
    crush_hold: u32,
    crush_count: u32,
    crush_value: f32,
    crush_step: f32,
    level: f32,
}

impl Voice {
    /// `fs` is the rate `tick` is called at (the oversampled rate).
    pub fn new(fs: f32, seed: u32) -> Self {
        let patch = default_patch();
        let mut v = Self {
            fs,
            target: patch,
            smooth: patch,
            k_smooth: one_pole_coeff(SMOOTH_TAU_S, fs / CONTROL_BLOCK as f32),
            noise: Noise::new(seed),
            exciter: Exciter::new(fs),
            engines: Engines::new(fs),
            engine: 0,
            ctx: Ctx::new(fs),
            block_pos: 0,
            env: 0.0,
            env_mul: 0.0,
            attack_left: 0,
            attack_len: ((ATTACK_S * fs) as u32).max(1),
            k_attack: one_pole_coeff(ATTACK_S / 3.0, fs),
            velocity: 1.0,
            sweep: 0.0,
            sweep_mul: 0.0,
            chaos: ChaosLfo::new(seed),
            jitter: [0.0; 4],
            art: Articulation::NEUTRAL,
            svf: Svf::default(),
            svf_coeffs: SvfCoeffs::new(1_000.0, 0.7, fs),
            filter_type: 0,
            fold_drive: 1.0,
            fold_mix: 0.0,
            crush_hold: 1,
            crush_count: 0,
            crush_value: 0.0,
            crush_step: 0.0,
            level: 0.0,
        };
        v.control();
        v
    }

    pub fn set_param(&mut self, param: Param, value: f32) {
        let v = param.sanitize(value);
        self.target[param.index()] = v;
        if !SMOOTHED[param.index()] {
            self.smooth[param.index()] = v;
        }
    }

    /// The whole patch (targets).
    pub fn patch(&self) -> VoicePatch {
        self.target
    }

    pub fn load_patch(&mut self, patch: &VoicePatch) {
        for p in crate::params::ALL_PARAMS {
            self.set_param(p, patch[p.index()]);
        }
    }

    pub fn param(&self, param: Param) -> f32 {
        self.target[param.index()]
    }

    /// The de-zippered value the voice is currently using.
    pub fn smoothed(&self, param: Param) -> f32 {
        self.smooth[param.index()]
    }

    /// Jump all smoothed parameters to their targets.
    pub fn snap_params(&mut self) {
        self.smooth = self.target;
        self.control();
    }

    /// Fire the trigger. `velocity` is the raw hit strength `0..=1`; SENSE
    /// scales it like a trigger input's preamp.
    pub fn trigger(&mut self, velocity: f32) {
        self.art = Articulation::NEUTRAL;
        self.fire(velocity);
    }

    /// Play a stroke: the trigger, articulated by how the hand hit.
    pub fn strike(&mut self, s: Strike) {
        self.art = Articulation::of(&s);
        self.fire(s.velocity * self.art.gain);
    }

    fn fire(&mut self, velocity: f32) {
        let gain = map::sense_gain(self.target[Param::Sense.index()]);
        self.velocity = (velocity.clamp(0.0, 1.0) * gain).clamp(0.02, 1.0);
        self.attack_left = self.attack_len;
        self.sweep = 1.0;
        let amount = self.smooth[Param::Jitter.index()];
        self.jitter = if amount > 0.0 {
            [
                JITTER_PITCH_OCT * amount * amount * self.noise.sample(),
                JITTER_MACRO * amount * self.noise.sample(),
                JITTER_MACRO * amount * self.noise.sample(),
                JITTER_MACRO * amount * self.noise.sample(),
            ]
        } else {
            [0.0; 4]
        };
        // Evaluate the controls now so the engine sees this hit's pitch,
        // jitter and velocity from its first sample.
        self.control();
        self.block_pos = 1;
        let p = &self.smooth;
        self.exciter.trigger(
            self.velocity,
            (p[Param::Strike.index()] + self.art.hardness).clamp(0.0, 1.0),
            (p[Param::Noise.index()] + self.art.noise).clamp(0.0, 1.0),
        );
        self.engines
            .trigger(self.engine, &self.ctx, &mut self.noise);
    }

    /// Chaos LED: lit on one lobe of the attractor while it modulates.
    pub fn lfo_led(&self) -> bool {
        self.smooth[Param::ChaosDepth.index()] > 0.0 && self.chaos.value() > 0.0
    }

    /// Current amplitude envelope (for metering).
    pub fn envelope(&self) -> f32 {
        self.env
    }

    /// The selected engine.
    pub fn engine(&self) -> u32 {
        self.engine
    }

    /// Control-rate update: smoothing, modulation, coefficients.
    fn control(&mut self) {
        let fs = self.fs;
        let block_s = CONTROL_BLOCK as f32 / fs;
        for ((cur, &target), &smoothed) in self.smooth.iter_mut().zip(&self.target).zip(&SMOOTHED) {
            if smoothed {
                *cur += (target - *cur) * self.k_smooth;
            }
        }
        let p = self.smooth;
        let param = |q: Param| p[q.index()];

        let engine = param(Param::Engine) as u32;
        if engine != self.engine {
            self.engines.reset(engine);
            self.engine = engine;
        }

        // Modulation.
        self.chaos.advance(param(Param::ChaosRate), block_s);
        let depth = param(Param::ChaosDepth);
        let chaos = self.chaos.value() * depth * depth;
        let dest = param(Param::ChaosDest) as u32;
        let chaos_to = |d: u32| if dest == d { chaos } else { 0.0 };

        let decay_s = map::decay_seconds(param(Param::Decay)) * self.art.decay;
        let sweep_oct = map::sweep_octaves(param(Param::SweepDepth)) * self.sweep;
        let oct = map::pitch_octaves(param(Param::Pitch), param(Param::Fine))
            + sweep_oct
            + CHAOS_PITCH_OCT * chaos_to(CHAOS_PITCH)
            + self.jitter[0]
            + self.art.pitch;
        let macro_value =
            |q: Param, d: u32, j: f32| (param(q) + CHAOS_MACRO * chaos_to(d) + j).clamp(0.0, 1.0);
        self.ctx = Ctx {
            fs,
            hz: oct.exp2().clamp(5.0, 0.45 * fs),
            x: macro_value(Param::MacroX, CHAOS_X, self.jitter[1]),
            y: macro_value(Param::MacroY, CHAOS_Y, self.jitter[2] + self.art.macro_y),
            z: macro_value(Param::MacroZ, CHAOS_Z, self.jitter[3]),
            decay_s,
            velocity: self.velocity,
            // Count the attack as the peak, so engines that scan with the
            // envelope start at the beginning.
            env_norm: if self.attack_left > 0 {
                1.0
            } else {
                (self.env / self.velocity).clamp(0.0, 1.0)
            },
        };
        self.engines.control(self.engine, &self.ctx);

        self.env_mul = t60_gain(decay_s, fs);
        self.sweep_mul = (-1.0 / (map::sweep_seconds(param(Param::SweepSpeed)) * fs)).exp();

        // Filter: the envelope moves the cutoff, the chaos can too.
        let cut_oct = map::cutoff_hz(param(Param::Cutoff)).log2()
            + map::filter_env_octaves(param(Param::FilterEnv)) * self.env
            + CHAOS_CUTOFF_OCT * chaos_to(CHAOS_CUTOFF);
        self.svf_coeffs = SvfCoeffs::new(cut_oct.exp2(), map::reso_q(param(Param::Reso)), fs);
        self.filter_type = param(Param::FilterType) as u32;

        let fold = param(Param::Fold);
        self.fold_drive = 1.0 + 6.0 * fold * fold;
        self.fold_mix = (fold * 4.0).min(1.0);

        let crush = param(Param::Crush);
        // Hold for up to 48 samples; quantize from 16 bits down to 3.
        self.crush_hold = 1 + (crush * crush * 47.0) as u32;
        self.crush_step = if crush > 0.0 {
            (13.0 * crush - 15.0).exp2()
        } else {
            0.0
        };
        self.level = map::level_gain(param(Param::Level));
    }

    #[inline]
    pub fn tick(&mut self) -> f32 {
        if self.block_pos == 0 {
            self.control();
        }
        self.block_pos += 1;
        if self.block_pos == CONTROL_BLOCK {
            self.block_pos = 0;
        }

        // Envelopes.
        if self.attack_left > 0 {
            self.attack_left -= 1;
            self.env += (self.velocity - self.env).max(0.0) * self.k_attack;
        } else {
            self.env *= self.env_mul;
            if self.env < 1e-6 {
                self.env = 0.0;
            }
        }
        self.sweep *= self.sweep_mul;
        if self.sweep < 1e-6 {
            self.sweep = 0.0;
        }

        let exc = self.exciter.tick(&mut self.noise);
        let mut x = self
            .engines
            .tick(self.engine, &self.ctx, exc, self.env, &mut self.noise);
        if !x.is_finite() {
            // Never let a numerical accident poison the channel.
            self.engines.reset(self.engine);
            self.svf.reset();
            x = 0.0;
        }

        // Wavefolder.
        if self.fold_mix > 0.0 {
            let folded = sin_turns(0.25 * self.fold_drive * x);
            x += (folded - x) * self.fold_mix;
        }

        // Filter.
        let (lp, bp, hp) = self.svf.process(x, &self.svf_coeffs);
        x = match self.filter_type {
            FILTER_BP => bp,
            FILTER_HP => hp,
            _ => lp,
        };

        // Crusher.
        if self.crush_step > 0.0 {
            if self.crush_count == 0 {
                self.crush_value = (x / self.crush_step).round() * self.crush_step;
            }
            self.crush_count += 1;
            if self.crush_count >= self.crush_hold {
                self.crush_count = 0;
            }
            x = self.crush_value;
        }

        x * self.level
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{ENGINE_COUNT, SPECTRA_RND_THRESHOLD};

    fn render(v: &mut Voice, n: usize) -> Vec<f32> {
        (0..n).map(|_| v.tick()).collect()
    }

    fn peak(x: &[f32]) -> f32 {
        x.iter().fold(0.0f32, |a, &b| a.max(b.abs()))
    }

    #[test]
    fn each_engine_sounds_and_decays() {
        let fs = 96_000.0;
        for engine in 0..ENGINE_COUNT {
            let mut v = Voice::new(fs, 1);
            v.set_param(Param::Engine, engine as f32);
            v.set_param(Param::Decay, 0.3);
            v.set_param(Param::Noise, 0.0);
            v.snap_params();
            v.trigger(1.0);
            let early = peak(&render(&mut v, 9_600));
            let _ = render(&mut v, fs as usize * 3);
            let late = peak(&render(&mut v, 9_600));
            assert!(early > 0.02, "engine {engine}: quiet ({early})");
            assert!(early < 2.5, "engine {engine}: loud ({early})");
            assert!(late < early * 0.01, "engine {engine}: {early} -> {late}");
        }
    }

    #[test]
    fn sweep_down_starts_high() {
        let fs = 96_000.0;
        let mut v = Voice::new(fs, 1);
        v.set_param(Param::Engine, 6.0); // FLUX: a sine at index 0
        v.set_param(Param::MacroY, 0.0);
        v.set_param(Param::MacroZ, 0.0);
        v.set_param(Param::Pitch, 0.3);
        v.set_param(Param::SweepDepth, 0.85);
        v.set_param(Param::SweepSpeed, 0.4);
        v.set_param(Param::Decay, 0.9);
        v.snap_params();
        v.trigger(1.0);
        let out = render(&mut v, 96_000);
        let zc = |s: &[f32]| s.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        let first = zc(&out[..9_600]);
        let last = zc(&out[out.len() - 9_600..]);
        assert!(first > last * 2, "{first} vs {last}");
    }

    #[test]
    fn jitter_makes_hits_differ() {
        let fs = 96_000.0;
        let mut v = Voice::new(fs, 7);
        v.set_param(Param::Engine, 6.0);
        v.set_param(Param::MacroY, 0.0);
        v.set_param(Param::Jitter, 1.0);
        v.set_param(Param::Decay, 0.8);
        v.snap_params();
        let mut pitches = Vec::new();
        for _ in 0..4 {
            v.trigger(1.0);
            let out = render(&mut v, 19_200);
            pitches.push(out.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count());
        }
        let (lo, hi) = (pitches.iter().min().unwrap(), pitches.iter().max().unwrap());
        assert!(*hi as f32 > *lo as f32 * 1.1, "{pitches:?}");
    }

    #[test]
    fn spectra_rnd_differs_per_hit() {
        let fs = 48_000.0;
        let mut v = Voice::new(fs, 3);
        v.set_param(Param::Engine, 5.0);
        v.set_param(Param::MacroX, SPECTRA_RND_THRESHOLD + 0.01);
        v.snap_params();
        v.trigger(1.0);
        let a = render(&mut v, 4_800);
        v.trigger(1.0);
        let b = render(&mut v, 4_800);
        assert_ne!(a, b);
    }

    #[test]
    fn a_touch_of_crush_is_nearly_transparent() {
        let fs = 48_000.0;
        let run = |crush: f32| {
            let mut v = Voice::new(fs, 1);
            v.set_param(Param::Crush, crush);
            v.snap_params();
            v.trigger(1.0);
            render(&mut v, 4_800)
        };
        let (clean, touched) = (run(0.0), run(0.001));
        let err = clean
            .iter()
            .zip(&touched)
            .fold(0.0f32, |a, (x, y)| a.max((x - y).abs()));
        assert!(peak(&touched) > 0.5 * peak(&clean));
        assert!(err < 1e-3, "{err}");
    }

    #[test]
    fn extremes_stay_finite() {
        let fs = 96_000.0;
        for engine in 0..ENGINE_COUNT {
            for &corner in &[0.0f32, 1.0] {
                let mut v = Voice::new(fs, 9);
                v.set_param(Param::Engine, engine as f32);
                for p in [
                    Param::Pitch,
                    Param::Decay,
                    Param::MacroX,
                    Param::MacroY,
                    Param::MacroZ,
                    Param::Strike,
                    Param::Noise,
                    Param::SweepDepth,
                    Param::Reso,
                    Param::Fold,
                    Param::Crush,
                ] {
                    v.set_param(p, corner);
                }
                v.set_param(Param::ChaosDepth, 1.0);
                v.set_param(Param::ChaosRate, 1.0);
                v.snap_params();
                v.trigger(1.0);
                for s in render(&mut v, 48_000) {
                    assert!(
                        s.is_finite() && s.abs() < 50.0,
                        "engine {engine} corner {corner}: {s}"
                    );
                }
            }
        }
    }
}
