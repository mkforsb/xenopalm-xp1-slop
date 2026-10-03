//! COMB: three tuned feedback comb resonators, struck together.
//!
//! * SPREAD detunes combs 2 and 3 away from the fundamental: unison at 0, then
//!   through fifths and ninths to wide inharmonic clusters.
//! * DIFFUSE puts a Schroeder allpass inside each loop. The loop stays
//!   lossless but its resonances are pulled apart inharmonically, for springs,
//!   rattles and metallic smears.
//! * TWIST is bipolar. Turning it down flips combs, one by one, to negative
//!   feedback at half the delay, which keeps the pitch but only the odd
//!   harmonics (hollow, square- or clarinet-like). Turning it up cross-couples
//!   the combs: each loop feeds a share of its output into the others, so
//!   they exchange energy and beat against each other.

use super::{Ctx, MIN_DELAY_HZ};
use crate::exciter::Excitation;
use crate::params::map;
use crate::util::{DcBlocker, DelayLine, flush, magnitude_first_order, phase_delay_first_order};

const COMBS: usize = 3;
/// Fraction of each loop taken by the diffusion allpass.
const AP_FRACTION: f32 = 0.3;
/// Damping lowpass pole inside the loops.
const DAMP_POLE: f32 = 0.25;
/// Level trim.
const INPUT_GAIN: f32 = 1.2;
/// In-loop DC blocker corner: a soft mallet longer than the loop would
/// otherwise leave a slowly decaying offset circulating.
const DC_HZ: f32 = 8.0;

#[derive(Clone, Debug)]
pub struct Comb {
    fs: f32,
    lines: [DelayLine; COMBS],
    aps: [DelayLine; COMBS],
    lp: [f32; COMBS],
    delay: [f32; COMBS],
    ap_delay: [f32; COMBS],
    gain: [f32; COMBS],
    ap_g: f32,
    coupling: f32,
    period: f32,
    dc: [DcBlocker; COMBS],
    dc_r: f32,
}

impl Comb {
    pub fn new(fs: f32) -> Self {
        let max = (fs / MIN_DELAY_HZ) as usize + 8;
        Self {
            fs,
            lines: core::array::from_fn(|_| DelayLine::new(max)),
            aps: core::array::from_fn(|_| DelayLine::new(max)),
            lp: [0.0; COMBS],
            delay: [100.0; COMBS],
            ap_delay: [30.0; COMBS],
            gain: [0.0; COMBS],
            ap_g: 0.0,
            coupling: 0.0,
            period: 100.0,
            dc: Default::default(),
            dc_r: 1.0 - core::f32::consts::TAU * DC_HZ / fs,
        }
    }

    pub fn reset(&mut self) {
        self.lines.iter_mut().for_each(DelayLine::clear);
        self.aps.iter_mut().for_each(DelayLine::clear);
        self.lp = [0.0; COMBS];
        self.dc.iter_mut().for_each(DcBlocker::reset);
    }

    pub fn control(&mut self, c: &Ctx) {
        let fs = self.fs;
        let ratios = map::comb_ratios(c.x);
        let twist = (c.z - 0.5) * 2.0;
        self.coupling = 0.6 * twist.max(0.0);
        self.ap_g = 0.75 * c.y;
        for i in 0..COMBS {
            let hz = (c.hz * ratios[i]).clamp(MIN_DELAY_HZ, 0.4 * fs);
            // Negative twist flips comb i once |twist| passes (i + 0.5) / 3.
            let negative = -twist > (i as f32 + 0.5) / COMBS as f32;
            let loop_len = if negative { 0.5 * fs / hz } else { fs / hz };
            let w0 = core::f32::consts::TAU * hz / fs;
            let b = DAMP_POLE;
            let tau_lp = phase_delay_first_order(1.0 - b, 0.0, -b, w0);
            let lp_mag = magnitude_first_order(1.0 - b, 0.0, -b, w0);
            self.ap_delay[i] = (AP_FRACTION * loop_len).max(2.0);
            self.delay[i] = (loop_len - self.ap_delay[i] - tau_lp).max(2.0);
            let per_trip = (-6.907_755 * loop_len / (c.decay_s.max(1e-3) * fs)).exp();
            let g = (per_trip / lp_mag).min(0.9995);
            self.gain[i] = if negative { -g } else { g };
        }
        self.period = fs / c.hz.max(MIN_DELAY_HZ);
    }

    #[inline]
    pub fn tick(&mut self, e: &Excitation) -> f32 {
        let x = e.displacement(self.period) * INPUT_GAIN;
        let y: [f32; COMBS] = core::array::from_fn(|i| self.lines[i].read(self.delay[i]));
        let mean = (y[0] + y[1] + y[2]) * (1.0 / COMBS as f32);
        let mut out = 0.0;
        for i in 0..COMBS {
            // Cross-coupling: a row-stochastic mix, so it can't add energy.
            let fb = (1.0 - self.coupling) * y[i] + self.coupling * mean;
            let fb = self.dc[i].process(fb, self.dc_r);
            self.lp[i] = flush((1.0 - DAMP_POLE) * fb + DAMP_POLE * self.lp[i]);
            let v = self.lp[i] * self.gain[i] + x;
            // Schroeder allpass inside the loop.
            let d = self.aps[i].read(self.ap_delay[i]);
            let w = v - self.ap_g * d;
            self.aps[i].write(w);
            self.lines[i].write(d + self.ap_g * w);
            out += y[i];
        }
        out * (1.0 / COMBS as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_for_all_extremes() {
        let fs = 96_000.0;
        for x in [0.0, 1.0] {
            for y in [0.0, 1.0] {
                for z in [0.0, 0.5, 1.0] {
                    let mut comb = Comb::new(fs);
                    let mut c = Ctx::new(fs);
                    c.hz = 300.0;
                    c.x = x;
                    c.y = y;
                    c.z = z;
                    c.decay_s = 1.0;
                    let mut peak_late = 0.0f32;
                    let mut peak = 0.0f32;
                    for n in 0..(3 * 96_000) {
                        if n % 16 == 0 {
                            comb.control(&c);
                        }
                        let o = comb.tick(&Excitation {
                            mallet: if n < 4 { 0.25 } else { 0.0 },
                            pulse_len: 4.0,
                            ..Default::default()
                        });
                        assert!(o.is_finite());
                        peak = peak.max(o.abs());
                        if n > 2 * 96_000 {
                            peak_late = peak_late.max(o.abs());
                        }
                    }
                    assert!(peak > 1e-3, "{x} {y} {z}: silent");
                    assert!(peak_late < peak * 1e-3, "{x} {y} {z}: {peak} {peak_late}");
                }
            }
        }
    }
}
