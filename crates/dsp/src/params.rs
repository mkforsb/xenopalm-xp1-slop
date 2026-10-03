//! Parameter definitions shared by the engine, the wasm worklet and the UI.
//!
//! There are two kinds: [`Param`]s exist once per drum channel and are the
//! Xenokussion XK-1's voice controls (its eight engines, exciter, sweep,
//! chaos, jitter, filter and dirt), plus MUTATE. [`GlobalParam`]s cover the
//! transport, the player, the shared room and the master level.
//!
//! Every parameter is stored as an `f32`. Continuous parameters are normalized
//! to `0.0..=1.0` (think "knob position"); choice parameters hold the index of
//! the selected option and integers the number itself. The [`map`] module converts knob positions into
//! physical units and is used by both the DSP and the UI read-outs, so what the
//! panel displays is exactly what the engine does.
//!
//! The three macro knobs (X, Y, Z) mean something different for every engine;
//! [`macro_label`] and [`macro_display`] give their per-engine names.

/// Per-voice parameters. The discriminant is the stable wire id.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Param {
    /// Synthesis engine (see [`ENGINE_NAMES`]).
    Engine = 0,
    /// Coarse pitch.
    Pitch,
    /// Fine tune.
    Fine,
    /// Decay time (resonator T60 or amplitude envelope, depending on the engine).
    Decay,
    /// Engine macro 1.
    MacroX,
    /// Engine macro 2.
    MacroY,
    /// Engine macro 3.
    MacroZ,
    /// Exciter hardness: soft mallet … hard click.
    Strike,
    /// Exciter noise burst amount and length.
    Noise,
    /// Pitch sweep time.
    SweepSpeed,
    /// Pitch sweep depth, bipolar (centre = off, up = starts high).
    SweepDepth,
    /// Chaos modulator (Lorenz attractor) rate.
    ChaosRate,
    /// Chaos modulator depth.
    ChaosDepth,
    /// Chaos modulator destination.
    ChaosDest,
    /// Per-hit random offsets of pitch and macros.
    Jitter,
    /// Filter cutoff.
    Cutoff,
    /// Filter resonance.
    Reso,
    /// Envelope → cutoff amount, bipolar.
    FilterEnv,
    /// Filter type: LP / BP / HP.
    FilterType,
    /// Sine wavefolder drive.
    Fold,
    /// Sample-rate and bit-depth reduction.
    Crush,
    /// Channel output level.
    Level,
    /// Trigger sensitivity.
    Sense,
    /// Stereo position.
    Pan,
    /// Send to the shared room (post channel output level).
    ReverbMix,
    /// MUTATE: 0 = off … 1 = a completely new random patch on every hit.
    /// A performance control: never randomized itself.
    Mutate,
    /// Engine lock: MUTATE leaves the engine choice alone (the rest of the
    /// patch still mutates). A performance control too.
    EngineLock,
}

pub const PARAM_COUNT: usize = 27;

pub const ALL_PARAMS: [Param; PARAM_COUNT] = [
    Param::Engine,
    Param::Pitch,
    Param::Fine,
    Param::Decay,
    Param::MacroX,
    Param::MacroY,
    Param::MacroZ,
    Param::Strike,
    Param::Noise,
    Param::SweepSpeed,
    Param::SweepDepth,
    Param::ChaosRate,
    Param::ChaosDepth,
    Param::ChaosDest,
    Param::Jitter,
    Param::Cutoff,
    Param::Reso,
    Param::FilterEnv,
    Param::FilterType,
    Param::Fold,
    Param::Crush,
    Param::Level,
    Param::Sense,
    Param::Pan,
    Param::ReverbMix,
    Param::Mutate,
    Param::EngineLock,
];

pub const ENGINE_MODAL: u32 = 0;
pub const ENGINE_STRING: u32 = 1;
pub const ENGINE_COMB: u32 = 2;
pub const ENGINE_GONG: u32 = 3;
pub const ENGINE_GRAIN: u32 = 4;
pub const ENGINE_SPECTRA: u32 = 5;
pub const ENGINE_FLUX: u32 = 6;
pub const ENGINE_LORENZ: u32 = 7;
pub const ENGINE_COUNT: usize = 8;

pub const ENGINE_NAMES: &[&str] = &[
    "MODAL", "STRING", "COMB", "GONG", "GRAIN", "SPECTRA", "FLUX", "LORENZ",
];

