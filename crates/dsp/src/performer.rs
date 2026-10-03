//! The player: reads the pattern and plays it with two hands.
//!
//! Everything here happens on the audio thread, ahead of time. Each step is
//! planned a little before it is due ([`LOOKAHEAD_S`]), so hits can land
//! early as well as late and grace notes can come before their main note. The
//! plan is a queue of timestamped strikes the engine fires sample-accurately.
//!
//! How a written note becomes sound:
//!
//! * **Swing** delays every second 16th towards a triplet feel.
//! * **Timing** combines a slow drift (the player rushes and drags a little
//!   over bars, an AR(1) process rather than white noise) with per-hit
//!   scatter. The weaker hand is a touch later than the strong one, and in
//!   two-handed unisons the hands never land quite together (Benadon's
//!   "near-unisons" in Afro-Cuban ensemble drumming).
//! * **Velo** scatters dynamics, lets the overall level breathe, leans on
//!   downbeats and makes the weak hand slightly softer.
//! * **Spot** scatters where on the head each hand lands.
//! * **Improv** embellishes on the fly: ghost notes in the gaps, open tones
//!   turned into slaps, notes moved to the other drum, flams and rolls. It
//!   saves its fireworks for the end of the phrase, and every fourth time
//!   round it plays a proper fill into the downbeat.
//! * **Ornaments**, written or improvised, become graces and rolls with their
//!   own slightly uneven spacing and dynamics.

use crate::params::{GRID_TRIPLET, GlobalParam, GlobalPatch, MAX_STEPS, map};
use crate::pattern::{DRUMS, HI, Note, Pattern, steps_per_beat};
use crate::stroke::{Ornament, Strike, Stroke};
use crate::util::Noise;

