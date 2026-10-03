//! The written part: a step pattern for two drums, the rhythm styles the
//! generator knows, and the generator itself.
//!
//! A pattern is what the player reads. How it is played (swing, human
//! timing, improvised embellishments) is up to the
//! [`Performer`](crate::performer::Performer).
//!
//! Patterns can be written as text, one lane per drum, one character per
//! step: `B H T M O S` are the strokes (lower case for a quiet ghost
//! stroke) and `.` is a rest. For example, the conga tumbao, two cycles of
//! heel-tip-slap-tip-heel-tip-open-open with the second pair of open tones
//! moving to the tumba:
//!
//! ```text
//! HI  htSthtOOhtStht..
//! LO  ..............OO
//! ```

use crate::params::{GRID_STRAIGHT, GRID_TRIPLET, MAX_STEPS};
use crate::stroke::{Ornament, Stroke};
use crate::util::Noise;

/// Drums per kit: 0 = HI, 1 = LO.
pub const DRUMS: usize = 2;
pub const HI: usize = 0;
pub const LO: usize = 1;

/// One written hit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Note {
    pub stroke: Stroke,
    /// `0..=1`.
    pub velocity: f32,
    /// Offset from the stroke's usual spot: towards the rim (+) or the
    /// centre (−), `-0.5..=0.5`.
    pub position: f32,
    pub ornament: Ornament,
    /// Probability that the note is played on a given pass, `0..=1`.
    pub chance: f32,
}

impl Note {
    pub fn new(stroke: Stroke) -> Self {
        Self {
            stroke,
            velocity: stroke.natural_velocity(),
            position: 0.0,
            ornament: Ornament::None,
            chance: 1.0,
        }
    }

    pub fn with_velocity(mut self, v: f32) -> Self {
        self.velocity = v;
        self.sanitized()
    }

    pub fn sanitized(mut self) -> Self {
        let fix =
            |v: f32, lo: f32, hi: f32, d: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        self.velocity = fix(self.velocity, 0.02, 1.0, 0.8);
        self.position = fix(self.position, -0.5, 0.5, 0.0);
        self.chance = fix(self.chance, 0.0, 1.0, 1.0);
        self
    }

    /// Flat wire encoding: `[stroke, velocity, position, ornament, chance]`.
    pub fn to_wire(self) -> [f32; 5] {
        [
            self.stroke.id() as f32,
            self.velocity,
            self.position,
            self.ornament.id() as f32,
            self.chance,
        ]
    }

    /// Inverse of [`Note::to_wire`]; a negative stroke means "no note".
    pub fn from_wire(w: [f32; 5]) -> Option<Self> {
        if w[0].is_nan() || w[0] < 0.0 {
            return None;
        }
        let stroke = Stroke::from_id(w[0] as u8)?;
        Some(
            Note {
                stroke,
                velocity: w[1],
                position: w[2],
                ornament: Ornament::from_id(w[3].max(0.0) as u8).unwrap_or_default(),
                chance: w[4],
            }
            .sanitized(),
        )
    }
}

pub type Step = [Option<Note>; DRUMS];

#[derive(Clone, Debug, PartialEq)]
pub struct Pattern {
    pub steps: [Step; MAX_STEPS],
}

impl Default for Pattern {
    fn default() -> Self {
        Self {
            steps: [[None; DRUMS]; MAX_STEPS],
        }
    }
}

impl Pattern {
    pub fn get(&self, step: usize, drum: usize) -> Option<Note> {
        self.steps
            .get(step)
            .and_then(|s| s.get(drum).copied().flatten())
    }

    pub fn set(&mut self, step: usize, drum: usize, note: Option<Note>) {
        if let Some(slot) = self.steps.get_mut(step).and_then(|s| s.get_mut(drum)) {
            *slot = note.map(Note::sanitized);
        }
    }

