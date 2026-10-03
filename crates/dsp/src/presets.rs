//! Factory voice presets.

use crate::params::{
    CHAOS_CUTOFF, CHAOS_PITCH, CHAOS_X, CHAOS_Z, ENGINE_COMB, ENGINE_FLUX, ENGINE_GONG,
    ENGINE_GRAIN, ENGINE_LORENZ, ENGINE_MODAL, ENGINE_SPECTRA, ENGINE_STRING, FILTER_BP, FILTER_HP,
    Param, VoicePatch, default_patch,
};

pub struct Preset {
    pub name: &'static str,
    pub values: &'static [(Param, f32)],
}

impl Preset {
    /// Full patch: defaults overlaid with this preset's values.
    pub fn patch(&self) -> VoicePatch {
        let mut p = default_patch();
        for &(param, v) in self.values {
            p[param.index()] = param.sanitize(v);
        }
        p
    }
}

const MODAL: f32 = ENGINE_MODAL as f32;
const STRING: f32 = ENGINE_STRING as f32;
const COMB: f32 = ENGINE_COMB as f32;
const GONG: f32 = ENGINE_GONG as f32;
const GRAIN: f32 = ENGINE_GRAIN as f32;
const SPECTRA: f32 = ENGINE_SPECTRA as f32;
const FLUX: f32 = ENGINE_FLUX as f32;
const LORENZ: f32 = ENGINE_LORENZ as f32;
const BP: f32 = FILTER_BP as f32;
const HP: f32 = FILTER_HP as f32;

use Param::*;

