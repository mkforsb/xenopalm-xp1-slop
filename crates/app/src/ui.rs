//! The panel: transport and player, the step sequencer with its inspector,
//! two XK-1 channel strips (each with a playable drum head and MUTATE) and
//! the room.

use dioxus::html::input_data::MouseButton;
use dioxus::prelude::*;
use xenopalm_dsp::params::{
    CHAOS_DEST_NAMES, ENGINE_BLURBS, ENGINE_NAMES, FILTER_TYPE_NAMES, GRID_NAMES, ParamKind,
    macro_label, map,
};
use xenopalm_dsp::pattern::{STYLES, steps_per_beat};
use xenopalm_dsp::presets::PRESETS;
use xenopalm_dsp::stroke::{ALL_ORNAMENTS, ALL_STROKES};
use xenopalm_dsp::{DRUMS, GlobalParam, Note, Ornament, Param, Strike, Stroke};

use crate::audio::AudioStatus;
use crate::state::{Drag, Synth, Target, drum_name, target_kind};

const STYLE: &str = include_str!("../assets/style.css");

/// Pixels of vertical drag for a knob's full travel.
const KNOB_SPAN_PX: f64 = 200.0;
/// Pixels of vertical drag for a note's full velocity range.
const CELL_SPAN_PX: f64 = 140.0;
/// Fader geometry; keep in sync with `.fader-track` / `.fader-cap` in the CSS.
const FADER_TRACK_PX: f64 = 140.0;
const FADER_CAP_PX: f64 = 24.0;
const FADER_LEDS: usize = 10;
/// Drum head pad size; keep in sync with `.head` in the CSS.
const HEAD_PX: f64 = 168.0;
/// Hit preview: columns, render rate and the velocity it is drawn at.
const SCOPE_COLUMNS: usize = 160;
const SCOPE_FS: f32 = 12_000.0;
const SCOPE_VELOCITY: f32 = 0.85;

/// Keyboard: two rows of strokes, centre to rim, one row per drum.
const KEY_STROKES: &[(Code, usize, Stroke)] = &[
    (Code::KeyQ, 0, Stroke::Bass),
    (Code::KeyW, 0, Stroke::Heel),
    (Code::KeyE, 0, Stroke::Tip),
    (Code::KeyR, 0, Stroke::Mute),
    (Code::KeyT, 0, Stroke::Open),
    (Code::KeyY, 0, Stroke::Slap),
    (Code::KeyA, 1, Stroke::Bass),
    (Code::KeyS, 1, Stroke::Heel),
    (Code::KeyD, 1, Stroke::Tip),
    (Code::KeyF, 1, Stroke::Mute),
    (Code::KeyG, 1, Stroke::Open),
    (Code::KeyH, 1, Stroke::Slap),
];

/// Keyboard: ornaments played live on the high drum.
const KEY_ORNAMENTS: &[(Code, Ornament)] = &[
    (Code::KeyZ, Ornament::Flam),
    (Code::KeyX, Ornament::Trill),
    (Code::KeyC, Ornament::Run),
];

fn key_label(stroke: Stroke, drum: usize) -> &'static str {
    let row = if drum == 0 {
        ["Q", "W", "E", "R", "T", "Y"]
    } else {
        ["A", "S", "D", "F", "G", "H"]
    };
    row[stroke as usize]
}

#[component]
pub fn App() -> Element {
    let synth = use_context_provider(Synth::new);
    let drag = synth.drag;

    let s_move = synth.clone();
    let s_up = synth.clone();
    let s_down = synth.clone();
    let s_key = synth.clone();
    let s_adopt = synth.clone();
    // Follow MUTATE: the engine reports each mutated patch, the knobs move.
    use_effect(move || {
        let status = *s_adopt.audio.engine.read();
        s_adopt.adopt(&status);
    });
    let end_drag = move || {
        let mut drag = s_up.drag;
        if drag.peek().is_some() {
            drag.set(None);
            s_up.save();
        }
    };
    let end_drag_leave = end_drag.clone();
    let end_drag_up = end_drag;

    rsx! {
        style { {STYLE} }
        div {
            class: if drag.read().is_some() { "root dragging" } else { "root" },
            tabindex: "0",
            autofocus: true,
            onpointerdown: move |_| s_down.audio.user_gesture(),
            onpointermove: move |e: PointerEvent| {
                let Some(d) = *s_move.drag.peek() else { return };
                e.prevent_default();
                let y = e.client_coordinates().y;
                let fine = e.modifiers().shift();
                let mut drag = s_move.drag;
                if fine != d.fine {
                    // Re-anchor so toggling SHIFT mid-drag doesn't jump.
                    drag.set(Some(Drag { start_y: y, start_value: s_move.value(d.target), fine, ..d }));
                    return;
                }
                let scale = if fine { 0.1 } else { 1.0 };
                let v = d.start_value as f64 + (d.start_y - y) / d.span_px * scale;
                s_move.set(d.target, v as f32);
            },
            onpointerup: move |_| end_drag_up(),
            onpointerleave: move |_| end_drag_leave(),
            onkeydown: move |e: KeyboardEvent| on_key(&s_key, e),
            Header {}
            main { class: "panel",
                Performance {}
                Sequencer {}
                div { class: "drums",
                    for d in 0..DRUMS {
                        ChannelStrip { key: "{d}", drum: d }
                    }
                }
                Room {}
            }
            footer { class: "footer",
                span { "Keys: " }
                kbd { "Q W E R T Y" }
                span { " high drum, " }
                kbd { "A S D F G H" }
                span { " low drum (bass · heel · tip · mute · open · slap; " }
                kbd { "Shift" }
                span { " = accent) · " }
                kbd { "Z" }
                span { " flam " }
                kbd { "X" }
                span { " trill " }
                kbd { "C" }
                span { " run " }
                kbd { "V" }
                span { " both hands · " }
                kbd { "Space" }
                span { " play/stop · " }
                kbd { "↑ ↓" }
                span { " velocity, " }
                kbd { "← →" }
                span { " move, " }
                kbd { "Del" }
                span { " erase the selected note. Drag knobs vertically (Shift = fine), scroll to nudge, double-click to reset. MUTATE acts once per note: every hit of the player, but not the strokes inside a flam, roll or trill." }
            }
        }
    }
}