    /// Parse one lane per drum (see the module docs); shorter lanes leave
    /// the remaining steps empty.
    pub fn from_lanes(lanes: [&str; DRUMS]) -> Self {
        let mut p = Pattern::default();
        for (drum, lane) in lanes.iter().enumerate() {
            for (step, c) in lane.chars().filter(|c| !c.is_whitespace()).enumerate() {
                p.set(step, drum, parse_note(c));
            }
        }
        p
    }

    /// The lanes as text, `length` steps each.
    pub fn to_lanes(&self, length: usize) -> [String; DRUMS] {
        core::array::from_fn(|drum| {
            (0..length.min(MAX_STEPS))
                .map(|s| match self.get(s, drum) {
                    None => '.',
                    Some(n) if n.velocity < 0.8 * n.stroke.natural_velocity() => {
                        n.stroke.letter().to_ascii_lowercase()
                    }
                    Some(n) => n.stroke.letter(),
                })
                .collect()
        })
    }

    /// Number of notes within the first `length` steps.
    pub fn count(&self, length: usize) -> usize {
        self.steps[..length.min(MAX_STEPS)]
            .iter()
            .flatten()
            .filter(|n| n.is_some())
            .count()
    }

    /// Rotate the first `length` steps by `by` (positive = later).
    pub fn rotate(&mut self, length: usize, by: i32) {
        let n = length.clamp(1, MAX_STEPS);
        let k = by.rem_euclid(n as i32) as usize;
        self.steps[..n].rotate_right(k);
    }

    /// Copy the first `length` steps over the rest of the pattern, so a
    /// longer LENGTH repeats the groove rather than falling silent.
    pub fn fill_from(&mut self, length: usize) {
        let n = length.clamp(1, MAX_STEPS);
        for s in n..MAX_STEPS {
            self.steps[s] = self.steps[s % n];
        }
    }
}

fn parse_note(c: char) -> Option<Note> {
    let stroke = Stroke::from_letter(c)?;
    let n = Note::new(stroke);
    Some(if c.is_ascii_lowercase() {
        n.with_velocity(n.velocity * GHOST)
    } else {
        n
    })
}

/// Lower-case notes play at this fraction of their natural velocity.
const GHOST: f32 = 0.6;

// --- styles ------------------------------------------------------------------

/// A rhythm the generator can write.
#[derive(Clone, Copy, Debug)]
pub struct Style {
    pub name: &'static str,
    pub blurb: &'static str,
    pub grid: u32,
    /// Comfortable tempo range.
    pub tempo: (f32, f32),
    /// Typical swing knob range.
    pub swing: (f32, f32),
    /// Template bars, `[HI, LO]` lanes; they alternate in longer patterns.
    pub bars: &'static [[&'static str; DRUMS]],
}

pub const STYLE_FREE: usize = 8;

