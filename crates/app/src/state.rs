//! Panel state, the controller shared through the Dioxus context, and
//! persistence (localStorage on the web, `~/.config/xenopalm` on desktop).
//!
//! MUTATE runs inside the audio engine, because that is where the player's
//! hits happen. The engine reports each mutated patch back and the panel
//! adopts it ([`Synth::adopt`]), so the knobs move with every hit as they did
//! on the XK-1.

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;
use xenopalm_dsp::mutate::{engine_locked, keeps_value_on_randomize, random_kit, random_patch};
use xenopalm_dsp::params::{
    ALL_GLOBAL_PARAMS, ALL_PARAMS, GlobalPatch, MAX_STEPS, ParamKind, default_global_patch,
    default_patch,
};
use xenopalm_dsp::pattern::{self, STYLE_COUNT, STYLES};
use xenopalm_dsp::presets::{PRESETS, Preset};
use xenopalm_dsp::util::Noise;
use xenopalm_dsp::{
    DRUMS, GlobalParam, Note, Ornament, Param, Pattern, Status, Stroke, VoicePatch,
};

use crate::audio::{AudioHandle, Command, Snapshot};

#[derive(Clone, Debug, PartialEq)]
pub struct PanelState {
    pub voices: [VoicePatch; DRUMS],
    pub globals: GlobalPatch,
    pub pattern: Pattern,
    /// Generator settings (UI only).
    pub style: usize,
    pub busy: f32,
}

fn preset(name: &str) -> VoicePatch {
    PRESETS
        .iter()
        .find(|p| p.name == name)
        .map(Preset::patch)
        .unwrap_or_else(default_patch)
}

impl Default for PanelState {
    fn default() -> Self {
        let mut voices = [preset("Membrane Tom"), preset("Slack Floor Tom")];
        voices[0][Param::Pan.index()] = 0.62;
        voices[1][Param::Pan.index()] = 0.38;
        // A hint of what this machine is for.
        voices[0][Param::Mutate.index()] = 0.08;
        Self {
            voices,
            globals: default_global_patch(),
            pattern: pattern::generate(1, 0.45, 16, 0, &mut Noise::new(7)),
            style: 1,
            busy: 0.45,
        }
    }
}

impl PanelState {
    pub fn global(&self, p: GlobalParam) -> f32 {
        self.globals[p.index()]
    }

    pub fn length(&self) -> usize {
        (self.global(GlobalParam::Length) as usize).clamp(1, MAX_STEPS)
    }

    pub fn grid(&self) -> u32 {
        self.global(GlobalParam::Grid) as u32
    }

    pub fn engine(&self, drum: usize) -> u32 {
        Param::Engine.sanitize(self.voices[drum][Param::Engine.index()]) as u32
    }

    pub fn engine_locked(&self, drum: usize) -> bool {
        engine_locked(&self.voices[drum])
    }

    pub fn to_snapshot(&self) -> Snapshot {
        Snapshot {
            voices: self.voices,
            globals: self.globals,
            pattern: self.pattern.clone(),
        }
    }

    pub fn serialize(&self) -> String {
        let mut out = format!(
            "{FORMAT_HEADER}\nstyle {}\nbusy {}\n",
            self.style, self.busy
        );
        for p in ALL_GLOBAL_PARAMS {
            out += &format!("g {p:?} {}\n", self.globals[p.index()]);
        }
        for (d, patch) in self.voices.iter().enumerate() {
            for p in ALL_PARAMS {
                out += &format!("v {d} {p:?} {}\n", patch[p.index()]);
            }
        }
        for step in 0..MAX_STEPS {
            for d in 0..DRUMS {
                if let Some(n) = self.pattern.get(step, d) {
                    out += &format!(
                        "n {step} {d} {} {} {} {} {}\n",
                        n.stroke.letter(),
                        n.velocity,
                        n.position,
                        n.ornament.id(),
                        n.chance
                    );
                }
            }
        }
        out
    }