pub const PRESETS: &[Preset] = &[
    Preset {
        name: "Init",
        values: &[],
    },
    // --- MODAL ---------------------------------------------------------
    Preset {
        name: "Glass Marimba",
        values: &[
            (Engine, MODAL),
            (Pitch, 0.55),
            (Decay, 0.5),
            (MacroX, 0.4),
            (MacroY, 0.35),
            (MacroZ, 0.6),
            (Strike, 0.55),
            (Noise, 0.0),
            (ReverbMix, 0.35),
        ],
    },
    Preset {
        name: "Temple Bell",
        values: &[
            (Engine, MODAL),
            (Pitch, 0.5),
            (Decay, 0.85),
            (MacroX, 1.0),
            (MacroY, 0.2),
            (MacroZ, 0.1),
            (Strike, 0.7),
            (Noise, 0.0),
            (ReverbMix, 0.5),
        ],
    },
    Preset {
        name: "Impossible Metal",
        values: &[
            (Engine, MODAL),
            (Pitch, 0.45),
            (Decay, 0.7),
            (MacroX, 0.7),
            (MacroY, 0.6),
            (MacroZ, 0.2),
            (Strike, 0.75),
            (ChaosRate, 0.5),
            (ChaosDepth, 0.6),
            (ChaosDest, CHAOS_X as f32),
            (ReverbMix, 0.4),
        ],
    },
    Preset {
        name: "Membrane Tom",
        values: &[
            (Engine, MODAL),
            (Pitch, 0.3),
            (Decay, 0.45),
            (MacroX, 0.0),
            (MacroY, 0.25),
            (MacroZ, 0.5),
            (Strike, 0.45),
            (Noise, 0.15),
            (SweepDepth, 0.62),
            (SweepSpeed, 0.55),
            (ReverbMix, 0.3),
        ],
    },
    Preset {
        name: "Crushed Bar",
        values: &[
            (Engine, MODAL),
            (Pitch, 0.5),
            (Decay, 0.5),
            (MacroX, 0.6),
            (MacroY, 0.4),
            (MacroZ, 0.3),
            (Strike, 0.8),
            (Fold, 0.3),
            (Crush, 0.45),
            (ReverbMix, 0.2),
        ],
    },
    // --- STRING --------------------------------------------------------
    Preset {
        name: "Tabla Boing",
        values: &[
            (Engine, STRING),
            (Pitch, 0.38),
            (Decay, 0.5),
            (MacroX, 0.15),
            (MacroY, 0.25),
            (MacroZ, 0.85),
            (Strike, 0.7),
            (Noise, 0.05),
            (ReverbMix, 0.25),
        ],
    },
    Preset {
        name: "Piano Wire",
        values: &[
            (Engine, STRING),
            (Pitch, 0.45),
            (Decay, 0.75),
            (MacroX, 0.8),
            (MacroY, 0.12),
            (MacroZ, 0.1),
            (Strike, 0.9),
            (Noise, 0.0),
            (ReverbMix, 0.35),
        ],
    },
    Preset {
        name: "Slack Floor Tom",
        values: &[
            (Engine, STRING),
            (Pitch, 0.22),
            (Decay, 0.55),
            (MacroX, 0.3),
            (MacroY, 0.5),
            (MacroZ, 1.0),
            (Strike, 0.4),
            (Noise, 0.2),
            (Cutoff, 0.7),
            (ReverbMix, 0.3),
        ],
    },
    // --- COMB ----------------------------------------------------------
    Preset {
        name: "Hollow Pipe",
        values: &[
            (Engine, COMB),
            (Pitch, 0.45),
            (Decay, 0.45),
            (MacroX, 0.0),
            (MacroY, 0.0),
            (MacroZ, 0.0),
            (Strike, 0.3),
            (Noise, 0.5),
            (ReverbMix, 0.35),
        ],
    },
    Preset {
        name: "Spring Box",
        values: &[
            (Engine, COMB),
            (Pitch, 0.35),
            (Decay, 0.6),
            (MacroX, 0.45),
            (MacroY, 0.85),
            (MacroZ, 0.5),
            (Strike, 0.8),
            (Noise, 0.2),
            (ReverbMix, 0.25),
        ],
    },
    Preset {
        name: "Sympathy",
        values: &[
            (Engine, COMB),
            (Pitch, 0.5),
            (Decay, 0.8),
            (MacroX, 0.7),
            (MacroY, 0.2),
            (MacroZ, 1.0),
            (Strike, 0.6),
            (Noise, 0.1),
            (ReverbMix, 0.4),
        ],
    },
    Preset {
        name: "Mutant Snare",
        values: &[
            (Engine, COMB),
            (Pitch, 0.45),
            (Decay, 0.3),
            (MacroX, 0.3),
            (MacroY, 0.5),
            (MacroZ, 0.6),
            (Strike, 0.9),
            (Noise, 0.8),
            (FilterType, HP),
            (Cutoff, 0.25),
            (ReverbMix, 0.3),
        ],
    },
    // --- GONG ----------------------------------------------------------
    Preset {
        name: "Crash Gong",
        values: &[
            (Engine, GONG),
            (Pitch, 0.25),
            (Decay, 0.85),
            (MacroX, 0.5),
            (MacroY, 0.9),
            (MacroZ, 0.3),
            (Strike, 0.8),
            (Noise, 0.1),
            (ReverbMix, 0.5),
        ],
    },
    Preset {
        name: "Sheet Metal",
        values: &[
            (Engine, GONG),
            (Pitch, 0.45),
            (Decay, 0.5),
            (MacroX, 0.15),
            (MacroY, 0.6),
            (MacroZ, 0.15),
            (Strike, 1.0),
            (Noise, 0.3),
            (ReverbMix, 0.3),
        ],
    },
    Preset {
        name: "Singing Bowl",
        values: &[
            (Engine, GONG),
            (Pitch, 0.5),
            (Decay, 0.95),
            (MacroX, 0.95),
            (MacroY, 0.15),
            (MacroZ, 0.55),
            (Strike, 0.2),
            (Noise, 0.0),
            (ChaosRate, 0.3),
            (ChaosDepth, 0.25),
            (ChaosDest, CHAOS_PITCH as f32),
            (ReverbMix, 0.45),
        ],
    },
    // --- GRAIN ---------------------------------------------------------
    Preset {
        name: "Rain on Tin",
        values: &[
            (Engine, GRAIN),
            (Pitch, 0.7),
            (Decay, 0.55),
            (MacroX, 0.55),
            (MacroY, 0.15),
            (MacroZ, 0.6),
            (Noise, 0.0),
            (ReverbMix, 0.45),
        ],
    },
    Preset {
        name: "Shaker Cloud",
        values: &[
            (Engine, GRAIN),
            (Pitch, 0.9),
            (Decay, 0.3),
            (MacroX, 0.8),
            (MacroY, 0.05),
            (MacroZ, 0.3),
            (Noise, 0.3),
            (FilterType, HP),
            (Cutoff, 0.5),
            (ReverbMix, 0.25),
        ],
    },
    Preset {
        name: "Bubble Swarm",
        values: &[
            (Engine, GRAIN),
            (Pitch, 0.6),
            (Decay, 0.7),
            (MacroX, 0.35),
            (MacroY, 0.55),
            (MacroZ, 0.9),
            (Noise, 0.0),
            (ReverbMix, 0.4),
        ],
    },
    // --- SPECTRA -------------------------------------------------------
    Preset {
        name: "Spectral Bell",
        values: &[
            (Engine, SPECTRA),
            (Pitch, 0.5),
            (Decay, 0.7),
            (MacroX, 0.3),
            (MacroY, 0.7),
            (MacroZ, 0.8),
            (Noise, 0.0),
            (ReverbMix, 0.45),
        ],
    },
    Preset {
        name: "Dice Roll",
        values: &[
            (Engine, SPECTRA),
            (Pitch, 0.45),
            (Decay, 0.45),
            (MacroX, 1.0),
            (MacroY, 0.5),
            (MacroZ, 0.5),
            (Jitter, 0.3),
            (Noise, 0.05),
            (ReverbMix, 0.3),
        ],
    },
    Preset {
        name: "Rubber Harmonics",
        values: &[
            (Engine, SPECTRA),
            (Pitch, 0.35),
            (Decay, 0.55),
            (MacroX, 0.6),
            (MacroY, 1.0),
            (MacroZ, 0.2),
            (SweepDepth, 0.6),
            (SweepSpeed, 0.5),
            (Noise, 0.0),
            (ReverbMix, 0.3),
        ],
    },
    // --- FLUX ----------------------------------------------------------
    Preset {
        name: "FM Clang",
        values: &[
            (Engine, FLUX),
            (Pitch, 0.45),
            (Decay, 0.5),
            (MacroX, 0.55),
            (MacroY, 0.7),
            (MacroZ, 0.2),
            (Noise, 0.0),
            (ReverbMix, 0.35),
        ],
    },
    Preset {
        name: "Chaos Kick",
        values: &[
            (Engine, FLUX),
            (Pitch, 0.15),
            (Decay, 0.45),
            (MacroX, 0.14),
            (MacroY, 0.55),
            (MacroZ, 0.3),
            (SweepDepth, 0.8),
            (SweepSpeed, 0.65),
            (Fold, 0.2),
            (Noise, 0.0),
            (ReverbMix, 0.1),
        ],
    },
    Preset {
        name: "Laser Pew",
        values: &[
            (Engine, FLUX),
            (Pitch, 0.55),
            (Decay, 0.45),
            (MacroX, 0.14),
            (MacroY, 0.15),
            (MacroZ, 0.0),
            (SweepDepth, 0.9),
            (SweepSpeed, 0.42),
            (Noise, 0.0),
            (ReverbMix, 0.45),
        ],
    },
    Preset {
        name: "Turbulence",
        values: &[
            (Engine, FLUX),
            (Pitch, 0.5),
            (Decay, 0.4),
            (MacroX, 0.8),
            (MacroY, 1.0),
            (MacroZ, 0.7),
            (FilterType, BP),
            (Cutoff, 0.7),
            (Reso, 0.4),
            (FilterEnv, 0.3),
            (Noise, 0.0),
            (ReverbMix, 0.35),
        ],
    },
    // --- LORENZ --------------------------------------------------------
    Preset {
        name: "Strange Tom",
        values: &[
            (Engine, LORENZ),
            (Pitch, 0.3),
            (Decay, 0.6),
            (Level, 0.95),
            (MacroX, 0.36),
            (MacroY, 0.4),
            (MacroZ, 0.0),
            (Noise, 0.1),
            (ReverbMix, 0.3),
        ],
    },
    Preset {
        name: "Butterfly Zap",
        values: &[
            (Engine, LORENZ),
            (Pitch, 0.55),
            (Decay, 0.35),
            (MacroX, 0.75),
            (MacroY, 0.4),
            (MacroZ, 0.5),
            (SweepDepth, 0.7),
            (SweepSpeed, 0.5),
            (Noise, 0.0),
            (ReverbMix, 0.4),
        ],
    },
    Preset {
        name: "Attractor Drone",
        values: &[
            (Engine, LORENZ),
            (Pitch, 0.35),
            (Decay, 0.95),
            (MacroX, 0.9),
            (MacroY, 0.7),
            (MacroZ, 1.0),
            (ChaosRate, 0.2),
            (ChaosDepth, 0.7),
            (ChaosDest, CHAOS_Z as f32),
            (Noise, 0.0),
            (ReverbMix, 0.6),
        ],
    },
    Preset {
        name: "Wobble Filter Hit",
        values: &[
            (Engine, LORENZ),
            (Pitch, 0.4),
            (Decay, 0.7),
            (MacroX, 0.55),
            (MacroY, 0.3),
            (MacroZ, 0.2),
            (Level, 0.95),
            (FilterType, BP),
            (Cutoff, 0.45),
            (Reso, 0.45),
            (ChaosRate, 0.75),
            (ChaosDepth, 0.8),
            (ChaosDest, CHAOS_CUTOFF as f32),
            (Noise, 0.0),
            (ReverbMix, 0.35),
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voice::Voice;

    #[test]
    fn preset_names_unique() {
        for (i, a) in PRESETS.iter().enumerate() {
            for b in &PRESETS[i + 1..] {
                assert_ne!(a.name, b.name);
            }
        }
    }

    #[test]
    fn presets_are_in_range() {
        for p in PRESETS {
            for &(param, v) in p.values {
                assert_eq!(param.sanitize(v), v, "{}: {:?}", p.name, param);
            }
        }
    }

    #[test]
    fn presets_sound_and_stay_sane() {
        let fs = 96_000.0;
        for p in PRESETS {
            let mut v = Voice::new(fs, 1);
            let patch = p.patch();
            for param in crate::params::ALL_PARAMS {
                v.set_param(param, patch[param.index()]);
            }
            v.snap_params();
            v.trigger(1.0);
            let out: Vec<f32> = (0..(fs * 0.5) as usize).map(|_| v.tick()).collect();
            let peak = out.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
            assert!(out.iter().all(|x| x.is_finite()), "{}", p.name);
            assert!(peak > 0.03 && peak < 3.0, "{}: peak {peak}", p.name);
        }
    }
}