/// One-line description of each engine, for the panel.
pub const ENGINE_BLURBS: [&str; ENGINE_COUNT] = [
    "Modal resonator bank: 16 struck modes, morphing between materials",
    "Waveguide string with stiffness and tension-modulated pitch",
    "Three tuned comb resonators with diffusion and cross-coupling",
    "Feedback delay network with an energy-scattering nonlinearity",
    "Grain cloud whose density decays with the hit",
    "Random additive wavetable, re-rolled per hit, with spectral stretch",
    "Cross-coupled phase-modulation pair that tips into chaos",
    "Lorenz attractor at audio rate: damped spiral or strange orbit",
];

pub const CHAOS_DEST_NAMES: &[&str] = &["PITCH", "X", "Y", "Z", "CUTOFF"];
pub const CHAOS_PITCH: u32 = 0;
pub const CHAOS_X: u32 = 1;
pub const CHAOS_Y: u32 = 2;
pub const CHAOS_Z: u32 = 3;
pub const CHAOS_CUTOFF: u32 = 4;

pub const FILTER_TYPE_NAMES: &[&str] = &["LP", "BP", "HP"];

pub const ENGINE_LOCK_NAMES: &[&str] = &["FREE", "LOCKED"];
pub const FILTER_LP: u32 = 0;
pub const FILTER_BP: u32 = 1;
pub const FILTER_HP: u32 = 2;

/// MODAL: the materials the STRUCT knob morphs between, in knob order.
pub const MATERIAL_NAMES: [&str; 6] = ["MEMBRANE", "STRING", "MARIMBA", "BAR", "PLATE", "BELL"];

/// SPECTRA: knob positions at or above this draw a new random table per hit.
pub const SPECTRA_RND_THRESHOLD: f32 = 0.96;
/// SPECTRA: number of distinct fixed seeds below the RND position.
pub const SPECTRA_SEEDS: u32 = 64;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParamKind {
    /// `0.0..=1.0`.
    Continuous,
    /// Index into the names.
    Choice(&'static [&'static str]),
    /// Whole numbers in `min..=max`.
    Int { min: i32, max: i32 },
}

impl ParamKind {
    pub fn sanitize(self, v: f32, default: f32) -> f32 {
        let v = if v.is_finite() { v } else { default };
        match self {
            ParamKind::Continuous => v.clamp(0.0, 1.0),
            ParamKind::Choice(names) => v.round().clamp(0.0, (names.len() - 1) as f32),
            ParamKind::Int { min, max } => v.round().clamp(min as f32, max as f32),
        }
    }

    /// Value → `0..=1` position for a knob or fader.
    pub fn to_knob(self, v: f32) -> f32 {
        match self {
            ParamKind::Continuous => v,
            ParamKind::Choice(names) => v / (names.len() - 1).max(1) as f32,
            ParamKind::Int { min, max } => (v - min as f32) / (max - min).max(1) as f32,
        }
    }