    /// Lenient: unknown lines are skipped and missing values keep defaults.
    pub fn deserialize(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        if lines.next()?.trim() != FORMAT_HEADER {
            return None;
        }
        let mut s = PanelState {
            pattern: Pattern::default(),
            ..Default::default()
        };
        let num = |v: &str| v.parse::<f32>().ok().filter(|v| v.is_finite());
        for line in lines {
            let parts: Vec<&str> = line.split_whitespace().collect();
            match parts.as_slice() {
                ["style", v] => {
                    if let Ok(v) = v.parse::<usize>() {
                        s.style = v.min(STYLE_COUNT - 1);
                    }
                }
                ["busy", v] => {
                    if let Some(v) = num(v) {
                        s.busy = v.clamp(0.0, 1.0);
                    }
                }
                ["g", name, v] => {
                    let p = ALL_GLOBAL_PARAMS.iter().find(|p| format!("{p:?}") == *name);
                    if let (Some(p), Some(v)) = (p, num(v)) {
                        s.globals[p.index()] = p.sanitize(v);
                    }
                }
                ["v", d, name, v] => {
                    let p = ALL_PARAMS.iter().find(|p| format!("{p:?}") == *name);
                    let d = d.parse::<usize>().ok().filter(|&d| d < DRUMS);
                    if let (Some(d), Some(p), Some(v)) = (d, p, num(v)) {
                        s.voices[d][p.index()] = p.sanitize(v);
                    }
                }
                ["n", step, d, stroke, vel, pos, orn, chance] => {
                    let (Ok(step), Ok(d)) = (step.parse::<usize>(), d.parse::<usize>()) else {
                        continue;
                    };
                    let Some(stroke) = stroke.chars().next().and_then(Stroke::from_letter) else {
                        continue;
                    };
                    let note = Note {
                        stroke,
                        velocity: num(vel).unwrap_or(0.8),
                        position: num(pos).unwrap_or(0.0),
                        ornament: orn
                            .parse::<u8>()
                            .ok()
                            .and_then(Ornament::from_id)
                            .unwrap_or_default(),
                        chance: num(chance).unwrap_or(1.0),
                    };
                    s.pattern.set(step, d, Some(note));
                }
                _ => {}
            }
        }
        Some(s)
    }
}

const FORMAT_HEADER: &str = "xenopalm-state 1";

/// What a knob or drag gesture is adjusting.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    Param(usize, Param),
    Global(GlobalParam),
    /// Generator busyness.
    Busy,
    NoteVelocity(usize, usize),
    NotePosition(usize, usize),
    NoteChance(usize, usize),
}

impl Target {
    pub fn is_bipolar(self) -> bool {
        match self {
            Target::Param(_, p) => p.is_bipolar(),
            Target::Global(p) => p.is_bipolar(),
            Target::NotePosition(..) => true,
            _ => false,
        }
    }
}

pub fn drum_name(d: usize) -> &'static str {
    if d == 0 { "HI" } else { "LO" }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drag {
    pub target: Target,
    pub start_y: f64,
    pub start_value: f32,
    /// Pixels of vertical travel for the full range.
    pub span_px: f64,
    pub fine: bool,
}

/// Everything the controls need, shared via `use_context::<Synth>()`.
#[derive(Clone)]
pub struct Synth {
    pub state: Signal<PanelState>,
    pub audio: AudioHandle,
    pub drag: Signal<Option<Drag>>,
    /// The stroke new notes are written with.
    pub brush: Signal<Stroke>,
    /// The note the inspector edits.
    pub selected: Signal<Option<(usize, usize)>>,
    /// Last touched control, for the display.
    pub touched: Signal<Option<Target>>,
    /// Whether the sequencer is running (as requested by the panel).
    pub playing: Signal<bool>,
    /// The engine's mutation counters the panel has caught up with.
    seen_mutations: Rc<RefCell<[u32; DRUMS]>>,
    rng: Rc<RefCell<Noise>>,
}

impl Synth {
    pub fn new() -> Self {
        let initial = load().unwrap_or_default();
        Self {
            audio: AudioHandle::new(initial.to_snapshot()),
            state: Signal::new(initial),
            drag: Signal::new(None),
            brush: Signal::new(Stroke::Open),
            selected: Signal::new(None),
            touched: Signal::new(None),
            playing: Signal::new(false),
            seen_mutations: Rc::new(RefCell::new([0; DRUMS])),
            rng: Rc::new(RefCell::new(Noise::new(random_seed()))),
        }
    }

    // --- knobs -------------------------------------------------------------