fn on_key(synth: &Synth, e: KeyboardEvent) {
    let m = e.modifiers();
    if m.ctrl() || m.alt() || m.meta() {
        return;
    }
    let code = e.code();
    let accent = m.shift();
    let velocity = |s: &Synth| {
        if accent {
            1.0
        } else {
            0.72 + 0.05 * s.jitter()
        }
    };
    if !e.is_auto_repeating()
        && let Some(&(_, drum, stroke)) = KEY_STROKES.iter().find(|(c, _, _)| *c == code)
    {
        // Also stops a focused <select> from type-ahead jumping.
        e.prevent_default();
        let mut n = Note::new(stroke).with_velocity(velocity(synth));
        n.position = 0.04 * synth.jitter();
        synth.live(drum, n);
        return;
    }
    if !e.is_auto_repeating()
        && let Some(&(_, ornament)) = KEY_ORNAMENTS.iter().find(|(c, _)| *c == code)
    {
        e.prevent_default();
        let mut n = Note::new(Stroke::Open).with_velocity(velocity(synth));
        n.ornament = ornament;
        synth.live(0, n);
        return;
    }
    let selected = *synth.selected.peek();
    match code {
        Code::KeyV if !e.is_auto_repeating() => {
            e.prevent_default();
            for d in 0..DRUMS {
                let n = Note::new(Stroke::Open).with_velocity(velocity(synth));
                synth.live(d, n);
            }
        }
        Code::Space if !e.is_auto_repeating() => {
            e.prevent_default();
            synth.toggle_play();
        }
        Code::Delete | Code::Backspace => {
            if let Some((s, d)) = selected {
                e.prevent_default();
                synth.set_note(s, d, None);
                synth.save();
            }
        }
        Code::ArrowUp | Code::ArrowDown => {
            if let Some((s, d)) = selected {
                e.prevent_default();
                let step = if accent { 0.01 } else { 0.05 };
                let delta = if code == Code::ArrowUp { step } else { -step };
                synth.update_note(s, d, |n| n.velocity = (n.velocity + delta).clamp(0.02, 1.0));
                synth.save();
            }
        }
        Code::ArrowLeft | Code::ArrowRight => {
            if let Some((s, d)) = selected {
                e.prevent_default();
                let len = synth.state.peek().length();
                let s = if code == Code::ArrowLeft {
                    (s + len - 1) % len
                } else {
                    (s + 1) % len
                };
                synth.select(Some((s, d)));
            }
        }
        _ => synth.audio.user_gesture(),
    }
}

// --- header ----------------------------------------------------------------------

#[component]
fn Header() -> Element {
    let synth = use_context::<Synth>();
    let status = synth.audio.status.read().clone();
    let (class, text) = match status {
        AudioStatus::NeedsGesture => (
            "status warn",
            "Click anywhere or press a key to start audio".to_string(),
        ),
        AudioStatus::Starting => ("status warn", "Starting audio…".to_string()),
        AudioStatus::Running {
            sample_rate,
            detail,
        } => (
            "status ok",
            format!("{detail} · {:.1} kHz", sample_rate as f32 / 1000.0),
        ),
        AudioStatus::Failed(e) => ("status err", format!("Audio unavailable: {e}")),
    };
    rsx! {
        header { class: "header",
            div { class: "brand",
                Logo {}
                span { class: "logo", "XENOPALM" }
                span { class: "model", "XP-1" }
                span { class: "tagline", "Alien Hand Drum Performance Synthesizer" }
            }
            div { class: "{class}",
                span { class: "status-dot" }
                span { "{text}" }
            }
            div { class: "master",
                Knob { target: Target::Global(GlobalParam::Master), size: "sm" }
            }
        }
    }
}

// --- transport & player -------------------------------------------------------------

/// The brand mark: a Lorenz butterfly (the XK-1's) over a drum head.
#[component]
fn Logo() -> Element {
    let path = use_hook(|| attractor_path(34.0, 22.0, 1_600));
    rsx! {
        svg { class: "brand-mark", view_box: "0 0 34 28",
            ellipse { class: "brand-head", cx: "17", cy: "24", rx: "15", ry: "3.2" }
            path { d: "{path}" }
        }
    }
}

#[component]
fn Performance() -> Element {
    let synth = use_context::<Synth>();
    let playing = *synth.playing.read();
    let s_play = synth.clone();
    let s_rnd = synth.clone();
    let g = Target::Global;
    rsx! {
        section { class: "sec perf",
            div { class: "transport",
                button {
                    class: if playing { "play on" } else { "play" },
                    title: "Start/stop the player (Space)",
                    onclick: move |_| s_play.toggle_play(),
                    svg { view_box: "0 0 20 20",
                        if playing {
                            path { d: "M5 5h10v10H5z" }
                        } else {
                            path { d: "M6 4l10 6-10 6z" }
                        }
                    }
                    span { if playing { "STOP" } else { "PLAY" } }
                }
                Knob { target: g(GlobalParam::Tempo) }
                Knob { target: g(GlobalParam::Swing) }
                Knob { target: g(GlobalParam::Length) }
                Switch { target: g(GlobalParam::Grid), names: GRID_NAMES.to_vec() }
                Lcd {}
            }
            div { class: "player",
                div { class: "sec-title", "PLAYER" }
                div { class: "row",
                    Knob { target: g(GlobalParam::Timing) }
                    Knob { target: g(GlobalParam::Velo) }
                    Knob { target: g(GlobalParam::Spot) }
                    Knob { target: g(GlobalParam::Improv) }
                }
            }
            div { class: "kit",
                button {
                    class: "btn rnd-all",
                    title: "Randomize everything: both channels (not their MUTATE), room, feel and pattern",
                    onclick: move |_| s_rnd.randomize_all(),
                    "RND ALL"
                }
            }
        }
    }
}

