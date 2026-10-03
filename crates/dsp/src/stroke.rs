//! What a hand does: the strokes of conga, bongo and djembe technique, the
//! ornaments a player wraps around them, and how a stroke articulates an
//! XK-1 voice.
//!
//! The voices are not drums, so a stroke can't be physics here. Each one
//! instead bends the voice's exciter and envelope the way the hand would
//! bend a drum: a bass is a soft, low palm; a slap a hard, noisy strike that
//! is choked by the grabbing fingers; heels, tips and muted tones are
//! choked by the hand left resting on the head.

/// A stroke, ordered from the centre of the head to the rim (and roughly from
/// dark to bright). The discriminant is the stable wire id.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Stroke {
    /// Flat palm in the centre, lifted at once: the deep "dum".
    Bass = 0,
    /// Heel of the hand dropped near the centre and left there (heel-toe).
    Heel,
    /// Fingertips while the heel still rests on the head (toe/tip).
    Tip,
    /// Closed tone: fingers strike the edge and stay pressed into the head.
    Mute,
    /// Open tone: fingers strike the edge and bounce off, so the head sings.
    Open,
    /// Slap: the palm ridge lands, the fingertips whip in and grab. A crack.
    Slap,
}

pub const STROKE_COUNT: usize = 6;

pub const ALL_STROKES: [Stroke; STROKE_COUNT] = [
    Stroke::Bass,
    Stroke::Heel,
    Stroke::Tip,
    Stroke::Mute,
    Stroke::Open,
    Stroke::Slap,
];

impl Stroke {
    pub fn id(self) -> u8 {
        self as u8
    }

    pub fn from_id(id: u8) -> Option<Self> {
        ALL_STROKES.get(id as usize).copied()
    }

    /// One-letter name, as in hand-drum notation.
    pub fn letter(self) -> char {
        match self {
            Stroke::Bass => 'B',
            Stroke::Heel => 'H',
            Stroke::Tip => 'T',
            Stroke::Mute => 'M',
            Stroke::Open => 'O',
            Stroke::Slap => 'S',
        }
    }

    pub fn from_letter(c: char) -> Option<Self> {
        ALL_STROKES
            .iter()
            .copied()
            .find(|s| s.letter() == c.to_ascii_uppercase())
    }

    pub fn name(self) -> &'static str {
        match self {
            Stroke::Bass => "BASS",
            Stroke::Heel => "HEEL",
            Stroke::Tip => "TIP",
            Stroke::Mute => "MUTE",
            Stroke::Open => "OPEN",
            Stroke::Slap => "SLAP",
        }
    }

    /// The strokes a supporting hand plays quietly between accents.
    pub fn is_ghost(self) -> bool {
        matches!(self, Stroke::Heel | Stroke::Tip)
    }

    /// The velocity a written stroke of this kind usually gets.
    pub fn natural_velocity(self) -> f32 {
        match self {
            Stroke::Bass => 0.82,
            Stroke::Heel => 0.42,
            Stroke::Tip => 0.45,
            Stroke::Mute => 0.7,
            Stroke::Open => 0.82,
            Stroke::Slap => 0.92,
        }
    }
}

/// One hit as a voice receives it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Strike {
    pub stroke: Stroke,
    /// Raw hit strength `0..=1`.
    pub velocity: f32,
    /// Offset from the stroke's usual spot: towards the rim (+) or the centre (−).
    pub position: f32,
}

impl Strike {
    pub fn new(stroke: Stroke, velocity: f32) -> Self {
        Self {
            stroke,
            velocity,
            position: 0.0,
        }
    }
}

/// How a stroke bends a voice for one hit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Articulation {
    /// Added to STRIKE (mallet hardness).
    pub hardness: f32,
    /// Added to NOISE (the burst).
    pub noise: f32,
    /// Multiplies DECAY until the next hit (a hand left on the head chokes it).
    pub decay: f32,
    /// Pitch offset in octaves (the centre of a head sounds lower).
    pub pitch: f32,
    /// Added to macro Y (MODAL's and STRING's strike/pluck position).
    pub macro_y: f32,
    /// Level relative to the velocity.
    pub gain: f32,
}

impl Articulation {
    pub const NEUTRAL: Self = Self {
        hardness: 0.0,
        noise: 0.0,
        decay: 1.0,
        pitch: 0.0,
        macro_y: 0.0,
        gain: 1.0,
    };

