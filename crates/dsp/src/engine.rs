//! The machine: two XK-1 voices played by the hand-drum player, a shared
//! room and the output stage.
//!
//! ```text
//!  pattern ─► PERFORMER ─► timed strikes ─► MUTATE? ─┬─► HI voice (2×) ─► decimate ─┬─ pan ──────────────────┐
//!  live hits ────────────────────────────────────────┘   LO voice (2×) ─► decimate ─┤                       (+)─► soft clip ─► out
//!                                                                                    └─ send ─► PLATE ─► EQ ──┘
//! ```
//!
//! Strikes fire on their exact sample. A strike flagged as the first stroke
//! of a note first mutates its voice by that channel's MUTATE amount (the
//! XK-1's `clamp(current + (random − current) × MUTATE)`), snapping the new
//! values in so they apply exactly on the hit. The engine reports each
//! mutated patch back so the panel can follow.

use crate::mutate::mutate_patch;
use crate::params::{
    ALL_PARAMS, GLOBAL_PARAM_COUNT, GlobalParam, GlobalPatch, PARAM_COUNT, Param, VoicePatch,
    default_global_patch, default_patch, map,
};
use crate::pattern::{DRUMS, Note, Pattern};
use crate::performer::{EventKind, EventQueue, Feel, Performer};
use crate::reverb::Plate;
use crate::stroke::{Strike, Stroke};
use crate::util::{Biquad, BiquadShape, DcBlocker, Noise, fast_tanh, one_pole_coeff};
use crate::voice::Voice;

pub const OVERSAMPLE: usize = 2;

/// Room EQ corner/centre frequencies.
pub const EQ_LOW_HZ: f32 = 220.0;
pub const EQ_MID_HZ: f32 = 1_200.0;
pub const EQ_HIGH_HZ: f32 = 4_500.0;

/// Windowed-sinc lowpass + 2:1 decimator (from the XK-1).
#[derive(Clone, Debug)]
struct Decimator {
    taps: [f32; Self::TAPS],
    /// Doubled history so the dot product is always over a contiguous slice.
    hist: [f32; 2 * Self::TAPS],
    pos: usize,
}

impl Decimator {
    const TAPS: usize = 47;

    fn new() -> Self {
        // Cutoff at 0.23 × the oversampled rate (≈ 22 kHz at 96 kHz).
        let fc = 0.23f32;
        let m = (Self::TAPS - 1) as f32 / 2.0;
        let mut taps = [0.0f32; Self::TAPS];
        for (n, t) in taps.iter_mut().enumerate() {
            let x = n as f32 - m;
            let sinc = if x == 0.0 {
                2.0 * fc
            } else {
                (2.0 * core::f32::consts::PI * fc * x).sin() / (core::f32::consts::PI * x)
            };
            let w = n as f32 / (Self::TAPS - 1) as f32 * core::f32::consts::TAU;
            let blackman = 0.42 - 0.5 * w.cos() + 0.08 * (2.0 * w).cos();
            *t = sinc * blackman;
        }
        let sum: f32 = taps.iter().sum();
        taps.iter_mut().for_each(|t| *t /= sum);
        Self {
            taps,
            hist: [0.0; 2 * Self::TAPS],
            pos: 0,
        }
    }

    #[inline]
    fn push(&mut self, x: f32) {
        self.pos = if self.pos == 0 {
            Self::TAPS - 1
        } else {
            self.pos - 1
        };
        self.hist[self.pos] = x;
        self.hist[self.pos + Self::TAPS] = x;
    }

    #[inline]
    fn process(&mut self, block: [f32; OVERSAMPLE]) -> f32 {
        for x in block {
            self.push(x);
        }
        let h = &self.hist[self.pos..self.pos + Self::TAPS];
        h.iter().zip(self.taps.iter()).map(|(a, b)| a * b).sum()
    }
}