/// Small read-out: the last touched control and where the player is.
#[component]
fn Lcd() -> Element {
    let synth = use_context::<Synth>();
    let status = *synth.audio.engine.read();
    let state = synth.state.read();
    let len = state.length();
    let line1 = match *synth.touched.read() {
        Some(t) => format!(
            "{} {}",
            synth.name(t).to_uppercase(),
            synth.display(t, synth.value(t))
        ),
        None => format!(
            "{} · {}",
            STYLES[state.style].name,
            GlobalParam::Tempo.display(state.global(GlobalParam::Tempo))
        ),
    };
    let line2 = if *synth.playing.read() && status.step >= 0 {
        let spb = steps_per_beat(state.grid());
        let step = status.step as usize;
        format!("▶ STEP {:>2}/{len} · BEAT {}", step + 1, step / spb + 1)
    } else {
        format!("■ {len} STEPS · {}", GRID_NAMES[state.grid() as usize])
    };
    rsx! {
        div { class: "lcd",
            div { class: "lcd-line", "{line1}" }
            div { class: "lcd-line", "{line2}" }
        }
    }
}

// --- sequencer ------------------------------------------------------------------------

#[component]
fn Sequencer() -> Element {
    let synth = use_context::<Synth>();
    let state = synth.state.read();
    let len = state.length();
    let bar = steps_per_beat(state.grid()) * 4;
    let rows = len.div_ceil(bar);
    let style = state.style;
    drop(state);
    let brush = *synth.brush.read();
    let s_style = synth.clone();
    let s_gen = synth.clone();
    let s_evo = synth.clone();
    let s_left = synth.clone();
    let s_right = synth.clone();
    let s_clear = synth.clone();
    rsx! {
        section { class: "sec seq",
            div { class: "seq-toolbar",
                div { class: "sec-title", "PATTERN" }
                select {
                    class: "preset",
                    title: "Rhythm style for the generator",
                    value: "{style}",
                    onchange: move |e| {
                        if let Ok(i) = e.value().parse::<usize>() {
                            s_style.set_style(i);
                        }
                    },
                    for (i, s) in STYLES.iter().enumerate() {
                        option { key: "{i}", value: "{i}", selected: i == style, "{s.name}" }
                    }
                }
                Knob { target: Target::Busy, size: "sm" }
                button { class: "btn", title: "Write a new pattern in this style", onclick: move |_| s_gen.generate(), "GENERATE" }
                button { class: "btn", title: "A few small changes", onclick: move |_| s_evo.evolve(), "EVOLVE" }
                button { class: "btn", title: "Shift the pattern one step earlier", onclick: move |_| s_left.rotate(-1), "◀" }
                button { class: "btn", title: "Shift the pattern one step later", onclick: move |_| s_right.rotate(1), "▶" }
                button { class: "btn", title: "Erase every note", onclick: move |_| s_clear.clear(), "CLEAR" }
                div { class: "brushes", role: "radiogroup", "aria-label": "Brush stroke",
                    span { class: "brush-title", "BRUSH" }
                    for s in ALL_STROKES {
                        BrushButton { key: "{s.letter()}", stroke: s, selected: s == brush }
                    }
                }
            }
            div { class: "style-blurb", "{STYLES[style].blurb}" }
            div { class: "seq-body",
                div { class: "grid-wrap",
                    for row in 0..rows {
                        BarRow { key: "{row}", start: row * bar, end: ((row + 1) * bar).min(len), bar }
                    }
                }
                Inspector {}
            }
        }
    }
}

#[component]
fn BrushButton(stroke: Stroke, selected: bool) -> Element {
    let synth = use_context::<Synth>();
    rsx! {
        button {
            class: if selected { "brush sel" } else { "brush" },
            role: "radio",
            "aria-checked": "{selected}",
            title: "{stroke.name()}: write new notes with this stroke",
            onclick: move |_| {
                let mut b = synth.brush;
                b.set(stroke);
            },
            StrokeGlyph { stroke }
            span { "{stroke.letter()}" }
        }
    }
}

/// One bar of the grid: step numbers and a lane per drum.
#[component]
fn BarRow(start: usize, end: usize, bar: usize) -> Element {
    let synth = use_context::<Synth>();
    let spb = bar / 4;
    let state = synth.state.read();
    let engines = [state.engine(0), state.engine(1)];
    drop(state);
    rsx! {
        div { class: "bar", style: "--steps: {bar}",
            div { class: "lane-label" }
            for s in start..end {
                div {
                    key: "{s}",
                    class: if s % spb == 0 { "step-num beat" } else { "step-num" },
                    if s % spb == 0 { "{s / spb % 4 + 1}" } else { "·" }
                }
            }
            for d in 0..DRUMS {
                div { key: "l{d}", class: "lane-label d{d}",
                    span { class: "lane-name", "{drum_name(d)}" }
                    span { class: "lane-model", "{ENGINE_NAMES[engines[d] as usize]}" }
                }
                for s in start..end {
                    Cell { key: "{d}-{s}", step: s, drum: d, beat: s % spb == 0 }
                }
            }
        }
    }
}