pub const STYLES: [Style; 9] = [
    Style {
        name: "PULSE",
        blurb: "The same open tone on every step: a blank canvas",
        grid: GRID_STRAIGHT,
        tempo: (90.0, 120.0),
        swing: (0.0, 0.0),
        bars: &[["OOOOOOOOOOOOOOOO", "................"]],
    },
    Style {
        name: "TUMBAO",
        blurb: "Son montuno marcha: heel-tip-slap-tip-heel-tip and two open tones, the second pair on the tumba",
        grid: GRID_STRAIGHT,
        tempo: (92.0, 112.0),
        swing: (0.0, 0.15),
        bars: &[
            ["htSthtOOhtStht..", "..............OO"],
            ["htSthtOOhtSthtO.", "...............O"],
        ],
    },
    Style {
        name: "BOLERO",
        blurb: "The marcha at half speed: room for heel-toe ghosting",
        grid: GRID_STRAIGHT,
        tempo: (66.0, 84.0),
        swing: (0.0, 0.1),
        bars: &[
            ["h.t.S.t.h.t.O.O.", "................"],
            ["h.t.S.t.h.t.....", "............O.O."],
        ],
    },
    Style {
        name: "MARTILLO",
        blurb: "The bongo 'hammer': index finger on the macho, thumb and fingers between, open tone on the hembra",
        grid: GRID_STRAIGHT,
        tempo: (92.0, 118.0),
        swing: (0.0, 0.12),
        bars: &[["OhStOh.tOhStOh.t", "......O.......O."]],
    },
    Style {
        name: "RUMBA",
        blurb: "Conga and tumba in conversation over a rumba clave",
        grid: GRID_STRAIGHT,
        tempo: (95.0, 115.0),
        swing: (0.05, 0.2),
        bars: &[
            ["h.tSh.tOh.tSh.t.", "..........O..O.O"],
            ["h.tSh.tOh.tS..t.", ".......O..O..O.."],
        ],
    },
    Style {
        name: "DJEMBE",
        blurb: "Bass, two tones, two slaps: a West African accompaniment with a low drum answering",
        grid: GRID_STRAIGHT,
        tempo: (100.0, 130.0),
        swing: (0.05, 0.25),
        bars: &[
            ["B.OOS.S.B.OOS.S.", "B.....B...B....."],
            ["B.OOS.S.B.OOS.SS", "B.....B...B.B..."],
        ],
    },
    Style {
        name: "6/8",
        blurb: "Triplet feel over the standard bell pattern, with a bass anchoring each bar",
        grid: GRID_TRIPLET,
        tempo: (88.0, 120.0),
        swing: (0.0, 0.0),
        bars: &[
            ["O.S.OS.S.O.S", "B.....B....."],
            ["O.S.OS.S.OSS", "B.....B..B.."],
        ],
    },
    Style {
        name: "CLAVE",
        blurb: "Son clave 3-2 on the high block over a pulse on the low one",
        grid: GRID_STRAIGHT,
        tempo: (90.0, 115.0),
        swing: (0.0, 0.1),
        bars: &[["O..O..O...O.O...", "M.m.M.m.M.m.M.m."]],
    },
    Style {
        name: "FREE",
        blurb: "Generated from scratch: metric accents, ghost notes and syncopation by chance",
        grid: GRID_STRAIGHT,
        tempo: (85.0, 125.0),
        swing: (0.0, 0.3),
        bars: &[],
    },
];

pub const STYLE_COUNT: usize = STYLES.len();

pub fn steps_per_beat(grid: u32) -> usize {
    if grid == GRID_TRIPLET { 3 } else { 4 }
}

// --- generator -----------------------------------------------------------------