    /// `0..=1` knob position → value (unrounded; sanitize afterwards).
    pub fn from_knob(self, k: f32) -> f32 {
        match self {
            ParamKind::Continuous => k,
            ParamKind::Choice(names) => k * (names.len() - 1) as f32,
            ParamKind::Int { min, max } => min as f32 + k * (max - min) as f32,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ParamInfo {
    /// Descriptive name.
    pub name: &'static str,
    /// Panel label.
    pub label: &'static str,
    pub default: f32,
    pub kind: ParamKind,
}

impl Param {
    pub fn id(self) -> u32 {
        self as u32
    }

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn from_id(id: u32) -> Option<Param> {
        ALL_PARAMS.get(id as usize).copied()
    }

    pub fn info(self) -> ParamInfo {
        use ParamKind::*;
        let (name, label, default, kind) = match self {
            Param::Engine => ("Engine", "ENGINE", 0.0, Choice(ENGINE_NAMES)),
            Param::Pitch => ("Pitch", "PITCH", 0.42, Continuous),
            Param::Fine => ("Fine tune", "FINE", 0.5, Continuous),
            Param::Decay => ("Decay", "DECAY", 0.5, Continuous),
            Param::MacroX => ("Macro X", "X", 0.35, Continuous),
            Param::MacroY => ("Macro Y", "Y", 0.3, Continuous),
            Param::MacroZ => ("Macro Z", "Z", 0.4, Continuous),
            Param::Strike => ("Strike hardness", "STRIKE", 0.5, Continuous),
            Param::Noise => ("Noise burst", "NOISE", 0.1, Continuous),
            Param::SweepSpeed => ("Sweep time", "TIME", 0.5, Continuous),
            Param::SweepDepth => ("Sweep depth", "DEPTH", 0.5, Continuous),
            Param::ChaosRate => ("Chaos rate", "RATE", 0.4, Continuous),
            Param::ChaosDepth => ("Chaos depth", "DEPTH", 0.0, Continuous),
            Param::ChaosDest => (
                "Chaos destination",
                "DEST",
                CHAOS_PITCH as f32,
                Choice(CHAOS_DEST_NAMES),
            ),
            Param::Jitter => ("Per-hit jitter", "JITTER", 0.0, Continuous),
            Param::Cutoff => ("Filter cutoff", "CUTOFF", 1.0, Continuous),
            Param::Reso => ("Filter resonance", "RESO", 0.1, Continuous),
            Param::FilterEnv => ("Filter envelope", "ENV", 0.5, Continuous),
            Param::FilterType => (
                "Filter type",
                "TYPE",
                FILTER_LP as f32,
                Choice(FILTER_TYPE_NAMES),
            ),
            Param::Fold => ("Wavefolder", "FOLD", 0.0, Continuous),
            Param::Crush => ("Crusher", "CRUSH", 0.0, Continuous),
            Param::Level => ("Output", "OUTPUT", 0.75, Continuous),
            Param::Sense => ("Sense", "SENSE", 0.5, Continuous),
            Param::Pan => ("Pan", "PAN", 0.5, Continuous),
            Param::ReverbMix => ("Room send", "ROOM", 0.3, Continuous),
            Param::Mutate => ("Mutate on every hit", "MUTATE", 0.0, Continuous),
            Param::EngineLock => ("Engine lock", "LOCK", 0.0, Choice(ENGINE_LOCK_NAMES)),
        };
        ParamInfo {
            name,
            label,
            default,
            kind,
        }
    }

    pub fn default_value(self) -> f32 {
        self.info().default
    }

    /// Clamp/quantize a raw value to what this parameter accepts.
    pub fn sanitize(self, v: f32) -> f32 {
        let info = self.info();
        info.kind.sanitize(v, info.default)
    }

    /// Bipolar controls are centred at 0.5 (the panel lights them from the middle).
    pub fn is_bipolar(self) -> bool {
        matches!(
            self,
            Param::Fine | Param::SweepDepth | Param::FilterEnv | Param::Pan
        )
    }

    /// The macro slot (0 = X, 1 = Y, 2 = Z) of a macro parameter.
    pub fn macro_slot(self) -> Option<usize> {
        match self {
            Param::MacroX => Some(0),
            Param::MacroY => Some(1),
            Param::MacroZ => Some(2),
            _ => None,
        }
    }

    /// Panel label, resolving macro knobs to the given engine's names.
    pub fn label_for(self, engine: u32) -> &'static str {
        match self.macro_slot() {
            Some(slot) => macro_label(engine, slot),
            None => self.info().label,
        }
    }

    /// Descriptive name, resolving macro knobs to the given engine's names.
    pub fn name_for(self, engine: u32) -> &'static str {
        match self.macro_slot() {
            Some(slot) => macro_name(engine, slot),
            None => self.info().name,
        }
    }

    /// Human readable value for the panel read-out. Macro knobs need the
    /// engine; use [`Param::display_for`] for those.
    pub fn display(self, v: f32) -> String {
        self.display_for(ENGINE_MODAL, v)
    }

    /// Human readable value, resolving macro knobs for the given engine.
    pub fn display_for(self, engine: u32, v: f32) -> String {
        if let Some(slot) = self.macro_slot() {
            return macro_display(engine, slot, v);
        }
        match self {
            Param::Engine | Param::ChaosDest | Param::FilterType | Param::EngineLock => {
                match self.info().kind {
                    ParamKind::Choice(names) => names[self.sanitize(v) as usize].to_string(),
                    _ => unreachable!(),
                }
            }
            Param::Pitch => fmt_hz(map::pitch_hz(v, 0.5)),
            Param::Fine => format!("{:+.0} ct", map::fine_semitones(v) * 100.0),
            Param::Decay => fmt_secs(map::decay_seconds(v)),
            Param::Strike => format!("{:.0}%", v * 100.0),
            Param::Noise => {
                if v <= 0.0 {
                    "OFF".into()
                } else {
                    fmt_secs(map::noise_seconds(v))
                }
            }
            Param::SweepSpeed => fmt_secs(map::sweep_seconds(v)),
            Param::SweepDepth => {
                let o = map::sweep_octaves(v);
                if o.abs() < 0.005 {
                    "OFF".into()
                } else {
                    format!("{:+.2} oct", o)
                }
            }
            Param::ChaosRate => fmt_hz(map::chaos_hz(v)),
            Param::ChaosDepth | Param::Jitter | Param::Fold | Param::Crush => {
                if v <= 0.0 {
                    "OFF".into()
                } else {
                    format!("{:.0}%", v * 100.0)
                }
            }
            Param::Cutoff => fmt_hz(map::cutoff_hz(v)),
            Param::Reso => format!("Q {:.1}", map::reso_q(v)),
            Param::FilterEnv => format!("{:+.1} oct", map::filter_env_octaves(v)),
            Param::Level => fmt_db(map::level_gain(v)),
            Param::Sense => format!("x{:.2}", map::sense_gain(v)),
            Param::Pan => {
                let p = (v - 0.5) * 200.0;
                if p.abs() < 1.0 {
                    "C".to_string()
                } else if p < 0.0 {
                    format!("L{:.0}", -p)
                } else {
                    format!("R{:.0}", p)
                }
            }
            Param::ReverbMix => fmt_db(map::reverb_gain(v)),
            Param::Mutate => {
                if v <= 0.0 {
                    "OFF".into()
                } else if v >= 1.0 {
                    "RND".into()
                } else {
                    format!("{:.0}%", v * 100.0)
                }
            }
            Param::MacroX | Param::MacroY | Param::MacroZ => unreachable!(),
        }
    }
}

/// Short panel label of an engine macro knob.
pub fn macro_label(engine: u32, slot: usize) -> &'static str {
    const LABELS: [[&str; 3]; ENGINE_COUNT] = [
        ["STRUCT", "POS", "DAMP"],
        ["STIFF", "POS", "TENSION"],
        ["SPREAD", "DIFFUSE", "TWIST"],
        ["SIZE", "CRASH", "DAMP"],
        ["DENSITY", "LENGTH", "SPREAD"],
        ["SEED", "SCAN", "STRETCH"],
        ["RATIO", "INDEX", "FEEDBK"],
        ["RHO", "SIGMA", "AXIS"],
    ];
    LABELS[(engine as usize).min(ENGINE_COUNT - 1)][slot.min(2)]
}

