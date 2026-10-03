//! Engine speed as a real-time factor, with both voices on the same engine
//! playing a dense, fully improvised pattern into the room.
//! `cargo run --release -p xenopalm-dsp --example bench`

use std::time::Instant;

use xenopalm_dsp::params::{ENGINE_NAMES, GlobalParam};
use xenopalm_dsp::pattern::generate;
use xenopalm_dsp::util::Noise;
use xenopalm_dsp::{DRUMS, Engine, Param};

fn main() {
    let fs = 48_000.0;
    let seconds = 10.0;
    for (engine, name) in ENGINE_NAMES.iter().enumerate() {
        let mut e = Engine::new(fs);
        for d in 0..DRUMS {
            e.set_param(d, Param::Engine, engine as f32);
            e.set_param(d, Param::Decay, 0.7);
            e.set_param(d, Param::ReverbMix, 0.5);
        }
        e.set_global(GlobalParam::Tempo, 140.0);
        e.set_global(GlobalParam::Improv, 1.0);
        e.set_global(GlobalParam::Length, 32.0);
        e.set_pattern(&generate(1, 1.0, 32, 0, &mut Noise::new(1)));
        e.play(true);
        let mut l = [0.0f32; 128];
        let mut r = [0.0f32; 128];
        let blocks = (seconds * fs / 128.0) as usize;
        let t = Instant::now();
        for _ in 0..blocks {
            e.render(&mut l, &mut r);
        }
        println!(
            "{name:8} {:6.1}x real time",
            seconds / t.elapsed().as_secs_f32()
        );
    }
    // MUTATE at RND on both channels: a fresh patch on every note.
    let mut e = Engine::new(fs);
    for d in 0..DRUMS {
        e.set_param(d, Param::Mutate, 1.0);
    }
    e.set_global(GlobalParam::Tempo, 140.0);
    e.set_global(GlobalParam::Length, 32.0);
    e.set_pattern(&generate(1, 1.0, 32, 0, &mut Noise::new(1)));
    e.play(true);
    let (mut l, mut r) = ([0.0f32; 128], [0.0f32; 128]);
    let blocks = (seconds * fs / 128.0) as usize;
    let t = Instant::now();
    for _ in 0..blocks {
        e.render(&mut l, &mut r);
    }
    println!(
        "MUTATE   {:6.1}x real time",
        seconds / t.elapsed().as_secs_f32()
    );
}