/// Write a pattern of `length` steps in `style`. `busy` (`0..=1`) goes from
/// a bare skeleton, through the written groove with natural dynamics, to a
/// dense part full of ghost notes, ornaments, chance and a fill at the end.
pub fn generate(style: usize, busy: f32, length: usize, grid: u32, rng: &mut Noise) -> Pattern {
    let style = &STYLES[style.min(STYLE_COUNT - 1)];
    let length = length.clamp(1, MAX_STEPS);
    let spb = steps_per_beat(grid);
    let busy = busy.clamp(0.0, 1.0);
    let mut p = if style.bars.is_empty() {
        free_skeleton(length, spb, busy, rng)
    } else {
        tile(style, length)
    };

    for step in 0..length {
        let on_beat = step % spb == 0;
        for drum in 0..DRUMS {
            let Some(mut n) = p.get(step, drum) else {
                continue;
            };
            // A sparse pass keeps only the backbone.
            if n.stroke.is_ghost() && busy < 0.3 && rng.chance((0.3 - busy) / 0.3 * 0.9) {
                p.set(step, drum, None);
                continue;
            }
            // Natural dynamics grow with BUSY; at zero every hit is as written.
            if on_beat {
                n.velocity += 0.1 * busy.min(0.5);
            }
            n.velocity += rng.gauss() * 0.04 * busy;
            n.position = rng.gauss() * 0.04 * busy;
            p.set(step, drum, Some(n));
        }
    }

    if busy > 0.35 {
        let p_ghost = (busy - 0.35) * 0.8;
        for step in 0..length {
            if p.steps[step].iter().all(Option::is_none) && rng.chance(p_ghost) {
                let stroke = if step % 2 == 0 {
                    Stroke::Heel
                } else {
                    Stroke::Tip
                };
                let mut n = Note::new(stroke).with_velocity(0.28 + 0.12 * rng.uniform());
                if busy > 0.7 {
                    n.chance = 0.5 + 0.25 * rng.below(2) as f32;
                }
                p.set(step, HI, Some(n));
            }
        }
    }

    if busy > 0.5 {
        let p_orn = (busy - 0.5) * 0.35;
        for step in 0..length {
            for drum in 0..DRUMS {
                let Some(mut n) = p.get(step, drum) else {
                    continue;
                };
                if n.stroke.is_ghost() || n.ornament != Ornament::None || !rng.chance(p_orn) {
                    continue;
                }
                n.ornament = match rng.below(10) {
                    0..=4 => Ornament::Flam,
                    5..=7 => Ornament::Double,
                    _ => Ornament::Drag,
                };
                p.set(step, drum, Some(n));
            }
        }
    }

    if busy > 0.6 {
        let p_move = (busy - 0.6) * 0.3;
        for step in 0..length {
            if let (Some(n), None) = (p.get(step, HI), p.get(step, LO))
                && n.stroke == Stroke::Open
                && rng.chance(p_move)
            {
                p.set(step, HI, None);
                p.set(step, LO, Some(n));
            }
        }
    }

    if busy > 0.55 && length >= 8 && rng.chance(busy - 0.3) {
        write_fill(&mut p, length, spb, rng);
    }
    p
}

/// Repeat the style's bars over the length.
fn tile(style: &Style, length: usize) -> Pattern {
    let bar_len = style.bars[0][0].len().max(1);
    let mut p = Pattern::default();
    for step in 0..length {
        let bar = &style.bars[(step / bar_len) % style.bars.len()];
        for drum in 0..DRUMS {
            let c = bar[drum]
                .as_bytes()
                .get(step % bar_len)
                .copied()
                .unwrap_or(b'.') as char;
            p.set(step, drum, parse_note(c));
        }
    }
    p
}

/// FREE: a groove from metric weights. Strong beats get bass or open tones,
/// weak positions ghost strokes or syncopated slaps.
fn free_skeleton(length: usize, spb: usize, busy: f32, rng: &mut Noise) -> Pattern {
    let mut p = Pattern::default();
    // The groove repeats every bar (or half bar) with small changes.
    let cell = if rng.chance(0.5) { spb * 4 } else { spb * 2 };
    let mut cell_notes: Vec<Step> = Vec::with_capacity(cell);
    for i in 0..cell {
        let weight = if i == 0 {
            1.0
        } else if i % spb == 0 {
            0.7
        } else if spb == 4 && i % 2 == 0 {
            0.45
        } else {
            0.25
        };
        let mut step: Step = [None; DRUMS];
        if rng.chance(0.3 + 0.6 * weight * (0.6 + 0.6 * busy)) {
            let stroke = match (weight >= 0.7, rng.below(4)) {
                (true, 0) => Stroke::Bass,
                (true, 1 | 2) => Stroke::Open,
                (true, _) => Stroke::Slap,
                (false, 0) => Stroke::Slap,
                (false, 1) => Stroke::Open,
                (false, _) if i % 2 == 0 => Stroke::Heel,
                _ => Stroke::Tip,
            };
            step[HI] = Some(Note::new(stroke));
        }
        if rng.chance(0.12 + 0.3 * weight * busy) {
            let stroke = if rng.chance(0.7) {
                Stroke::Open
            } else {
                Stroke::Bass
            };
            step[LO] = Some(Note::new(stroke));
            if step[HI].is_some_and(|n| n.stroke.is_ghost()) {
                step[HI] = None;
            }
        }
        cell_notes.push(step);
    }
    for s in 0..length {
        p.steps[s] = cell_notes[s % cell];
    }
    p
}