/// Descriptive name of an engine macro knob.
pub fn macro_name(engine: u32, slot: usize) -> &'static str {
    const NAMES: [[&str; 3]; ENGINE_COUNT] = [
        [
            "Structure (material)",
            "Strike position",
            "High-mode damping",
        ],
        [
            "Stiffness (dispersion)",
            "Pluck position",
            "Tension modulation",
        ],
        ["Comb spread", "Diffusion", "Twist (polarity ↔ coupling)"],
        ["Size spread", "Crash (nonlinear scattering)", "Damping"],
        ["Grain density", "Grain length", "Pitch spread"],
        ["Table seed", "Spectral scan", "Harmonic stretch"],
        ["Frequency ratio", "Coupling index", "Self-feedback"],
        ["Rho (chaos)", "Sigma (shape)", "Output axis"],
    ];
    NAMES[(engine as usize).min(ENGINE_COUNT - 1)][slot.min(2)]
}

/// Read-out of an engine macro knob.
pub fn macro_display(engine: u32, slot: usize, v: f32) -> String {
    let pct = || format!("{:.0}%", v * 100.0);
    match (engine, slot) {
        (ENGINE_MODAL, 0) => {
            let (i, f) = map::material_segment(v);
            if f < 0.02 {
                MATERIAL_NAMES[i].to_string()
            } else if f > 0.98 {
                MATERIAL_NAMES[i + 1].to_string()
            } else {
                format!(
                    "{}>{} {:.0}%",
                    &MATERIAL_NAMES[i][..3],
                    &MATERIAL_NAMES[i + 1][..3],
                    f * 100.0
                )
            }
        }
        (ENGINE_MODAL | ENGINE_STRING, 1) => format!("{:.2}", map::strike_position(v)),
        (ENGINE_STRING, 0) => pct(),
        (ENGINE_STRING, 2) => format!("{:+.2} oct", map::tension_octaves(v)),
        (ENGINE_COMB, 0) => {
            let r = map::comb_ratios(v);
            format!("1:{:.2}:{:.2}", r[1], r[2])
        }
        (ENGINE_COMB, 2) => {
            let t = (v - 0.5) * 2.0;
            if t.abs() < 0.02 {
                "PLAIN".into()
            } else if t < 0.0 {
                format!("NEG {:.0}%", -t * 100.0)
            } else {
                format!("COUPLE {:.0}%", t * 100.0)
            }
        }
        (ENGINE_GRAIN, 0) => format!("{:.0}/s", map::grain_density(v)),
        (ENGINE_GRAIN, 1) => fmt_secs(map::grain_seconds(v)),
        (ENGINE_GRAIN, 2) => format!("±{:.1} oct", map::grain_spread_octaves(v)),
        (ENGINE_SPECTRA, 0) => match map::spectra_seed(v) {
            Some(s) => format!("#{:02}", s),
            None => "RND".into(),
        },
        (ENGINE_SPECTRA, 2) => format!("k^{:.2}", map::stretch_exponent(v)),
        (ENGINE_FLUX, 0) => format!("x{:.3}", map::flux_ratio(v)),
        (ENGINE_FLUX, 1) => format!("{:.2}", map::flux_index(v)),
        (ENGINE_LORENZ, 0) => format!("ρ {:.1}", map::lorenz_rho(v)),
        (ENGINE_LORENZ, 1) => format!("σ {:.1}", map::lorenz_sigma(v)),
        (ENGINE_LORENZ, 2) => {
            if v < 0.34 {
                "X".into()
            } else if v < 0.67 {
                "Y".into()
            } else {
                "Z".into()
            }
        }
        _ => pct(),
    }
}

