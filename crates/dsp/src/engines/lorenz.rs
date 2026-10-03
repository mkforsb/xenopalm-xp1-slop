//! LORENZ: the Lorenz system integrated at audio rate.
//!
//! ```text
//!  ẋ = σ(y − x)    ẏ = x(ρ − z) − y    ż = xy − βz
//! ```
//!
//! Below ρ ≈ 24.74 (for σ = 10) the two off-centre fixed points are stable
//! spirals: kick the state away and it spirals back in, a damped
//! oscillation. That is already a percussive sound, and a strange one: the
//! decay and the pitch both come from the equations. Above that value the
//! fixed points go unstable and the kick throws the state onto the strange
//! attractor. The amplitude envelope then shapes an endless, never-repeating
//! orbit that hops between lobes.
//!
//! PITCH is honoured by scaling the time step so the spiral's angular
//! frequency (the imaginary part of the Jacobian's eigenvalues at the fixed
//! point) lands on the requested pitch.
//!
//! * RHO: 8 … 48, from damped spiral through the Hopf point into chaos.
//! * SIGMA: 4 … 20, changing the spiral's damping and the orbit's shape.
//! * AXIS: which state variable is heard (x → y → z, crossfaded). x carries
//!   the lobe hopping as a square-ish thump, z is smoother and sits an
//!   octave up.

use super::Ctx;
use crate::params::map;
use crate::util::DcBlocker;

const BETA: f32 = 8.0 / 3.0;
/// Largest integration step (in Lorenz time units) before RK2 gets unreliable.
const MAX_DT: f32 = 0.012;

/// Angular frequency (rad per Lorenz time unit) of the spiral around the
/// fixed points `(±√(β(ρ−1)), ±√(β(ρ−1)), ρ−1)`, or `None` if the
/// eigenvalues there are all real.
pub fn spiral_omega(rho: f32, sigma: f32) -> Option<f32> {
    // Characteristic polynomial: λ³ + a2λ² + a1λ + a0.
    let a2 = sigma + BETA + 1.0;
    let a1 = BETA * (sigma + rho);
    let a0 = 2.0 * BETA * sigma * (rho - 1.0);
    // The real root is negative; Newton from well left of it.
    let mut r = -a2 - 1.0;
    for _ in 0..40 {
        let p = ((r + a2) * r + a1) * r + a0;
        let dp = (3.0 * r + 2.0 * a2) * r + a1;
        if dp.abs() < 1e-9 {
            break;
        }
        r -= p / dp;
    }
    // Deflate to λ² + pλ + q.
    let p = a2 + r;
    let q = a1 + r * p;
    let disc = 4.0 * q - p * p;
    (disc > 0.0).then(|| 0.5 * disc.sqrt())
}

#[derive(Clone, Debug)]
pub struct Lorenz {
    fs: f32,
    x: f32,
    y: f32,
    z: f32,
    rho: f32,
    sigma: f32,
    dt: f32,
    axis: f32,
    fixed: f32,
    dc: DcBlocker,
    dc_r: f32,
}

impl Lorenz {
    pub fn new(fs: f32) -> Self {
        let mut l = Self {
            fs,
            x: 0.0,
            y: 0.0,
            z: 0.0,
            rho: 20.0,
            sigma: 10.0,
            dt: 0.001,
            axis: 0.0,
            fixed: 1.0,
            dc: DcBlocker::default(),
            dc_r: 1.0 - core::f32::consts::TAU * 15.0 / fs,
        };
        l.reset();
        l
    }

    fn fixed_point(&self) -> f32 {
        (BETA * (self.rho - 1.0)).max(1e-3).sqrt()
    }

    pub fn reset(&mut self) {
        self.fixed = self.fixed_point();
        self.x = self.fixed;
        self.y = self.fixed;
        self.z = self.rho - 1.0;
        self.dc.reset();
    }

    pub fn control(&mut self, c: &Ctx) {
        self.rho = map::lorenz_rho(c.x);
        self.sigma = map::lorenz_sigma(c.y);
        self.axis = c.z;
        self.fixed = self.fixed_point();
        let omega = spiral_omega(self.rho, self.sigma).unwrap_or(10.0).max(0.5);
        self.dt = (core::f32::consts::TAU * c.hz / (omega * self.fs)).min(MAX_DT);
    }