/// What the panel shows: where the sequencer is, what was last hit, and
/// what MUTATE turned each channel into.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Status {
    /// Step that is sounding, or -1 when stopped.
    pub step: i32,
    pub playing: bool,
    /// Hits per drum since start (the panel flashes on changes).
    pub hits: [u32; DRUMS],
    /// The most recent strike on each drum.
    pub last: [Strike; DRUMS],
    /// Mutations per drum since start; `patches` is new whenever this changes.
    pub mutations: [u32; DRUMS],
    /// Each voice's patch after its latest mutation.
    pub patches: [VoicePatch; DRUMS],
}

impl Status {
    /// Flat encoding for the worklet: `[step, playing, hits×2, (stroke, vel,
    /// pos)×2, mutations×2, patch×2]`.
    pub const WIRE_LEN: usize = 12 + DRUMS * PARAM_COUNT;

    pub fn to_wire(&self) -> [f32; Self::WIRE_LEN] {
        let mut w = [0.0; Self::WIRE_LEN];
        w[0] = self.step as f32;
        w[1] = self.playing as u8 as f32;
        w[2] = self.hits[0] as f32;
        w[3] = self.hits[1] as f32;
        for d in 0..DRUMS {
            let s = self.last[d];
            w[4 + 3 * d] = s.stroke.id() as f32;
            w[5 + 3 * d] = s.velocity;
            w[6 + 3 * d] = s.position;
            w[10 + d] = self.mutations[d] as f32;
            w[12 + d * PARAM_COUNT..12 + (d + 1) * PARAM_COUNT].copy_from_slice(&self.patches[d]);
        }
        w
    }

    pub fn from_wire(w: &[f32]) -> Option<Self> {
        if w.len() < Self::WIRE_LEN {
            return None;
        }
        let strike = |i: usize| Strike {
            stroke: Stroke::from_id(w[i] as u8).unwrap_or(Stroke::Open),
            velocity: w[i + 1],
            position: w[i + 2],
        };
        let patch = |d: usize| {
            let mut p = default_patch();
            p.copy_from_slice(&w[12 + d * PARAM_COUNT..12 + (d + 1) * PARAM_COUNT]);
            p
        };
        Some(Self {
            step: w[0] as i32,
            playing: w[1] > 0.5,
            hits: [w[2] as u32, w[3] as u32],
            last: [strike(4), strike(7)],
            mutations: [w[10] as u32, w[11] as u32],
            patches: [patch(0), patch(1)],
        })
    }
}

impl Default for Status {
    fn default() -> Self {
        Self {
            step: -1,
            playing: false,
            hits: [0; DRUMS],
            last: [Strike::new(Stroke::Open, 0.0); DRUMS],
            mutations: [0; DRUMS],
            patches: [default_patch(); DRUMS],
        }
    }
}

#[derive(Clone, Debug)]
pub struct Engine {
    fs: f32,
    voices: [Voice; DRUMS],
    dec: [Decimator; DRUMS],
    globals: GlobalPatch,
    /// Smoothed room/EQ/master controls.
    smooth: GlobalPatch,
    k_smooth: f32,
    plate: Plate,
    eq: [[Biquad; 3]; 2],
    eq_db: [f32; 3],
    performer: Performer,
    queue: EventQueue,
    pattern: Pattern,
    rng: Noise,
    time: u64,
    status: Status,
    dc: [DcBlocker; 2],
    dc_r: f32,
}

impl Engine {
    pub fn new(sample_rate: f32) -> Self {
        let g = default_global_patch();
        let fs2 = sample_rate * OVERSAMPLE as f32;
        let mut e = Self {
            fs: sample_rate,
            voices: [Voice::new(fs2, 1), Voice::new(fs2, 2)],
            dec: [Decimator::new(), Decimator::new()],
            globals: g,
            smooth: g,
            k_smooth: one_pole_coeff(0.01, sample_rate),
            plate: Plate::new(sample_rate),
            eq: [[Biquad::default(); 3]; 2],
            eq_db: [f32::NAN; 3],
            performer: Performer::new(sample_rate, 0x5eed),
            queue: EventQueue::default(),
            pattern: Pattern::default(),
            rng: Noise::new(0x0b5e_55ed),
            time: 0,
            status: Status::default(),
            dc: [DcBlocker::default(), DcBlocker::default()],
            dc_r: 1.0 - core::f32::consts::TAU * 8.0 / sample_rate,
        };
        e.update_eq();
        e
    }