pub fn fmt_hz(hz: f32) -> String {
    if hz >= 1000.0 {
        format!("{:.2} kHz", hz / 1000.0)
    } else if hz >= 10.0 {
        format!("{:.0} Hz", hz)
    } else {
        format!("{:.2} Hz", hz)
    }
}

pub fn fmt_db(gain: f32) -> String {
    if gain <= 1e-4 {
        "-inf dB".to_string()
    } else {
        format!("{:.1} dB", 20.0 * gain.log10())
    }
}

pub fn fmt_secs(s: f32) -> String {
    if s >= 1.0 {
        format!("{:.2} s", s)
    } else if s >= 0.01 {
        format!("{:.0} ms", s * 1000.0)
    } else {
        format!("{:.1} ms", s * 1000.0)
    }
}

// --- global parameters ---------------------------------------------------------

pub const GRID_NAMES: &[&str] = &["1/16", "1/8T"];
pub const GRID_STRAIGHT: u32 = 0;
pub const GRID_TRIPLET: u32 = 1;

/// Global parameters. The discriminant is the stable wire id.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GlobalParam {
    /// Beats per minute.
    Tempo = 0,
    /// 16th-note swing: 50 % (straight) … 75 %.
    Swing,
    /// Pattern length in steps.
    Length,
    /// Step grid: 16ths or triplet 8ths (12/8 feel).
    Grid,
    /// Human timing: drift and scatter around the grid.
    Timing,
    /// Human dynamics: velocity scatter and drift.
    Velo,
    /// Strike-position scatter.
    Spot,
    /// How freely the player embellishes the written pattern.
    Improv,
    ReverbDecay,
    ReverbTone,
    ReverbPredelay,
    EqLow,
    EqMid,
    EqHigh,
    Master,
}

pub const GLOBAL_PARAM_COUNT: usize = 15;

pub const ALL_GLOBAL_PARAMS: [GlobalParam; GLOBAL_PARAM_COUNT] = [
    GlobalParam::Tempo,
    GlobalParam::Swing,
    GlobalParam::Length,
    GlobalParam::Grid,
    GlobalParam::Timing,
    GlobalParam::Velo,
    GlobalParam::Spot,
    GlobalParam::Improv,
    GlobalParam::ReverbDecay,
    GlobalParam::ReverbTone,
    GlobalParam::ReverbPredelay,
    GlobalParam::EqLow,
    GlobalParam::EqMid,
    GlobalParam::EqHigh,
    GlobalParam::Master,
];

pub type GlobalPatch = [f32; GLOBAL_PARAM_COUNT];

pub const MIN_TEMPO: i32 = 40;
pub const MAX_TEMPO: i32 = 240;
pub const MAX_STEPS: usize = 64;

impl GlobalParam {
    pub fn id(self) -> u32 {
        self as u32
    }

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn from_id(id: u32) -> Option<Self> {
        ALL_GLOBAL_PARAMS.get(id as usize).copied()
    }

    pub fn info(self) -> ParamInfo {
        use ParamKind::*;
        let (name, label, default, kind) = match self {
            GlobalParam::Tempo => (
                "Tempo",
                "TEMPO",
                100.0,
                Int {
                    min: MIN_TEMPO,
                    max: MAX_TEMPO,
                },
            ),
            GlobalParam::Swing => ("Swing", "SWING", 0.0, Continuous),
            GlobalParam::Length => (
                "Pattern length",
                "LENGTH",
                16.0,
                Int {
                    min: 1,
                    max: MAX_STEPS as i32,
                },
            ),
            GlobalParam::Grid => ("Step grid", "GRID", 0.0, Choice(GRID_NAMES)),
            GlobalParam::Timing => ("Human timing", "TIMING", 0.25, Continuous),
            GlobalParam::Velo => ("Human dynamics", "VELO", 0.3, Continuous),
            GlobalParam::Spot => ("Strike-position scatter", "SPOT", 0.3, Continuous),
            GlobalParam::Improv => ("Improvisation", "IMPROV", 0.15, Continuous),
            GlobalParam::ReverbDecay => ("Room decay", "DECAY", 0.3, Continuous),
            GlobalParam::ReverbTone => ("Room tone", "TONE", 0.6, Continuous),
            GlobalParam::ReverbPredelay => ("Room pre-delay", "PRE", 0.1, Continuous),
            GlobalParam::EqLow => ("Room EQ low", "LOW", 0.5, Continuous),
            GlobalParam::EqMid => ("Room EQ mid", "MID", 0.5, Continuous),
            GlobalParam::EqHigh => ("Room EQ high", "HIGH", 0.5, Continuous),
            GlobalParam::Master => ("Master volume", "MASTER", 0.8, Continuous),
        };
        ParamInfo {
            name,
            label,
            default,
            kind,
        }
    }