#[component]
fn Cell(step: usize, drum: usize, beat: bool) -> Element {
    let synth = use_context::<Synth>();
    let note = synth.note(step, drum);
    let playing = *synth.playing.read();
    let here = playing && synth.audio.engine.read().step == step as i32;
    let selected = *synth.selected.read() == Some((step, drum));
    let mut class = String::from("cell");
    class += &format!(" d{drum}");
    if beat {
        class += " beat";
    }
    if here {
        class += " now";
    }
    if selected {
        class += " sel";
    }
    if let Some(n) = note {
        class += " on";
        if n.stroke.is_ghost() || n.velocity < 0.45 {
            class += " ghost";
        }
        if n.chance < 1.0 {
            class += " maybe";
        }
    }
    let vel = note.map_or(0.0, |n| n.velocity);
    let title = match note {
        Some(n) => format!(
            "{} · vel {:.0}{}{}",
            n.stroke.name(),
            n.velocity * 127.0,
            if n.ornament != Ornament::None {
                format!(" · {}", n.ornament.name())
            } else {
                String::new()
            },
            if n.chance < 1.0 {
                format!(" · {:.0}%", n.chance * 100.0)
            } else {
                String::new()
            },
        ),
        None => "empty: click to write the brush stroke".into(),
    };
    let s_down = synth.clone();
    let s_dbl = synth.clone();
    rsx! {
        div {
            class: "{class}",
            style: "--vel: {vel:.3}",
            title: "{title}",
            oncontextmenu: move |e| e.prevent_default(),
            onpointerdown: move |e: PointerEvent| {
                e.prevent_default();
                let synth = &s_down;
                synth.audio.user_gesture();
                if e.trigger_button() == Some(MouseButton::Secondary) {
                    synth.set_note(step, drum, None);
                    synth.save();
                    return;
                }
                let brush = *synth.brush.peek();
                match synth.note(step, drum) {
                    None => {
                        synth.set_note(step, drum, Some(Note::new(brush)));
                        // Let the player hear what was written.
                        if !*synth.playing.peek() {
                            synth.live(drum, Note::new(brush));
                        }
                    }
                    Some(n) if e.modifiers().shift() && n.stroke != brush => {
                        synth.update_note(step, drum, |n| n.stroke = brush);
                    }
                    Some(_) => {}
                }
                synth.select(Some((step, drum)));
                begin_drag(synth, Target::NoteVelocity(step, drum), &e, CELL_SPAN_PX);
            },
            ondoubleclick: move |_| {
                s_dbl.set_note(step, drum, None);
                s_dbl.select(None);
                s_dbl.save();
            },
            if let Some(n) = note {
                div { class: "vel-bar" }
                StrokeGlyph { stroke: n.stroke }
                if n.ornament != Ornament::None {
                    span { class: "orn", "{n.ornament.badge()}" }
                }
                if n.position.abs() > 0.06 {
                    span {
                        class: "pos",
                        style: "left: {50.0 + n.position * 80.0:.0}%",
                    }
                }
            }
        }
    }
}

/// Editor for the selected note.
#[component]
fn Inspector() -> Element {
    let synth = use_context::<Synth>();
    let Some((step, drum)) = *synth.selected.read() else {
        return rsx! {
            div { class: "inspector empty",
                div { class: "sec-title", "NOTE" }
                p { "Click a step to write the brush stroke, then drag up or down to set its velocity." }
                p { "Click a note to edit it here. Shift-click paints the brush over it; right-click or double-click erases." }
                p { "Ornaments turn a note into flams, rolls, trills and runs across both drums." }
            }
        };
    };
    let Some(note) = synth.note(step, drum) else {
        return rsx! {
            div { class: "inspector empty",
                div { class: "sec-title", "NOTE" }
                p { "Step {step + 1} on the {drum_name(drum)} drum is empty." }
            }
        };
    };
    let s_play = synth.clone();
    let s_del = synth.clone();
    let s_move = synth.clone();
    rsx! {
        div { class: "inspector",
            div { class: "sec-title",
                "STEP {step + 1} · {drum_name(drum)}"
                button {
                    class: "btn mini",
                    title: "Move the note to the other drum",
                    onclick: move |_| {
                        let other = 1 - drum;
                        if s_move.note(step, other).is_none() {
                            s_move.set_note(step, other, Some(note));
                            s_move.set_note(step, drum, None);
                            s_move.select(Some((step, other)));
                            s_move.save();
                        }
                    },
                    "⇅"
                }
            }
            div { class: "insp-strokes",
                for s in ALL_STROKES {
                    InspectorStroke { key: "{s.letter()}", step, drum, stroke: s, selected: s == note.stroke }
                }
            }
            div { class: "row",
                Knob { target: Target::NoteVelocity(step, drum), size: "sm" }
                Knob { target: Target::NotePosition(step, drum), size: "sm" }
                Knob { target: Target::NoteChance(step, drum), size: "sm" }
            }
            div { class: "insp-orns",
                for o in ALL_ORNAMENTS {
                    InspectorOrnament { key: "{o.id()}", step, drum, ornament: o, selected: o == note.ornament }
                }
            }
            div { class: "insp-actions",
                button { class: "btn", onclick: move |_| s_play.live(drum, note), "AUDITION" }
                button {
                    class: "btn",
                    onclick: move |_| {
                        s_del.set_note(step, drum, None);
                        s_del.select(None);
                        s_del.save();
                    },
                    "ERASE"
                }
            }
        }
    }
}

#[component]
fn InspectorStroke(step: usize, drum: usize, stroke: Stroke, selected: bool) -> Element {
    let synth = use_context::<Synth>();
    rsx! {
        button {
            class: if selected { "brush sel" } else { "brush" },
            title: "{stroke.name()}",
            onclick: move |_| {
                synth.update_note(step, drum, |n| n.stroke = stroke);
                synth.save();
            },
            StrokeGlyph { stroke }
            span { "{stroke.name()}" }
        }
    }
}

#[component]
fn InspectorOrnament(step: usize, drum: usize, ornament: Ornament, selected: bool) -> Element {
    let synth = use_context::<Synth>();
    rsx! {
        button {
            class: if selected { "chip sel" } else { "chip" },
            onclick: move |_| {
                synth.update_note(step, drum, |n| n.ornament = ornament);
                synth.save();
            },
            "{ornament.name()}"
        }
    }
}

/// Hand-drum notation: filled circles are palm strokes (big) and tips
/// (small), an open circle is an open tone, a crossed one a slap, a barred
/// one a muted tone.
#[component]
fn StrokeGlyph(stroke: Stroke) -> Element {
    rsx! {
        svg { class: "glyph", view_box: "0 0 20 20",
            match stroke {
                Stroke::Bass => rsx! { circle { class: "fill", cx: "10", cy: "10", r: "7" } },
                Stroke::Heel => rsx! {
                    circle { class: "line", cx: "10", cy: "10", r: "7" }
                    path { class: "fill", d: "M3 10a7 7 0 0 0 14 0z" }
                },
                Stroke::Tip => rsx! { circle { class: "fill", cx: "10", cy: "10", r: "3.6" } },
                Stroke::Mute => rsx! {
                    circle { class: "line", cx: "10", cy: "10", r: "7" }
                    path { class: "line", d: "M4.5 10h11" }
                },
                Stroke::Open => rsx! { circle { class: "line", cx: "10", cy: "10", r: "7" } },
                Stroke::Slap => rsx! {
                    circle { class: "line", cx: "10", cy: "10", r: "7" }
                    path { class: "line", d: "M5.6 5.6l8.8 8.8M14.4 5.6l-8.8 8.8" }
                },
            }
        }
    }
}