    /// Knob position `0..=1` of a target.
    pub fn value(&self, target: Target) -> f32 {
        let s = self.state.read();
        let note = |step: usize, d: usize| s.pattern.get(step, d);
        match target {
            Target::Param(d, p) => p.info().kind.to_knob(s.voices[d][p.index()]),
            Target::Global(p) => p.info().kind.to_knob(s.globals[p.index()]),
            Target::Busy => s.busy,
            Target::NoteVelocity(st, d) => note(st, d).map_or(0.0, |n| n.velocity),
            Target::NotePosition(st, d) => note(st, d).map_or(0.5, |n| n.position + 0.5),
            Target::NoteChance(st, d) => note(st, d).map_or(1.0, |n| n.chance),
        }
    }

    /// Panel label; macro knobs take the channel engine's names.
    pub fn label(&self, target: Target) -> String {
        match target {
            Target::Param(d, p) => p.label_for(self.state.read().engine(d)).into(),
            Target::Global(p) => p.info().label.into(),
            Target::Busy => "BUSY".into(),
            Target::NoteVelocity(..) => "VEL".into(),
            Target::NotePosition(..) => "SPOT".into(),
            Target::NoteChance(..) => "CHANCE".into(),
        }
    }

    pub fn name(&self, target: Target) -> String {
        match target {
            Target::Param(d, p) => format!(
                "{} {}",
                drum_name(d),
                p.name_for(self.state.read().engine(d))
            ),
            Target::Global(p) => p.info().name.into(),
            Target::Busy => "Generator busyness".into(),
            Target::NoteVelocity(..) => "Note velocity".into(),
            Target::NotePosition(..) => "Note strike position (centre ↔ rim)".into(),
            Target::NoteChance(..) => "Note probability".into(),
        }
    }

    /// Read-out for a target at knob position `k`.
    pub fn display(&self, target: Target, k: f32) -> String {
        let s = self.state.read();
        match target {
            Target::Param(d, p) => {
                p.display_for(s.engine(d), p.sanitize(p.info().kind.from_knob(k)))
            }
            Target::Global(p) => p.display(p.sanitize(p.info().kind.from_knob(k))),
            Target::Busy => format!("{:.0}%", k * 100.0),
            Target::NoteVelocity(..) => format!("{:.0}", k * 127.0),
            Target::NotePosition(..) => {
                let p = k - 0.5;
                if p.abs() < 0.01 {
                    "AS PLAYED".into()
                } else if p < 0.0 {
                    format!("CENTRE {:.0}", -p * 200.0)
                } else {
                    format!("RIM {:.0}", p * 200.0)
                }
            }
            Target::NoteChance(..) => format!("{:.0}%", k * 100.0),
        }
    }

    /// Set a target from a knob position.
    pub fn set(&self, target: Target, k: f32) {
        let k = k.clamp(0.0, 1.0);
        match target {
            Target::Param(d, p) => self.set_param(d, p, p.info().kind.from_knob(k)),
            Target::Global(p) => self.set_global(p, p.info().kind.from_knob(k)),
            Target::Busy => {
                let mut state = self.state;
                if state.peek().busy != k {
                    state.write().busy = k;
                }
            }
            Target::NoteVelocity(st, d) => self.update_note(st, d, |n| n.velocity = k.max(0.02)),
            Target::NotePosition(st, d) => self.update_note(st, d, |n| n.position = k - 0.5),
            Target::NoteChance(st, d) => self.update_note(st, d, |n| n.chance = k),
        }
        let mut touched = self.touched;
        if *touched.peek() != Some(target) {
            touched.set(Some(target));
        }
    }

    pub fn reset(&self, target: Target) {
        let k = match target {
            Target::Param(_, p) => p.info().kind.to_knob(p.default_value()),
            Target::Global(p) => p.info().kind.to_knob(p.default_value()),
            Target::Busy => 0.45,
            Target::NoteVelocity(st, d) => self
                .state
                .peek()
                .pattern
                .get(st, d)
                .map_or(0.8, |n| n.stroke.natural_velocity()),
            Target::NotePosition(..) => 0.5,
            Target::NoteChance(..) => 1.0,
        };
        self.set(target, k);
        self.save();
    }

