//! GONG: a four-line feedback delay network with an energy-scattering
//! nonlinearity, for gongs, cymbals, sheet metal and bowls.
//!
//! ```text
//!  force ─►(+)─► line i ─► Hadamard mix ─► ×g_i ─► damping ─► nonlinear allpass ─┐
//!           ▲                                                                     │
//!           └─────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! * SIZE spreads the four line lengths from almost equal (a dense, beating,
//!   plate-like cluster) to wide ratios (a sparse, bell-like cluster).
//! * CRASH scatters energy upwards. Each loop contains a first-order allpass
//!   in normalized-lattice form, `[y, s'] = [[a, c], [c, −a]]·[x, s]` with
//!   `c = √(1 − a²)`. That matrix is orthogonal for any `a`, so the section is
//!   lossless even when `a` changes every sample. Making `a` follow the
//!   signal's own amplitude gives an amplitude-dependent, energy-preserving
//!   nonlinearity. Loud hits shimmer and "bloom" into high partials the way a
//!   struck gong or crash cymbal does, and the network can never blow up.
//! * DAMP lowpasses inside the loops so the highs die first.
//!
//! Every line's gain comes from its length (after Jot), so all modes share
//! the DECAY time before damping.

use super::{Ctx, MIN_DELAY_HZ};
use crate::exciter::Excitation;
use crate::util::{DcBlocker, DelayLine, flush};

const LINES: usize = 4;
/// Quarter-octave-spaced base ratios, raised to a SIZE-dependent power.
const BASE_RATIOS: [f32; LINES] = [1.0, 1.189_2, core::f32::consts::SQRT_2, 1.681_8];
const IN_SIGNS: [f32; LINES] = [1.0, -1.0, 1.0, 1.0];
const OUT_SIGNS: [f32; LINES] = [1.0, 1.0, -1.0, 1.0];
/// Level trim.
const INPUT_GAIN: f32 = 0.9;
/// In-loop DC blocker corner (see COMB).
const DC_HZ: f32 = 8.0;

#[derive(Clone, Debug)]
pub struct Gong {
    fs: f32,
    lines: [DelayLine; LINES],
    delay: [f32; LINES],
    gain: [f32; LINES],
    lp: [f32; LINES],
    nl: [f32; LINES],
    damp_k: f32,
    crash: f32,
    period: f32,
    dc: [DcBlocker; LINES],
    dc_r: f32,
}

impl Gong {
    pub fn new(fs: f32) -> Self {
        let max = (fs / MIN_DELAY_HZ) as usize + 8;
        Self {
            fs,
            lines: core::array::from_fn(|_| DelayLine::new(max)),
            delay: [100.0; LINES],
            gain: [0.0; LINES],
            lp: [0.0; LINES],
            nl: [0.0; LINES],
            damp_k: 1.0,
            crash: 0.0,
            period: 100.0,
            dc: Default::default(),
            dc_r: 1.0 - core::f32::consts::TAU * DC_HZ / fs,
        }
    }

    pub fn reset(&mut self) {
        self.lines.iter_mut().for_each(DelayLine::clear);
        self.lp = [0.0; LINES];
        self.nl = [0.0; LINES];
        self.dc.iter_mut().for_each(DcBlocker::reset);
    }

    pub fn control(&mut self, c: &Ctx) {
        let fs = self.fs;
        let period = fs / c.hz.clamp(MIN_DELAY_HZ, 0.4 * fs);
        let power = 0.15 + 2.4 * c.x;
        let t60 = c.decay_s.max(1e-3);
        for i in 0..LINES {
            self.delay[i] = (period / BASE_RATIOS[i].powf(power)).max(2.0);
            self.gain[i] = (-6.907_755 * self.delay[i] / (t60 * fs)).exp();
        }
        self.crash = 0.97 * c.y;
        self.damp_k = 1.0 - 0.92 * c.z;
        self.period = period;
    }

    #[inline]
    pub fn tick(&mut self, e: &Excitation) -> f32 {
        let x = e.displacement(self.period) * INPUT_GAIN;
        let o: [f32; LINES] = core::array::from_fn(|i| self.lines[i].read(self.delay[i]));
        // Normalized 4×4 Hadamard: orthogonal, so it conserves energy.
        let (s01, d01, s23, d23) = (o[0] + o[1], o[0] - o[1], o[2] + o[3], o[2] - o[3]);
        let h = [
            0.5 * (s01 + s23),
            0.5 * (d01 + d23),
            0.5 * (s01 - s23),
            0.5 * (d01 - d23),
        ];
        let mut out = 0.0;
        for i in 0..LINES {
            let v = self.dc[i].process(h[i] * self.gain[i], self.dc_r);
            self.lp[i] = flush(self.lp[i] + self.damp_k * (v - self.lp[i]));
            let v = self.lp[i];
            let a = self.crash * (3.0 * v.abs()).min(1.0);
            let c = (1.0 - a * a).sqrt();
            let s = self.nl[i];
            let y = a * v + c * s;
            self.nl[i] = flush(c * v - a * s);
            self.lines[i].write(y + IN_SIGNS[i] * x);
            out += OUT_SIGNS[i] * o[i];
        }
        out * 0.5
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strike(n: usize, height: f32) -> Excitation {
        Excitation {
            mallet: if n < 4 { height } else { 0.0 },
            pulse_len: 4.0,
            ..Default::default()
        }
    }

    fn run(c: &Ctx, frames: usize, hit: f32) -> Vec<f32> {
        let mut g = Gong::new(c.fs);
        (0..frames)
            .map(|n| {
                if n % 16 == 0 {
                    g.control(c);
                }
                g.tick(&strike(n, hit))
            })
            .collect()
    }

    #[test]
    fn lossless_core_decays_only_through_the_loop_gain() {
        // With the loop gain and damping neutral, energy must be conserved
        // even at full CRASH: check it does not grow.
        let fs = 48_000.0;
        let mut g = Gong::new(fs);
        let mut c = Ctx::new(fs);
        c.hz = 200.0;
        c.y = 1.0;
        c.z = 0.0;
        c.decay_s = 1e9;
        g.control(&c);
        let mut peak = 0.0f32;
        let mut late = 0.0f32;
        for n in 0..fs as usize * 4 {
            let o = g.tick(&strike(n, 0.5));
            if n < 4_800 {
                peak = peak.max(o.abs());
            } else {
                late = late.max(o.abs());
            }
        }
        assert!(late < peak * 1.5, "energy grew: {peak} -> {late}");
    }

    #[test]
    fn decays_at_every_setting() {
        let fs = 96_000.0;
        for x in [0.0, 1.0] {
            for y in [0.0, 1.0] {
                for z in [0.0, 1.0] {
                    let mut c = Ctx::new(fs);
                    c.hz = 120.0;
                    c.x = x;
                    c.y = y;
                    c.z = z;
                    c.decay_s = 0.8;
                    let out = run(&c, 2 * 96_000, 0.25);
                    assert!(out.iter().all(|v| v.is_finite()));
                    let early = out[..9_600].iter().fold(0.0f32, |a, &b| a.max(b.abs()));
                    let late = out[180_000..].iter().fold(0.0f32, |a, &b| a.max(b.abs()));
                    assert!(
                        early > 1e-3 && late < early * 1e-2,
                        "{x} {y} {z}: {early} {late}"
                    );
                }
            }
        }
    }
}