// --- drums -----------------------------------------------------------------------------

// --- channels ----------------------------------------------------------------------

#[component]
fn ChannelStrip(drum: usize) -> Element {
    let synth = use_context::<Synth>();
    let engine = synth.state.read().engine(drum) as usize;
    let locked = synth.state.read().engine_locked(drum);
    let other = 1 - drum;
    let s_preset = synth.clone();
    let s_rand = synth.clone();
    let s_copy = synth.clone();
    let p = |param: Param| Target::Param(drum, param);
    let chaos_names: Vec<&'static str> = CHAOS_DEST_NAMES
        .iter()
        .enumerate()
        .map(|(i, n)| match i {
            1..=3 => macro_label(engine as u32, i - 1),
            _ => n,
        })
        .collect();
    rsx! {
        section { class: "sec drum d{drum}",
            div { class: "drum-head",
                h2 { "{drum_name(drum)} CHANNEL" }
                select {
                    class: "preset",
                    title: "Load an XK-1 preset (keeps pan and MUTATE)",
                    onchange: move |e| {
                        if let Some(p) = e.value().parse::<usize>().ok().and_then(|i| PRESETS.get(i)) {
                            s_preset.load_preset(drum, p);
                        }
                    },
                    option { value: "", disabled: true, selected: true, "Preset…" }
                    for (i, p) in PRESETS.iter().enumerate() {
                        option { key: "{i}", value: "{i}", "{p.name}" }
                    }
                }
                button {
                    class: "btn",
                    title: "Randomize this channel",
                    onclick: move |_| s_rand.randomize(drum),
                    "RND"
                }
                button {
                    class: "btn",
                    title: "Copy this channel to the {drum_name(other)} channel",
                    onclick: move |_| s_copy.copy_voice(drum, other),
                    "→ {drum_name(other)}"
                }
            }
            div { class: "models engines", role: "radiogroup", "aria-label": "Engine",
                for (i, name) in ENGINE_NAMES.iter().enumerate() {
                    EngineButton {
                        key: "{i}",
                        drum,
                        index: i,
                        name,
                        selected: i == engine,
                        locked: i == engine && locked,
                    }
                }
            }
            div { class: "model-blurb", "{ENGINE_BLURBS[engine]}" }
            div { class: "drum-play",
                div { class: "head-col",
                    DrumHead { drum }
                    div { class: "keys", title: "Keyboard",
                        for s in ALL_STROKES {
                            span { key: "{s.letter()}",
                                kbd { "{key_label(s, drum)}" }
                                "{s.letter()}"
                            }
                        }
                    }
                }
                HitScope { drum }
            }
            div { class: "drum-body",
                div { class: "row faders",
                    div { class: "tune-group",
                        Knob { target: p(Param::Fine), size: "sm" }
                        Fader { target: p(Param::Pitch) }
                    }
                    Fader { target: p(Param::Decay) }
                    div { class: "macros",
                        Knob { target: p(Param::MacroX), size: "xl" }
                        Knob { target: p(Param::MacroY), size: "xl" }
                        Knob { target: p(Param::MacroZ), size: "xl" }
                    }
                }
                div { class: "blocks",
                    div { class: "block",
                        div { class: "sec-title", "EXCITER" }
                        div { class: "row",
                            Knob { target: p(Param::Strike), size: "sm" }
                            Knob { target: p(Param::Noise), size: "sm" }
                        }
                    }
                    div { class: "block",
                        div { class: "sec-title", "SWEEP" }
                        div { class: "row",
                            Knob { target: p(Param::SweepSpeed), size: "sm" }
                            Knob { target: p(Param::SweepDepth), size: "sm" }
                        }
                    }
                    div { class: "block",
                        div { class: "sec-title", "DIRT" }
                        div { class: "row",
                            Knob { target: p(Param::Fold), size: "sm" }
                            Knob { target: p(Param::Crush), size: "sm" }
                        }
                    }
                    div { class: "block wide",
                        div { class: "sec-title",
                            "CHAOS"
                            ChaosGlyph { drum }
                        }
                        div { class: "row",
                            Knob { target: p(Param::ChaosRate), size: "sm" }
                            Knob { target: p(Param::ChaosDepth), size: "sm" }
                            Switch { target: p(Param::ChaosDest), names: chaos_names, vertical: true }
                            Knob { target: p(Param::Jitter), size: "sm" }
                        }
                    }
                    div { class: "block wide",
                        div { class: "sec-title", "FILTER" }
                        div { class: "row",
                            Knob { target: p(Param::Cutoff), size: "sm" }
                            Knob { target: p(Param::Reso), size: "sm" }
                            Knob { target: p(Param::FilterEnv), size: "sm" }
                            Switch { target: p(Param::FilterType), names: FILTER_TYPE_NAMES.to_vec(), vertical: true }
                        }
                    }
                }
                div { class: "row out",
                    Knob { target: p(Param::Level), size: "sm" }
                    Knob { target: p(Param::Sense), size: "sm" }
                    Knob { target: p(Param::Pan), size: "sm" }
                    Knob { target: p(Param::ReverbMix), size: "sm" }
                    div { class: "mutate",
                        Knob { target: p(Param::Mutate) }
                    }
                }
            }
        }
    }
}

#[component]
/// An engine button. Clicking the selected engine locks it against MUTATE
/// (a padlock appears); clicking it again unlocks it.
fn EngineButton(
    drum: usize,
    index: usize,
    name: &'static str,
    selected: bool,
    locked: bool,
) -> Element {
    let synth = use_context::<Synth>();
    let title = match (selected, locked) {
        (true, true) => format!(
            "{}: locked, MUTATE leaves the engine alone. Click to unlock.",
            ENGINE_BLURBS[index]
        ),
        (true, false) => format!(
            "{}. Click again to lock it against MUTATE.",
            ENGINE_BLURBS[index]
        ),
        _ => ENGINE_BLURBS[index].to_string(),
    };
    rsx! {
        button {
            class: "model",
            class: if selected { "sel" },
            class: if locked { "locked" },
            role: "radio",
            "aria-checked": "{selected}",
            "aria-pressed": if selected { "{locked}" },
            title: "{title}",
            onclick: move |_| {
                if selected {
                    synth.toggle_engine_lock(drum);
                } else {
                    synth.set_param(drum, Param::Engine, index as f32);
                }
                synth.save();
            },
            svg { class: "model-glyph", view_box: "0 0 24 24",
                path { d: "{engine_glyph(index)}" }
            }
            span { "{name}" }
            if locked {
                svg { class: "lock", view_box: "0 0 12 14", "aria-label": "locked",
                    path { class: "lock-shackle", d: "M3.5 6V4a2.5 2.5 0 0 1 5 0v2" }
                    rect { class: "lock-body", x: "2", y: "6", width: "8", height: "6.5", rx: "1.2" }
                }
            }
        }
    }
}