    pub fn default_value(self) -> f32 {
        self.info().default
    }

    pub fn sanitize(self, v: f32) -> f32 {
        let info = self.info();
        info.kind.sanitize(v, info.default)
    }

    pub fn is_bipolar(self) -> bool {
        matches!(
            self,
            GlobalParam::EqLow | GlobalParam::EqMid | GlobalParam::EqHigh
        )
    }

    pub fn display(self, v: f32) -> String {
        let pct_or_off = || {
            if v <= 0.0 {
                "OFF".to_string()
            } else {
                format!("{:.0}%", v * 100.0)
            }
        };
        match self {
            GlobalParam::Tempo => format!("{:.0} BPM", v),
            GlobalParam::Swing => format!("{:.0}%", map::swing_ratio(v) * 100.0),
            GlobalParam::Length => format!("{:.0} steps", v),
            GlobalParam::Grid => GRID_NAMES[self.sanitize(v) as usize].to_string(),
            GlobalParam::Timing => {
                if v <= 0.0 {
                    "OFF".into()
                } else {
                    format!("±{:.0} ms", map::timing_ms(v))
                }
            }
            GlobalParam::Velo | GlobalParam::Spot | GlobalParam::Improv => pct_or_off(),
            GlobalParam::ReverbDecay => fmt_secs(map::reverb_seconds(v)),
            GlobalParam::ReverbTone => fmt_hz(map::reverb_tone_hz(v)),
            GlobalParam::ReverbPredelay => fmt_secs(map::reverb_predelay_seconds(v)),
            GlobalParam::EqLow | GlobalParam::EqMid | GlobalParam::EqHigh => {
                format!("{:+.1} dB", map::eq_db(v))
            }
            GlobalParam::Master => fmt_db(map::level_gain(v)),
        }
    }
}

pub fn default_global_patch() -> GlobalPatch {
    ALL_GLOBAL_PARAMS.map(GlobalParam::default_value)
}

/// Knob position → physical unit mappings.
pub mod map {
    use super::{SPECTRA_RND_THRESHOLD, SPECTRA_SEEDS};

    /// Lowest pitch at the bottom of the PITCH fader.
    pub const PITCH_MIN_HZ: f32 = 20.0;
    /// Span of the PITCH fader in octaves.
    pub const PITCH_OCTAVES: f32 = 8.0;

    #[inline]
    fn bipolar(v: f32) -> f32 {
        (v - 0.5) * 2.0
    }

    pub fn fine_semitones(fine: f32) -> f32 {
        bipolar(fine) * 2.0
    }

    pub fn pitch_octaves(pitch: f32, fine: f32) -> f32 {
        PITCH_MIN_HZ.log2() + pitch * PITCH_OCTAVES + fine_semitones(fine) / 12.0
    }

    pub fn pitch_hz(pitch: f32, fine: f32) -> f32 {
        pitch_octaves(pitch, fine).exp2()
    }

    /// -60 dB decay time.
    pub fn decay_seconds(decay: f32) -> f32 {
        0.015 * 800f32.powf(decay)
    }

    /// Noise burst decay time constant.
    pub fn noise_seconds(noise: f32) -> f32 {
        0.001 + 0.08 * noise * noise
    }

    /// Mallet contact time: soft (long) to hard (short).
    pub fn strike_seconds(hardness: f32) -> f32 {
        0.006 * (0.000_08f32 / 0.006).powf(hardness.clamp(0.0, 1.0))
    }

    /// Pitch sweep time constant. Higher knob = shorter.
    pub fn sweep_seconds(speed: f32) -> f32 {
        1.5 * (0.002f32 / 1.5).powf(speed)
    }

    /// Bipolar sweep depth in octaves: above centre the hit starts high and
    /// falls back to the pitch, below it starts low and rises.
    pub fn sweep_octaves(depth: f32) -> f32 {
        let x = bipolar(depth);
        6.0 * x * x.abs().sqrt()
    }