    pub fn set_param(&self, drum: usize, param: Param, value: f32) {
        let value = param.sanitize(value);
        let mut state = self.state;
        if state.peek().voices[drum][param.index()] != value {
            state.write().voices[drum][param.index()] = value;
            self.audio.send(Command::Param { drum, param, value });
        }
    }

    pub fn set_global(&self, param: GlobalParam, value: f32) {
        let value = param.sanitize(value);
        let mut state = self.state;
        let old = state.peek().globals[param.index()];
        if old == value {
            return;
        }
        state.write().globals[param.index()] = value;
        self.audio.send(Command::Global { param, value });
        // A longer pattern repeats what is there rather than falling silent.
        if param == GlobalParam::Length && value > old {
            let (old, new) = (old as usize, value as usize);
            let empty =
                (old..new).all(|s| (0..DRUMS).all(|d| state.peek().pattern.get(s, d).is_none()));
            if empty {
                let mut p = state.peek().pattern.clone();
                for s in old..new {
                    p.steps[s] = p.steps[s % old.max(1)];
                }
                self.set_pattern(p);
            }
        }
        let selected = *self.selected.peek();
        if param == GlobalParam::Length
            && let Some((s, _)) = selected
            && s >= value as usize
        {
            self.select(None);
        }
    }

    /// Follow the engine's MUTATE: adopt each newly mutated patch. The
    /// controls MUTATE never touches stay as the panel has them, so a knob
    /// being turned right now isn't yanked back.
    pub fn adopt(&self, status: &Status) {
        let mut seen = self.seen_mutations.borrow_mut();
        for d in 0..DRUMS {
            if status.mutations[d] == seen[d] {
                continue;
            }
            seen[d] = status.mutations[d];
            let mut state = self.state;
            let current = state.peek().voices[d];
            let mut patch = status.patches[d];
            for p in ALL_PARAMS {
                if keeps_value_on_randomize(p) {
                    patch[p.index()] = current[p.index()];
                }
            }
            if patch != current {
                state.write().voices[d] = patch;
            }
        }
    }

    // --- pattern -----------------------------------------------------------

    pub fn note(&self, step: usize, drum: usize) -> Option<Note> {
        self.state.read().pattern.get(step, drum)
    }

    pub fn set_note(&self, step: usize, drum: usize, note: Option<Note>) {
        let note = note.map(Note::sanitized);
        let mut state = self.state;
        if state.peek().pattern.get(step, drum) != note {
            state.write().pattern.set(step, drum, note);
            self.audio.send(Command::Note { step, drum, note });
        }
    }

    pub fn update_note(&self, step: usize, drum: usize, f: impl FnOnce(&mut Note)) {
        // Copy the note out first: the read guard must be gone before the write.
        let note = self.state.peek().pattern.get(step, drum);
        if let Some(mut n) = note {
            f(&mut n);
            self.set_note(step, drum, Some(n));
        }
    }

    /// Replace the whole pattern, sending only the cells that changed.
    pub fn set_pattern(&self, p: Pattern) {
        for step in 0..MAX_STEPS {
            for d in 0..DRUMS {
                self.set_note(step, d, p.get(step, d));
            }
        }
    }

    pub fn select(&self, cell: Option<(usize, usize)>) {
        let mut sel = self.selected;
        if *sel.peek() != cell {
            sel.set(cell);
        }
        // The display may be showing a note that is no longer there.
        let touched = *self.touched.peek();
        if matches!(
            touched,
            Some(Target::NoteVelocity(..) | Target::NotePosition(..) | Target::NoteChance(..))
        ) && cell.is_none()
        {
            let mut t = self.touched;
            t.set(None);
        }
    }

    /// Write a fresh pattern with the generator's style and busyness.
    pub fn generate(&self) {
        self.select(None);
        let s = self.state.peek().clone();
        let p = pattern::generate(
            s.style,
            s.busy,
            s.length(),
            s.grid(),
            &mut self.rng.borrow_mut(),
        );
        self.set_pattern(p);
        self.save();
    }

    /// Choose a style; its grid comes with it.
    pub fn set_style(&self, style: usize) {
        let style = style.min(STYLE_COUNT - 1);
        let mut state = self.state;
        state.write().style = style;
        let grid = STYLES[style].grid;
        if self.state.peek().grid() != grid {
            self.set_global(GlobalParam::Grid, grid as f32);
            let bar = pattern::steps_per_beat(grid) * 4;
            self.set_global(GlobalParam::Length, bar as f32);
        }
        self.generate();
    }

