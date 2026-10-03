//! C ABI over the engine for the AudioWorkletProcessor (`worklet.js`).
//!
//! The module imports nothing, so the worklet can instantiate it with an empty
//! import object. Each worklet node owns one engine handle.

use xenopalm_dsp::{Engine, GlobalParam, Note, Param, Status};

/// Frames per render call; matches the WebAudio render quantum.
pub const BLOCK: usize = 128;

pub struct Worklet {
    engine: Engine,
    left: [f32; BLOCK],
    right: [f32; BLOCK],
    status: [f32; Status::WIRE_LEN],
}

#[unsafe(no_mangle)]
pub extern "C" fn xp_new(sample_rate: f32) -> *mut Worklet {
    Box::into_raw(Box::new(Worklet {
        engine: Engine::new(sample_rate),
        left: [0.0; BLOCK],
        right: [0.0; BLOCK],
        status: Status::default().to_wire(),
    }))
}

/// # Safety
/// `w` must come from [`xp_new`] and not have been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xp_free(w: *mut Worklet) {
    if !w.is_null() {
        drop(unsafe { Box::from_raw(w) });
    }
}

/// # Safety
/// `w` must come from [`xp_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xp_set_param(w: *mut Worklet, drum: u32, id: u32, value: f32) {
    if let Some(p) = Param::from_id(id) {
        unsafe { &mut *w }.engine.set_param(drum as usize, p, value);
    }
}

/// # Safety
/// `w` must come from [`xp_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xp_set_global(w: *mut Worklet, id: u32, value: f32) {
    if let Some(p) = GlobalParam::from_id(id) {
        unsafe { &mut *w }.engine.set_global(p, value);
    }
}

/// Write one pattern cell; a negative `stroke` clears it.
///
/// # Safety
/// `w` must come from [`xp_new`].
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn xp_set_note(
    w: *mut Worklet,
    step: u32,
    drum: u32,
    stroke: f32,
    velocity: f32,
    position: f32,
    ornament: f32,
    chance: f32,
) {
    let note = Note::from_wire([stroke, velocity, position, ornament, chance]);
    unsafe { &mut *w }
        .engine
        .set_note(step as usize, drum as usize, note);
}

/// # Safety
/// `w` must come from [`xp_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xp_play(w: *mut Worklet, on: u32) {
    unsafe { &mut *w }.engine.play(on != 0);
}

/// Play a note now, with its ornament.
///
/// # Safety
/// `w` must come from [`xp_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xp_live(
    w: *mut Worklet,
    drum: u32,
    stroke: f32,
    velocity: f32,
    position: f32,
    ornament: f32,
) {
    if let Some(note) = Note::from_wire([stroke, velocity, position, ornament, 1.0]) {
        unsafe { &mut *w }.engine.live(drum as usize, note);
    }
}

/// Jump smoothed parameters to their targets (after the initial sync).
///
/// # Safety
/// `w` must come from [`xp_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xp_snap(w: *mut Worklet) {
    unsafe { &mut *w }.engine.snap_params();
}

/// Render `frames` (≤ [`BLOCK`]) into the buffers returned by [`xp_left`]/[`xp_right`].
///
/// # Safety
/// `w` must come from [`xp_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xp_render(w: *mut Worklet, frames: u32) {
    let w = unsafe { &mut *w };
    let n = (frames as usize).min(BLOCK);
    w.engine.render(&mut w.left[..n], &mut w.right[..n]);
    w.status = w.engine.status().to_wire();
}

/// # Safety
/// `w` must come from [`xp_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xp_left(w: *mut Worklet) -> *const f32 {
    unsafe { (*w).left.as_ptr() }
}

/// # Safety
/// `w` must come from [`xp_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xp_right(w: *mut Worklet) -> *const f32 {
    unsafe { (*w).right.as_ptr() }
}

/// The engine status after the last render, [`Status::WIRE_LEN`] floats.
///
/// # Safety
/// `w` must come from [`xp_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn xp_status(w: *mut Worklet) -> *const f32 {
    unsafe { (*w).status.as_ptr() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abi_plays_a_pattern() {
        unsafe {
            let w = xp_new(48_000.0);
            xp_set_note(w, 0, 0, 4.0, 0.9, 0.0, 0.0, 1.0);
            xp_set_global(w, GlobalParam::Tempo.id(), 120.0);
            xp_play(w, 1);
            let mut energy = 0.0;
            for _ in 0..40 {
                xp_render(w, 128);
                let l = std::slice::from_raw_parts(xp_left(w), 128);
                energy += l.iter().map(|v| v * v).sum::<f32>();
            }
            assert!(energy > 0.01);
            let status = std::slice::from_raw_parts(xp_status(w), Status::WIRE_LEN);
            let s = Status::from_wire(status).unwrap();
            assert!(s.playing && s.hits[0] == 1);
            xp_free(w);
        }
    }
}