    /// Approximate orbit rate of the chaos modulator.
    pub fn chaos_hz(rate: f32) -> f32 {
        0.05 * 600f32.powf(rate)
    }

    pub fn cutoff_hz(cutoff: f32) -> f32 {
        20.0 * 1000f32.powf(cutoff)
    }

    pub fn reso_q(reso: f32) -> f32 {
        0.5 * 60f32.powf(reso)
    }

    pub fn filter_env_octaves(env: f32) -> f32 {
        let x = bipolar(env);
        8.0 * x * x.abs().sqrt()
    }

    pub fn level_gain(level: f32) -> f32 {
        level * level
    }

    pub fn sense_gain(sense: f32) -> f32 {
        0.25 * 16f32.powf(sense)
    }

    pub fn reverb_gain(amount: f32) -> f32 {
        amount * amount
    }

    /// Approximate RT60 of the plate.
    pub fn reverb_seconds(decay: f32) -> f32 {
        0.3 * 40f32.powf(decay)
    }

    /// Damping lowpass cutoff (input bandwidth and tank damping).
    pub fn reverb_tone_hz(tone: f32) -> f32 {
        1500.0 * 12f32.powf(tone)
    }

    pub fn reverb_predelay_seconds(predelay: f32) -> f32 {
        0.15 * predelay
    }

    /// Swing: where the off-16th lands within its pair, 0.5 = straight.
    pub fn swing_ratio(swing: f32) -> f32 {
        0.5 + 0.25 * swing.clamp(0.0, 1.0)
    }

    /// Standard deviation of the per-hit timing scatter.
    pub fn timing_ms(timing: f32) -> f32 {
        12.0 * timing * timing.sqrt().max(timing)
    }

    /// Room EQ band gain, ±12 dB around the centre.
    pub fn eq_db(v: f32) -> f32 {
        (v - 0.5) * 24.0
    }

    // --- engine macros ---------------------------------------------------

    /// MODAL STRUCT: (lower material index, fraction towards the next one).
    pub fn material_segment(structure: f32) -> (usize, f32) {
        let s = structure.clamp(0.0, 1.0) * 5.0;
        let i = (s as usize).min(4);
        (i, s - i as f32)
    }

    /// MODAL/STRING position along the body: near the edge to the centre.
    pub fn strike_position(pos: f32) -> f32 {
        0.02 + 0.48 * pos
    }

    /// STRING TENSION: pitch rise in octaves at full amplitude.
    pub fn tension_octaves(tension: f32) -> f32 {
        1.5 * tension * tension
    }

    /// COMB SPREAD: the three comb frequency ratios.
    pub fn comb_ratios(spread: f32) -> [f32; 3] {
        let s = spread.clamp(0.0, 1.0);
        [1.0, (0.585 * 1.2 * s).exp2(), (1.26 * 1.2 * s).exp2()]
    }

    /// GRAIN DENSITY in grains per second at the start of a full-velocity hit.
    pub fn grain_density(density: f32) -> f32 {
        8.0 * 400f32.powf(density)
    }

    /// GRAIN LENGTH.
    pub fn grain_seconds(length: f32) -> f32 {
        0.000_8 * 100f32.powf(length)
    }

    /// GRAIN SPREAD: random pitch range, ± octaves.
    pub fn grain_spread_octaves(spread: f32) -> f32 {
        3.0 * spread * spread
    }

    /// SPECTRA SEED: a fixed table number, or `None` for a new table per hit.
    pub fn spectra_seed(seed: f32) -> Option<u32> {
        if seed >= SPECTRA_RND_THRESHOLD {
            None
        } else {
            Some(((seed / SPECTRA_RND_THRESHOLD) * SPECTRA_SEEDS as f32) as u32)
        }
    }

    /// SPECTRA STRETCH: partial k sits at `k^exponent` times the fundamental.
    pub fn stretch_exponent(stretch: f32) -> f32 {
        1.0 + bipolar(stretch) * 0.6
    }

    /// FLUX RATIO: the modulator/carrier frequency ratio (interpolated in
    /// log space between musically interesting points).
    pub fn flux_ratio(ratio: f32) -> f32 {
        const POINTS: [f32; 8] = [0.5, 1.0, 1.414, 2.0, 2.756, 3.5, 5.404, 7.13];
        let s = ratio.clamp(0.0, 1.0) * (POINTS.len() - 1) as f32;
        let i = (s as usize).min(POINTS.len() - 2);
        let f = s - i as f32;
        (POINTS[i].ln() * (1.0 - f) + POINTS[i + 1].ln() * f).exp()
    }