/// End the pattern with a fill over its last beat or two.
fn write_fill(p: &mut Pattern, length: usize, spb: usize, rng: &mut Noise) {
    let span = if rng.chance(0.5) {
        spb
    } else {
        (2 * spb).min(length / 2)
    };
    let start = length - span;
    match rng.below(3) {
        // Open tones walking down from the high drum to the low one, getting louder.
        0 => {
            for (i, s) in (start..length).enumerate() {
                let drum = if i < span / 2 { HI } else { LO };
                let v = 0.6 + 0.4 * i as f32 / span as f32;
                p.steps[s] = [None; DRUMS];
                p.set(s, drum, Some(Note::new(Stroke::Open).with_velocity(v)));
            }
        }
        // A trill rising into the downbeat.
        1 => {
            for s in start..length {
                p.steps[s] = [None; DRUMS];
            }
            let mut n = Note::new(Stroke::Open).with_velocity(0.7);
            n.ornament = Ornament::Trill;
            p.set(start, HI, Some(n));
            for s in (start + 1)..length {
                let mut n = Note::new(Stroke::Open).with_velocity(0.7 + 0.1 * (s - start) as f32);
                n.ornament = if s + 1 == length {
                    Ornament::Run
                } else {
                    Ornament::Double
                };
                p.set(s, HI, Some(n));
            }
        }
        // Slaps and open tones crossing the drums.
        _ => {
            for (i, s) in (start..length).enumerate() {
                p.steps[s] = [None; DRUMS];
                let (drum, stroke) = if i % 2 == 0 {
                    (HI, Stroke::Slap)
                } else {
                    (LO, Stroke::Open)
                };
                p.set(s, drum, Some(Note::new(stroke)));
            }
            if let Some(mut n) = p.get(length - 1, LO).or(p.get(length - 1, HI)) {
                n.ornament = Ornament::Flam;
                let drum = if p.get(length - 1, LO).is_some() {
                    LO
                } else {
                    HI
                };
                p.set(length - 1, drum, Some(n));
            }
        }
    }
}