    pub fn evolve(&self) {
        let s = self.state.peek().clone();
        let mut p = s.pattern.clone();
        pattern::evolve(&mut p, s.length(), &mut self.rng.borrow_mut());
        self.set_pattern(p);
        self.save();
    }

    pub fn clear(&self) {
        self.set_pattern(Pattern::default());
        self.select(None);
        self.save();
    }

    pub fn rotate(&self, by: i32) {
        let s = self.state.peek().clone();
        let mut p = s.pattern.clone();
        p.rotate(s.length(), by);
        self.set_pattern(p);
        self.select(None);
        self.save();
    }

    // --- sounds --------------------------------------------------------------

    /// Set a whole patch, sending only the values that changed.
    fn load_voice(&self, drum: usize, patch: &VoicePatch) {
        for p in ALL_PARAMS {
            self.set_param(drum, p, patch[p.index()]);
        }
    }

    /// Load a factory preset, keeping the channel's pan, MUTATE and engine lock.
    pub fn load_preset(&self, drum: usize, preset: &Preset) {
        let current = self.state.peek().voices[drum];
        let mut patch = preset.patch();
        for p in [Param::Pan, Param::Mutate, Param::EngineLock] {
            patch[p.index()] = current[p.index()];
        }
        self.load_voice(drum, &patch);
        self.save();
    }

    /// Lock or unlock the channel's engine against MUTATE.
    pub fn toggle_engine_lock(&self, drum: usize) {
        let locked = self.state.peek().engine_locked(drum);
        self.set_param(drum, Param::EngineLock, if locked { 0.0 } else { 1.0 });
    }

    /// RND on a channel, as on the XK-1.
    pub fn randomize(&self, drum: usize) {
        let current = self.state.peek().voices[drum];
        let patch = random_patch(&current, &mut self.rng.borrow_mut());
        self.load_voice(drum, &patch);
        self.save();
    }

    /// Copy a channel to the other one, keeping the destination's pan, MUTATE
    /// and engine lock.
    pub fn copy_voice(&self, from: usize, to: usize) {
        let s = self.state.peek().clone();
        let mut patch = s.voices[from];
        for p in [Param::Pan, Param::Mutate, Param::EngineLock] {
            patch[p.index()] = s.voices[to][p.index()];
        }
        self.load_voice(to, &patch);
        self.save();
    }

    /// RND ALL: new voices, room, feel and pattern.
    pub fn randomize_all(&self) {
        let s = self.state.peek().clone();
        let kit = random_kit(&s.voices, &s.globals, &mut self.rng.borrow_mut());
        for (d, patch) in kit.voices.iter().enumerate() {
            self.load_voice(d, patch);
        }
        for p in ALL_GLOBAL_PARAMS {
            self.set_global(p, kit.globals[p.index()]);
        }
        {
            let mut state = self.state;
            let mut w = state.write();
            w.style = kit.style;
            w.busy = kit.busy;
        }
        self.generate();
    }

    // --- playing -------------------------------------------------------------

    pub fn toggle_play(&self) {
        self.audio.user_gesture();
        let mut playing = self.playing;
        let on = !*playing.peek();
        playing.set(on);
        self.audio.send(Command::Play(on));
        // MUTATE may have been rewriting the channels: keep what they became.
        if !on {
            self.save();
        }
    }

    pub fn live(&self, drum: usize, note: Note) {
        self.audio.user_gesture();
        self.audio.send(Command::Live { drum, note });
    }

    /// A small random number, for live variation.
    pub fn jitter(&self) -> f32 {
        self.rng.borrow_mut().sample()
    }

    pub fn save(&self) {
        save(&self.state.peek());
    }
}

/// Knob kind of a target (for steppers and drag spans).
pub fn target_kind(target: Target) -> ParamKind {
    match target {
        Target::Param(_, p) => p.info().kind,
        Target::Global(p) => p.info().kind,
        _ => ParamKind::Continuous,
    }
}

#[cfg(all(feature = "web", not(feature = "desktop")))]
fn random_seed() -> u32 {
    (js_sys::Math::random() * u32::MAX as f64) as u32
}