/// Line-art glyph per engine (from the XK-1), in a 24×24 box.
fn engine_glyph(engine: usize) -> &'static str {
    match engine {
        // MODAL: mode lines of falling height.
        0 => "M3 21V5M7 21V9M11 21V7M15 21V13M19 21V11M22 21V16",
        // STRING: a plucked string between two bridges.
        1 => "M2 12h2M20 12h2M4 9v6M20 9v6M4 12C8 4 10 4 12 12S16 20 20 12",
        // COMB: comb teeth with a feedback loop.
        2 => "M3 7h18M5 7v12M9 7v9M13 7v12M17 7v9M21 7v12M3 3h18",
        // GONG: concentric rings.
        3 => "M12 4a8 8 0 1 0 0.01 0M12 8a4 4 0 1 0 0.01 0M12 11.5a0.5 0.5 0 1 0 0.01 0",
        // GRAIN: a scatter of grains.
        4 => "M4 6h1M9 4h1M15 7h1M19 4h1M6 12h1M12 11h1M18 13h1M3 18h1M9 17h1M14 20h1M20 18h1",
        // SPECTRA: random partial bars.
        5 => "M3 21V6M6 21V14M9 21V9M12 21V17M15 21V11M18 21V19M21 21V15",
        // FLUX: two oscillators modulating each other.
        6 => "M9 12a5 5 0 1 0 0.01 0M15 12a5 5 0 1 0 0.01 0",
        // LORENZ: a butterfly.
        _ => "M12 12C8 3 2 6 5 12S10 17 12 12C14 7 22 4 19 12S14 21 12 12",
    }
}

/// Where each stroke usually lands, centre (0) to rim (1), for the drum head.
fn stroke_radius(stroke: Stroke) -> f32 {
    match stroke {
        Stroke::Bass => 0.05,
        Stroke::Heel => 0.3,
        Stroke::Tip => 0.55,
        Stroke::Mute => 0.78,
        Stroke::Open => 0.82,
        Stroke::Slap => 0.68,
    }
}

/// A playable (alien) drum head. Where you hit chooses the stroke and how far
/// off its usual spot it lands: bass in the middle, tips, then open tones
/// and slaps at the edge. Shift gives the closed versions. The last hit ripples.
#[component]
fn DrumHead(drum: usize) -> Element {
    let synth = use_context::<Synth>();
    let status = *synth.audio.engine.read();
    let hits = status.hits[drum];
    let last = status.last[drum];
    let r = ((stroke_radius(last.stroke) + last.position).clamp(0.0, 0.97) * 44.0) as f64;
    // Spread hits around the lower half of the head, where hands land.
    let angle = 200.0 + (hits as f64 * 137.5) % 140.0;
    let (hx, hy) = polar(50.0, 50.0, r, angle);
    let size = 4.0 + 10.0 * last.velocity as f64;
    let s_down = synth.clone();
    rsx! {
        div {
            class: "head",
            title: "Hit the {drum_name(drum)} channel: centre for bass, edge for open tones and slaps. Shift for muted strokes.",
            onpointerdown: move |e: PointerEvent| {
                e.prevent_default();
                let p = e.element_coordinates();
                let (dx, dy) = (p.x - HEAD_PX / 2.0, p.y - HEAD_PX / 2.0);
                let r = ((dx * dx + dy * dy).sqrt() / (HEAD_PX * 0.44)).min(1.0) as f32;
                let muted = e.modifiers().shift();
                let stroke = match (r, muted) {
                    (r, false) if r < 0.3 => Stroke::Bass,
                    (r, true) if r < 0.45 => Stroke::Heel,
                    (r, false) if r < 0.62 => Stroke::Tip,
                    (_, true) => Stroke::Mute,
                    (r, false) if r < 0.88 => Stroke::Open,
                    _ => Stroke::Slap,
                };
                let mut n = Note::new(stroke).with_velocity(0.82 + 0.05 * s_down.jitter());
                n.position = (r - stroke_radius(stroke)).clamp(-0.5, 0.5);
                s_down.live(drum, n);
            },
            svg { view_box: "0 0 100 100",
                circle { class: "rim", cx: "50", cy: "50", r: "48" }
                circle { class: "skin", cx: "50", cy: "50", r: "44" }
                circle { class: "zone", cx: "50", cy: "50", r: "13" }
                circle { class: "zone", cx: "50", cy: "50", r: "27" }
                circle { class: "zone", cx: "50", cy: "50", r: "38.5" }
                text { class: "zone-label", x: "50", y: "52", "B" }
                text { class: "zone-label", x: "50", y: "33", "T" }
                text { class: "zone-label", x: "50", y: "18", "O" }
                text { class: "zone-label", x: "50", y: "9", "S" }
                if hits > 0 {
                    for k in std::iter::once(hits) {
                        circle {
                            key: "{k}",
                            class: "ripple",
                            cx: "{hx:.1}",
                            cy: "{hy:.1}",
                            r: "{size:.1}",
                        }
                    }
                }
            }
        }
    }
}

