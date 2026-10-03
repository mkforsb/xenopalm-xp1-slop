//! STRING: a digital waveguide (extended Karplus–Strong) string.
//!
//! ```text
//!  force ─► pluck-position comb ─►(+)─► delay ─┬─► loss LPF ─► ×g ─► 4 × dispersion allpass ─┐
//!                                  ▲          └─► out                                         │
//!                                  └──────────────────────────────────────────────────────────┘
//! ```
//!
//! * STIFF sets the four first-order allpasses' coefficient. They delay low
//!   frequencies more than high ones, so upper partials run sharp like a
//!   stiff string, piano wire or bar.
//! * POS is the pluck position: exciting through `x[n] − x[n − p·P]` notches
//!   out the harmonics that have a node there.
//! * TENSION models tension modulation. The string's amplitude stretches it,
//!   raising the pitch, so a hard hit starts sharp and glides down as it
//!   decays. This is the "boing" of a tabla, a slack floor tom or a spring.
//!
//! The loop delay is corrected for the phase delay of the loss filter and the
//! allpasses at the fundamental, and the loop gain for the loss filter's
//! magnitude there, so PITCH and DECAY stay accurate at any STIFF setting.

use super::{Ctx, MIN_DELAY_HZ};
use crate::exciter::Excitation;
use crate::params::map;
use crate::util::{
    DelayLine, flush, magnitude_first_order, one_pole_coeff, phase_delay_first_order,
};

const ALLPASSES: usize = 4;
/// Loss filter pole: how much faster the upper harmonics decay.
const LOSS_POLE: f32 = 0.18;
/// Release of the amplitude (peak) follower driving tension modulation.
const ENERGY_RELEASE_S: f32 = 0.03;
/// Level trim.
const INPUT_GAIN: f32 = 0.95;

#[derive(Clone, Debug)]
pub struct KString {
    fs: f32,
    line: DelayLine,
    pos_line: DelayLine,
    lp: f32,
    ap: [f32; ALLPASSES],
    a: f32,
    loop_gain: f32,
    read_delay: f32,
    pos_delay: f32,
    period: f32,
    energy: f32,
    k_energy: f32,
    tension_oct: f32,
}

