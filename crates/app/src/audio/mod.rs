//! Platform audio backends behind one small handle.
//!
//! * `web`: an AudioWorklet running the DSP compiled to a standalone wasm module.
//! * `desktop`: a PulseAudio playback stream fed from a dedicated thread.
//!
//! The UI only ever talks to [`AudioHandle`]; it never blocks and never touches
//! the engine directly. The engine reports back where the sequencer is and
//! what it last hit ([`Status`]) through a channel that lands in
//! [`AudioHandle::engine`].

// Without a platform feature only the null backend exists.
#![cfg_attr(not(any(feature = "web", feature = "desktop")), allow(dead_code))]

use dioxus::prelude::*;
use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use futures_util::StreamExt;
use xenopalm_dsp::params::{ALL_GLOBAL_PARAMS, ALL_PARAMS, GlobalPatch, MAX_STEPS, VoicePatch};
use xenopalm_dsp::{DRUMS, GlobalParam, Note, Param, Pattern, Status};

#[cfg(feature = "desktop")]
mod pulse;
#[cfg(all(feature = "web", not(feature = "desktop")))]
mod web;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    Param {
        drum: usize,
        param: Param,
        value: f32,
    },
    Global {
        param: GlobalParam,
        value: f32,
    },
    /// Write one pattern cell (`None` clears it).
    Note {
        step: usize,
        drum: usize,
        note: Option<Note>,
    },
    /// Play a note now.
    Live {
        drum: usize,
        note: Note,
    },
    Play(bool),
}

#[derive(Clone, Debug, PartialEq)]
pub enum AudioStatus {
    /// Web only: waiting for a user gesture before the AudioContext may start.
    #[cfg_attr(feature = "desktop", allow(dead_code))]
    NeedsGesture,
    Starting,
    Running {
        sample_rate: u32,
        detail: String,
    },
    Failed(String),
}

/// Everything sent so far, so a backend that comes up late (the web one,
/// after a user gesture) can be brought in sync in one go.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub voices: [VoicePatch; DRUMS],
    pub globals: GlobalPatch,
    pub pattern: Pattern,
}

impl Snapshot {
    #[cfg_attr(feature = "desktop", allow(dead_code))]
    pub fn apply(&mut self, cmd: Command) {
        match cmd {
            Command::Param { drum, param, value } => {
                if let Some(d) = self.voices.get_mut(drum) {
                    d[param.index()] = value;
                }
            }
            Command::Global { param, value } => self.globals[param.index()] = value,
            Command::Note { step, drum, note } => self.pattern.set(step, drum, note),
            Command::Live { .. } | Command::Play(_) => {}
        }
    }

    /// The commands that recreate this state in a fresh engine.
    pub fn commands(&self) -> impl Iterator<Item = Command> + '_ {
        let voices = self.voices.iter().enumerate().flat_map(|(drum, patch)| {
            ALL_PARAMS.iter().map(move |&param| Command::Param {
                drum,
                param,
                value: patch[param.index()],
            })
        });
        let globals = ALL_GLOBAL_PARAMS.iter().map(|&param| Command::Global {
            param,
            value: self.globals[param.index()],
        });
        let notes = (0..MAX_STEPS).flat_map(move |step| {
            (0..DRUMS).filter_map(move |drum| {
                self.pattern.get(step, drum).map(|n| Command::Note {
                    step,
                    drum,
                    note: Some(n),
                })
            })
        });
        voices.chain(globals).chain(notes)
    }
}

#[cfg(feature = "desktop")]
type Backend = pulse::PulseBackend;
#[cfg(all(feature = "web", not(feature = "desktop")))]
type Backend = web::WebBackend;
#[cfg(not(any(feature = "web", feature = "desktop")))]
type Backend = NullBackend;

/// Cheap to clone; shared through the Dioxus context.
#[derive(Clone)]
pub struct AudioHandle {
    backend: std::rc::Rc<Backend>,
    pub status: Signal<AudioStatus>,
    /// Latest engine status: playhead and hits.
    pub engine: Signal<Status>,
}

impl AudioHandle {
    /// Must be called inside the Dioxus runtime (e.g. from `use_hook`).
    pub fn new(initial: Snapshot) -> Self {
        let status = Signal::new(AudioStatus::Starting);
        let engine = Signal::new(Status::default());
        let (tx, rx) = unbounded::<Status>();
        spawn(forward(rx, engine));
        let backend = std::rc::Rc::new(Backend::new(status, initial, tx));
        Self {
            backend,
            status,
            engine,
        }
    }

    pub fn send(&self, cmd: Command) {
        self.backend.send(cmd);
    }

    /// Call from user-gesture handlers; lets the web backend start or resume.
    pub fn user_gesture(&self) {
        self.backend.user_gesture();
    }
}

/// Copy engine statuses from the backend into the signal, on the UI thread.
async fn forward(mut rx: UnboundedReceiver<Status>, mut engine: Signal<Status>) {
    while let Some(s) = rx.next().await {
        // Several may queue up while the UI is busy; only the newest matters.
        let mut latest = s;
        while let Ok(s) = rx.try_recv() {
            latest = s;
        }
        engine.set(latest);
    }
}

pub type StatusSender = UnboundedSender<Status>;

/// Used when building without a platform feature (e.g. `cargo check`).
#[cfg(not(any(feature = "web", feature = "desktop")))]
pub struct NullBackend;

#[cfg(not(any(feature = "web", feature = "desktop")))]
impl NullBackend {
    fn new(mut status: Signal<AudioStatus>, _initial: Snapshot, _tx: StatusSender) -> Self {
        status.set(AudioStatus::Failed("built without an audio backend".into()));
        Self
    }
    fn send(&self, _cmd: Command) {}
    fn user_gesture(&self) {}
}