/// Offline-rendered outline of the last stroke played on this channel, with
/// its current (possibly just mutated) patch.
#[component]
fn HitScope(drum: usize) -> Element {
    let synth = use_context::<Synth>();
    let engine = synth.audio.engine;
    let state = synth.state;
    let last_stroke = use_memo(move || {
        let s = engine.read();
        if s.hits[drum] == 0 {
            Stroke::Open
        } else {
            s.last[drum].stroke
        }
    });
    let patch = use_memo(move || {
        let mut p = state.read().voices[drum];
        // The scope shows the sound itself, not its placement or room.
        p[Param::Pan.index()] = 0.5;
        p[Param::ReverbMix.index()] = 0.0;
        p
    });
    let outline = use_memo(move || {
        let patch = patch();
        let stroke = last_stroke();
        let decay = map::decay_seconds(patch[Param::Decay.index()]);
        let seconds = (decay * 1.1).clamp(0.2, 1.5);
        let cols = xenopalm_dsp::preview::hit_outline(
            &patch,
            Strike::new(stroke, SCOPE_VELOCITY),
            SCOPE_FS,
            seconds,
            SCOPE_COLUMNS,
        );
        (cols, seconds, stroke)
    });
    let hits = synth.audio.engine.read().hits[drum];
    let (cols, seconds, stroke) = &*outline.read();
    let peak = cols
        .iter()
        .fold(0.0f32, |a, &(lo, hi)| a.max(lo.abs()).max(hi.abs()));
    let scale = 1.0 / peak.max(1e-3);
    let (w, h) = (SCOPE_COLUMNS as f32, 60.0f32);
    let mut d = String::with_capacity(SCOPE_COLUMNS * 24);
    for (i, &(_, hi)) in cols.iter().enumerate() {
        let cmd = if i == 0 { 'M' } else { 'L' };
        d += &format!("{cmd}{i} {:.1}", h / 2.0 - hi * scale * h * 0.46);
    }
    for (i, &(lo, _)) in cols.iter().enumerate().rev() {
        d += &format!("L{i} {:.1}", h / 2.0 - lo * scale * h * 0.46);
    }
    d += "Z";
    let time = if *seconds >= 1.0 {
        format!("{seconds:.1} s")
    } else {
        format!("{:.0} ms", seconds * 1000.0)
    };
    let peak_db = if peak > 1e-5 {
        format!("{:+.0} dB", 20.0 * peak.log10())
    } else {
        "silent".into()
    };
    rsx! {
        div { class: "scope",
            div { class: "sec-title scope-title",
                "{stroke.name()}"
                span { class: "scope-meta", "{time} · peak {peak_db}" }
            }
            svg {
                class: "scope-svg",
                view_box: "0 0 {w} {h}",
                preserve_aspect_ratio: "none",
                line {
                    class: "scope-axis",
                    x1: "0",
                    y1: "{h / 2.0}",
                    x2: "{w}",
                    y2: "{h / 2.0}",
                }
                // Re-keyed on every hit so the flash replays.
                for k in std::iter::once(hits) {
                    path {
                        key: "{k}",
                        class: if k > 0 { "scope-wave played" } else { "scope-wave" },
                        d: "{d}",
                    }
                }
            }
        }
    }
}

/// Animated attractor next to the CHAOS title (from the XK-1); moves at the
/// chaos rate when the modulator has depth.
#[component]
fn ChaosGlyph(drum: usize) -> Element {
    let synth = use_context::<Synth>();
    let path = use_hook(|| attractor_path(30.0, 16.0, 700));
    let state = synth.state.read();
    let patch = &state.voices[drum];
    let active = patch[Param::ChaosDepth.index()] > 0.0;
    // One trip around the drawn path covers roughly ten orbits.
    let period = 10.0 / map::chaos_hz(patch[Param::ChaosRate.index()]);
    rsx! {
        svg {
            class: if active { "chaos-glyph on" } else { "chaos-glyph" },
            view_box: "0 0 30 16",
            path { class: "chaos-trail", d: "{path}" }
            path {
                class: "chaos-comet",
                d: "{path}",
                "pathLength": "100",
                style: "animation-duration: {period:.2}s",
            }
        }
    }
}

/// An SVG path of the Lorenz attractor (x–z projection) scaled to `w`×`h`.
fn attractor_path(w: f64, h: f64, steps: usize) -> String {
    let (mut x, mut y, mut z) = (1.0f64, 1.0f64, 20.0f64);
    let dt = 0.008;
    let mut pts = Vec::with_capacity(steps);
    for i in 0..steps + 300 {
        let (dx, dy, dz) = (10.0 * (y - x), x * (28.0 - z) - y, x * y - 8.0 / 3.0 * z);
        x += dx * dt;
        y += dy * dt;
        z += dz * dt;
        if i >= 300 {
            pts.push((x, z));
        }
    }
    // The attractor spans about x ∈ [-20, 20], z ∈ [5, 48].
    let sx = |x: f64| (x + 21.0) / 42.0 * w;
    let sz = |z: f64| h - (z - 3.0) / 47.0 * h;
    let mut d = String::with_capacity(steps * 12);
    for (i, (x, z)) in pts.iter().enumerate() {
        let cmd = if i == 0 { 'M' } else { 'L' };
        d += &format!("{cmd}{:.1} {:.1}", sx(*x), sz(*z));
    }
    d
}

// --- room --------------------------------------------------------------------------------

#[component]
fn Room() -> Element {
    let g = Target::Global;
    rsx! {
        section { class: "sec room",
            div { class: "room-part",
                div { class: "sec-title", "ROOM" }
                div { class: "row",
                    Knob { target: g(GlobalParam::ReverbDecay) }
                    Knob { target: g(GlobalParam::ReverbTone) }
                    Knob { target: g(GlobalParam::ReverbPredelay) }
                }
            }
            div { class: "room-part",
                div { class: "sec-title", "ROOM EQ" }
                div { class: "row",
                    Knob { target: g(GlobalParam::EqLow) }
                    Knob { target: g(GlobalParam::EqMid) }
                    Knob { target: g(GlobalParam::EqHigh) }
                }
            }
        }
    }
}

// --- controls ------------------------------------------------------------------------------

fn polar(cx: f64, cy: f64, r: f64, deg: f64) -> (f64, f64) {
    let a = deg.to_radians();
    (cx + r * a.sin(), cy - r * a.cos())
}

fn arc_path(cx: f64, cy: f64, r: f64, from_deg: f64, to_deg: f64) -> String {
    let (x0, y0) = polar(cx, cy, r, from_deg);
    let (x1, y1) = polar(cx, cy, r, to_deg);
    let large = if (to_deg - from_deg).abs() > 180.0 {
        1
    } else {
        0
    };
    format!("M {x0:.2} {y0:.2} A {r} {r} 0 {large} 1 {x1:.2} {y1:.2}")
}