impl KString {
    pub fn new(fs: f32) -> Self {
        let max = (fs / MIN_DELAY_HZ) as usize + 8;
        Self {
            fs,
            line: DelayLine::new(max),
            pos_line: DelayLine::new(max),
            lp: 0.0,
            ap: [0.0; ALLPASSES],
            a: 0.0,
            loop_gain: 0.0,
            read_delay: 100.0,
            pos_delay: 10.0,
            period: 100.0,
            energy: 0.0,
            k_energy: one_pole_coeff(ENERGY_RELEASE_S, fs),
            tension_oct: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.line.clear();
        self.pos_line.clear();
        self.lp = 0.0;
        self.ap = [0.0; ALLPASSES];
        self.energy = 0.0;
    }

    pub fn control(&mut self, c: &Ctx) {
        let fs = self.fs;
        let bend = map::tension_octaves(c.z) * (self.energy * 1.5).min(1.0);
        self.tension_oct = bend;
        let hz = (c.hz * bend.exp2()).clamp(MIN_DELAY_HZ, 0.4 * fs);
        let period = fs / hz;
        let w0 = core::f32::consts::TAU * hz / fs;

        // Dispersion: keep the allpasses' DC delay within a fifth of the loop.
        let budget = 0.2 * period / ALLPASSES as f32;
        let a_min = (1.0 - budget) / (1.0 + budget);
        self.a = (-0.92 * c.x.sqrt()).max(a_min).min(0.0);

        let b = LOSS_POLE;
        let tau_lp = phase_delay_first_order(1.0 - b, 0.0, -b, w0);
        let tau_ap = phase_delay_first_order(self.a, 1.0, self.a, w0);
        self.read_delay = (period - tau_lp - ALLPASSES as f32 * tau_ap).max(2.0);
        let lp_mag = magnitude_first_order(1.0 - b, 0.0, -b, w0);
        let per_trip = (-6.907_755 / (c.decay_s.max(1e-3) * hz)).exp();
        self.loop_gain = (per_trip / lp_mag).min(0.9995);

        self.pos_delay = (map::strike_position(c.y) * period).max(2.0);
        self.period = period;
    }

    #[inline]
    pub fn tick(&mut self, e: &Excitation) -> f32 {
        let x = e.displacement(self.period) * INPUT_GAIN;
        let excite = x - self.pos_line.read(self.pos_delay);
        self.pos_line.write(x);

        let y = self.line.read(self.read_delay);
        self.lp = flush((1.0 - LOSS_POLE) * y + LOSS_POLE * self.lp);
        let mut v = self.lp * self.loop_gain;
        for s in self.ap.iter_mut() {
            let out = self.a * v + *s;
            *s = flush(v - self.a * out);
            v = out;
        }
        self.line.write(v + excite);
        let a = y.abs();
        self.energy = if a > self.energy {
            a
        } else {
            self.energy + (a - self.energy) * self.k_energy
        };
        y
    }

    /// Current tension-modulation pitch offset, in octaves.
    pub fn tension_bend(&self) -> f32 {
        self.tension_oct
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unit-area rectangular mallet `len` samples long.
    fn hit(n: usize, len: usize) -> Excitation {
        Excitation {
            mallet: if n < len { 1.0 / len as f32 } else { 0.0 },
            pulse_len: len as f32,
            ..Default::default()
        }
    }

    fn pluck(c: &Ctx, frames: usize) -> Vec<f32> {
        let mut s = KString::new(c.fs);
        s.control(c);
        (0..frames)
            .map(|n| {
                if n % 16 == 0 {
                    s.control(c);
                }
                s.tick(&hit(n, 4))
            })
            .collect()
    }

    /// Pitch from the autocorrelation peak within ±40% of the expected period.
    fn autocorr_hz(x: &[f32], fs: f32, expect_hz: f32) -> f32 {
        let p = fs / expect_hz;
        let n = x.len() - (1.5 * p) as usize - 2;
        let r = |lag: usize| (0..n).map(|i| x[i] * x[i + lag]).sum::<f32>();
        let (lo, hi) = ((0.6 * p) as usize, (1.4 * p) as usize);
        let best = (lo..=hi).max_by(|&a, &b| r(a).total_cmp(&r(b))).unwrap();
        // Parabolic interpolation around the peak.
        let (a, b, c) = (r(best - 1), r(best), r(best + 1));
        let off = 0.5 * (a - c) / (a - 2.0 * b + c);
        fs / (best as f32 + off)
    }

    #[test]
    fn pitch_is_accurate_with_and_without_stiffness() {
        let fs = 96_000.0;
        for stiff in [0.0f32, 0.5, 1.0] {
            for hz in [110.0f32, 440.0] {
                let mut c = Ctx::new(fs);
                c.hz = hz;
                c.x = stiff;
                c.y = 1.0; // centre pluck: odd harmonics only, easy to count
                c.z = 0.0;
                c.decay_s = 4.0;
                let out = pluck(&c, 96_000);
                // Let the upper partials die, then measure the fundamental.
                // Isolate the fundamental: stiffness makes the upper
                // partials deliberately sharp.
                let k = 1.0 - (-core::f32::consts::TAU * 1.2 * hz / fs).exp();
                let mut lp = [0.0f32; 4];
                let fundamental: Vec<f32> = out
                    .iter()
                    .map(|&x| {
                        let mut v = x;
                        for s in lp.iter_mut() {
                            *s += (v - *s) * k;
                            v = *s;
                        }
                        v
                    })
                    .collect();
                let f = autocorr_hz(&fundamental[48_000..72_000], fs, hz);
                let cents = 1200.0 * (f / hz).log2();
                assert!(cents.abs() < 10.0, "stiff {stiff} hz {hz}: {f}");
            }
        }
    }

    #[test]
    fn decays_and_stays_finite() {
        let fs = 96_000.0;
        let mut c = Ctx::new(fs);
        c.hz = 200.0;
        c.decay_s = 0.3;
        let out = pluck(&c, 96_000);
        assert!(out.iter().all(|x| x.is_finite()));
        let early = out[..4_800].iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        let late = out[90_000..].iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        assert!(late < early * 1e-2, "{early} {late}");
    }

    #[test]
    fn tension_bends_the_attack_upwards() {
        let fs = 96_000.0;
        let mut c = Ctx::new(fs);
        c.hz = 150.0;
        c.y = 1.0;
        c.z = 1.0;
        c.decay_s = 2.0;
        let mut s = KString::new(fs);
        s.control(&c);
        let mut peak_bend = 0.0f32;
        for n in 0..48_000 {
            if n % 16 == 0 {
                s.control(&c);
                peak_bend = peak_bend.max(s.tension_bend());
            }
            s.tick(&hit(n, 8));
        }
        assert!(peak_bend > 0.3, "{peak_bend}");
        assert!(s.tension_bend() < peak_bend * 0.5);
    }
}