    pub fn sample_rate(&self) -> f32 {
        self.fs
    }

    pub fn set_param(&mut self, drum: usize, p: Param, value: f32) {
        if let Some(v) = self.voices.get_mut(drum) {
            v.set_param(p, value);
        }
    }

    pub fn param(&self, drum: usize, p: Param) -> f32 {
        self.voices[drum].param(p)
    }

    pub fn load_patch(&mut self, drum: usize, patch: &VoicePatch) {
        if let Some(v) = self.voices.get_mut(drum) {
            for p in ALL_PARAMS {
                v.set_param(p, patch[p.index()]);
            }
        }
    }

    pub fn set_global(&mut self, p: GlobalParam, value: f32) {
        self.globals[p.index()] = p.sanitize(value);
        // Only the room, EQ and master glide; the transport takes effect at once.
        if !matches!(
            p,
            GlobalParam::ReverbDecay
                | GlobalParam::ReverbTone
                | GlobalParam::ReverbPredelay
                | GlobalParam::EqLow
                | GlobalParam::EqMid
                | GlobalParam::EqHigh
                | GlobalParam::Master
        ) {
            self.smooth[p.index()] = self.globals[p.index()];
        }
    }

    pub fn global(&self, p: GlobalParam) -> f32 {
        self.globals[p.index()]
    }

    pub fn set_note(&mut self, step: usize, drum: usize, note: Option<Note>) {
        self.pattern.set(step, drum, note);
    }

    pub fn pattern(&self) -> &Pattern {
        &self.pattern
    }

    pub fn set_pattern(&mut self, p: &Pattern) {
        self.pattern = p.clone();
    }

    pub fn play(&mut self, on: bool) {
        if on {
            self.performer.start(self.time);
        } else {
            self.performer.stop(&mut self.queue);
            self.status.step = -1;
        }
        self.status.playing = on;
    }

    pub fn is_playing(&self) -> bool {
        self.performer.is_playing()
    }

    /// Play a note now (keyboard, pads), with its ornament.
    pub fn live(&mut self, drum: usize, note: Note) {
        let feel = Feel::from_globals(&self.globals);
        self.performer
            .live(self.time, drum, note.sanitized(), &feel, &mut self.queue);
    }

    /// Strike a voice immediately, bypassing the player. `mutate` lets MUTATE act.
    pub fn strike(&mut self, drum: usize, s: Strike, mutate: bool) {
        if drum < DRUMS {
            self.fire(drum, s, mutate);
        }
    }

    fn fire(&mut self, drum: usize, s: Strike, mutate: bool) {
        let voice = &mut self.voices[drum];
        let amount = voice.param(Param::Mutate);
        if mutate && amount > 0.0 {
            let patch = mutate_patch(&voice.patch(), amount, &mut self.rng);
            voice.load_patch(&patch);
            // The new sound starts exactly on this hit rather than gliding in.
            voice.snap_params();
            self.status.mutations[drum] = self.status.mutations[drum].wrapping_add(1);
            self.status.patches[drum] = patch;
        }
        voice.strike(s);
        self.status.hits[drum] = self.status.hits[drum].wrapping_add(1);
        self.status.last[drum] = s;
    }

    /// Jump smoothed parameters straight to their targets.
    pub fn snap_params(&mut self) {
        self.voices.iter_mut().for_each(Voice::snap_params);
        self.smooth = self.globals;
        self.update_eq();
    }