/// A few small edits: the pattern evolves rather than being replaced.
pub fn evolve(p: &mut Pattern, length: usize, rng: &mut Noise) {
    let length = length.clamp(1, MAX_STEPS);
    let edits = 1 + rng.below(3);
    for _ in 0..edits {
        let step = rng.below(length);
        let drum = if rng.chance(0.7) { HI } else { LO };
        match (p.get(step, drum), rng.below(5)) {
            (None, _) => {
                let stroke = if rng.chance(0.6) {
                    if step.is_multiple_of(2) {
                        Stroke::Heel
                    } else {
                        Stroke::Tip
                    }
                } else {
                    [Stroke::Open, Stroke::Slap, Stroke::Mute][rng.below(3)]
                };
                p.set(step, drum, Some(Note::new(stroke)));
            }
            (Some(mut n), 0) => {
                n.stroke = match n.stroke {
                    Stroke::Open => Stroke::Slap,
                    Stroke::Slap => Stroke::Open,
                    Stroke::Mute => Stroke::Open,
                    Stroke::Heel => Stroke::Tip,
                    Stroke::Tip => Stroke::Heel,
                    Stroke::Bass => Stroke::Open,
                };
                p.set(step, drum, Some(n));
            }
            (Some(mut n), 1) => {
                n.ornament = if n.ornament == Ornament::None {
                    [Ornament::Flam, Ornament::Double, Ornament::Drag][rng.below(3)]
                } else {
                    Ornament::None
                };
                p.set(step, drum, Some(n));
            }
            (Some(mut n), 2) => {
                n.velocity += rng.gauss() * 0.15;
                p.set(step, drum, Some(n));
            }
            (Some(n), 3) if n.stroke.is_ghost() => p.set(step, drum, None),
            (Some(n), _) => {
                let other = 1 - drum;
                if p.get(step, other).is_none() {
                    p.set(step, drum, None);
                    p.set(step, other, Some(n));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lanes_roundtrip() {
        let lanes = ["htSthtOOhtStht..", "..............OO"];
        let p = Pattern::from_lanes(lanes);
        assert_eq!(p.to_lanes(16), lanes.map(String::from));
        assert_eq!(p.count(16), 16);
        assert!(p.get(0, HI).unwrap().velocity < p.get(2, HI).unwrap().velocity);
    }

    #[test]
    fn notes_roundtrip_through_the_wire() {
        let mut n = Note::new(Stroke::Slap).with_velocity(0.66);
        n.position = -0.2;
        n.ornament = Ornament::Trill;
        n.chance = 0.5;
        assert_eq!(Note::from_wire(n.to_wire()), Some(n));
        assert_eq!(Note::from_wire([-1.0, 0.0, 0.0, 0.0, 0.0]), None);
        assert_eq!(Note::from_wire([f32::NAN, 0.0, 0.0, 0.0, 0.0]), None);
    }

    #[test]
    fn templates_fit_their_grid() {
        for s in STYLES {
            for bar in s.bars {
                let n = bar[0].len();
                assert_eq!(n, bar[1].len(), "{}", s.name);
                assert_eq!(n % (steps_per_beat(s.grid) * 4), 0, "{}", s.name);
                for lane in bar {
                    assert!(
                        lane.chars()
                            .all(|c| c == '.' || Stroke::from_letter(c).is_some())
                    );
                }
            }
        }
    }

    #[test]
    fn generator_fills_the_length_and_busy_adds_notes() {
        let mut rng = Noise::new(1);
        for (i, s) in STYLES.iter().enumerate() {
            for length in [7, 16, 32, 48] {
                let sparse = (0..20)
                    .map(|_| generate(i, 0.0, length, s.grid, &mut rng).count(length))
                    .sum::<usize>();
                let dense = (0..20)
                    .map(|_| generate(i, 1.0, length, s.grid, &mut rng).count(length))
                    .sum::<usize>();
                assert!(sparse > 0, "{} {length}", s.name);
                // PULSE already fills every step.
                if i == 0 {
                    assert!(dense >= sparse);
                } else {
                    assert!(dense > sparse, "{} {length}: {dense} vs {sparse}", s.name);
                }
                let p = generate(i, 1.0, length, s.grid, &mut rng);
                assert!(p.steps[length..].iter().flatten().all(Option::is_none));
            }
        }
    }

    #[test]
    fn pulse_at_zero_busy_is_identical_hits() {
        let p = generate(0, 0.0, 16, GRID_STRAIGHT, &mut Noise::new(3));
        let first = p.get(0, HI).unwrap();
        for s in 0..16 {
            assert_eq!(p.get(s, HI), Some(first));
        }
    }

    #[test]
    fn evolve_changes_little() {
        let mut rng = Noise::new(5);
        let base = generate(1, 0.5, 16, GRID_STRAIGHT, &mut rng);
        let mut changed_total = 0;
        for _ in 0..50 {
            let mut p = base.clone();
            evolve(&mut p, 16, &mut rng);
            let changed = (0..16)
                .flat_map(|s| (0..DRUMS).map(move |d| (s, d)))
                .filter(|&(s, d)| p.get(s, d) != base.get(s, d))
                .count();
            assert!(changed <= 6, "{changed}");
            changed_total += changed;
        }
        assert!(changed_total > 30);
    }

    #[test]
    fn rotate_and_fill() {
        let mut p = Pattern::from_lanes(["O...", "...."]);
        p.rotate(4, 1);
        assert_eq!(p.to_lanes(4)[0], ".O..");
        p.rotate(4, -2);
        assert_eq!(p.to_lanes(4)[0], "...O");
        p.fill_from(4);
        assert_eq!(p.to_lanes(8)[0], "...O...O");
    }
}
