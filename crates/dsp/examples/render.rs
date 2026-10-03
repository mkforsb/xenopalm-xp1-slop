//! Render a few preset pairings and RND ALL rolls playing to WAV files.
//! `cargo run --release -p xenopalm-dsp --example render -- [out_dir] [seconds]`

use xenopalm_dsp::params::{ALL_GLOBAL_PARAMS, GlobalParam, default_global_patch};
use xenopalm_dsp::pattern::{STYLES, generate};
use xenopalm_dsp::presets::PRESETS;
use xenopalm_dsp::util::Noise;
use xenopalm_dsp::{Engine, Param};

const FS: u32 = 48_000;

fn write_wav(path: &std::path::Path, l: &[f32], r: &[f32]) -> std::io::Result<()> {
    let mut b = Vec::with_capacity(44 + l.len() * 4);
    let data_len = (l.len() * 4) as u32;
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&FS.to_le_bytes());
    b.extend_from_slice(&(FS * 4).to_le_bytes());
    b.extend_from_slice(&4u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for (a, c) in l.iter().zip(r) {
        for s in [a, c] {
            b.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32_767.0) as i16).to_le_bytes());
        }
    }
    std::fs::write(path, b)
}

fn slug(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn preset(name: &str) -> xenopalm_dsp::VoicePatch {
    PRESETS.iter().find(|p| p.name == name).unwrap().patch()
}

fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(args.next().unwrap_or_else(|| "renders".into()));
    let seconds: f32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(10.0);
    std::fs::create_dir_all(&dir)?;
    let mut rng = Noise::new(2026);
    let frames = (seconds * FS as f32) as usize;

    // (name, HI preset, LO preset, style, HI mutate, LO mutate)
    let pairings = [
        (
            "membrane-tumbao",
            "Membrane Tom",
            "Slack Floor Tom",
            1,
            0.0,
            0.0,
        ),
        (
            "tabla-martillo-mutating",
            "Tabla Boing",
            "Hollow Pipe",
            3,
            0.15,
            0.0,
        ),
        (
            "metal-djembe",
            "Impossible Metal",
            "Chaos Kick",
            5,
            0.0,
            0.1,
        ),
        (
            "rain-clave-rnd",
            "Rain on Tin",
            "Glass Marimba",
            7,
            1.0,
            0.0,
        ),
        ("lorenz-6-8", "Strange Tom", "Butterfly Zap", 6, 0.3, 0.3),
    ];
    for (name, hi, lo, style, mh, ml) in pairings {
        let mut e = Engine::new(FS as f32);
        for (d, (p, m)) in [(hi, mh), (lo, ml)].into_iter().enumerate() {
            let mut patch = preset(p);
            patch[Param::Mutate.index()] = m;
            e.load_patch(d, &patch);
        }
        let s = &STYLES[style];
        e.set_global(GlobalParam::Tempo, 0.5 * (s.tempo.0 + s.tempo.1));
        e.set_global(GlobalParam::Grid, s.grid as f32);
        let len = if s.grid == 1 { 24 } else { 32 };
        e.set_global(GlobalParam::Length, len as f32);
        e.set_pattern(&generate(style, 0.45, len, s.grid, &mut rng));
        e.snap_params();
        e.play(true);
        let (l, r) = e.render_vec(frames);
        let path = dir.join(format!("{name}.wav"));
        write_wav(&path, &l, &r)?;
        println!("{}", path.display());
    }

    for i in 0..4 {
        let voices = [xenopalm_dsp::params::default_patch(); 2];
        let kit = xenopalm_dsp::mutate::random_kit(&voices, &default_global_patch(), &mut rng);
        let mut e = Engine::new(FS as f32);
        for (d, patch) in kit.voices.iter().enumerate() {
            e.load_patch(d, patch);
        }
        for p in ALL_GLOBAL_PARAMS {
            e.set_global(p, kit.globals[p.index()]);
        }
        let len = kit.globals[GlobalParam::Length.index()] as usize;
        let grid = kit.globals[GlobalParam::Grid.index()] as u32;
        e.set_pattern(&generate(kit.style, kit.busy, len, grid, &mut rng));
        e.snap_params();
        e.play(true);
        let (l, r) = e.render_vec(frames);
        let path = dir.join(format!("rnd-all-{i}-{}.wav", slug(STYLES[kit.style].name)));
        write_wav(&path, &l, &r)?;
        println!("{}", path.display());
    }
    Ok(())
}