/// How far ahead steps are planned.
pub const LOOKAHEAD_S: f32 = 0.1;
/// The largest timing deviation the player is allowed, either way.
const MAX_OFFSET_S: f32 = 0.04;
const QUEUE_CAPACITY: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EventKind {
    Hit {
        drum: u8,
        strike: Strike,
        /// The first stroke of a note on this drum: MUTATE acts here, and not
        /// on the graces and roll strokes that follow it.
        mutate: bool,
    },
    /// The sequencer reached a step (for the playhead).
    Step(u16),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Event {
    /// Sample time.
    pub time: u64,
    pub kind: EventKind,
    /// Planned by the sequencer (dropped on stop) rather than played live.
    pub sequenced: bool,
}

/// Fixed-capacity time-ordered queue; never allocates after construction.
#[derive(Clone, Debug)]
pub struct EventQueue {
    buf: Vec<Event>,
}

impl Default for EventQueue {
    fn default() -> Self {
        Self {
            buf: Vec::with_capacity(QUEUE_CAPACITY),
        }
    }
}

impl EventQueue {
    pub fn push(&mut self, e: Event) {
        if self.buf.len() >= QUEUE_CAPACITY {
            return;
        }
        // Stable: equal times keep their insertion order.
        let at = self.buf.partition_point(|x| x.time <= e.time);
        self.buf.insert(at, e);
    }

    /// The next event due at or before `now`.
    pub fn pop_due(&mut self, now: u64) -> Option<Event> {
        if self.buf.first().is_some_and(|e| e.time <= now) {
            Some(self.buf.remove(0))
        } else {
            None
        }
    }

    pub fn next_time(&self) -> Option<u64> {
        self.buf.first().map(|e| e.time)
    }

    pub fn drop_sequenced(&mut self) {
        self.buf.retain(|e| !e.sequenced);
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hand {
    Strong,
    Weak,
}

/// The global controls the player reads.
#[derive(Clone, Copy, Debug)]
pub struct Feel {
    pub tempo: f32,
    pub swing: f32,
    pub length: usize,
    pub grid: u32,
    pub timing: f32,
    pub velo: f32,
    pub spot: f32,
    pub improv: f32,
}

impl Feel {
    pub fn from_globals(g: &GlobalPatch) -> Self {
        let v = |p: GlobalParam| g[p.index()];
        Self {
            tempo: v(GlobalParam::Tempo).max(1.0),
            swing: v(GlobalParam::Swing),
            length: (v(GlobalParam::Length) as usize).clamp(1, MAX_STEPS),
            grid: v(GlobalParam::Grid) as u32,
            timing: v(GlobalParam::Timing),
            velo: v(GlobalParam::Velo),
            spot: v(GlobalParam::Spot),
            improv: v(GlobalParam::Improv),
        }
    }

    pub fn steps_per_beat(&self) -> usize {
        steps_per_beat(self.grid)
    }

    /// Length of one step in samples.
    pub fn step_samples(&self, fs: f32) -> f64 {
        fs as f64 * 60.0 / self.tempo as f64 / self.steps_per_beat() as f64
    }

    /// How late step `k` lands because of swing, in samples.
    fn swing_offset(&self, k: usize, step: f64) -> f64 {
        if self.grid == GRID_TRIPLET || k.is_multiple_of(2) {
            0.0
        } else {
            (map::swing_ratio(self.swing) as f64 - 0.5) * 2.0 * step
        }
    }
}

#[derive(Clone, Debug)]
pub struct Performer {
    fs: f32,
    playing: bool,
    /// Next step to plan, and its unswung time.
    step: usize,
    next_time: f64,
    loop_count: u32,
    rng: Noise,
    /// Slow timing drift, ms, and dynamic drift, relative.
    drift_ms: f32,
    dyn_drift: f32,
    last_hand: Hand,
    last_strong_step: Option<usize>,
}

impl Performer {
    pub fn new(fs: f32, seed: u32) -> Self {
        Self {
            fs,
            playing: false,
            step: 0,
            next_time: 0.0,
            loop_count: 0,
            rng: Noise::new(seed),
            drift_ms: 0.0,
            dyn_drift: 0.0,
            last_hand: Hand::Weak,
            last_strong_step: None,
        }
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Times round the pattern since PLAY.
    pub fn loops(&self) -> u32 {
        self.loop_count
    }

    pub fn start(&mut self, now: u64) {
        if self.playing {
            return;
        }
        self.playing = true;
        self.step = 0;
        self.loop_count = 0;
        // Leave room for the first step's graces and early hits.
        self.next_time = now as f64 + (MAX_OFFSET_S + 0.02) as f64 * self.fs as f64;
        self.drift_ms = 0.0;
        self.dyn_drift = 0.0;
        self.last_strong_step = None;
    }

    pub fn stop(&mut self, queue: &mut EventQueue) {
        self.playing = false;
        queue.drop_sequenced();
    }

    /// Plan every step that comes due before `until` (+ lookahead).
    pub fn advance(
        &mut self,
        now: u64,
        until: u64,
        pattern: &Pattern,
        feel: &Feel,
        q: &mut EventQueue,
    ) {
        if !self.playing {
            return;
        }
        let horizon = until as f64 + (LOOKAHEAD_S * self.fs) as f64;
        let step_len = feel.step_samples(self.fs);
        while self.next_time <= horizon {
            if self.step >= feel.length {
                self.step = 0;
                self.loop_count += 1;
            }
            self.plan_step(now, pattern, feel, step_len, q);
            self.next_time += step_len;
            self.step += 1;
            if self.step >= feel.length {
                self.step = 0;
                self.loop_count += 1;
            }
        }
    }

    fn plan_step(
        &mut self,
        now: u64,
        pattern: &Pattern,
        feel: &Feel,
        step_len: f64,
        q: &mut EventQueue,
    ) {
        let k = self.step;
        let fs = self.fs;
        let t = self.next_time + feel.swing_offset(k, step_len);
        let t_next = self.next_time + step_len + feel.swing_offset(k + 1, step_len);
        let dur = (t_next - t).max(1.0);
        q.push(Event {
            time: t.max(now as f64) as u64,
            kind: EventKind::Step(k as u16),
            sequenced: true,
        });

        let mut notes = pattern.steps[k];
        self.improvise(k, feel, &mut notes);
        for n in notes.iter_mut() {
            if let Some(note) = n
                && note.chance < 1.0
                && !self.rng.chance(note.chance)
            {
                *n = None;
            }
        }

        // The player's slow drift, updated once per step.
        let sigma = map::timing_ms(feel.timing);
        self.drift_ms = 0.93 * self.drift_ms + self.rng.gauss() * 0.3 * sigma;
        self.dyn_drift =
            (0.96 * self.dyn_drift + self.rng.gauss() * 0.02 * feel.velo).clamp(-0.2, 0.2);

        let both = notes.iter().all(Option::is_some);
        let on_beat = k.is_multiple_of(feel.steps_per_beat());
        for drum in 0..DRUMS {
            let Some(n) = notes[drum] else { continue };
            let hand = if both {
                if drum == HI { Hand::Strong } else { Hand::Weak }
            } else if n.stroke.is_ghost()
                || (self.last_hand == Hand::Strong
                    && self.last_strong_step == Some((k + feel.length - 1) % feel.length))
            {
                Hand::Weak
            } else {
                Hand::Strong
            };
            if hand == Hand::Strong {
                self.last_strong_step = Some(k);
            }
            self.last_hand = hand;

            let mut offset_ms = self.drift_ms + self.rng.gauss() * sigma;
            let mut vel =
                n.velocity * (1.0 + self.rng.gauss() * 0.1 * feel.velo) * (1.0 + self.dyn_drift);
            if on_beat {
                vel += 0.04 * feel.velo;
            }
            if hand == Hand::Weak {
                offset_ms += 0.3 * sigma
                    + if both {
                        self.rng.gauss() * (0.5 + 0.4 * sigma)
                    } else {
                        0.0
                    };
                vel *= 1.0 - 0.06 * feel.velo;
            }
            let offset =
                offset_ms.clamp(-1e3 * MAX_OFFSET_S, 1e3 * MAX_OFFSET_S) as f64 * 1e-3 * fs as f64;
            let pos = n.position + self.rng.gauss() * 0.07 * feel.spot;
            self.emit(now, drum, n, t + offset, dur, vel, pos, feel, true, q);
        }
    }

    /// Embellish a step according to IMPROV.
    fn improvise(&mut self, k: usize, feel: &Feel, notes: &mut [Option<Note>; DRUMS]) {
        let i = feel.improv;
        if i <= 0.0 {
            return;
        }
        let phase = k as f32 / feel.length.max(1) as f32;
        let fill_zone = phase >= 0.75;
        let fill_loop = self.loop_count % 4 == 3;
        let f = if fill_zone {
            if fill_loop { 3.5 } else { 1.8 }
        } else {
            1.0
        };
        let rng = &mut self.rng;

        if notes.iter().all(Option::is_none) {
            if rng.chance(0.2 * i) {
                let stroke = if k.is_multiple_of(2) {
                    Stroke::Heel
                } else {
                    Stroke::Tip
                };
                notes[HI] = Some(Note::new(stroke).with_velocity(0.22 + 0.13 * rng.uniform()));
            } else if fill_zone && rng.chance(0.12 * i * f) {
                let stroke = if rng.chance(0.6) {
                    Stroke::Open
                } else {
                    Stroke::Slap
                };
                let drum = rng.below(DRUMS);
                notes[drum] = Some(Note::new(stroke).with_velocity(0.65 + 0.25 * rng.uniform()));
            }
            return;
        }

        for drum in 0..DRUMS {
            let Some(mut n) = notes[drum] else { continue };
            if n.stroke.is_ghost() && rng.chance(0.12 * i) {
                notes[drum] = None;
                continue;
            }
            n.stroke = match n.stroke {
                Stroke::Open if rng.chance(0.1 * i * f) => Stroke::Slap,
                Stroke::Slap if rng.chance(0.08 * i) => Stroke::Open,
                Stroke::Mute if rng.chance(0.06 * i) => Stroke::Open,
                s => s,
            };
            if n.ornament == Ornament::None && !n.stroke.is_ghost() && rng.chance(0.08 * i * f) {
                n.ornament = if fill_zone {
                    [
                        Ornament::Trill,
                        Ornament::Triple,
                        Ornament::Run,
                        Ornament::Flam,
                    ][rng.below(4)]
                } else {
                    [
                        Ornament::Flam,
                        Ornament::Flam,
                        Ornament::Flam,
                        Ornament::Double,
                        Ornament::Drag,
                    ][rng.below(5)]
                };
            }
            if rng.chance(0.1 * i) {
                n.velocity = (n.velocity + 0.15).min(1.0);
            }
            let other = 1 - drum;
            if n.stroke == Stroke::Open && notes[other].is_none() && rng.chance(0.06 * i * f) {
                notes[drum] = None;
                notes[other] = Some(n);
            } else {
                notes[drum] = Some(n);
            }
        }
    }

    /// Play a note with its ornament. `t` is when the main stroke lands;
    /// `dur` the time until the next step (rolls fill it).
    #[allow(clippy::too_many_arguments)]
    fn emit(
        &mut self,
        now: u64,
        drum: usize,
        n: Note,
        t: f64,
        dur: f64,
        vel: f32,
        pos: f32,
        feel: &Feel,
        sequenced: bool,
        q: &mut EventQueue,
    ) {
        let fs = self.fs as f64;
        let push = |q: &mut EventQueue,
                    time: f64,
                    drum: usize,
                    stroke: Stroke,
                    velocity: f32,
                    position: f32,
                    mutate: bool| {
            q.push(Event {
                time: time.max(now as f64) as u64,
                kind: EventKind::Hit {
                    drum: drum as u8,
                    strike: Strike {
                        stroke,
                        velocity: velocity.clamp(0.02, 1.0),
                        position,
                    },
                    mutate,
                },
                sequenced,
            });
        };
        // Roll strokes are never perfectly even.
        let wobble = 0.15 + 0.6 * feel.timing;
        let grace_stroke = match n.stroke {
            Stroke::Slap | Stroke::Mute => Stroke::Open,
            Stroke::Heel => Stroke::Tip,
            s => s,
        };
        match n.ornament {
            Ornament::None => push(q, t, drum, n.stroke, vel, pos, true),
            Ornament::Flam => {
                let gap = (0.016 + 0.012 * self.rng.uniform()) as f64 * fs;
                push(
                    q,
                    t - gap.min(0.45 * dur),
                    drum,
                    grace_stroke,
                    vel * 0.45,
                    pos - 0.05,
                    true,
                );
                push(q, t, drum, n.stroke, vel, pos, false);
            }
            Ornament::Drag => {
                let gap = ((0.022 + 0.008 * self.rng.uniform()) as f64 * fs).min(0.3 * dur);
                push(q, t - 2.0 * gap, drum, grace_stroke, vel * 0.38, pos, true);
                push(
                    q,
                    t - gap,
                    drum,
                    grace_stroke,
                    vel * 0.48,
                    pos + 0.03,
                    false,
                );
                push(q, t, drum, n.stroke, vel, pos, false);
            }
            Ornament::Double | Ornament::Triple | Ornament::Trill => {
                let count = match n.ornament {
                    Ornament::Double => 2,
                    Ornament::Triple => 3,
                    _ => 4,
                };
                let gap = dur / count as f64;
                for i in 0..count {
                    let jitter = if i == 0 {
                        0.0
                    } else {
                        self.rng.gauss() as f64 * wobble as f64 * 1e-3 * fs
                    };
                    let (stroke, v) = if i == 0 {
                        (n.stroke, vel)
                    } else {
                        let stroke = match n.stroke {
                            Stroke::Heel | Stroke::Tip if i % 2 == 1 => {
                                if n.stroke == Stroke::Heel {
                                    Stroke::Tip
                                } else {
                                    Stroke::Heel
                                }
                            }
                            Stroke::Slap => Stroke::Open,
                            s => s,
                        };
                        // Fillers are softer and swell towards the next beat.
                        (stroke, vel * (0.6 + 0.12 * i as f32 / count as f32))
                    };
                    let p = pos + if i % 2 == 1 { 0.04 } else { 0.0 };
                    push(q, t + i as f64 * gap + jitter, drum, stroke, v, p, i == 0);
                }
            }
            Ornament::Run => {
                let gap = dur / 4.0;
                let other = 1 - drum;
                for i in 0..4 {
                    let d = if i % 2 == 0 { drum } else { other };
                    let jitter = if i == 0 {
                        0.0
                    } else {
                        self.rng.gauss() as f64 * wobble as f64 * 1e-3 * fs
                    };
                    let stroke = if i == 0 { n.stroke } else { Stroke::Open };
                    let v = vel * (0.7 + 0.1 * i as f32);
                    // The first stroke on each drum of the run mutates.
                    push(q, t + i as f64 * gap + jitter, d, stroke, v, pos, i < 2);
                }
            }
        }
    }

    /// A hit played live (keyboard or pad), with its ornament. Grace notes
    /// need time before the main stroke, so an ornamented live hit lands a
    /// moment later.
    pub fn live(&mut self, now: u64, drum: usize, note: Note, feel: &Feel, q: &mut EventQueue) {
        let fs = self.fs as f64;
        let lead = match note.ornament {
            Ornament::Flam => 0.03 * fs,
            Ornament::Drag => 0.065 * fs,
            _ => 0.0,
        };
        // Rolls fill a 16th at the current tempo.
        let dur = fs * 60.0 / feel.tempo as f64 / 4.0;
        let t = now as f64 + lead;
        self.emit(
            now,
            drum.min(DRUMS - 1),
            note,
            t,
            dur,
            note.velocity,
            note.position,
            feel,
            false,
            q,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::default_global_patch;
    use crate::pattern::LO;

    const FS: f32 = 48_000.0;

    fn feel() -> Feel {
        let mut f = Feel::from_globals(&default_global_patch());
        f.timing = 0.0;
        f.velo = 0.0;
        f.spot = 0.0;
        f.improv = 0.0;
        f.swing = 0.0;
        f.tempo = 120.0;
        f.length = 16;
        f
    }

    /// Run the performer for `seconds` and collect the hits it fires.
    fn play(pattern: &Pattern, feel: &Feel, seconds: f32, seed: u32) -> Vec<(u64, u8, Strike)> {
        let mut p = Performer::new(FS, seed);
        let mut q = EventQueue::default();
        p.start(0);
        let mut out = Vec::new();
        let block = 128u64;
        let mut now = 0u64;
        while now < (seconds * FS) as u64 {
            p.advance(now, now + block, pattern, feel, &mut q);
            while let Some(e) = q.pop_due(now + block) {
                if let EventKind::Hit { drum, strike, .. } = e.kind {
                    out.push((e.time, drum, strike));
                }
            }
            now += block;
        }
        out
    }

    #[test]
    fn plays_the_pattern_on_the_grid() {
        let pat = Pattern::from_lanes(["O.O.O.O.O.O.O.O.", "................"]);
        let f = feel();
        let hits = play(&pat, &f, 2.0, 1);
        // 120 BPM, 16ths: a hit every 8th note = 0.25 s.
        assert_eq!(hits.len(), 8);
        let step = f.step_samples(FS);
        for w in hits.windows(2) {
            let gap = (w[1].0 - w[0].0) as f64;
            assert!((gap - 2.0 * step).abs() < 2.0, "{gap}");
        }
        assert!(hits.iter().all(|h| h.2.velocity == hits[0].2.velocity));
    }

    #[test]
    fn swing_delays_the_off_sixteenths() {
        let pat = Pattern::from_lanes(["OOOOOOOOOOOOOOOO", ""]);
        let mut f = feel();
        f.swing = 1.0;
        let hits = play(&pat, &f, 0.5, 1);
        let step = f.step_samples(FS);
        let first = (hits[1].0 - hits[0].0) as f64;
        let second = (hits[2].0 - hits[1].0) as f64;
        // 75% swing: long-short = 1.5 : 0.5 steps.
        assert!((first - 1.5 * step).abs() < 2.0, "{first}");
        assert!((second - 0.5 * step).abs() < 2.0, "{second}");
    }

    #[test]
    fn human_timing_scatters_but_stays_close() {
        let pat = Pattern::from_lanes(["O.O.O.O.O.O.O.O.", ""]);
        let mut f = feel();
        f.timing = 0.5;
        let hits = play(&pat, &f, 8.0, 3);
        let step = f.step_samples(FS) * 2.0;
        let start = hits[0].0 as f64;
        let devs: Vec<f64> = hits
            .iter()
            .enumerate()
            .map(|(i, h)| (h.0 as f64 - start - i as f64 * step) / FS as f64 * 1e3)
            .collect();
        let spread = devs.iter().fold(0.0f64, |a, &d| a.max(d.abs()));
        assert!(spread > 1.0, "{spread} ms: not humanized");
        assert!(spread < 85.0, "{spread} ms: too sloppy");
    }

    #[test]
    fn ornaments_add_strokes() {
        let mut pat = Pattern::from_lanes(["O...............", ""]);
        let count = |pat: &Pattern| play(pat, &feel(), 0.45, 1).len();
        assert_eq!(count(&pat), 1);
        for (orn, n) in [
            (Ornament::Flam, 2),
            (Ornament::Drag, 3),
            (Ornament::Double, 2),
            (Ornament::Triple, 3),
            (Ornament::Trill, 4),
            (Ornament::Run, 4),
        ] {
            let mut note = pat.get(0, HI).unwrap();
            note.ornament = orn;
            pat.set(0, HI, Some(note));
            assert_eq!(count(&pat), n, "{orn:?}");
        }
        // A run crosses to the other drum.
        let hits = play(&pat, &feel(), 0.45, 1);
        assert!(hits.iter().any(|h| h.1 == LO as u8));
    }

    /// MUTATE acts once per note per drum: on the first stroke of a flam,
    /// drag or roll, and on the first stroke on each drum of a run.
    #[test]
    fn only_the_first_stroke_of_an_ornament_mutates() {
        for (orn, expect) in [
            (Ornament::None, vec![true]),
            (Ornament::Flam, vec![true, false]),
            (Ornament::Drag, vec![true, false, false]),
            (Ornament::Trill, vec![true, false, false, false]),
            (Ornament::Run, vec![true, true, false, false]),
        ] {
            let mut pat = Pattern::from_lanes(["....O...........", ""]);
            let mut note = pat.get(4, HI).unwrap();
            note.ornament = orn;
            pat.set(4, HI, Some(note));
            let mut p = Performer::new(FS, 1);
            let mut q = EventQueue::default();
            p.start(0);
            p.advance(0, 48_000, &pat, &feel(), &mut q);
            let mut flags = Vec::new();
            while let Some(e) = q.pop_due(u64::MAX) {
                if let EventKind::Hit { mutate, .. } = e.kind {
                    flags.push(mutate);
                }
            }
            assert_eq!(flags, expect, "{orn:?}");
        }
    }

    #[test]
    fn graces_come_before_the_beat() {
        let mut pat = Pattern::from_lanes(["....O...........", ""]);
        let mut note = pat.get(4, HI).unwrap();
        note.ornament = Ornament::Flam;
        pat.set(4, HI, Some(note));
        let f = feel();
        let hits = play(&pat, &f, 1.0, 1);
        assert_eq!(hits.len(), 2);
        assert!(hits[0].2.velocity < hits[1].2.velocity);
        let gap_ms = (hits[1].0 - hits[0].0) as f32 / FS * 1e3;
        assert!((10.0..35.0).contains(&gap_ms), "{gap_ms}");
    }

    #[test]
    fn improv_varies_each_pass_and_zero_improv_repeats() {
        let pat = Pattern::from_lanes(["htSthtOOhtStht..", "..............OO"]);
        let f = feel();
        let steady = play(&pat, &f, 8.0, 7);
        let per_loop = pat.count(16);
        assert_eq!(steady.len() % per_loop, 0);
        let mut f2 = f;
        f2.improv = 1.0;
        let wild = play(&pat, &f2, 8.0, 7);
        assert_ne!(
            wild.iter().map(|h| h.2.stroke).collect::<Vec<_>>(),
            steady.iter().map(|h| h.2.stroke).collect::<Vec<_>>()
        );
    }

    #[test]
    fn chance_thins_notes_out() {
        let mut pat = Pattern::from_lanes(["OOOOOOOOOOOOOOOO", ""]);
        for s in 0..16 {
            let mut n = pat.get(s, HI).unwrap();
            n.chance = 0.5;
            pat.set(s, HI, Some(n));
        }
        let n = play(&pat, &feel(), 8.0, 11).len();
        // 8 s of 16ths at 120 BPM = 64 steps; about half should play.
        assert!((20..45).contains(&n), "{n}");
    }

    #[test]
    fn stop_drops_planned_hits() {
        let pat = Pattern::from_lanes(["OOOOOOOOOOOOOOOO", ""]);
        let mut p = Performer::new(FS, 1);
        let mut q = EventQueue::default();
        p.start(0);
        p.advance(0, 128, &pat, &feel(), &mut q);
        assert!(!q.is_empty());
        p.stop(&mut q);
        assert!(q.is_empty());
    }

    #[test]
    fn queue_orders_by_time() {
        let mut q = EventQueue::default();
        for t in [50u64, 10, 30, 10] {
            q.push(Event {
                time: t,
                kind: EventKind::Step(t as u16),
                sequenced: false,
            });
        }
        let mut got = Vec::new();
        while let Some(e) = q.pop_due(100) {
            got.push(e.time);
        }
        assert_eq!(got, vec![10, 10, 30, 50]);
    }
}