    /// Kick the state off its fixed point.
    pub fn trigger(&mut self, c: &Ctx) {
        self.control(c);
        let k = self.fixed * (0.3 + 0.9 * c.velocity);
        self.y -= k;
        self.x += 0.3 * k;
    }

    #[inline]
    fn deriv(&self, x: f32, y: f32, z: f32) -> (f32, f32, f32) {
        (
            self.sigma * (y - x),
            x * (self.rho - z) - y,
            x * y - BETA * z,
        )
    }

    #[inline]
    pub fn tick(&mut self, env: f32) -> f32 {
        // Midpoint (RK2) step.
        let h = self.dt;
        let (dx, dy, dz) = self.deriv(self.x, self.y, self.z);
        let (mx, my, mz) = (
            self.x + 0.5 * h * dx,
            self.y + 0.5 * h * dy,
            self.z + 0.5 * h * dz,
        );
        let (dx, dy, dz) = self.deriv(mx, my, mz);
        self.x += h * dx;
        self.y += h * dy;
        self.z += h * dz;
        if !(self.x.is_finite() && self.y.is_finite() && self.z.is_finite())
            || self.x.abs() > 500.0
            || self.z.abs() > 1000.0
        {
            self.reset();
        }
        let f = self.fixed;
        let (ox, oy, oz) = (self.x / f, self.y / f, (self.z - (self.rho - 1.0)) / f);
        let a = self.axis * 2.0;
        let raw = if a < 1.0 {
            ox * (1.0 - a) + oy * a
        } else {
            oy * (2.0 - a) + oz * (a - 1.0)
        };
        self.dc.process(raw, self.dc_r) * env * 0.45
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_eigenvalues() {
        // ρ = 28, σ = 10: eigenvalues at C± are −13.85 and 0.094 ± 10.19i.
        let w = spiral_omega(28.0, 10.0).unwrap();
        assert!((w - 10.19).abs() < 0.05, "{w}");
    }

    #[test]
    fn damped_regime_rings_down_by_itself() {
        let fs = 96_000.0;
        let mut l = Lorenz::new(fs);
        let mut c = Ctx::new(fs);
        c.hz = 200.0;
        c.x = 0.2; // ρ = 16: stable spiral
        c.y = 0.4;
        c.z = 0.0;
        c.velocity = 1.0;
        l.trigger(&c);
        let out: Vec<f32> = (0..96_000 * 2).map(|_| l.tick(1.0)).collect();
        assert!(out.iter().all(|v| v.is_finite()));
        let early = out[..9_600].iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        let late = out[out.len() - 9_600..]
            .iter()
            .fold(0.0f32, |a, &b| a.max(b.abs()));
        assert!(early > 0.1 && late < early * 0.05, "{early} {late}");
        // The ring should be near the requested pitch.
        let seg = &out[2_000..20_000];
        let crossings = seg.windows(2).filter(|w| w[0] <= 0.0 && w[1] > 0.0).count();
        let hz = crossings as f32 / (seg.len() as f32 / fs);
        assert!((hz / 200.0 - 1.0).abs() < 0.2, "{hz}");
    }

    #[test]
    fn chaotic_regime_keeps_going_and_stays_bounded() {
        let fs = 96_000.0;
        let mut l = Lorenz::new(fs);
        let mut c = Ctx::new(fs);
        c.hz = 300.0;
        c.x = 0.8; // ρ = 40
        c.y = 0.4;
        c.velocity = 1.0;
        l.trigger(&c);
        let out: Vec<f32> = (0..96_000 * 2).map(|_| l.tick(1.0)).collect();
        let late = out[out.len() - 9_600..]
            .iter()
            .fold(0.0f32, |a, &b| a.max(b.abs()));
        assert!(late > 0.1, "{late}");
        assert!(out.iter().all(|v| v.is_finite() && v.abs() < 10.0));
    }
}
