//! RND, MUTATE and RND ALL.
//!
//! [`random_patch`] and [`mutate_patch`] are the Xenokussion XK-1's randomizer,
//! moved from its panel into the DSP crate: here the sequencer plays hits on
//! the audio thread, so that is where MUTATE has to act.

use crate::params::{
    ALL_PARAMS, FILTER_BP, FILTER_HP, GRID_TRIPLET, GlobalParam, GlobalPatch, Param, ParamKind,
    VoicePatch, default_global_patch, map,
};
use crate::pattern::{DRUMS, STYLES};
use crate::util::Noise;

/// Controls RND and MUTATE never change: the channel's level, trigger
/// sensitivity, stereo placement, MUTATE itself and the engine lock.
pub fn keeps_value_on_randomize(p: Param) -> bool {
    matches!(
        p,
        Param::Level | Param::Sense | Param::Pan | Param::Mutate | Param::EngineLock
    )
}

/// Whether a patch has its engine locked against MUTATE.
pub fn engine_locked(patch: &VoicePatch) -> bool {
    patch[Param::EngineLock.index()] >= 0.5
}

/// A random patch, as produced by the RND button (and MUTATE at RND on every
/// hit). `current` supplies the values of the controls that RND leaves alone.
///
/// Every control draws exactly one number, in parameter order, and the draw
/// is shaped so the results are playable across all eight engines: moderate
/// decays, open-ish filters, modulation and dirt that are usually subtle and
/// occasionally extreme.
pub fn random_patch(current: &VoicePatch, rng: &mut Noise) -> VoicePatch {
    let mut patch = *current;
    let mut draws = [0.0f32; crate::params::PARAM_COUNT];
    for p in ALL_PARAMS {
        draws[p.index()] = rng.sample() * 0.5 + 0.5;
    }
    let bipolar = |x: f32, depth: f32| 0.5 + depth * (2.0 * x - 1.0).powi(3);
    for p in ALL_PARAMS {
        if keeps_value_on_randomize(p) {
            continue;
        }
        let x = draws[p.index()];
        patch[p.index()] = p.sanitize(match (p, p.info().kind) {
            (_, ParamKind::Choice(names)) => (x * names.len() as f32).floor(),
            (Param::Pitch, _) => 0.12 + 0.7 * x,
            (Param::Fine, _) => 0.3 + 0.4 * x,
            (Param::Decay, _) => 0.2 + 0.55 * x,
            (Param::Strike, _) => 0.2 + 0.8 * x,
            (Param::Noise, _) => 0.7 * x * x,
            (Param::SweepSpeed, _) => 0.3 + 0.6 * x,
            (Param::SweepDepth, _) => bipolar(x, 0.45),
            (Param::ChaosDepth, _) => 0.8 * x * x,
            (Param::Jitter, _) => 0.3 * x * x,
            (Param::Cutoff, _) => 0.55 + 0.45 * x,
            (Param::Reso, _) => 0.55 * x,
            (Param::FilterEnv, _) => bipolar(x, 0.35),
            (Param::Fold, _) => 0.5 * x * x * x,
            (Param::Crush, _) => 0.6 * x.powi(4),
            // Often dry, sometimes drenched.
            (Param::ReverbMix, _) => 0.75 * x * x,
            _ => x,
        });
    }
    // A sweep that starts far above the audio band never comes down in
    // time: keep its starting pitch below about 6 kHz.
    let hz = map::pitch_hz(patch[Param::Pitch.index()], patch[Param::Fine.index()]);
    let max_up = (6_000.0 / hz).log2().max(0.0);
    if map::sweep_octaves(patch[Param::SweepDepth.index()]) > max_up {
        // Inverse of `sweep_octaves` for an upward sweep.
        patch[Param::SweepDepth.index()] = 0.5 + 0.5 * (max_up / 6.0).powf(2.0 / 3.0);
    }
    // A hit that starts octaves above its pitch (SWEEP) needs a mallet hard
    // enough to excite modes up there, or almost nothing gets in.
    let sweep = map::sweep_octaves(patch[Param::SweepDepth.index()]);
    if sweep > 1.0 {
        let floor = (0.35 + 0.12 * sweep).min(0.8);
        patch[Param::Strike.index()] = patch[Param::Strike.index()].max(floor);
    }
    // A highpass with a high cutoff would leave little of most engines:
    // pull its cutoff down into the body of the sound instead. A bandpass
    // only works where the sound is, so put it between the pitch and three
    // octaves above, among the harmonics.
    let x = draws[Param::Cutoff.index()];
    match patch[Param::FilterType.index()] as u32 {
        FILTER_HP => patch[Param::Cutoff.index()] = 0.05 + 0.35 * x,
        FILTER_BP => {
            let oct = 3.0 * x;
            let cutoff = (hz * oct.exp2() / 20.0).log10() / 3.0;
            patch[Param::Cutoff.index()] = Param::Cutoff.sanitize(cutoff);
        }
        _ => {}
    }
    patch
}