    /// FLUX INDEX: peak phase-modulation index (radians-ish).
    pub fn flux_index(index: f32) -> f32 {
        8.0 * index * index
    }

    pub fn lorenz_rho(rho: f32) -> f32 {
        8.0 + 40.0 * rho
    }

    pub fn lorenz_sigma(sigma: f32) -> f32 {
        4.0 + 16.0 * sigma
    }

    /// Rho above which the Lorenz fixed points turn unstable (the subcritical
    /// Hopf bifurcation, β = 8/3): below it a hit rings down by itself, above
    /// it the state wanders the strange attractor.
    pub fn lorenz_hopf_rho(sigma: f32) -> f32 {
        const BETA: f32 = 8.0 / 3.0;
        let s = sigma;
        s * (s + BETA + 3.0) / (s - BETA - 1.0)
    }
}

/// A complete voice patch: one value per [`Param`], indexed by `Param::index`.
pub type VoicePatch = [f32; PARAM_COUNT];

pub fn default_patch() -> VoicePatch {
    let mut p = [0.0; PARAM_COUNT];
    for param in ALL_PARAMS {
        p[param.index()] = param.default_value();
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_roundtrip() {
        for (i, p) in ALL_PARAMS.iter().enumerate() {
            assert_eq!(p.index(), i);
            assert_eq!(Param::from_id(p.id()), Some(*p));
        }
        for (i, p) in ALL_GLOBAL_PARAMS.iter().enumerate() {
            assert_eq!(p.index(), i);
            assert_eq!(GlobalParam::from_id(p.id()), Some(*p));
        }
        assert_eq!(Param::from_id(PARAM_COUNT as u32), None);
    }

    #[test]
    fn sanitize_quantizes_choices() {
        assert_eq!(Param::Engine.sanitize(2.4), 2.0);
        assert_eq!(Param::Engine.sanitize(99.0), 7.0);
        assert_eq!(Param::Pitch.sanitize(1.5), 1.0);
        assert_eq!(GlobalParam::Tempo.sanitize(1000.0), MAX_TEMPO as f32);
        assert_eq!(GlobalParam::Length.sanitize(0.0), 1.0);
        assert_eq!(
            Param::Pitch.sanitize(f32::NAN),
            Param::Pitch.default_value()
        );
    }

    #[test]
    fn mapping_ranges() {
        assert!((map::pitch_hz(0.0, 0.5) - 20.0).abs() < 0.01);
        assert!((map::pitch_hz(1.0, 0.5) - 5120.0).abs() < 1.0);
        assert!((map::sweep_seconds(1.0) - 0.002).abs() < 1e-5);
        assert!((map::decay_seconds(1.0) - 12.0).abs() < 1e-3);
        assert!((map::cutoff_hz(1.0) - 20_000.0).abs() < 1.0);
        assert_eq!(map::sweep_octaves(0.5), 0.0);
        assert!(map::sweep_octaves(1.0) > 5.9 && map::sweep_octaves(0.0) < -5.9);
        assert_eq!(map::spectra_seed(1.0), None);
        assert_eq!(map::spectra_seed(0.0), Some(0));
        assert_eq!(map::spectra_seed(0.959), Some(SPECTRA_SEEDS - 1));
        assert!((map::flux_ratio(0.0) - 0.5).abs() < 1e-4);
        assert!((map::flux_ratio(1.0) - 7.13).abs() < 1e-3);
        assert_eq!(map::material_segment(1.0), (4, 1.0));
    }

    #[test]
    fn display_is_sane() {
        assert_eq!(Param::Engine.display(3.0), "GONG");
        assert_eq!(Param::Pan.display(0.5), "C");
        assert_eq!(Param::Pitch.display(1.0), "5.12 kHz");
        assert_eq!(Param::SweepDepth.display(0.5), "OFF");
        assert_eq!(Param::MacroX.display_for(ENGINE_MODAL, 0.0), "MEMBRANE");
        assert_eq!(Param::MacroX.display_for(ENGINE_MODAL, 1.0), "BELL");
        assert_eq!(Param::MacroX.display_for(ENGINE_SPECTRA, 1.0), "RND");
        assert_eq!(Param::MacroX.label_for(ENGINE_LORENZ), "RHO");
        assert_eq!(Param::Decay.label_for(ENGINE_LORENZ), "DECAY");
    }

    #[test]
    fn every_macro_has_a_label_and_display() {
        for e in 0..ENGINE_COUNT as u32 {
            for slot in 0..3 {
                assert!(!macro_label(e, slot).is_empty());
                assert!(!macro_name(e, slot).is_empty());
                for v in [0.0, 0.33, 0.5, 1.0] {
                    assert!(!macro_display(e, slot, v).is_empty());
                }
            }
        }
    }
}