#[cfg(not(all(feature = "web", not(feature = "desktop"))))]
fn random_seed() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() ^ d.as_secs() as u32)
        .unwrap_or(12345)
}

// --- persistence -------------------------------------------------------------

#[cfg(all(feature = "web", not(feature = "desktop")))]
const STORAGE_KEY: &str = "xenopalm.state";

#[cfg(all(feature = "web", not(feature = "desktop")))]
fn load() -> Option<PanelState> {
    let storage = web_sys::window()?.local_storage().ok()??;
    PanelState::deserialize(&storage.get_item(STORAGE_KEY).ok()??)
}

#[cfg(all(feature = "web", not(feature = "desktop")))]
fn save(state: &PanelState) {
    if let Some(Ok(Some(storage))) = web_sys::window().map(|w| w.local_storage()) {
        let _ = storage.set_item(STORAGE_KEY, &state.serialize());
    }
}

#[cfg(not(all(feature = "web", not(feature = "desktop"))))]
fn state_path() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config"))
        })?;
    Some(base.join("xenopalm").join("state.txt"))
}

#[cfg(not(all(feature = "web", not(feature = "desktop"))))]
fn load() -> Option<PanelState> {
    PanelState::deserialize(&std::fs::read_to_string(state_path()?).ok()?)
}

#[cfg(not(all(feature = "web", not(feature = "desktop"))))]
fn save(state: &PanelState) {
    let Some(path) = state_path() else { return };
    let result = path
        .parent()
        .map(std::fs::create_dir_all)
        .unwrap_or(Ok(()))
        .and_then(|_| std::fs::write(&path, state.serialize()));
    if let Err(e) = result {
        eprintln!("xenopalm: could not save state to {}: {e}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xenopalm_dsp::params::ENGINE_MODAL;

    #[test]
    fn default_state_is_a_tumbao_on_xk1_toms() {
        let s = PanelState::default();
        assert_eq!(s.engine(0), ENGINE_MODAL);
        assert!(s.pattern.count(16) > 8);
        assert_eq!(s.length(), 16);
    }

    #[test]
    fn state_roundtrip() {
        let mut s = PanelState {
            style: 3,
            busy: 0.8,
            ..Default::default()
        };
        s.globals[GlobalParam::Tempo.index()] = 133.0;
        s.voices[1][Param::Engine.index()] = 6.0;
        s.voices[1][Param::Mutate.index()] = 1.0;
        let mut n = Note::new(Stroke::Slap).with_velocity(0.5);
        n.ornament = Ornament::Run;
        n.chance = 0.25;
        n.position = 0.125;
        s.pattern.set(40, 1, Some(n));
        let back = PanelState::deserialize(&s.serialize()).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn rejects_foreign_text_and_tolerates_garbage() {
        assert!(PanelState::deserialize("hello").is_none());
        assert!(PanelState::deserialize("palmkussion-state 1\n").is_none());
        let text = format!(
            "{FORMAT_HEADER}\ng Tempo 150\nv 0 Nope 1\nv 7 Pitch 0.2\nn 3 0 Q 1 0 0 1\nn 2 1 s 0.5 0 1 1\nv 1 Mutate 2\n"
        );
        let s = PanelState::deserialize(&text).unwrap();
        assert_eq!(s.global(GlobalParam::Tempo), 150.0);
        assert_eq!(s.pattern.count(64), 1);
        assert_eq!(s.voices[1][Param::Mutate.index()], 1.0);
    }

    #[test]
    fn snapshot_replays_everything() {
        let s = PanelState::default();
        let snap = s.to_snapshot();
        let mut rebuilt = Snapshot {
            voices: [default_patch(); DRUMS],
            globals: default_global_patch(),
            pattern: Pattern::default(),
        };
        for cmd in snap.commands() {
            rebuilt.apply(cmd);
        }
        assert_eq!(rebuilt.voices, s.voices);
        assert_eq!(rebuilt.globals, s.globals);
        assert_eq!(rebuilt.pattern, s.pattern);
    }

    /// The JS processor must read exactly as many status floats as the engine writes.
    #[test]
    fn worklet_status_length_matches_the_engine() {
        let js = include_str!("audio/worklet.js");
        assert!(js.contains(&format!("const STATUS_LEN = {};", Status::WIRE_LEN)));
    }
}