/// Move every randomizable control `amount` (0..=1) of the way towards a fresh
/// random patch: `clamp(current + (random - current) * amount)`. At 1 this is
/// exactly [`random_patch`]; choice controls round to the nearest position.
/// A locked engine (see [`engine_locked`]) stays as it is.
pub fn mutate_patch(current: &VoicePatch, amount: f32, rng: &mut Noise) -> VoicePatch {
    let mut target = random_patch(current, rng);
    if engine_locked(current) {
        target[Param::Engine.index()] = current[Param::Engine.index()];
    }
    if amount >= 1.0 {
        return target;
    }
    let amount = amount.max(0.0);
    let mut patch = *current;
    for p in ALL_PARAMS {
        let (c, t) = (current[p.index()], target[p.index()]);
        patch[p.index()] = p.sanitize(c + (t - c) * amount);
    }
    patch
}

/// Everything RND ALL rolls.
#[derive(Clone, Debug)]
pub struct RandomKit {
    pub voices: [VoicePatch; DRUMS],
    pub globals: GlobalPatch,
    pub style: usize,
    pub busy: f32,
}

fn range(rng: &mut Noise, lo: f32, hi: f32) -> f32 {
    lo + (hi - lo) * rng.uniform()
}

/// Roll everything: two new voices (their level, sense, pan and MUTATE
/// kept), a rhythm (any but the bare PULSE), a tempo and swing that suit it,
/// a subtle human feel and a room. The master volume is kept.
pub fn random_kit(
    voices: &[VoicePatch; DRUMS],
    current: &GlobalPatch,
    rng: &mut Noise,
) -> RandomKit {
    let voices = [random_patch(&voices[0], rng), random_patch(&voices[1], rng)];
    let style = 1 + rng.below(STYLES.len() - 1);
    let s = &STYLES[style];
    let mut g = default_global_patch();
    g[GlobalParam::Master.index()] = current[GlobalParam::Master.index()];
    let mut set = |q: GlobalParam, v: f32| g[q.index()] = q.sanitize(v);
    set(GlobalParam::Tempo, range(rng, s.tempo.0, s.tempo.1));
    set(GlobalParam::Swing, range(rng, s.swing.0, s.swing.1));
    set(GlobalParam::Grid, s.grid as f32);
    let bar = if s.grid == GRID_TRIPLET { 12.0 } else { 16.0 };
    set(
        GlobalParam::Length,
        if rng.chance(0.6) { bar } else { 2.0 * bar },
    );
    set(GlobalParam::Timing, range(rng, 0.12, 0.35));
    set(GlobalParam::Velo, range(rng, 0.2, 0.45));
    set(GlobalParam::Spot, range(rng, 0.15, 0.4));
    set(GlobalParam::Improv, range(rng, 0.05, 0.3));
    set(GlobalParam::ReverbDecay, range(rng, 0.15, 0.5));
    set(GlobalParam::ReverbTone, range(rng, 0.4, 0.75));
    set(GlobalParam::ReverbPredelay, range(rng, 0.04, 0.2));
    for band in [GlobalParam::EqLow, GlobalParam::EqMid, GlobalParam::EqHigh] {
        set(band, range(rng, 0.42, 0.58));
    }
    RandomKit {
        voices,
        globals: g,
        style,
        busy: range(rng, 0.3, 0.6),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::default_patch;
    use crate::voice::Voice;

    fn is_sanitized(patch: &VoicePatch) -> bool {
        ALL_PARAMS
            .iter()
            .all(|p| p.sanitize(patch[p.index()]) == patch[p.index()])
    }

    #[test]
    fn mutate_off_changes_nothing() {
        let current = default_patch();
        assert_eq!(mutate_patch(&current, 0.0, &mut Noise::new(7)), current);
    }

    #[test]
    fn mutate_at_rnd_equals_the_rnd_button() {
        let current = default_patch();
        let a = mutate_patch(&current, 1.0, &mut Noise::new(7));
        let b = random_patch(&current, &mut Noise::new(7));
        assert_eq!(a, b);
        assert_ne!(a, current);
    }

    #[test]
    fn partial_mutation_moves_part_way_towards_the_random_patch() {
        let current = default_patch();
        for seed in 1..50 {
            let target = random_patch(&current, &mut Noise::new(seed));
            let half = mutate_patch(&current, 0.5, &mut Noise::new(seed));
            assert!(is_sanitized(&half));
            for p in ALL_PARAMS {
                let (c, t, h) = (current[p.index()], target[p.index()], half[p.index()]);
                match p.info().kind {
                    ParamKind::Choice(_) => assert_eq!(h, p.sanitize(c + (t - c) * 0.5), "{p:?}"),
                    _ => assert!((h - (c + (t - c) * 0.5)).abs() < 1e-6, "{p:?}"),
                }
            }
        }
    }

    #[test]
    fn mutation_keeps_level_sense_pan_and_mutate() {
        let mut current = default_patch();
        current[Param::Level.index()] = 0.33;
        current[Param::Sense.index()] = 0.66;
        current[Param::Pan.index()] = 0.1;
        current[Param::Mutate.index()] = 0.7;
        current[Param::EngineLock.index()] = 1.0;
        let mut rng = Noise::new(3);
        for amount in [0.25, 0.5, 1.0] {
            let m = mutate_patch(&current, amount, &mut rng);
            for p in [
                Param::Level,
                Param::Sense,
                Param::Pan,
                Param::Mutate,
                Param::EngineLock,
            ] {
                assert_eq!(m[p.index()], current[p.index()]);
            }
        }
    }

    #[test]
    fn a_locked_engine_survives_mutation_but_the_rest_still_mutates() {
        let mut current = default_patch();
        current[Param::Engine.index()] = crate::params::ENGINE_LORENZ as f32;
        current[Param::EngineLock.index()] = 1.0;
        let mut rng = Noise::new(5);
        let mut patch = current;
        for _ in 0..200 {
            let next = mutate_patch(&patch, 1.0, &mut rng);
            assert_eq!(next[Param::Engine.index()], current[Param::Engine.index()]);
            assert_ne!(next, patch, "everything else still changes");
            patch = next;
        }
        // Unlocked, RND changes the engine sooner or later.
        current[Param::EngineLock.index()] = 0.0;
        let engines: std::collections::HashSet<u32> = (0..50)
            .map(|_| mutate_patch(&current, 1.0, &mut rng)[Param::Engine.index()] as u32)
            .collect();
        assert!(engines.len() > 3, "{engines:?}");
    }

    #[test]
    fn repeated_mutation_stays_in_range() {
        let mut patch = default_patch();
        let mut rng = Noise::new(11);
        for i in 0..1000 {
            patch = mutate_patch(&patch, [0.1, 0.5, 0.9][i % 3], &mut rng);
            assert!(is_sanitized(&patch));
        }
    }

    /// RND (and MUTATE at RND) must always give something audible and sane,
    /// whichever engine it lands on.
    #[test]
    fn random_patches_are_playable() {
        let mut rng = Noise::new(2024);
        let mut engines = [0usize; crate::params::ENGINE_COUNT];
        let fs = 48_000.0;
        for i in 0..1_000 {
            let patch = random_patch(&default_patch(), &mut rng);
            engines[patch[Param::Engine.index()] as usize] += 1;
            let mut v = Voice::new(fs, i);
            v.load_patch(&patch);
            v.snap_params();
            v.trigger(0.9);
            let out: Vec<f32> = (0..(fs * 0.3) as usize).map(|_| v.tick()).collect();
            let peak = out.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
            assert!(out.iter().all(|x| x.is_finite()), "patch {i}");
            assert!(
                peak > 2e-3 && peak < 4.0,
                "patch {i}: peak {peak} {patch:?}"
            );
        }
        assert!(engines.iter().all(|&n| n > 60), "engine spread {engines:?}");
    }

    #[test]
    fn random_kits_pick_a_real_rhythm_and_keep_master() {
        let mut rng = Noise::new(9);
        let mut g = default_global_patch();
        g[GlobalParam::Master.index()] = 0.42;
        for _ in 0..20 {
            let kit = random_kit(&[default_patch(); DRUMS], &g, &mut rng);
            assert!(kit.style > 0 && kit.style < STYLES.len());
            assert_eq!(kit.globals[GlobalParam::Master.index()], 0.42);
            assert_eq!(
                kit.globals[GlobalParam::Grid.index()],
                STYLES[kit.style].grid as f32
            );
        }
    }
}
