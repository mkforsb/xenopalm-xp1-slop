//! Xenopalm DSP: the Xenokussion XK-1's eight experimental percussion
//! engines (modal, waveguide string, comb network, nonlinear FDN, grains,
//! random spectra, chaotic FM and a Lorenz attractor), played by Palmkussion
//! PK-1's simulated hand drummer. Every channel has the XK-1's MUTATE, which
//! now acts on every note the player plays.
//!
//! The crate has no dependencies and no platform code so it can be compiled to
//! a standalone wasm module for an AudioWorklet as well as used natively.

// The engines loop over several parallel per-mode/per-line arrays at once;
// index loops read better there than zipped iterators.
#![allow(clippy::needless_range_loop)]

pub mod engine;
pub mod engines;
pub mod exciter;
pub mod modulation;
pub mod mutate;
pub mod params;
pub mod pattern;
pub mod performer;
pub mod presets;
pub mod preview;
pub mod reverb;
pub mod stroke;
pub mod util;
pub mod voice;

pub use engine::{Engine, Status};
pub use params::{GlobalParam, Param, VoicePatch};
pub use pattern::{DRUMS, Note, Pattern};
pub use stroke::{Ornament, Strike, Stroke};