    pub fn status(&self) -> Status {
        self.status
    }

    pub fn lfo_led(&self, drum: usize) -> bool {
        self.voices[drum].lfo_led()
    }

    fn update_eq(&mut self) {
        let bands = [GlobalParam::EqLow, GlobalParam::EqMid, GlobalParam::EqHigh];
        let db = bands.map(|b| map::eq_db(self.smooth[b.index()]));
        if db == self.eq_db {
            return;
        }
        self.eq_db = db;
        for ch in self.eq.iter_mut() {
            ch[0].design(BiquadShape::LowShelf, EQ_LOW_HZ, 0.7, db[0], self.fs);
            ch[1].design(BiquadShape::Peak, EQ_MID_HZ, 0.8, db[1], self.fs);
            ch[2].design(BiquadShape::HighShelf, EQ_HIGH_HZ, 0.7, db[2], self.fs);
        }
    }

    /// Render stereo audio. `left` and `right` must be the same length.
    pub fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        debug_assert_eq!(left.len(), right.len());
        let n = left.len() as u64;
        let feel = Feel::from_globals(&self.globals);
        self.performer.advance(
            self.time,
            self.time + n,
            &self.pattern,
            &feel,
            &mut self.queue,
        );

        // Control-rate room/EQ smoothing, once per block.
        let k = 1.0 - (1.0 - self.k_smooth).powi(n as i32);
        for i in 0..GLOBAL_PARAM_COUNT {
            self.smooth[i] += (self.globals[i] - self.smooth[i]) * k;
        }
        self.update_eq();
        let g = |p: GlobalParam| self.smooth[p.index()];
        let controls = self.plate.controls(
            g(GlobalParam::ReverbDecay),
            g(GlobalParam::ReverbTone),
            g(GlobalParam::ReverbPredelay),
        );
        let master = map::level_gain(g(GlobalParam::Master)) * 1.4;

        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            while let Some(e) = self.queue.pop_due(self.time) {
                match e.kind {
                    EventKind::Hit {
                        drum,
                        strike,
                        mutate,
                    } => self.fire(drum as usize, strike, mutate),
                    EventKind::Step(s) => {
                        if self.performer.is_playing() {
                            self.status.step = s as i32;
                        }
                    }
                }
            }
            let (mut yl, mut yr, mut send) = (0.0, 0.0, 0.0);
            for (v, dec) in self.voices.iter_mut().zip(self.dec.iter_mut()) {
                let mut block = [0.0; OVERSAMPLE];
                for s in block.iter_mut() {
                    *s = v.tick();
                }
                let y = dec.process(block);
                // Equal-power pan, normalized so the centre is unity per side.
                let a = v.smoothed(Param::Pan) * core::f32::consts::FRAC_PI_2;
                yl += y * a.cos() * core::f32::consts::SQRT_2;
                yr += y * a.sin() * core::f32::consts::SQRT_2;
                send += y * map::reverb_gain(v.smoothed(Param::ReverbMix));
            }
            let (mut wl, mut wr) = self.plate.process(send, &controls);
            for b in self.eq[0].iter_mut() {
                wl = b.process(wl);
            }
            for b in self.eq[1].iter_mut() {
                wr = b.process(wr);
            }
            *l = soft_clip(self.dc[0].process(yl + wl, self.dc_r) * master);
            *r = soft_clip(self.dc[1].process(yr + wr, self.dc_r) * master);
            self.time += 1;
        }
    }

    /// Convenience for offline rendering.
    pub fn render_vec(&mut self, frames: usize) -> (Vec<f32>, Vec<f32>) {
        let mut l = vec![0.0; frames];
        let mut r = vec![0.0; frames];
        for (cl, cr) in l.chunks_mut(128).zip(r.chunks_mut(128)) {
            self.render(cl, cr);
        }
        (l, r)
    }
}