    /// The articulation of a strike: its stroke, moved by its position
    /// (towards the rim is harder and moves macro Y up).
    pub fn of(s: &Strike) -> Self {
        let mut a = match s.stroke {
            Stroke::Bass => Self {
                hardness: -0.35,
                pitch: -0.4,
                macro_y: -0.25,
                ..Self::NEUTRAL
            },
            Stroke::Heel => Self {
                hardness: -0.3,
                decay: 0.25,
                pitch: -0.2,
                macro_y: -0.15,
                gain: 0.7,
                ..Self::NEUTRAL
            },
            Stroke::Tip => Self {
                hardness: 0.1,
                decay: 0.3,
                gain: 0.7,
                ..Self::NEUTRAL
            },
            Stroke::Mute => Self {
                decay: 0.2,
                gain: 0.9,
                ..Self::NEUTRAL
            },
            Stroke::Open => Self::NEUTRAL,
            Stroke::Slap => Self {
                hardness: 0.4,
                noise: 0.25,
                decay: 0.45,
                macro_y: 0.15,
                gain: 1.05,
                ..Self::NEUTRAL
            },
        };
        let pos = s.position.clamp(-0.5, 0.5);
        a.hardness += 0.25 * pos;
        a.macro_y += 0.4 * pos;
        a
    }
}

/// Decoration around a written note. The discriminant is the stable wire id.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Ornament {
    #[default]
    None = 0,
    /// One soft grace note from the other hand just before the note.
    Flam,
    /// Two grace notes (one hand bouncing) just before the note.
    Drag,
    /// The step is split into two even strokes.
    Double,
    /// Three strokes in the step (a triplet roll).
    Triple,
    /// Four strokes in the step: a trill.
    Trill,
    /// Four strokes crossing between the two drums: a flourish.
    Run,
}

pub const ORNAMENT_COUNT: usize = 7;

pub const ALL_ORNAMENTS: [Ornament; ORNAMENT_COUNT] = [
    Ornament::None,
    Ornament::Flam,
    Ornament::Drag,
    Ornament::Double,
    Ornament::Triple,
    Ornament::Trill,
    Ornament::Run,
];

impl Ornament {
    pub fn id(self) -> u8 {
        self as u8
    }

    pub fn from_id(id: u8) -> Option<Self> {
        ALL_ORNAMENTS.get(id as usize).copied()
    }

    pub fn name(self) -> &'static str {
        match self {
            Ornament::None => "NONE",
            Ornament::Flam => "FLAM",
            Ornament::Drag => "DRAG",
            Ornament::Double => "2X",
            Ornament::Triple => "3X",
            Ornament::Trill => "TRILL",
            Ornament::Run => "RUN",
        }
    }

    /// Short badge for the step grid.
    pub fn badge(self) -> &'static str {
        match self {
            Ornament::None => "",
            Ornament::Flam => "f",
            Ornament::Drag => "ff",
            Ornament::Double => "2",
            Ornament::Triple => "3",
            Ornament::Trill => "4",
            Ornament::Run => "~",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_is_neutral_and_mutes_choke() {
        assert_eq!(
            Articulation::of(&Strike::new(Stroke::Open, 1.0)),
            Articulation::NEUTRAL
        );
        for s in [Stroke::Heel, Stroke::Tip, Stroke::Mute, Stroke::Slap] {
            assert!(Articulation::of(&Strike::new(s, 1.0)).decay < 1.0, "{s:?}");
        }
        let rim = Strike {
            position: 0.5,
            ..Strike::new(Stroke::Open, 1.0)
        };
        assert!(Articulation::of(&rim).hardness > 0.0);
    }

    #[test]
    fn ids_and_letters_roundtrip() {
        for (i, s) in ALL_STROKES.iter().enumerate() {
            assert_eq!(s.id() as usize, i);
            assert_eq!(Stroke::from_id(s.id()), Some(*s));
            assert_eq!(Stroke::from_letter(s.letter()), Some(*s));
            assert_eq!(
                Stroke::from_letter(s.letter().to_ascii_lowercase()),
                Some(*s)
            );
        }
        for (i, o) in ALL_ORNAMENTS.iter().enumerate() {
            assert_eq!(o.id() as usize, i);
            assert_eq!(Ornament::from_id(o.id()), Some(*o));
        }
        assert_eq!(Stroke::from_id(STROKE_COUNT as u8), None);
    }
}