fn begin_drag(synth: &Synth, target: Target, e: &PointerEvent, span_px: f64) {
    e.prevent_default();
    synth.audio.user_gesture();
    let mut touched = synth.touched;
    touched.set(Some(target));
    let mut drag = synth.drag;
    drag.set(Some(Drag {
        target,
        start_y: e.client_coordinates().y,
        start_value: synth.value(target),
        span_px,
        fine: e.modifiers().shift(),
    }));
}

fn nudge(synth: &Synth, target: Target, e: &WheelEvent) {
    e.prevent_default();
    let dy = e.delta().strip_units().y;
    if dy == 0.0 {
        return;
    }
    let step = match target_kind(target) {
        ParamKind::Int { min, max } => 1.0 / (max - min) as f32,
        ParamKind::Choice(names) => 1.0 / (names.len() - 1).max(1) as f32,
        ParamKind::Continuous if e.modifiers().shift() => 0.002,
        ParamKind::Continuous => 0.02,
    };
    let v = synth.value(target) - (dy.signum() as f32) * step;
    synth.set(target, v);
    synth.save();
}

#[component]
fn Knob(target: Target, #[props(default = "lg")] size: &'static str) -> Element {
    let synth = use_context::<Synth>();
    let value = synth.value(target);
    let label = synth.label(target);
    let name = synth.name(target);
    let active = matches!(*synth.drag.read(), Some(d) if d.target == target);
    let is_macro = matches!(target, Target::Param(_, p) if p.macro_slot().is_some());
    let angle = -135.0 + 270.0 * value as f64;
    let (px, py) = polar(30.0, 30.0, 15.0, angle);
    let track = arc_path(30.0, 30.0, 26.0, -135.0, 135.0);
    // Bipolar controls light the arc from the centre.
    let (a0, a1) = if target.is_bipolar() {
        (angle.min(0.0), angle.max(0.0))
    } else {
        (-135.0, angle)
    };
    let lit = if (a1 - a0).abs() > 0.5 {
        arc_path(30.0, 30.0, 26.0, a0, a1)
    } else {
        String::new()
    };
    let text = synth.display(target, value);
    let s_down = synth.clone();
    let s_dbl = synth.clone();
    let s_wheel = synth.clone();
    rsx! {
        div {
            class: "knob knob-{size}",
            class: if active { "active" },
            class: if is_macro { "macro" },
            div { class: "ctl-label", "{label}" }
            svg {
                class: "knob-svg",
                view_box: "0 0 60 60",
                role: "slider",
                "aria-label": "{name}",
                "aria-valuetext": "{text}",
                onpointerdown: move |e| begin_drag(&s_down, target, &e, KNOB_SPAN_PX),
                ondoubleclick: move |_| s_dbl.reset(target),
                onwheel: move |e| nudge(&s_wheel, target, &e),
                title { "{name}: {text}" }
                path { class: "knob-track", d: "{track}" }
                if !lit.is_empty() {
                    path { class: "knob-lit", d: "{lit}" }
                }
                circle { class: "knob-skirt", cx: "30", cy: "30", r: "21" }
                circle { class: "knob-body", cx: "30", cy: "30", r: "17" }
                line { class: "knob-pointer", x1: "30", y1: "30", x2: "{px:.2}", y2: "{py:.2}" }
            }
            div { class: "ctl-value", "{text}" }
        }
    }
}

#[component]
fn Fader(target: Target) -> Element {
    let synth = use_context::<Synth>();
    let value = synth.value(target);
    let text = synth.display(target, value);
    let label = synth.label(target);
    let name = synth.name(target);
    let active = matches!(*synth.drag.read(), Some(d) if d.target == target);
    let travel = FADER_TRACK_PX - FADER_CAP_PX;
    let cap_top = (1.0 - value as f64) * travel;
    let s_down = synth.clone();
    let s_dbl = synth.clone();
    let s_wheel = synth.clone();
    rsx! {
        div { class: "fader", class: if active { "active" },
            div { class: "ctl-label", "{label}" }
            div {
                class: "fader-body",
                role: "slider",
                title: "{name}: {text}",
                "aria-label": "{name}",
                "aria-valuetext": "{text}",
                onpointerdown: move |e| begin_drag(&s_down, target, &e, travel),
                ondoubleclick: move |_| s_dbl.reset(target),
                onwheel: move |e| nudge(&s_wheel, target, &e),
                div { class: "fader-leds",
                    for i in (0..FADER_LEDS).rev() {
                        div {
                            key: "{i}",
                            class: if value * FADER_LEDS as f32 >= i as f32 + 0.5 { "led on" } else { "led" },
                        }
                    }
                }
                div { class: "fader-track",
                    div { class: "fader-slot" }
                    div { class: "fader-cap", style: "top: {cap_top:.1}px" }
                }
            }
            div { class: "ctl-value", "{text}" }
        }
    }
}

/// Segmented switch for a choice parameter.
#[component]
fn Switch(target: Target, names: Vec<&'static str>, #[props(default)] vertical: bool) -> Element {
    let synth = use_context::<Synth>();
    let n = names.len();
    let current = (synth.value(target) * (n - 1) as f32).round() as usize;
    rsx! {
        div { class: "switch",
            div { class: "ctl-label", "{synth.label(target)}" }
            div {
                class: if vertical { "segments vertical" } else { "segments" },
                title: "{synth.name(target)}",
                for (i, name) in names.into_iter().enumerate() {
                    Segment { key: "{i}", target, index: i, count: n, name, selected: i == current }
                }
            }
        }
    }
}

#[component]
fn Segment(
    target: Target,
    index: usize,
    count: usize,
    name: &'static str,
    selected: bool,
) -> Element {
    let synth = use_context::<Synth>();
    rsx! {
        button {
            class: if selected { "segment sel" } else { "segment" },
            onclick: move |_| {
                synth.set(target, index as f32 / (count - 1).max(1) as f32);
                synth.save();
            },
            "{name}"
        }
    }
}