/// Output stage: transparent below ~-6 dBFS, gently saturating above.
#[inline]
fn soft_clip(x: f32) -> f32 {
    const KNEE: f32 = 0.5;
    let a = x.abs();
    if a <= KNEE {
        x
    } else {
        let over = (a - KNEE) / (1.0 - KNEE);
        (KNEE + (1.0 - KNEE) * fast_tanh(over)).copysign(x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{ENGINE_COUNT, ParamKind};
    use crate::pattern::{HI, LO, STYLES, generate};
    use crate::stroke::Ornament;

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    #[test]
    fn silent_until_played() {
        let mut e = Engine::new(48_000.0);
        let (l, r) = e.render_vec(4_800);
        assert!(rms(&l) < 1e-6 && rms(&r) < 1e-6);
    }

    #[test]
    fn every_engine_sounds_through_the_player() {
        for engine in 0..ENGINE_COUNT {
            let mut e = Engine::new(48_000.0);
            for d in 0..DRUMS {
                e.set_param(d, Param::Engine, engine as f32);
            }
            e.set_pattern(&Pattern::from_lanes(["O...S...", "..B...M."]));
            e.set_global(GlobalParam::Length, 8.0);
            e.play(true);
            let (l, _) = e.render_vec(48_000);
            assert!(rms(&l) > 0.005, "engine {engine}");
            assert!(e.status().hits.iter().all(|&h| h > 0));
        }
    }

    #[test]
    fn sequencer_plays_and_reports_its_step() {
        let mut e = Engine::new(48_000.0);
        e.set_pattern(&Pattern::from_lanes([
            "O...O...O...O...",
            "..O...O...O...O.",
        ]));
        e.set_global(GlobalParam::Tempo, 120.0);
        e.play(true);
        e.render_vec(48_000);
        let s = e.status();
        assert!(s.playing && s.step >= 0);
        assert_eq!(s.hits, [2, 2]);
        e.play(false);
        assert_eq!(e.status().step, -1);
    }

    #[test]
    fn mutate_off_leaves_the_patch_alone() {
        let mut e = Engine::new(48_000.0);
        let before = e.voices[HI].patch();
        e.set_pattern(&Pattern::from_lanes(["OOOOOOOOOOOOOOOO", ""]));
        e.play(true);
        e.render_vec(48_000);
        assert_eq!(e.voices[HI].patch(), before);
        assert_eq!(e.status().mutations, [0, 0]);
    }

    #[test]
    fn mutate_acts_on_every_sequenced_hit_but_not_inside_rolls() {
        let mut e = Engine::new(48_000.0);
        e.set_param(HI, Param::Mutate, 1.0);
        let mut p = Pattern::from_lanes(["O...O...O...O...", ""]);
        let mut trill = p.get(4, HI).unwrap();
        trill.ornament = Ornament::Trill;
        p.set(4, HI, Some(trill));
        e.set_pattern(&p);
        e.set_global(GlobalParam::Tempo, 120.0);
        e.play(true);
        // One full pass: 16 steps at 120 BPM = 2 s.
        e.render_vec(96_000 + 2_000);
        let s = e.status();
        // 4 notes, one of them a 4-stroke trill: 7 strokes, 4 mutations.
        assert_eq!(s.hits[HI], 7);
        assert_eq!(s.mutations[HI], 4);
        assert_eq!(s.mutations[LO], 0);
        assert_eq!(s.patches[HI], e.voices[HI].patch());
        assert_eq!(s.patches[HI][Param::Mutate.index()], 1.0, "MUTATE survives");
    }

    #[test]
    fn engine_lock_holds_through_sequenced_mutation() {
        let mut e = Engine::new(48_000.0);
        e.set_param(HI, Param::Engine, crate::params::ENGINE_GONG as f32);
        e.set_param(HI, Param::EngineLock, 1.0);
        e.set_param(HI, Param::Mutate, 1.0);
        e.set_pattern(&Pattern::from_lanes(["OOOOOOOOOOOOOOOO", ""]));
        e.play(true);
        e.render_vec(96_000);
        let s = e.status();
        assert!(s.mutations[HI] > 10);
        assert_eq!(
            e.param(HI, Param::Engine),
            crate::params::ENGINE_GONG as f32
        );
        assert_eq!(s.patches[HI][Param::EngineLock.index()], 1.0);
    }

    #[test]
    fn mutate_acts_on_live_hits_too() {
        let mut e = Engine::new(48_000.0);
        e.set_param(LO, Param::Mutate, 0.5);
        let before = e.voices[LO].patch();
        e.live(LO, Note::new(Stroke::Open));
        e.render_vec(480);
        assert_eq!(e.status().mutations[LO], 1);
        assert_ne!(e.voices[LO].patch(), before);
    }

    #[test]
    fn every_style_renders_bounded_with_full_mutation_and_humanity() {
        let mut rng = Noise::new(3);
        for (i, style) in STYLES.iter().enumerate() {
            let mut e = Engine::new(44_100.0);
            for d in 0..DRUMS {
                e.set_param(d, Param::Mutate, 1.0);
            }
            for p in [
                GlobalParam::Timing,
                GlobalParam::Velo,
                GlobalParam::Spot,
                GlobalParam::Improv,
            ] {
                e.set_global(p, 1.0);
            }
            e.set_global(GlobalParam::Master, 1.0);
            e.set_global(GlobalParam::Grid, style.grid as f32);
            e.set_pattern(&generate(i, 1.0, 32, style.grid, &mut rng));
            e.set_global(GlobalParam::Length, 32.0);
            e.play(true);
            let (l, r) = e.render_vec(44_100 * 4);
            assert!(
                l.iter().chain(&r).all(|s| s.is_finite() && s.abs() <= 1.0),
                "{}",
                style.name
            );
            assert!(rms(&l) > 0.001, "{} silent", style.name);
        }
    }

    #[test]
    fn stroke_articulation_shapes_the_hit() {
        let hit = |stroke: Stroke| {
            let mut e = Engine::new(48_000.0);
            e.set_param(HI, Param::ReverbMix, 0.0);
            e.set_param(HI, Param::Decay, 0.6);
            e.snap_params();
            e.strike(HI, Strike::new(stroke, 0.9), false);
            e.render_vec(24_000).0
        };
        let (open, mute) = (hit(Stroke::Open), hit(Stroke::Mute));
        assert!(
            rms(&mute[9_600..]) < 0.3 * rms(&open[9_600..]),
            "mute chokes"
        );
    }

    #[test]
    fn status_roundtrips_through_the_wire() {
        let mut s = Status {
            step: 5,
            playing: true,
            hits: [3, 9],
            mutations: [2, 7],
            ..Default::default()
        };
        s.last[0] = Strike {
            stroke: Stroke::Slap,
            velocity: 0.5,
            position: 0.1,
        };
        s.patches[1][Param::Pitch.index()] = 0.123;
        assert_eq!(Status::from_wire(&s.to_wire()), Some(s));
    }

    #[test]
    fn output_is_finite_and_bounded_under_random_patches() {
        let mut rng = Noise::new(99);
        let mut e = Engine::new(44_100.0);
        for round in 0..60 {
            for v in 0..DRUMS {
                for p in ALL_PARAMS {
                    let x = rng.uniform();
                    let val = match p.info().kind {
                        ParamKind::Choice(n) => (x * n.len() as f32).floor(),
                        _ => x,
                    };
                    e.set_param(v, p, val);
                }
                e.strike(v, Strike::new(Stroke::Slap, rng.uniform()), true);
            }
            e.set_global(GlobalParam::Master, 1.0);
            let (l, r) = e.render_vec(2_205);
            for s in l.iter().chain(r.iter()) {
                assert!(s.is_finite() && s.abs() <= 1.0, "round {round}: {s}");
            }
        }
    }
}
